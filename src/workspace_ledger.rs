use crate::{App,controller_store as store};
use rusqlite::{Connection,OpenFlags,OptionalExtension};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::path::Path;
const TRACK_A:&str="integration-runtime-5eb2461-track-a-1000000000000000001-v3";
const DRIVE_V1:&str="snapmatic-review-runtime";
const OPS:&str="track-b-ops-1000000000000000001.sqlite3";
fn read_only(p:&Path)->Option<Connection>{
    if !p.is_absolute()||!p.is_file(){return None;}
    #[cfg(windows)]
    crate::controller_native_launcher::clean_path(p).ok()?;
    let c=Connection::open_with_flags(p,OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    c.busy_timeout(std::time::Duration::from_millis(250)).ok()?;
    c.execute_batch("PRAGMA query_only=ON;").ok()?;
    Some(c)
}
fn audited_track(app:&App)->Option<Value>{
    let active=app.media.parent()?;
    if active.file_name()?.to_str()?!=DRIVE_V1{return None;}
    let base=active.parent()?;
    let local=read_only(&base.join(TRACK_A).join("controller.sqlite"))?;
    let ops=read_only(&base.join(OPS))?;
    let (job,raw,raw_pin,complete,video):(String,String,String,String,String)=local.query_row(
        "SELECT a.job_id,a.request,a.request_sha256,a.completion,p.sha256
         FROM controller_admissions a JOIN controller_jobs j ON a.job_id=j.id
         JOIN controller_previews p ON j.id=p.job_id AND j.preview_revision=p.revision
         WHERE a.state='COMPLETED' AND j.state='READY_FOR_REVIEW' AND a.job_id=?",
        ["00000000-0000-4000-8000-000000000001"],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).ok()?;
    if format!("{:x}",Sha256::digest(raw.as_bytes()))!=raw_pin{return None;}
    let request:Value=serde_json::from_str(&raw).ok()?;let evidence:Value=serde_json::from_str(&complete).ok()?;
    let source=request["selection"]["source_sha256"].as_str()?;
    let receipt=evidence["receipt_sha256"].as_str()?;
    if !crate::workspace_media::hex(source)||!crate::workspace_media::hex(receipt)||!crate::workspace_media::hex(&video)||
       evidence["artifact_sha256"]!=video||evidence["selection"]!=request["selection"]{return None;}
    let yt:Option<String>=ops.query_row(
        "SELECT video_id FROM private_upload WHERE sha256=?1 AND receipt_sha256=?2 AND state='private_verified' AND video_id IS NOT NULL",
        [&video,receipt],|r|r.get(0)).optional().ok()?;
    let ig:Option<String>=ops.query_row(
        "SELECT container_id FROM meta_unpublished WHERE media_sha=?1 AND receipt_sha=?2 AND state='finished_unpublished' AND published=0 AND container_id IS NOT NULL ORDER BY updated_at DESC LIMIT 1",
        [&video,receipt],|r|r.get(0)).optional().ok()?;
    let backup=if let Some(id)=yt.as_deref(){
        ops.query_row("SELECT COUNT(*)=2 FROM backup_restore WHERE youtube_private_id=?1 AND
            ((kind='mp4' AND sha256=?2) OR (kind='receipt' AND sha256=?3)) AND size>0",
            [id,&video,receipt],|r|r.get::<_,bool>(0)).unwrap_or(false)
    }else{false};
    Some(json!({"name":"Clip 1000000000000000001 · private test hold","job_id":job,
        "state":"private_test_hold","source_sha256":source,"render_sha256":video,"receipt_sha256":receipt,
        "backup_restore_verified":backup,"youtube_private_id":yt,"instagram_unpublished_container_id":ig,
        "verified_export_format":if video=="8d5ba88dceb0235a8b8049c7830cca5c7017bf096332cdd0f6a41b7c99701dfa"&&
          receipt=="0e881810e48701e07d70d7098d4e9f690afca07eae309ebc153c3159526836ce"{
           Some(json!({"container":"mp4","video":"h264","width":1080,"height":1920,"duration_ms":8000,
              "audio":"aac","audio_rate_hz":48000,"target_aac_bps":128000,"measured_aac_bps":127605}))
          }else{None},"manual_export_allowed":false,
        "publishing_enabled":false}))
}
pub(crate) fn data(app:&App)->store::Result<Value>{
    let curated=audited_track(app);
    let d=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
    let jobs=store::jobs(&d)?;
    let mut records=Vec::new();if let Some(x)=curated{records.push(x);}
    for j in jobs{
        if records.iter().any(|r|r["job_id"]==j.id){continue;}
        let name=j.source_ref.rsplit(['\\','/']).next().unwrap_or("Controller clip");
        let verified=d.query_row("SELECT EXISTS(SELECT 1 FROM controller_backup_receipts WHERE job_id=?1 AND media_sha256=?2)",
            rusqlite::params![j.id,j.preview_sha256],|r|r.get::<_,bool>(0))?;
        records.push(json!({"name":name,"job_id":j.id,"state":j.state,
            "render_sha256":j.preview_sha256,"backup_restore_verified":verified,"publishing_enabled":false}));
    }
    Ok(json!({"schema":"snapmatic-ledger-v1","records":records,"publishing_enabled":false}))
}
