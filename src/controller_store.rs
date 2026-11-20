//! Durable control/review state only. No worker, publisher, reservation or backup I/O.
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const PREVIEW_LIMIT: usize = 16 * 1024 * 1024;
pub const PLATFORMS: [&str; 3] = ["tiktok", "instagram", "youtube"];
#[derive(Debug, PartialEq, Eq)]
pub struct Error(pub &'static str);
impl From<rusqlite::Error> for Error { fn from(_: rusqlite::Error)->Self { Self("storage_unavailable") } }
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct Settings { pub paused:bool, pub autopilot:bool, pub revision:i64, pub review_required:bool }
#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct Job {
    pub id:String, pub source_id:i64, pub source_ref:String, pub platform:String,
    #[serde(skip_serializing)] pub source_binding:String,
    pub state:String, pub review:String, pub hold_reason:String, pub retry_safe:bool,
    pub revision:i64, pub preview_revision:Option<i64>, pub preview_sha256:Option<String>,
}
#[derive(Serialize,Debug)]
pub struct Quota {
    pub platform:&'static str, pub confirmed_posts:i64, pub limit:i64,
    pub remaining_before_reservations:i64,
}
#[derive(Serialize,Debug)]
pub struct QuotaSnapshot {
    pub schema:&'static str, pub as_of_unix:i64, pub window_start_exclusive:i64,
    pub reservation_authority:&'static str, pub grants_admission:bool,
    pub reservations_included:bool, pub platforms:Vec<Quota>,
}

pub fn init(db:&Connection)->Result<()> {
    let has_meta=db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='controller_meta')",[],|r|r.get::<_,bool>(0))?;
    if has_meta {
        let version=db.query_row("SELECT schema_version FROM controller_meta WHERE singleton=1",[],|r|r.get::<_,i64>(0))?;
        return if version==1 {crate::controller_admission::init(db)} else {Err(Error("controller_schema_unknown"))};
    }
    if db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name GLOB 'controller_*')",[],|r|r.get::<_,bool>(0))?{return Err(Error("controller_schema_unknown"));}
    let tx=db.unchecked_transaction()?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS controller_settings(
      singleton INTEGER PRIMARY KEY CHECK(singleton=1), paused INTEGER NOT NULL CHECK(paused IN(0,1)),
      autopilot INTEGER NOT NULL CHECK(autopilot IN(0,1)), revision INTEGER NOT NULL);
      INSERT OR IGNORE INTO controller_settings VALUES(1,1,0,0);
      CREATE TABLE IF NOT EXISTS controller_jobs(
      id TEXT PRIMARY KEY, source_id INTEGER NOT NULL, source_ref TEXT NOT NULL, source_binding TEXT NOT NULL,
      platform TEXT NOT NULL CHECK(platform IN('tiktok','instagram','youtube')),
      state TEXT NOT NULL CHECK(state IN('QUEUED','PROCESSING','READY_FOR_REVIEW','HELD','FAILED','POSTED','BACKED_UP')),
      review TEXT NOT NULL CHECK(review IN('PENDING','APPROVED','REJECTED')),
      hold_reason TEXT NOT NULL, retry_safe INTEGER NOT NULL CHECK(retry_safe IN(0,1)),
      revision INTEGER NOT NULL, preview_revision INTEGER, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
      CHECK(state NOT IN('READY_FOR_REVIEW','POSTED','BACKED_UP') OR preview_revision IS NOT NULL),
      CHECK(state NOT IN('POSTED','BACKED_UP') OR review='APPROVED'));
      CREATE UNIQUE INDEX IF NOT EXISTS controller_active_source ON controller_jobs(source_id,source_ref,platform)
       WHERE state NOT IN('POSTED','BACKED_UP');
      CREATE TABLE IF NOT EXISTS controller_previews(
       job_id TEXT NOT NULL, revision INTEGER NOT NULL, sha256 TEXT NOT NULL, bytes BLOB NOT NULL,
       PRIMARY KEY(job_id,revision), FOREIGN KEY(job_id) REFERENCES controller_jobs(id));
      CREATE TABLE IF NOT EXISTS controller_events(
       seq INTEGER PRIMARY KEY, job_id TEXT, revision INTEGER NOT NULL, action TEXT NOT NULL,
       state TEXT NOT NULL, reason TEXT NOT NULL, at INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS controller_post_receipts(
       platform TEXT NOT NULL, receipt_id TEXT NOT NULL, job_id TEXT NOT NULL UNIQUE,
       posted_at INTEGER NOT NULL, media_sha256 TEXT NOT NULL,
       PRIMARY KEY(platform,receipt_id), FOREIGN KEY(job_id) REFERENCES controller_jobs(id));
      CREATE INDEX IF NOT EXISTS controller_post_window ON controller_post_receipts(platform,posted_at);
      CREATE TABLE IF NOT EXISTS controller_backup_receipts(
       job_id TEXT PRIMARY KEY, receipt_id TEXT NOT NULL UNIQUE, media_sha256 TEXT NOT NULL,
       verified_at INTEGER NOT NULL, FOREIGN KEY(job_id) REFERENCES controller_jobs(id));")?;
    tx.execute_batch("CREATE TABLE controller_meta(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL); INSERT INTO controller_meta VALUES(1,1);")?;
    tx.commit()?;
    crate::controller_admission::init(db)
}
fn event(tx:&Transaction,id:Option<&str>,revision:i64,action:&str,state:&str,reason:&str,now:i64)->Result<()> {
    tx.execute("INSERT INTO controller_events(job_id,revision,action,state,reason,at) VALUES(?,?,?,?,?,?)",
        params![id,revision,action,state,reason,now])?;Ok(())
}
pub fn settings(db:&Connection)->Result<Settings>{
    Ok(db.query_row("SELECT paused,autopilot,revision FROM controller_settings WHERE singleton=1",[],|r|Ok(Settings{
        paused:r.get(0)?,autopilot:r.get(1)?,revision:r.get(2)?,review_required:true}))?)
}
pub fn configure(db:&mut Connection,revision:i64,action:&str,now:i64)->Result<Settings>{
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let old=settings(&tx)?;if old.revision!=revision{return Err(Error("stale_revision"));}
    let (paused,auto)=match action {
        "pause"=>(true,old.autopilot),"resume"=>(false,old.autopilot),
        "autopilot_on"=>(old.paused,true),"autopilot_off"=>(old.paused,false),
        _=>return Err(Error("invalid_action")),
    };
    tx.execute("UPDATE controller_settings SET paused=?,autopilot=?,revision=revision+1 WHERE singleton=1",params![paused,auto])?;
    event(&tx,None,revision+1,action,"SETTINGS","",now)?;tx.commit()?;settings(db)
}
pub fn recover(db:&mut Connection,now:i64)->Result<()> {
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute("UPDATE controller_admissions SET state='HELD' WHERE state='RESERVED'",[])?;
    let pending={let mut q=tx.prepare("SELECT id,revision FROM controller_jobs WHERE state='PROCESSING'")?;
        q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?.collect::<std::result::Result<Vec<_>,_>>()?};
    for (id,revision) in pending {
        tx.execute("UPDATE controller_jobs SET state='HELD',hold_reason='interrupted_processing',retry_safe=0,revision=revision+1,updated_at=? WHERE id=?",params![now,id])?;
        event(&tx,Some(&id),revision+1,"recover","HELD","interrupted_processing",now)?;
    }
    let old=settings(&tx)?;
    if !old.paused || old.autopilot {
        tx.execute("UPDATE controller_settings SET paused=1,autopilot=0,revision=revision+1 WHERE singleton=1",[])?;
        event(&tx,None,old.revision+1,"recover","SETTINGS","restart_paused",now)?;
    }
    tx.commit()?;Ok(())
}
const SELECT_JOB:&str="SELECT j.id,j.source_id,j.source_ref,j.platform,j.state,j.review,j.hold_reason,j.retry_safe,j.revision,j.preview_revision,p.sha256,j.source_binding FROM controller_jobs j LEFT JOIN controller_previews p ON p.job_id=j.id AND p.revision=j.preview_revision";
fn row(r:&rusqlite::Row<'_>)->rusqlite::Result<Job>{Ok(Job{id:r.get(0)?,source_id:r.get(1)?,source_ref:r.get(2)?,platform:r.get(3)?,state:r.get(4)?,review:r.get(5)?,hold_reason:r.get(6)?,retry_safe:r.get(7)?,revision:r.get(8)?,preview_revision:r.get(9)?,preview_sha256:r.get(10)?,source_binding:r.get(11)?})}
pub fn job(db:&Connection,id:&str)->Result<Job>{db.query_row(&format!("{SELECT_JOB} WHERE j.id=?"),[id],row).optional()?.ok_or(Error("not_found"))}
pub fn jobs(db:&Connection)->Result<Vec<Job>>{let mut q=db.prepare(&format!("{SELECT_JOB} ORDER BY j.created_at DESC,j.id LIMIT 500"))?;Ok(q.query_map([],row)?.collect::<std::result::Result<Vec<_>,_>>()?)}
pub fn enqueue(db:&mut Connection,source_id:i64,reference:&str,platform:&str,binding:&str,now:i64)->Result<Job>{
    if binding.is_empty() || source_id<=0 || reference.is_empty() || reference.len()>4096 || !PLATFORMS.contains(&platform){return Err(Error("invalid_selection"));}
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if tx.query_row("SELECT EXISTS(SELECT 1 FROM controller_jobs WHERE source_id=? AND source_ref=? AND platform=? AND state NOT IN('POSTED','BACKED_UP'))",params![source_id,reference,platform],|r|r.get::<_,bool>(0))?{return Err(Error("already_queued"));}
    let id=uuid::Uuid::new_v4().to_string();
    tx.execute("INSERT INTO controller_jobs(id,source_id,source_ref,source_binding,platform,state,review,hold_reason,retry_safe,revision,created_at,updated_at) VALUES(?,?,?,?,?,'QUEUED','PENDING','',0,0,?,?)",params![id,source_id,reference,binding,platform,now,now])?;
    event(&tx,Some(&id),0,"enqueue","QUEUED","",now)?;tx.commit()?;job(db,&id)
}
fn expected(db:&Connection,id:&str,revision:i64)->Result<Job>{let j=job(db,id)?;if j.revision!=revision{return Err(Error("stale_revision"));}Ok(j)}
pub fn review(db:&mut Connection,id:&str,revision:i64,action:&str,now:i64)->Result<Job>{
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;let j=expected(&tx,id,revision)?;
    match action {
        "approve"|"reject"=>{
            if j.state!="READY_FOR_REVIEW" || j.review!="PENDING" {return Err(Error("review_unavailable"));}
            preview(&tx,id)?; // Approval binds to this exact verified snapshot/revision.
            let (state,review,reason,safe)=if action=="approve" {("READY_FOR_REVIEW","APPROVED","",false)}else{("HELD","REJECTED","review_rejected",true)};
            tx.execute("UPDATE controller_jobs SET state=?,review=?,hold_reason=?,retry_safe=?,revision=revision+1,updated_at=? WHERE id=?",params![state,review,reason,safe,now,id])?;
            event(&tx,Some(id),revision+1,action,state,reason,now)?;
        },
        "resume"=>{
            if !matches!(j.state.as_str(),"HELD"|"FAILED") || !j.retry_safe{return Err(Error("reconciliation_required"));}
            tx.execute("UPDATE controller_jobs SET state='QUEUED',review='PENDING',hold_reason='',retry_safe=0,preview_revision=NULL,revision=revision+1,updated_at=? WHERE id=?",params![now,id])?;
            event(&tx,Some(id),revision+1,action,"QUEUED","",now)?;
        },
        _=>return Err(Error("invalid_action")),
    }
    tx.commit()?;job(db,id)
}
// Adapter-only seams below. No HTTP endpoint accepts worker/publication claims.
// The future worker must validate provenance and hold an idempotent job lease
// before claim; verification/rights and OPS reservation authority are not supplied here.
pub fn claim(db:&mut Connection,id:&str,revision:i64,now:i64)->Result<Job>{
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if settings(&tx)?.paused{return Err(Error("controller_paused"));}
    let j=expected(&tx,id,revision)?;if j.state!="QUEUED"{return Err(Error("invalid_transition"));}
    tx.execute("UPDATE controller_jobs SET state='PROCESSING',revision=revision+1,updated_at=? WHERE id=?",params![now,id])?;
    event(&tx,Some(id),revision+1,"claim","PROCESSING","",now)?;tx.commit()?;job(db,id)
}
pub fn ready(db:&mut Connection,id:&str,revision:i64,verified_bytes:&[u8],verified_sha256:&str,now:i64)->Result<Job>{
    if verified_bytes.is_empty() || verified_bytes.len()>PREVIEW_LIMIT || format!("{:x}",Sha256::digest(verified_bytes))!=verified_sha256{return Err(Error("invalid_preview"));}
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;let j=expected(&tx,id,revision)?;
    if j.state!="PROCESSING"{return Err(Error("invalid_transition"));}
    tx.execute("INSERT INTO controller_previews VALUES(?,?,?,?)",params![id,revision+1,verified_sha256,verified_bytes])?;
    tx.execute("UPDATE controller_jobs SET state='READY_FOR_REVIEW',review='PENDING',hold_reason='',preview_revision=?,revision=revision+1,updated_at=? WHERE id=?",params![revision+1,now,id])?;
    event(&tx,Some(id),revision+1,"verified_preview","READY_FOR_REVIEW","review_required",now)?;tx.commit()?;job(db,id)
}
pub fn failed(db:&mut Connection,id:&str,revision:i64,retry_safe:bool,now:i64)->Result<Job>{
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;let j=expected(&tx,id,revision)?;
    if j.state!="PROCESSING"{return Err(Error("invalid_transition"));}
    tx.execute("UPDATE controller_jobs SET state='FAILED',hold_reason='worker_failed',retry_safe=?,revision=revision+1,updated_at=? WHERE id=?",params![retry_safe,now,id])?;
    event(&tx,Some(id),revision+1,"worker_failed","FAILED","worker_failed",now)?;tx.commit()?;job(db,id)
}
// Read-only: no touch timestamp, events, child processes, submit or retry.
pub fn preview(db:&Connection,id:&str)->Result<Vec<u8>>{
    let j=job(db,id)?;
    if !matches!(j.state.as_str(),"READY_FOR_REVIEW"|"POSTED"|"BACKED_UP"){return Err(Error("preview_unavailable"));}
    let bytes:Vec<u8>=db.query_row("SELECT bytes FROM controller_previews WHERE job_id=? AND revision=?",params![id,j.preview_revision],|r|r.get(0))?;
    if bytes.is_empty() || bytes.len()>PREVIEW_LIMIT || Some(format!("{:x}",Sha256::digest(&bytes)))!=j.preview_sha256{return Err(Error("preview_integrity_failed"));}
    Ok(bytes)
}
pub fn quota(db:&Connection,now:i64)->Result<QuotaSnapshot>{
    if now<86400{return Err(Error("invalid_clock"));}
    let mut platforms=Vec::new();
    for platform in PLATFORMS {
        let used=db.query_row("SELECT COUNT(*) FROM controller_post_receipts WHERE platform=? AND posted_at>? AND posted_at<=?",params![platform,now-86400,now],|r|r.get::<_,i64>(0))?;
        platforms.push(Quota{platform,confirmed_posts:used,limit:15,remaining_before_reservations:(15-used).max(0)});
    }
    Ok(QuotaSnapshot{schema:"controller-quota-read-v1",as_of_unix:now,window_start_exclusive:now-86400,reservation_authority:"OPS",grants_admission:false,reservations_included:false,platforms})
}
// Record independently verified historical facts, not authorization to post.
// A post already made must still be counted if Pause/quota changed in flight.
pub fn posted(db:&mut Connection,id:&str,revision:i64,receipt:&str,posted_at:i64,now:i64)->Result<Job>{
    if receipt.is_empty() || receipt.len()>256 || posted_at<0 || posted_at>now{return Err(Error("invalid_receipt"));}
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;let j=expected(&tx,id,revision)?;
    if j.state!="READY_FOR_REVIEW" || j.review!="APPROVED" {return Err(Error("review_required"));}
    preview(&tx,id)?;
    tx.execute("INSERT INTO controller_post_receipts VALUES(?,?,?,?,?)",params![j.platform,receipt,id,posted_at,j.preview_sha256])?;
    tx.execute("UPDATE controller_jobs SET state='POSTED',revision=revision+1,updated_at=? WHERE id=?",params![now,id])?;
    event(&tx,Some(id),revision+1,"verified_post_receipt","POSTED","",now)?;tx.commit()?;job(db,id)
}
pub fn backed_up(db:&mut Connection,id:&str,revision:i64,receipt:&str,restored_sha256:&str,now:i64)->Result<Job>{
    if receipt.is_empty() || receipt.len()>256{return Err(Error("invalid_receipt"));}
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;let j=expected(&tx,id,revision)?;
    if j.state!="POSTED" || j.preview_sha256.as_deref()!=Some(restored_sha256){return Err(Error("backup_unverified"));}
    tx.execute("INSERT INTO controller_backup_receipts VALUES(?,?,?,?)",params![id,receipt,restored_sha256,now])?;
    tx.execute("UPDATE controller_jobs SET state='BACKED_UP',revision=revision+1,updated_at=? WHERE id=?",params![now,id])?;
    event(&tx,Some(id),revision+1,"verified_restore_receipt","BACKED_UP","",now)?;tx.commit()?;job(db,id)
}

#[cfg(test)]
mod tests {
    use super::*;
    const T:i64=2_000_000;
    fn db()->Connection{let d=Connection::open_in_memory().unwrap();init(&d).unwrap();d}
    fn queue(d:&mut Connection,n:i64)->Job{enqueue(d,n,&format!("source/{n}.mp4"),"tiktok","test-binding",T).unwrap()}
    fn prepared(d:&mut Connection,n:i64)->Job{
        if settings(d).unwrap().paused{let r=settings(d).unwrap().revision;configure(d,r,"resume",T).unwrap();}
        let j=queue(d,n);let j=claim(d,&j.id,j.revision,T).unwrap();
        ready(d,&j.id,j.revision,b"synthetic preview",&format!("{:x}",Sha256::digest(b"synthetic preview")),T).unwrap()
    }
    #[test] fn controller_review_gate_and_complete_state_path(){
        let mut d=db();assert_eq!(settings(&d).unwrap(),Settings{paused:true,autopilot:false,revision:0,review_required:true});
        let j=queue(&mut d,1);assert_eq!(claim(&mut d,&j.id,0,T).unwrap_err().0,"controller_paused");
        configure(&mut d,0,"resume",T).unwrap();let j=claim(&mut d,&j.id,0,T).unwrap();
        assert!(review(&mut d,&j.id,j.revision,"approve",T).is_err());
        let j=ready(&mut d,&j.id,j.revision,b"preview",&format!("{:x}",Sha256::digest(b"preview")),T).unwrap();
        assert!(posted(&mut d,&j.id,j.revision,"receipt",T,T).is_err());
        let old=j.revision;let j=review(&mut d,&j.id,j.revision,"approve",T).unwrap();assert!(review(&mut d,&j.id,old,"reject",T).is_err());
        let j=posted(&mut d,&j.id,j.revision,"receipt",T,T).unwrap();assert_eq!(j.state,"POSTED");
        assert!(backed_up(&mut d,&j.id,j.revision,"backup","wrong",T).is_err());
        let j=backed_up(&mut d,&j.id,j.revision,"backup",j.preview_sha256.as_deref().unwrap(),T).unwrap();assert_eq!(j.state,"BACKED_UP");
    }
    #[test] fn controller_reject_resume_invalidates_preview_and_approval(){
        let mut d=db();let j=prepared(&mut d,1);let j=review(&mut d,&j.id,j.revision,"reject",T).unwrap();
        assert_eq!(j.state,"HELD");assert_eq!(j.hold_reason,"review_rejected");assert!(preview(&d,&j.id).is_err());
        let j=review(&mut d,&j.id,j.revision,"resume",T).unwrap();assert_eq!(j.review,"PENDING");assert!(j.preview_revision.is_none());
        assert!(posted(&mut d,&j.id,j.revision,"forged",T,T).is_err());
        assert_eq!(d.query_row("SELECT COUNT(*) FROM controller_previews",[],|r|r.get::<_,i64>(0)).unwrap(),1); // retained history
    }
    #[test] fn controller_restart_durable_hold_never_retries_ambiguous_work(){
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("controller-test-data").join(uuid::Uuid::new_v4().to_string());std::fs::create_dir_all(&path).unwrap();let file=path.join("controller.sqlite");
        let id;
        {let mut d=Connection::open(&file).unwrap();init(&d).unwrap();configure(&mut d,0,"resume",T).unwrap();configure(&mut d,1,"autopilot_on",T).unwrap();let j=queue(&mut d,1);id=j.id;claim(&mut d,&id,0,T).unwrap();}
        let mut d=Connection::open(file).unwrap();init(&d).unwrap();recover(&mut d,T+1).unwrap();let j=job(&d,&id).unwrap();
        assert_eq!(j.state,"HELD");assert_eq!(j.hold_reason,"interrupted_processing");assert!(!j.retry_safe);assert!(settings(&d).unwrap().paused);assert!(!settings(&d).unwrap().autopilot);
        assert!(review(&mut d,&id,j.revision,"resume",T+2).is_err());assert!(ready(&mut d,&id,1,b"late",&format!("{:x}",Sha256::digest(b"late")),T+2).is_err());
        let count=d.query_row("SELECT COUNT(*) FROM controller_events",[],|r|r.get::<_,i64>(0)).unwrap();recover(&mut d,T+2).unwrap();assert_eq!(count,d.query_row("SELECT COUNT(*) FROM controller_events",[],|r|r.get::<_,i64>(0)).unwrap());
    }
    #[test] fn controller_failed_resume_requires_positive_reconciliation(){
        let mut d=db();configure(&mut d,0,"resume",T).unwrap();let j=queue(&mut d,1);let j=claim(&mut d,&j.id,0,T).unwrap();let j=failed(&mut d,&j.id,j.revision,false,T).unwrap();assert_eq!(j.state,"FAILED");assert!(review(&mut d,&j.id,j.revision,"resume",T).is_err());
        let j=queue(&mut d,2);let j=claim(&mut d,&j.id,0,T).unwrap();let j=failed(&mut d,&j.id,j.revision,true,T).unwrap();assert_eq!(review(&mut d,&j.id,j.revision,"resume",T).unwrap().state,"QUEUED");
    }
    #[test] fn controller_quota_rolling_window_and_no_reservations(){
        let mut d=db();
        for n in 1..=17 {let j=prepared(&mut d,n);let j=review(&mut d,&j.id,j.revision,"approve",T).unwrap();let at=if n==1{T-86400}else if n==2{T-86399}else{T};posted(&mut d,&j.id,j.revision,&format!("post-{n}"),at,T).unwrap();}
        let q=quota(&d,T).unwrap();assert_eq!(q.platforms[0].confirmed_posts,16);assert_eq!(q.platforms[0].remaining_before_reservations,0);assert_eq!(q.platforms[1].confirmed_posts,0);assert!(!q.grants_admission);assert!(!q.reservations_included);
        assert_eq!(quota(&d,T+1).unwrap().platforms[0].confirmed_posts,15);
        assert_eq!(quota(&d,T+86400).unwrap().platforms[0].confirmed_posts,0);
    }
    #[test] fn controller_preview_readonly_and_tamper_fails_closed(){
        let mut d=db();let j=prepared(&mut d,1);let before=d.query_row("SELECT total_changes()",[],|r|r.get::<_,i64>(0)).unwrap();d.execute_batch("PRAGMA query_only=ON").unwrap();
        for _ in 0..5{assert_eq!(preview(&d,&j.id).unwrap(),b"synthetic preview");quota(&d,T).unwrap();}
        assert_eq!(before,d.query_row("SELECT total_changes()",[],|r|r.get::<_,i64>(0)).unwrap());
        d.execute_batch("PRAGMA query_only=OFF").unwrap();d.execute("UPDATE controller_previews SET bytes=x'00'",[]).unwrap();assert_eq!(preview(&d,&j.id).unwrap_err().0,"preview_integrity_failed");
    }
    #[test] fn controller_duplicate_claim_and_duplicate_enqueue_are_closed(){
        let mut d=db();configure(&mut d,0,"resume",T).unwrap();let j=queue(&mut d,1);
        assert_eq!(enqueue(&mut d,1,"source/1.mp4","tiktok","test-binding",T).unwrap_err().0,"already_queued");
        claim(&mut d,&j.id,0,T).unwrap();assert_eq!(claim(&mut d,&j.id,0,T).unwrap_err().0,"stale_revision");
        configure(&mut d,1,"pause",T).unwrap();let second=queue(&mut d,2);assert_eq!(claim(&mut d,&second.id,0,T).unwrap_err().0,"controller_paused");
    }
    #[test] fn controller_concurrent_claim_has_one_durable_winner(){
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("controller-test-data").join(uuid::Uuid::new_v4().to_string());std::fs::create_dir_all(&root).unwrap();let path=root.join("race.sqlite");
        let mut d=Connection::open(&path).unwrap();init(&d).unwrap();configure(&mut d,0,"resume",T).unwrap();let j=queue(&mut d,1);drop(d);
        let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));let mut workers=Vec::new();
        for _ in 0..2{let path=path.clone();let id=j.id.clone();let barrier=barrier.clone();workers.push(std::thread::spawn(move||{let mut db=Connection::open(path).unwrap();db.busy_timeout(std::time::Duration::from_secs(2)).unwrap();barrier.wait();claim(&mut db,&id,0,T)}));}
        let results=workers.into_iter().map(|w|w.join().unwrap()).collect::<Vec<_>>();assert_eq!(results.iter().filter(|r|r.is_ok()).count(),1);
        let db=Connection::open(path).unwrap();assert_eq!(job(&db,&j.id).unwrap().revision,1);assert_eq!(db.query_row("SELECT COUNT(*) FROM controller_events WHERE action='claim'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    }

    #[test] fn controller_schema_version_is_idempotent_and_unknown_versions_hold(){
        let d=db();init(&d).unwrap();d.execute("UPDATE controller_meta SET schema_version=2",[]).unwrap();assert_eq!(init(&d).unwrap_err().0,"controller_schema_unknown");
        let d=Connection::open_in_memory().unwrap();d.execute("CREATE TABLE controller_partial(x)",[]).unwrap();assert_eq!(init(&d).unwrap_err().0,"controller_schema_unknown");assert!(!d.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='controller_jobs')",[],|r|r.get::<_,bool>(0)).unwrap());
    }

}
