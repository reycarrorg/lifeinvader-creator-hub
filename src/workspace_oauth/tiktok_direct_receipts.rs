use rusqlite::{params, Connection, OptionalExtension, Result, Row};
use uuid::Uuid;

#[derive(Debug)]
pub(super) struct Receipt {
    pub attempt: String,
    pub status: String,
    pub publish_id: Option<String>,
    pub detail: String,
    pub post_id: Option<String>,
    pub privacy_level: Option<String>,
}

pub(super) enum Reservation {
    New(Receipt),
    Existing(Receipt),
}

pub(super) enum State {
    Processing,
    Complete,
    Failed,
    Ambiguous,
}

pub(super) fn init(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_tiktok_direct_attempts (
            id INTEGER PRIMARY KEY,
            attempt TEXT NOT NULL UNIQUE,
            account_id TEXT NOT NULL,
            content_sha256 TEXT NOT NULL,
            clip_id TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'RESERVED'
                CHECK (status IN ('RESERVED', 'UPLOADING', 'PROCESSING', 'UNKNOWN', 'PUBLISH_COMPLETE', 'FAILED')),
            publish_id TEXT UNIQUE,
            post_id TEXT,
            privacy_level TEXT,
            detail TEXT NOT NULL DEFAULT '',
            updated_at INTEGER NOT NULL DEFAULT (unixepoch())
         );
         CREATE UNIQUE INDEX IF NOT EXISTS workspace_tiktok_direct_active
            ON workspace_tiktok_direct_attempts(account_id, content_sha256) WHERE status <> 'FAILED';
         CREATE INDEX IF NOT EXISTS workspace_tiktok_direct_history
            ON workspace_tiktok_direct_attempts(account_id, content_sha256, id DESC);",
    )?;
    let has_privacy: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('workspace_tiktok_direct_attempts') WHERE name='privacy_level')",
        [], |row| row.get(0),
    )?;
    if !has_privacy {
        db.execute("ALTER TABLE workspace_tiktok_direct_attempts ADD COLUMN privacy_level TEXT", [])?;
    }
    Ok(())
}

fn receipt(row: &Row<'_>) -> Result<Receipt> {
    Ok(Receipt {
        attempt: row.get(0)?,
        status: row.get(1)?,
        publish_id: row.get(2)?,
        detail: row.get(3)?,
        post_id: row.get(4)?,
        privacy_level: row.get(5)?,
    })
}

pub(super) fn lookup(db: &Connection, account: &str, sha: &str) -> Result<Option<Receipt>> {
    db.query_row(
        "SELECT attempt, status, publish_id, detail, post_id, privacy_level
         FROM workspace_tiktok_direct_attempts
         WHERE account_id = ?1 AND content_sha256 = ?2 ORDER BY id DESC LIMIT 1",
        params![account, sha],
        receipt,
    )
    .optional()
}

pub(super) fn recover_unstarted(db: &Connection, account: &str, sha: &str) -> Result<()> {
    // FILE_UPLOAD cannot create a post before the PUT. A worker must first
    // persist its publish_id using record_id, which refuses a retired attempt.
    db.execute(
        "UPDATE workspace_tiktok_direct_attempts
         SET status = 'FAILED', detail = 'Upload never started.', updated_at = unixepoch()
         WHERE account_id = ?1 AND content_sha256 = ?2 AND status = 'RESERVED'
           AND publish_id IS NULL AND updated_at < unixepoch() - 600",
        params![account, sha],
    )?;
    Ok(())
}

pub(super) fn reserve(
    db: &Connection,
    account: &str,
    sha: &str,
    clip_id: &str,
    privacy_level: &str,
) -> Result<Reservation> {
    let tx = db.unchecked_transaction()?;
    recover_unstarted(&tx, account, sha)?;
    let attempt = Uuid::new_v4().to_string();
    let inserted = tx.execute(
        "INSERT INTO workspace_tiktok_direct_attempts (attempt, account_id, content_sha256, clip_id, privacy_level)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(account_id, content_sha256) WHERE status <> 'FAILED' DO NOTHING",
        params![attempt, account, sha, clip_id, privacy_level],
    )?;
    let current = lookup(&tx, account, sha)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    tx.commit()?;
    Ok(if inserted == 1 {
        Reservation::New(current)
    } else {
        Reservation::Existing(current)
    })
}

/// Commit the remote session before uploading any bytes. Only a true result
/// gives the caller permission to use that session's upload URL.
pub(super) fn record_id(db: &Connection, attempt: &str, publish_id: &str) -> Result<bool> {
    Ok(db.execute(
        "UPDATE workspace_tiktok_direct_attempts
         SET publish_id = ?2, status = 'UPLOADING', updated_at = unixepoch()
         WHERE attempt = ?1 AND status = 'RESERVED' AND publish_id IS NULL AND length(?2) > 0",
        params![attempt, publish_id],
    )? == 1)
}

pub(super) fn set_state(
    db: &Connection,
    attempt: &str,
    state: State,
    detail: &str,
    post_id: Option<&str>,
) -> Result<bool> {
    let status = match state {
        State::Processing => "PROCESSING",
        State::Complete => "PUBLISH_COMPLETE",
        State::Failed => "FAILED",
        State::Ambiguous => "UNKNOWN",
    };
    let post_id = post_id.filter(|id| status == "PUBLISH_COMPLETE" && !id.is_empty());
    Ok(db.execute(
        "UPDATE workspace_tiktok_direct_attempts
         SET status = ?2, detail = ?3, post_id = coalesce(?4, post_id), updated_at = unixepoch()
         WHERE attempt = ?1
           AND (publish_id IS NOT NULL OR ?2 IN ('FAILED', 'UNKNOWN'))
           AND (status NOT IN ('FAILED', 'PUBLISH_COMPLETE')
             OR (status = 'PUBLISH_COMPLETE' AND ?2 = 'PUBLISH_COMPLETE' AND post_id IS NULL AND ?4 IS NOT NULL))",
        params![attempt, status, detail, post_id],
    )? == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Barrier, Mutex};

    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        db
    }

    fn new_attempt(db: &Connection) -> String {
        match reserve(db, "account", "hash", "clip", "SELF_ONLY").unwrap() {
            Reservation::New(r) => r.attempt,
            Reservation::Existing(_) => panic!("expected a new attempt"),
        }
    }

    #[test]
    fn privacy_migration_preserves_existing_receipts_and_retry_uses_saved_audience() {
        let db = database();
        let previous = new_attempt(&db);
        db.execute("ALTER TABLE workspace_tiktok_direct_attempts DROP COLUMN privacy_level", []).unwrap();
        init(&db).unwrap();
        let old = lookup(&db, "account", "hash").unwrap().unwrap();
        assert_eq!(old.attempt, previous);
        assert!(old.privacy_level.is_none());
        let Reservation::New(public) = reserve(&db, "account", "other", "clip", "PUBLIC_TO_EVERYONE").unwrap()
            else { panic!("expected a new public post") };
        let Reservation::Existing(duplicate) = reserve(&db, "account", "other", "alias", "SELF_ONLY").unwrap()
            else { panic!("duplicate") };
        assert_eq!(public.attempt, duplicate.attempt);
        assert_eq!(duplicate.privacy_level.as_deref(), Some("PUBLIC_TO_EVERYONE"));
    }

    #[test]
    fn parallel_clip_aliases_reserve_one_upload_per_account_and_asset() {
        let db = Mutex::new(database());
        let barrier = Barrier::new(8);
        let reservations = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..8)
                .map(|alias| {
                    let (db, barrier) = (&db, &barrier);
                    scope.spawn(move || {
                        barrier.wait();
                        reserve(&db.lock().unwrap(), "account", "hash", &alias.to_string(), "SELF_ONLY").unwrap()
                    })
                })
                .collect();
            workers.into_iter().map(|w| w.join().unwrap()).collect::<Vec<_>>()
        });
        let new = reservations.iter().filter(|r| matches!(r, Reservation::New(_)));
        assert_eq!(new.count(), 1);
        let db = db.into_inner().unwrap();
        let first = lookup(&db, "account", "hash").unwrap().unwrap();
        for reservation in reservations {
            let r = match reservation {
                Reservation::New(r) | Reservation::Existing(r) => r,
            };
            assert_eq!(r.attempt, first.attempt);
        }
        assert!(matches!(reserve(&db, "another account", "hash", "clip", "SELF_ONLY").unwrap(), Reservation::New(_)));
    }

    #[test]
    fn ambiguous_upload_can_be_reconciled_but_cannot_be_resent() {
        let db = database();
        let attempt = new_attempt(&db);
        assert!(!set_state(&db, &attempt, State::Complete, "", None).unwrap());
        assert!(record_id(&db, &attempt, "remote").unwrap());
        assert!(!record_id(&db, &attempt, "replacement").unwrap());
        assert!(set_state(&db, &attempt, State::Ambiguous, "Upload response lost", None).unwrap());
        assert!(matches!(reserve(&db, "account", "hash", "alias", "PUBLIC_TO_EVERYONE").unwrap(), Reservation::Existing(_)));
        let pending = lookup(&db, "account", "hash").unwrap().unwrap();
        assert_eq!(pending.publish_id.as_deref(), Some("remote"));
        assert_eq!(pending.detail, "Upload response lost");
        assert!(set_state(&db, &attempt, State::Processing, "", None).unwrap());
        assert!(set_state(&db, &attempt, State::Complete, "", None).unwrap());
        assert!(!set_state(&db, &attempt, State::Failed, "stale poll", None).unwrap());
        assert!(set_state(&db, &attempt, State::Complete, "", Some("post")).unwrap());
        let complete = lookup(&db, "account", "hash").unwrap().unwrap();
        assert_eq!(complete.status, "PUBLISH_COMPLETE");
        assert_eq!(complete.post_id.as_deref(), Some("post"));
        assert!(matches!(reserve(&db, "account", "hash", "alias", "PUBLIC_TO_EVERYONE").unwrap(), Reservation::Existing(_)));
    }

    #[test]
    fn definitive_failure_allows_a_new_attempt_and_fences_old_callbacks() {
        let db = database();
        let first = new_attempt(&db);
        assert!(record_id(&db, &first, "first-remote-id").unwrap());
        assert!(set_state(&db, &first, State::Failed, "Provider rejected media", None).unwrap());
        let second = new_attempt(&db);
        assert_ne!(first, second);
        assert!(!set_state(&db, &first, State::Complete, "", Some("wrong")).unwrap());
        assert!(record_id(&db, &second, "next-remote-id").unwrap());
        assert_eq!(lookup(&db, "account", "hash").unwrap().unwrap().attempt, second);
    }

    #[test]
    fn interrupted_reservation_expires_only_before_a_remote_id_is_attached() {
        let db = database();
        let first = new_attempt(&db);
        db.execute("UPDATE workspace_tiktok_direct_attempts SET updated_at = 0", []).unwrap();
        recover_unstarted(&db, "account", "hash").unwrap();
        assert_eq!(lookup(&db, "account", "hash").unwrap().unwrap().status, "FAILED");
        let second = new_attempt(&db);
        assert!(!record_id(&db, &first, "too-late").unwrap());
        assert!(record_id(&db, &second, "remote").unwrap());
        db.execute("UPDATE workspace_tiktok_direct_attempts SET updated_at = 0", []).unwrap();
        assert!(matches!(reserve(&db, "account", "hash", "alias", "PUBLIC_TO_EVERYONE").unwrap(), Reservation::Existing(_)));
    }

    #[test]
    fn pending_remote_id_survives_a_database_reopen() {
        let path = std::env::temp_dir().join(format!("snapmatic-direct-{}.sqlite3", Uuid::new_v4()));
        {
            let db = Connection::open(&path).unwrap();
            init(&db).unwrap();
            let attempt = new_attempt(&db);
            assert!(record_id(&db, &attempt, "durable-session").unwrap());
        }
        {
            let db = Connection::open(&path).unwrap();
            init(&db).unwrap();
            let Reservation::Existing(pending) = reserve(&db, "account", "hash", "alias", "PUBLIC_TO_EVERYONE").unwrap() else {
                panic!("an interrupted upload must retain its remote session");
            };
            assert_eq!(pending.status, "UPLOADING");
            assert_eq!(pending.publish_id.as_deref(), Some("durable-session"));
        }
        std::fs::remove_file(path).unwrap();
    }
}
