//! Normal Controller manual admission. Default unavailable; never starts on load.
use crate::{App,controller_store as store};
use std::{path::Path,sync::Arc};
#[derive(Clone,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorSelection {pub source_id:i64,pub source_ref:String,pub start_ms:u64,pub end_ms:u64}
fn check_editor_selection(app:&App,r:&crate::controller_admission::Request,e:&EditorSelection)->store::Result<()> {
 if e.start_ms>=e.end_ms||e.start_ms!=r.selection.start_ms||e.end_ms!=r.selection.end_ms{return Err(store::Error("reviewed_job_mismatch"))}
 let j={let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;store::job(&db,&r.job_id)?};
 if j.source_id!=e.source_id||j.source_ref!=e.source_ref||j.revision!=r.revision||j.state!="QUEUED"||j.source_binding!=r.source_binding{return Err(store::Error("reviewed_job_mismatch"))}
 let file=crate::controller_source::controller_file(app,e.source_id,&e.source_ref).map_err(|_|store::Error("source_unavailable"))?;
 #[cfg(windows)] let _lease=crate::controller_native_process::lease(&file,&r.selection.source_sha256.hex())?;
 #[cfg(not(windows))] {use sha2::{Digest,Sha256};let bytes=std::fs::read(file).map_err(|_|store::Error("source_unavailable"))?;if format!("{:x}",Sha256::digest(bytes))!=r.selection.source_sha256.hex(){return Err(store::Error("source_changed"))}}
 let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
 if !crate::controller_source::controller_current(&db,e.source_id,&r.source_binding){return Err(store::Error("stale_revision"))}
 Ok(())
}
pub trait Manual:Send+Sync {
 fn configured(&self)->bool{false}
 fn reviewed_job_id(&self)->Option<&str>{None}
 fn shutdown(&self){}
 fn admit(&self,_:&App,_:&str,_:i64)->store::Result<serde_json::Value>{Err(store::Error("native_manual_unavailable"))}
 fn admit_editor(&self,_:&App,_:&EditorSelection)->store::Result<serde_json::Value>{Err(store::Error("native_manual_unavailable"))}
}
pub struct Unavailable;impl Manual for Unavailable{}
pub fn provision(args:&[String],db:&Path,media:&Path)->store::Result<Arc<dyn Manual>>{
 let paths:Vec<_>=args.windows(2).filter(|v|v[0]=="--isolated-review-plan").collect();
 let pins:Vec<_>=args.windows(2).filter(|v|v[0]=="--isolated-review-sha256").collect();
 if paths.is_empty()&&pins.is_empty()&&!args.iter().any(|s|s=="--isolated-review-plan"||s=="--isolated-review-sha256"){return Ok(Arc::new(Unavailable));}
 if paths.len()!=1||pins.len()!=1{return Err(store::Error("isolated_review_arguments"));}
 #[cfg(windows)] {return windows::load(Path::new(&paths[0][1]),&pins[0][1],db,media);}
 #[cfg(not(windows))] {let _=(db,media);Err(store::Error("native_windows_required"))}
}
#[cfg(windows)] mod windows {
 use super::*;
 use crate::{controller_admission as admission,controller_native_launcher as launch,controller_native_process as native,controller_supervisor as supervisor};
 use std::{fs::File,path::PathBuf,sync::{Mutex,atomic::{AtomicBool,Ordering}}};
 use serde::Deserialize;
 #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Plan {
  schema:String,publishing_enabled:bool,request:admission::Request,runtime:launch::RuntimeBinding,
  executable:PathBuf,helper:PathBuf,approval:PathBuf,db:PathBuf,media:PathBuf,seconds:u64,
 }
 struct Approved {plan:Plan,_lease:File}
 impl admission::Authority for Approved {
  fn approves(&self,r:&admission::Request,now:i64)->bool{r==&self.plan.request&&now<r.expires_at&&r.expires_at-now<=900}
  fn completion(&self,_:&admission::Request,_:&admission::Completion,_:&[u8])->bool{false} // supervisor verifies real receipt independently
 }
 impl launch::RuntimeAuthority for Approved {
  fn approves_runtime(&self,r:&admission::Request,b:&launch::RuntimeBinding,now:i64)->bool{
   admission::Authority::approves(self,r,now)&&b==&self.plan.runtime
  }
 }
 struct Provisioned {cancel:Arc<AtomicBool>,approved:Arc<Approved>,attempt:Mutex<bool>,thread:Mutex<Option<std::thread::JoinHandle<store::Result<store::Job>>>>}
 pub(super) fn load(path:&Path,pin:&str,db:&Path,media:&Path)->store::Result<Arc<dyn Manual>> {
  if pin.len()!=64||!pin.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){return Err(store::Error("independent_review_pin_required"));}
  launch::clean_path(path)?;
  let lease=native::lease(path,pin)?;
  if lease.metadata().map_err(|_|store::Error("review_plan_read"))?.len()>65536{return Err(store::Error("review_plan_size"));}
  let plan:Plan=serde_json::from_slice(&std::fs::read(path).map_err(|_|store::Error("review_plan_read"))?).map_err(|_|store::Error("review_plan_schema"))?;
  if plan.schema!="isolated-controller-manual-review-v1"||plan.publishing_enabled||plan.db!=db||plan.media!=media||!db.starts_with(&plan.runtime.isolated_root)||!media.starts_with(&plan.runtime.isolated_root)||!(1..=180).contains(&plan.seconds){return Err(store::Error("isolated_review_scope"));}
  launch::clean_path(db)?;launch::clean_path(media)?;
  // Validated before opening/migrating app data. A plan's own hash is not read
  // from that plan: the external operator supplies it on explicit invocation.
  let _config=launch::check_config(&plan.request,&plan.runtime)?;
  Ok(Arc::new(Provisioned{cancel:Arc::new(AtomicBool::new(false)),approved:Arc::new(Approved{plan,_lease:lease}),attempt:Mutex::new(false),thread:Mutex::new(None)}))
 }
 impl Manual for Provisioned {
  fn admit_editor(&self,app:&App,e:&EditorSelection)->store::Result<serde_json::Value>{
   // A UI selection is not authority. Match the independently pinned reviewed
   // request exactly, then retain all existing runtime, rubric/cut and claim gates.
   check_editor_selection(app,&self.approved.plan.request,e)?;
   let r=&self.approved.plan.request;self.admit(app,&r.job_id,r.revision)
  }
  fn configured(&self)->bool{true}
  fn reviewed_job_id(&self)->Option<&str>{Some(&self.approved.plan.request.job_id)}
  fn shutdown(&self){
   self.cancel.store(true,Ordering::SeqCst);
   if let Ok(mut slot)=self.thread.lock(){if let Some(handle)=slot.take(){let _=handle.join();}}
  }
  fn admit(&self,app:&App,id:&str,revision:i64)->store::Result<serde_json::Value>{
   if self.cancel.load(Ordering::SeqCst){return Err(store::Error("native_session_closing"));}
   let r=&self.approved.plan.request;
   if id!=r.job_id||revision!=r.revision{return Err(store::Error("reviewed_job_mismatch"));}
   let mut attempted=self.attempt.lock().map_err(|_|store::Error("native_admission_unavailable"))?;
   if *attempted{return Err(store::Error("native_attempt_already_consumed"));}
   let _config=launch::check_config(r,&self.approved.plan.runtime)?;
   let now=chrono::Utc::now().timestamp();
   let admission_id={let mut db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
    admission::reserve(&mut db,"admit_one_clip",r,self.approved.as_ref(),now)?};
   *attempted=true;
   let p=&self.approved.plan;
   let files=supervisor::Files{executable:p.executable.clone(),helper:p.helper.clone(),approval:p.approval.clone()};
   match launch::start_isolated_manual(app.clone(),admission_id.clone(),files,p.runtime.clone(),self.approved.clone(),self.approved.clone(),self.cancel.clone(),p.seconds){
    Ok(handle)=>{*self.thread.lock().map_err(|_|store::Error("native_admission_unavailable"))?=Some(handle);Ok(serde_json::json!({"state":"PROCESSING","admission_id":admission_id,"publish":false}))},
    Err(error)=>{if let Ok(mut db)=app.db.lock(){let _=admission::hold_prelaunch(&mut db,&admission_id,now,&error);}Err(error)}
   }
  }
 }
}

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn editor_binding_is_exact_and_never_issues_authority(){
  let(app,r)=crate::controller_admission::tests::setup();
  let e=EditorSelection{source_id:1,source_ref:"sample-source/original.mp4".into(),start_ms:1000,end_ms:31000};
  assert!(check_editor_selection(&app,&r,&e).is_ok());
  assert!(Unavailable.admit_editor(&app,&e).is_err());
  for changed in [EditorSelection{start_ms:1001,..e.clone()},EditorSelection{end_ms:31001,..e.clone()},EditorSelection{source_id:2,..e.clone()},EditorSelection{source_ref:"sample-source/other.mp4".into(),..e.clone()}]{assert!(check_editor_selection(&app,&r,&changed).is_err());}
  let mut wrong=r.clone();wrong.selection.source_sha256=crate::controller_proof::Hash256::parse(&"0".repeat(64)).unwrap();assert!(check_editor_selection(&app,&wrong,&e).is_err());
  let db=app.db.lock().unwrap();assert_eq!(db.query_row("SELECT COUNT(*) FROM controller_admissions",[],|r|r.get::<_,i64>(0)).unwrap(),0);assert_eq!(db.query_row("SELECT COUNT(*) FROM controller_admission_launches",[],|r|r.get::<_,i64>(0)).unwrap(),0);
 }
 #[test] fn manual_native_provision_requires_external_explicit_pin(){
  let db=Path::new("unused-db");let media=Path::new("unused-media");
  assert!(!provision(&[],db,media).unwrap().configured());
  assert!(provision(&["--isolated-review-plan".into()],db,media).is_err());
  assert!(provision(&["--isolated-review-plan".into(),"unused".into(),"--isolated-review-sha256".into(),"not-a-pin".into()],db,media).is_err());
 }
}
