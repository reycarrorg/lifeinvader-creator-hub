#![forbid(unsafe_code)]
//! Independent read-side checks for the native standalone queue schema.
//! No path resolution, launch, retry, cleanup or mutation is performed here.
//! The caller owns the authenticated configuration and safe Windows/Unix
//! opening of SQLite and worker receipt, and rechecks both at preview serve.

use crate::controller_proof::{ArtifactProof, CommittedReceipt, Hash256, SelectedCut};
use rusqlite::{Connection, MAIN_DB, TransactionBehavior};
use serde::Deserialize;
use std::{fs::File,io::{Read,Seek,SeekFrom},time::Duration};

const MAX_RECEIPT: u64=16_384;

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum NativeHold {
    UnsafeInput,
    QueueBusy,
    Unavailable,
    NoUniqueCommit,
    EvidenceMismatch,
}

#[derive(Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
struct WorkerReceipt {
    job_id:String, clip_id:String, revision:String, kind:String,
    sha256:String, bytes:u64, duration:String, recipe:String, publication_enabled:bool,
}

fn identifier(s:&str)->bool {
    !s.is_empty() && s.len()<=256 && s.bytes().all(|b| b.is_ascii_alphanumeric()
            || matches!(b,b'-'|b'_'|b'.'|b':'))
}

fn valid_duration(raw:&str,start_ms:u64,end_ms:u64)->bool {
    let Some(planned)=end_ms.checked_sub(start_ms) else {return false};
    let mut parts=raw.split('.');
    let first=parts.next().unwrap_or("");
    let second=parts.next();
    planned>0 && raw.len()<=32 && !first.is_empty() && first.bytes().all(|b|b.is_ascii_digit())
        && second.is_none_or(|s|!s.is_empty() && s.bytes().all(|b|b.is_ascii_digit()))
        && parts.next().is_none()
        && raw.parse::<f64>().is_ok_and(|x| x.is_finite() && x>0.0
            && (x - (planned as f64 / 1000.0)).abs() <= 0.15)
}

fn read_sidecar(mut checked_receipt:File)->Result<Vec<u8>,NativeHold> {
    let unavailable=|_| NativeHold::Unavailable;
    let meta=checked_receipt.metadata().map_err(unavailable)?;
    if !meta.is_file() || meta.len()==0 || meta.len()>MAX_RECEIPT {return Err(NativeHold::UnsafeInput)}
    let modified=meta.modified().map_err(unavailable)?;
    checked_receipt.seek(SeekFrom::Start(0)).map_err(unavailable)?;
    let mut data=Vec::with_capacity(meta.len() as usize);
    (&mut checked_receipt).take(MAX_RECEIPT+1).read_to_end(&mut data).map_err(unavailable)?;
    let after=checked_receipt.metadata().map_err(unavailable)?;
    if data.len() as u64!=meta.len() || after.len()!=meta.len() || after.modified().map_err(unavailable)?!=modified {
        return Err(NativeHold::EvidenceMismatch);
    }
    Ok(data)
}

pub fn read_native_receipt(
    connection:&mut Connection,
    checked_receipt:File,
    selection:&SelectedCut,
    canonical_source_sha256:&str,
    canonical_cut_key:&str,
    max_artifact_bytes:u64,
)->Result<CommittedReceipt,NativeHold> {
    let unavailable=|_| NativeHold::Unavailable;
    if !connection.is_readonly(MAIN_DB).map_err(unavailable)?
        || Hash256::parse(canonical_source_sha256)!=Some(selection.source_sha256)
        || Hash256::parse(canonical_cut_key)!=Some(selection.durable_cut_key)
        || selection.start_ms>=selection.end_ms
        || selection.generation==0
        || !crate::controller_proof::canonical_recipe_base(&selection.recipe_version)
    {return Err(NativeHold::UnsafeInput);}
    connection.busy_timeout(Duration::from_millis(500)).map_err(unavailable)?;
    connection.execute_batch("PRAGMA query_only=ON;").map_err(unavailable)?;
    let read=connection.transaction_with_behavior(TransactionBehavior::Deferred).map_err(unavailable)?;
    let blocked:i64=read.query_row("SELECT EXISTS(SELECT 1 FROM manager)
        OR EXISTS(SELECT 1 FROM queue_hold)
        OR EXISTS(SELECT 1 FROM jobs WHERE state='claimed')",[],|row|row.get(0)).map_err(unavailable)?;
    if blocked!=0 {return Err(NativeHold::QueueBusy);}
    let mut stmt=read.prepare("SELECT j.job_id,j.state,
        CASE WHEN length(CAST(c.receipt AS BLOB)) BETWEEN 1 AND 16384 THEN c.receipt END
        FROM jobs j LEFT JOIN completion_proofs c USING(job_id)
        WHERE j.clip_id=?1 AND j.video_hash=?2 AND j.kind='render'").map_err(unavailable)?;
    let mut rows=stmt.query([canonical_source_sha256,canonical_cut_key]).map_err(unavailable)?;
    let Some(row)=rows.next().map_err(unavailable)? else {return Err(NativeHold::NoUniqueCommit)};
    let job_id:String=row.get(0).map_err(unavailable)?;
    let state:String=row.get(1).map_err(unavailable)?;
    let raw_receipt:Option<String>=row.get(2).map_err(unavailable)?;
    if rows.next().map_err(unavailable)?.is_some() || state!="finished" || !identifier(&job_id) {
        return Err(NativeHold::NoUniqueCommit);
    }
    let raw_receipt=raw_receipt.ok_or(NativeHold::NoUniqueCommit)?;
    let sidecar=read_sidecar(checked_receipt)?;
    let db:WorkerReceipt=serde_json::from_str(&raw_receipt).map_err(|_|NativeHold::EvidenceMismatch)?;
    let file:WorkerReceipt=serde_json::from_slice(&sidecar).map_err(|_|NativeHold::EvidenceMismatch)?;
    let recipe=format!("{}:cut={}-{}",selection.recipe_version,selection.start_ms,selection.end_ms);
    if db!=file || db.job_id!=job_id || db.kind!="render" || db.clip_id!=canonical_source_sha256
        || db.revision!=canonical_cut_key || db.recipe!=recipe || db.publication_enabled
        || db.bytes==0 || db.bytes>max_artifact_bytes || !valid_duration(&db.duration,selection.start_ms,selection.end_ms)
    {return Err(NativeHold::EvidenceMismatch);}
    let artifact=ArtifactProof{
        sha256:Hash256::parse(&db.sha256).ok_or(NativeHold::EvidenceMismatch)?,
        bytes:db.bytes,
    };
    Ok(CommittedReceipt{selection:selection.clone(),artifact})
}
