//! Owner-write registry + immutable in-memory observation provider. No HTTP writes.
//! No live authorization/OS lease implementation is installed by default.
#![forbid(unsafe_code)]
use crate::{controller_worker::{self as bridge,Binding,Observation,Reader,View},controller_store::Job,controller_proof::Hash256};
use rusqlite::{Connection,OptionalExtension,params};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{collections::BTreeMap,fs::File,io::{Read,Seek,SeekFrom},sync::{Arc,Mutex,RwLock}};
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub schema:String,pub approval_id:String,pub revision:u64,pub expires_at:i64,
    pub binding:Binding,pub worker_job_id:String,pub worker_root_identity:String,
    // Independent owner-reviewed receipt/artifact expectations; not taken on trust
    // from a worker message or synthesized from a source filename.
    pub receipt_sha256:String,pub artifact_sha256:String,pub artifact_bytes:u64,
}
/// Trusted owner must verify exact review against its independently admitted state.
/// No boolean approval field, self-signed row, HTTP endpoint or default allow.
pub trait Authority:Send+Sync {fn allows(&self,review:&Review,now:i64)->bool;}
/// Integration must retain source, root, queue, receipt and artifact immutable
/// leases. It must compare exact identities and current source to this review.
pub trait LeaseWitness {fn current(&self,review:&Review)->bool;}
#[derive(Clone)]struct Cached {review_hash:String,view:View}
struct Shared {db:Mutex<Connection>,cache:RwLock<BTreeMap<String,Cached>>,authority:Arc<dyn Authority>}
pub struct OwnerWriter {shared:Arc<Shared>}
pub struct Provider {shared:Arc<Shared>}
fn hash(raw:&[u8])->String{format!("{:x}",Sha256::digest(raw))}
fn valid(r:&Review,now:i64)->bool {
    r.schema=="controller-reviewed-cut-v1" && !r.approval_id.is_empty() && r.approval_id.len()<=256
    && r.revision>0 && r.revision<=i64::MAX as u64 && r.expires_at>now && !r.binding.job_id.is_empty() && r.binding.job_revision>=0
    && !r.binding.source_binding.is_empty() && !r.worker_job_id.is_empty() && r.worker_job_id.len()<=256
    && !r.worker_root_identity.is_empty() && r.worker_root_identity.len()<=1024
    && r.binding.selection.generation>0 && !r.binding.selection.source_revision.is_empty()
    && r.binding.selection.start_ms<r.binding.selection.end_ms
    && Hash256::parse(&r.receipt_sha256).is_some() && Hash256::parse(&r.artifact_sha256).is_some()
    && (1..=16*1024*1024).contains(&r.artifact_bytes)
}
fn load(db:&Connection,id:&str)->Result<Option<Review>,()>{
    let raw:Option<String>=db.query_row("SELECT review FROM controller_review_registry WHERE job_id=?1",[id],|r|r.get(0)).optional().map_err(|_|())?;
    raw.map(|s|if s.len()>16384 {Err(())}else{serde_json::from_str(&s).map_err(|_|())}).transpose()
}
/// Explicit trusted owner setup only. Supply a protected dedicated registry DB;
/// never the worker ledger. Caller retains filesystem/ACL protection of this DB.
pub fn configure(db:Connection,authority:Arc<dyn Authority>)->Result<(OwnerWriter,Provider),()> {
    if db.is_readonly(rusqlite::MAIN_DB).map_err(|_|())?{return Err(());}
    db.busy_timeout(std::time::Duration::from_millis(500)).map_err(|_|())?;
    // Never initialize this registry in the standalone worker or product ledger.
    let foreign:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT IN ('controller_review_registry','controller_review_registry_meta'))",[],|r|r.get(0)).map_err(|_|())?;
    if foreign{return Err(());}

    let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='controller_review_registry')",[],|r|r.get(0)).map_err(|_|())?;
    if !exists {
        db.execute_batch("BEGIN IMMEDIATE; CREATE TABLE controller_review_registry(job_id TEXT PRIMARY KEY, revision INTEGER NOT NULL, review TEXT NOT NULL); CREATE TABLE controller_review_registry_meta(version INTEGER NOT NULL); INSERT INTO controller_review_registry_meta VALUES(1); COMMIT;").map_err(|_|())?;
    }
    let versions:Vec<i64>=db.prepare("SELECT version FROM controller_review_registry_meta").map_err(|_|())?.query_map([],|r|r.get(0)).map_err(|_|())?.collect::<Result<_,_>>().map_err(|_|())?;
    if versions!=[1]{return Err(());}
    let shared=Arc::new(Shared{db:Mutex::new(db),cache:RwLock::new(BTreeMap::new()),authority});
    Ok((OwnerWriter{shared:shared.clone()},Provider{shared}))
}
impl OwnerWriter {
    /// Compare-and-swap owner admission; denied input writes nothing.
    pub fn register(&self,r:&Review,expected_previous:u64,now:i64)->Result<(),()> {
        if !valid(r,now)||r.revision!=expected_previous.checked_add(1).ok_or(())?||!self.shared.authority.allows(r,now){return Err(());}
        let raw=serde_json::to_string(r).map_err(|_|())?;if raw.len()>16384{return Err(());}
        let mut db=self.shared.db.lock().map_err(|_|())?;let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|_|())?;
        let previous:Option<i64>=tx.query_row("SELECT revision FROM controller_review_registry WHERE job_id=?1",[&r.binding.job_id],|r|r.get(0)).optional().map_err(|_|())?;
        if previous.unwrap_or(0)!=i64::try_from(expected_previous).map_err(|_|())?{return Err(());}
        tx.execute("INSERT INTO controller_review_registry VALUES(?1,?2,?3) ON CONFLICT(job_id) DO UPDATE SET revision=excluded.revision,review=excluded.review",params![r.binding.job_id,i64::try_from(r.revision).map_err(|_|())?,raw]).map_err(|_|())?;
        tx.commit().map_err(|_|())?;
        self.shared.cache.write().map_err(|_|())?.remove(&r.binding.job_id);Ok(())
    }
    /// No worker ledger write. Hold immutable leased Files throughout this call.
    /// A private copied View is installed only after before/after authority checks.
    pub fn observe(&self,job:&Job,o:&Observation,db:&mut Connection,mut receipt:Option<File>,artifact:Option<File>,lease:&dyn LeaseWitness,now:i64)->Result<(),()> {
        // Registry lock serializes approval/revision changes with snapshot install.
        let registry=self.shared.db.lock().map_err(|_|())?;
        self.shared.cache.write().map_err(|_|())?.remove(&job.id);
        let r=load(&registry,&job.id)?.ok_or(())?;
        if !valid(&r,now)||!self.shared.authority.allows(&r,now)||!lease.current(&r){return Err(());}
        let checked=(||{
            if let Some(file)=receipt.as_mut(){
                let mut bytes=Vec::new();file.seek(SeekFrom::Start(0)).map_err(|_|())?;
                file.by_ref().take(16385).read_to_end(&mut bytes).map_err(|_|())?;
                if bytes.len()>16384||hash(&bytes)!=r.receipt_sha256{return Err(());}
                let parsed:serde_json::Value=serde_json::from_slice(&bytes).map_err(|_|())?;
                if parsed.get("job_id").and_then(|v|v.as_str())!=Some(&r.worker_job_id)
                    ||parsed.get("sha256").and_then(|v|v.as_str())!=Some(&r.artifact_sha256)
                    ||parsed.get("bytes").and_then(|v|v.as_u64())!=Some(r.artifact_bytes){return Err(());}
                file.seek(SeekFrom::Start(0)).map_err(|_|())?;
            }
            Ok(bridge::observe(job,&r.binding,o,db,receipt,artifact,now))
        })();
        let view=checked.unwrap_or_else(|_|View::hold("reviewed_receipt_identity_mismatch"));
        if !self.shared.authority.allows(&r,now)||!lease.current(&r){
            self.shared.cache.write().map_err(|_|())?.remove(&job.id);return Err(());
        }
        self.shared.cache.write().map_err(|_|())?.insert(job.id.clone(),Cached{review_hash:hash(&serde_json::to_vec(&r).map_err(|_|())?),view});Ok(())
    }
}
impl Reader for Provider {
    fn read(&self,job:&Job,now:i64)->View {
        let Ok(db)=self.shared.db.lock() else{return View::unknown()};
        let Ok(Some(r))=load(&db,&job.id) else{return View::unknown()};
        if !valid(&r,now)||!self.shared.authority.allows(&r,now)
            ||r.binding.job_id!=job.id||r.binding.job_revision!=job.revision||r.binding.source_binding!=job.source_binding{return View::hold("reviewed_registry_unavailable");}
        let Ok(cache)=self.shared.cache.read() else{return View::unknown()};
        let Some(c)=cache.get(&job.id) else{return View::unknown()};
        if serde_json::to_vec(&r).ok().map(|v|hash(&v)).as_deref()!=Some(&c.review_hash){return View::hold("review_changed");}
        if c.view.observed_at.is_some_and(|t|now.checked_sub(t).is_none_or(|age|!(0..=30).contains(&age))){return View::hold("stale_worker_evidence");}
        c.view.clone()
    }
}
#[cfg(test)] mod tests {
 use super::*;
 struct Exact(Vec<String>);impl Authority for Exact{fn allows(&self,r:&Review,_:i64)->bool{self.0.contains(&hash(&serde_json::to_vec(r).unwrap()))}}
 struct Lease(bool);impl LeaseWitness for Lease{fn current(&self,_:&Review)->bool{self.0}}
 fn review(b:Binding,p:&std::path::Path)->Review{let a=std::fs::read(p.join("artifact")).unwrap();Review{schema:"controller-reviewed-cut-v1".into(),approval_id:"synthetic-owner-approval".into(),revision:1,expires_at:200,binding:b,worker_job_id:"one".into(),worker_root_identity:"synthetic-pinned-volume-file-id".into(),receipt_sha256:hash(&std::fs::read(p.join("receipt")).unwrap()),artifact_sha256:hash(&a),artifact_bytes:a.len() as u64}}
 fn configured(r:&Review)->(OwnerWriter,Provider){configure(Connection::open_in_memory().unwrap(),Arc::new(Exact(vec![hash(&serde_json::to_vec(r).unwrap())]))).unwrap()}
 #[test]fn registry_bridge_requires_exact_owner_admission(){
  let(j,b,_,_,_,_,p)=crate::controller_worker::tests::fixture();let r=review(b,&p);let(w,provider)=configured(&r);
  let mut forged=r.clone();forged.binding.selection.source_revision="forged".into();assert!(w.register(&forged,0,100).is_err());assert_eq!(provider.read(&j,100).state,"UNKNOWN");assert_eq!(w.shared.db.lock().unwrap().query_row("SELECT COUNT(*) FROM controller_review_registry",[],|r|r.get::<_,i64>(0)).unwrap(),0);
  w.register(&r,0,100).unwrap();assert!(w.register(&r,0,100).is_err());assert_eq!(provider.read(&j,100).state,"UNKNOWN");
 }
 #[test]fn registry_bridge_immutable_view_no_ui_or_ledger_writes(){
  let(j,b,o,mut db,receipt,artifact,p)=crate::controller_worker::tests::fixture();let r=review(b,&p);let(w,provider)=configured(&r);w.register(&r,0,100).unwrap();
  let before=std::fs::read(p.join("queue.sqlite")).unwrap();w.observe(&j,&o,&mut db,Some(receipt),Some(artifact),&Lease(true),110).unwrap();
  let count=w.shared.db.lock().unwrap().total_changes();w.shared.db.lock().unwrap().execute_batch("PRAGMA query_only=ON").unwrap();
  for _ in 0..3 {assert_eq!(provider.read(&j,110).state,"READY_FOR_REVIEW");}
  assert_eq!(count,w.shared.db.lock().unwrap().total_changes());assert_eq!(db.total_changes(),0);assert_eq!(before,std::fs::read(p.join("queue.sqlite")).unwrap());
  assert_eq!(provider.read(&j,131).reason,"stale_worker_evidence");assert_eq!(provider.read(&j,201).state,"HELD");
 }
 #[test]fn registry_bridge_rejects_identity_and_lost_lease(){
  let(j,b,o,mut db,receipt,artifact,p)=crate::controller_worker::tests::fixture();let mut r=review(b,&p);r.worker_job_id="different-reviewed-job".into();let(w,provider)=configured(&r);w.register(&r,0,100).unwrap();
  w.observe(&j,&o,&mut db,Some(receipt),Some(artifact),&Lease(true),110).unwrap();assert_eq!(provider.read(&j,110).reason,"reviewed_receipt_identity_mismatch");
  assert!(w.observe(&j,&o,&mut db,None,None,&Lease(false),110).is_err());assert_eq!(provider.read(&j,110).state,"UNKNOWN");
 }
 #[test]fn registry_bridge_restart_has_no_cached_completion(){
  let(j,b,o,mut db,receipt,artifact,p)=crate::controller_worker::tests::fixture();let r=review(b,&p);let approved=hash(&serde_json::to_vec(&r).unwrap());let path=p.join("registry.sqlite");
  {let(w,provider)=configure(Connection::open(&path).unwrap(),Arc::new(Exact(vec![approved.clone()]))).unwrap();w.register(&r,0,100).unwrap();w.observe(&j,&o,&mut db,Some(receipt),Some(artifact),&Lease(true),110).unwrap();assert_eq!(provider.read(&j,110).state,"READY_FOR_REVIEW");}
  let(_,provider)=configure(Connection::open(&path).unwrap(),Arc::new(Exact(vec![approved]))).unwrap();assert_eq!(provider.read(&j,111).state,"UNKNOWN");
 }
 #[test]fn registry_bridge_refuses_worker_database(){
  let(_,b,_,_,_,_,p)=crate::controller_worker::tests::fixture();let r=review(b,&p);let path=p.join("queue.sqlite");let before=std::fs::read(&path).unwrap();
  assert!(configure(Connection::open(&path).unwrap(),Arc::new(Exact(vec![hash(&serde_json::to_vec(&r).unwrap())]))).is_err());
  assert_eq!(before,std::fs::read(&path).unwrap());
 }

}
