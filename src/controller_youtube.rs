#![cfg(windows)]
//! Native YouTube token preparation. This module has no upload capability.
use rusqlite::{Connection,OpenFlags,params};
use serde::Deserialize;
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::{collections::BTreeSet,ffi::c_void,path::{Path,PathBuf},ptr::{null,null_mut}};
use windows_sys::Win32::{
    Foundation::{CloseHandle,HANDLE,WAIT_ABANDONED,WAIT_OBJECT_0},
    Networking::WinHttp::*,
    Security::Credentials::*,
    System::Threading::{CreateMutexW,ReleaseMutex,WaitForSingleObject},
};
type Result<T>=std::result::Result<T,&'static str>;
const CHANNEL:&str="UC0000000000000000000000";
const CLIENT_PIN:&str="fcc70a8c1c62af0caee509b1eb75bca88e4f2dcb00f73cc265c22e86b17b5c9b";
const SCOPES:[&str;2]=["youtube.readonly","youtube.upload"];
const LEDGER_LEAF:&str="track-b-ops-1000000000000000001.sqlite3";
const SKEW:i64=600;

#[derive(Deserialize)]
struct DesktopFile { installed:DesktopClient }
#[derive(Deserialize)]
struct DesktopClient {client_id:String,client_secret:String,project_id:String,token_uri:String}
struct Config { client:DesktopClient, target:String, ledger:PathBuf }
fn sha(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
fn time()->i64{chrono::Utc::now().timestamp()}
fn text(v:&str)->bool{!v.is_empty()&&v.len()<=4096&&v.bytes().all(|c|(33..=126).contains(&c))}
fn plain_local(path:&Path)->bool{
    use std::os::windows::fs::MetadataExt;
    path.is_absolute() && matches!(path.components().next(),
        Some(std::path::Component::Prefix(p)) if matches!(p.kind(),std::path::Prefix::Disk(_)))
        && !path.components().any(|c|matches!(c,std::path::Component::ParentDir))
        && path.ancestors().all(|p|std::fs::symlink_metadata(p).is_ok_and(|m|m.file_attributes()&0x400==0))
}
impl Config {
    fn read()->Result<Self> {
        let path=PathBuf::from(std::env::var_os("LIFEINVADER_YOUTUBE_INSTALLED_CLIENT").ok_or("unconfigured")?);
        let ledger=PathBuf::from(std::env::var_os("LIFEINVADER_YOUTUBE_ATTEMPT_LEDGER").ok_or("unconfigured")?);
        // The approved file lives beneath managed OneDrive/Desktop reparse directories.
        // Only this exact local leaf is allowed; the byte digest below anchors its provenance.
        use std::{io::Read,os::windows::fs::MetadataExt};
        let approved=PathBuf::from(std::env::var_os("USERPROFILE").ok_or("configuration_unavailable")?)
            .join("OneDrive").join("Desktop").join("desktop-oauth-client.json");
        let valid_client_path=path==approved && path.is_absolute()
            && std::fs::symlink_metadata(&path).is_ok_and(|m|m.is_file()&&m.file_attributes()&0x400==0);
        if !valid_client_path||!plain_local(&ledger)||!ledger.is_file()
            || ledger.file_name().and_then(|s|s.to_str())!=Some(LEDGER_LEAF) {return Err("configuration_mismatch");}
        let mut raw=Vec::new();
        std::fs::File::open(path).map_err(|_|"configuration_unavailable")?
            .take(16385).read_to_end(&mut raw).map_err(|_|"configuration_unavailable")?;
        if raw.len()>16384||sha(&raw)!=CLIENT_PIN{return Err("client_provenance_mismatch");}
        let client=serde_json::from_slice::<DesktopFile>(raw.strip_prefix(&[0xef,0xbb,0xbf]).unwrap_or(&raw))
            .map_err(|_|"client_schema")?.installed;
        if client.token_uri!="https://oauth2.googleapis.com/token" || !client.client_id.ends_with(".apps.googleusercontent.com")
           || ![&client.client_id,&client.client_secret,&client.project_id].into_iter().all(|s|text(s)) {
            return Err("client_provenance_mismatch");
        }
        let fingerprint=sha(format!("{}\0{}",client.project_id,client.client_id).as_bytes());
        Ok(Self{client,target:format!("Lifeinvader/YouTube/v1/{fingerprint}/{CHANNEL}"),ledger})
    }
}

fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}
struct HeldMutex(HANDLE,bool);
impl Drop for HeldMutex {fn drop(&mut self){unsafe{if self.1{ReleaseMutex(self.0);}CloseHandle(self.0);}}}
fn lock(target:&str)->Result<HeldMutex>{
    let n=wide(&format!(r"Global\LifeinvaderMeta-{}",sha(target.as_bytes())));
    let h=unsafe{CreateMutexW(null(),0,n.as_ptr())};
    if h.is_null(){return Err("vault_lock_failed");}
    let mut m=HeldMutex(h,false);
    let s=unsafe{WaitForSingleObject(h,5000)};
    m.1=s==WAIT_OBJECT_0||s==WAIT_ABANDONED;
    if s!=WAIT_OBJECT_0{return Err("vault_lock_unavailable");}
    Ok(m)
}
struct NativeCred(*mut CREDENTIALW);
impl Drop for NativeCred{fn drop(&mut self){unsafe{CredFree(self.0.cast());}}}
fn read_vault(target:&str)->Result<Vec<u8>>{
    let name=wide(target);let mut ptr=null_mut();
    if unsafe{CredReadW(name.as_ptr(),CRED_TYPE_GENERIC,0,&mut ptr)}==0||ptr.is_null(){return Err("vault_unavailable");}
    let c=NativeCred(ptr);
    let v=unsafe{&*c.0};
    if v.Type!=CRED_TYPE_GENERIC||v.TargetName.is_null()||v.CredentialBlob.is_null()
        || !(1..=2560).contains(&v.CredentialBlobSize)
        || !(0..name.len()).all(|i|unsafe{*v.TargetName.add(i)}==name[i]){return Err("vault_format");}
    Ok(unsafe{std::slice::from_raw_parts(v.CredentialBlob,v.CredentialBlobSize as usize)}.to_vec())
}
fn write_vault(target:&str,raw:&mut [u8])->Result<()>{
    if raw.is_empty()||raw.len()>2560{return Err("vault_format");}
    let mut name=wide(target);
    let c=CREDENTIALW{Type:CRED_TYPE_GENERIC,TargetName:name.as_mut_ptr(),CredentialBlobSize:raw.len() as u32,
        CredentialBlob:raw.as_mut_ptr(),Persist:CRED_PERSIST_LOCAL_MACHINE,..Default::default()};
    if unsafe{CredWriteW(&c,0)}==0{return Err("vault_write_unconfirmed");}
    if read_vault(target)?!=raw{return Err("vault_write_unconfirmed");}
    Ok(())
}
fn scope_exact(v:impl Iterator<Item=impl AsRef<str>>)->bool{
    let scopes:Vec<_>=v.map(|s|s.as_ref().to_owned()).collect();
    scopes.len()==2 && scopes.iter().map(String::as_str).collect::<BTreeSet<_>>()==SCOPES.into_iter().collect()
}
fn record<'a>(d:&'a Value,target:&str)->Result<&'a Value>{
    if d["target"].as_str()!=Some(target)||d["state"]!="ready"||d.get("lease")!=Some(&Value::Null){return Err("vault_held");}
    let r=d.get("record").ok_or("vault_format")?;
    let vals=r["scopes"].as_array().ok_or("vault_scope_mismatch")?;
    if !scope_exact(vals.iter().map(|s|s.as_str().unwrap_or(""))){return Err("vault_scope_mismatch");}
    if !r["refresh_token"].as_str().is_some_and(text)||!r["access_token"].as_str().is_some_and(text)
        || r["expires_at"].as_i64().is_none()||r["version"].as_i64().is_none_or(|v|v<0||v==i64::MAX){
        return Err("vault_format");
    }
    Ok(r)
}

struct H(*mut c_void);
impl H{fn new(h:*mut c_void)->Result<Self>{if h.is_null(){Err("provider_transport")}else{Ok(Self(h))}}}
impl Drop for H{fn drop(&mut self){unsafe{WinHttpCloseHandle(self.0);}}}
enum Request<'a>{Refresh(&'a [u8]),Mine(&'a str),Handle(&'a str)}
fn request(spec:Request<'_>)->Result<Value>{
    let refresh_call=matches!(&spec,Request::Refresh(_));
    let (host,verb,path,body,token)=match spec{
        Request::Refresh(b)=>("oauth2.googleapis.com","POST","/token",b,None),
        Request::Mine(t)=>("www.googleapis.com","GET","/youtube/v3/channels?part=snippet&mine=true",&[][..],Some(t)),
        Request::Handle(t)=>("www.googleapis.com","GET","/youtube/v3/channels?part=id&forHandle=%40Demo_Channel",&[][..],Some(t)),
    };
    let s=H::new(unsafe{WinHttpOpen(wide("Lifeinvader Native").as_ptr(),WINHTTP_ACCESS_TYPE_NO_PROXY,null(),null(),0)})?;
    if unsafe{WinHttpSetTimeouts(s.0,5000,5000,8000,12000)}==0{return Err("provider_transport");}
    let c=H::new(unsafe{WinHttpConnect(s.0,wide(host).as_ptr(),443,0)})?;
    let h=H::new(unsafe{WinHttpOpenRequest(c.0,wide(verb).as_ptr(),wide(path).as_ptr(),null(),null(),null(),WINHTTP_FLAG_SECURE)})?;
    for (opt,val) in [(WINHTTP_OPTION_REDIRECT_POLICY,WINHTTP_OPTION_REDIRECT_POLICY_NEVER),
        (WINHTTP_OPTION_AUTOLOGON_POLICY,WINHTTP_AUTOLOGON_SECURITY_LEVEL_HIGH),
        (WINHTTP_OPTION_DISABLE_FEATURE,WINHTTP_DISABLE_COOKIES|WINHTTP_DISABLE_AUTHENTICATION)]{
        if unsafe{WinHttpSetOption(h.0,opt,(&val as *const u32).cast(),4)}==0{return Err("provider_policy");}
    }
    let mut headers=String::from("Accept: application/json\r\n");
    if !body.is_empty(){headers.push_str("Content-Type: application/x-www-form-urlencoded\r\n");}
    if let Some(t)=token{
        if !text(t){return Err("provider_response_invalid");}
        headers.push_str("Authorization: Bearer ");headers.push_str(t);headers.push_str("\r\n");
    }
    let hdr=wide(&headers);
    let data=if body.is_empty(){null()}else{body.as_ptr().cast()};
    let size=body.len() as u32;
    if unsafe{WinHttpSendRequest(h.0,hdr.as_ptr(),(hdr.len()-1) as u32,data,size,size,0)}==0
        || unsafe{WinHttpReceiveResponse(h.0,null_mut())}==0 {return Err("provider_transport");}
    let mut status=0u32;let mut len=4u32;
    if unsafe{WinHttpQueryHeaders(h.0,WINHTTP_QUERY_STATUS_CODE|WINHTTP_QUERY_FLAG_NUMBER,null(),(&mut status as *mut u32).cast(),&mut len,null_mut())}==0{
        return Err("provider_transport");
    }
    if (300..400).contains(&status){return Err("provider_redirect_refused");}
    if status!=200&&!(refresh_call&&status==400){return Err("provider_request_rejected");}
    let mut out=Vec::new();let mut chunk=[0u8;8192];
    loop {
        let mut n=0u32;
        if unsafe{WinHttpReadData(h.0,chunk.as_mut_ptr().cast(),chunk.len() as u32,&mut n)}==0{return Err("provider_transport");}
        if n==0{break;}
        if out.len()+n as usize>65536{return Err("provider_response_oversize");}
        out.extend_from_slice(&chunk[..n as usize]);
    }
    let parsed:Value=serde_json::from_slice(&out).map_err(|_|"provider_response_invalid")?;
    if status!=200 {return Err(if parsed.get("error").and_then(Value::as_str)==Some("invalid_grant"){"invalid_grant"}else{"provider_request_rejected"});}
    Ok(parsed)
}
fn encode(s:&str)->String{
    let mut o=String::new();
    for b in s.bytes(){if b.is_ascii_alphanumeric()||matches!(b,b'-'|b'_'|b'.'|b'~'){o.push(b as char);}else{o.push_str(&format!("%{b:02X}"));}}
    o
}
fn channel(v:Value,require_handle:bool)->bool{
    v["items"].as_array().is_some_and(|a|a.len()==1&&a[0]["id"]==CHANNEL &&
        (!require_handle||a[0]["snippet"]["customUrl"].as_str().is_some_and(|s|s.eq_ignore_ascii_case("@Demo_Channel"))))
}
fn ledger(config:&Config,version:i64)->Result<(Connection,String)>{
    let db=Connection::open_with_flags(&config.ledger,OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(|_|"attempt_ledger_unavailable")?;
    let key=format!("youtube-refresh:{CHANNEL}:{version}");
    db.execute("INSERT INTO attempts(key,state,started_at) VALUES(?, 'attempted', ?)",params![key,time()])
      .map_err(|_|"refresh_attempt_held")?;
    Ok((db,key))
}
#[derive(Copy,Clone)]
enum Mode{ReadOnly,BeforeUpload}
fn safe_refresh_outcome(reason:&str)->&'static str{
    match reason{
        "invalid_grant"=>"invalid_grant",
        "provider_request_rejected"|"provider_redirect_refused"=>"provider_rejected",
        "provider_transport"=>"transport_unconfirmed",
        _=>"verification_unconfirmed",
    }
}
fn audit_table(db:&Connection)->Result<()>{
    db.execute_batch("CREATE TABLE IF NOT EXISTS youtube_refresh_outcomes_v1(
          key TEXT PRIMARY KEY, outcome TEXT NOT NULL CHECK(outcome IN ('invalid_grant','provider_rejected','transport_unconfirmed','verification_unconfirmed','verified')),
          checked_at INTEGER NOT NULL);").map_err(|_|"refresh_audit_unconfirmed")
}
fn prior_outcome(c:&Config,version:i64)->Option<&'static str>{
    let db=Connection::open_with_flags(&c.ledger,OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let key=format!("youtube-refresh:{CHANNEL}:{version}");
    let state:String=db.query_row("SELECT state FROM attempts WHERE key=?",[&key],|r|r.get(0)).ok()?;
    if state!="attempted"{return None;}
    Some(match db.query_row("SELECT outcome FROM youtube_refresh_outcomes_v1 WHERE key=?",[&key],|r|r.get::<_,String>(0))
        .ok().as_deref(){
            Some("invalid_grant")=>"invalid_grant",Some("provider_rejected")=>"provider_rejected",
            Some("transport_unconfirmed")=>"transport_unconfirmed",Some("verification_unconfirmed")=>"verification_unconfirmed",
            _=>"refresh_attempt_held_unknown"
        })
}
fn run(mode:Mode)->Result<(i64,bool,bool,Option<i64>,Option<&'static str>)>{
    let c=Config::read()?;
    let _lock=lock(&c.target)?;
    let raw=read_vault(&c.target)?;
    let mut d:Value=serde_json::from_slice(&raw).map_err(|_|"vault_format")?;
    let r=record(&d,&c.target)?;
    let expiry=r["expires_at"].as_i64().ok_or("vault_format")?;
    let version=r["version"].as_i64().ok_or("vault_format")?;
    let last_verified=version.checked_sub(1).and_then(|previous|{
        let db=Connection::open_with_flags(&c.ledger,OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
        let key=format!("youtube-refresh:{CHANNEL}:{previous}");
        let when=db.query_row("SELECT started_at FROM attempts WHERE key=? AND state='verified'",[key],|r|r.get::<_,i64>(0)).ok()?;
        (when>0&&when<=time()&&when<expiry).then_some(when)
    });
    if expiry.saturating_sub(time())>SKEW{return Ok((expiry,false,true,last_verified,None));}
    let status=prior_outcome(&c,version);
    if matches!(mode,Mode::ReadOnly)||status.is_some(){return Ok((expiry,false,false,last_verified,status));}
    let old_refresh=r["refresh_token"].as_str().ok_or("vault_format")?.to_owned();
    let (mut db,key)=ledger(&c,version)?;
    audit_table(&db)?;
    let result=(||->Result<i64>{
    let body=format!("grant_type=refresh_token&client_id={}&client_secret={}&refresh_token={}",
        encode(&c.client.client_id),encode(&c.client.client_secret),encode(&old_refresh));
    let v=request(Request::Refresh(body.as_bytes()))?;
    if let Some(s)=v.get("scope"){
        let s=s.as_str().ok_or("refresh_scope_mismatch")?;
        if !scope_exact(s.split_whitespace().map(|v|v.strip_prefix("https://www.googleapis.com/auth/").unwrap_or(""))){
            return Err("refresh_scope_mismatch");
        }
    }
    let access=v["access_token"].as_str().filter(|s|text(s)).ok_or("provider_response_invalid")?;
    let refresh=match v.get("refresh_token"){None=>old_refresh.as_str(),Some(v)=>v.as_str().filter(|s|text(s)).ok_or("provider_response_invalid")?};
    let ttl=v["expires_in"].as_i64().filter(|v|(31..=86400).contains(v)).ok_or("provider_response_invalid")?;
    if !v["token_type"].as_str().is_some_and(|s|s.eq_ignore_ascii_case("bearer")){return Err("provider_response_invalid");}
    let expires=time()+ttl;
    if !channel(request(Request::Mine(access))?,true)||!channel(request(Request::Handle(access))?,false){
        return Err("canonical_channel_mismatch");
    }
    if expires<=time()+30{return Err("refreshed_token_expired");}
    let refresh=refresh.to_owned();
    d["record"]["access_token"]=json!(access);
    d["record"]["refresh_token"]=json!(refresh);
    d["record"]["expires_at"]=json!(expires);
    d["record"]["version"]=json!(version+1);
    let mut payload=serde_json::to_vec(&d).map_err(|_|"vault_format")?;
    write_vault(&c.target,&mut payload)?;
    Ok(expires)
    })();
    let observed=time();
    let outcome=match result{Ok(_)=>"verified",Err(reason)=>safe_refresh_outcome(reason)};
    let tx=db.transaction().map_err(|_|"refresh_audit_unconfirmed")?;
    tx.execute("INSERT INTO youtube_refresh_outcomes_v1(key,outcome,checked_at) VALUES(?,?,?)",
       params![&key,outcome,observed]).map_err(|_|"refresh_audit_unconfirmed")?;
    if result.is_ok()&&tx.execute("UPDATE attempts SET state='verified' WHERE key=? AND state='attempted'",[&key]).ok()!=Some(1){
       return Err("refresh_audit_unconfirmed");
    }
    tx.commit().map_err(|_|"refresh_audit_unconfirmed")?;
    match result{
        Ok(expires)=>Ok((expires,true,true,Some(observed),None)),
        Err(_)=>Ok((expiry,false,false,last_verified,Some(outcome))),
    }
}
pub(crate) fn health(prepare:bool)->Value{
    let mode=if prepare{Mode::BeforeUpload}else{Mode::ReadOnly};
    match run(mode){
        Ok((expiry,refreshed,ready,last_verified,reason))=>json!({"provider":"youtube","health":if ready{"ready"}else if expiry<=time(){"expired"}else{"refresh_pending"},
            "reason":reason,"checked_at_unix":time(),"expires_at_unix":expiry,"canonical_channel_id":CHANNEL,
            "credential_type":"google_oauth_user","last_refresh_at_unix":last_verified,"access_expires_at_unix":expiry,
            "refresh_expires_at_unix":null,"data_access_expires_at_unix":null,"access_expiry_kind":"deadline",
            "state":if ready{"ready"}else if expiry<=time(){"expired"}else{"due"},
            "scope_ids":SCOPES,"automatic_refresh_configured":true,"refreshed":refreshed,"publishing_enabled":false}),
        Err(reason)=>json!({"provider":"youtube","health":if reason=="unconfigured"{"unconfigured"}else{"held"},
            "reason":reason,"checked_at_unix":time(),"state":if reason=="unconfigured"{"disconnected"}else{"gated"},
            "automatic_refresh_configured":false,"refreshed":false,"publishing_enabled":false})
    }
}
