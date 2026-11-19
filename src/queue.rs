use crate::{ApiResult, App, bad, internal, source};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct Add {
    source_id: i64,
    path: String,
    #[serde(default)]
    scheduled_for: String,
}
#[derive(Deserialize)]
pub struct Update {
    state: String,
    #[serde(default)]
    caption: String,
    #[serde(default)]
    post_url: String,
    #[serde(default)]
    scheduled_for: String,
}
#[derive(Serialize)]
pub struct Entry {
    id: i64,
    source_id: i64,
    source_name: String,
    platform: String,
    path: String,
    post_id: String,
    title: String,
    description: String,
    posted_date: String,
    posted_date_kind: String,
    original_url: String,
    channel_url: String,
    state: String,
    caption: String,
    post_url: String,
    scheduled_for: String,
    available: bool,
}
fn schedule(s: &str) -> ApiResult<()> {
    if !s.is_empty()
        && (s.len() != 16 || chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").is_err())
    {
        return Err(bad("Use a valid local scheduled date and time."));
    }
    Ok(())
}
fn destination(db: &rusqlite::Connection, id: i64) -> ApiResult<()> {
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM profiles WHERE id=? AND role='destination')",
            [id],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if exists {
        Ok(())
    } else {
        Err((StatusCode::NOT_FOUND, "Posting profile not found.".into()))
    }
}
pub async fn list(State(app): State<App>, Path(id): Path<i64>) -> ApiResult<Json<Vec<Entry>>> {
    let db = app.db.lock().map_err(internal)?;
    destination(&db, id)?;
    let current_root = source::config(&db)
        .ok()
        .and_then(|c| source::checked_root(&c).ok())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut stmt=db.prepare("SELECT q.id,q.source_id,p.name,q.platform,q.path,q.post_id,q.title,q.description,q.posted_date,q.original_url,q.channel_url,q.state,q.caption,q.post_url,q.scheduled_for,q.root,q.posted_date_kind FROM posting_queue q JOIN profiles p ON p.id=q.source_id WHERE q.profile_id=? ORDER BY CASE WHEN q.state='queued' THEN 0 ELSE 1 END, q.id DESC").map_err(internal)?;
    let rows = stmt
        .query_map([id], |r| {
            Ok((
                Entry {
                    id: r.get(0)?,
                    source_id: r.get(1)?,
                    source_name: r.get(2)?,
                    platform: r.get(3)?,
                    path: r.get(4)?,
                    post_id: r.get(5)?,
                    title: r.get(6)?,
                    description: r.get(7)?,
                    posted_date: r.get(8)?,
                    posted_date_kind: r.get(16)?,
                    original_url: r.get(9)?,
                    channel_url: r.get(10)?,
                    state: r.get(11)?,
                    caption: r.get(12)?,
                    post_url: r.get(13)?,
                    scheduled_for: r.get(14)?,
                    available: false,
                },
                r.get::<_, String>(15)?,
            ))
        })
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    drop(stmt);
    drop(db);
    let entries = rows
        .into_iter()
        .map(|(mut e, root)| {
            e.available = root == current_root
                && source::resolve_file(&app, e.source_id, &e.path, "source").is_ok();
            e
        })
        .collect();
    Ok(Json(entries))
}
pub async fn add(
    State(app): State<App>,
    Path(id): Path<i64>,
    Json(input): Json<Add>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    schedule(&input.scheduled_for)?;
    let target = source::resolve_file(&app, input.source_id, &input.path, "source")?;
    let (mut date, mut post_id, _, title, description, url, _) =
        source::post_metadata(&target).unwrap_or_default();
    let db = app.db.lock().map_err(internal)?;
    destination(&db, id)?;
    let (mut platform, mut channel_url, _) = source::source_profile(&db, input.source_id)?;
    if channel_url == crate::solo::SOLO_URL {
        let (_, original_channel, original_platform) = source::original_account(&target);
        channel_url = original_channel.unwrap_or_default();
        platform = original_platform.unwrap_or_else(|| "other".into());
    }
    let root_path = source::checked_root(&source::config(&db).map_err(internal)?)?;
    let mut date_kind = date.as_ref().map(|_| "metadata".to_owned());
    crate::date_audit::resolve(
        &db,
        input.source_id,
        &root_path,
        &platform,
        &input.path,
        target.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        &mut post_id,
        &mut date,
        &mut date_kind,
    )
    .map_err(internal)?;
    let root = root_path.to_string_lossy().into_owned();
    let pid = post_id.unwrap_or_default();
    let key = if pid.is_empty() {
        format!("file:{}:{}", input.source_id, input.path)
    } else {
        format!("post:{pid}")
    };
    let outcome=db.execute("INSERT OR IGNORE INTO posting_queue(profile_id,source_id,platform,identity_key,root,path,post_id,title,description,posted_date,original_url,channel_url,scheduled_for,posted_date_kind) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",params![id,input.source_id,platform,key,root,input.path,pid,title.unwrap_or_default(),description.unwrap_or_default(),date.unwrap_or_default(),url.unwrap_or_default(),channel_url,input.scheduled_for,date_kind.unwrap_or_default()]).map_err(internal)?;
    if outcome == 0 {
        return Err((
            StatusCode::CONFLICT,
            "This post is already queued or recorded for this profile.".into(),
        ));
    }
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"id":db.last_insert_rowid()})),
    ))
}
pub async fn update(
    State(app): State<App>,
    Path((id, qid)): Path<(i64, i64)>,
    Json(input): Json<Update>,
) -> ApiResult<StatusCode> {
    if !["queued", "posted", "checked"].contains(&input.state.as_str()) {
        return Err(bad("Choose queued, posted, or checked."));
    }
    schedule(&input.scheduled_for)?;
    if input.caption.len() > 10000 || input.post_url.len() > 2000 {
        return Err(bad("Caption or link is too long."));
    }
    if !input.post_url.is_empty() && !input.post_url.starts_with("https://") {
        return Err(bad("Use an https:// post link."));
    }
    let db = app.db.lock().map_err(internal)?;
    destination(&db, id)?;
    let updated=db.execute("UPDATE posting_queue SET state=?,caption=?,post_url=?,scheduled_for=?,updated_at=datetime('now') WHERE id=? AND profile_id=?",params![input.state,input.caption,input.post_url,input.scheduled_for,qid,id]).map_err(internal)?;
    if updated == 0 {
        return Err((StatusCode::NOT_FOUND, "Queue entry not found.".into()));
    }
    if input.state == "posted" {
        db.execute("UPDATE analytics_bridge SET next_run=unixepoch() WHERE profile_id=? AND enabled=1 AND after_post=1",[id]).map_err(internal)?;
    }
    Ok(StatusCode::NO_CONTENT)
}
