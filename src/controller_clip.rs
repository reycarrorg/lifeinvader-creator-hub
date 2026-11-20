//! Main Controller dry-run preparation. No queue claim or render authority.
use crate::{App,controller_store as store};
use serde::Deserialize;
use sha2::{Digest,Sha256};
use std::{io::{Read,Write},path::PathBuf,process::{Command,Stdio},time::{Duration,Instant}};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    job_id:String, revision:i64, source_binding:String,
    python:PathBuf, helper:PathBuf, preparation:serde_json::Value,
    #[serde(default)] completion:Option<Completion>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Completion {
    queue:PathBuf, receipt:PathBuf, artifact:PathBuf,
    receipt_sha256:String, artifact_sha256:String, artifact_bytes:u64,
    selection:crate::controller_proof::SelectedCut,
}

fn config()->store::Result<Config> {
    let path=std::env::var_os("LIFEINVADER_CLIP_CONFIG").ok_or(store::Error("worker_configuration_unavailable"))?;
    let expected=std::env::var("LIFEINVADER_CLIP_CONFIG_SHA256").map_err(|_|store::Error("worker_configuration_unavailable"))?;
    let mut raw=Vec::new();std::fs::File::open(path).map_err(|_|store::Error("worker_configuration_unavailable"))?
        .take(16385).read_to_end(&mut raw).map_err(|_|store::Error("worker_configuration_unavailable"))?;
    if raw.len()>16384 || format!("{:x}",Sha256::digest(&raw))!=expected {return Err(store::Error("worker_configuration_mismatch"));}
    serde_json::from_slice(&raw).map_err(|_|store::Error("worker_configuration_mismatch"))
}
pub fn prepare(app:&App,id:&str,revision:i64)->store::Result<serde_json::Value> {
    let config=config()?;
    let current=||->store::Result<store::Job>{
        let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
        let j=store::job(&db,id)?;
        if j.revision!=revision || j.id!=config.job_id || config.revision!=revision || j.source_binding!=config.source_binding
            || !crate::controller_source::controller_current(&db,j.source_id,&j.source_binding) {return Err(store::Error("stale_revision"));}
        if j.state!="QUEUED" {return Err(store::Error("invalid_transition"));} Ok(j)
    };
    let before=current()?;
    let source=crate::controller_source::controller_file(app,before.source_id,&before.source_ref).map_err(store::Error)?;
    let configured=config.preparation.get("source").and_then(|s|s.as_str()).ok_or(store::Error("invalid_selection"))?;
    if std::fs::canonicalize(configured).ok()!=Some(source.canonicalize().map_err(|_|store::Error("source_unavailable"))?)
        || !config.python.is_absolute() || !config.helper.is_absolute() {return Err(store::Error("invalid_selection"));}
    let input=serde_json::to_vec(&config.preparation).map_err(|_|store::Error("invalid_selection"))?;
    let mut command=Command::new(config.python);
    command.arg(config.helper).arg("--dry-run").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .env("PYTHONDONTWRITEBYTECODE","1").env("HF_HUB_OFFLINE","1").env("TRANSFORMERS_OFFLINE","1");
    #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
    let mut child=command.spawn().map_err(|_|store::Error("worker_configuration_unavailable"))?;
    let sent=child.stdin.take().ok_or(store::Error("worker_configuration_unavailable"))?.write_all(&input);
    if sent.is_err(){let _=child.kill();let _=child.wait();return Err(store::Error("worker_preparation_failed"));}
    let deadline=Instant::now()+Duration::from_secs(20);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success()=>break,
            Ok(Some(_))=>return Err(store::Error("worker_preparation_failed")),
            Err(_)=>{let _=child.kill();let _=child.wait();return Err(store::Error("worker_preparation_failed"));},
            _=>{}
        }
        if Instant::now()>=deadline {let _=child.kill();let _=child.wait();return Err(store::Error("worker_preparation_timeout"));}
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut output=Vec::new();child.stdout.take().ok_or(store::Error("worker_preparation_failed"))?.take(4097).read_to_end(&mut output).map_err(|_|store::Error("worker_preparation_failed"))?;
    if output.len()>4096 || current()?!=before {return Err(store::Error("stale_revision"));}
    let result:serde_json::Value=serde_json::from_slice(&output).map_err(|_|store::Error("worker_preparation_failed"))?;
    if result.get("render_authorized")!=Some(&serde_json::json!(false)) || result.get("worker_started")!=Some(&serde_json::json!(false)) || result.get("publish")!=Some(&serde_json::json!(false)) {return Err(store::Error("worker_preparation_failed"));}
    Ok(result)
}

fn checked_file(path:&std::path::Path,cap:u64)->store::Result<(std::fs::File,Vec<u8>)> {
    let err=|_|store::Error("completion_evidence_unavailable");
    let mut options=std::fs::OpenOptions::new();options.read(true);
    #[cfg(windows)] {use std::os::windows::fs::OpenOptionsExt;options.share_mode(1).custom_flags(0x00200000);}
    let mut file=options.open(path).map_err(err)?;let meta=file.metadata().map_err(err)?;
    #[cfg(windows)] {use std::os::windows::fs::MetadataExt;if meta.file_attributes()&0x400!=0{return Err(store::Error("completion_evidence_unavailable"));}}
    if !meta.is_file() || meta.len()==0 || meta.len()>cap{return Err(store::Error("completion_evidence_unavailable"));}
    let mut data=Vec::new();Read::by_ref(&mut file).take(cap+1).read_to_end(&mut data).map_err(err)?;
    if data.len() as u64!=meta.len(){return Err(store::Error("completion_evidence_changed"));}
    Ok((file,data))
}

pub fn import_completed(app:&App,id:&str,revision:i64,now:i64)->store::Result<store::Job> {
    let queued={let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;store::job(&db,id)?.state=="QUEUED"};
    if queued && prepare(app,id,revision)?.get("state").and_then(|v|v.as_str())!=Some("DRY_RUN_READY") {
        return Err(store::Error("reviewed_preparation_required"));
    }
    import_with(app,id,revision,now,&config()?)
}
fn import_with(app:&App,id:&str,revision:i64,now:i64,c:&Config)->store::Result<store::Job> {
    let e=c.completion.as_ref().ok_or(store::Error("completion_not_reviewed"))?;
    let before={let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;store::job(&db,id)?};
    if before.id!=c.job_id || before.source_binding!=c.source_binding || revision!=before.revision
        || !matches!(before.state.as_str(),"QUEUED"|"READY_FOR_REVIEW")
        || (before.state=="QUEUED" && revision!=c.revision) {return Err(store::Error("stale_revision"));}
    let source=crate::controller_source::controller_file(app,before.source_id,&before.source_ref).map_err(store::Error)?;
    // All evidence expectations come from independently pinned local config, not POST.
    let selection=crate::controller_proof::SelectedCut::from_authenticated_profile(e.selection.clone()).ok_or(store::Error("invalid_selection"))?;
    let recipe=format!("{}:cut={}-{}",selection.recipe_version,selection.start_ms,selection.end_ms);
    if format!("{:x}",Sha256::digest(format!("{}{}",selection.source_sha256.hex(),recipe).as_bytes()))!=selection.durable_cut_key.hex(){return Err(store::Error("invalid_selection"));}
    let mut source_options=std::fs::OpenOptions::new();source_options.read(true);
    #[cfg(windows)] {use std::os::windows::fs::OpenOptionsExt;source_options.share_mode(1).custom_flags(0x00200000);}
    let mut source_file=source_options.open(source).map_err(|_|store::Error("source_unavailable"))?;
    let mut hash=Sha256::new();let mut chunk=[0;65536];loop {let n=source_file.read(&mut chunk).map_err(|_|store::Error("source_unavailable"))?;if n==0{break;}hash.update(&chunk[..n]);}
    if format!("{:x}",hash.finalize())!=selection.source_sha256.hex(){return Err(store::Error("source_changed"));}
    let (receipt,receipt_bytes)=checked_file(&e.receipt,16384)?;
    let (_artifact,bytes)=checked_file(&e.artifact,store::PREVIEW_LIMIT as u64)?;
    if format!("{:x}",Sha256::digest(&receipt_bytes))!=e.receipt_sha256 || bytes.len() as u64!=e.artifact_bytes
        || format!("{:x}",Sha256::digest(&bytes))!=e.artifact_sha256 {return Err(store::Error("completion_evidence_mismatch"));}
    let mut queue=rusqlite::Connection::open_with_flags(&e.queue,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|_|store::Error("worker_queue_unavailable"))?;
    let proof=crate::controller_receipt::read_native_receipt(&mut queue,receipt,&selection,&selection.source_sha256.hex(),&selection.durable_cut_key.hex(),store::PREVIEW_LIMIT as u64).map_err(|_|store::Error("completion_receipt_unverified"))?;
    if proof.artifact.sha256.hex()!=e.artifact_sha256 || proof.artifact.bytes!=e.artifact_bytes{return Err(store::Error("completion_evidence_mismatch"));}
    let mut db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
    if store::job(&db,id)?!=before || !crate::controller_source::controller_current(&db,before.source_id,&before.source_binding){return Err(store::Error("stale_revision"));}
    if before.state=="READY_FOR_REVIEW" {
        if before.preview_sha256.as_deref()!=Some(e.artifact_sha256.as_str()){return Err(store::Error("completion_evidence_mismatch"));}
        return Ok(before); // no claim, write, event or second producer
    }
    let processing=store::claim(&mut db,id,revision,now)?;
    // Crash between transitions remains PROCESSING and requires reconciliation;
    // never invoke a producer/retry based on this observation.
    store::ready(&mut db,id,processing.revision,&bytes,&e.artifact_sha256,now)
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn staged_queue_completion_import_duplicate_and_restart() {
        let app=crate::controller::tests::app();
        std::fs::write(app.media.join("sample-source/original.mp4"),b"inert original source").unwrap();
        let admission=crate::controller_source::controller_selection(&app,1,"sample-source/original.mp4").unwrap();
        let binding=crate::controller_source::controller_binding(&admission);
        let j={let mut db=app.db.lock().unwrap();store::configure(&mut db,0,"resume",100).unwrap();store::enqueue(&mut db,1,"sample-source/original.mp4","tiktok",&binding,100).unwrap()};
        let output=Command::new("python").arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/clip_queue_fixture.py"))
            .env("PYTHONDONTWRITEBYTECODE","1").output().unwrap();
        assert!(output.status.success(),"synthetic queue fixture failed: {}",String::from_utf8_lossy(&output.stderr));
        let e:Completion=serde_json::from_slice(&output.stdout).unwrap();
        let ledger=std::fs::read(&e.queue).unwrap();
        let mut c=Config{job_id:j.id.clone(),revision:j.revision,source_binding:binding,python:PathBuf::new(),helper:PathBuf::new(),preparation:serde_json::json!({}),completion:Some(e)};
        // An independent expectation mismatch cannot claim the Controller job.
        let original=c.completion.as_ref().unwrap().artifact_sha256.clone();
        c.completion.as_mut().unwrap().artifact_sha256="0".repeat(64);
        assert!(import_with(&app,&j.id,j.revision,101,&c).is_err());
        assert_eq!(store::job(&app.db.lock().unwrap(),&j.id).unwrap().state,"QUEUED");
        c.completion.as_mut().unwrap().artifact_sha256=original;
        let ready=import_with(&app,&j.id,j.revision,102,&c).unwrap();
        assert_eq!(ready.state,"READY_FOR_REVIEW");assert_eq!(ready.review,"PENDING");
        let changes=app.db.lock().unwrap().total_changes();
        assert_eq!(import_with(&app,&j.id,ready.revision,103,&c).unwrap(),ready);
        assert_eq!(app.db.lock().unwrap().total_changes(),changes);
        assert_eq!(std::fs::read(&c.completion.as_ref().unwrap().queue).unwrap(),ledger);
        assert!(import_with(&app,&j.id,j.revision,104,&c).is_err());
        // Durable preview reads do not require a producer or runtime cache.
        assert_eq!(store::preview(&app.db.lock().unwrap(),&j.id).unwrap(),b"external synthetic completed bytes; not playable video");
        let restart_path=app.media.join("controller-restart.sqlite");
        app.db.lock().unwrap().execute("VACUUM INTO ?",[restart_path.to_str().unwrap()]).unwrap();
        let mut restarted=rusqlite::Connection::open(restart_path).unwrap();
        store::init(&restarted).unwrap();store::recover(&mut restarted,104).unwrap();
        assert_eq!(store::job(&restarted,&j.id).unwrap(),ready);
        assert_eq!(store::preview(&restarted,&j.id).unwrap(),b"external synthetic completed bytes; not playable video");
        assert!(store::settings(&restarted).unwrap().paused);
        app.db.lock().unwrap().execute("UPDATE profiles SET archived=1 WHERE id=1",[]).unwrap();
        assert!(import_with(&app,&j.id,ready.revision,105,&c).is_err());
    }
}
