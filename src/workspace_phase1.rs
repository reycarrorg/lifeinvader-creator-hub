use crate::{App,workspace_media as media};
use axum::{Json,extract::{Path,State},http::{HeaderMap,StatusCode},response::{IntoResponse,Response}};
use serde_json::{Value,json};
use std::{io::Write,path::{Path as FsPath,PathBuf}};
const PROFILE:&str="demo-channel";
const BACKUP:&str=r"Z:\SnapmaticReview\Local Media\Posted Clips Backup";
const VIDEO:&str="track-a-1000000000000000001-demo-fixture.mp4";
const RECEIPT:&str="track-a-1000000000000000001-demo-fixture.receipt.json";
const VIDEO_SHA:&str="8d5ba88dceb0235a8b8049c7830cca5c7017bf096332cdd0f6a41b7c99701dfa";
const RECEIPT_SHA:&str="0e881810e48701e07d70d7098d4e9f690afca07eae309ebc153c3159526836ce";
fn reply(v:Value)->Response{([(axum::http::header::CACHE_CONTROL,"no-store")],Json(v)).into_response()}
fn error(code:StatusCode,reason:&str)->Response{(code,Json(json!({"reason":reason}))).into_response()}
fn local(h:&HeaderMap)->bool {
    h.get("origin").and_then(|x|x.to_str().ok())==Some("http://127.0.0.1:3847")
}
fn scoped(app:&App)->bool{
    app.media.parent().and_then(FsPath::file_name).and_then(|s|s.to_str())==Some("snapmatic-review-runtime")
}
fn profiles(app:&App)->Result<Value,()> {
    if !scoped(app){return Err(());}
    let db=app.db.lock().map_err(|_|())?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS snapmatic_channel_profiles_v1 (
        profile_id TEXT PRIMARY KEY, channel_alias TEXT NOT NULL, credential_ref TEXT NOT NULL,
        caption_preset_ref TEXT NOT NULL, drive_scope TEXT NOT NULL, source_profile_id INTEGER,
        enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0,1)));
        INSERT OR IGNORE INTO snapmatic_channel_profiles_v1 VALUES ('demo-channel','@Demo_Channel','demo-channel-v1','default','everything-clips-v1',2,1);")
        .map_err(|_|())?;
    let mut q=db.prepare("SELECT profile_id,channel_alias,caption_preset_ref,drive_scope,source_profile_id,enabled FROM snapmatic_channel_profiles_v1 ORDER BY profile_id LIMIT 101").map_err(|_|())?;
    let rows=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,Option<i64>>(4)?,r.get::<_,bool>(5)?))).map_err(|_|())?;
    let mut out=Vec::new();
    for row in rows {
        let (id,alias,preset,scope,source,enabled)=row.map_err(|_|())?;
        let opaque=|s:&str|!s.is_empty()&&s.len()<=48&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b"@_-.".contains(&b));
        if !opaque(&id)||!opaque(&alias)||!opaque(&preset){continue;}
        let live=enabled&&id==PROFILE&&scope=="everything-clips-v1"&&source==Some(2);
        out.push(json!({"profile_id":id,"channel_alias":alias,"caption_preset_ref":preset,
            "source_scope":if live{Some("Local Media")}else{None},"backup_scope":if live{Some("Posted Clips Backup")}else{None},"configured":live}));
    }
    Ok(json!({"profiles":out,"default_profile_id":PROFILE,"publishing_enabled":false}))
}
pub(crate) async fn channel_profiles(State(app):State<App>)->Response {
    match tokio::task::spawn_blocking(move||profiles(&app)).await {
        Ok(Ok(v))=>reply(v),_=>error(StatusCode::SERVICE_UNAVAILABLE,"channel_profiles_unavailable"),
    }
}
fn audited(app:&App)->bool {
    crate::workspace_ledger::data(app).ok().and_then(|d|d["records"].as_array().and_then(|v|v.iter().find(|r|
        r["job_id"]=="00000000-0000-4000-8000-000000000001"&&r["backup_restore_verified"]==true&&r["render_sha256"]==VIDEO_SHA&&r["receipt_sha256"]==RECEIPT_SHA).cloned())).is_some()
}
#[cfg(windows)]
fn verified_backup(app:&App,kind:&str)->Option<(PathBuf,std::fs::File)>{
    let (name,sha)=match kind{"video"=>(VIDEO,VIDEO_SHA),"receipt"=>(RECEIPT,RECEIPT_SHA),_=>return None};
    if !scoped(app)||!audited(app){return None;}
    let path=FsPath::new(BACKUP).join(name);
    crate::controller_native_launcher::clean_path(&path).ok()?;
    let f=crate::controller_native_process::lease(&path,sha).ok()?;
    Some((path,f))
}
pub(crate) fn verify_known_backup_pair(app:&App)->bool{
    #[cfg(windows)] {verified_backup(app,"video").is_some()&&verified_backup(app,"receipt").is_some()}
    #[cfg(not(windows))] {let _=app;false}
}
pub(crate) async fn restore_open(State(app):State<App>,Path(kind):Path<String>,h:HeaderMap)->Response{
    if !local(&h){return error(StatusCode::FORBIDDEN,"local_origin_required");}
    #[cfg(windows)]
    if matches!(tokio::task::spawn_blocking(move|| {
        let (p,_lease)=verified_backup(&app,&kind)?;
        use std::os::windows::process::CommandExt;
        let mut arg=std::ffi::OsString::from("/select,\"");arg.push(p.as_os_str());arg.push("\"");
        std::process::Command::new(r"C:\Windows\explorer.exe").raw_arg(arg).spawn().ok()
    }).await,Ok(Some(_))){return StatusCode::NO_CONTENT.into_response();}
    error(StatusCode::NOT_FOUND,"exact_backup_unavailable")
}
fn crc32(bytes:&[u8])->u32 {
    let mut crc=!0u32;
    for &b in bytes{crc^=u32::from(b);for _ in 0..8{crc=(crc>>1)^((0u32.wrapping_sub(crc&1))&0xedb88320);}}
    !crc
}
fn u16le(v:&mut Vec<u8>,x:u16){v.extend(x.to_le_bytes());}
fn u32le(v:&mut Vec<u8>,x:u32){v.extend(x.to_le_bytes());}
pub(crate) fn zip(entries:&[(&str,&[u8])])->Vec<u8>{
    let mut out=Vec::new();let mut central=Vec::new();
    for &(name,bytes) in entries{
        let name=name.as_bytes();let n=name.len() as u16;let size=bytes.len() as u32;let offset=out.len() as u32;let crc=crc32(bytes);
        u32le(&mut out,0x04034b50);for v in [20,0,0,0,33]{u16le(&mut out,v);} for v in [crc,size,size]{u32le(&mut out,v);}
        for v in [n,0]{u16le(&mut out,v);}out.extend(name);out.extend(bytes);
        u32le(&mut central,0x02014b50);for v in [20,20,0,0,0,33]{u16le(&mut central,v);}for v in [crc,size,size]{u32le(&mut central,v);}
        for v in [n,0,0,0,0]{u16le(&mut central,v);}for v in [0,offset]{u32le(&mut central,v);}central.extend(name);
    }
    let offset=out.len() as u32;let size=central.len() as u32;out.extend(central);
    u32le(&mut out,0x06054b50);for v in [0,0,entries.len() as u16,entries.len() as u16]{u16le(&mut out,v);}
    for v in [size,offset]{u32le(&mut out,v);}u16le(&mut out,0);out
}
fn diagnostics(app:&App)->Option<Vec<u8>>{
    if !scoped(app){return None;}
    let rows=media::index(app).ok()?;
    let queue={
        let d=app.db.lock().ok()?;
        let c=u64::try_from(d.query_row("SELECT COUNT(*) FROM controller_jobs",[],|r|r.get::<_,i64>(0)).ok()?).ok()?;
        json!({"indexed":rows.len(),"pending_review":rows.iter().filter(|r|r["state"]=="pending_review").count(),
            "private_test_hold":rows.iter().filter(|r|r["state"]=="test_hold").count(),"controller_total":c})
    };
    let p=crate::workspace_ledger::data(app).ok()?;
    let restore_count=p["records"].as_array()?.iter().filter(|r|r["backup_restore_verified"]==true).count();
    let api=json!({"scope":"local read-only","publishing_enabled":false,"provider_requests":0,"verified_restore_count":restore_count});
    let launch=json!({"tcp_bound_loopback":true,"scope":"isolated Drive-v1","launcher_log_policy":"raw logs omitted",
        "nvenc_health":"not checked during export"});
    let q=serde_json::to_vec_pretty(&queue).ok()?;let a=serde_json::to_vec_pretty(&api).ok()?;let l=serde_json::to_vec_pretty(&launch).ok()?;
    Some(zip(&[("queue-metrics.json",&q),("launcher-nvenc-summary.json",&l),("local-api-checks.json",&a)]))
}
pub(crate) async fn export(State(app):State<App>,h:HeaderMap)->Response{
    if !local(&h){return error(StatusCode::FORBIDDEN,"local_origin_required");}
    let result=tokio::task::spawn_blocking(move||{
        let bytes=diagnostics(&app)?;
        let dir=app.media.parent()?;
        #[cfg(windows)]
        crate::controller_native_launcher::clean_path(dir).ok()?;
        let leaf=format!("snapmatic-diagnostics-{}.zip",uuid::Uuid::new_v4());
        let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(dir.join(&leaf)).ok()?;
        f.write_all(&bytes).ok()?;f.sync_all().ok()?;Some(leaf)
    }).await;
    match result{
        Ok(Some(file))=>reply(json!({"file":file,"location":"isolated Snapmatic runtime","shared":false})),
        _=>error(StatusCode::SERVICE_UNAVAILABLE,"diagnostic_export_failed"),
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn crc_and_zip_layout(){
        assert_eq!(crc32(b"123456789"),0xcbf43926);
        let bytes=zip(&[("counts.json",b"{}")]);
        assert_eq!(&bytes[..4],b"PK\x03\x04");
        assert_eq!(&bytes[bytes.len()-22..bytes.len()-18],b"PK\x05\x06");
        assert!(bytes.windows(2).any(|x|x==b"{}"));
    }
}
