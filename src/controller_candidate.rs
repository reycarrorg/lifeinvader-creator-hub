//! Durable proposals only: never claims, approves, creates plans or starts workers.
use crate::{App,controller_store as store,controller_manual::EditorSelection};
use sha2::{Digest,Sha256};
use rusqlite::{params,OptionalExtension};
pub fn save(app:&App,e:&EditorSelection)->store::Result<serde_json::Value>{
    if e.source_ref.len()>4096||e.start_ms>=e.end_ms||!(1..=60000).contains(&(e.end_ms-e.start_ms))||e.end_ms>604800000{return Err(store::Error("invalid_selection"))}
    let(a,path,source_sha,authorization_ref)=crate::controller_source::authorized_local_candidate(app,e.source_id,&e.source_ref).map_err(store::Error)?;
    #[cfg(windows)] let _source=crate::controller_native_process::lease(&path,&source_sha)?;
    #[cfg(not(windows))] {let _=path;return Err(store::Error("native_windows_required"));}
    let recipe=include_str!("../helpers/clip_worker/export_profile.py").lines().find_map(|s|s.strip_prefix("RECIPE='").and_then(|s|s.strip_suffix('\''))).ok_or(store::Error("recipe_unavailable"))?;
    let source_binding=crate::controller_source::controller_binding(&a);
    let full_recipe=format!("{recipe}:cut={}-{}",e.start_ms,e.end_ms);
    let cut_key=format!("{:x}",Sha256::digest(format!("{source_sha}{full_recipe}").as_bytes()));
    let candidate=serde_json::json!({"schema":"manual-clip-candidate-v1","source_id":e.source_id,"source_ref":e.source_ref,"source_binding":source_binding,"source_sha256":source_sha,"authorization_ref":authorization_ref,"start_ms":e.start_ms,"end_ms":e.end_ms,"recipe":recipe,"cut_key":cut_key});
    let raw=serde_json::to_string(&candidate).map_err(|_|store::Error("invalid_selection"))?;
    let id=format!("{:x}",Sha256::digest(raw.as_bytes()));
    let mut db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if !crate::controller_source::matches_admission(&tx,&a){return Err(store::Error("stale_revision"))}
    tx.execute_batch("CREATE TABLE IF NOT EXISTS manual_clip_candidates_v1(id TEXT PRIMARY KEY,candidate TEXT NOT NULL,state TEXT NOT NULL CHECK(state='AWAITING_INDEPENDENT_REVIEW')); ")?;
    let existing:Option<(String,String)>=tx.query_row("SELECT candidate,state FROM manual_clip_candidates_v1 WHERE id=?",[&id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let duplicate=existing.is_some();
    if let Some((prior,state))=existing {if prior!=raw||state!="AWAITING_INDEPENDENT_REVIEW"{return Err(store::Error("candidate_conflict"))}}
    else{tx.execute("INSERT INTO manual_clip_candidates_v1 VALUES(?,?,'AWAITING_INDEPENDENT_REVIEW')",params![id,raw])?;}
    tx.commit()?;
    Ok(serde_json::json!({"candidate_id":id,"candidate":candidate,"state":"AWAITING_INDEPENDENT_REVIEW","duplicate":duplicate,"render_authorized":false,"worker_started":false,"missing":["sealed_source_duration_and_speech_envelope","exact_six_cut_checks","independent_bound_rubric_and_attestations","pinned_per_job_runtime_plan","final_explicit_user_admission"]}))
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn reviewed_local_candidate_survives_reopen_without_authority(){
  let mut app=crate::controller::tests::app();
  {let db=app.db.lock().unwrap();db.execute("UPDATE profiles SET platform='local',url=? WHERE id=1",[crate::controller_source::LOCAL_SYNTHETIC_URL]).unwrap();}
  let a=crate::controller_source::controller_selection(&app,1,"sample-source/original.mp4").unwrap();
  let raw=serde_json::to_vec(&serde_json::json!({"schema":"authorized-source-catalog-v1","sources":[{"source_id":"1","revision":crate::controller_source::controller_binding(&a),"topic":"gta6","origin":"local-owned","rights_basis":"owned","authorization_ref":"inert-test-only","source_sha256":format!("{:x}",Sha256::digest(b"inert source"))}]})).unwrap();
  app.controller_catalog=Some(std::sync::Arc::new(crate::controller_source::Catalog::parse(&raw,&format!("{:x}",Sha256::digest(&raw))).unwrap()));
  let e=EditorSelection{source_id:1,source_ref:"sample-source/original.mp4".into(),start_ms:1000,end_ms:31000};
  let first=save(&app,&e).unwrap();assert_eq!(first["render_authorized"],false);assert_eq!(first["duplicate"],false);
  let path=app.media.join("candidate-reopen.sqlite");
  {let db=app.db.lock().unwrap();db.execute("VACUUM INTO ?",[path.to_str().unwrap()]).unwrap();}
  app.db=std::sync::Arc::new(std::sync::Mutex::new(rusqlite::Connection::open(path).unwrap()));
  let second=save(&app,&e).unwrap();assert_eq!(second["candidate_id"],first["candidate_id"]);assert_eq!(second["duplicate"],true);
  let changed=save(&app,&EditorSelection{start_ms:1100,..e.clone()}).unwrap();assert_ne!(changed["candidate_id"],first["candidate_id"]);assert_ne!(changed["candidate"]["cut_key"],first["candidate"]["cut_key"]);
  {let db=app.db.lock().unwrap();assert_eq!(db.query_row("SELECT count(*) FROM manual_clip_candidates_v1",[],|r|r.get::<_,i64>(0)).unwrap(),2);assert_eq!(db.query_row("SELECT count(*) FROM controller_admissions",[],|r|r.get::<_,i64>(0)).unwrap(),0);assert_eq!(db.query_row("SELECT count(*) FROM controller_admission_launches",[],|r|r.get::<_,i64>(0)).unwrap(),0);}
  std::fs::write(app.media.join("sample-source/original.mp4"),b"changed inert source").unwrap();assert!(save(&app,&e).is_err());
 }
 #[test]fn absent_catalog_never_persists_or_claims(){let app=crate::controller::tests::app();let e=EditorSelection{source_id:1,source_ref:"sample-source/original.mp4".into(),start_ms:1000,end_ms:31000};assert!(save(&app,&e).is_err());let db=app.db.lock().unwrap();assert_eq!(db.query_row("SELECT count(*) FROM sqlite_master WHERE name='manual_clip_candidates_v1'",[],|r|r.get::<_,i64>(0)).unwrap(),0);assert_eq!(db.query_row("SELECT count(*) FROM controller_admissions",[],|r|r.get::<_,i64>(0)).unwrap(),0);}
}
