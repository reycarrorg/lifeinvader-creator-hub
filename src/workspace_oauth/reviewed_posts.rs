use rusqlite::{Connection, TransactionBehavior, params};
use std::sync::Mutex;

pub(super) fn init(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_reviewed_public_posts (
            provider TEXT NOT NULL, account_id TEXT NOT NULL, content_sha256 TEXT NOT NULL,
            media_reference TEXT NOT NULL, post_id TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY(provider, account_id, content_sha256)
        );",
    )
}

pub(super) struct Target<'a> {
    pub provider: &'a str,
    pub account: &'a str,
    pub digest: &'a str,
    pub media_reference: &'a str,
}

pub(super) enum Failure {
    Rejected(String),
    Unconfirmed(String),
}

impl Failure {
    pub(super) fn http(status: u32, message: Option<&str>, invalid_request: bool) -> Self {
        let detail = message.unwrap_or("The platform did not confirm publication.").to_owned();
        if message.is_some() && (matches!(status, 401 | 403 | 404 | 413 | 415 | 422) || (status == 400 && invalid_request)) {
            Self::Rejected(detail)
        } else {
            Self::Unconfirmed(detail)
        }
    }

    fn message(self) -> String {
        match self {
            Self::Rejected(message) | Self::Unconfirmed(message) => message,
        }
    }
}

// Reserve only the final public API call. Uploading a media container alone
// cannot publish, so preparation failures require no claim or reconciliation.
pub(super) fn publish(
    db: &Mutex<Connection>,
    target: Option<Target<'_>>,
    send: impl FnOnce() -> Result<String, Failure>,
) -> Result<(String, bool), String> {
    let Some(target) = target else {
        return send().map(|id| (id, false)).map_err(Failure::message);
    };
    let key = params![target.provider, target.account, target.digest];
    let hold = format!(
        "This {} post is in progress or its outcome is unconfirmed. Check the destination account before retrying.",
        target.provider
    );
    let storage = |err: rusqlite::Error| format!("Could not update the {} posting receipt: {err}", target.provider);
    {
        let mut conn = db.lock().map_err(|_| "Posting receipts unavailable.".to_owned())?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(storage)?;
        let inserted = tx.execute(
            "INSERT INTO workspace_reviewed_public_posts(provider,account_id,content_sha256,media_reference)
             VALUES(?1,?2,?3,?4) ON CONFLICT(provider,account_id,content_sha256) DO NOTHING",
            params![target.provider, target.account, target.digest, target.media_reference],
        ).map_err(storage)?;
        if inserted == 0 {
            let id: String = tx.query_row(
                "SELECT post_id FROM workspace_reviewed_public_posts WHERE provider=?1 AND account_id=?2 AND content_sha256=?3",
                key, |row| row.get(0),
            ).map_err(storage)?;
            return if id.is_empty() { Err(hold) } else { Ok((id, true)) };
        }
        tx.commit().map_err(storage)?;
    }
    match send() {
        Ok(id) if !id.trim().is_empty() => {
            let conn = db.lock().map_err(|_| hold.clone())?;
            conn.execute(
                "UPDATE workspace_reviewed_public_posts SET post_id=?4 WHERE provider=?1 AND account_id=?2 AND content_sha256=?3",
                params![target.provider, target.account, target.digest, id],
            ).map_err(storage)?;
            Ok((id, false))
        }
        Err(Failure::Rejected(reason)) => {
            let conn = db.lock().map_err(|_| hold.clone())?;
            conn.execute(
                "DELETE FROM workspace_reviewed_public_posts WHERE provider=?1 AND account_id=?2 AND content_sha256=?3 AND post_id=''",
                key,
            ).map_err(storage)?;
            Err(reason)
        }
        Err(Failure::Unconfirmed(reason)) => Err(format!("{hold} {reason}")),
        Ok(_) => Err(hold),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(account: &str) -> Option<Target<'_>> {
        Some(Target { provider: "instagram", account, digest: "bytes", media_reference: "container" })
    }

    #[test]
    fn reserves_before_send_and_reuses_success_only_on_the_same_destination() {
        let db = Mutex::new(Connection::open_in_memory().unwrap());
        init(&db.lock().unwrap()).unwrap();
        let first = publish(&db, target("first"), || {
            assert!(publish(&db, target("first"), || panic!("concurrent duplicate")).is_err());
            Ok("published-id".into())
        }).unwrap();
        assert_eq!(first, ("published-id".into(), false));
        assert_eq!(publish(&db, target("first"), || panic!("duplicate")).unwrap(), ("published-id".into(), true));
        assert_eq!(publish(&db, target("second"), || Ok("second-id".into())).unwrap(), ("second-id".into(), false));
    }

    #[test]
    fn holds_ambiguous_results_and_releases_explicit_provider_rejections() {
        let db = Mutex::new(Connection::open_in_memory().unwrap());
        init(&db.lock().unwrap()).unwrap();
        assert!(publish(&db, target("a"), || Err(Failure::http(500, Some("unavailable"), false))).is_err());
        assert!(publish(&db, target("a"), || panic!("resending uncertain post")).is_err());
        assert!(publish(&db, target("b"), || Err(Failure::http(400, Some("invalid text"), true))).is_err());
        assert!(publish(&db, target("b"), || Ok("corrected".into())).is_ok());
        assert!(publish(&db, target("c"), || Err(Failure::http(400, Some("unknown graph error"), false))).is_err());
        assert!(publish(&db, target("c"), || panic!("resending unknown HTTP 400")).is_err());
    }
}
