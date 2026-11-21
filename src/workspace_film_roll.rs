use crate::{ApiResult, App, bad, controller_store as store, internal, source};
use axum::{
    Json,
    extract::{Path as AxumPath, State},
    http::StatusCode,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub(crate) fn init(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_film_roll (
            clip_id TEXT PRIMARY KEY,
            source_id INTEGER NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
            path TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            bytes INTEGER NOT NULL DEFAULT 0,
            added_at INTEGER NOT NULL DEFAULT (unixepoch()),
            UNIQUE(source_id, path)
        );",
    )
}

pub(crate) fn stable_id(source_id: i64, path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"snapmatic-film-roll-v1\0");
    hasher.update(source_id.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(path.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Deserialize)]
pub(crate) struct AddItem {
    path: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct AddRequest {
    source_id: i64,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    items: Vec<AddItem>,
}

fn is_video_path(p: &std::path::Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "mp4" | "webm" | "mov" | "mkv"))
}

pub(crate) async fn list(State(app): State<App>) -> ApiResult<Json<Value>> {
    let db = app.db.lock().map_err(internal)?;
    let mut stmt = db
        .prepare(
            "SELECT f.clip_id, f.source_id, f.path, f.title
             FROM workspace_film_roll f
             JOIN profiles p ON p.id = f.source_id
             WHERE p.archived = 0 AND p.role = 'source'
             ORDER BY f.added_at DESC, f.clip_id",
        )
        .map_err(internal)?;
    let items: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "clip_id": r.get::<_, String>(0)?,
                "source_id": r.get::<_, i64>(1)?,
                "path": r.get::<_, String>(2)?,
                "title": r.get::<_, String>(3)?,
            }))
        })
        .map_err(internal)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(internal)?;
    Ok(Json(json!({ "items": items })))
}

pub(crate) async fn add(
    State(app): State<App>,
    Json(input): Json<AddRequest>,
) -> ApiResult<Json<Value>> {
    let mut pending = input.items;
    if let Some(path) = input.path {
        pending.push(AddItem {
            path,
            title: input.title,
            description: input.description,
        });
    }
    if pending.is_empty() {
        return Err(bad("Choose at least one video to send to Film Roll."));
    }
    if pending.len() > 500 {
        return Err(bad("Send at most 500 videos at once."));
    }

    let mut validated = Vec::with_capacity(pending.len());
    for item in pending {
        let target = source::resolve_file(&app, input.source_id, &item.path, "source")?;
        if !is_video_path(&target) {
            continue;
        }
        let meta = std::fs::metadata(&target).map_err(internal)?;
        let fallback_name = target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&item.path)
            .to_owned();
        let title = item
            .title
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
            .unwrap_or(fallback_name);
        let description = item
            .description
            .map(|d| d.trim().chars().take(2000).collect::<String>())
            .unwrap_or_default();
        let clip_id = stable_id(input.source_id, &item.path);
        validated.push((clip_id, item.path, title, description, meta.len() as i64));
    }
    if validated.is_empty() {
        return Err(bad("Only video files can be added to Film Roll."));
    }

    let mut db = app.db.lock().map_err(internal)?;
    let tx = db.transaction().map_err(internal)?;
    let mut added = 0usize;
    let mut out_items = Vec::with_capacity(validated.len());
    for (clip_id, path, title, description, bytes) in validated {
        let existed: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workspace_film_roll WHERE clip_id=?1)",
                [&clip_id],
                |r| r.get(0),
            )
            .map_err(internal)?;
        tx.execute(
            "INSERT INTO workspace_film_roll(clip_id, source_id, path, title, description, bytes, added_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, unixepoch())
             ON CONFLICT(source_id, path) DO UPDATE SET
               title=excluded.title,
               description=excluded.description,
               bytes=excluded.bytes",
            params![clip_id, input.source_id, path, title, description, bytes],
        )
        .map_err(internal)?;
        if !existed {
            added += 1;
        }
        out_items.push(json!({
            "clip_id": clip_id,
            "source_id": input.source_id,
            "path": path,
            "title": title,
        }));
    }
    tx.commit().map_err(internal)?;
    Ok(Json(json!({ "added": added, "items": out_items })))
}

pub(crate) async fn remove(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
) -> ApiResult<StatusCode> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let db = app.db.lock().map_err(internal)?;
    db.execute("DELETE FROM workspace_film_roll WHERE clip_id=?1", [&clip_id])
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) fn rows(app: &App) -> store::Result<Vec<Value>> {
    let entries: Vec<(String, i64, String, String, String, i64)> = {
        let db = app.db.lock().map_err(|_| store::Error("storage_unavailable"))?;
        let mut stmt = db
            .prepare(
                "SELECT f.clip_id, f.source_id, f.path, f.title, f.description, f.bytes
                 FROM workspace_film_roll f
                 JOIN profiles p ON p.id = f.source_id
                 WHERE p.archived = 0 AND p.role = 'source'
                 ORDER BY f.added_at DESC, f.clip_id
                 LIMIT 500",
            )
            .map_err(|_| store::Error("storage_unavailable"))?;
        let mapped = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            })
            .map_err(|_| store::Error("storage_unavailable"))?;
        mapped
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|_| store::Error("storage_unavailable"))?
    };

    let mut out = Vec::with_capacity(entries.len());
    for (clip_id, source_id, path, title, description, saved_bytes) in entries {
        let Ok(resolved) = source::resolve_file(app, source_id, &path, "source") else {
            continue;
        };
        let bytes = std::fs::metadata(&resolved)
            .map(|m| m.len())
            .unwrap_or(saved_bytes.max(0) as u64);
        let name = if title.is_empty() {
            path.rsplit(['\\', '/']).next().unwrap_or("Downloaded video").to_owned()
        } else {
            title
        };
        let mut row = json!({
            "kind": "original",
            "clip_id": clip_id,
            "source_id": source_id,
            "source_path": path,
            "name": name,
            "state": "pending_review",
            "preview_only": true,
            "preview_available": true,
            "poster_available": true,
            "preview_bytes": bytes,
            "from_sources": true,
        });
        if !description.is_empty() {
            row["import_metadata"] = json!({ "description": description });
        }
        out.push(row);
    }
    Ok(out)
}

pub(crate) fn resolve_source_ref(app: &App, clip_id: &str) -> Option<(i64, String)> {
    let db = app.db.lock().ok()?;
    db.query_row(
        "SELECT f.source_id, f.path
         FROM workspace_film_roll f
         JOIN profiles p ON p.id = f.source_id
         WHERE f.clip_id=?1 AND p.archived=0 AND p.role='source'",
        [clip_id],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
    )
    .optional()
    .ok()?
}

pub(crate) fn resolve_video(app: &App, clip_id: &str) -> Option<PathBuf> {
    let (source_id, path) = resolve_source_ref(app, clip_id)?;
    source::resolve_file(app, source_id, &path, "source").ok()
}
