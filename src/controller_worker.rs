//! Read-only evidence projection. Trusted host supplies pinned immutable leases;
//! never accept these bindings, paths or evidence from an HTTP request.
use crate::{controller_store::Job,controller_proof::{SelectedCut,Hash256},controller_receipt::read_native_receipt};
use rusqlite::{Connection,MAIN_DB};
use serde::Serialize;
use sha2::{Digest,Sha256};
use std::{fs::File,io::Read};
#[derive(Debug,Clone,Serialize)]
pub struct View {pub schema:&'static str,pub state:&'static str,pub reason:&'static str,
    pub actions_enabled:bool,pub retry_safe:bool,pub review_required:bool,pub observed_at:Option<i64>}
impl View {
    fn at(state:&'static str,reason:&'static str,t:Option<i64>)->Self{Self{schema:"controller-worker-evidence-v1",state,reason,actions_enabled:false,retry_safe:false,review_required:true,observed_at:t}}
    pub fn unknown()->Self{Self::at("UNKNOWN","worker_evidence_unavailable",None)}
    pub(crate) fn hold(reason:&'static str)->Self{Self::at("HELD",reason,None)}
}
pub trait Reader:Send+Sync {fn read(&self,job:&Job,now:i64)->View;}
pub struct Unavailable;
impl Reader for Unavailable {fn read(&self,_:&Job,_:i64)->View{View::unknown()}}
// No persistent review-to-cut registry exists in Controller v1. A trusted host
// must supply this independently reviewed mapping; do not infer it from filename.
#[derive(Clone,serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {pub job_id:String,pub job_revision:i64,pub source_binding:String,pub selection:SelectedCut}
pub struct Observation {pub selection:SelectedCut,pub observed_at:i64}
fn hex(h:Hash256)->String{h.hex()}
fn state(db:&Connection,source:&str,cut:&str)->Option<String>{
    let hold:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM queue_hold)",[],|r|r.get(0)).ok()?;
    if hold{return Some("hold".into());}
    let mut stmt=db.prepare("SELECT state FROM jobs WHERE clip_id=?1 AND video_hash=?2 AND kind='render'").ok()?;
    let mut rows=stmt.query([source,cut]).ok()?;let row=rows.next().ok()??;let s:String=row.get(0).ok()?;
    if rows.next().ok()?.is_some(){return None;}Some(s)
}
/// Caller retains immutable source/root/queue/sidecar/artifact leases and proves
/// observation selection/current source against the independently reviewed binding.
/// This function consumes no path and exposes no worker mutation capability.
/// Staleness bound is 30 seconds; future observations are rejected.
pub fn observe(job:&Job,expected:&Binding,observation:&Observation,db:&mut Connection,
    receipt:Option<File>,artifact:Option<File>,now:i64)->View {
    if expected.job_id!=job.id || expected.job_revision!=job.revision || expected.source_binding!=job.source_binding
        || expected.selection!=observation.selection || expected.source_binding.is_empty()
        || expected.selection.generation==0 || expected.selection.source_revision.is_empty()
        || expected.selection.start_ms>=expected.selection.end_ms {
        return View::hold("reviewed_binding_mismatch");
    }
    if observation.observed_at<0 || now.checked_sub(observation.observed_at).is_none_or(|age|!(0..=30).contains(&age)){
        return View::hold("stale_worker_evidence");
    }
    if !db.is_readonly(MAIN_DB).unwrap_or(false){return View::hold("worker_connection_not_readonly");}
    let source=hex(expected.selection.source_sha256);let cut=hex(expected.selection.durable_cut_key);
    let Some(before)=state(db,&source,&cut) else{return View::unknown()};
    let t=Some(observation.observed_at);
    match before.as_str(){
        "hold"=>View::at("HELD","worker_queue_hold",t),
        "claimed"=>View::at("PROCESSING","worker_claim_observed_not_retry_authority",t),
        "finished"=>{
            let (Some(receipt),Some(mut artifact))=(receipt,artifact) else{return View::hold("completion_proof_incomplete")};
            let Ok(proof)=read_native_receipt(db,receipt,&expected.selection,&source,&cut,16*1024*1024) else{return View::hold("receipt_unverified")};
            let Ok(meta)=artifact.metadata() else{return View::hold("artifact_unavailable")};
            if !meta.is_file() || meta.len()!=proof.artifact.bytes{return View::hold("artifact_mismatch");}
            let mut bytes=Vec::new();
            if artifact.by_ref().take(16*1024*1024+1).read_to_end(&mut bytes).is_err()
                || bytes.len() as u64!=proof.artifact.bytes || Hash256::parse(&format!("{:x}",Sha256::digest(&bytes)))!=Some(proof.artifact.sha256)
                || state(db,&source,&cut).as_deref()!=Some("finished") {return View::hold("artifact_or_queue_changed");}
            View::at("READY_FOR_REVIEW","verified_receipt_and_artifact",t)
        },
        // Do not guess undocumented queue states or interpret errors as retry-safe.
        _=>View::at("HELD","worker_state_unmapped",t),
    }
}
#[cfg(test)] pub(crate) mod tests {
 use super::*;use rusqlite::OpenFlags;
 pub(crate) fn fixture()->(Job,Binding,Observation,Connection,File,File,std::path::PathBuf){
    let p=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("bridge-test-data").join(uuid::Uuid::new_v4().to_string());std::fs::create_dir_all(&p).unwrap();
    let db=Connection::open(p.join("queue.sqlite")).unwrap();db.execute_batch("CREATE TABLE jobs(job_id TEXT,state TEXT,clip_id TEXT,video_hash TEXT,kind TEXT);CREATE TABLE manager(x);CREATE TABLE queue_hold(x);CREATE TABLE completion_proofs(job_id TEXT,receipt TEXT);").unwrap();
    let source="a".repeat(64);let cut="b".repeat(64);let bytes=b"synthetic artifact bytes, not playable video";let hash=format!("{:x}",Sha256::digest(bytes));
    let raw=serde_json::json!({"job_id":"one","clip_id":source,"revision":cut,"kind":"render","sha256":hash,"bytes":bytes.len(),"duration":"1.0","recipe":"recipe:cut=0-1000","publication_enabled":false}).to_string();
    db.execute("INSERT INTO jobs VALUES('one','finished',?1,?2,'render')",[&source,&cut]).unwrap();db.execute("INSERT INTO completion_proofs VALUES('one',?1)",[&raw]).unwrap();drop(db);
    std::fs::write(p.join("receipt"),raw).unwrap();std::fs::write(p.join("artifact"),bytes).unwrap();
    let job=Job{id:"controller-one".into(),source_id:1,source_ref:"source/original.mp4".into(),platform:"tiktok".into(),source_binding:"reviewed-root".into(),state:"PROCESSING".into(),review:"PENDING".into(),hold_reason:String::new(),retry_safe:false,revision:2,preview_revision:None,preview_sha256:None};
    let sel=SelectedCut{generation:1,provider:"synthetic".into(),provider_id:"1".into(),source_sha256:Hash256::parse(&source).unwrap(),source_revision:"revision1".into(),start_ms:0,end_ms:1000,recipe_version:"recipe".into(),durable_cut_key:Hash256::parse(&cut).unwrap()};
    let binding=Binding{job_id:job.id.clone(),job_revision:2,source_binding:job.source_binding.clone(),selection:sel.clone()};
    let obs=Observation{selection:sel,observed_at:100};
    (job,binding,obs,Connection::open_with_flags(p.join("queue.sqlite"),OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap(),File::open(p.join("receipt")).unwrap(),File::open(p.join("artifact")).unwrap(),p)
 }
 #[test]fn worker_bridge_verified_read_does_not_change_ledger(){let(j,b,o,mut db,r,a,p)=fixture();let before=std::fs::read(p.join("queue.sqlite")).unwrap();let v=observe(&j,&b,&o,&mut db,Some(r),Some(a),110);assert_eq!(v.state,"READY_FOR_REVIEW");assert!(!v.actions_enabled&&!v.retry_safe);assert_eq!(db.total_changes(),0);assert_eq!(before,std::fs::read(p.join("queue.sqlite")).unwrap());assert_eq!(j.state,"PROCESSING");}
 #[test]fn worker_bridge_rejects_stale_and_revision_mismatch(){let(j,b,mut o,mut db,_,_,_)=fixture();assert_eq!(observe(&j,&b,&o,&mut db,None,None,131).reason,"stale_worker_evidence");o.selection.source_revision="other".into();assert_eq!(observe(&j,&b,&o,&mut db,None,None,110).reason,"reviewed_binding_mismatch");}
 #[test]fn worker_bridge_rejects_missing_and_tampered_artifact(){let(j,b,o,mut db,r,_,p)=fixture();assert_eq!(observe(&j,&b,&o,&mut db,None,None,110).reason,"completion_proof_incomplete");std::fs::write(p.join("artifact"),b"bad").unwrap();assert_eq!(observe(&j,&b,&o,&mut db,Some(r),Some(File::open(p.join("artifact")).unwrap()),110).state,"HELD");}
 #[test]fn worker_bridge_maps_claim_and_hold_without_completion(){let(j,b,o,mut db,_,_,p)=fixture();let writer=Connection::open(p.join("queue.sqlite")).unwrap();writer.execute("UPDATE jobs SET state='claimed'",[]).unwrap();assert_eq!(observe(&j,&b,&o,&mut db,None,None,110).state,"PROCESSING");writer.execute("INSERT INTO queue_hold VALUES(1)",[]).unwrap();assert_eq!(observe(&j,&b,&o,&mut db,None,None,110).state,"HELD");assert_eq!(db.total_changes(),0);}
}
