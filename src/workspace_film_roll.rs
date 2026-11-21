use crate::{ApiResult, App, bad, controller_store as store, internal, source};
use axum::{
    Json,
    extract::{Path as AxumPath, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tower::ServiceExt;
use tower_http::services::ServeFile;

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
            review_state TEXT NOT NULL DEFAULT 'pending_review',
            exported_path TEXT NOT NULL DEFAULT '',
            UNIQUE(source_id, path)
        );",
    )?;
    let cols: Vec<String> = {
        let mut stmt = db.prepare("PRAGMA table_info(workspace_film_roll)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if !cols.iter().any(|c| c == "review_state") {
        db.execute(
            "ALTER TABLE workspace_film_roll ADD COLUMN review_state TEXT NOT NULL DEFAULT 'pending_review'",
            [],
        )?;
    }
    if !cols.iter().any(|c| c == "exported_path") {
        db.execute(
            "ALTER TABLE workspace_film_roll ADD COLUMN exported_path TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    Ok(())
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

#[derive(Deserialize)]
pub(crate) struct StateRequest {
    state: String,
}

#[derive(Deserialize)]
pub(crate) struct ClearRequest {
    #[serde(default)]
    state: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ExportRequest {
    #[serde(default)]
    in_ms: Option<u64>,
    #[serde(default)]
    out_ms: Option<u64>,
    #[serde(default = "default_framing")]
    framing: String,
    #[serde(default)]
    mute: bool,
}

fn default_framing() -> String {
    "original".to_owned()
}

fn is_video_path(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "mp4" | "webm" | "mov" | "mkv"))
}

pub(crate) fn find_ffmpeg() -> Option<PathBuf> {
    if let Some(env_path) = std::env::var_os("LIFEINVADER_FFMPEG") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
        let nested = p.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
        if nested.is_file() {
            return Some(nested);
        }
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let local = dir
            .join("tools")
            .join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
        if local.is_file() {
            return Some(local);
        }
    }
    let cwd_local = PathBuf::from("tools").join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
    if cwd_local.is_file() {
        return Some(cwd_local);
    }
    None
}

pub(crate) fn exports_dir(app: &App) -> PathBuf {
    app.media.join("exports")
}

pub(crate) async fn list(State(app): State<App>) -> ApiResult<Json<Value>> {
    let db = app.db.lock().map_err(internal)?;
    let mut stmt = db
        .prepare(
            "SELECT f.clip_id, f.source_id, f.path, f.title, f.review_state, f.exported_path
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
                "state": r.get::<_, String>(4)?,
                "exported": !r.get::<_, String>(5)?.is_empty(),
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

pub(crate) async fn set_state(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
    Json(input): Json<StateRequest>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let state = match input.state.as_str() {
        "pending_review" | "approved" | "hold" | "exported" => input.state.as_str(),
        _ => return Err(bad("Choose pending_review, approved, hold, or exported.")),
    };
    let db = app.db.lock().map_err(internal)?;
    let updated = db
        .execute(
            "UPDATE workspace_film_roll SET review_state=?1 WHERE clip_id=?2",
            params![state, clip_id],
        )
        .map_err(internal)?;
    if updated == 0 {
        return Err((StatusCode::NOT_FOUND, "Clip not found in Film Roll.".into()));
    }
    Ok(Json(json!({ "clip_id": clip_id, "state": state })))
}

pub(crate) async fn clear(
    State(app): State<App>,
    Json(input): Json<ClearRequest>,
) -> ApiResult<Json<Value>> {
    let db = app.db.lock().map_err(internal)?;
    let removed = match input.state.as_deref() {
        Some(s) if matches!(s, "pending_review" | "approved" | "hold" | "exported") => db
            .execute("DELETE FROM workspace_film_roll WHERE review_state=?1", [s])
            .map_err(internal)?,
        Some("all") | None => db
            .execute("DELETE FROM workspace_film_roll", [])
            .map_err(internal)?,
        _ => return Err(bad("Invalid clear filter.")),
    };
    Ok(Json(json!({ "removed": removed })))
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

fn sanitize_stem(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    let short: String = trimmed.chars().take(48).collect();
    if short.is_empty() { "clip".to_owned() } else { short }
}

pub(crate) async fn export_clip(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
    Json(input): Json<ExportRequest>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    if !matches!(input.framing.as_str(), "original" | "center_crop" | "blurred_fit") {
        return Err(bad("Choose original, center_crop, or blurred_fit framing."));
    }
    if let (Some(a), Some(b)) = (input.in_ms, input.out_ms)
        && a >= b
    {
        return Err(bad("Out time must be after In time."));
    }
    let Some(source_path) = resolve_video(&app, &clip_id) else {
        return Err((StatusCode::NOT_FOUND, "Source video is unavailable.".into()));
    };
    let Some(ffmpeg) = find_ffmpeg() else {
        return Err(bad("FFmpeg was not found in tools/ffmpeg.exe or LIFEINVADER_FFMPEG."));
    };
    let title: String = {
        let db = app.db.lock().map_err(internal)?;
        db.query_row(
            "SELECT title FROM workspace_film_roll WHERE clip_id=?1",
            [&clip_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?
        .unwrap_or_default()
    };
    let out_dir = exports_dir(&app);
    std::fs::create_dir_all(&out_dir).map_err(internal)?;
    let stem = sanitize_stem(if title.is_empty() {
        source_path.file_stem().and_then(|s| s.to_str()).unwrap_or("clip")
    } else {
        &title
    });
    let file_name = format!("{stem}-{}.mp4", &clip_id[..8]);
    let final_path = out_dir.join(&file_name);
    let tmp_path = out_dir.join(format!("{stem}-{}.tmp.mp4", &clip_id[..8]));

    let framing = input.framing.clone();
    let in_ms = input.in_ms;
    let out_ms = input.out_ms;
    let mute = input.mute;
    let src_clone = source_path.clone();
    let tmp_clone = tmp_path.clone();
    let final_clone = final_path.clone();

    let render = tokio::task::spawn_blocking(move || -> Result<u64, String> {
        let mut cmd = std::process::Command::new(&ffmpeg);
        cmd.arg("-y").arg("-hide_banner").arg("-loglevel").arg("error");
        if let Some(start) = in_ms
            && start > 0
        {
            cmd.arg("-ss").arg(format!("{:.3}", start as f64 / 1000.0));
        }
        cmd.arg("-i").arg(&src_clone);
        if let Some(end) = out_ms {
            let start = in_ms.unwrap_or(0);
            if end > start {
                cmd.arg("-t").arg(format!("{:.3}", (end - start) as f64 / 1000.0));
            }
        }
        match framing.as_str() {
            "center_crop" => {
                cmd.arg("-vf").arg("crop='min(iw,ih*9/16)':'min(ih,iw*16/9)',scale=1080:1920:flags=lanczos");
            }
            "blurred_fit" => {
                cmd.arg("-filter_complex").arg("[0:v]split[bg][fg];[bg]scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920,boxblur=20:5[b];[fg]scale=1080:1920:force_original_aspect_ratio=decrease[f];[b][f]overlay=(W-w)/2:(H-h)/2[v]").arg("-map").arg("[v]");
                if !mute {
                    cmd.arg("-map").arg("0:a?");
                }
            }
            _ => {}
        }
        cmd.args(["-c:v", "libx264", "-preset", "fast", "-crf", "20", "-pix_fmt", "yuv420p"]);
        if mute {
            cmd.arg("-an");
        } else {
            cmd.args(["-c:a", "aac", "-b:a", "192k"]);
        }
        cmd.args(["-movflags", "+faststart"]).arg(&tmp_clone);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let out = cmd.output().map_err(|e| format!("Failed to start FFmpeg: {e}"))?;
        if !out.status.success() {
            let _ = std::fs::remove_file(&tmp_clone);
            let msg = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            return Err(if msg.is_empty() {
                "FFmpeg could not render this clip.".to_owned()
            } else {
                msg
            });
        }
        std::fs::rename(&tmp_clone, &final_clone)
            .map_err(|e| format!("Could not finalize exported MP4: {e}"))?;
        let len = std::fs::metadata(&final_clone)
            .map(|m| m.len())
            .map_err(|e| format!("Could not read exported MP4: {e}"))?;
        Ok(len)
    })
    .await
    .map_err(internal)?;

    let bytes = render.map_err(bad)?;
    {
        let db = app.db.lock().map_err(internal)?;
        db.execute(
            "UPDATE workspace_film_roll SET exported_path=?1, review_state='exported' WHERE clip_id=?2",
            params![file_name, clip_id],
        )
        .map_err(internal)?;
    }
    Ok(Json(json!({
        "clip_id": clip_id,
        "file_name": file_name,
        "bytes": bytes,
        "export_url": format!("/api/workspace/film-roll/{clip_id}/export"),
        "state": "exported",
    })))
}

pub(crate) async fn stream_export(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
    request: Request,
) -> Response {
    if !crate::workspace_media::hex(&clip_id) {
        return (StatusCode::NOT_FOUND, Json(json!({"reason": "export_unavailable"}))).into_response();
    }
    let file_name: Option<String> = app
        .db
        .lock()
        .ok()
        .and_then(|db| {
            db.query_row(
                "SELECT exported_path FROM workspace_film_roll WHERE clip_id=?1",
                [&clip_id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
        })
        .filter(|s| !s.is_empty() && !s.contains(['/', '\\', ':', '\0']) && !s.starts_with('.'));
    let Some(name) = file_name else {
        return (StatusCode::NOT_FOUND, Json(json!({"reason": "export_unavailable"}))).into_response();
    };
    let path = exports_dir(&app).join(name);
    if !path.is_file() {
        return (StatusCode::NOT_FOUND, Json(json!({"reason": "export_unavailable"}))).into_response();
    }
    let resp = ServeFile::new(path).oneshot(request).await.unwrap_or_else(|never| match never {});
    let (mut parts, body) = resp.into_response().into_parts();
    parts.headers.insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
    parts.headers.insert(header::X_CONTENT_TYPE_OPTIONS, header::HeaderValue::from_static("nosniff"));
    Response::from_parts(parts, body)
}

pub(crate) async fn open_exports(State(app): State<App>) -> ApiResult<StatusCode> {
    let dir = exports_dir(&app);
    std::fs::create_dir_all(&dir).map_err(internal)?;
    #[cfg(windows)]
    let mut cmd = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(&dir).spawn().map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn local_status(State(app): State<App>) -> ApiResult<Json<Value>> {
    let ffmpeg_ready = find_ffmpeg().is_some();
    let (source_root, ytdlp_ready, total, pending, approved, hold, exported) = {
        let db = app.db.lock().map_err(internal)?;
        let cfg = source::config(&db).map_err(internal)?;
        let ytdlp_ready = !cfg.downloader.is_empty() && Path::new(&cfg.downloader).is_file();
        let mut counts = (0i64, 0i64, 0i64, 0i64, 0i64);
        if let Ok(mut stmt) = db.prepare(
            "SELECT f.review_state, f.exported_path
             FROM workspace_film_roll f
             JOIN profiles p ON p.id = f.source_id
             WHERE p.archived = 0 AND p.role = 'source'",
        ) && let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        {
            for row in rows.flatten() {
                counts.0 += 1;
                match row.0.as_str() {
                    "approved" => counts.2 += 1,
                    "hold" => counts.3 += 1,
                    "exported" => counts.4 += 1,
                    _ => counts.1 += 1,
                }
            }
        }
        (cfg.root, ytdlp_ready, counts.0, counts.1, counts.2, counts.3, counts.4)
    };
    let exports = exports_dir(&app).to_string_lossy().into_owned();
    Ok(Json(json!({
        "ffmpeg_ready": ffmpeg_ready,
        "ytdlp_ready": ytdlp_ready,
        "source_root": source_root,
        "exports_dir": exports,
        "film_roll_total": total,
        "pending_count": pending,
        "approved_count": approved,
        "hold_count": hold,
        "exported_count": exported,
    })))
}

pub(crate) async fn channel_profiles(State(app): State<App>) -> Response {
    let mut out = vec![json!({
        "profile_id": "all",
        "channel_alias": "All Channels",
        "configured": true
    })];
    if let Ok(db) = app.db.lock()
        && let Ok(mut stmt) = db.prepare(
            "SELECT id, name, platform FROM profiles WHERE role='destination' AND archived=0 ORDER BY name COLLATE NOCASE LIMIT 100",
        )
        && let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
    {
        for (id, name, platform) in rows.flatten() {
            out.push(json!({
                "profile_id": format!("profile-{id}"),
                "channel_alias": format!("@{name} ({platform})"),
                "configured": true
            }));
        }
    }
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({
            "profiles": out,
            "default_profile_id": "all",
            "publishing_enabled": false
        })),
    )
        .into_response()
}

pub(crate) fn rows(app: &App) -> store::Result<Vec<Value>> {
    let entries: Vec<(String, i64, String, String, String, i64, String, String, String, String)> = {
        let db = app.db.lock().map_err(|_| store::Error("storage_unavailable"))?;
        let mut stmt = db
            .prepare(
                "SELECT f.clip_id, f.source_id, f.path, f.title, f.description, f.bytes,
                        f.review_state, f.exported_path, p.name, p.platform
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
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                ))
            })
            .map_err(|_| store::Error("storage_unavailable"))?;
        mapped
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|_| store::Error("storage_unavailable"))?
    };

    let out_dir = exports_dir(app);
    let mut out = Vec::with_capacity(entries.len());
    for (
        clip_id,
        source_id,
        path,
        title,
        description,
        saved_bytes,
        review_state,
        exported_path,
        source_name,
        source_platform,
    ) in entries
    {
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
        let has_export = !exported_path.is_empty() && out_dir.join(&exported_path).is_file();
        let mut row = json!({
            "kind": "original",
            "clip_id": clip_id,
            "source_id": source_id,
            "source_name": source_name,
            "source_platform": source_platform,
            "source_path": path,
            "name": name,
            "state": review_state,
            "preview_only": true,
            "preview_available": true,
            "poster_available": true,
            "preview_bytes": bytes,
            "preview_sha256": clip_id,
            "exported": has_export,
            "exported_file": if has_export { Some(exported_path) } else { None },
            "export_url": if has_export { Some(format!("/api/workspace/film-roll/{clip_id}/export")) } else { None },
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

pub(crate) fn exists(app: &App, clip_id: &str) -> bool {
    resolve_source_ref(app, clip_id).is_some()
}
