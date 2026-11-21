#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#[allow(dead_code)] mod controller;
#[allow(dead_code)] mod controller_store;
#[allow(dead_code)] mod controller_ops;
#[allow(dead_code)] mod controller_worker;
#[cfg(feature="snapmatic-hud")] mod workspace;
#[cfg(feature="snapmatic-hud")] mod workspace_media;
#[cfg(feature="snapmatic-hud")] mod workspace_events;
#[cfg(feature="snapmatic-hud")] mod workspace_ledger;
#[cfg(feature="snapmatic-hud")] mod workspace_drafts;
#[cfg(feature="snapmatic-hud")] #[allow(dead_code)] mod workspace_phase1;
#[cfg(feature="snapmatic-hud")] mod workspace_platform_drafts;
#[cfg(feature="snapmatic-hud")] #[allow(dead_code)] mod workspace_nvenc;
#[cfg(feature="snapmatic-hud")] #[allow(dead_code)] mod workspace_phase4;
#[cfg(feature="snapmatic-hud")] mod workspace_triage;
#[cfg(feature="snapmatic-hud")] mod workspace_film_roll;
#[cfg(feature="snapmatic-hud")] mod workspace_studio;
#[cfg(feature="snapmatic-hud")] mod workspace_oauth;
#[cfg(feature="snapmatic-hud")] mod workspace_tiktok_export;
#[cfg(feature="snapmatic-hud")] mod controller_tiktok_inbox;
#[cfg(windows)] #[allow(dead_code)] mod controller_youtube;
#[cfg(windows)] #[allow(dead_code)] mod controller_instagram;
#[allow(dead_code)] mod controller_registry;
#[allow(dead_code)] mod controller_source;
#[allow(dead_code)] mod controller_proof;
#[allow(dead_code)] mod controller_receipt;
#[allow(dead_code)] mod controller_clip;
#[allow(dead_code)] mod controller_admission;
#[allow(dead_code)] mod controller_supervisor;
#[allow(dead_code)] mod controller_manual;
#[allow(dead_code)] mod controller_ui_lifetime;
#[allow(dead_code)] mod controller_candidate;
#[cfg(windows)] #[allow(dead_code)] mod controller_native_process;
#[cfg(windows)] #[allow(dead_code)] mod controller_native_launcher;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path as AxumPath, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
};
use chrono::{DateTime, NaiveDate};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tower_http::services::ServeDir;
use uuid::Uuid;
mod analytics;
mod auto_queue;
mod editor;
mod recovery;
mod handoff;
mod solo;
mod selection;
mod bridge;
mod categories;
mod date_audit;
mod queue;
mod scheduler;
mod source;
mod tools;
#[allow(dead_code)] mod thumbnail_cache;

type Db = Arc<Mutex<Connection>>;
#[derive(Clone)]
struct App {
    db: Db,
    media: PathBuf,
    log_dir: PathBuf,
    #[allow(dead_code)]
    cache_scope: String,
    controller_ui: Arc<controller_ui_lifetime::Lifetime>,
    controller_catalog: Option<Arc<controller_source::Catalog>>,
    controller_manual: Arc<dyn controller_manual::Manual>,
    controller_ops: Arc<dyn controller_ops::StatusReader>,
    controller_worker: Arc<dyn controller_worker::Reader>,
}
type ApiResult<T> = Result<T, (StatusCode, String)>;
fn bad(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, e.to_string())
}
fn internal(e: impl std::fmt::Display) -> (StatusCode, String) {
    eprintln!("{e}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "The operation could not be completed.".into(),
    )
}

#[derive(Serialize)]
struct Media {
    id: i64,
    platform: String,
    account: String,
    post_id: String,
    source_url: String,
    posted: String,
    caption: String,
    category: String,
    files: Vec<FileInfo>,
    statuses: Vec<PostStatus>,
}
#[derive(Serialize)]
struct FileInfo {
    name: String,
    url: String,
    kind: String,
}
#[derive(Serialize)]
struct PostStatus {
    profile_id: i64,
    destination: String,
    state: String,
    claimed_by: String,
    updated_by: String,
    caption: String,
    post_url: String,
}
#[derive(Deserialize)]
struct Update {
    destination: Option<String>,
    profile_id: Option<i64>,
    user: String,
    action: String,
    caption: Option<String>,
    post_url: Option<String>,
}
#[derive(Deserialize)]
struct Settings {
    cutoff: String,
}
#[derive(Deserialize)]
struct Metadata {
    account: String,
    source_url: String,
    posted: String,
    caption: String,
}
#[derive(Serialize)]
struct Profile {
    id: i64,
    name: String,
    platform: String,
    url: String,
    role: String,
    tags: Vec<String>,
    avatar_url: String,
    source_folder: String,
}
#[derive(Deserialize)]
struct ProfileInput {
    name: String,
    platform: String,
    url: String,
    role: String,
    tags: Vec<String>,
    #[serde(default)]
    source_folder: String,
    #[serde(default)]
    setup_selection: bool,
}

fn init(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
    CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    INSERT OR IGNORE INTO settings VALUES ('cutoff','2026-11-19T00:00:00-06:00');
    CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY, platform TEXT NOT NULL, account TEXT NOT NULL, post_id TEXT NOT NULL, source_url TEXT NOT NULL, posted TEXT NOT NULL, caption TEXT NOT NULL, UNIQUE(platform, post_id));
    CREATE TABLE IF NOT EXISTS files (id INTEGER PRIMARY KEY, item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE, name TEXT NOT NULL, path TEXT NOT NULL, kind TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS statuses (item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE, destination TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'available', claimed_by TEXT NOT NULL DEFAULT '', updated_by TEXT NOT NULL DEFAULT '', caption TEXT NOT NULL DEFAULT '', post_url TEXT NOT NULL DEFAULT '', PRIMARY KEY(item_id,destination));
    CREATE TABLE IF NOT EXISTS profiles (id INTEGER PRIMARY KEY, name TEXT NOT NULL, platform TEXT NOT NULL, url TEXT NOT NULL COLLATE NOCASE UNIQUE, role TEXT NOT NULL CHECK(role IN ('destination','source')), tags TEXT NOT NULL DEFAULT '[]');
    CREATE TABLE IF NOT EXISTS profile_statuses (item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE, profile_id INTEGER NOT NULL REFERENCES profiles(id), state TEXT NOT NULL DEFAULT 'available', claimed_by TEXT NOT NULL DEFAULT '', updated_by TEXT NOT NULL DEFAULT '', caption TEXT NOT NULL DEFAULT '', post_url TEXT NOT NULL DEFAULT '', PRIMARY KEY(item_id,profile_id));")?;
    let migrated: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key='profiles_v1')",
        [],
        |r| r.get(0),
    )?;
    if !migrated {
        db.execute_batch("BEGIN IMMEDIATE;
        INSERT INTO profiles(name,platform,url,role,tags)
        SELECT 'Legacy TikTok','tiktok','https://www.tiktok.com/@example','destination','[]'
        WHERE EXISTS(SELECT 1 FROM statuses WHERE destination='tiktok');
        INSERT INTO profiles(name,platform,url,role,tags)
        SELECT 'Legacy Instagram','instagram','https://www.instagram.com/example','destination','[]'
        WHERE EXISTS(SELECT 1 FROM statuses WHERE destination='instagram');
        INSERT INTO profile_statuses(item_id,profile_id,state,claimed_by,updated_by,caption,post_url)
        SELECT s.item_id,p.id,s.state,s.claimed_by,s.updated_by,s.caption,s.post_url FROM statuses s JOIN profiles p ON p.platform=s.destination AND p.role='destination';
        INSERT OR IGNORE INTO profile_statuses(item_id,profile_id) SELECT i.id,p.id FROM items i CROSS JOIN profiles p WHERE p.role='destination';
        INSERT INTO settings VALUES('profiles_v1','1'); COMMIT;")?;
    }
    let has_avatar = {
        let mut columns = db.prepare("PRAGMA table_info(profiles)")?;
        let names = columns
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        names.iter().any(|name| name == "avatar_path")
    };
    if !has_avatar {
        db.execute(
            "ALTER TABLE profiles ADD COLUMN avatar_path TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    let has_source_folder = {
        let mut columns = db.prepare("PRAGMA table_info(profiles)")?;
        let names = columns
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        names.iter().any(|name| name == "source_folder")
    };
    if !has_source_folder {
        db.execute(
            "ALTER TABLE profiles ADD COLUMN source_folder TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    let has_archived = {
        let mut columns = db.prepare("PRAGMA table_info(profiles)")?;
        let names = columns.query_map([], |r| r.get::<_, String>(1))?.collect::<rusqlite::Result<Vec<_>>>()?;
        names.iter().any(|name| name == "archived")
    };
    if !has_archived { db.execute("ALTER TABLE profiles ADD COLUMN archived INTEGER NOT NULL DEFAULT 0", [])?; }
    db.execute_batch("CREATE TABLE IF NOT EXISTS source_sync_runs (id INTEGER PRIMARY KEY, profile_id INTEGER NOT NULL REFERENCES profiles(id), started TEXT NOT NULL DEFAULT (datetime('now')), finished TEXT, state TEXT NOT NULL, log_file TEXT NOT NULL, exit_code INTEGER);")?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS source_schedules (profile_id INTEGER PRIMARY KEY REFERENCES profiles(id), interval_seconds INTEGER NOT NULL, enabled INTEGER NOT NULL, next_run INTEGER);
    CREATE TABLE IF NOT EXISTS source_queue (id INTEGER PRIMARY KEY, batch_id TEXT NOT NULL, profile_id INTEGER NOT NULL REFERENCES profiles(id), origin TEXT NOT NULL, state TEXT NOT NULL, run_id INTEGER, message TEXT NOT NULL DEFAULT '', created INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS source_preferences (profile_id INTEGER PRIMARY KEY REFERENCES profiles(id), include_in_batch INTEGER NOT NULL DEFAULT 1);
    CREATE TABLE IF NOT EXISTS categories (role TEXT NOT NULL CHECK(role IN ('destination','source')), name TEXT NOT NULL COLLATE NOCASE, PRIMARY KEY(role,name));
    CREATE TABLE IF NOT EXISTS source_new_files (profile_id INTEGER NOT NULL REFERENCES profiles(id), root TEXT NOT NULL, path TEXT NOT NULL, downloaded_at INTEGER NOT NULL, seen INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(profile_id,root,path));")?;
    let category_columns = { let mut q=db.prepare("PRAGMA table_info(categories)")?; q.query_map([],|r|r.get::<_,String>(1))?.collect::<rusqlite::Result<Vec<_>>>()? };
    for (name,default) in [("icon", "#"),("color", "pink"),("image_path", "")] { if !category_columns.iter().any(|x|x==name) { db.execute(&format!("ALTER TABLE categories ADD COLUMN {name} TEXT NOT NULL DEFAULT '{default}'"),[])?; } }
    let has_dead = { let mut q=db.prepare("PRAGMA table_info(source_preferences)")?; let names=q.query_map([],|r|r.get::<_,String>(1))?.collect::<rusqlite::Result<Vec<_>>>()?; names.iter().any(|x|x=="dead") };
    if !has_dead { db.execute("ALTER TABLE source_preferences ADD COLUMN dead INTEGER NOT NULL DEFAULT 0",[])?; }
    db.execute_batch("CREATE TABLE IF NOT EXISTS source_selection(profile_id INTEGER PRIMARY KEY REFERENCES profiles(id),pending INTEGER NOT NULL,allowed TEXT NOT NULL,excluded TEXT NOT NULL, mode TEXT NOT NULL DEFAULT 'all');")?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS source_duration_cache(profile_id INTEGER NOT NULL REFERENCES profiles(id),root TEXT NOT NULL,path TEXT NOT NULL,file_size INTEGER NOT NULL,file_modified INTEGER NOT NULL,seconds REAL NOT NULL,PRIMARY KEY(profile_id,root,path));")?;
    let selection_has_mode = { let mut stmt=db.prepare("PRAGMA table_info(source_selection)")?; let cols=stmt.query_map([],|r|r.get::<_,String>(1))?.collect::<Result<Vec<_>,_>>()?; cols.iter().any(|c|c=="mode") };
    if !selection_has_mode { db.execute("ALTER TABLE source_selection ADD COLUMN mode TEXT NOT NULL DEFAULT 'all'",[])?; }
    db.execute_batch("CREATE TABLE IF NOT EXISTS profile_downloads (profile_id INTEGER NOT NULL REFERENCES profiles(id), root TEXT NOT NULL, platform TEXT NOT NULL, post_id TEXT NOT NULL, path TEXT NOT NULL, posted_date TEXT, PRIMARY KEY(profile_id,root,platform,post_id));")?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS profile_download_files (profile_id INTEGER NOT NULL REFERENCES profiles(id), root TEXT NOT NULL, platform TEXT NOT NULL, post_id TEXT NOT NULL, asset_id TEXT NOT NULL, path TEXT NOT NULL, PRIMARY KEY(profile_id,root,platform,asset_id));")?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS analytics_snapshots (profile_id INTEGER NOT NULL REFERENCES profiles(id) ON DELETE CASCADE, date TEXT NOT NULL, post_id TEXT NOT NULL DEFAULT '', followers INTEGER, following INTEGER, views INTEGER, likes INTEGER, comments INTEGER, shares INTEGER, source TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(profile_id,date,post_id));")?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS posting_queue (id INTEGER PRIMARY KEY, profile_id INTEGER NOT NULL REFERENCES profiles(id), source_id INTEGER NOT NULL REFERENCES profiles(id), platform TEXT NOT NULL, identity_key TEXT NOT NULL, root TEXT NOT NULL, path TEXT NOT NULL, post_id TEXT NOT NULL DEFAULT '', title TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '', posted_date TEXT NOT NULL DEFAULT '', original_url TEXT NOT NULL DEFAULT '', channel_url TEXT NOT NULL DEFAULT '', state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','posted','checked')), caption TEXT NOT NULL DEFAULT '', post_url TEXT NOT NULL DEFAULT '', scheduled_for TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL DEFAULT (datetime('now')), updated_at TEXT NOT NULL DEFAULT (datetime('now')), UNIQUE(profile_id,platform,identity_key));")?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS posting_queue_events (id INTEGER PRIMARY KEY, queue_id INTEGER NOT NULL REFERENCES posting_queue(id) ON DELETE CASCADE, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')), event_type TEXT NOT NULL, previous_state TEXT NOT NULL DEFAULT '', state TEXT NOT NULL, caption TEXT NOT NULL DEFAULT '', post_url TEXT NOT NULL DEFAULT '', scheduled_for TEXT NOT NULL DEFAULT '', note TEXT NOT NULL DEFAULT ''); CREATE INDEX IF NOT EXISTS posting_queue_events_queue_id ON posting_queue_events(queue_id,id);")?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS posting_queue_claims (queue_id INTEGER PRIMARY KEY REFERENCES posting_queue(id) ON DELETE CASCADE, profile_id INTEGER NOT NULL REFERENCES profiles(id), token TEXT NOT NULL, claimed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));")?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS auto_queue_rules (profile_id INTEGER PRIMARY KEY REFERENCES profiles(id), enabled INTEGER NOT NULL DEFAULT 0, interval_seconds INTEGER NOT NULL, max_pending INTEGER NOT NULL, filters TEXT NOT NULL, next_run INTEGER, last_result TEXT NOT NULL DEFAULT '', instructions TEXT NOT NULL DEFAULT '', generation INTEGER NOT NULL DEFAULT 0);")?;
    let auto_columns = { let mut q=db.prepare("PRAGMA table_info(auto_queue_rules)")?; q.query_map([],|r|r.get::<_,String>(1))?.collect::<rusqlite::Result<Vec<_>>>()? };
    if !auto_columns.iter().any(|name|name=="instructions") { db.execute("ALTER TABLE auto_queue_rules ADD COLUMN instructions TEXT NOT NULL DEFAULT ''",[])?; }
    if !auto_columns.iter().any(|name|name=="generation") { db.execute("ALTER TABLE auto_queue_rules ADD COLUMN generation INTEGER NOT NULL DEFAULT 0",[])?; }
    db.execute_batch("CREATE TABLE IF NOT EXISTS analytics_bridge(profile_id INTEGER PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,enabled INTEGER NOT NULL DEFAULT 0,interval_seconds INTEGER NOT NULL DEFAULT 14400,after_post INTEGER NOT NULL DEFAULT 0,next_run INTEGER,last_attempt INTEGER,last_success INTEGER,last_error TEXT NOT NULL DEFAULT '',job_id TEXT,lease_until INTEGER); CREATE TABLE IF NOT EXISTS analytics_bridge_snapshots(id INTEGER PRIMARY KEY,profile_id INTEGER NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,observed_at TEXT NOT NULL,followers INTEGER,following INTEGER,likes INTEGER,followers_approx INTEGER NOT NULL DEFAULT 0,following_approx INTEGER NOT NULL DEFAULT 0,likes_approx INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS analytics_bridge_links(profile_id INTEGER NOT NULL REFERENCES profiles(id) ON DELETE CASCADE, url TEXT NOT NULL, captured_at TEXT NOT NULL DEFAULT (datetime('now')), PRIMARY KEY(profile_id,url));")?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS source_date_results(profile_id INTEGER NOT NULL REFERENCES profiles(id),root TEXT NOT NULL,path TEXT NOT NULL,post_id TEXT NOT NULL DEFAULT '',posted_date TEXT NOT NULL DEFAULT '',date_kind TEXT NOT NULL DEFAULT '',title TEXT NOT NULL DEFAULT '',description TEXT NOT NULL DEFAULT '',original_url TEXT NOT NULL DEFAULT '',file_size INTEGER NOT NULL DEFAULT -1,file_modified INTEGER NOT NULL DEFAULT -1,checked_at TEXT NOT NULL DEFAULT (datetime('now')),PRIMARY KEY(profile_id,root,path)); CREATE TABLE IF NOT EXISTS source_date_audits(id INTEGER PRIMARY KEY,state TEXT NOT NULL,online INTEGER NOT NULL,processed INTEGER NOT NULL DEFAULT 0,total INTEGER NOT NULL DEFAULT 0,verified INTEGER NOT NULL DEFAULT 0,estimated INTEGER NOT NULL DEFAULT 0,unknown INTEGER NOT NULL DEFAULT 0,failed INTEGER NOT NULL DEFAULT 0,cancel_requested INTEGER NOT NULL DEFAULT 0,message TEXT NOT NULL DEFAULT '',started TEXT NOT NULL DEFAULT (datetime('now')),finished TEXT);")?;
    let result_columns = {
        let mut q = db.prepare("PRAGMA table_info(source_date_results)")?;
        let rows = q
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for col in ["file_size", "file_modified"] {
        if !result_columns.iter().any(|x| x == col) {
            db.execute(
                &format!(
                    "ALTER TABLE source_date_results ADD COLUMN {col} INTEGER NOT NULL DEFAULT -1"
                ),
                [],
            )?;
        }
    }
    db.execute_batch("CREATE TABLE IF NOT EXISTS media_edits (id TEXT PRIMARY KEY, profile_id INTEGER NOT NULL REFERENCES profiles(id), role TEXT NOT NULL, root TEXT NOT NULL, original_path TEXT NOT NULL, edited_path TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (datetime('now')));")?;
    let has_edit_id = { let mut q=db.prepare("PRAGMA table_info(posting_queue)")?; let names=q.query_map([],|r|r.get::<_,String>(1))?.collect::<rusqlite::Result<Vec<_>>>()?; names.iter().any(|x|x=="edit_id") };
    if !has_edit_id { db.execute("ALTER TABLE posting_queue ADD COLUMN edit_id TEXT NOT NULL DEFAULT ''",[])?; }
    let has_queue_revision = {
        let mut s = db.prepare("PRAGMA table_info(posting_queue)")?;
        let names=s.query_map([],|r|r.get::<_,String>(1))?.collect::<rusqlite::Result<Vec<_>>>()?;
        names.iter().any(|x|x=="revision")
    };
    if !has_queue_revision { db.execute("ALTER TABLE posting_queue ADD COLUMN revision INTEGER NOT NULL DEFAULT 0",[])?; }
    let has_date_kind = {
        let mut s = db.prepare("PRAGMA table_info(posting_queue)")?;
        let names = s
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        names.iter().any(|x| x == "posted_date_kind")
    };
    if !has_date_kind {
        db.execute(
            "ALTER TABLE posting_queue ADD COLUMN posted_date_kind TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    #[cfg(target_os = "macos")]
    thumbnail_cache::init(db)?;
    Ok(())
}
fn text_field(map: &HashMap<String, String>, key: &str) -> String {
    map.get(key)
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}
fn valid_posted(s: &str) -> bool {
    s.is_empty()
        || NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
        || DateTime::parse_from_rfc3339(s).is_ok()
}
fn path_part(s: &str) -> String {
    let x: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(80)
        .collect();
    if x.is_empty() { "unknown".into() } else { x }
}
fn bucket(posted: &str, cutoff: &str) -> &'static str {
    let Ok(c) = DateTime::parse_from_rfc3339(cutoff) else {
        return "review";
    };
    if let Ok(d) = DateTime::parse_from_rfc3339(posted) {
        return if d < c { "before" } else { "after" };
    }
    if let Ok(d) = NaiveDate::parse_from_str(posted, "%Y-%m-%d") {
        return if d < c.date_naive() {
            "before"
        } else if d > c.date_naive() {
            "after"
        } else {
            "review"
        };
    }
    "review"
}
fn items(db: &Connection, cutoff: &str) -> rusqlite::Result<Vec<Media>> {
    let mut stmt = db.prepare(
        "SELECT id,platform,account,post_id,source_url,posted,caption FROM items ORDER BY id DESC",
    )?;
    let mut all = stmt
        .query_map([], |r| {
            Ok(Media {
                id: r.get(0)?,
                platform: r.get(1)?,
                account: r.get(2)?,
                post_id: r.get(3)?,
                source_url: r.get(4)?,
                posted: r.get(5)?,
                caption: r.get(6)?,
                category: String::new(),
                files: vec![],
                statuses: vec![],
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for item in &mut all {
        item.category = bucket(&item.posted, cutoff).into();
        let mut s = db.prepare("SELECT name,path,kind FROM files WHERE item_id=? ORDER BY id")?;
        item.files = s
            .query_map([item.id], |r| {
                Ok(FileInfo {
                    name: r.get(0)?,
                    url: format!("/media/{}", r.get::<_, String>(1)?),
                    kind: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut s = db.prepare("SELECT p.platform,s.state,s.claimed_by,s.updated_by,s.caption,s.post_url,s.profile_id FROM profile_statuses s JOIN profiles p ON p.id=s.profile_id WHERE s.item_id=? ORDER BY p.id")?;
        item.statuses = s
            .query_map([item.id], |r| {
                Ok(PostStatus {
                    destination: r.get(0)?,
                    state: r.get(1)?,
                    claimed_by: r.get(2)?,
                    updated_by: r.get(3)?,
                    caption: r.get(4)?,
                    post_url: r.get(5)?,
                    profile_id: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    Ok(all)
}
fn profiles(db: &Connection) -> rusqlite::Result<Vec<Profile>> { profiles_by_visibility(db, false) }
fn profiles_by_visibility(db: &Connection, archived: bool) -> rusqlite::Result<Vec<Profile>> {
    let mut stmt = db.prepare(
        "SELECT id,name,platform,url,role,tags,avatar_path,source_folder FROM profiles WHERE archived=? ORDER BY id",
    )?;
    stmt.query_map([archived], |r| {
        let raw: String = r.get(5)?;
        Ok(Profile {
            id: r.get(0)?,
            name: r.get(1)?,
            platform: r.get(2)?,
            url: r.get(3)?,
            role: r.get(4)?,
            tags: serde_json::from_str(&raw).unwrap_or_default(),
            avatar_url: {
                let path: String = r.get(6)?;
                if path.is_empty() {
                    String::new()
                } else {
                    format!("/media/{path}")
                }
            },
            source_folder: r.get(7)?,
        })
    })?
    .collect()
}
#[derive(Deserialize)]
struct ArchiveInput { archived: bool }
async fn hidden_profiles(State(app): State<App>) -> ApiResult<Json<Vec<Profile>>> {
    let db = app.db.lock().map_err(internal)?;
    Ok(Json(profiles_by_visibility(&db, true).map_err(internal)?))
}
async fn archive_profile(State(app): State<App>, AxumPath(id): AxumPath<i64>, Json(input): Json<ArchiveInput>) -> ApiResult<StatusCode> {
    let mut db = app.db.lock().map_err(internal)?;
    let tx = db.transaction().map_err(internal)?;
    let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM profiles WHERE id=?)", [id], |r| r.get(0)).map_err(internal)?;
    if !exists { return Err((StatusCode::NOT_FOUND, "Account not found.".into())); }
    if input.archived {
        let busy: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM source_sync_runs WHERE profile_id=? AND state='running') OR EXISTS(SELECT 1 FROM source_queue WHERE profile_id=? AND state IN ('starting','running')) OR EXISTS(SELECT 1 FROM analytics_bridge WHERE profile_id=? AND lease_until>unixepoch()) OR EXISTS(SELECT 1 FROM posting_queue_claims c JOIN posting_queue q ON q.id=c.queue_id WHERE q.profile_id=? OR q.source_id=?)", params![id,id,id,id,id], |r| r.get(0)).map_err(internal)?;
        if busy { return Err((StatusCode::CONFLICT, "This account has a sync, refresh or posting claim in progress. Resolve it before removing the account.".into())); }
        tx.execute("UPDATE source_schedules SET enabled=0,next_run=NULL WHERE profile_id=?", [id]).map_err(internal)?;
        tx.execute("UPDATE analytics_bridge SET enabled=0,next_run=NULL WHERE profile_id=?", [id]).map_err(internal)?;
        tx.execute("UPDATE source_queue SET state='cancelled',message='Account removed from workspace' WHERE profile_id=? AND state='pending'", [id]).map_err(internal)?;
        tx.execute("UPDATE auto_queue_rules SET enabled=0,next_run=NULL WHERE profile_id=?", [id]).map_err(internal)?;
    }
    tx.execute("UPDATE profiles SET archived=? WHERE id=?", params![input.archived,id]).map_err(internal)?;
    tx.commit().map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn upload_avatar(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    mut form: Multipart,
) -> ApiResult<StatusCode> {
    let field = form
        .next_field()
        .await
        .map_err(bad)?
        .ok_or_else(|| bad("Choose a profile picture."))?;
    if field.name() != Some("photo") {
        return Err(bad("Choose a profile picture."));
    }
    let bytes = field.bytes().await.map_err(bad)?;
    if bytes.is_empty() || bytes.len() > 5 * 1024 * 1024 {
        return Err(bad("Choose a picture smaller than 5 MB."));
    }
    let ext = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "jpg"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "gif"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "webp"
    } else {
        return Err(bad("Choose a JPG, PNG, GIF or WebP image."));
    };
    let db = app.db.lock().map_err(internal)?;
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM profiles WHERE id=?)",
            [id],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if !exists {
        return Err((StatusCode::NOT_FOUND, "Profile not found.".into()));
    }
    let relative = format!("profile-photos/{}.{}", Uuid::new_v4(), ext);
    let path = app.media.join(&relative);
    std::fs::create_dir_all(app.media.join("profile-photos")).map_err(internal)?;
    std::fs::write(&path, bytes).map_err(internal)?;
    if let Err(e) = db.execute(
        "UPDATE profiles SET avatar_path=? WHERE id=?",
        params![relative, id],
    ) {
        let _ = std::fs::remove_file(path);
        return Err(internal(e));
    }
    Ok(StatusCode::NO_CONTENT)
}
fn validate_profile(p: &ProfileInput) -> ApiResult<(String, String)> {
    if !p.source_folder.trim().is_empty() && !source::safe_folder_name(p.source_folder.trim()) {
        return Err(bad("The source folder must be a single folder name."));
    }
    if p.name.trim().is_empty()
        || p.name.len() > 100
        || !["tiktok", "instagram", "youtube", "other"].contains(&p.platform.as_str())
        || !["source", "destination"].contains(&p.role.as_str())
    {
        return Err(bad("Enter a profile name, platform, link and type."));
    }
    let without_extras = p.url.trim().split(['?', '#']).next().unwrap_or("");
    let parsed: axum::http::Uri = without_extras
        .parse()
        .map_err(|_| bad("Enter a full profile link beginning with https://."))?;
    if parsed.scheme_str() != Some("https")
        || parsed.host().is_none()
        || parsed.authority().is_some_and(|a| a.as_str().contains('@'))
    {
        return Err(bad(
            "Enter a valid https:// profile link without credentials.",
        ));
    }
    let normalized = without_extras.trim_end_matches('/').to_owned();
    let mut tags: Vec<String> = Vec::new();
    for tag in &p.tags {
        let t = tag.trim();
        if !t.is_empty() && t.len() <= 50 && !tags.iter().any(|x| x.eq_ignore_ascii_case(t)) {
            tags.push(t.to_owned());
        }
    }
    if tags.len() > 20 {
        return Err(bad("Use no more than 20 tags."));
    }
    Ok((normalized, serde_json::to_string(&tags).map_err(internal)?))
}
async fn add_profile(
    State(app): State<App>,
    Json(p): Json<ProfileInput>,
) -> ApiResult<impl IntoResponse> {
    let (url, tags) = validate_profile(&p)?;
    let mut db = app.db.lock().map_err(internal)?;
    if db
        .query_row("SELECT id FROM profiles WHERE url=?", [&url], |r| {
            r.get::<_, i64>(0)
        })
        .optional()
        .map_err(internal)?
        .is_some()
    {
        return Err((
            StatusCode::CONFLICT,
            "That profile link is already added. Check Removed accounts if you previously removed it.".into(),
        ));
    }
    let tx = db.transaction().map_err(internal)?;
    tx.execute(
        "INSERT INTO profiles(name,platform,url,role,tags,source_folder) VALUES(?,?,?,?,?,?)",
        params![
            p.name.trim().trim_start_matches('@'),
            p.platform,
            url,
            p.role,
            tags,
            p.source_folder.trim()
        ],
    )
    .map_err(internal)?;
    let id = tx.last_insert_rowid();
    if p.role == "source" && p.setup_selection { tx.execute("INSERT OR REPLACE INTO settings(key,value) VALUES(?, '1')",[format!("source_needs_selection_{id}")]).map_err(internal)?; }
    if p.role == "destination" {
        tx.execute(
            "INSERT INTO profile_statuses(item_id,profile_id) SELECT id,? FROM items",
            [id],
        )
        .map_err(internal)?;
    }
    tx.commit().map_err(internal)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"id":id}))))
}
async fn edit_profile(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    Json(p): Json<ProfileInput>,
) -> ApiResult<StatusCode> {
    let (url, tags) = validate_profile(&p)?;
    let db = app.db.lock().map_err(internal)?;
    let existing: Option<(String, String)> = db
        .query_row("SELECT platform,role FROM profiles WHERE id=?", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()
        .map_err(internal)?;
    let Some((platform, role)) = existing else {
        return Err((StatusCode::NOT_FOUND, "Profile not found.".into()));
    };
    if platform != p.platform || role != p.role {
        return Err(bad(
            "The platform and profile type cannot be changed after adding it.",
        ));
    }
    if db
        .query_row(
            "SELECT id FROM profiles WHERE url=? AND id<>?",
            params![url, id],
            |r| r.get::<_, i64>(0),
        )
        .optional()
        .map_err(internal)?
        .is_some()
    {
        return Err((
            StatusCode::CONFLICT,
            "That profile link is already added.".into(),
        ));
    }
    db.execute(
        "UPDATE profiles SET name=?,url=?,tags=?,source_folder=? WHERE id=?",
        params![
            p.name.trim().trim_start_matches('@'),
            url,
            tags,
            p.source_folder.trim(),
            id
        ],
    )
    .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn list(State(app): State<App>) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().map_err(internal)?;
    let cutoff: String = db
        .query_row("SELECT value FROM settings WHERE key='cutoff'", [], |r| {
            r.get(0)
        })
        .map_err(internal)?;
    Ok(Json(
        serde_json::json!({"cutoff":cutoff,"items":items(&db,&cutoff).map_err(internal)?,"profiles":profiles(&db).map_err(internal)?,"categories":categories::list(&db).map_err(internal)?}),
    ))
}
async fn settings(State(app): State<App>, Json(s): Json<Settings>) -> ApiResult<StatusCode> {
    DateTime::parse_from_rfc3339(&s.cutoff)
        .map_err(|_| bad("Use a timestamp with time zone, such as 2026-11-19T00:00:00-06:00."))?;
    app.db
        .lock()
        .map_err(internal)?
        .execute("UPDATE settings SET value=? WHERE key='cutoff'", [s.cutoff])
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn import(State(app): State<App>, mut form: Multipart) -> ApiResult<impl IntoResponse> {
    let mut fields = HashMap::new();
    let mut files = Vec::new();
    let mut total = 0usize;
    while let Some(field) = form.next_field().await.map_err(bad)? {
        let key = field.name().unwrap_or("").to_owned();
        if key == "files" {
            let name = field.file_name().unwrap_or("").to_owned();
            let ext = Path::new(&name)
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let kind = match ext.as_str() {
                "jpg" | "jpeg" | "png" | "webp" | "gif" => "image",
                "mp4" | "webm" | "mov" => "video",
                _ => return Err(bad("Choose JPG, PNG, WebP, GIF, MP4, WebM or MOV files.")),
            };
            let bytes = field.bytes().await.map_err(bad)?;
            total += bytes.len();
            if bytes.is_empty() || total > 250 * 1024 * 1024 {
                return Err(bad("Files must be nonempty and total no more than 250 MB."));
            }
            files.push((name, ext, kind.to_string(), bytes));
        } else {
            fields.insert(key, field.text().await.map_err(bad)?);
        }
    }
    let platform = text_field(&fields, "platform").to_lowercase();
    let account = text_field(&fields, "account");
    let post_id = text_field(&fields, "post_id");
    let posted = text_field(&fields, "posted");
    let source_url = text_field(&fields, "source_url");
    let caption = text_field(&fields, "caption");
    if !["tiktok", "instagram", "other"].contains(&platform.as_str())
        || account.is_empty()
        || post_id.is_empty()
        || files.is_empty()
    {
        return Err(bad(
            "Platform, source account, original post ID, and at least one file are required.",
        ));
    }
    if !valid_posted(&posted) {
        return Err(bad(
            "Use YYYY-MM-DD, or an ISO timestamp with a time-zone offset.",
        ));
    }
    if !source_url.is_empty()
        && !(source_url.starts_with("https://") || source_url.starts_with("http://"))
    {
        return Err(bad(
            "The original link must start with http:// or https://.",
        ));
    }
    let mut db = app.db.lock().map_err(internal)?;
    if let Some(id) = db
        .query_row(
            "SELECT id FROM items WHERE platform=? AND post_id=?",
            params![platform, post_id],
            |r| r.get::<_, i64>(0),
        )
        .optional()
        .map_err(internal)?
    {
        return Err((
            StatusCode::CONFLICT,
            format!("Already in the library as item #{id}. No duplicate was imported."),
        ));
    }
    let tx = db.transaction().map_err(internal)?;
    tx.execute(
        "INSERT INTO items(platform,account,post_id,source_url,posted,caption) VALUES(?,?,?,?,?,?)",
        params![platform, account, post_id, source_url, posted, caption],
    )
    .map_err(internal)?;
    let id = tx.last_insert_rowid();
    let mut created: Vec<PathBuf> = Vec::new();
    let folder = format!("{}/{}", path_part(&platform), path_part(&account));
    std::fs::create_dir_all(app.media.join(&folder)).map_err(internal)?;
    let result: ApiResult<()> = (|| {
        for (name, ext, kind, bytes) in files {
            let filename = format!(
                "{folder}/{}_{}.{}",
                path_part(&post_id),
                Uuid::new_v4(),
                ext
            );
            let path = app.media.join(&filename);
            std::fs::write(&path, bytes).map_err(internal)?;
            created.push(path);
            tx.execute(
                "INSERT INTO files(item_id,name,path,kind) VALUES(?,?,?,?)",
                params![id, name, filename, kind],
            )
            .map_err(internal)?;
        }
        for dest in ["tiktok", "instagram"] {
            tx.execute(
                "INSERT INTO statuses(item_id,destination) VALUES(?,?)",
                params![id, dest],
            )
            .map_err(internal)?;
        }
        tx.execute("INSERT INTO profile_statuses(item_id,profile_id) SELECT ?,id FROM profiles WHERE role='destination'",[id]).map_err(internal)?;
        Ok(())
    })();
    if let Err(e) = result {
        drop(tx);
        for p in created {
            let _ = std::fs::remove_file(p);
        }
        return Err(e);
    }
    if let Err(e) = tx.commit() {
        for p in created {
            let _ = std::fs::remove_file(p);
        }
        return Err(internal(e));
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"id":id}))))
}
async fn metadata(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    Json(m): Json<Metadata>,
) -> ApiResult<StatusCode> {
    if m.account.trim().is_empty() || !valid_posted(m.posted.trim()) {
        return Err(bad("Enter a source account and a valid date or timestamp."));
    }
    if !m.source_url.is_empty()
        && !(m.source_url.starts_with("http://") || m.source_url.starts_with("https://"))
    {
        return Err(bad("Original link must start with http:// or https://."));
    }
    let count = app
        .db
        .lock()
        .map_err(internal)?
        .execute(
            "UPDATE items SET account=?,source_url=?,posted=?,caption=? WHERE id=?",
            params![
                m.account.trim(),
                m.source_url.trim(),
                m.posted.trim(),
                m.caption,
                id
            ],
        )
        .map_err(internal)?;
    if count == 0 {
        return Err((StatusCode::NOT_FOUND, "Item not found.".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}
async fn owner_review_policy(request: axum::extract::Request,next:axum::middleware::Next)->axum::response::Response {
    let target=request.uri().path();
    let lifeinvader_local=target=="/api/items"||target.starts_with("/api/items/")
        ||target=="/api/profiles"||target.starts_with("/api/profiles/")
        ||target=="/api/categories"||target.starts_with("/api/categories/")
        ||target=="/api/source-settings"||target.starts_with("/api/source-settings/")
        ||target=="/api/source-durations"||target=="/api/solo-posts"
        ||target=="/api/source-schedules"||target=="/api/source-batches"
        ||target.starts_with("/api/sources/")
        ||target=="/api/date-audit"||target.starts_with("/api/date-audit/")
        ||target=="/api/editor/render"||target=="/api/settings"
        ||target.starts_with("/api/bridge/");
    let workspace_local=lifeinvader_local||target=="/api/workspace/publish"
        ||target=="/api/workspace/film-roll"||target.starts_with("/api/workspace/film-roll/")
        ||target.starts_with("/api/workspace/studio/")
        ||target=="/api/workspace/thumbnail-cache/clear"
        ||target=="/api/workspace/caption-presets"||target.starts_with("/api/workspace/caption-presets/")
        ||target.starts_with("/api/oauth/")
        ||target=="/api/workspace/triage"||target=="/api/controller/ui-lifetime"
        ||(target.starts_with("/api/clips/")&&(target.ends_with("/draft")||target.ends_with("/platform-draft")));
    if target.starts_with("/api/")&&((request.method()!=axum::http::Method::GET&&request.method()!=axum::http::Method::HEAD&&!workspace_local)
        ||target.starts_with("/api/posting-handoff")
        ||target.starts_with("/api/controller/youtube")||target.starts_with("/api/controller/instagram")||target.starts_with("/api/controller/tiktok")){
        return (axum::http::StatusCode::FORBIDDEN,Json(serde_json::json!({"reason":"owner_review_local_only","publishing_enabled":false}))).into_response();
    }
    next.run(request).await
}
async fn local_only(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let host = request
        .headers()
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let origin = request
        .headers()
        .get("origin")
        .and_then(|v| v.to_str().ok());
    let loopback_host = host
        .strip_prefix("127.0.0.1:")
        .or_else(|| host.strip_prefix("localhost:"))
        .is_some_and(|port| port.parse::<u16>().is_ok());
    let path = request.uri().path();
    let extension_origin = origin.filter(|o| {
        o.starts_with("chrome-extension://")
            && o.len() == "chrome-extension://".len() + 32
            && o["chrome-extension://".len()..]
                .chars()
                .all(|c| ('a'..='p').contains(&c))
    });
    let bridge_path = matches!(
        path,
        "/api/bridge/poll" | "/api/bridge/result" | "/api/bridge/check"
    );
    if !loopback_host
        || origin.is_some_and(|o| {
            o != format!("http://{host}") && !(bridge_path && extension_origin.is_some())
        })
    {
        return (StatusCode::FORBIDDEN, "Local access only").into_response();
    }
    let ext = extension_origin.map(str::to_owned);
    if request.method() == axum::http::Method::OPTIONS && bridge_path && ext.is_some() {
        let mut response = StatusCode::NO_CONTENT.into_response();
        let headers = response.headers_mut();
        headers.insert(
            axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
            ext.unwrap().parse().unwrap(),
        );
        headers.insert(
            axum::http::header::ACCESS_CONTROL_ALLOW_METHODS,
            "GET, POST, OPTIONS".parse().unwrap(),
        );
        headers.insert(
            axum::http::header::ACCESS_CONTROL_ALLOW_HEADERS,
            "content-type, x-lifeinvader-token".parse().unwrap(),
        );
        return response;
    }
    let mut response = next.run(request).await;
    if let Some(o) = ext {
        response.headers_mut().insert(
            axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
            o.parse().unwrap(),
        );
    }
    response
}
async fn update(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    Json(u): Json<Update>,
) -> ApiResult<StatusCode> {
    if u.user.trim().is_empty() || u.user.len() > 80 {
        return Err(bad("Choose a destination and enter your name."));
    }
    let db = app.db.lock().map_err(internal)?;
    let profile_id = if let Some(id) = u.profile_id {
        id
    } else if let Some(destination) = u.destination.as_deref() {
        db.query_row(
            "SELECT id FROM profiles WHERE platform=? AND role='destination' ORDER BY id LIMIT 1",
            [destination],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?
        .ok_or_else(|| bad("Destination not found."))?
    } else {
        return Err(bad("Choose a destination profile."));
    };
    let changed=match u.action.as_str() {
        "claim"=>db.execute("UPDATE profile_statuses SET state='claimed',claimed_by=?,updated_by=? WHERE item_id=? AND profile_id=? AND state='available'",params![u.user,u.user,id,profile_id]),
        "release"=>db.execute("UPDATE profile_statuses SET state='available',claimed_by='',updated_by=? WHERE item_id=? AND profile_id=? AND state='claimed' AND claimed_by=?",params![u.user,id,profile_id,u.user]),
        "posted"=>{
            let link=u.post_url.unwrap_or_default(); if !link.is_empty() && !(link.starts_with("http://")||link.starts_with("https://")){return Err(bad("Post link must start with http:// or https://."));}
            db.execute("UPDATE profile_statuses SET state='reported_posted',updated_by=?,caption=?,post_url=? WHERE item_id=? AND profile_id=? AND state='claimed' AND claimed_by=?",params![u.user,u.caption.unwrap_or_default(),link,id,profile_id,u.user])
        },
        "reset"=>db.execute("UPDATE profile_statuses SET state='available',claimed_by='',updated_by=? WHERE item_id=? AND profile_id=? AND state='reported_posted'",params![u.user,id,profile_id]),
        _=>return Err(bad("Unknown action."))
    }.map_err(internal)?;
    if changed == 0 {
        return Err((
            StatusCode::CONFLICT,
            "This item is unavailable or claimed by someone else. Refresh the library.".into(),
        ));
    }
    if u.action == "posted" {
        db.execute("UPDATE analytics_bridge SET next_run=unixepoch() WHERE profile_id=? AND enabled=1 AND after_post=1",[profile_id]).map_err(internal)?;
    }
    Ok(StatusCode::NO_CONTENT)
}
async fn index() -> Html<String> {
    let _source=include_str!("../web/index.html");
    #[cfg(feature="snapmatic-hud")]
    return Html(r#"<meta http-equiv="refresh" content="0;url=/workspace?view=studio"><a href="/workspace?view=studio">Open Snapmatic Studio</a>"#.to_owned());
    #[cfg(not(feature="snapmatic-hud"))]
    Html(_source.replace(r#"<a href="/workspace">Film Roll</a>"#,""))
}
async fn css() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../web/style.css"),
    )
}
async fn js() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/javascript; charset=utf-8",
        )],
        include_str!("../web/app.js"),
    )
}
async fn chart_js() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/javascript; charset=utf-8",
        )],
        include_str!("../web/chart.umd.min.js"),
    )
}
async fn library_js() -> impl IntoResponse {
    ([(axum::http::header::CONTENT_TYPE, "text/javascript; charset=utf-8")], include_str!("../web/library.js"))
}
async fn previews_js() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/javascript; charset=utf-8",
        )],
        include_str!("../web/previews.js"),
    )
}

#[cfg(windows)]
fn show_error(message: &str) {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: isize, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    let text: Vec<u16> = std::ffi::OsStr::new(message)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let title: Vec<u16> = std::ffi::OsStr::new("Snapmatic")
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        MessageBoxW(0, text.as_ptr(), title.as_ptr(), 0x10);
    }
}

#[cfg(windows)]
fn launch_window(port:u16, isolated_profile:Option<&Path>) -> Result<std::process::Child, Box<dyn std::error::Error>> {
    let candidates = ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"];
    let edge = candidates
        .iter()
        .filter_map(std::env::var_os)
        .map(|root| PathBuf::from(root).join("Microsoft/Edge/Application/msedge.exe"))
        .find(|path| path.is_file())
        .ok_or(
            "Microsoft Edge is required to open the Lifeinvader window, but it was not found.",
        )?;
    let profile = match isolated_profile {
        Some(profile)=>profile.to_path_buf(),
        None=>PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("Local app data folder was not found.")?)
            .join("LifeinvaderHub/WindowProfile"),
    };
    Ok(std::process::Command::new(edge)
        .arg(if cfg!(feature="snapmatic-hud"){format!("--app=http://127.0.0.1:{port}/workspace?view=studio")}else{format!("--app=http://127.0.0.1:{port}/")})
        .arg("--new-window")
        .arg(format!("--user-data-dir={}", profile.display()))
        .args([
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-mode",
            "--window-size=1440,900",
        ])
        .spawn()?)
}

#[cfg(all(windows,feature="snapmatic-hud"))]
struct SnapmaticInstance(windows_sys::Win32::Foundation::HANDLE);
#[cfg(all(windows,feature="snapmatic-hud"))]
impl Drop for SnapmaticInstance{
    fn drop(&mut self){ unsafe{windows_sys::Win32::Foundation::CloseHandle(self.0);}}
}
#[cfg(all(windows,feature="snapmatic-hud"))]
impl SnapmaticInstance{
    fn open(fixture:bool)->std::io::Result<Self>{
        use windows_sys::Win32::Foundation::{GetLastError,ERROR_ALREADY_EXISTS};
        #[link(name="kernel32")]
        unsafe extern "system"{
            fn CreateMutexW(attributes:*const std::ffi::c_void,owner:i32,name:*const u16)->windows_sys::Win32::Foundation::HANDLE;
        }
        let name=if fixture{r"Local\Snapmatic_OwnerReview_ReopenFixture_43855"}else{r"Local\Snapmatic_OwnerReview_Instance"};
        let name=name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let (raw,last)=unsafe{let raw=CreateMutexW(std::ptr::null(),0,name.as_ptr());(raw,GetLastError())};
        if raw.is_null(){return Err(std::io::Error::from_raw_os_error(last as i32));}
        let mutex=Self(raw);
        if last==ERROR_ALREADY_EXISTS{return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists,"SNAPMATIC_INSTANCE_ALREADY_ACTIVE"));}
        Ok(mutex)
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        let message = format!(
            "Could not start Snapmatic.\n\n{e}\n\nIf it is already open, close the other copy and try again."
        );
        eprintln!("{message}");
        #[cfg(windows)]
        if !std::env::args().any(|arg| arg == "--server-only") {
            show_error(&message);
        }
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let server_only = std::env::args().any(|arg| arg == "--server-only");
    #[cfg(windows)]
    if !server_only {
        // Keep data next to the program even when launched from a desktop shortcut.
        if let Some(parent) = std::env::current_exe()?.parent() {
            std::env::set_current_dir(parent)?;
        }
    }
    #[cfg(all(windows,feature="snapmatic-hud"))]
    let reopen_fixture = !server_only && std::env::args().any(|a|a=="--snapmatic-reopen-fixture") &&
        std::env::var_os("LIFEINVADER_DB").is_some_and(|p|{
            let path=PathBuf::from(p);
            path.is_absolute() && path.file_name().is_some_and(|n|n=="lifeinvader.sqlite3") &&
                path.parent().and_then(Path::file_name).is_some_and(|n|n=="snapmatic-best-practice-reopen-fixture") &&
                controller_native_launcher::clean_path(&path).is_ok()
        }) && std::env::var_os("LIFEINVADER_YOUTUBE_DUE_GUARDIAN").is_none() &&
        std::env::var_os("LIFEINVADER_PHASE4_SCOPE").is_none() && std::env::var_os("LIFEINVADER_PHASE4_ADMISSION_SHA256").is_none();
    #[cfg(not(all(windows,feature="snapmatic-hud")))]
    let reopen_fixture=false;
    if std::env::args().any(|a|a=="--snapmatic-reopen-fixture")&&!reopen_fixture{
        return Err("REOPEN_FIXTURE_SCOPE_INVALID".into());
    }
    let port = if reopen_fixture {43855} else if server_only {
        std::env::var("LIFEINVADER_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(3847)
    } else {
        3847
    };
    // This OS session guard precedes all local database and external provider activity.
    #[cfg(all(windows,feature="snapmatic-hud"))]
    let _desktop_instance=if !server_only{Some(SnapmaticInstance::open(reopen_fixture)?)}else{None};
    // Claim the local port before opening or migrating the database.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await
        .map_err(|e|std::io::Error::new(e.kind(),format!("BIND_FAILED 127.0.0.1:{port}: {e}")))?;
    eprintln!("BIND_OK 127.0.0.1:{port}");
    let args = std::env::args().collect::<Vec<_>>();
    let isolated_review = args.iter().any(|s|s=="--isolated-review-plan" || s=="--isolated-review-sha256");
    let configured_db = std::env::var_os("LIFEINVADER_DB");
    if isolated_review && configured_db.is_none() {
        return Err("Isolated review requires an explicit seeded LIFEINVADER_DB filesystem path.".into());
    }
    let configured_db = PathBuf::from(configured_db.unwrap_or_else(|| "data/lifeinvader.sqlite3".into()));
    if isolated_review && !configured_db.is_absolute() {
        return Err("Isolated review database path must be absolute.".into());
    }
    #[cfg(windows)]
    if isolated_review && !matches!(
        configured_db.components().next(), Some(std::path::Component::Prefix(p)) if matches!(p.kind(),std::path::Prefix::Disk(_))
    ){
        return Err("Isolated review requires a plain drive-qualified SQLite file path (not URI or device prefix).".into());
    }
    let db_path = std::path::absolute(configured_db)?;
    if isolated_review && !std::fs::symlink_metadata(&db_path).is_ok_and(|m|m.is_file()){
        return Err("Isolated review seeded database is missing or not a regular file.".into());
    }
    let media =
        PathBuf::from(std::env::var("LIFEINVADER_MEDIA").unwrap_or_else(|_| "media".into()));
    if let Some(p) = db_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(p)?;
    }
    // Explicit media paths must already exist, so a disconnected external disk isn't silently replaced.
    if std::env::var_os("LIFEINVADER_MEDIA").is_some() && !media.is_dir() {
        return Err("Configured media folder does not exist or is not mounted".into());
    }
    std::fs::create_dir_all(&media)?;
    let log_dir = db_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .join("sync-logs");
    let controller_catalog = controller_source::provision_catalog(&args).map_err(std::io::Error::other)?;
    let controller_manual = controller_manual::provision(&args, &db_path, &media).map_err(|e|std::io::Error::other(e.0))?;
    let mut db = Connection::open(&db_path)?;
    db.busy_timeout(std::time::Duration::from_millis(2500))?;
    init(&db)?;
    #[cfg(feature="snapmatic-hud")]
    workspace_film_roll::init(&db)?;
    #[cfg(feature="snapmatic-hud")]
    workspace_oauth::init(&db)?;
    controller_store::init(&db).map_err(|e|std::io::Error::other(e.0))?;
    controller_store::recover(&mut db, chrono::Utc::now().timestamp()).map_err(|e|std::io::Error::other(e.0))?;
    let cache_scope = {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        std::fs::canonicalize(&db_path).unwrap_or(db_path.clone()).hash(&mut hash);
        format!("{:016x}", hash.finish())
    };
    db.execute("UPDATE source_date_audits SET state='interrupted',finished=datetime('now'),message='The app closed before the audit finished.' WHERE state='running'",[])?;
    db.execute("UPDATE source_sync_runs SET state='interrupted', finished=datetime('now') WHERE state='running'",[])?;
    db.execute("UPDATE source_queue SET state='interrupted',message='App closed before queued sync finished' WHERE state IN ('pending','starting','running')",[])?;
    let app = App {
        db: Arc::new(Mutex::new(db)),
        media: media.clone(),
        log_dir,
        cache_scope,
        controller_ui: Arc::new(controller_ui_lifetime::Lifetime::new(
            controller_manual.configured() || (!server_only && std::env::var_os("LIFEINVADER_INTEGRATION_REVIEW").is_some()))),
        controller_catalog,
        controller_manual,
        controller_ops: Arc::new(controller_ops::Unavailable),
        controller_worker: Arc::new(controller_worker::Unavailable),
    };
    #[cfg(target_os = "macos")]
    let _cache_worker = tokio::spawn(thumbnail_cache::worker(app.clone()));
    // Review validation must never wake the original source-sync or auto-queue schedules.
    let drive_intake = {
        let db = app.db.lock().map_err(|_| std::io::Error::other("source settings unavailable"))?;
        source::config(&db)?.root.eq_ignore_ascii_case(source::DRIVE_SOURCE_ROOT)
    };
    let review_only = true; // Owner-review builds never start background posting or sync.
    #[cfg(all(windows,feature="snapmatic-hud"))]
    let triage_fixture=server_only&&port==43850&&review_only&&args.iter().any(|a|a=="--snapmatic-triage-fixture")&&workspace_triage::fixture_enabled(&app);
    let scheduler = (!drive_intake).then(|| tokio::spawn(scheduler::worker(app.clone())));
    let auto_scheduler = (!review_only).then(|| tokio::spawn(auto_queue::worker(app.clone())));
    #[cfg(all(windows,feature="snapmatic-hud"))]
    let phase4_fixture=server_only&&args.iter().any(|s|s=="--phase4-fixture")&&workspace_phase4::fixture_enabled(&app);
    #[cfg(all(windows,feature="snapmatic-hud"))]
    let phase4_watcher=if phase4_fixture||(!server_only&&drive_intake&&app.controller_ui.enabled()){
        workspace_phase4::start(&app)
    }else{None};
    #[cfg(windows)]
    let sync_db = app.db.clone();
    #[cfg(windows)]
    let ui_lifetime=app.controller_ui.clone();
    #[cfg(windows)]
    let manual_shutdown=app.controller_manual.clone();
    let router = Router::new().merge(controller::routes());
    #[cfg(feature="snapmatic-hud")]
    let router = router.merge(workspace::routes(app.clone()));
    #[cfg(all(windows,feature="snapmatic-hud"))]
    let reopen_profile=if !server_only && review_only && (drive_intake || reopen_fixture) && !isolated_review
        && std::env::var_os("LIFEINVADER_SNAPMATIC_REOPEN").as_deref()==Some(std::ffi::OsStr::new("1")) {
        let profile_name=if reopen_fixture{"desktop-window-snapmatic-current".into()}else{
            let raw=std::env::var("LIFEINVADER_SNAPMATIC_DESKTOP_PROFILE").map_err(|_|"Snapmatic desktop profile is not pinned")?;
            if raw.strip_prefix("desktop-window-").and_then(|id|uuid::Uuid::parse_str(id).ok().filter(|value|value.to_string()==id)).is_none(){
                return Err("SNAPMATIC_DESKTOP_PROFILE_INVALID".into());
            }
            raw
        };
        let folder=db_path.parent().ok_or("No Snapmatic database parent")?.join(profile_name);
        if !reopen_fixture && !folder.is_dir(){return Err("SNAPMATIC_DESKTOP_PROFILE_MISSING".into());}
        if folder.try_exists()? { controller_native_launcher::clean_path(&folder).map_err(|e|std::io::Error::other(e.0))?; }
        else { std::fs::create_dir(&folder)?; }
        Some(folder)
    }else{None};
    #[cfg(all(windows,feature="snapmatic-hud"))]
    let router={
        let owned=reopen_profile.is_some();
        router.route("/api/workspace/desktop-identity",get(move||async move{
            if !owned{return axum::http::StatusCode::NOT_FOUND.into_response();}
            ([(axum::http::header::CACHE_CONTROL,"no-store")],Json(serde_json::json!({
               "schema":"snapmatic-desktop-owner-v1","pid":std::process::id(),
               "workspace":if reopen_fixture{"reopen-fixture-only"}else{"drive-review-only"},"publishing_enabled":false
            }))).into_response()
        }))
    };
    let router = router.route("/", get(index))
        .route("/style.css", get(css))
        .route("/app.js", get(js))
        .route("/previews.js", get(previews_js))
        .route("/library.js", get(library_js))
        .route("/chart.js", get(chart_js))
        .route("/api/items", get(list).post(import))
        .route("/api/posting-handoff", get(handoff::get))
        .route("/api/posting-handoff/prompt", get(handoff::prompt))
        .route("/api/posting-handoff/results", post(handoff::results))
        .route("/api/posting-handoff/claim", post(handoff::claim))
        .route("/api/auto-queue", get(auto_queue::list))
        .route("/api/profiles/{id}/auto-queue", post(auto_queue::save))
        .route("/api/profiles", post(add_profile))
        .route("/api/profiles/hidden", get(hidden_profiles))
        .route("/api/profiles/{id}/archive", post(archive_profile))
        .route("/api/categories", post(categories::create))
        .route("/api/categories/appearance", post(categories::appearance))
        .route("/api/categories/photo", post(categories::photo))
        .route("/api/categories/members", post(categories::set_members))
        .route("/api/categories/update", post(categories::update))
        .route("/api/profiles/{id}", post(edit_profile))
        .route("/api/profiles/{id}/photo", post(upload_avatar))
        .route(
            "/api/profiles/{id}/analytics-bridge",
            get(bridge::config).post(bridge::save_config),
        )
        .route(
            "/api/profiles/{id}/analytics-bridge/refresh",
            post(bridge::refresh),
        )
        .route("/api/bridge/pair", get(bridge::pair).delete(bridge::reset))
        .route("/api/bridge/poll", get(bridge::poll))
        .route("/api/bridge/check", get(bridge::check))
        .route("/api/bridge/result", post(bridge::result))
        .route(
            "/api/profiles/{id}/schedule",
            post(scheduler::save_profile_schedule),
        )
        .route(
            "/api/profiles/{id}/analytics",
            get(analytics::get).post(analytics::add),
        )
        .route(
            "/api/profiles/{id}/analytics/import",
            post(analytics::import),
        )
        .route(
            "/api/profiles/{id}/queue",
            get(queue::list).post(queue::add),
        )
        .route("/api/profiles/{id}/queue/{qid}", post(queue::update))
        .route("/api/profiles/{id}/queue/{qid}/events", get(handoff::events))
        .route("/api/profiles/{id}/queue/{qid}/release", post(handoff::release))
        .route("/api/profiles/{id}/sync-recovery", get(recovery::get).post(recovery::save))
        .route(
            "/api/profiles/{id}/downloads",
            get(source::profile_overview),
        )
        .route(
            "/api/profiles/{id}/download-status",
            get(source::profile_status),
        )
        .route(
            "/api/profiles/{id}/download-file",
            get(source::profile_file),
        )
        .route(
            "/api/profiles/{id}/download-thumbnail",
            get(source::profile_thumbnail),
        )
        .route(
            "/api/profiles/{id}/download-sync",
            post(source::profile_sync),
        )
        .route(
            "/api/profiles/{id}/downloads-seen",
            post(source::profile_seen),
        )
        .route(
            "/api/source-settings",
            get(source::get_config).post(source::save_config),
        )
        .route("/api/source-settings/browse", post(source::browse_folder))
        .route("/api/source-durations", post(source::save_durations))
        .route("/api/solo-posts", post(solo::start))
        .route("/api/sources/{id}/preview", post(selection::preview))
        .route("/api/sources/{id}/selection", post(selection::choose))
        .route("/api/sources/discover", get(source::discover))
        .route("/api/sources/cancel-sync", post(source::cancel_sync))
        .route("/api/sources/{id}/sync-recovery", get(recovery::get_source).post(recovery::save_source))
        .route(
            "/api/date-audit",
            get(date_audit::status).post(date_audit::start),
        )
        .route("/api/date-audit/cancel", post(date_audit::cancel))
        .route("/api/sources/{id}", get(source::overview))
        .route("/api/sources/{id}/browse", get(source::interactive_overview))
        .route("/api/sources/{id}/status", get(source::status))
        .route("/api/sources/{id}/summary", get(source::summary))
        .route("/api/sources/{id}/file", get(source::file))
        .route("/api/sources/{id}/thumbnail", get(source::thumbnail))
        .route("/api/sources/{id}/sync", post(source::sync))
        .route("/api/sources/{id}/seen", post(source::mark_seen))
        .route("/api/sources/{id}/schedule", post(scheduler::save_schedule))
        .route(
            "/api/sources/{id}/batch-setting",
            post(scheduler::save_batch_setting),
        )
        .route("/api/source-schedules", get(scheduler::schedules))
        .route("/api/editor/render", post(editor::render))
        .route("/api/source-batches", post(scheduler::enqueue_batch))
        .route("/api/sources/{id}/dead", post(scheduler::set_dead))
        .route("/api/sources/{id}/folder", get(source::source_folder_location).post(source::open_source_folder))
        .route("/api/profiles/{id}/folder", get(source::profile_folder_location).post(source::open_profile_folder))
        .route("/api/settings", post(settings))
        .route("/api/items/{id}/status", post(update))
        .route("/api/items/{id}/metadata", post(metadata))
        .nest_service("/media", ServeDir::new(media))
        .layer(DefaultBodyLimit::max(255 * 1024 * 1024))
        .layer(axum::middleware::from_fn(local_only))
        .layer(axum::middleware::from_fn(owner_review_policy))
        .with_state(app);
    println!("Snapmatic: http://127.0.0.1:{port}");
    #[cfg(windows)]
    if !server_only {
        let isolated_ui=ui_lifetime.enabled();
        #[cfg(feature="snapmatic-hud")]
        let candidate=reopen_profile;
        #[cfg(not(feature="snapmatic-hud"))]
        let candidate:Option<PathBuf>=None;
        let profile=if candidate.is_some(){candidate}else if isolated_ui {
            let profile=db_path.parent().ok_or("No isolated database parent")?.join(format!("desktop-window-{}",uuid::Uuid::new_v4()));
            std::fs::create_dir(&profile)?;
            Some(profile)
        }else{None};
        let mut window = launch_window(port,profile.as_deref())
            .map_err(|e|std::io::Error::other(format!("UI_BOOTSTRAP_START_FAILED: {e}")))?;
        eprintln!("UI_BOOTSTRAP_STARTED pid={}",window.id());
        let _youtube_guardian=(false && isolated_ui && drive_intake
            && std::env::var("LIFEINVADER_YOUTUBE_DUE_GUARDIAN").ok().as_deref()==Some("1"))
            .then(|| tokio::spawn(controller::youtube_guardian()));
        let db = sync_db;
        let (lifetime_tx,lifetime_rx)=tokio::sync::oneshot::channel();
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let close_reason=if isolated_ui {
                    let reason=controller_ui_lifetime::wait(&ui_lifetime,Some(window)).await;
                    let _=tokio::task::spawn_blocking(move||manual_shutdown.shutdown()).await;
                    Some(reason)
                }else{
                    match tokio::task::spawn_blocking(move || window.wait()).await {
                        Ok(Ok(status))=>eprintln!("UI_BOOTSTRAP_EXIT code={:?}",status.code()),
                        Ok(Err(error))=>eprintln!("UI_BOOTSTRAP_WAIT_ERROR: {error}"),
                        Err(error)=>eprintln!("UI_BOOTSTRAP_JOIN_ERROR: {error}"),
                    }
                    None
                };
                #[cfg(feature="snapmatic-hud")]
                if let Some(w)=phase4_watcher{let _=tokio::task::spawn_blocking(move||w.stop()).await;}
                let _=lifetime_tx.send(close_reason);
                if let Some(worker) = scheduler { worker.abort(); }
                if let Some(worker) = auto_scheduler { worker.abort(); }
                if let Ok(db)=db.lock(){let _=db.execute("UPDATE source_queue SET state='interrupted',message='App closed before queued sync started' WHERE state IN ('pending','starting')",[]);let _=db.execute("UPDATE source_date_audits SET cancel_requested=1 WHERE state='running'",[]);}
                // Keep the service and the archive lock alive until an active download exits.
                loop {
                    let busy = db.lock().ok().and_then(|db|db.query_row("SELECT EXISTS(SELECT 1 FROM source_sync_runs WHERE state='running') OR EXISTS(SELECT 1 FROM source_date_audits WHERE state='running')",[],|r|r.get::<_,bool>(0)).ok()).unwrap_or(false);
                    if !busy { break; }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            })
            .await.map_err(|e|std::io::Error::other(format!("SERVE_FAILED: {e}")))?;
        if let Ok(Some(reason))=lifetime_rx.await {
            if reason!="UI_CLOSE_REQUESTED" {return Err(std::io::Error::other(reason).into());}
        }
        return Ok(());
    }
    #[cfg(not(windows))]
    let _ = server_only;
    #[cfg(all(windows,feature="snapmatic-hud"))]
    if phase4_fixture{
        let run=async move{axum::serve(listener,router).await};
        let secs=if std::env::var_os("LIFEINVADER_PHASE4_ADMISSION_SHA256").is_some(){
            std::env::var("LIFEINVADER_PHASE4_FIXTURE_LIFETIME_SECS").ok()
                .and_then(|s|s.parse::<u64>().ok()).filter(|s|(15..=250).contains(s)).unwrap_or(250)
        }else{115};
        let result=tokio::time::timeout(std::time::Duration::from_secs(secs),run).await;
        if let Some(w)=phase4_watcher{let _=tokio::task::spawn_blocking(move||w.stop()).await;}
        if let Ok(Err(e))=result{return Err(std::io::Error::other(format!("PHASE4_FIXTURE_SERVE_FAILED: {e}")).into());}
        return Ok(());
    }
    #[cfg(all(windows,feature="snapmatic-hud"))]
    if triage_fixture {
        if let Ok(Err(e))=tokio::time::timeout(std::time::Duration::from_secs(35),axum::serve(listener,router)).await {
            return Err(std::io::Error::other(format!("TRIAGE_FIXTURE_SERVE_FAILED: {e}")).into());
        }
        return Ok(());
    }
    axum::serve(listener, router).await.map_err(|e|std::io::Error::other(format!("SERVE_FAILED: {e}")))?;
    if let Some(worker) = scheduler { worker.abort(); }
    if let Some(worker) = auto_scheduler { worker.abort(); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cutoff_needs_precision() {
        let c = "2026-11-19T00:00:00-06:00";
        assert_eq!(bucket("2026-11-18", c), "before");
        assert_eq!(bucket("2026-11-19", c), "review");
        assert_eq!(bucket("2026-11-20", c), "after");
        assert_eq!(bucket("", c), "review");
        assert_eq!(bucket("2026-11-19T05:59:59Z", c), "before");
        assert_eq!(bucket("2026-11-19T06:00:00Z", c), "after");
    }
    #[test]
    fn source_ids_are_strings_and_unique_per_platform() {
        let db = Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        db.execute("INSERT INTO items(platform,account,post_id,source_url,posted,caption) VALUES('tiktok','a','001234567890123456789','','','')",[]).unwrap();
        assert!(db.execute("INSERT INTO items(platform,account,post_id,source_url,posted,caption) VALUES('tiktok','b','001234567890123456789','','','')",[]).is_err());
    }
}
