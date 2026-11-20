//! Durable manual admission only. No default producer, process or HTTP route.
use crate::{controller_store as store,controller_proof::{SelectedCut,Hash256}};
use rusqlite::{Connection,params};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub job_id:String,pub revision:i64,pub source_binding:String,pub selection:SelectedCut,
    pub approval_sha256:String,pub worker_job_id:String,pub workspace:String,
    pub executable_sha256:String,pub helper_sha256:String,pub expires_at:i64,
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    pub worker_job_id:String,pub receipt_sha256:String,pub artifact_sha256:String,
    pub selection:SelectedCut,
    pub editorial_review:Option<crate::controller_proof::EditorialReview>,
}
/// Integration must independently authenticate operator approval/current leases
/// and verify native committed receipt + actual runtime/helper identities.
/// Hash syntax or a serialized Request alone is never approval.
pub trait Authority {
    fn approves(&self,request:&Request,now:i64)->bool;
    fn completion(&self,request:&Request,evidence:&Completion,bytes:&[u8])->bool;
}
pub struct Unavailable;
impl Authority for Unavailable {
    fn approves(&self,_:&Request,_:i64)->bool{false}
    fn completion(&self,_:&Request,_:&Completion,_:&[u8])->bool{false}
}
pub fn init(db:&Connection)->store::Result<()> {
    let tx=db.unchecked_transaction()?;
    let present:(bool,bool)=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='controller_admissions'),EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='controller_admission_meta')",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if present.0!=present.1{return Err(store::Error("admission_schema_unknown"));}
    tx.execute_batch("CREATE TABLE IF NOT EXISTS controller_admission_meta(version INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS controller_admissions(
        id TEXT PRIMARY KEY,job_id TEXT NOT NULL UNIQUE,admitted_revision INTEGER NOT NULL,
        worker_job_id TEXT NOT NULL UNIQUE,cut_key TEXT NOT NULL UNIQUE,
        request TEXT NOT NULL,request_sha256 TEXT NOT NULL,
        state TEXT NOT NULL CHECK(state IN('RESERVED','COMPLETED','HELD')),
        completion TEXT,created_at INTEGER NOT NULL,
        FOREIGN KEY(job_id) REFERENCES controller_jobs(id));
        CREATE UNIQUE INDEX IF NOT EXISTS controller_one_native_admission
        ON controller_admissions((1)) WHERE state IN('RESERVED','HELD');
        CREATE TABLE IF NOT EXISTS controller_admission_launches(
        admission_id TEXT PRIMARY KEY,at INTEGER NOT NULL,
        FOREIGN KEY(admission_id) REFERENCES controller_admissions(id));")?;
    let versions:Vec<i64>=tx.prepare("SELECT version FROM controller_admission_meta")?
        .query_map([],|r|r.get(0))?.collect::<std::result::Result<_,_>>()?;
    if versions.is_empty() && !present.0{tx.execute("INSERT INTO controller_admission_meta VALUES(1)",[])?;}
    else if versions!=[1]{return Err(store::Error("admission_schema_unknown"));}
    tx.commit()?;Ok(())
}

#[cfg(test)] pub(crate) mod tests {
    use super::*;
    struct Fake {request:Request}
    impl Authority for Fake {
        fn approves(&self,r:&Request,_:i64)->bool{r==&self.request}
        fn completion(&self,r:&Request,e:&Completion,bytes:&[u8])->bool{
            r==&self.request && e.worker_job_id==r.worker_job_id && e.receipt_sha256==hash(b"fake committed receipt") && bytes==b"fake completed media"
        }
    }
    #[test] fn prelaunch_diagnostic_is_durable_and_allowlisted(){
        let (app,mut r)=setup();r.expires_at=200;
        let authority=Fake{request:r.clone()};let mut db=app.db.lock().unwrap();
        let id=reserve(&mut db,"admit_one_clip",&r,&authority,100).unwrap();
        hold_prelaunch(&mut db,&id,101,&store::Error("source_unavailable")).unwrap();
        let job=store::job(&db,&r.job_id).unwrap();
        assert_eq!(job.state,"HELD");assert_eq!(job.hold_reason,"prelaunch_source_unavailable");
        assert_eq!(prelaunch_reason(&store::Error("private untrusted text")),"prelaunch_unclassified");
        assert_eq!(db.query_row("SELECT COUNT(*) FROM controller_admission_launches",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
    pub(crate) fn setup()->(crate::App,Request){
        let app=crate::controller::tests::app();
        let source=crate::controller_source::controller_selection(&app,1,"sample-source/original.mp4").unwrap();
        let binding=crate::controller_source::controller_binding(&source);
        let j={let mut db=app.db.lock().unwrap();store::configure(&mut db,0,"resume",100).unwrap();store::enqueue(&mut db,1,"sample-source/original.mp4","tiktok",&binding,100).unwrap()};
        let sha=Hash256::parse(&hash(b"inert source")).unwrap();
        let cut=Hash256::parse(&hash(format!("{}recipe-v1:cut=1000-31000",sha.hex()).as_bytes())).unwrap();
        let selection=SelectedCut{generation:1,speech_id:None,caption_status:crate::controller_proof::CaptionStatus::NoSpeech,provider:"synthetic".into(),provider_id:"1".into(),source_sha256:sha,source_revision:"revision1".into(),start_ms:1000,end_ms:31000,recipe_version:"recipe-v1".into(),durable_cut_key:cut};
        let r=Request{job_id:j.id,revision:j.revision,source_binding:binding,selection,approval_sha256:hash(b"independent fake review"),worker_job_id:"worker-one".into(),workspace:app.media.join("isolated-worker").to_string_lossy().into_owned(),executable_sha256:hash(b"fake runtime pin"),helper_sha256:hash(b"fake helper pin"),expires_at:200};
        (app,r)
    }
    #[test] fn manual_admission_fake_lifecycle_reopen_and_stale_hold(){
        let(app,r)=setup();let authority=Fake{request:r.clone()};let mut db=app.db.lock().unwrap();
        let before=db.total_changes();
        assert!(reserve(&mut db,"admit_one_clip",&r,&Unavailable,101).is_err());
        assert!(reserve(&mut db,"automatic",&r,&authority,101).is_err());
        let mut wrong=r.clone();wrong.revision+=1;
        assert!(reserve(&mut db,"admit_one_clip",&wrong,&authority,101).is_err());
        assert_eq!(db.total_changes(),before);
        let id=reserve(&mut db,"admit_one_clip",&r,&authority,101).unwrap();
        assert_eq!(store::job(&db,&r.job_id).unwrap().state,"PROCESSING");
        assert!(reserve(&mut db,"admit_one_clip",&r,&authority,102).is_err());
        let e=Completion{worker_job_id:r.worker_job_id.clone(),receipt_sha256:hash(b"fake committed receipt"),artifact_sha256:hash(b"fake completed media"),selection:r.selection.clone(),editorial_review:None};
        let mut bad=e.clone();bad.worker_job_id="other-worker".into();
        assert!(complete(&mut db,&id,&bad,b"fake completed media",&authority,102).is_err());
        let ready=complete(&mut db,&id,&e,b"fake completed media",&authority,103).unwrap();
        assert_eq!(ready.state,"READY_FOR_REVIEW");assert_eq!(ready.review,"PENDING");
        let changes=db.total_changes();assert_eq!(complete(&mut db,&id,&e,b"fake completed media",&authority,104).unwrap(),ready);assert_eq!(db.total_changes(),changes);
        let file=app.media.join("admission-reopen.sqlite");db.execute("VACUUM INTO ?",[file.to_str().unwrap()]).unwrap();drop(db);
        let mut reopened=Connection::open(file).unwrap();store::init(&reopened).unwrap();store::recover(&mut reopened,105).unwrap();
        assert_eq!(store::preview(&reopened,&r.job_id).unwrap(),b"fake completed media");assert!(store::settings(&reopened).unwrap().paused);
        assert_eq!(complete(&mut reopened,&id,&e,b"fake completed media",&authority,106).unwrap(),ready);
        // A distinct interrupted reservation never recovers authority or retries.
        let(app,r)=setup();let authority=Fake{request:r.clone()};let mut db=app.db.lock().unwrap();
        let id=reserve(&mut db,"admit_one_clip",&r,&authority,101).unwrap();store::recover(&mut db,102).unwrap();
        assert_eq!(store::job(&db,&r.job_id).unwrap().state,"HELD");
        assert_eq!(db.query_row("SELECT state FROM controller_admissions WHERE id=?",[&id],|r|r.get::<_,String>(0)).unwrap(),"HELD");
        assert!(complete(&mut db,&id,&e,b"fake completed media",&authority,103).is_err());
        let j=store::job(&db,&r.job_id).unwrap();assert!(store::review(&mut db,&j.id,j.revision,"resume",104).is_err());
    }
    #[test] fn manual_admission_stale_source_expiry_and_sole_worker(){
        let(app,r)=setup();let authority=Fake{request:r.clone()};let mut db=app.db.lock().unwrap();
        assert!(reserve(&mut db,"admit_one_clip",&r,&authority,200).is_err());
        db.execute("UPDATE profiles SET archived=1 WHERE id=1",[]).unwrap();let before=db.total_changes();
        assert!(reserve(&mut db,"admit_one_clip",&r,&authority,101).is_err());assert_eq!(db.total_changes(),before);
        db.execute("UPDATE profiles SET archived=0 WHERE id=1",[]).unwrap();reserve(&mut db,"admit_one_clip",&r,&authority,101).unwrap();
        let j=store::enqueue(&mut db,1,"sample-source/original.mp4","instagram",&r.source_binding,102).unwrap();
        let mut second=r.clone();second.job_id=j.id;second.worker_job_id="worker-two".into();let other=Fake{request:second.clone()};
        assert!(reserve(&mut db,"admit_one_clip",&second,&other,103).is_err());assert_eq!(store::job(&db,&second.job_id).unwrap().state,"QUEUED");
    }
}
fn hash(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
pub fn reserved(db:&Connection,id:&str)->store::Result<Request>{
    let(raw,pin,state):(String,String,String)=db.query_row("SELECT request,request_sha256,state FROM controller_admissions WHERE id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    if state!="RESERVED" || raw.len()>16384 || hash(raw.as_bytes())!=pin{return Err(store::Error("admission_reconciliation_required"));}
    serde_json::from_str(&raw).map_err(|_|store::Error("admission_integrity_failed"))
}
pub fn consume_launch(db:&mut Connection,id:&str,r:&Request,now:i64)->store::Result<()> {
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if reserved(&tx,id)?!=*r || !valid(r,now){return Err(store::Error("manual_admission_denied"));}
    let j=store::job(&tx,&r.job_id)?;
    if j.state!="PROCESSING" || j.revision!=r.revision+1 || store::settings(&tx)?.paused
        || !crate::controller_source::controller_current(&tx,j.source_id,&r.source_binding)
        || !crate::controller_source::local_selection_allowed(&tx,j.source_id,&r.selection.provider,&r.selection.provider_id){return Err(store::Error("stale_revision"));}
    tx.execute("INSERT INTO controller_admission_launches VALUES(?,?)",params![id,now])?;
    tx.commit()?;Ok(())
}
pub fn hold(db:&mut Connection,id:&str,now:i64)->store::Result<()> {
    hold_reason(db,id,now,"supervisor_unresolved")
}
pub fn prelaunch_reason(error:&store::Error)->&'static str {
    match error.0 {
        "source_unavailable"=>"prelaunch_source_unavailable",
        "source_changed"=>"prelaunch_source_changed",
        "worker_pin_mismatch"|"worker_pin_unavailable"|"worker_pin_changed"|"unsafe_worker_path"=>"prelaunch_worker_pins",
        "fresh_worker_workspace_required"|"workspace_unavailable"=>"prelaunch_workspace",
        "isolated_controller_db_required"|"isolated_path_invalid"|"isolated_path_unavailable"|"isolated_path_reparse"=>"prelaunch_path_scope",
        "runtime_config_binding"|"runtime_config_schema"|"runtime_config_size"|"runtime_config_read"|"pin_mismatch"|"pin_open"|"pin_kind"|"pin_read"=>"prelaunch_runtime_config",
        "manual_admission_denied"|"isolated_runtime_admission_denied"=>"prelaunch_authority",
        "stale_revision"=>"prelaunch_source_revision",
        "supervisor_start_failed"=>"prelaunch_supervisor_thread",
        _=>"prelaunch_unclassified",
    }
}
pub fn hold_prelaunch(db:&mut Connection,id:&str,now:i64,error:&store::Error)->store::Result<()> {
    hold_reason(db,id,now,prelaunch_reason(error))
}
fn hold_reason(db:&mut Connection,id:&str,now:i64,reason:&'static str)->store::Result<()> {
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let r=reserved(&tx,id)?;
    tx.execute("UPDATE controller_admissions SET state='HELD' WHERE id=?",[id])?;
    tx.execute("UPDATE controller_jobs SET state='HELD',hold_reason=?,retry_safe=0,revision=revision+1,updated_at=? WHERE id=? AND state='PROCESSING'",params![reason,now,r.job_id])?;
    tx.commit()?;Ok(())
}
fn valid(r:&Request,now:i64)->bool {
    let s=&r.selection;
    r.revision>=0 && r.revision<i64::MAX && now>=0 && r.expires_at>now
    && r.expires_at.saturating_sub(now)<=900 && !r.job_id.is_empty()
    && !r.source_binding.is_empty() && !r.worker_job_id.is_empty() && r.worker_job_id.len()<=256
    && std::path::Path::new(&r.workspace).is_absolute()
    && [&r.approval_sha256,&r.executable_sha256,&r.helper_sha256].iter().all(|v|Hash256::parse(v).is_some())
    && SelectedCut::from_authenticated_profile(s.clone()).as_ref()==Some(s)
    && (1..=60000).contains(&s.end_ms.saturating_sub(s.start_ms))
    && hash(format!("{}{}:cut={}-{}",s.source_sha256.hex(),s.recipe_version,s.start_ms,s.end_ms).as_bytes())==s.durable_cut_key.hex()
}
pub fn reserve(db:&mut Connection,action:&str,r:&Request,authority:&dyn Authority,now:i64)->store::Result<String> {
    if action!="admit_one_clip" || !valid(r,now) || !authority.approves(r,now){return Err(store::Error("manual_admission_denied"));}
    let raw=serde_json::to_string(r).map_err(|_|store::Error("invalid_selection"))?;
    if raw.len()>16384{return Err(store::Error("invalid_selection"));}
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let j=store::job(&tx,&r.job_id)?;
    if store::settings(&tx)?.paused || j.state!="QUEUED" || j.revision!=r.revision
        || j.source_binding!=r.source_binding || !crate::controller_source::controller_current(&tx,j.source_id,&r.source_binding)
        || !crate::controller_source::local_selection_allowed(&tx,j.source_id,&r.selection.provider,&r.selection.provider_id)
        || !authority.approves(r,now){return Err(store::Error("manual_admission_denied"));}
    if tx.query_row("SELECT EXISTS(SELECT 1 FROM controller_admissions WHERE state IN('RESERVED','HELD') OR job_id=? OR worker_job_id=? OR cut_key=?)",
        params![r.job_id,r.worker_job_id,r.selection.durable_cut_key.hex()],|row|row.get::<_,bool>(0))?{return Err(store::Error("admission_reconciliation_required"));}
    let id=uuid::Uuid::new_v4().to_string();
    tx.execute("INSERT INTO controller_admissions VALUES(?,?,?,?,?,?,?,'RESERVED',NULL,?)",
        params![id,r.job_id,r.revision,r.worker_job_id,r.selection.durable_cut_key.hex(),raw,hash(raw.as_bytes()),now])?;
    tx.execute("UPDATE controller_jobs SET state='PROCESSING',revision=revision+1,retry_safe=0,updated_at=? WHERE id=?",params![now,r.job_id])?;
    tx.execute("INSERT INTO controller_events(job_id,revision,action,state,reason,at) VALUES(?,?,'manual_admission','PROCESSING',?,?)",params![r.job_id,r.revision+1,id,now])?;
    tx.commit()?;Ok(id)
}
pub fn complete(db:&mut Connection,id:&str,e:&Completion,bytes:&[u8],authority:&dyn Authority,now:i64)->store::Result<store::Job> {
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (raw,pin,state,saved):(String,String,String,Option<String>)=tx.query_row("SELECT request,request_sha256,state,completion FROM controller_admissions WHERE id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    if hash(raw.as_bytes())!=pin{return Err(store::Error("admission_integrity_failed"));}
    let r:Request=serde_json::from_str(&raw).map_err(|_|store::Error("admission_integrity_failed"))?;
    let completion=serde_json::to_string(e).map_err(|_|store::Error("completion_evidence_mismatch"))?;
    let j=store::job(&tx,&r.job_id)?;
    if (r.worker_job_id==crate::controller_proof::EditorialReview::WORKER)!=e.editorial_review.is_some()
        || e.editorial_review.as_ref().is_some_and(|v|!v.matches(&r.selection,&r.worker_job_id,Some(&r.approval_sha256)))
        || e.worker_job_id!=r.worker_job_id || e.selection!=r.selection || Hash256::parse(&e.receipt_sha256).is_none()
        || bytes.is_empty() || bytes.len()>store::PREVIEW_LIMIT || hash(bytes)!=e.artifact_sha256
        || j.source_binding!=r.source_binding || !crate::controller_source::controller_current(&tx,j.source_id,&r.source_binding)
        || !crate::controller_source::local_selection_allowed(&tx,j.source_id,&r.selection.provider,&r.selection.provider_id)
        || !authority.completion(&r,e,bytes){return Err(store::Error("completion_evidence_mismatch"));}
    if state=="COMPLETED" && saved.as_deref()==Some(completion.as_str()) && j.state=="READY_FOR_REVIEW"
        && j.preview_sha256.as_deref()==Some(e.artifact_sha256.as_str()) && j.caption_status.as_deref()==Some(e.selection.caption_status.as_str()){return Ok(j);}
    if state!="RESERVED" || j.state!="PROCESSING" || j.revision!=r.revision+1{return Err(store::Error("admission_reconciliation_required"));}
    let next=j.revision.checked_add(1).ok_or(store::Error("stale_revision"))?;
    tx.execute("INSERT INTO controller_previews VALUES(?,?,?,?)",params![r.job_id,next,e.artifact_sha256,bytes])?;
    store::record_caption_status(&tx,&r.job_id,next,Some(e.selection.caption_status.as_str()))?;
    store::record_editorial_review(&tx,&r.job_id,next,e.editorial_review.as_ref())?;
    tx.execute("UPDATE controller_jobs SET state='READY_FOR_REVIEW',review='PENDING',hold_reason='',preview_revision=?,revision=?,updated_at=? WHERE id=?",params![next,next,now,r.job_id])?;
    tx.execute("UPDATE controller_admissions SET state='COMPLETED',completion=? WHERE id=?",params![completion,id])?;
    tx.execute("INSERT INTO controller_events(job_id,revision,action,state,reason,at) VALUES(?,?,'admitted_completion','READY_FOR_REVIEW',?,?)",params![r.job_id,next,id,now])?;
    tx.commit()?;store::job(db,&r.job_id)
}
