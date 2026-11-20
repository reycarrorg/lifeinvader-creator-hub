//! One locally reviewed, no-publication AV1 derivative. Nothing dispatches on load.
use crate::{App,controller_store as store,workspace_media as media};
use serde::{Deserialize,Serialize};
use serde_json::{Value,json};
use rusqlite::OptionalExtension;
use std::{fs::File,path::{Path,PathBuf}};
pub(crate) const CLIP:&str="d471e12c49d298ef7276d8b147160636f16da76ad3b71fe43727516f3f292707e";
const SOURCE:&str="c157e6bf0f844b99932be9d12525e0f8137b88a9072eac01cc08bfe24c4dcdc6";
const ROOT:&str="snapmatic-demo-render";
const SOURCE_REV:&str="ef1cc1847673438a3ea063a52d4575761251ae1558c6d064dd9b7fd68cea429c";
const DURATION:u64=20800;
const WIDTH:u64=1440;
const HEIGHT:u64=2560;
const CAP:u64=180*1024*1024;

#[derive(Clone,Debug,Deserialize,Serialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    schema:String, clip_id:String, source_sha256:String, source_revision:String, in_ms:u64,out_ms:u64,
    framing:Framing,transcript_sha256:Option<String>,edits:std::collections::BTreeMap<String,String>,
}
#[derive(Clone,Debug,Deserialize,Serialize,PartialEq)]
#[serde(tag="mode",rename_all="snake_case",deny_unknown_fields)]
enum Framing{Center,Blur,DualZone{face:[u64;4],game:[u64;4]}}
#[derive(Clone,Debug,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
struct Segment{id:String,start_ms:u64,end_ms:u64,text:String}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Transcript{source_sha256:String,source_revision:String,segments:Vec<Segment>}
#[derive(Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
struct Audio{integrated_lufs:Option<f64>,true_peak_dbtp:Option<f64>,status:String}
#[derive(Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt{
    schema:String,request_sha256:String,source_sha256:String,source_revision:String,
    sha256:String,bytes:u64,audio_target_bps:u64,audio_measured_bps:u64,audio:Audio,publication_enabled:bool
}
fn opaque(s:&str)->bool{!s.is_empty()&&s.len()<=256&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b"._:-".contains(&b))}
fn rect(r:[u64;4])->bool{
    let [x,y,w,h]=r;
    [x,y,w,h].into_iter().all(|p|p%2==0)&&w>=2&&h>=2&&x.checked_add(w).is_some_and(|p|p<=WIDTH)&&y.checked_add(h).is_some_and(|p|p<=HEIGHT)
}
fn valid(r:&Request,segments:&[Segment],has_transcript:bool)->bool{
    if r.schema!="snapmatic-preview-transcode-v1"||r.clip_id!=CLIP||r.source_sha256!=SOURCE||
       r.source_revision!=SOURCE_REV||r.in_ms>=r.out_ms||r.out_ms>DURATION||
       (r.transcript_sha256.is_some()!=has_transcript)||r.transcript_sha256.as_deref().is_some_and(|s|!media::hex(s))||
       r.edits.len()>512||(!has_transcript&&!r.edits.is_empty()){return false;}
    if let Framing::DualZone{face,game}=r.framing {if !rect(face)||!rect(game){return false;}}
    let text=|s:&str|!s.trim().is_empty()&&s.chars().count()<=150&&!s.contains(char::from(0));
    let mut end=0;let mut ids=std::collections::BTreeSet::new();
    for s in segments {
        if !opaque(&s.id)||s.start_ms<end||s.start_ms>=s.end_ms||s.end_ms>DURATION||!text(&s.text)||!ids.insert(&s.id){return false;}
        end=s.end_ms;
    }
    r.edits.iter().all(|(id,value)|ids.contains(id)&&text(value))
}
fn audio_valid(a:&Audio)->bool {
    match a.status.as_str(){
        "measured"=>a.integrated_lufs.is_some_and(|v|v.is_finite()&&(-140.0..=10.0).contains(&v))
            &&a.true_peak_dbtp.is_some_and(|v|v.is_finite()&&(-140.0..=10.0).contains(&v)),
        "silence"|"unavailable"=>a.integrated_lufs.is_none()&&a.true_peak_dbtp.is_none(),
        _=>false,
    }
}
// Fresh owner-authorized attempt; the separate v1 HELD row is preserved.
fn setup(db:&rusqlite::Connection)->store::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS snapmatic_preview_jobs_v2
      (request_key TEXT PRIMARY KEY, source_clip TEXT UNIQUE NOT NULL,source_revision TEXT NOT NULL,state TEXT NOT NULL,
       receipt_sha TEXT, failure TEXT);").map_err(Into::into)
}
pub(crate) fn events(db:&rusqlite::Connection)->store::Result<Vec<Value>> {
    if !db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='snapmatic_preview_jobs_v2' AND type='table')",
        [],|r|r.get::<_,bool>(0))?{return Ok(Vec::new());}
    let mut q=db.prepare("SELECT request_key,state FROM snapmatic_preview_jobs_v2 WHERE source_clip=?1 LIMIT 1")?;
    let rows=q.query_map([CLIP],|r|{let id:String=r.get(0)?;let state:String=r.get(1)?;
        Ok(json!({"job_id":id,"clip_id":CLIP,"stage":state,"controller_state":"LOCAL_PREVIEW_ONLY"}))
    })?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
#[cfg(windows)]
mod win {
 use super::*;
 use sha2::{Digest,Sha256};
 use std::{io::Read,time::{Duration,Instant}};
 use crate::{controller_native_launcher::clean_path,controller_native_process::{self as native,Owned}};
 #[derive(Deserialize)] #[serde(deny_unknown_fields)]
 struct Authority {
    schema:String,workspace:PathBuf,executable:PathBuf,executable_sha256:String,
    bootstrap:PathBuf,bootstrap_sha256:String,config:PathBuf,config_sha256:String,publishing_enabled:bool,
 }
 struct Plan {
    a:Authority,manifest:PathBuf,manifest_pin:String,key:String,request:Request,segments:Vec<Segment>,
    source:PathBuf,expires:i64,_authority:File
 }
 fn fail(s:&'static str)->store::Error{store::Error(s)}
 fn vf(v:&Value,key:&str)->store::Result<String>{v[key].as_str().map(str::to_owned).ok_or(fail("preview_plan_schema"))}
 fn vp(v:&Value,key:&str)->store::Result<PathBuf>{Ok(PathBuf::from(vf(v,key)?))}
 fn read(path:&Path,pin:&str,max:u64)->store::Result<(File,Vec<u8>)>{
    clean_path(path)?;if !media::hex(pin){return Err(fail("preview_pin_invalid"));}
    let f=native::lease(path,pin)?;let meta=f.metadata().map_err(|_|fail("preview_file_read"))?;
    if !(1..=max).contains(&meta.len()){return Err(fail("preview_file_size"));}
    use std::io::{Seek,SeekFrom};
    let mut f=f;f.seek(SeekFrom::Start(0)).map_err(|_|fail("preview_file_read"))?;
    let mut raw=Vec::new();(&mut f).take(max+1).read_to_end(&mut raw).map_err(|_|fail("preview_file_read"))?;
    if meta.len()!=raw.len() as u64{return Err(fail("preview_file_changed"));}
    Ok((f,raw))
 }
 fn path_root(app:&App)->store::Result<PathBuf> {
    let active=app.media.parent().ok_or(fail("preview_scope"))?;
    if active.file_name().and_then(|x|x.to_str())!=Some("snapmatic-review-runtime"){return Err(fail("preview_scope"));}
    let base=active.parent().ok_or(fail("preview_scope"))?;
    Ok(base.join(ROOT))
 }
 fn authority(app:&App)->store::Result<Plan>{
    let root=path_root(app)?;
    let p=PathBuf::from(std::env::var_os("LIFEINVADER_PHASE3_AUTHORITY").ok_or(fail("preview_plan_unconfigured"))?);
    let pin=std::env::var("LIFEINVADER_PHASE3_AUTHORITY_SHA256").map_err(|_|fail("preview_plan_unconfigured"))?;
    if p.parent()!=Some(&root)||!root.is_dir(){return Err(fail("preview_scope"));}
    let (lease,raw)=read(&p,&pin,16384)?;
    let a:Authority=serde_json::from_slice(&raw).map_err(|_|fail("preview_plan_schema"))?;
    if a.schema!="snapmatic-phase3-authority-v1"||a.publishing_enabled||a.workspace!=root||
        a.config.parent()!=Some(&root)||!media::hex(&a.bootstrap_sha256)||!media::hex(&a.executable_sha256)||
        a.bootstrap.file_name().and_then(|x|x.to_str())!=Some("native_bootstrap.py")||
        a.executable.file_name().and_then(|x|x.to_str())!=Some("python.exe"){return Err(fail("preview_scope"));}
    let (_cfg,c)=read(&a.config,&a.config_sha256,16384)?;
    let cfg:Value=serde_json::from_slice(&c).map_err(|_|fail("preview_plan_schema"))?;
    if cfg["schema"]!="native-bootstrap-preview-v1"||cfg["mode"]!="reviewed-preview"||
       cfg["publishing_enabled"]!=false||cfg["wait_cancel"]!=false||vp(&cfg,"output")?!=root.join("preview-receipt.json"){
       return Err(fail("preview_plan_schema"));
    }
    let expires=cfg["expires_at"].as_i64().ok_or(fail("preview_plan_schema"))?;
    let req=vp(&cfg,"preview_request")?;let manifest=vp(&cfg,"manifest")?;
    let producer=vp(&cfg,"producer_config")?;let source=vp(&cfg,"source")?;
    if [req.as_path(),manifest.as_path(),producer.as_path()].into_iter().any(|x|x.parent()!=Some(&root)){
        return Err(fail("preview_scope"));
    }
    let key=vf(&cfg,"preview_request_sha256")?;let manifest_pin=vf(&cfg,"manifest_sha256")?;
    let (_r,rr)=read(&req,&key,16384)?;
    let request:Request=serde_json::from_slice(&rr).map_err(|_|fail("preview_plan_schema"))?;
    if cfg["source_revision"]!=request.source_revision{return Err(fail("preview_source_revision"));}
    let mut segments=Vec::new();
    let has_transcript=!cfg["transcript"].is_null();
    if has_transcript{
        let t=vp(&cfg,"transcript")?;
        if t.parent()!=Some(&root){return Err(fail("preview_scope"));}
        let (_lease,raw)=read(&t,request.transcript_sha256.as_deref().ok_or(fail("preview_transcript"))?,16384)?;
        let tr:Transcript=serde_json::from_slice(&raw).map_err(|_|fail("preview_transcript"))?;
        if tr.source_sha256!=SOURCE||tr.source_revision!=request.source_revision{return Err(fail("preview_transcript"));}
        segments=tr.segments;
    }
    if !valid(&request,&segments,has_transcript){return Err(fail("preview_request_bounds"));}
    Ok(Plan{a,manifest,manifest_pin,key,request,segments,source,expires,_authority:lease})
 }
 fn item_state(app:&App)->store::Result<Option<(String,String,Option<String>,Option<String>)>>{
    let d=app.db.lock().map_err(|_|fail("storage_unavailable"))?;
    if events(&d)?.is_empty(){return Ok(None);}
    Ok(d.query_row("SELECT request_key,state,receipt_sha,failure FROM snapmatic_preview_jobs_v2 WHERE source_clip=?1",
      [CLIP],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?)
 }
 fn receipt(root:&Path,key:&str,revision:&str,pin:Option<&str>)->store::Result<(Receipt,String,PathBuf,File)>{
    if !media::hex(key){return Err(fail("preview_pin_invalid"));}
    let dir=root.join(key);let p=dir.join("preview-receipt.json");let movie=dir.join("preview.mp4");
    clean_path(&p)?;clean_path(&movie)?;
    let mut raw=Vec::new();
    File::open(&p).map_err(|_|fail("preview_receipt_unavailable"))?.take(16385).read_to_end(&mut raw).map_err(|_|fail("preview_receipt_unavailable"))?;
    if raw.is_empty()||raw.len()>16384{return Err(fail("preview_receipt_unavailable"));}
    let h=format!("{:x}",Sha256::digest(&raw));
    if pin.is_some_and(|pin|pin!=h){return Err(fail("preview_receipt_changed"));}
    let (_receipt_lease,confirmed)=read(&p,&h,16384)?;
    if raw!=confirmed{return Err(fail("preview_receipt_changed"));}
    let r:Receipt=serde_json::from_slice(&raw).map_err(|_|fail("preview_receipt_schema"))?;
    if r.schema!="snapmatic-preview-receipt-v1"||r.request_sha256!=key||r.source_sha256!=SOURCE||r.source_revision!=revision||
        r.publication_enabled||!(1..=CAP).contains(&r.bytes)||!media::hex(&r.sha256)||!audio_valid(&r.audio)||r.audio_target_bps!=128000||!(1..=130560).contains(&r.audio_measured_bps){
        return Err(fail("preview_receipt_mismatch"));
    }
    let v=native::lease(&movie,&r.sha256)?;
    if v.metadata().map_err(|_|fail("preview_output_invalid"))?.len()!=r.bytes{return Err(fail("preview_output_invalid"));}
    Ok((r,h,movie,v))
 }
 fn stored_ready(app:&App,key:&str)->store::Result<(Receipt,PathBuf,File)>{
    let (rev,pin)={
     let db=app.db.lock().map_err(|_|fail("storage_unavailable"))?;
     if events(&db)?.is_empty(){return Err(fail("preview_unavailable"));}
     db.query_row("SELECT source_revision,receipt_sha FROM snapmatic_preview_jobs_v2 WHERE request_key=?1 AND source_clip=?2 AND state='ready'",
        rusqlite::params![key,CLIP],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).map_err(|_|fail("preview_unavailable"))?
    };
    let (r,_,p,f)=receipt(&path_root(app)?,key,&rev,Some(&pin))?;
    Ok((r,p,f))
 }
 pub(super) fn status(app:&App)->store::Result<Value>{
    if let Some((key,state,_,failure))=item_state(app)?{
        let local=if state=="ready"{Some(stored_ready(app,&key)?.0)}else{None};
        let timeline=local.as_ref().and_then(|_|authority(app).ok()).filter(|p|p.key==key&&p.request.transcript_sha256.is_some()).map(|p|json!({"source_sha256":p.request.source_sha256,"in_ms":p.request.in_ms,"out_ms":p.request.out_ms,"speech_segments":p.segments.iter().map(|s|json!({"start_ms":s.start_ms,"end_ms":s.end_ms})).collect::<Vec<_>>()}));
        return Ok(json!({"clip_id":CLIP,"stage":state,"job_id":key,"receipt":local,"timeline":timeline,"failure":failure,"can_dispatch":false,"publishing_enabled":false}));
    }
    let p=authority(app)?;
    let now=chrono::Utc::now().timestamp();
    let eligible=p.expires>now&&p.expires-now<=900;
    Ok(json!({"clip_id":CLIP,"can_dispatch":eligible,"stage":if eligible{"reviewed"}else{"hold"},
       "reason":if eligible{None}else{Some("preview_review_expired")},"request":p.request,"segments":p.segments,"publishing_enabled":false}))
 }
 fn advance(app:&App,key:&str,state:&str,pin:Option<&str>,failure:Option<&str>)->store::Result<()>{
    let d=app.db.lock().map_err(|_|fail("storage_unavailable"))?;
    if d.execute("UPDATE snapmatic_preview_jobs_v2 SET state=?1,receipt_sha=?2,failure=?3 WHERE request_key=?4 AND state!='ready'",
        rusqlite::params![state,pin,failure,key])?!=1{return Err(fail("preview_state_conflict"));}
    Ok(())
 }
 fn produce(app:&App,p:&Plan)->store::Result<()>{
    let next=chrono::Utc::now().timestamp();
    if p.expires<=next||p.expires-next>900{return Err(fail("preview_review_expired"));}
    let (src,_src_lease)=media::open(app,CLIP,false)?;
    if src!=p.source{return Err(fail("preview_source_mismatch"));}
    let mut child=Owned::gated_manifest(&p.a.executable,&p.a.bootstrap,&p.a.config,
        [&p.a.executable_sha256,&p.a.bootstrap_sha256,&p.a.config_sha256],&p.a.workspace,&p.manifest,&p.manifest_pin)?;
    advance(app,&p.key,"processing_evidence_pending",None,None)?;
    let end=Instant::now()+Duration::from_secs(250);
    loop {
        if let Some(success)=child.poll()?{if success&&child.active()?==0{break;}return Err(fail("preview_worker_held"));}
        if Instant::now()>=end{let _=child.cancel();return Err(fail("preview_worker_timeout"));}
        std::thread::sleep(Duration::from_millis(40));
    }
    advance(app,&p.key,"verifying_sha256",None,None)?;
    let (_,pin,_,_)=receipt(&p.a.workspace,&p.key,&p.request.source_revision,None)?;
    advance(app,&p.key,"ready",Some(&pin),None)?;
    Ok(())
 }
 pub(super) fn dispatch(app:App,r:Request)->store::Result<Value>{
    if !app.controller_ui.ready(){return Err(fail("preview_ui_not_ready"));}
    let p=authority(&app)?;let now=chrono::Utc::now().timestamp();
    if p.expires<=now||p.expires-now>900||p.request!=r{return Err(fail("preview_request_not_reviewed"));}
    if p.a.workspace.join(&p.key).exists()||p.a.workspace.join("native-phases.txt").exists(){return Err(fail("preview_attempt_already_exists"));}
    {let d=app.db.lock().map_err(|_|fail("storage_unavailable"))?;setup(&d)?;
     d.execute("INSERT INTO snapmatic_preview_jobs_v2 VALUES (?1,?2,?3,'queued',NULL,NULL)",
        rusqlite::params![p.key,CLIP,p.request.source_revision]).map_err(|_|fail("preview_attempt_already_exists"))?;}
    let key=p.key.clone();let child_app=app.clone();let k=key.clone();
    if std::thread::Builder::new().name("snapmatic-preview".into()).spawn(move||{
        if let Err(e)=produce(&child_app,&p){let _=advance(&child_app,&p.key,"hold",None,Some(e.0));}
    }).is_err(){let _=advance(&app,&k,"hold",None,Some("preview_supervisor_unavailable"));return Err(fail("preview_supervisor_unavailable"));}
    Ok(json!({"job_id":key,"stage":"queued","publishing_enabled":false}))
 }
 pub(super) fn open(app:&App,key:&str)->store::Result<(PathBuf,File)>{
    let (_,p,f)=stored_ready(app,key)?;Ok((p,f))
 }
}
pub(crate) fn status(app:&App)->Value{
    #[cfg(windows)] {return win::status(app).unwrap_or_else(|e|json!({"clip_id":CLIP,"can_dispatch":false,"reason":e.0,"publishing_enabled":false}));}
    #[allow(unreachable_code)] { let _=app;json!({"clip_id":CLIP,"can_dispatch":false,"reason":"native_windows_required","publishing_enabled":false})}
}
pub(crate) fn dispatch(app:App,r:Request)->store::Result<Value>{
    #[cfg(windows)] {win::dispatch(app,r)}
    #[cfg(not(windows))] {let _=(app,r);Err(store::Error("native_windows_required"))}
}
pub(crate) fn open(app:&App,key:&str)->store::Result<(PathBuf,File)>{
    if !media::hex(key){return Err(store::Error("preview_unavailable"));}
    #[cfg(windows)] {win::open(app,key)}
    #[cfg(not(windows))] {let _=app;Err(store::Error("native_windows_required"))}
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn exact_source_trim_and_grounded_edits(){
        let mut r=Request{schema:"snapmatic-preview-transcode-v1".into(),clip_id:CLIP.into(),source_sha256:SOURCE.into(),
            source_revision:SOURCE_REV.into(),in_ms:0,out_ms:DURATION,framing:Framing::Center,transcript_sha256:None,edits:Default::default()};
        assert!(valid(&r,&[],false));
        r.out_ms+=1;assert!(!valid(&r,&[],false));r.out_ms=DURATION;
        r.edits.insert("line-1".into(),"literal {not markup}".into());
        assert!(!valid(&r,&[],false));
        r.transcript_sha256=Some("a".repeat(64));assert!(!valid(&r,&[],true));
        let s=Segment{id:"line-1".into(),start_ms:100,end_ms:500,text:"grounded original".into()};
        assert!(valid(&r,&[s.clone()],true));
        let bad=Segment{id:"line-2".into(),start_ms:400,end_ms:600,text:"overlap".into()};
        assert!(!valid(&r,&[s,bad],true));
        r.framing=Framing::DualZone{face:[0,0,1442,720],game:[0,720,1440,1800]};assert!(!valid(&r,&[],true));
    }
    #[test] fn fresh_registry_keeps_previous_hold_immutable(){
        let db=rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE snapmatic_preview_jobs_v1 (request_key TEXT, state TEXT);
            INSERT INTO snapmatic_preview_jobs_v1 VALUES ('old', 'hold');").unwrap();
        setup(&db).unwrap();
        assert!(events(&db).unwrap().is_empty());
        let k="a".repeat(64);
        db.execute("INSERT INTO snapmatic_preview_jobs_v2 VALUES (?1,?2,?3,'queued',NULL,NULL)",
            rusqlite::params![k,CLIP,SOURCE_REV]).unwrap();
        assert_eq!(events(&db).unwrap()[0]["stage"],"queued");
        assert_eq!(db.query_row("SELECT state FROM snapmatic_preview_jobs_v1 WHERE request_key='old'",[],
            |r|r.get::<_,String>(0)).unwrap(),"hold");
        assert!(db.execute("INSERT INTO snapmatic_preview_jobs_v2 VALUES ('other',?1,?2,'queued',NULL,NULL)",
            rusqlite::params![CLIP,SOURCE_REV]).is_err());
    }
    #[test] fn fresh_receipt_reads_nominal_and_measured_separately(){
        let r:Receipt=serde_json::from_value(json!({
            "schema":"snapmatic-preview-receipt-v1","request_sha256":"a".repeat(64),
            "source_sha256":SOURCE,"source_revision":SOURCE_REV,"sha256":"b".repeat(64),"bytes":123,
            "audio_target_bps":128000,"audio_measured_bps":129420,"audio":{"integrated_lufs":-15.08,"true_peak_dbtp":-1.48,"status":"measured"},
            "publication_enabled":false
        })).unwrap();
        assert_eq!((r.audio_target_bps,r.audio_measured_bps),(128000,129420));
        assert!(audio_valid(&r.audio));
    }
    #[test] fn measured_not_invented(){
        let mut a=Audio{integrated_lufs:Some(-14.0),true_peak_dbtp:Some(-1.5),status:"measured".into()};
        assert!(audio_valid(&a));a.true_peak_dbtp=None;assert!(!audio_valid(&a));
        a.status="silence".into();assert!(!audio_valid(&a));a.integrated_lufs=None;assert!(audio_valid(&a));
    }
}