//! Explicit one-post Snapmatic download. No automatic source dispatch.
use crate::{App,controller_store as store,workspace_media as media};
use axum::{extract::{Path,State},http::{HeaderMap,StatusCode},response::{IntoResponse,Response},Json};
use rusqlite::{params,Connection,OptionalExtension};
use serde::Deserialize;
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::{fs::File,io::{Read,Seek,SeekFrom},path::{Path as FsPath,PathBuf}};

#[derive(Clone,Debug,PartialEq,Eq)]
pub(crate) struct ExactSelection{pub profile_id:i64,pub platform:String,pub source_url:String,pub post_id:String,pub excluded:Vec<String>,pub request_id:String}
pub(crate) fn selection(db:&Connection,id:i64)->store::Result<Option<ExactSelection>>{
 let active:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM profiles WHERE id=?1 AND archived=0 AND role='source')",[id],|r|r.get(0))?;
 if !active{return Ok(None);}
 let Ok((platform,source_url,_))=crate::source::source_profile(db,id) else{return Ok(None)};
 let Some((provider,source_url))=crate::workspace_sources_v2::normalize_profile(&source_url) else{return Ok(None)};
 if provider!=crate::workspace_sources_v2::Provider::TikTok||provider.atom()!=platform{return Ok(None);}
 let saved:Option<(String,String)>=db.query_row("SELECT allowed,excluded FROM source_selection WHERE profile_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
 let Some((allowed,excluded))=saved else{return Ok(None)};
 let ids:Vec<String>=serde_json::from_str(&allowed).map_err(|_|store::Error("saved_selection_invalid"))?;
 let [post_id]=ids.as_slice() else{return Ok(None)}; let excluded:Vec<String>=serde_json::from_str(&excluded).map_err(|_|store::Error("saved_selection_invalid"))?;if excluded.iter().any(|id|id==post_id){return Ok(None);}
 if !crate::workspace_sources_v2::post_id(provider,post_id){return Ok(None);}
 let mut sha=Sha256::new();
 for v in ["snapmatic-source-selection-v1",&id.to_string(),&platform,&source_url,post_id]{sha.update(v.as_bytes());sha.update([0]);}
 Ok(Some(ExactSelection{profile_id:id,platform,source_url,post_id:post_id.clone(),excluded,request_id:format!("{:x}",sha.finalize())}))
}

#[cfg(windows)]
fn atomic_control(p:&FsPath,bytes:&[u8])->store::Result<()>{
 use std::io::Write;
 use std::os::windows::ffi::OsStrExt;
 #[link(name="kernel32")]
 unsafe extern "system"{fn MoveFileExW(existing:*const u16,dest:*const u16,flags:u32)->i32;}
 let parent=p.parent().ok_or(store::Error("download_control_scope"))?;
 crate::controller_native_launcher::clean_path(parent)?;
 if p.try_exists().map_err(|_|store::Error("download_control_scope"))? {return Err(store::Error("download_control_collision"));}
 let tmp=parent.join(format!(".download-{}.pending",uuid::Uuid::new_v4()));
 let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp).map_err(|_|store::Error("download_control_collision"))?;
 f.write_all(bytes).and_then(|_|f.sync_all()).map_err(|_|store::Error("download_control_write"))?;
 drop(f);
 let wide=|s:&FsPath|s.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
 let (from,to)=(wide(&tmp),wide(p));
 // WRITE_THROUGH, deliberately without REPLACE_EXISTING or cross-volume COPY_ALLOWED.
 if unsafe{MoveFileExW(from.as_ptr(),to.as_ptr(),8)}==0{return Err(store::Error("download_control_commit_refused"));}
 Ok(())
}

#[derive(Clone)]
pub(crate) struct Verified{pub sha:String,pub bytes:u64,pub poster_sha:String}
impl Verified{fn valid(&self)->bool{media::hex(&self.sha)&&media::hex(&self.poster_sha)&&(1..=180*1024*1024).contains(&self.bytes)}}
fn schema(db:&Connection)->store::Result<()>{
 db.execute_batch("CREATE TABLE IF NOT EXISTS snapmatic_source_downloads_v1(
    request_id TEXT PRIMARY KEY, profile_id INTEGER NOT NULL, stage TEXT NOT NULL,
    media_sha TEXT, poster_sha TEXT, media_bytes INTEGER, failure_code TEXT,
    updated_at INTEGER NOT NULL);
 CREATE UNIQUE INDEX IF NOT EXISTS snapmatic_source_download_sha_v1
 ON snapmatic_source_downloads_v1(media_sha) WHERE stage='pending_review';")?;Ok(())
}
fn exists(db:&Connection)->store::Result<bool>{
 Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='snapmatic_source_downloads_v1')",[],|r|r.get(0))?)
}
pub(crate) fn claim(db:&mut Connection,s:&ExactSelection)->store::Result<bool>{
 schema(db)?;
 let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 if selection(&tx,s.profile_id)?.as_ref()!=Some(s){return Err(store::Error("selection_changed"));}
 let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM snapmatic_source_downloads_v1 WHERE stage IN ('running','verifying'))",[],|r|r.get(0))?;
 if active{return Ok(false);}
 let n=tx.execute("INSERT OR IGNORE INTO snapmatic_source_downloads_v1(request_id,profile_id,stage,updated_at) VALUES(?1,?2,'running',?3)",
   params![s.request_id,s.profile_id,chrono::Utc::now().timestamp()])?;
 tx.commit()?;Ok(n==1)
}
pub(crate) fn hold(db:&Connection,id:&str,code:&'static str)->store::Result<()>{
 db.execute("UPDATE snapmatic_source_downloads_v1 SET stage='held',failure_code=?2,updated_at=?3 WHERE request_id=?1 AND stage IN ('running','verifying')",params![id,code,chrono::Utc::now().timestamp()])?;Ok(())
}
pub(crate) fn complete(db:&mut Connection,s:&ExactSelection,v:&Verified,known_duplicate:bool)->store::Result<&'static str>{
 if !v.valid(){return Err(store::Error("output_not_verified"));}
 let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
 if selection(&tx,s.profile_id)?.as_ref()!=Some(s){return Err(store::Error("selection_changed"));}
 let found:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM snapmatic_source_downloads_v1 WHERE stage='pending_review' AND media_sha=?1)",[&v.sha],|r|r.get(0))?;
 let stage=if known_duplicate||found{"duplicate"}else{"pending_review"};
 if tx.execute("UPDATE snapmatic_source_downloads_v1 SET stage=?2,media_sha=?3,poster_sha=?4,media_bytes=?5,updated_at=?6 WHERE request_id=?1 AND stage='verifying'",
 params![s.request_id,stage,v.sha,v.poster_sha,v.bytes as i64,chrono::Utc::now().timestamp()])?!=1{return Err(store::Error("download_state_changed"));}
 tx.commit()?;Ok(stage)
}
#[cfg(windows)]
pub(crate) fn root(app:&App)->store::Result<PathBuf>{
 let p=PathBuf::from(std::env::var_os("LIFEINVADER_SOURCE_DOWNLOAD_SCOPE").ok_or(store::Error("download_adapter_unconfigured"))?);
 let active=app.media.parent().ok_or(store::Error("download_stage_path"))?;
 let base=active.parent().ok_or(store::Error("download_stage_path"))?;
 if p.parent()!=Some(base)||!matches!(p.file_name().and_then(|x|x.to_str()),Some("snapmatic-selected-local-cache-v1"|"snapmatic-selected-fixtures-v1"))
  ||!matches!(p.components().next(),Some(std::path::Component::Prefix(x)) if matches!(x.kind(),std::path::Prefix::Disk(b'C'|b'c'))) || !p.is_dir(){
   return Err(store::Error("download_stage_path"));
 }
 crate::controller_native_launcher::clean_path(&p)?;
 Ok(p)
}
#[cfg(windows)]
pub(crate) fn asset(root:&FsPath,id:&str,v:&Verified,poster:bool)->store::Result<(PathBuf,File)>{
 if !media::hex(id)||!v.valid(){return Err(store::Error("download_asset_invalid"));}
 let path=root.join("objects").join(&v.sha).join(if poster{"poster.webp"}else{"media.mp4"});
 crate::controller_native_launcher::clean_path(&path)?;
 let mut f=crate::controller_native_process::lease(&path,if poster{&v.poster_sha}else{&v.sha})?;
 let size=f.metadata().map_err(|_|store::Error("download_asset_invalid"))?.len();
 if poster{
  if !(30..=2*1024*1024).contains(&size){return Err(store::Error("download_poster_invalid"));}
  let mut h=[0;30];f.seek(SeekFrom::Start(0)).and_then(|_|f.read_exact(&mut h)).map_err(|_|store::Error("download_poster_invalid"))?;
  if crate::workspace_phase4::webp_dimensions(&h)!=Some((540,960)){return Err(store::Error("download_poster_invalid"));}
 }else if size!=v.bytes{return Err(store::Error("download_asset_invalid"));}
 Ok((path,f))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt{schema:String,job_id:String,selection:Evidence,object_sha256:String,manifest_sha256:String,publication_enabled:bool}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence{profile_id:i64,platform:String,account:String,video_id:String}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest{schema:String,sha256:String,bytes:u64,poster_sha256:String,poster_bytes:u64}
#[cfg(windows)]
fn bounded_receipt(path:&FsPath)->store::Result<Vec<u8>>{
 use std::fs::OpenOptions;
 use std::os::windows::fs::{OpenOptionsExt,MetadataExt};
 let cap=if path.file_name().and_then(|v|v.to_str()).is_some_and(|n|matches!(n,"authority.json"|"direct-authority.json")){65536}else{16384};
 crate::controller_native_launcher::clean_path(path)?;
 let mut f=OpenOptions::new().read(true).share_mode(1).custom_flags(0x00200000).open(path).map_err(|_|store::Error("download_receipt_missing"))?;
 let m=f.metadata().map_err(|_|store::Error("download_receipt_invalid"))?;
 if !m.is_file()||m.file_attributes()&0x400!=0||!(1..=cap).contains(&m.len()){return Err(store::Error("download_receipt_invalid"));}
 let mut b=Vec::new();f.by_ref().take(cap+1).read_to_end(&mut b).map_err(|_|store::Error("download_receipt_invalid"))?;
 if b.len() as u64!=m.len(){return Err(store::Error("download_receipt_invalid"));}Ok(b)
}
#[cfg(windows)]
fn accept(root:&FsPath,s:&ExactSelection,worker_id:&str,account:&str,platform:&str)->store::Result<Verified>{
 if uuid::Uuid::parse_str(worker_id).ok().is_none_or(|id|id.to_string()!=worker_id){return Err(store::Error("download_job_binding"));}
 let r:Receipt=serde_json::from_slice(&bounded_receipt(&root.join(worker_id).join("receipt.json"))?).map_err(|_|store::Error("download_receipt_invalid"))?;
 if r.schema!="selected-local-stage-v1"||r.job_id!=worker_id||r.selection.profile_id!=s.profile_id||
 r.selection.platform!=platform||r.selection.account!=account||r.selection.video_id!=s.post_id||r.publication_enabled||
 !media::hex(&r.object_sha256)||!media::hex(&r.manifest_sha256){return Err(store::Error("download_receipt_binding"));}
 let raw=bounded_receipt(&root.join("objects").join(&r.object_sha256).join("manifest.json"))?;
 if format!("{:x}",Sha256::digest(&raw))!=r.manifest_sha256{return Err(store::Error("download_manifest_pin"));}
 let m:Manifest=serde_json::from_slice(&raw).map_err(|_|store::Error("download_manifest_invalid"))?;
 if m.schema!="selected-local-object-v1"||m.sha256!=r.object_sha256||!(30..=2*1024*1024).contains(&m.poster_bytes){return Err(store::Error("download_manifest_invalid"));}
 let v=Verified{sha:m.sha256,bytes:m.bytes,poster_sha:m.poster_sha256};
 let _video=asset(root,&s.request_id,&v,false)?;
 let (_,p)=asset(root,&s.request_id,&v,true)?;
 if p.metadata().map_err(|_|store::Error("download_poster_invalid"))?.len()!=m.poster_bytes{return Err(store::Error("download_poster_invalid"));}
 Ok(v)
}
pub(crate) fn rows(app:&App)->store::Result<Vec<Value>>{
 let d=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
 if !exists(&d)?{return Ok(vec![]);}
 let mut q=d.prepare("SELECT request_id,media_sha,media_bytes,profile_id FROM snapmatic_source_downloads_v1 WHERE stage='pending_review' ORDER BY updated_at DESC,request_id LIMIT 500")?;
 let list=q.query_map([],|r|Ok(json!({"clip_id":r.get::<_,String>(0)?,"kind":"original","state":"pending_review","name":format!("Downloaded selection - source {}",r.get::<_,i64>(3)?),
    "preview_available":true,"poster_available":true,"preview_sha256":r.get::<_,String>(1)?,"preview_bytes":r.get::<_,i64>(2)?,"preview_only":true})))?;
 Ok(list.collect::<Result<Vec<_>,_>>()?)
}
pub(crate) fn open(app:&App,id:&str,poster:bool)->store::Result<(PathBuf,File)>{
 #[cfg(windows)] {
  if !media::hex(id){return Err(store::Error("preview_unavailable"));}
  let d=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
  if !exists(&d)?{return Err(store::Error("preview_unavailable"));}
  let v=d.query_row("SELECT media_sha,media_bytes,poster_sha FROM snapmatic_source_downloads_v1 WHERE request_id=?1 AND stage='pending_review'",[id],
   |r|Ok(Verified{sha:r.get(0)?,bytes:u64::try_from(r.get::<_,i64>(1)?).unwrap_or(0),poster_sha:r.get(2)?})).optional()?.ok_or(store::Error("preview_unavailable"))?;
  drop(d);return direct::root(app).and_then(|root|asset(&root,id,&v,poster)).or_else(|_|root(app).and_then(|root|asset(&root,id,&v,poster)));
 }
 #[allow(unreachable_code)] {let _=(app,id,poster);Err(store::Error("preview_unavailable"))}
}
fn lookup(app:&App,id:i64)->store::Result<Value>{
 let d=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
 let Some(s)=selection(&d,id)? else{return Ok(json!({"state":"choose_selection","publishing_enabled":false}));};
 let entry=if exists(&d)?{
  d.query_row("SELECT stage,failure_code FROM snapmatic_source_downloads_v1 WHERE request_id=?1",[&s.request_id],
    |r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?))).optional()?
 }else{None};
 let (mut state,mut reason)=entry.unwrap_or(("approval_required".into(),Some("selected_post_permission_or_transfer_not_verified".into())));
 #[cfg(windows)] {
  if state=="approval_required"&&(direct::ready(app,&s)||run::ready(app,&s)){state="ready".into();reason=None;}
  if matches!(state.as_str(),"running"|"verifying")&&!run::running(){state="held".into();reason=Some("interrupted_transfer_review_required".into());}
 }
 let lifecycle=match state.as_str(){
  "ready"=>"queued_for_operator", "running"=>"running", "verifying"=>"verifying",
  "pending_review"|"duplicate"=>"completed", "held"=>"failed_or_interrupted", _=>"held_before_dispatch",
 };
 Ok(json!({"state":state,"lifecycle":lifecycle,"reason":reason,"selection_key":s.request_id,"selected_post_id":s.post_id,
   "requires_explicit_action":true,"publishing_enabled":false}))
}
pub(crate) async fn status(State(app):State<App>,Path(id):Path<i64>)->Response{
 match lookup(&app,id){Ok(v)=>Json(v).into_response(),Err(e)=>(StatusCode::SERVICE_UNAVAILABLE,Json(json!({"reason":e.0}))).into_response()}
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Action{action:String,selection_key:String}
pub(crate) async fn start(State(app):State<App>,Path(id):Path<i64>,h:HeaderMap,Json(input):Json<Action>)->Response{
 #[cfg(windows)]
 if run::origin(&app,&h)&&input.action=="download_selected_to_pending_review"&&media::hex(&input.selection_key){
  let res=tokio::task::spawn_blocking(move||{
     let current=lookup(&app,id)?;
     if current["selection_key"].as_str()!=Some(&input.selection_key){return Err(store::Error("selected_post_changed_refresh_first"));}
     if matches!(current["state"].as_str(),Some("running"|"verifying"|"pending_review"|"duplicate"|"held")){return Ok(current);}
     if direct::configured(){direct::begin(&app,id,&input.selection_key)?;}else{run::begin(&app,id,&input.selection_key)?;}
     lookup(&app,id)
  }).await;
  return match res{
    Ok(Ok(v))=>Json(v).into_response(),
    Ok(Err(e))=>(StatusCode::CONFLICT,Json(json!({"reason":e.0,"publishing_enabled":false}))).into_response(),
    Err(_)=>(StatusCode::SERVICE_UNAVAILABLE,Json(json!({"reason":"download_unavailable","publishing_enabled":false}))).into_response()
  };
 }
 let _=(app,id,h,input);
 (StatusCode::FORBIDDEN,Json(json!({"reason":"explicit_local_download_required","publishing_enabled":false}))).into_response()
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test] fn exact_selection_reuse_and_restart_are_non_dispatching(){
  let path=std::env::temp_dir().join(format!("snapmatic-source-claim-{}.sqlite3",uuid::Uuid::new_v4()));
  let mut db=Connection::open(&path).unwrap();
  crate::init(&db).unwrap();
  db.execute("INSERT INTO profiles(id,name,platform,url,role,source_folder) VALUES(3,'source','tiktok','https://www.tiktok.com/@fixture','source','')",[]).unwrap();
  db.execute("INSERT INTO source_selection VALUES(3,1,?1,'[]','all')",[r#"["123456789012345678"]"#]).unwrap();
  let s=selection(&db,3).unwrap().unwrap();
  db.execute("UPDATE profiles SET url='https://tiktok.com/@FIXTURE/' WHERE id=3",[]).unwrap();
  assert_eq!(selection(&db,3).unwrap().unwrap(),s);
  assert!(claim(&mut db,&s).unwrap());assert!(!claim(&mut db,&s).unwrap());
  db.execute("UPDATE snapmatic_source_downloads_v1 SET stage='verifying' WHERE request_id=?1",[&s.request_id]).unwrap();
  let v=Verified{sha:"a".repeat(64),bytes:3600,poster_sha:"b".repeat(64)};
  assert_eq!(complete(&mut db,&s,&v,false).unwrap(),"pending_review");
  drop(db);
  let mut db=Connection::open(&path).unwrap();
  assert_eq!(selection(&db,3).unwrap().unwrap(),s);assert!(!claim(&mut db,&s).unwrap());
  db.execute("UPDATE source_selection SET allowed=?1 WHERE profile_id=3",[r#"["123456789012345679"]"#]).unwrap();
  let s2=selection(&db,3).unwrap().unwrap();assert_ne!(s2,s);
  assert!(claim(&mut db,&s2).unwrap());
  db.execute("UPDATE snapmatic_source_downloads_v1 SET stage='verifying' WHERE request_id=?1",[&s2.request_id]).unwrap();
  assert_eq!(complete(&mut db,&s2,&v,false).unwrap(),"duplicate");
 }
}

#[cfg(windows)]
mod run {
 use super::*;
 use std::{io::Write,sync::atomic::{AtomicBool,Ordering},time::{Duration,Instant}};
 pub(super) static RUNNING:AtomicBool=AtomicBool::new(false);
 const CLI_SHA:&str="0a6cf8b1c556ac08a24951b92c5f5c8b5a757cc201c88d0fddae539de8cb4ae4";
 #[derive(Deserialize)]
 #[serde(deny_unknown_fields)]
 struct Authority{schema:String,mode:String,profile_id:i64,account:String,video_id:String,
  cli:PathBuf,python:PathBuf,python_sha256:String,files:std::collections::BTreeMap<String,String>,
  tools:Value,fixture:Option<Fixture>,expires_at_unix:i64}
 #[derive(Deserialize)]
 #[serde(deny_unknown_fields)]
 struct Fixture{path:PathBuf,sha256:String}
 fn error(s:&'static str)->store::Error{store::Error(s)}
 fn write(p:&FsPath,b:&[u8])->store::Result<()>{atomic_control(p,b)}
 pub(super) fn running()->bool{RUNNING.load(Ordering::Acquire)}
 pub(super) fn fixture_app(app:&App)->bool{
  app.media.parent().and_then(|p|p.file_name()).and_then(|p|p.to_str()).is_some_and(|p|p.starts_with("snapmatic-source-download-qa-"))
 }
 pub(super) fn origin(app:&App,h:&HeaderMap)->bool{
  let o=h.get("origin").and_then(|v|v.to_str().ok());
  o==Some("http://127.0.0.1:3847")||(fixture_app(app)&&matches!(o,Some("http://127.0.0.1:43851"|"http://127.0.0.1:43853"|"http://127.0.0.1:43856")))
 }
 fn load(app:&App,s:&ExactSelection)->store::Result<(PathBuf,Authority)>{
  let p=root(app)?;
  let pin=std::env::var("LIFEINVADER_SOURCE_DOWNLOAD_AUTHORITY_SHA256").map_err(|_|error("download_approval_required"))?;
  if !media::hex(&pin){return Err(error("download_approval_required"));}
  let raw=bounded_receipt(&p.join("authority.json"))?;
  if format!("{:x}",Sha256::digest(&raw))!=pin{return Err(error("download_authority_changed"));}
  let a:Authority=serde_json::from_slice(&raw).map_err(|_|error("download_authority_invalid"))?;
  let ap=s.source_url.parse::<axum::http::Uri>().map_err(|_|error("download_selection_invalid"))?;
  let valid_account=a.account.len()<=24&&!a.account.is_empty()&&a.account.bytes().all(|c|c.is_ascii_alphanumeric()||b"._".contains(&c));
  // V1 owns only isolated Unix-free fixture qualification. A Python shim or an
  // arbitrary normal-mode descriptor is not qualified live transport.
  let right_mode=a.mode=="fixture"&&a.fixture.is_some()&&fixture_app(app)&&p.file_name().and_then(|x|x.to_str())==Some("snapmatic-selected-fixtures-v1");
  if a.schema!="snapmatic-selected-download-authority-v1"||!right_mode||s.platform!="tiktok"||a.profile_id!=s.profile_id||a.video_id!=s.post_id
    ||ap.scheme_str()!=Some("https")||ap.query().is_some()||ap.path().trim_end_matches('/')!=format!("/@{}",a.account)
    ||!valid_account||!(15..=22).contains(&a.video_id.len())||!a.video_id.bytes().all(|x|x.is_ascii_digit())
    ||a.expires_at_unix<=chrono::Utc::now().timestamp()||a.files.len()>128
    ||a.cli.file_name().and_then(|x|x.to_str())!=Some("selected_staging_cli.py")
    ||a.files.get(&a.cli.to_string_lossy().to_string()).map(String::as_str)!=Some(CLI_SHA)
    ||a.files.get(&a.python.to_string_lossy().to_string())!=Some(&a.python_sha256){return Err(error("download_authority_invalid"));}
  // Native lease additionally binds each dependency by SHA before sending GO.
  for (path,sha) in &a.files{
   if !media::hex(sha){return Err(error("download_dependency_pin"));}
   crate::controller_native_launcher::clean_path(FsPath::new(path))?;
  }
  Ok((p,a))
 }
 pub(super) fn ready(app:&App,s:&ExactSelection)->bool{load(app,s).is_ok()}
 fn actual(app:&App,s:&ExactSelection,p:&FsPath,a:Authority)->store::Result<()>{
  let id=uuid::Uuid::new_v4().to_string();
  let dir=p.join(format!("control-{id}"));
  std::fs::create_dir(&dir).map_err(|_|error("download_control_collision"))?;
  crate::controller_native_launcher::clean_path(&dir)?;
  let mut files=a.files;
  let fixture=if let Some(f)=a.fixture{
   crate::controller_native_launcher::clean_path(&f.path)?;
   let mut lease=crate::controller_native_process::lease(&f.path,&f.sha256)?;
   lease.seek(SeekFrom::Start(0)).map_err(|_|error("download_fixture_pin"))?;
   if lease.metadata().map_err(|_|error("download_fixture_pin"))?.len()>16*1024*1024{return Err(error("download_fixture_size"));}
   let mut b=Vec::new();lease.take(16*1024*1024+1).read_to_end(&mut b).map_err(|_|error("download_fixture_pin"))?;
   let dest=dir.join("fixture.mp4");write(&dest,&b)?;files.insert(dest.to_string_lossy().into_owned(),f.sha256.clone());
   json!({"path":dest,"sha256":f.sha256})
  }else{Value::Null};
  let m=dir.join("manifest.json");
  let bytes=serde_json::to_vec(&json!({"schema":"native-runtime-files-v1","files":files})).map_err(|_|error("download_manifest_write"))?;
  let m_sha=format!("{:x}",Sha256::digest(&bytes));write(&m,&bytes)?;
  let cfg=dir.join("config.json");
  let raw=serde_json::to_vec(&json!({"schema":"selected-stage-launch-v1","mode":a.mode,"job_id":id,
    "profile_id":s.profile_id,"account":a.account,"video_id":s.post_id,"allowed":[s.post_id],"excluded":s.excluded,
    "tools":a.tools,"manifest":m,"manifest_sha256":m_sha,"fixture":fixture})).map_err(|_|error("download_control_write"))?;
  let cfg_sha=format!("{:x}",Sha256::digest(&raw));write(&cfg,&raw)?;
  let mut child=crate::controller_native_process::Owned::gated_manifest(&a.python,&a.cli,&cfg,[&a.python_sha256,CLI_SHA,&cfg_sha],&dir,&m,&m_sha)?;
  let until=Instant::now()+Duration::from_secs(370);
  loop{
   match child.poll()?{
    Some(true)=>break,
    Some(false)=>{
      let f=bounded_receipt(&p.join(&id).join("failure.json")).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok());
      return Err(error(match f.as_ref().and_then(|r|r["reason"].as_str()){Some("downloader_failed")=>"download_provider_or_transport_held",Some("validation_failed")=>"download_media_validation_held",_=>"download_worker_held"}));
    }
    None=>{}
   }
   if Instant::now()>=until{let _=child.cancel();return Err(error("download_timeout_held"));}
   std::thread::sleep(Duration::from_millis(100));
  }
  if child.active()?!=0{return Err(error("download_descendants_unresolved"));}
  {
   let db=app.db.lock().map_err(|_|error("storage_unavailable"))?;
   db.execute("UPDATE snapmatic_source_downloads_v1 SET stage='verifying' WHERE request_id=?1 AND stage='running'",[&s.request_id])?;
  }
  let v=accept(p,s,&id,&a.account,if a.mode=="fixture"{"fixture"}else{"tiktok"})?;
  let static_rows=crate::workspace_media::index(app);
  if a.mode!="fixture"&&static_rows.is_err(){return Err(error("download_existing_index_unverified"));}
  let duplicate=static_rows.unwrap_or_default().iter().any(|x|x["preview_sha256"].as_str()==Some(&v.sha))
   ||crate::workspace_phase4::rows(app)?.iter().any(|x|x["preview_sha256"].as_str()==Some(&v.sha));
  let mut db=app.db.lock().map_err(|_|error("storage_unavailable"))?;
  let _=complete(&mut db,s,&v,duplicate)?;
  Ok(())
 }
 pub(super) fn begin(app:&App,id:i64,expected:&str)->store::Result<()>{
  let s={let db=app.db.lock().map_err(|_|error("storage_unavailable"))?;
   selection(&db,id)?.ok_or(error("choose_exactly_one_post"))?};
  if s.request_id!=expected{return Err(error("selected_post_changed_refresh_first"));}
  let (p,a)=load(app,&s)?;
  if RUNNING.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return Ok(());}
  let reserve=(||{
   let mut db=app.db.lock().map_err(|_|error("storage_unavailable"))?;
   claim(&mut db,&s)
  })();
  match reserve{
   Ok(true)=>{
    let app=app.clone();let key=s.request_id.clone();
    let fallback=app.clone();
    let thread=std::thread::Builder::new().name("snapmatic-selected-download".into()).spawn(move||{
      let result=actual(&app,&s,&p,a);
      if let Err(e)=result{if let Ok(db)=app.db.lock(){let _=hold(&db,&s.request_id,e.0);}}
      RUNNING.store(false,Ordering::Release);
    });
    if thread.is_err(){RUNNING.store(false,Ordering::Release);if let Ok(db)=fallback.db.lock(){let _=hold(&db,&key,"download_launcher_unavailable");}return Err(error("download_launcher_unavailable"));}
    Ok(())
   },
   Ok(false)=>{RUNNING.store(false,Ordering::Release);Ok(())},
   Err(e)=>{RUNNING.store(false,Ordering::Release);Err(e)}
  }
 }
}


#[cfg(windows)]
mod direct {
 use super::*;
 use std::{collections::BTreeMap,io::Write,sync::atomic::Ordering,time::{Duration,Instant}};
 const FILE_FIXTURE_CLI_SHA:&str="5c1c919e11e078a3123ed6d932724948943c9e0a3d6acdfb435d31c5d4b5d11d";
 const TLS_FIXTURE_CLI_SHA:&str="a25ade5416b177070a33ab43a5b64244d7b055083fe7536438d0f4e666664e65";
 const PYTHON_SHA:&str="4d6f5f81a4bca11191c4c7c6b43632694d0a4ce74e068619d8fdc161d469859a";
 #[derive(Deserialize)]
 #[serde(deny_unknown_fields)]
 struct Authority{
  schema:String,mode:String,profile_id:i64,profile:String,video_id:String,url:String,
  source_sha256:String,bytes:u64,expires_at:i64,cli:PathBuf,python:PathBuf,
  files:BTreeMap<String,String>,tools:Value,fixture:Option<PathBuf>,fixture_tls:Option<TlsFixture>
 }
 #[derive(Deserialize)]
 #[serde(deny_unknown_fields)]
 struct TlsFixture{port:u16,ca_path:PathBuf,ca_sha256:String}
 #[derive(Deserialize)]
 #[serde(deny_unknown_fields)]
 struct DirectIdentity{provider:String,source:String,id:String,policy:String,permission_sha256:String}
 #[derive(Deserialize)]
 #[serde(deny_unknown_fields)]
 struct DirectReceipt{schema:String,job_id:String,selection:DirectIdentity,object_sha256:String,manifest_sha256:String,publication_enabled:bool}
 #[derive(Deserialize)]
 #[serde(deny_unknown_fields)]
 struct ArchiveIdentity{provider:String,source:String,id:String}
 #[derive(Deserialize)]
 #[serde(deny_unknown_fields)]
 struct Archive{schema:String,identity:ArchiveIdentity,object_sha256:String,manifest_sha256:String,stage_receipt_sha256:String,publication_enabled:bool}
 fn e(x:&'static str)->store::Error{store::Error(x)}
 fn sha(x:&[u8])->String{format!("{:x}",Sha256::digest(x))}
 fn tls_fixture(app:&App)->bool{
  super::run::fixture_app(app)&&std::env::args().any(|a|a=="--server-only")&&std::env::var("LIFEINVADER_PORT").ok().as_deref()==Some("43856")
  &&std::env::var("LIFEINVADER_CREATOR_DIRECT_TLS_FIXTURE").ok().as_deref()==Some("1")
 }
 pub(super) fn root(app:&App)->store::Result<PathBuf>{
  let root=PathBuf::from(std::env::var_os("LIFEINVADER_CREATOR_DIRECT_SCOPE").ok_or(e("creator_download_authority_missing"))?);
  let base=app.media.parent().and_then(FsPath::parent).ok_or(e("creator_download_scope_invalid"))?;
  let leaf=if tls_fixture(app){"snapmatic-direct-tls-fixtures-v1"}else if super::run::fixture_app(app){"snapmatic-direct-fixtures-v1"}else{"snapmatic-direct-local-v1"};
  if root!=base.join(leaf)||!root.is_dir()||!matches!(root.components().next(),Some(std::path::Component::Prefix(p)) if matches!(p.kind(),std::path::Prefix::Disk(b'C'|b'c'))){
    return Err(e("creator_download_scope_invalid"));
  }
  crate::controller_native_launcher::clean_path(&root)?;Ok(root)
 }
 pub(super) fn configured()->bool{std::env::var_os("LIFEINVADER_CREATOR_DIRECT_SCOPE").is_some()}
 fn safe_url(s:&str,fixture:bool)->bool{
  if s.len()>2048||!s.is_ascii()||!s.starts_with("https://")||s.bytes().any(|b|b<=32||b"%\\?#".contains(&b)){return false;}
  let Ok(u)=s.parse::<axum::http::Uri>() else{return false};
  let Some(host)=u.host()else{return false};
  let Some(auth)=u.authority()else{return false};
  if auth.as_str()!=host||u.port_u16().is_some()||host=="fixture.invalid"&&!fixture||host!="fixture.invalid"&&fixture{return false;}
  let labels=host.split('.').collect::<Vec<_>>();
  if labels.len()<2||labels.last().is_none_or(|l|l.len()<2||!l.bytes().all(|b|b.is_ascii_lowercase()))||
    labels.iter().any(|l|l.is_empty()||l.starts_with('-')||l.ends_with('-')||!l.bytes().all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()||b==b'-')){return false;}
  let p=u.path();
  p.ends_with(".mp4")&&p.bytes().all(|b|b.is_ascii_alphanumeric()||b"._~/-".contains(&b))&&!p.split('/').any(|x|x=="."||x=="..")
 }
 fn load(app:&App,s:&ExactSelection)->store::Result<(PathBuf,Authority,String)>{
  let dir=root(app)?;
  let pin=std::env::var("LIFEINVADER_CREATOR_DIRECT_AUTHORITY_SHA256").map_err(|_|e("creator_download_authority_missing"))?;
  let b=bounded_receipt(&dir.join("direct-authority.json"))?;
  if !media::hex(&pin)||sha(&b)!=pin{return Err(e("creator_download_authority_changed"));}
  let a:Authority=serde_json::from_slice(&b).map_err(|_|e("creator_download_authority_invalid"))?;
  let fixture=super::run::fixture_app(app);
  let elapsed=a.expires_at.checked_sub(chrono::Utc::now().timestamp()).unwrap_or(0);
  let at=|p:&FsPath|a.files.get(&p.to_string_lossy().into_owned()).map(String::as_str);
  let tls=tls_fixture(app);
  let cli_pin=if tls{TLS_FIXTURE_CLI_SHA.to_owned()}else if fixture{FILE_FIXTURE_CLI_SHA.to_owned()}else{
   let reviewed=std::env::var("LIFEINVADER_CREATOR_DIRECT_REVIEWED_CLI_SHA256").map_err(|_|e("creator_download_reviewed_worker_unconfigured"))?;
   if !media::hex(&reviewed){return Err(e("creator_download_reviewed_worker_unconfigured"));}
   reviewed
  };
  if a.schema!="snapmatic-creator-direct-authority-v1"||a.mode!=(if tls{"tls_fixture"}else if fixture{"fixture"}else{"creator_direct"})
    ||a.fixture.is_some()!=(fixture&&!tls)||a.fixture_tls.is_some()!=tls||a.profile_id!=s.profile_id||a.profile!=s.source_url||a.video_id!=s.post_id
    ||!media::hex(&a.source_sha256)||!(12..=180*1024*1024).contains(&a.bytes)||!(1..=900).contains(&elapsed)||!safe_url(&a.url,fixture)
    ||a.files.len()>128||a.cli.file_name().and_then(|v|v.to_str())!=Some("selected_direct_v2.py")
    ||at(&a.cli)!=Some(cli_pin.as_str())||at(&a.python)!=Some(PYTHON_SHA){
   return Err(e("creator_download_permission_unverified"));
  }
  if let Some(t)=&a.fixture_tls{
   let fixture_dir=app.media.parent().ok_or(e("creator_download_fixture_invalid"))?;
   if t.port!=43857||t.ca_path!=fixture_dir.join("fixture-ca.pem")||!media::hex(&t.ca_sha256)
     ||a.url!="https://fixture.invalid/clip.mp4" {return Err(e("creator_download_fixture_invalid"));}
  }
  let parent=a.cli.parent();
  if parent.and_then(FsPath::parent).and_then(FsPath::parent).and_then(FsPath::parent)!=dir.parent(){
   return Err(e("creator_download_scope_invalid"));
  }
  for (p,h) in &a.files{
   if !media::hex(h){return Err(e("creator_download_dependency_pin"));}
   crate::controller_native_launcher::clean_path(FsPath::new(p))?;
  }
  let _cli=crate::controller_native_process::lease(&a.cli,&cli_pin)?;
  Ok((dir,a,cli_pin))
 }
 pub(super) fn ready(app:&App,s:&ExactSelection)->bool{load(app,s).is_ok()}
 fn write(p:&FsPath,b:&[u8])->store::Result<()>{atomic_control(p,b)}
 fn accept_direct(root:&FsPath,id:&str,s:&ExactSelection,a:&Authority,permission:&str)->store::Result<Verified>{
  let path=root.join(id);
  let raw=bounded_receipt(&path.join("receipt.json"))?;
  let r:DirectReceipt=serde_json::from_slice(&raw).map_err(|_|e("creator_download_receipt_invalid"))?;
  let archive:Archive=serde_json::from_slice(&bounded_receipt(&path.join("archive.json"))?).map_err(|_|e("creator_download_archive_invalid"))?;
  if r.schema!="selected-local-stage-v1"||r.job_id!=id||r.selection.provider!=s.platform||
     r.selection.source!=s.source_url||r.selection.id!=s.post_id||r.selection.policy!=(if a.fixture_tls.is_some(){"snapmatic_owned_test"}else{"creator_supplied_direct_mp4"})||
     r.selection.permission_sha256!=permission||r.publication_enabled||r.object_sha256!=a.source_sha256||
     !media::hex(&r.manifest_sha256)||archive.schema!="selected-source-archive-v2"||archive.publication_enabled||
     archive.identity.provider!=s.platform||archive.identity.source!=s.source_url||archive.identity.id!=s.post_id||
     archive.stage_receipt_sha256!=sha(&raw)||archive.object_sha256!=r.object_sha256||archive.manifest_sha256!=r.manifest_sha256{
   return Err(e("creator_download_receipt_binding"));
  }
  let raw=bounded_receipt(&root.join("objects").join(&r.object_sha256).join("manifest.json"))?;
  if sha(&raw)!=r.manifest_sha256{return Err(e("creator_download_manifest_pin"));}
  let m:Manifest=serde_json::from_slice(&raw).map_err(|_|e("creator_download_manifest_invalid"))?;
  if m.schema!="selected-local-object-v1"||m.sha256!=a.source_sha256||m.bytes!=a.bytes||!(30..=2*1024*1024).contains(&m.poster_bytes){return Err(e("creator_download_manifest_invalid"));}
  let v=Verified{sha:m.sha256,bytes:m.bytes,poster_sha:m.poster_sha256};
  let _media=asset(root,&s.request_id,&v,false)?;
  let (_,poster)=asset(root,&s.request_id,&v,true)?;
  if poster.metadata().map_err(|_|e("download_poster_invalid"))?.len()!=m.poster_bytes{return Err(e("download_poster_invalid"));}
  Ok(v)
 }
 fn worker(app:&App,s:&ExactSelection,root:&FsPath,a:Authority,cli_pin:&str)->store::Result<()>{
  let id=uuid::Uuid::new_v4().to_string();let dir=root.join(format!("control-{id}"));
  std::fs::create_dir(&dir).map_err(|_|e("creator_download_control_collision"))?;
  crate::controller_native_launcher::clean_path(&dir)?;
  let permission=dir.join("permission.json");
  let p=serde_json::to_vec(&json!({"basis":if a.fixture_tls.is_some(){"snapmatic_owned_test"}else{"creator_supplied_direct_mp4"},"job_id":id,"profile":s.source_url,"video_id":s.post_id,
    "url":a.url,"source_sha256":a.source_sha256,"bytes":a.bytes,"expires_at":a.expires_at})).map_err(|_|e("creator_download_control_write"))?;
  let pp=sha(&p);write(&permission,&p)?;
  let mut files=a.files.clone();
  files.insert(permission.to_string_lossy().into_owned(),pp.clone());
  if let Some(f)=a.fixture.as_ref(){
   crate::controller_native_launcher::clean_path(f)?;
   let mut lease=crate::controller_native_process::lease(f,&a.source_sha256)?;
   lease.seek(SeekFrom::Start(0)).map_err(|_|e("creator_download_fixture_invalid"))?;
   if a.bytes>16*1024*1024||lease.metadata().map_err(|_|e("creator_download_fixture_invalid"))?.len()!=a.bytes{return Err(e("creator_download_fixture_invalid"));}
   let mut b=Vec::new();lease.take(a.bytes+1).read_to_end(&mut b).map_err(|_|e("creator_download_fixture_invalid"))?;
   if b.len() as u64!=a.bytes{return Err(e("creator_download_fixture_invalid"));}
   let fp=dir.join("fixture.mp4");write(&fp,&b)?;files.insert(fp.to_string_lossy().into_owned(),a.source_sha256.clone());
  }

  if let Some(t)=&a.fixture_tls{
   let mut ca=crate::controller_native_process::lease(&t.ca_path,&t.ca_sha256)?;
   ca.seek(SeekFrom::Start(0)).map_err(|_|e("creator_download_fixture_invalid"))?;
   let mut raw=Vec::new();ca.take(16385).read_to_end(&mut raw).map_err(|_|e("creator_download_fixture_invalid"))?;
   if !(200..=16384).contains(&raw.len())||sha(&raw)!=t.ca_sha256{return Err(e("creator_download_fixture_invalid"));}
   let cert=dir.join("fixture-ca.pem");write(&cert,&raw)?;files.insert(cert.to_string_lossy().into_owned(),t.ca_sha256.clone());
  }
  let manifest=dir.join("manifest.json");let x=serde_json::to_vec(&json!({"schema":"native-runtime-files-v1","files":files})).map_err(|_|e("creator_download_manifest_invalid"))?;
  let mh=sha(&x);write(&manifest,&x)?;
  let ctl=dir.join("config.json");
  let mut spec=json!({"schema":if a.fixture_tls.is_some(){"selected-direct-tls-fixture-v1"}else if a.mode=="fixture"{"selected-direct-fixture-v1"}else{"selected-direct-creator-v1"},"job_id":id,
    "profile":s.source_url,"video_id":s.post_id,"permission":permission,"permission_sha256":pp,
    "manifest":manifest,"manifest_sha256":mh,"tools":a.tools,"max_bytes":180*1024*1024});
  if let Some(t)=&a.fixture_tls{spec["fixture_port"]=json!(t.port);spec["fixture_ca_sha256"]=json!(t.ca_sha256);}
  let b=serde_json::to_vec(&spec).map_err(|_|e("creator_download_control_write"))?;
  let ch=sha(&b);write(&ctl,&b)?;
  let mut child=crate::controller_native_process::Owned::gated_manifest(&a.python,&a.cli,&ctl,[PYTHON_SHA,cli_pin,&ch],&dir,&manifest,&mh)?;
  let until=Instant::now()+Duration::from_secs(260);
  loop{
   match child.poll()?{Some(true)=>break,Some(false)=>return Err(e("authorized_transfer_or_validation_failed")),None=>{}}
   if Instant::now()>=until{let _=child.cancel();return Err(e("creator_download_timeout_held"));}
   std::thread::sleep(Duration::from_millis(100));
  }
  if child.active()?!=0{return Err(e("download_descendants_unresolved"));}
  {
   let d=app.db.lock().map_err(|_|e("storage_unavailable"))?;
   d.execute("UPDATE snapmatic_source_downloads_v1 SET stage='verifying' WHERE request_id=?1 AND stage='running'",[&s.request_id])?;
  }
  let v=accept_direct(root,&id,s,&a,&pp)?;
  let rows=crate::workspace_media::index(app)?;
  let dup=rows.iter().chain(crate::workspace_phase4::rows(app)?.iter()).any(|x|x["preview_sha256"].as_str()==Some(&v.sha));
  let mut d=app.db.lock().map_err(|_|e("storage_unavailable"))?;
  complete(&mut d,s,&v,dup)?;Ok(())
 }
 pub(super) fn begin(app:&App,id:i64,expected:&str)->store::Result<()>{
  let s={let d=app.db.lock().map_err(|_|e("storage_unavailable"))?;selection(&d,id)?.ok_or(e("choose_exactly_one_post"))?};
  if s.request_id!=expected{return Err(e("selected_post_changed_refresh_first"));}
  let (root,a,cli_pin)=load(app,&s)?;
  if super::run::RUNNING.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return Ok(());}
  let reserved=(||{let mut d=app.db.lock().map_err(|_|e("storage_unavailable"))?;claim(&mut d,&s)})();
  match reserved {
   Ok(true)=>{
    let app=app.clone();let fallback=app.clone();let key=s.request_id.clone();
    if std::thread::Builder::new().name("snapmatic-authorized-creator-download".into()).spawn(move||{
     if let Err(e)=worker(&app,&s,&root,a,&cli_pin){if let Ok(d)=app.db.lock(){let _=hold(&d,&s.request_id,e.0);}}
     super::run::RUNNING.store(false,Ordering::Release);
    }).is_err(){
     super::run::RUNNING.store(false,Ordering::Release);
     if let Ok(d)=fallback.db.lock(){let _=hold(&d,&key,"download_launcher_unavailable");}
     return Err(e("download_launcher_unavailable"));
    }
    Ok(())
   },
   Ok(false)=>{super::run::RUNNING.store(false,Ordering::Release);Ok(())},
   Err(e)=>{super::run::RUNNING.store(false,Ordering::Release);Err(e)}
  }
 }
}
