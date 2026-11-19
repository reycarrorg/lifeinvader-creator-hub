use crate::{ApiResult, App, bad, internal};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
fn token(db: &rusqlite::Connection) -> ApiResult<String> {
    let existing: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key='bridge_token'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?;
    if let Some(s) = existing {
        return Ok(s);
    }
    let value = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    db.execute(
        "INSERT OR IGNORE INTO settings(key,value) VALUES('bridge_token',?)",
        [&value],
    )
    .map_err(internal)?;
    db.query_row(
        "SELECT value FROM settings WHERE key='bridge_token'",
        [],
        |r| r.get(0),
    )
    .map_err(internal)
}
fn authenticated(db: &rusqlite::Connection, h: &HeaderMap) -> bool {
    let provided = h
        .get("x-lifeinvader-token")
        .and_then(|x| x.to_str().ok())
        .unwrap_or("");
    let stored: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key='bridge_token'",
            [],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None);
    match stored {
        Some(s) if s.len() == provided.len() && !s.is_empty() => {
            s.bytes()
                .zip(provided.bytes())
                .fold(0u8, |v, (a, b)| v | (a ^ b))
                == 0
        }
        _ => false,
    }
}
fn auth(db: &rusqlite::Connection, h: &HeaderMap) -> ApiResult<()> {
    if authenticated(db, h) {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            "Pair the browser extension first.".into(),
        ))
    }
}
pub async fn pair(State(app): State<App>) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().map_err(internal)?;
    Ok(Json(serde_json::json!({"token":token(&db)?})))
}
pub async fn reset(State(app): State<App>) -> ApiResult<StatusCode> {
    let db = app.db.lock().map_err(internal)?;
    db.execute("DELETE FROM settings WHERE key='bridge_token'", [])
        .map_err(internal)?;
    db.execute(
        "UPDATE analytics_bridge SET job_id=NULL,lease_until=NULL",
        [],
    )
    .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
pub struct Config {
    enabled: bool,
    interval_seconds: i64,
    after_post: bool,
}
fn profile(db: &rusqlite::Connection, id: i64) -> ApiResult<()> {
    let found:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM profiles WHERE id=? AND archived=0 AND role IN ('destination','source') AND platform IN ('tiktok','instagram'))",[id],|r|r.get(0)).map_err(internal)?;
    if found {
        Ok(())
    } else {
        Err(bad("Brave refresh supports TikTok and Instagram accounts."))
    }
}
pub async fn config(
    State(app): State<App>,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().map_err(internal)?;
    profile(&db, id)?;
    let row:Option<(bool,i64,bool,Option<i64>,Option<i64>,Option<i64>,String)>=db.query_row("SELECT enabled,interval_seconds,after_post,next_run,last_attempt,last_success,last_error FROM analytics_bridge WHERE profile_id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(internal)?;
    let (enabled, seconds, after, next, attempt, success, error) =
        row.unwrap_or((false, 14400, false, None, None, None, String::new()));
    Ok(Json(
        serde_json::json!({"enabled":enabled,"interval_seconds":seconds,"after_post":after,"next_run":next,"last_attempt":attempt,"last_success":success,"last_error":error}),
    ))
}
pub async fn save_config(
    State(app): State<App>,
    Path(id): Path<i64>,
    Json(c): Json<Config>,
) -> ApiResult<StatusCode> {
    if !(3600..=604800).contains(&c.interval_seconds) {
        return Err(bad("Choose an interval between one hour and seven days."));
    }
    let db = app.db.lock().map_err(internal)?;
    profile(&db, id)?;
    db.execute("INSERT INTO analytics_bridge(profile_id,enabled,interval_seconds,after_post,next_run) VALUES(?,?,?,?,CASE WHEN ? THEN unixepoch() ELSE NULL END) ON CONFLICT(profile_id) DO UPDATE SET enabled=excluded.enabled,interval_seconds=excluded.interval_seconds,after_post=excluded.after_post,next_run=CASE WHEN excluded.enabled THEN unixepoch() ELSE NULL END,job_id=NULL,lease_until=NULL",params![id,c.enabled,c.interval_seconds,c.after_post,c.enabled]).map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn refresh(State(app): State<App>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let db = app.db.lock().map_err(internal)?;
    profile(&db, id)?;
    let changed=db.execute("UPDATE analytics_bridge SET next_run=unixepoch(),last_error='' WHERE profile_id=? AND enabled=1",[id]).map_err(internal)?;
    if changed == 0 {
        return Err(bad("Enable Brave refresh for this profile first."));
    }
    Ok(StatusCode::ACCEPTED)
}
#[derive(Serialize)]
struct Job {
    id: String,
    profile_id: i64,
    platform: String,
    url: String,
}
pub async fn poll(
    State(app): State<App>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    let mut db = app.db.lock().map_err(internal)?;
    auth(&db, &headers)?;
    let tx = db.transaction().map_err(internal)?;
    let row:Option<(i64,String,String)>=tx.query_row("SELECT p.id,p.platform,p.url FROM analytics_bridge b JOIN profiles p ON p.id=b.profile_id WHERE p.archived=0 AND p.role IN ('destination','source') AND p.platform IN ('instagram','tiktok') AND b.enabled=1 AND b.next_run<=unixepoch() AND (b.lease_until IS NULL OR b.lease_until<unixepoch()) ORDER BY b.next_run,p.id LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(internal)?;
    let job = if let Some((id, platform, url)) = row {
        let j = Uuid::new_v4().to_string();
        tx.execute("UPDATE analytics_bridge SET job_id=?,lease_until=unixepoch()+180,last_attempt=unixepoch(),last_error='',next_run=unixepoch()+interval_seconds WHERE profile_id=?",params![j,id]).map_err(internal)?;
        Some(Job {
            id: j,
            profile_id: id,
            platform,
            url,
        })
    } else {
        None
    };
    tx.commit().map_err(internal)?;
    Ok(Json(serde_json::json!({"job":job})))
}
#[derive(Deserialize)]
pub struct ResultInput {
    profile_id: i64,
    job_id: String,
    followers: Option<i64>,
    following: Option<i64>,
    likes: Option<i64>,
    #[serde(default)]
    followers_approx: bool,
    #[serde(default)]
    following_approx: bool,
    #[serde(default)]
    likes_approx: bool,
    #[serde(default)]
    post_links: Vec<String>,
    error: Option<String>,
}
pub async fn result(
    State(app): State<App>,
    headers: HeaderMap,
    Json(r): Json<ResultInput>,
) -> ApiResult<StatusCode> {
    let mut db = app.db.lock().map_err(internal)?;
    auth(&db, &headers)?;
    let tx = db.transaction().map_err(internal)?;
    let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM analytics_bridge WHERE profile_id=? AND enabled=1 AND job_id=? AND lease_until>=unixepoch())",params![r.profile_id,r.job_id],|r|r.get(0)).map_err(internal)?;
    if !exists {
        return Err((
            StatusCode::CONFLICT,
            "Refresh expired or was cancelled.".into(),
        ));
    }
    let values = [r.followers, r.following, r.likes];
    if values
        .iter()
        .flatten()
        .any(|n| *n < 0 || *n > 9_007_199_254_740_991)
    {
        return Err(bad("Invalid count."));
    }
    let error = r.error.unwrap_or_default();
    let err = match error.as_str() {
        "" if values.iter().any(Option::is_some) => "",
        "identity" => "The page opened a different profile.",
        "login" => "The profile was not visible in Brave. Check your sign-in and account access.",
        "timeout" => "The profile did not finish loading in Brave.",
        _ => {
            "Brave could not read exact profile counts. The page may have changed or blocked access."
        }
    };
    if err.is_empty() {
        let profile_url:String=tx.query_row("SELECT CASE WHEN platform='instagram' THEN url ELSE '' END FROM profiles WHERE id=?",[r.profile_id],|row|row.get(0)).map_err(internal)?;
        let account = profile_url
            .parse::<axum::http::Uri>()
            .ok()
            .and_then(|u| {
                u.path()
                    .split('/')
                    .filter(|v| !v.is_empty())
                    .next()
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        for link in r.post_links.iter().take(50) {
            let Ok(uri) = link.parse::<axum::http::Uri>() else {
                continue;
            };
            if uri.scheme_str() != Some("https")
                || !["instagram.com", "www.instagram.com"].contains(&uri.host().unwrap_or(""))
            {
                continue;
            }
            let parts: Vec<&str> = uri.path().split('/').filter(|v| !v.is_empty()).collect();
            if parts.len() != 3
                || !parts[0].eq_ignore_ascii_case(&account)
                || !["p", "reel"].contains(&parts[1])
                || !(5..=40).contains(&parts[2].len())
                || !parts[2]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                continue;
            }
            let canonical = format!("https://www.instagram.com/{}/{}/", parts[1], parts[2]);
            tx.execute("INSERT INTO analytics_bridge_links(profile_id,url,captured_at) VALUES(?,?,datetime('now')) ON CONFLICT(profile_id,url) DO UPDATE SET captured_at=excluded.captured_at",params![r.profile_id,canonical]).map_err(internal)?;
        }
        tx.execute("INSERT INTO analytics_bridge_snapshots(profile_id,observed_at,followers,following,likes,followers_approx,following_approx,likes_approx) VALUES(?,strftime('%Y-%m-%dT%H:%M:%fZ','now'),?,?,?,?,?,?)",params![r.profile_id,r.followers,r.following,r.likes,r.followers_approx,r.following_approx,r.likes_approx]).map_err(internal)?;
    }
    tx.execute("UPDATE analytics_bridge SET job_id=NULL,lease_until=NULL,last_error=?,last_success=CASE WHEN ?='' THEN unixepoch() ELSE last_success END,next_run=next_run WHERE profile_id=?",params![err,err,r.profile_id]).map_err(internal)?;
    tx.commit().map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn check(State(app): State<App>, headers: HeaderMap) -> ApiResult<StatusCode> {
    let db = app.db.lock().map_err(internal)?;
    auth(&db, &headers)?;
    Ok(StatusCode::NO_CONTENT)
}
