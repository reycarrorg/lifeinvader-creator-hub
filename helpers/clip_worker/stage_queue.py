"""Single-owner execution seam. No CLI, subprocess, download or default producer.

The main app must explicitly admit an isolated staging workspace and a reviewed
plan; dry-run readiness alone is not an invocation of this function.
"""
import hashlib, json, os, re, sqlite3, uuid
from pathlib import Path
from contextlib import closing
from prepare import prepare, digest
from single_owner_queue import Queue, readonly_db_uri
from finalize import finish_artifact

def run_reviewed(config, workspace, *, producer, preclaim=None, worker_job_id=None,cancel=None):
    if cancel is not None:cancel()
    if worker_job_id is not None and (type(worker_job_id) is not str or not re.fullmatch(r'[A-Za-z0-9_.-]{1,256}',worker_job_id)):
        raise ValueError('invalid admitted worker identity')
    plan=prepare(config)
    root=Path(workspace)
    if not root.is_absolute() or not root.is_dir() or root.is_symlink():
        raise ValueError('explicit existing isolated workspace required')
    editorial=plan.get('editorial_review')
    if editorial:
        from rubric_gate import OWNER_JOB,OWNER_WORKSPACE
        if worker_job_id!=OWNER_JOB or root!=OWNER_WORKSPACE or root.resolve()!=OWNER_WORKSPACE:raise ValueError('Owner decision workspace or job mismatch')
    key=plan['cut_key'];output=root/(key+'.mp4');receipt=root/(key+'.receipt.json');ledger=root/'queue.sqlite'
    if ledger.exists():
        with closing(sqlite3.connect(readonly_db_uri(ledger),uri=True)) as db:
            db.execute('PRAGMA query_only=ON');db.execute('BEGIN')
            if db.execute('SELECT 1 FROM manager').fetchone() or db.execute('SELECT 1 FROM queue_hold').fetchone() or db.execute("SELECT 1 FROM jobs WHERE state='claimed'").fetchone():
                raise RuntimeError('unresolved owner/claim HOLD')
            rows=db.execute('SELECT j.job_id,j.state,c.receipt FROM jobs j LEFT JOIN completion_proofs c USING(job_id) WHERE clip_id=? AND video_hash=? AND kind=?',(config['source_sha256'],key,'render')).fetchall()
            if rows:
                if len(rows)!=1 or rows[0][1]!='finished' or rows[0][2] is None:raise RuntimeError('unresolved duplicate HOLD')
                if worker_job_id is not None and rows[0][0]!=worker_job_id:raise RuntimeError('completion worker lineage HOLD')
                r=json.loads(receipt.read_bytes())
                if r.get('editorial_review')!=editorial or r!=json.loads(rows[0][2]) or r['job_id']!=rows[0][0] or r['caption_status']!=plan['caption_status'] or r['recipe']!=plan['recipe'] or r['publication_enabled'] is not False or (output.stat().st_size,digest(output))!=(r['bytes'],r['sha256']):raise RuntimeError('completion mismatch HOLD')
                poster=output.with_suffix('.webp')
                if r.get('poster')!=dict(sha256=digest(poster),bytes=poster.stat().st_size,width=540,height=960,format='webp'):raise RuntimeError('poster mismatch HOLD')
                return r
            # A fresh isolated admission cannot adopt another queued identity.
            # In particular Queue.claim selects the oldest pending row, not a
            # caller-specified job. Refuse foreign history before any mutation.
            if db.execute('SELECT 1 FROM jobs LIMIT 1').fetchone():
                raise RuntimeError('foreign queue lineage HOLD')
    if output.exists() or receipt.exists():raise RuntimeError('uncommitted output HOLD')
    if preclaim is not None:preclaim(plan,root)
    if cancel is not None:cancel()
    q=Queue(ledger);pid=os.getpid();owner=q.start(pid)
    # No auto-adoption or retry after a crash. Queue persists owner/claim/HOLD.
    admitted_job=worker_job_id if worker_job_id is not None else 'job-'+uuid.uuid4().hex
    q.enqueue(owner,admitted_job,config['source_sha256'],key,'render')
    claim=q.claim(owner,pid)
    expected=dict(job_id=admitted_job,clip_id=config['source_sha256'],video_hash=key,kind='render')
    if claim!=expected:raise RuntimeError('claim lineage mismatch HOLD')
    try:
        duration=producer(dict(plan,worker_job_id=claim['job_id']),output)
        if cancel is not None:cancel()
        proof=finish_artifact(q,owner,pid,claim,output,receipt,duration,plan['recipe'],plan['caption_status'],editorial)
    except Exception:
        # Ambiguous producer/completion leaves the durable claim for investigation.
        raise
    q.stop(owner)
    return proof
