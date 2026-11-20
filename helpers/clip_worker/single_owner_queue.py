"""Local coordination primitive for a clipping queue, with no job executor.

This module does not launch GPU workers, render clips, modify review records,
publish, or delete. It only coordinates callers that explicitly use this DB.
Already-running OpenShorts processes are outside its scope.
"""
from __future__ import annotations

import sqlite3
import json
import hashlib
import uuid
import os
import re
import stat
from contextlib import closing
from datetime import datetime, timezone
from pathlib import Path


def _now():
    return datetime.now(timezone.utc).isoformat()


def local_db_path(value):
    raw = os.fspath(value)
    if not isinstance(raw, str) or not raw or raw != raw.strip():
        raise ValueError('explicit database path required')
    for prefix in ('sqlite:///', 'sqlite://'):
        if raw.startswith(prefix):
            raw = raw[len(prefix):]
            break
    raw = raw.replace('/', '\\')
    if raw.startswith('\\\\?\\'):
        raw = raw[4:]
    if not re.fullmatch(r'[Cc]:\\[^:*?"<>|]+', raw):
        raise ValueError('explicit local C-drive database required')
    parts = raw[3:].split('\\')
    if any(not part or part in ('.','..') or part.endswith((' ','.')) or Path(part).is_reserved() for part in parts):
        raise ValueError('unstable database path')
    path = Path(raw)
    for item in (*reversed(path.parents),path):
        if os.path.lexists(item) and item.lstat().st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT:
            raise ValueError('reparse database path')
    if path.exists() and (not path.is_file() or path.stat().st_nlink!=1):
        raise ValueError('ambiguous database identity')
    return path


def readonly_db_uri(value):
    path = local_db_path(value)
    if not path.is_file():
        raise ValueError('existing database required')
    return path.as_uri() + '?mode=ro'


class Queue:
    def __init__(self, path):
        self.path = local_db_path(path)
        if self.path.name.lower() != 'queue.sqlite':
            raise ValueError('worker queue.sqlite required')
        if not self.path.exists():
            os.makedirs(self.path.parent, exist_ok=True)
        with closing(sqlite3.connect(self.path, timeout=10)) as db:
            db.executescript('''
                CREATE TABLE IF NOT EXISTS manager (
                    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                    token TEXT NOT NULL, pid INTEGER NOT NULL, started TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS jobs (
                    job_id TEXT PRIMARY KEY, clip_id TEXT NOT NULL,
                    video_hash TEXT NOT NULL, kind TEXT NOT NULL,
                    state TEXT NOT NULL CHECK(state IN ('pending','claimed','finished','failed')),
                    worker_pid INTEGER, manager_token TEXT,
                    UNIQUE(clip_id, video_hash, kind)
                );
                CREATE UNIQUE INDEX IF NOT EXISTS one_claim_per_clip
                    ON jobs(clip_id) WHERE state = 'claimed';
                CREATE TABLE IF NOT EXISTS completion_proofs(job_id TEXT PRIMARY KEY, receipt TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS queue_hold (
                    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                    failed_job_id TEXT NOT NULL
                );
                INSERT OR IGNORE INTO queue_hold(singleton, failed_job_id)
                    SELECT 1, job_id FROM jobs WHERE state='failed' ORDER BY rowid LIMIT 1;
                CREATE TABLE IF NOT EXISTS events (
                    id INTEGER PRIMARY KEY, time TEXT NOT NULL, event TEXT NOT NULL,
                    token TEXT, job_id TEXT, pid INTEGER
                );
            ''')

    def start(self, pid):
        if not isinstance(pid, int) or pid <= 0:
            raise ValueError('Positive process ID required')
        token = uuid.uuid4().hex
        with closing(sqlite3.connect(self.path, timeout=10)) as db:
            try:
                db.execute('BEGIN IMMEDIATE')
                db.execute('INSERT INTO manager VALUES (1,?,?,?)', (token,pid,_now()))
                db.execute('INSERT INTO events(time,event,token,pid) VALUES (?,?,?,?)',
                           (_now(),'manager_started',token,pid))
                db.commit()
            except sqlite3.IntegrityError as exc:
                db.rollback()
                raise RuntimeError('A manager is already registered; stale state requires investigation') from exc
        return token

    @staticmethod
    def _require_owner(db, token):
        if not db.execute('SELECT 1 FROM manager WHERE singleton=1 AND token=?', (token,)).fetchone():
            raise RuntimeError('Not the current queue manager')

    def enqueue(self, token, job_id, clip_id, video_hash, kind):
        if not all(isinstance(x,str) and x for x in (job_id,clip_id,video_hash,kind)):
            raise ValueError('All job identifiers are required')
        with closing(sqlite3.connect(self.path, timeout=10)) as db:
            db.execute('BEGIN IMMEDIATE')
            self._require_owner(db, token)
            try:
                db.execute("INSERT INTO jobs(job_id,clip_id,video_hash,kind,state) VALUES (?,?,?,?,'pending')",
                           (job_id,clip_id,video_hash,kind))
            except sqlite3.IntegrityError as exc:
                db.rollback()
                raise RuntimeError('Duplicate job or clip revision and kind') from exc
            db.execute('INSERT INTO events(time,event,token,job_id) VALUES (?,?,?,?)',
                       (_now(),'enqueued',token,job_id))
            db.commit()

    def claim(self, token, worker_pid):
        if not isinstance(worker_pid,int) or worker_pid <= 0:
            raise ValueError('Positive worker PID required')
        with closing(sqlite3.connect(self.path, timeout=10)) as db:
            db.execute('BEGIN IMMEDIATE')
            self._require_owner(db, token)
            if db.execute('SELECT 1 FROM queue_hold WHERE singleton=1').fetchone():
                db.rollback()
                raise RuntimeError('Queue on hold after job failure; investigation required')
            row = db.execute("""SELECT job_id,clip_id,video_hash,kind FROM jobs j
                WHERE state='pending' AND NOT EXISTS
                (SELECT 1 FROM jobs c WHERE c.state='claimed' AND c.clip_id=j.clip_id)
                ORDER BY rowid LIMIT 1""").fetchone()
            if row is None:
                db.commit()
                return None
            db.execute("UPDATE jobs SET state='claimed',worker_pid=?,manager_token=? WHERE job_id=? AND state='pending'",
                       (worker_pid,token,row[0]))
            db.execute('INSERT INTO events(time,event,token,job_id,pid) VALUES (?,?,?,?,?)',
                       (_now(),'claimed',token,row[0],worker_pid))
            db.commit()
        return dict(zip(('job_id','clip_id','video_hash','kind'),row))

    def finish(self, token, job_id, worker_pid, *, success, receipt_path=None, artifact_path=None):
        with closing(sqlite3.connect(self.path, timeout=10)) as db:
            db.execute('BEGIN IMMEDIATE')
            self._require_owner(db, token)
            rejected = False
            if success:
                row = db.execute("SELECT clip_id,video_hash,kind FROM jobs WHERE job_id=? AND state='claimed' AND manager_token=? AND worker_pid=?", (job_id,token,worker_pid)).fetchone()
                if row is None:
                    raise RuntimeError('Job is not claimed by this manager and worker')
                try:
                    r = json.loads(Path(receipt_path).read_text(encoding='utf-8'))
                    if not isinstance(r,dict) or (r.get('job_id'),r.get('clip_id'),r.get('revision'),r.get('kind')) != (job_id,*row):
                        raise ValueError('Receipt identity mismatch')
                    if r.get('caption_status') not in ('captioned','no_speech'):raise ValueError('Receipt caption status')
                    size=0
                    digest=hashlib.sha256()
                    with Path(artifact_path).open('rb') as stream:
                        for chunk in iter(lambda:stream.read(1048576),b''):
                            size+=len(chunk)
                            digest.update(chunk)
                    if not size or type(r.get('bytes')) is not int or (r.get('bytes'),r.get('sha256')) != (size,digest.hexdigest()):
                        raise ValueError('Artifact evidence mismatch')
                    db.execute('INSERT INTO completion_proofs VALUES (?,?)',(job_id,json.dumps(r,sort_keys=True)))
                except (OSError,TypeError,ValueError):
                    success=False
                    rejected=True
            cur = db.execute("UPDATE jobs SET state=? WHERE job_id=? AND state='claimed' AND manager_token=? AND worker_pid=?",
                             ('finished' if success else 'failed',job_id,token,worker_pid))
            if cur.rowcount != 1:
                db.rollback()
                raise RuntimeError('Job is not claimed by this manager and worker')
            if not success:
                db.execute('INSERT OR IGNORE INTO queue_hold VALUES (1,?)', (job_id,))
            db.execute('INSERT INTO events(time,event,token,job_id,pid) VALUES (?,?,?,?,?)',
                       (_now(),'finished' if success else 'failed',token,job_id,worker_pid))
            db.commit()
        if rejected:
            raise RuntimeError('Completion proof rejected; queue is on hold')

    def stop(self, token):
        with closing(sqlite3.connect(self.path, timeout=10)) as db:
            db.execute('BEGIN IMMEDIATE')
            self._require_owner(db, token)
            if db.execute("SELECT 1 FROM jobs WHERE state='claimed' AND manager_token=? LIMIT 1",(token,)).fetchone():
                db.rollback()
                raise RuntimeError('A claimed job must be resolved before stopping')
            db.execute('INSERT INTO events(time,event,token) VALUES (?,?,?)',(_now(),'manager_stopped',token))
            db.execute('DELETE FROM manager WHERE token=?',(token,))
            db.commit()
