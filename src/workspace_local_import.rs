//! Explicit local-file import. The browser supplies bytes, never a filesystem path.
use crate::{App,controller_store as store,workspace_media as media};
use axum::{body::Body,extract::State,http::{HeaderMap,StatusCode,header},response::{IntoResponse,Response},Json};
use futures_util::StreamExt;
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{fs::File,io::{Read,Seek,SeekFrom,Write},path::{Path,PathBuf},sync::atomic::{AtomicBool,Ordering}};

const MAX:u64=180*1024*1024;
#[derive(Debug,Clone,Default,serde::Deserialize,serde::Serialize)]
#[serde(default,deny_unknown_fields)]
struct Sidecar{title:Option<String>,description:Option<String>,uploader:Option<String>,upload_date:Option<String>,tags:Vec<String>,id:Option<String>}
fn sidecar(h:&HeaderMap)->Result<Option<Sidecar>,()>{
 let Some(raw)=h.get("x-snapmatic-sidecar") else{return Ok(None);};
 let raw=raw.to_str().map_err(|_|())?.as_bytes();
 if raw.is_empty()||raw.len()>4000||raw.len()%2!=0{return Err(());}
 let hex=|n:u8|match n{b'0'..=b'9'=>Some(n-b'0'),b'a'..=b'f'=>Some(n-b'a'+10),_=>None};
 let raw=raw.chunks_exact(2).map(|c|Some(hex(c[0])?*16+hex(c[1])?)).collect::<Option<Vec<_>>>().ok_or(())?;
 let x:Sidecar=serde_json::from_slice(&raw).map_err(|_|())?;
 Ok(Some(sanitize(x)))
}
fn sanitize(mut x:Sidecar)->Sidecar{
 let clean=|text:Option<String>,cap:usize|text.and_then(|s|{
  let t=s.trim();let lower=t.to_ascii_lowercase();
  (!t.is_empty()&&t.len()<=cap&&!t.chars().any(|c|c.is_control()&&c!='\n'&&c!='\t')
   &&!["://","www.","cookie","bearer ","authorization","token","password","api_key","sessionid","eyj"].iter().any(|needle|lower.contains(needle)))
  .then(||t.to_owned())
 });
 x.title=clean(x.title,250);x.description=clean(x.description,800);
 x.uploader=x.uploader.filter(|s|(1..=64).contains(&s.len())&&s.bytes().all(|c|c.is_ascii_alphanumeric()||b"@_.".contains(&c)));
 x.id=x.id.filter(|s|(15..=22).contains(&s.len())&&s.bytes().all(|c|c.is_ascii_digit()));
 x.upload_date=x.upload_date.filter(|s|s.len()==8&&chrono::NaiveDate::parse_from_str(s,"%Y%m%d").is_ok());
 x.tags.truncate(20);
 x.tags.retain(|t|!t.is_empty()&&t.len()<=50&&t.chars().all(|c|c.is_alphanumeric()||matches!(c,'#'|'_')));
 x
}
#[cfg(test)]
mod metadata_tests{
 use super::*;
 fn header(raw:&str)->HeaderMap{
  let mut h=HeaderMap::new();
  h.insert("x-snapmatic-sidecar",raw.bytes().map(|b|format!("{b:02x}")).collect::<String>().parse().unwrap());
  h
 }
 #[test] fn sidecar_preserves_only_safe_unverified_clip_facts(){
  let h=header(r##"{"title":"Demo 🎥","description":"https://not-used.invalid/media","uploader":"@example","upload_date":"20260230","id":"7710000000000000001","tags":["#GTA","url.invalid","safe_tag"]}"##);
  let x=sidecar(&h).unwrap().unwrap();
  assert_eq!(x.title.as_deref(),Some("Demo 🎥"));assert!(x.description.is_none()&&x.upload_date.is_none());
  assert_eq!(x.id.as_deref(),Some("7710000000000000001"));assert_eq!(x.tags,["#GTA","safe_tag"]);
  assert!(sidecar(&header(r#"{"cookies":"never-retained"}"#)).is_err());
 }
}
const PROTECTED:[&str;4]=[
 "8d5ba88dceb0235a8b8049c7830cca5c7017bf096332cdd0f6a41b7c99701dfa",
 "133883ce3f0222a43a90db28b127f5c1dbd33f5336a598da713f703165fe86fd",
 "3c45bfc8aadcaaae1bef5c5beda8525899e8113254efde014019b5cde3ea7e89",
 "c157e6bf0f844b99932be9d12525e0f8137b88a9072eac01cc08bfe24c4dcdc6"
];
static OWNER:AtomicBool=AtomicBool::new(false);
struct Lease;
impl Drop for Lease{fn drop(&mut self){OWNER.store(false,Ordering::Release);}}
fn err(code:StatusCode,reason:&'static str)->Response{(code,Json(json!({"reason":reason,"publishing_enabled":false}))).into_response()}
fn data(v:Value)->Response{([(header::CACHE_CONTROL,"no-store")],Json(v)).into_response()}
fn scope_fixture(app:&App)->bool{
 app.media.parent().and_then(|p|p.file_name()).and_then(|p|p.to_str()).is_some_and(|x|x.starts_with("snapmatic-local-import-qa-"))
}
fn intent(app:&App,h:&HeaderMap)->bool{
 let origin=h.get(header::ORIGIN).and_then(|x|x.to_str().ok());
 let exact=h.get("x-snapmatic-import").and_then(|x|x.to_str().ok());
 exact==Some("authorized-local-mp4")&&(origin==Some("http://127.0.0.1:3847")||(scope_fixture(app)&&matches!(origin,Some("http://127.0.0.1:43852"|"http://127.0.0.1:43858"))))
}
fn declared(h:&HeaderMap)->Option<u64>{
 if !matches!(h.get(header::CONTENT_TYPE).and_then(|r|r.to_str().ok()),Some("video/mp4"|"application/octet-stream")){return None;}
 let n:u64=h.get(header::CONTENT_LENGTH)?.to_str().ok()?.parse().ok()?;
 (12..=MAX).contains(&n).then_some(n)
}
fn basic_mp4(bytes:&[u8])->bool{
 bytes.len()>=12&&bytes[4..8]==*b"ftyp"&&u32::from_be_bytes(bytes[..4].try_into().unwrap_or([0;4]))>=12
}
fn exists(db:&Connection)->store::Result<bool>{
 Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='snapmatic_authorized_import_v1')",[],|r|r.get(0))?)
}
fn init(db:&Connection)->store::Result<()>{
 db.execute_batch("CREATE TABLE IF NOT EXISTS snapmatic_authorized_import_v1(
  clip_id TEXT PRIMARY KEY,media_sha TEXT NOT NULL UNIQUE,media_bytes INTEGER NOT NULL,stage TEXT NOT NULL,
  poster_sha TEXT, failure_code TEXT, job_id TEXT NOT NULL UNIQUE, updated_at INTEGER NOT NULL);
  CREATE TABLE IF NOT EXISTS snapmatic_import_sidecar_v1(clip_id TEXT PRIMARY KEY REFERENCES snapmatic_authorized_import_v1(clip_id),safe_json TEXT NOT NULL);")?;Ok(())
}
#[derive(Clone)]
struct Record{id:String,sha:String,bytes:u64,job_id:String,poster_sha:String}
impl Record{
 fn valid(&self)->bool{media::hex(&self.id)&&media::hex(&self.sha)&&media::hex(&self.poster_sha)&&(12..=MAX).contains(&self.bytes)&&
    uuid::Uuid::parse_str(&self.job_id).ok().is_some_and(|v|v.to_string()==self.job_id)}
 fn projection(&self)->Value{
  json!({"clip_id":self.id,"kind":"imported_original","state":"pending_review","name":"Authorized local MP4",
   "preview_only":true,"preview_available":true,"poster_available":true,"preview_bytes":self.bytes,"preview_sha256":self.sha})
 }
}
fn record(r:&rusqlite::Row)->rusqlite::Result<Record>{
 Ok(Record{id:r.get(0)?,sha:r.get(1)?,bytes:u64::try_from(r.get::<_,i64>(2)?).unwrap_or(0),job_id:r.get(3)?,poster_sha:r.get(4)?})
}
fn identity(sha:&str)->String{
 let mut h=Sha256::new();h.update(b"snapmatic-authorized-import-v1");h.update([0]);h.update(sha.as_bytes());format!("{:x}",h.finalize())
}
pub(crate) fn rows(app:&App)->store::Result<Vec<Value>>{
 let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
 if !exists(&db)?{return Ok(vec![]);}
 let mut q=db.prepare("SELECT clip_id,media_sha,media_bytes,job_id,poster_sha FROM snapmatic_authorized_import_v1 WHERE stage='pending_review' ORDER BY updated_at DESC,clip_id LIMIT 500")?;
 let list=q.query_map([],record)?.collect::<Result<Vec<_>,_>>()?;
 let meta_exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='snapmatic_import_sidecar_v1')",[],|r|r.get(0))?;
 let mut rows=Vec::new();
 for r in list.into_iter().filter(Record::valid){
  let mut p=r.projection();
  if meta_exists{
   let raw:Option<String>=db.query_row("SELECT safe_json FROM snapmatic_import_sidecar_v1 WHERE clip_id=?1",[&r.id],|q|q.get(0)).optional()?;
   if let Some(v)=raw.and_then(|s|serde_json::from_str::<Sidecar>(&s).ok()){
    if let Some(title)=v.title.as_ref(){p["name"]=json!(title);}
    p["import_metadata"]=json!(v);p["import_metadata_trust"]=json!("user_supplied_unverified");
   }
  }
  rows.push(p)
 }
 Ok(rows)
}
#[cfg(windows)]
fn scope(app:&App)->store::Result<PathBuf>{
 let root=PathBuf::from(std::env::var_os("LIFEINVADER_LOCAL_IMPORT_SCOPE").ok_or(store::Error("local_import_not_configured"))?);
 let base=app.media.parent().and_then(Path::parent).ok_or(store::Error("local_import_scope_invalid"))?;
 let leaf="snapmatic-selected-imports-v2";
 if root!=base.join(leaf)||!root.is_dir()||!matches!(root.components().next(),Some(std::path::Component::Prefix(p)) if matches!(p.kind(),std::path::Prefix::Disk(b'C'|b'c'))){
  return Err(store::Error("local_import_scope_invalid"));
 }
 crate::controller_native_launcher::clean_path(&root)?;Ok(root)
}
#[cfg(windows)]
fn files(root:&Path,r:&Record,poster:bool)->store::Result<(PathBuf,File)>{
 if !r.valid(){return Err(store::Error("import_asset_unverified"));}
 let leaf=if poster{"poster.webp"}else{"media.mp4"};
 let path=root.join("objects").join(&r.sha).join(leaf);
 crate::controller_native_launcher::clean_path(&path)?;
 let mut f=crate::controller_native_process::lease(&path,if poster{&r.poster_sha}else{&r.sha})?;
 let size=f.metadata().map_err(|_|store::Error("import_asset_unverified"))?.len();
 if poster{
  let mut bytes=[0u8;30];
  if !(30..=2*1024*1024).contains(&size)||f.seek(SeekFrom::Start(0)).and_then(|_|f.read_exact(&mut bytes)).is_err()||
      crate::workspace_phase4::webp_dimensions(&bytes)!=Some((540,960)){return Err(store::Error("import_poster_unverified"));}
 }else if size!=r.bytes{return Err(store::Error("import_asset_unverified"));}
 Ok((path,f))
}
pub(crate) fn open(app:&App,id:&str,poster:bool)->store::Result<(PathBuf,File)>{
 #[cfg(windows)]{
 let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
 if !media::hex(id)||!exists(&db)?{return Err(store::Error("preview_unavailable"));}
 let row=db.query_row("SELECT clip_id,media_sha,media_bytes,job_id,poster_sha FROM snapmatic_authorized_import_v1 WHERE stage='pending_review' AND clip_id=?1",[id],record)
   .optional()?.ok_or(store::Error("preview_unavailable"))?;
 drop(db);
 return files(&scope(app)?,&row,poster);
 }
 #[allow(unreachable_code)]{let _=(app,id,poster);Err(store::Error("preview_unavailable"))}
}
#[cfg(windows)]
async fn receive(mut body:axum::body::BodyDataStream,declared:u64,path:&Path)->store::Result<String>{
 let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|_|store::Error("import_stage_create_failed"))?;
 let mut digest=Sha256::new();let mut received=0u64;let mut prefix=Vec::with_capacity(12);
 while let Some(c)=body.next().await{
  let bytes=c.map_err(|_|store::Error("import_body_interrupted"))?;
  let next=received.checked_add(bytes.len() as u64).ok_or(store::Error("import_size_limit"))?;
  if next>declared||next>MAX{return Err(store::Error("import_size_limit"));}
  if prefix.len()<12{prefix.extend(bytes.iter().take(12-prefix.len()));if prefix.len()==12&&!basic_mp4(&prefix){return Err(store::Error("import_not_mp4"));}}
  file.write_all(&bytes).map_err(|_|store::Error("import_stage_write_failed"))?;
  digest.update(&bytes);received=next;
 }
 if received!=declared||!basic_mp4(&prefix){return Err(store::Error("import_length_or_type_unverified"));}
 file.sync_all().map_err(|_|store::Error("import_stage_write_failed"))?;
 Ok(format!("{:x}",digest.finalize()))
}
fn prior(app:&App,sha:&str)->store::Result<bool>{
 if PROTECTED.contains(&sha){return Err(store::Error("import_protected_artifact"));}
 let d=crate::workspace_media::index(app)?.into_iter().chain(crate::workspace_phase4::rows(app)?)
   .chain(crate::workspace_source_download::rows(app)?).any(|r|r["preview_sha256"].as_str()==Some(sha));
 Ok(d)
}
pub(crate) async fn status(State(app):State<App>)->Response{
 #[cfg(windows)] let app_status=app.clone();
 #[cfg(windows)] let enabled=tokio::task::spawn_blocking(move||native::load(&app).is_ok()).await.unwrap_or(false);
 #[cfg(not(windows))] let enabled={let _=app;false};
 #[cfg(windows)] let inbox=enabled&&inbox_path(&app_status).is_some_and(|dir|
  dir.is_dir()&&crate::controller_native_launcher::clean_path(&dir).is_ok())&&std::env::var("LIFEINVADER_LOCAL_IMPORT_INBOX").ok().as_deref()==Some("1");
 #[cfg(not(windows))]let inbox=false;
 data(json!({"import_enabled":enabled,"watcher_enabled":inbox,"requires_ready_seal":true,"max_bytes":MAX,"publishing_enabled":false,
  "reason":if enabled{None}else{Some("local_import_adapter_unconfigured")}}))
}
pub(crate) async fn submit(State(app):State<App>,h:HeaderMap,body:Body)->Response{
 if !intent(&app,&h){return err(StatusCode::FORBIDDEN,"explicit_local_import_required");}
 let Some(size)=declared(&h)else{return err(StatusCode::UNPROCESSABLE_ENTITY,"import_size_or_type_invalid")};
 let meta=match sidecar(&h){Ok(v)=>v,Err(())=>return err(StatusCode::UNPROCESSABLE_ENTITY,"import_sidecar_invalid")};
 #[cfg(windows)] {
  let a=app.clone();
  let (root,authority)=match tokio::task::spawn_blocking(move||native::load(&a)).await{
    Ok(Ok(v))=>v,Ok(Err(e))=>return err(StatusCode::SERVICE_UNAVAILABLE,e.0),
    Err(_)=>return err(StatusCode::SERVICE_UNAVAILABLE,"local_import_not_configured"),
  };
  if OWNER.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return err(StatusCode::CONFLICT,"import_already_running");}
  let owner=Lease;
  let job=uuid::Uuid::new_v4().to_string();
  let dir=root.join(format!("control-{job}"));
  if std::fs::create_dir(&dir).is_err()||crate::controller_native_launcher::clean_path(&dir).is_err(){
   return err(StatusCode::SERVICE_UNAVAILABLE,"import_stage_create_failed");
  }
  let accepted=dir.join("accepted.mp4");
  let sha=match tokio::time::timeout(std::time::Duration::from_secs(80),receive(body.into_data_stream(),size,&accepted)).await{
   Ok(Ok(v))=>v,Ok(Err(e))=>return err(StatusCode::UNPROCESSABLE_ENTITY,e.0),Err(_)=>return err(StatusCode::REQUEST_TIMEOUT,"import_receive_timed_out"),
  };
  let id=identity(&sha);
  let result=tokio::task::spawn_blocking(move||{
   let _owner=owner;
   if prior(&app,&sha)?{return Ok(json!({"state":"duplicate","publishing_enabled":false}));}
   {
    let mut db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
    if let Some(old)=existing(&db,&sha)?{
     if old=="pending_review"{return Ok(json!({"state":"duplicate","clip_id":id,"publishing_enabled":false}));}
     return Err(store::Error("import_previous_attempt_held"));
    }
    if !claim(&mut db,&id,&sha,size,&job)?{return Err(store::Error("import_claim_unverified"));}
   }
   if let Err(e)=native::run(&app,&root,authority,&job,&id,&sha,size,&dir,meta.as_ref()){
    if let Ok(db)=app.db.lock(){let _=hold(&db,&id,e.0);}
    return Err(e);
   }
   Ok(json!({"state":"pending_review","clip_id":id,"sha256":sha,"publishing_enabled":false}))
  }).await;
  return match result{
   Ok(Ok(x))=>data(x),Ok(Err(e))=>err(StatusCode::CONFLICT,e.0),
   Err(_)=>err(StatusCode::SERVICE_UNAVAILABLE,"import_outcome_unverified"),
  };
 }
 #[allow(unreachable_code)]{let _=(app,h,body,size);err(StatusCode::SERVICE_UNAVAILABLE,"local_import_not_configured")}
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn admission_requires_bounded_mp4_and_distinct_identity(){
  assert!(basic_mp4(b"\0\0\0\x18ftypisom"));assert!(!basic_mp4(b"\0\0\0\x18moovisom"));
  let mut h=HeaderMap::new();h.insert(header::CONTENT_TYPE,"video/mp4".parse().unwrap());
  h.insert(header::CONTENT_LENGTH,"12".parse().unwrap());assert_eq!(declared(&h),Some(12));
  h.insert(header::CONTENT_LENGTH,"188743681".parse().unwrap());assert_eq!(declared(&h),None);
  assert_ne!(identity(&"a".repeat(64)),identity(&"b".repeat(64)));
 }
}

fn existing(db:&Connection,sha:&str)->store::Result<Option<String>>{
 if !exists(db)?{return Ok(None);}
 Ok(db.query_row("SELECT stage FROM snapmatic_authorized_import_v1 WHERE media_sha=?1",[sha],|r|r.get(0)).optional()?)
}
fn claim(db:&mut Connection,id:&str,sha:&str,size:u64,job:&str)->store::Result<bool>{
 if !media::hex(id)||!media::hex(sha)||id!=identity(sha)||!(12..=MAX).contains(&size){return Err(store::Error("import_unsafe_selection"));}
 init(db)?;
 let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 if existing(&tx,sha)?.is_some(){return Ok(false);}
 let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM snapmatic_authorized_import_v1 WHERE stage='validating')",[],|r|r.get(0))?;
 if active{return Err(store::Error("import_unresolved_previous"));}
 tx.execute("INSERT INTO snapmatic_authorized_import_v1(clip_id,media_sha,media_bytes,stage,job_id,updated_at) VALUES(?1,?2,?3,'validating',?4,?5)",
 params![id,sha,size as i64,job,chrono::Utc::now().timestamp()])?;
 tx.commit()?;Ok(true)
}
fn finish(db:&mut Connection,r:&Record,meta:Option<&Sidecar>)->store::Result<()>{
 if !r.valid()||r.id!=identity(&r.sha){return Err(store::Error("import_receipt_invalid"));}
 let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 let n=tx.execute("UPDATE snapmatic_authorized_import_v1 SET stage='pending_review',poster_sha=?1,updated_at=?2
 WHERE stage='validating' AND clip_id=?3 AND media_sha=?4 AND media_bytes=?5 AND job_id=?6",
 params![r.poster_sha,chrono::Utc::now().timestamp(),r.id,r.sha,r.bytes as i64,r.job_id])?;
 if n!=1{return Err(store::Error("import_state_changed"));}
 if let Some(meta)=meta{
  let v=serde_json::to_string(meta).map_err(|_|store::Error("import_sidecar_invalid"))?;
  tx.execute("INSERT INTO snapmatic_import_sidecar_v1(clip_id,safe_json) VALUES(?1,?2)",params![r.id,v])?;
 }
 tx.commit()?;Ok(())
}
fn hold(db:&Connection,id:&str,why:&'static str)->store::Result<()>{
 db.execute("UPDATE snapmatic_authorized_import_v1 SET stage='held',failure_code=?2,updated_at=?3 WHERE clip_id=?1 AND stage='validating'",
  params![id,why,chrono::Utc::now().timestamp()])?;Ok(())
}



#[cfg(windows)]
mod native {
 use super::*;
 use serde::Deserialize;
 use std::{collections::BTreeMap,time::{Duration,Instant}};
 const NAME:&str="selected_ingest_cli_v2.py";
 const CLI_SHA:&str="da30a7dffd9760b0e598be1edeec586695e761f5f372d681ae735d38b81f06b2";
 // The trusted installation pins the exact independently reviewed CLI and dependency inventory.
 const PYTHON_SHA:&str="4d6f5f81a4bca11191c4c7c6b43632694d0a4ce74e068619d8fdc161d469859a";
 #[derive(Deserialize)]
 #[serde(deny_unknown_fields)]
 pub(super) struct Authority{schema:String,pub cli:PathBuf,pub cli_sha256:String,python:PathBuf,files:BTreeMap<String,String>,tools:Tools}
 #[derive(Deserialize,serde::Serialize)]
 #[serde(deny_unknown_fields)]
 struct Tools{ffmpeg:String,ffmpeg_sha256:String,ffprobe:String,ffprobe_sha256:String}
 fn fail(s:&'static str)->store::Error{store::Error(s)}
 fn read(p:&Path,cap:u64)->store::Result<Vec<u8>>{
  use std::os::windows::fs::{MetadataExt,OpenOptionsExt};
  crate::controller_native_launcher::clean_path(p)?;
  let f=std::fs::OpenOptions::new().read(true).share_mode(1).custom_flags(0x00200000).open(p).map_err(|_|fail("import_proof_missing"))?;
  let m=f.metadata().map_err(|_|fail("import_proof_missing"))?;
  if !m.is_file()||m.file_attributes()&0x400!=0||!(1..=cap).contains(&m.len()){return Err(fail("import_proof_invalid"));}
  let mut b=Vec::new();f.take(cap+1).read_to_end(&mut b).map_err(|_|fail("import_proof_invalid"))?;
  if b.len() as u64!=m.len(){return Err(fail("import_proof_invalid"));}Ok(b)
 }
 fn sha(b:&[u8])->String{format!("{:x}",Sha256::digest(b))}
 pub(super) fn load(app:&App)->store::Result<(PathBuf,Authority)>{
  let root=scope(app)?;
  let pin=std::env::var("LIFEINVADER_LOCAL_IMPORT_AUTHORITY_SHA256").map_err(|_|fail("local_import_not_configured"))?;
  if !media::hex(&pin){return Err(fail("local_import_authority_invalid"));}
  let bytes=read(&root.join("import-authority.json"),65536)?;
  if sha(&bytes)!=pin{return Err(fail("local_import_authority_pin"));}
  let a:Authority=serde_json::from_slice(&bytes).map_err(|_|fail("local_import_authority_invalid"))?;
  let at=|p:&Path|a.files.get(&p.to_string_lossy().into_owned()).map(String::as_str);
  let cli_parent=a.cli.parent();
  if a.schema!="snapmatic-local-import-authority-v1"
    ||a.cli.file_name().and_then(|x|x.to_str())!=Some(NAME)
    ||cli_parent.and_then(|x|x.file_name()).and_then(|x|x.to_str())!=Some("clip_worker")
    ||cli_parent.and_then(Path::parent).and_then(Path::parent).and_then(Path::parent)!=root.parent()
    ||a.cli_sha256!=CLI_SHA||at(&a.cli)!=Some(CLI_SHA)
    ||at(&a.python)!=Some(PYTHON_SHA)||a.files.len()>128
    ||at(Path::new(&a.tools.ffmpeg))!=Some(&a.tools.ffmpeg_sha256)
    ||at(Path::new(&a.tools.ffprobe))!=Some(&a.tools.ffprobe_sha256){return Err(fail("local_import_authority_invalid"));}
  for (p,h) in &a.files{
   if !media::hex(h){return Err(fail("local_import_dependency_pin"));}
   crate::controller_native_launcher::clean_path(Path::new(p))?;
  }
  Ok((root,a))
 }
 fn write(p:&Path,b:&[u8])->store::Result<()>{
  let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(p).map_err(|_|fail("import_control_collision"))?;
  f.write_all(b).and_then(|_|f.sync_all()).map_err(|_|fail("import_control_write"))
 }
 pub(super) fn run(app:&App,root:&Path,a:Authority,job:&str,id:&str,digest:&str,size:u64,dir:&Path,meta:Option<&Sidecar>)->store::Result<()>{
  let manifest=dir.join("manifest.json");
  let mut deps=a.files;
  deps.insert(dir.join("accepted.mp4").to_string_lossy().into_owned(),digest.to_owned());
  let m=serde_json::to_vec(&json!({"schema":"native-runtime-files-v1","files":deps})).map_err(|_|fail("import_manifest_invalid"))?;
  let pin=sha(&m);write(&manifest,&m)?;
  let control=dir.join("config.json");
  let c=serde_json::to_vec(&json!({"schema":"selected-ingest-local-authorized-v2","job_id":job,
   "profile":null,"video_id":null,"source_sha256":digest,"source_bytes":size,"max_bytes":MAX,
   "tools":a.tools,"manifest":manifest,"manifest_sha256":pin})).map_err(|_|fail("import_config_invalid"))?;
  let cfg=sha(&c);write(&control,&c)?;
  let mut child=crate::controller_native_process::Owned::gated_manifest(&a.python,&a.cli,&control,[PYTHON_SHA,&a.cli_sha256,&cfg],dir,&manifest,&pin)?;
  let limit=Instant::now()+Duration::from_secs(190);
  loop{
   match child.poll()?{
    Some(true)=>break,Some(false)=>return Err(fail("import_media_validation_failed")),None=>{}
   }
   if Instant::now()>=limit{let _=child.cancel();return Err(fail("import_timeout_held"));}
   std::thread::sleep(Duration::from_millis(100));
  }
  if child.active()?!=0{return Err(fail("import_descendants_unresolved"));}
  let r=proof(root,job,id,digest,size)?;
  let mut d=app.db.lock().map_err(|_|fail("storage_unavailable"))?;
  finish(&mut d,&r,meta)
 }
 fn proof(root:&Path,job:&str,id:&str,digest:&str,size:u64)->store::Result<Record>{
  let dir=root.join(job);
  let raw=read(&dir.join("receipt.json"),16384)?;
  let value:Value=serde_json::from_slice(&raw).map_err(|_|fail("import_proof_invalid"))?;
  let identity=&value["selection"];
  if value["schema"]!="selected-local-stage-v1"||value["job_id"]!=job||value["publication_enabled"]!=false
    ||value["object_sha256"]!=digest||identity.as_object().is_none_or(|m|m.len()!=2)||identity["kind"]!="local"||identity["source_sha256"]!=digest{
   return Err(fail("import_receipt_binding"));
  }
  let mp=root.join("objects").join(digest).join("manifest.json");
  let mr=read(&mp,16384)?;
  if Some(sha(&mr).as_str())!=value["manifest_sha256"].as_str(){return Err(fail("import_manifest_pin"));}
  let m:Value=serde_json::from_slice(&mr).map_err(|_|fail("import_manifest_invalid"))?;
  let poster_sha=m["poster_sha256"].as_str().ok_or(fail("import_manifest_invalid"))?.to_owned();
  if m["schema"]!="selected-local-object-v1"||m["sha256"]!=digest||m["bytes"].as_u64()!=Some(size){
   return Err(fail("import_manifest_invalid"));
  }
  let rec=Record{id:id.to_owned(),sha:digest.to_owned(),bytes:size,job_id:job.to_owned(),poster_sha};
  let _video=files(root,&rec,false)?;let (_,poster)=files(root,&rec,true)?;
  if poster.metadata().ok().map(|m|m.len())!=m["poster_bytes"].as_u64(){return Err(fail("import_poster_unverified"));}
  Ok(rec)
 }
}


#[cfg(windows)]
fn inbox_path(app:&App)->Option<PathBuf>{
 let runtime=app.media.parent()?;
 if !scope_fixture(app)&&runtime.file_name()?.to_str()?!="snapmatic-review-runtime"{return None;}
 Some(runtime.join("staging").join("inbox"))
}
#[cfg(windows)]
pub(crate) fn supervised_inbox(app:App,port:u16,server_only:bool)->Option<tokio::task::JoinHandle<()>>{
 if std::env::var("LIFEINVADER_LOCAL_IMPORT_INBOX").ok().as_deref()!=Some("1")
   ||(server_only&&!(scope_fixture(&app)&&port==43858&&std::env::args().any(|a|a=="--snapmatic-inbox-fixture")))||(!server_only&&port!=3847){return None;}
 // Outer host owns the lifetime, and submit retains its bounded native Job Object.
 Some(tokio::spawn(async move{
  use std::collections::HashMap;
  let mut stable=HashMap::new();
  let mut tick=tokio::time::interval(std::time::Duration::from_secs(4));
  tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
  loop{
   tick.tick().await;
   let a=app.clone();
   let Ok((next,found))=tokio::task::spawn_blocking(move||inbox_scan(&a,stable)).await else{break;};
   stable=next;
   let Some((id,file,bytes,metadata))=found else{continue;};
   // Never reuse a failed or ambiguous scan in this running process.
   if let Some(value)=stable.get_mut(&id){value.1=true;}
   let mut h=HeaderMap::new();
   let origin=format!("http://127.0.0.1:{port}").parse();
   let (Ok(origin),Ok(length))=(origin,bytes.to_string().parse())else{continue;};
   h.insert(header::ORIGIN,origin);h.insert(header::CONTENT_LENGTH,length);
   h.insert(header::CONTENT_TYPE,axum::http::HeaderValue::from_static("video/mp4"));
   h.insert("x-snapmatic-import",axum::http::HeaderValue::from_static("authorized-local-mp4"));
   if let Some(meta)=metadata{
    let Ok(raw)=serde_json::to_vec(&meta) else{continue;};
    let wire=raw.into_iter().map(|byte|format!("{byte:02x}")).collect::<String>();
    let Ok(valid)=wire.parse() else{continue;};
    h.insert("x-snapmatic-sidecar",valid);
   }
   let body=Body::from_stream(futures_util::stream::try_unfold(tokio::fs::File::from_std(file),|mut f|async {
    use tokio::io::AsyncReadExt;
    let mut buf=vec![0;64*1024];let len=f.read(&mut buf).await?;if len==0{return Ok::<_,std::io::Error>(None);}
    buf.truncate(len);Ok(Some((buf,f)))
   }));
   let _response=submit(State(app.clone()),h,body).await;
  }
 }))
}
#[cfg(windows)]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct InboxReady{schema:String,sha256:String,media_bytes:u64,metadata_sha256:Option<String>}
#[cfg(windows)]
type Observation=(String,bool);
#[cfg(windows)]
type InboxCandidate=(String,File,u64,Option<Sidecar>);
#[cfg(windows)]
fn inbox_scan(app:&App,prior:std::collections::HashMap<String,Observation>)
 ->(std::collections::HashMap<String,Observation>,Option<InboxCandidate>){
 use std::collections::HashMap;
 let mut next=HashMap::new();
 if OWNER.load(Ordering::Acquire){return (prior,None);}
 if native::load(app).is_err(){return (next,None);}
 let Some(inbox)=inbox_path(app)else{return (next,None);};
 if crate::controller_native_launcher::clean_path(&inbox).is_err(){return (next,None);}
 let Ok(list)=std::fs::read_dir(&inbox) else{return (next,None);};
 let mut candidate=None;
 for entry in list.take(128).flatten(){
  let name=entry.file_name();let Some(id)=name.to_str().and_then(|n|n.strip_suffix(".ready.json")).filter(|s|media::hex(s)) else{continue;};
  if PROTECTED.contains(&id){continue;}
  let media=inbox.join(format!("{id}.mp4"));
  let manifest=entry.path();
  let side_path=inbox.join(format!("{id}.info.json"));
  let Some((stamp,ready))=inbox_stamp(&manifest,&media,&side_path,id).ok() else{continue;};
  let recorded=app.db.lock().ok().and_then(|d|existing(&d,id).ok()).flatten();
  if recorded.is_some(){continue;}
  let prev=prior.get(id);
  let settled=prev.is_some_and(|(s,_)|s==&stamp);
  let attempted=prev.is_some_and(|(s,a)|s==&stamp&&*a);
  next.insert(id.to_owned(),(stamp,attempted));
  if !settled||attempted||candidate.is_some(){continue;}
  let Ok(mut f)=inbox_lease(&media,id,ready.media_bytes)else{continue;};
  if f.metadata().ok().is_none_or(|m|m.len()!=ready.media_bytes)||f.seek(SeekFrom::Start(0)).is_err(){continue;}
  let metadata=match ready.metadata_sha256{
   Some(h)=>{
    let Ok(mut f)=inbox_lease(&side_path,&h,262144)else{continue;};
    let mut raw=Vec::new();if f.seek(SeekFrom::Start(0)).and_then(|_|f.take(262145).read_to_end(&mut raw)).is_err()||raw.len()>262144{continue;}
    let Ok(Value::Object(mut map))=serde_json::from_slice::<Value>(&raw)else{continue;};
    map.retain(|k,_|matches!(k.as_str(),"title"|"description"|"uploader"|"upload_date"|"tags"|"id"));
    let Ok(mut facts)=serde_json::from_value::<Sidecar>(Value::Object(map))else{continue;};
    // Ensure the same bound header envelope as picker imports.
    if let Some(s)=facts.description.as_mut(){*s=s.chars().take(200).collect();}
    facts.tags.truncate(10);
    Some(sanitize(facts))
   },None=>None
  };
  candidate=Some((id.to_owned(),f,ready.media_bytes,metadata));
 }
 (next,candidate)
}
#[cfg(windows)]
fn inbox_stamp(manifest:&Path,media:&Path,side:&Path,expected:&str)->store::Result<(String,InboxReady)>{
 use std::os::windows::fs::{MetadataExt,OpenOptionsExt};
 let err=||store::Error("inbox_seal_invalid");
 let fingerprint=|p:&Path|->store::Result<String>{
  crate::controller_native_launcher::clean_path(p)?;
  let m=std::fs::symlink_metadata(p).map_err(|_|err())?;
  if !m.is_file()||m.file_attributes()&0x400!=0{return Err(err());}
  Ok(format!("{}:{}",m.len(),m.last_write_time()))
 };
 let manifest_id=fingerprint(manifest)?;
 let f=std::fs::OpenOptions::new().read(true).share_mode(1).custom_flags(0x00200000).open(manifest).map_err(|_|err())?;
 let mut b=Vec::new();f.take(4097).read_to_end(&mut b).map_err(|_|err())?;if b.len()>4096{return Err(err());}
 let r:InboxReady=serde_json::from_slice(&b).map_err(|_|err())?;
 if r.schema!="snapmatic-import-ready-v1"||r.sha256!=expected||!(12..=MAX).contains(&r.media_bytes){return Err(err());}
 let media_id=fingerprint(media)?;
 if std::fs::metadata(media).map_err(|_|err())?.len()!=r.media_bytes{return Err(err());}
 let side_id=if let Some(h)=&r.metadata_sha256{
  if !media::hex(h)||std::fs::metadata(side).map_err(|_|err())?.len()>262144{return Err(err());}
  fingerprint(side)?
 }else{String::new()};
 Ok((format!("{manifest_id}:{media_id}:{side_id}:{:x}",Sha256::digest(&b)),r))
}
#[cfg(windows)]
fn inbox_lease(p:&Path,expected:&str,cap:u64)->store::Result<File>{
 use std::os::windows::fs::{MetadataExt,OpenOptionsExt};
 let err=||store::Error("inbox_asset_unverified");
 crate::controller_native_launcher::clean_path(p)?;
 let mut f=std::fs::OpenOptions::new().read(true).share_mode(1).custom_flags(0x00200000).open(p).map_err(|_|err())?;
 let m=f.metadata().map_err(|_|err())?;
 if !m.is_file()||m.file_attributes()&0x400!=0||!(1..=cap).contains(&m.len()){return Err(err());}
 let mut take=std::io::Read::by_ref(&mut f).take(cap+1);
 let mut h=Sha256::new();let mut size=0;let mut buf=[0u8;65536];
 loop{let n=take.read(&mut buf).map_err(|_|err())?;if n==0{break;}size+=n as u64;h.update(&buf[..n]);}
 if size!=m.len()||format!("{:x}",h.finalize())!=expected{return Err(err());}
 f.seek(SeekFrom::Start(0)).map_err(|_|err())?;Ok(f)
}