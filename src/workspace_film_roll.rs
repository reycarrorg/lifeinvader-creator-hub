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
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use tower::ServiceExt;
use tower_http::services::ServeFile;

#[derive(Clone, Default)]
struct ExportProgressState {
    active: bool,
    clip_id: String,
    title: String,
    percent: u8,
    stage: String,
    message: String,
    encoder: String,
    batch_index: usize,
    batch_total: usize,
    batch_completed: usize,
    batch_failed: usize,
    active_pid: Option<u32>,
}

fn export_state() -> &'static Mutex<ExportProgressState> {
    static STATE: OnceLock<Mutex<ExportProgressState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(ExportProgressState::default()))
}

fn export_cancel_flag() -> &'static AtomicBool {
    static FLAG: OnceLock<AtomicBool> = OnceLock::new();
    FLAG.get_or_init(|| AtomicBool::new(false))
}

fn kill_pid(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F", "/T"])
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

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
            probe_json TEXT NOT NULL DEFAULT '',
            UNIQUE(source_id, path)
        );
        CREATE TABLE IF NOT EXISTS workspace_export_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            clip_id TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            source_name TEXT NOT NULL DEFAULT '',
            file_name TEXT NOT NULL,
            caption_file TEXT NOT NULL DEFAULT '',
            cover_file TEXT NOT NULL DEFAULT '',
            framing TEXT NOT NULL DEFAULT 'original',
            quality TEXT NOT NULL DEFAULT 'balanced',
            in_ms INTEGER NOT NULL DEFAULT 0,
            out_ms INTEGER NOT NULL DEFAULT 0,
            muted INTEGER NOT NULL DEFAULT 0,
            normalized_audio INTEGER NOT NULL DEFAULT 0,
            bytes INTEGER NOT NULL DEFAULT 0,
            exported_at INTEGER NOT NULL DEFAULT (unixepoch())
        );
        CREATE TABLE IF NOT EXISTS workspace_caption_presets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            title_prefix TEXT NOT NULL DEFAULT '',
            caption_body TEXT NOT NULL DEFAULT '',
            tags TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL DEFAULT (unixepoch())
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
    if !cols.iter().any(|c| c == "probe_json") {
        db.execute(
            "ALTER TABLE workspace_film_roll ADD COLUMN probe_json TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    for col in ["yt_video_id", "yt_publish_at", "ig_media_id", "ig_permalink", "tt_publish_id", "x_tweet_id", "mood_tags", "content_sha256"] {
        if !cols.iter().any(|c| c == col) {
            db.execute(
                &format!("ALTER TABLE workspace_film_roll ADD COLUMN {col} TEXT NOT NULL DEFAULT ''"),
                [],
            )?;
        }
    }
    if !cols.iter().any(|c| c == "rating") {
        db.execute(
            "ALTER TABLE workspace_film_roll ADD COLUMN rating INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if !cols.iter().any(|c| c == "markers_json") {
        db.execute(
            "ALTER TABLE workspace_film_roll ADD COLUMN markers_json TEXT NOT NULL DEFAULT '[]'",
            [],
        )?;
    }
    let hcols: Vec<String> = {
        let mut stmt = db.prepare("PRAGMA table_info(workspace_export_history)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for col in ["youtube_video_id", "ig_permalink", "tiktok_publish_id", "x_tweet_id"] {
        if !hcols.iter().any(|c| c == col) {
            db.execute(
                &format!("ALTER TABLE workspace_export_history ADD COLUMN {col} TEXT NOT NULL DEFAULT ''"),
                [],
            )?;
        }
    }
    for (name, title_prefix, caption_body, tags) in [
        ("GTA Clips", "", "", "#gta #gtav #gtaonline #gaming #clips"),
        ("Shorts Hook", "", "Watch till the end!", "#shorts #gaming #viral"),
        ("Reels Clean", "", "Save this for later.", "#reels #gamingclips #fyp"),
        ("GTA 6 Funny", "", "Wait until the end 😂", "#GTA6 #GTAVI #GTA6Clips #GTA6Funny #GamingClips"),
        ("GTA 6 Secrets & Details", "", "Did you notice this detail? 👀", "#GTA6 #GTAVI #GTA6Details #GTA6Secrets #RockstarGames"),
        ("GTA 6 Gameplay", "", "Welcome back to Vice City 🌴", "#GTA6 #GTAVI #GrandTheftAutoVI #RockstarGames #ViceCity"),
    ] {
        let _ = db.execute(
            "INSERT OR IGNORE INTO workspace_caption_presets(name, title_prefix, caption_body, tags) VALUES(?1, ?2, ?3, ?4)",
            params![name, title_prefix, caption_body, tags],
        );
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
pub(crate) struct BatchRequest {
    clip_ids: Vec<String>,
    action: String,
}

#[derive(Deserialize, Clone)]
pub(crate) struct ExportRequest {
    #[serde(default)]
    in_ms: Option<u64>,
    #[serde(default)]
    out_ms: Option<u64>,
    #[serde(default = "default_framing")]
    framing: String,
    #[serde(default = "default_quality")]
    quality: String,
    #[serde(default)]
    mute: bool,
    #[serde(default)]
    normalize_audio: bool,
    #[serde(default)]
    use_hw_encode: bool,
    #[serde(default)]
    save_caption_txt: bool,
    #[serde(default)]
    caption_text: Option<String>,
    #[serde(default)]
    cover_png_base64: Option<String>,
    #[serde(default)]
    hook_text: Option<String>,
    #[serde(default)]
    watermark_text: Option<String>,
    #[serde(default = "default_wm_pos")]
    watermark_pos: String,
    #[serde(default)]
    fade_io: bool,
    #[serde(default)]
    vivid_filter: bool,
    #[serde(default)]
    speed: Option<f64>,
    #[serde(default)]
    srt_text: Option<String>,
    #[serde(default)]
    subtitle_style: Option<String>,
    #[serde(default)]
    crop_pan: Option<f64>,
    #[serde(default)]
    rotate_deg: Option<i32>,
    #[serde(default)]
    mirror_h: bool,
    #[serde(default)]
    subtle_zoom: bool,
    #[serde(default)]
    outro_cta: Option<String>,
    #[serde(default)]
    audio_preset: Option<String>,
    #[serde(default)]
    audio_gain_db: Option<f64>,
    #[serde(default)]
    music_path: Option<String>,
    #[serde(default)]
    music_volume_db: Option<f64>,
    #[serde(default)]
    max_duration_s: Option<f64>,
    #[serde(default)]
    force_30fps: bool,
}

#[derive(Deserialize)]
pub(crate) struct MetaUpdateRequest {
    #[serde(default)]
    rating: Option<i64>,
    #[serde(default)]
    mood_tags: Option<String>,
    #[serde(default)]
    markers: Option<Vec<Value>>,
}

#[derive(Deserialize)]
pub(crate) struct SplitPartsRequest {
    #[serde(default = "default_part_seconds")]
    part_seconds: u64,
    #[serde(default = "default_framing")]
    framing: String,
    #[serde(default = "default_quality")]
    quality: String,
    #[serde(default)]
    use_hw_encode: bool,
    #[serde(default)]
    crop_pan: Option<f64>,
    #[serde(default)]
    hook_prefix: Option<String>,
    #[serde(default)]
    watermark_text: Option<String>,
    #[serde(default = "default_wm_pos")]
    watermark_pos: String,
    #[serde(default)]
    normalize_audio: bool,
    #[serde(default)]
    fade_io: bool,
    #[serde(default)]
    vivid_filter: bool,
}

fn default_part_seconds() -> u64 {
    55
}

#[derive(Deserialize)]
pub(crate) struct ZipKitRequest {
    #[serde(default)]
    caption_text: Option<String>,
    #[serde(default)]
    cover_png_base64: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct BatchExportRequest {
    #[serde(default)]
    clip_ids: Vec<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default = "default_framing")]
    framing: String,
    #[serde(default = "default_quality")]
    quality: String,
    #[serde(default)]
    mute: bool,
    #[serde(default)]
    normalize_audio: bool,
    #[serde(default)]
    use_hw_encode: bool,
    #[serde(default)]
    save_caption_txt: bool,
    #[serde(default)]
    hook_text: Option<String>,
    #[serde(default)]
    watermark_text: Option<String>,
    #[serde(default = "default_wm_pos")]
    watermark_pos: String,
    #[serde(default)]
    fade_io: bool,
    #[serde(default)]
    vivid_filter: bool,
    #[serde(default)]
    speed: Option<f64>,
}

#[derive(Deserialize)]
pub(crate) struct CaptionPresetInput {
    name: String,
    #[serde(default)]
    title_prefix: String,
    #[serde(default)]
    caption_body: String,
    #[serde(default)]
    tags: String,
}

fn default_framing() -> String {
    "original".to_owned()
}

fn default_quality() -> String {
    "balanced".to_owned()
}

fn default_wm_pos() -> String {
    "bottom_right".to_owned()
}

fn cached_hw_encoder() -> Option<String> {
    static HW: OnceLock<Option<String>> = OnceLock::new();
    HW.get_or_init(|| {
        let ffmpeg = find_ffmpeg()?;
        let mut list_cmd = std::process::Command::new(&ffmpeg);
        list_cmd.args(["-hide_banner", "-encoders"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            list_cmd.creation_flags(0x08000000);
        }
        let out = list_cmd.output().ok()?;
        let encoders = String::from_utf8_lossy(&out.stdout);
        for candidate in ["h264_nvenc", "h264_qsv", "h264_amf"] {
            if !encoders.contains(candidate) {
                continue;
            }
            // Verify with a tiny 0.1s null encode so we only advertise an encoder that actually works on this GPU/driver.
            let mut probe = std::process::Command::new(&ffmpeg);
            probe.args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=256x256:d=0.1",
                "-c:v",
                candidate,
                "-f",
                "null",
                "-",
            ]);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                probe.creation_flags(0x08000000);
            }
            if probe.status().is_ok_and(|s| s.success()) {
                return Some(candidate.to_owned());
            }
        }
        None
    })
    .clone()
}

fn find_overlay_font() -> Option<String> {
    #[cfg(windows)]
    {
        for p in [
            r"C:\Windows\Fonts\segoeuib.ttf",
            r"C:\Windows\Fonts\arialbd.ttf",
            r"C:\Windows\Fonts\arial.ttf",
        ] {
            if Path::new(p).is_file() {
                // FFmpeg drawtext fontfile requires forward slashes and escaped drive colon.
                return Some(p.replace('\\', "/").replace(':', "\\:"));
            }
        }
    }
    None
}

fn escape_drawtext(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 8);
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            ':' => out.push_str(r"\:"),
            '\'' => out.push_str(r"\'"),
            '\r' | '\n' => out.push(' '),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn build_drawtext_filters(
    hook_text: Option<&str>,
    watermark_text: Option<&str>,
    watermark_pos: &str,
    outro_cta: Option<&str>,
    out_dur_s: f64,
) -> Vec<String> {
    let font_opt = find_overlay_font()
        .map(|f| format!("fontfile='{f}':"))
        .unwrap_or_default();
    let mut filters = Vec::new();
    if let Some(hook) = hook_text.map(str::trim).filter(|s| !s.is_empty()) {
        let safe = escape_drawtext(&hook.chars().take(80).collect::<String>());
        filters.push(format!(
            "drawtext={font_opt}expansion=none:text='{safe}':fontcolor=white:fontsize=max(24\\,min(w/18\\,52)):box=1:boxcolor=black@0.68:boxborderw=16:x=(w-text_w)/2:y=h*0.11"
        ));
    }
    if let Some(wm) = watermark_text.map(str::trim).filter(|s| !s.is_empty()) {
        let safe = escape_drawtext(&wm.chars().take(40).collect::<String>());
        let (x, y) = match watermark_pos {
            "bottom_left" => ("w*0.05", "h-text_h-h*0.07"),
            "top_left" => ("w*0.05", "h*0.04"),
            "top_right" => ("w-text_w-w*0.05", "h*0.04"),
            _ => ("w-text_w-w*0.05", "h-text_h-h*0.07"),
        };
        filters.push(format!(
            "drawtext={font_opt}expansion=none:text='{safe}':fontcolor=white@0.82:shadowcolor=black@0.75:shadowx=2:shadowy=2:fontsize=max(18\\,min(w/28\\,34)):x={x}:y={y}"
        ));
    }
    if let Some(cta) = outro_cta.map(str::trim).filter(|s| !s.is_empty()) && out_dur_s > 2.2 {
        let safe = escape_drawtext(&cta.chars().take(70).collect::<String>());
        let start_s = (out_dur_s - 2.2).max(0.0);
        filters.push(format!(
            "drawtext={font_opt}expansion=none:enable='gte(t\\,{start_s:.2})':text='{safe}':fontcolor=#29B6F6:fontsize=max(22\\,min(w/20\\,44)):box=1:boxcolor=black@0.80:boxborderw=18:x=(w-text_w)/2:y=h*0.76"
        ));
    }
    filters
}

fn is_video_path(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "mp4" | "webm" | "mov" | "mkv"))
}

fn find_tool_binary(env_var: &str, win_name: &str, unix_name: &str) -> Option<PathBuf> {
    let bin = if cfg!(windows) { win_name } else { unix_name };
    if let Some(env_path) = std::env::var_os(env_var) {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
        let nested = p.join(bin);
        if nested.is_file() {
            return Some(nested);
        }
        if let Some(parent) = p.parent() {
            let sibling = parent.join(bin);
            if sibling.is_file() {
                return Some(sibling);
            }
        }
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let local = dir.join("tools").join(bin);
        if local.is_file() {
            return Some(local);
        }
    }
    let cwd_local = PathBuf::from("tools").join(bin);
    if cwd_local.is_file() {
        return Some(cwd_local);
    }
    None
}

pub(crate) fn find_ffmpeg() -> Option<PathBuf> {
    find_tool_binary("LIFEINVADER_FFMPEG", "ffmpeg.exe", "ffmpeg")
}

pub(crate) fn find_ffprobe() -> Option<PathBuf> {
    if let Some(p) = find_tool_binary("LIFEINVADER_FFPROBE", "ffprobe.exe", "ffprobe") {
        return Some(p);
    }
    if let Some(ffmpeg) = find_ffmpeg()
        && let Some(dir) = ffmpeg.parent()
    {
        let sibling = dir.join(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" });
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    None
}

pub(crate) fn find_gallery_dl() -> Option<PathBuf> {
    let candidates = [
        ("tools", "gallery-dl-python", "bin", "gallery-dl.exe"),
        ("tools", "gallery-dl-python", "bin", "gallery-dl"),
        ("tools", "gallery-dl-python", "Scripts", "gallery-dl.exe"),
        ("tools", "gallery-dl-python", "gallery_dl", "__main__.py"),
    ];
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        for (a, b, c, d) in candidates {
            let p = dir.join(a).join(b).join(c).join(d);
            if p.is_file() {
                return Some(p);
            }
        }
        let direct = dir.join("tools").join(if cfg!(windows) { "gallery-dl.exe" } else { "gallery-dl" });
        if direct.is_file() {
            return Some(direct);
        }
    }
    for (a, b, c, d) in candidates {
        let p = PathBuf::from(a).join(b).join(c).join(d);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn run_version_line(path: &Path, flag: &str) -> Option<String> {
    let mut cmd = std::process::Command::new(path);
    cmd.arg(flag);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let out = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next().map(|l| l.trim().chars().take(120).collect())
}

fn cached_ffmpeg_version() -> Option<String> {
    static VER: OnceLock<Option<String>> = OnceLock::new();
    VER.get_or_init(|| find_ffmpeg().and_then(|p| run_version_line(&p, "-version"))).clone()
}

fn cached_ytdlp_version(path: &Path) -> Option<String> {
    static VER: OnceLock<Option<String>> = OnceLock::new();
    VER.get_or_init(|| run_version_line(path, "--version")).clone()
}

fn dir_stats(root: &Path, max_depth: usize) -> (u64, u64) {
    fn walk(dir: &Path, depth: usize, bytes: &mut u64, files: &mut u64) {
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_file() {
                *bytes = bytes.saturating_add(meta.len());
                *files = files.saturating_add(1);
            } else if meta.is_dir() {
                walk(&entry.path(), depth - 1, bytes, files);
            }
        }
    }
    let (mut bytes, mut files) = (0u64, 0u64);
    if root.is_dir() {
        walk(root, max_depth, &mut bytes, &mut files);
    }
    (bytes, files)
}

pub(crate) fn decode_base64(raw: &str) -> Option<Vec<u8>> {
    let trimmed = raw.trim();
    let payload = trimmed
        .strip_prefix("data:image/png;base64,")
        .unwrap_or(trimmed);
    let mut out = Vec::with_capacity((payload.len() * 3) / 4);
    let mut buf = 0u32;
    let mut bits = 0u8;
    for b in payload.bytes() {
        if b.is_ascii_whitespace() || b == b'=' {
            continue;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Some(out)
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

pub(crate) async fn batch_action(
    State(app): State<App>,
    Json(input): Json<BatchRequest>,
) -> ApiResult<Json<Value>> {
    if input.clip_ids.is_empty() || input.clip_ids.len() > 200 {
        return Err(bad("Select between 1 and 200 clips for a batch action."));
    }
    if !matches!(
        input.action.as_str(),
        "approve" | "hold" | "pending_review" | "remove"
    ) {
        return Err(bad("Unsupported batch action."));
    }
    for id in &input.clip_ids {
        if !crate::workspace_media::hex(id) {
            return Err(bad("Invalid clip ID in batch."));
        }
    }
    let mut db = app.db.lock().map_err(internal)?;
    let tx = db.transaction().map_err(internal)?;
    let mut affected = 0usize;
    for id in &input.clip_ids {
        let changed = match input.action.as_str() {
            "remove" => tx
                .execute("DELETE FROM workspace_film_roll WHERE clip_id=?1", [id])
                .map_err(internal)?,
            "approve" => tx
                .execute(
                    "UPDATE workspace_film_roll SET review_state='approved' WHERE clip_id=?1",
                    [id],
                )
                .map_err(internal)?,
            "hold" => tx
                .execute(
                    "UPDATE workspace_film_roll SET review_state='hold' WHERE clip_id=?1",
                    [id],
                )
                .map_err(internal)?,
            _ => tx
                .execute(
                    "UPDATE workspace_film_roll SET review_state='pending_review' WHERE clip_id=?1",
                    [id],
                )
                .map_err(internal)?,
        };
        affected += changed;
    }
    tx.commit().map_err(internal)?;
    Ok(Json(json!({ "action": input.action, "affected": affected })))
}

pub(crate) async fn probe_clip(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let Some(source_path) = resolve_video(&app, &clip_id) else {
        return Err((StatusCode::NOT_FOUND, "Source video is unavailable.".into()));
    };
    let current_size = std::fs::metadata(&source_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let cached: Option<String> = {
        let db = app.db.lock().map_err(internal)?;
        db.query_row(
            "SELECT probe_json FROM workspace_film_roll WHERE clip_id=?1",
            [&clip_id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(internal)?
        .filter(|s| !s.is_empty())
    };
    if let Some(raw) = cached
        && let Ok(val) = serde_json::from_str::<Value>(&raw)
        && val["format"]["size"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| val["format"]["size"].as_u64())
            == Some(current_size)
    {
        return Ok(Json(json!({
            "clip_id": clip_id,
            "preview_sha256": clip_id,
            "facts": val,
            "cached": true
        })));
    }
    let Some(ffprobe) = find_ffprobe() else {
        return Err(bad("ffprobe was not found in tools/ffprobe.exe."));
    };
    let path_clone = source_path.clone();
    let probed = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let mut cmd = std::process::Command::new(&ffprobe);
        cmd.args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(&path_clone);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let out = cmd
            .output()
            .map_err(|e| format!("Failed to start ffprobe: {e}"))?;
        if !out.status.success() {
            return Err("ffprobe could not inspect this video.".to_owned());
        }
        let mut parsed: Value = serde_json::from_slice(&out.stdout)
            .map_err(|_| "ffprobe returned unreadable JSON.".to_owned())?;
        if let Some(fmt) = parsed.get_mut("format").and_then(|v| v.as_object_mut()) {
            fmt.insert("size".to_owned(), json!(current_size.to_string()));
        }
        Ok(parsed)
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;

    if let Ok(serialized) = serde_json::to_string(&probed)
        && let Ok(db) = app.db.lock()
    {
        let _ = db.execute(
            "UPDATE workspace_film_roll SET probe_json=?1 WHERE clip_id=?2",
            params![serialized, clip_id],
        );
    }
    Ok(Json(json!({
        "clip_id": clip_id,
        "preview_sha256": clip_id,
        "facts": probed,
        "cached": false
    })))
}

fn escape_ffmpeg_subtitles_path(p: &Path) -> String {
    p.to_string_lossy()
        .replace('\\', "/")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

fn run_ffmpeg_once(
    ffmpeg: &Path,
    src: &Path,
    tmp_out: &Path,
    framing: &str,
    crop_pan: f64,
    quality: &str,
    in_ms: Option<u64>,
    out_ms: Option<u64>,
    mute: bool,
    audio_filters: &[String],
    music_bed: Option<(&Path, f64)>,
    encoder: &str,
    video_filters: &[String],
    target_duration_us: u64,
) -> Result<(), String> {
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.arg("-y")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-nostats")
        .arg("-progress")
        .arg("pipe:1");
    if let Some(start) = in_ms
        && start > 0
    {
        cmd.arg("-ss").arg(format!("{:.3}", start as f64 / 1000.0));
    }
    cmd.arg("-i").arg(src);
    if let Some((music_file, _)) = music_bed {
        cmd.args(["-stream_loop", "-1", "-i"]).arg(music_file);
    }
    if let Some(end) = out_ms {
        let start = in_ms.unwrap_or(0);
        if end > start {
            cmd.arg("-t")
                .arg(format!("{:.3}", (end - start) as f64 / 1000.0));
        }
    }
    let dt_chain = if video_filters.is_empty() {
        String::new()
    } else {
        format!(",{}", video_filters.join(","))
    };
    let pan_frac = ((crop_pan.clamp(-1.0, 1.0) + 1.0) / 2.0).clamp(0.0, 1.0);
    let mut used_filter_complex = false;
    let mut fc_parts: Vec<String> = Vec::new();
    match framing {
        "center_crop" => {
            let vf = format!(
                "crop='min(iw,ih*9/16)':'min(ih,iw*16/9)':'(iw-ow)*{pan_frac:.4}':'(ih-oh)/2',scale=1080:1920:flags=lanczos{dt_chain}"
            );
            if music_bed.is_some() {
                fc_parts.push(format!("[0:v]{vf}[v]"));
                used_filter_complex = true;
            } else {
                cmd.arg("-vf").arg(vf);
            }
        }
        "blurred_fit" => {
            let vf = if video_filters.is_empty() {
                "[0:v]split[bg][fg];[bg]scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920,boxblur=20:5[b];[fg]scale=1080:1920:force_original_aspect_ratio=decrease[f];[b][f]overlay=(W-w)/2:(H-h)/2[v]".to_owned()
            } else {
                format!(
                    "[0:v]split[bg][fg];[bg]scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920,boxblur=20:5[b];[fg]scale=1080:1920:force_original_aspect_ratio=decrease[f];[b][f]overlay=(W-w)/2:(H-h)/2,{}[v]",
                    video_filters.join(",")
                )
            };
            fc_parts.push(vf);
            used_filter_complex = true;
        }
        "split_top_left" | "split_top_right" | "split_bottom_left" | "split_bottom_right" => {
            let (cx, cy) = match framing {
                "split_top_left" => ("0", "0"),
                "split_bottom_left" => ("0", "ih-oh"),
                "split_bottom_right" => ("iw-ow", "ih-oh"),
                _ => ("iw-ow", "0"),
            };
            let vf = format!(
                "[0:v]split[csrc][gsrc];[csrc]crop='iw*0.38':'ih*0.38':'{cx}':'{cy}',scale=1080:672:force_original_aspect_ratio=increase,crop=1080:672[cam];[gsrc]crop='min(iw,ih*0.865)':'ih':'(iw-ow)*{pan_frac:.4}':'0',scale=1080:1248:force_original_aspect_ratio=increase,crop=1080:1248[game];[cam][game]vstack=inputs=2{dt_chain}[v]"
            );
            fc_parts.push(vf);
            used_filter_complex = true;
        }
        _ => {
            if music_bed.is_some() {
                if video_filters.is_empty() {
                    fc_parts.push("[0:v]null[v]".to_owned());
                } else {
                    fc_parts.push(format!("[0:v]{}[v]", video_filters.join(",")));
                }
                used_filter_complex = true;
            } else if !video_filters.is_empty() {
                cmd.arg("-vf").arg(video_filters.join(","));
            }
        }
    }
    if let Some((_, music_db)) = music_bed {
        let clamped_db = music_db.clamp(-36.0, 0.0);
        if mute {
            fc_parts.push(format!("[1:a]volume={clamped_db:.1}dB[aout]"));
        } else {
            let main_af = if audio_filters.is_empty() {
                "[0:a]anull[a0]".to_owned()
            } else {
                format!("[0:a]{}[a0]", audio_filters.join(","))
            };
            fc_parts.push(format!(
                "{main_af};[1:a]volume={clamped_db:.1}dB[m];[a0][m]amix=inputs=2:duration=first:dropout_transition=2[aout]"
            ));
        }
        cmd.arg("-filter_complex")
            .arg(fc_parts.join(";"))
            .args(["-map", "[v]", "-map", "[aout]", "-shortest"]);
    } else if used_filter_complex {
        cmd.arg("-filter_complex").arg(fc_parts.join(";")).args(["-map", "[v]"]);
        if !mute {
            cmd.args(["-map", "0:a?"]);
        }
    }

    match encoder {
        "h264_nvenc" => {
            let (preset, cq, maxrate, bufsize) = match quality {
                "high" => ("p5", "18", "12M", "24M"),
                "fast" => ("p2", "25", "5M", "10M"),
                _ => ("p4", "21", "7M", "14M"),
            };
            cmd.args([
                "-c:v",
                "h264_nvenc",
                "-preset",
                preset,
                "-rc",
                "vbr",
                "-cq",
                cq,
                "-b:v",
                "0",
                "-maxrate",
                maxrate,
                "-bufsize",
                bufsize,
                "-pix_fmt",
                "yuv420p",
            ]);
        }
        "h264_qsv" => {
            let (q, maxrate, bufsize) = match quality {
                "high" => ("18", "12M", "24M"),
                "fast" => ("25", "5M", "10M"),
                _ => ("21", "7M", "14M"),
            };
            cmd.args([
                "-c:v",
                "h264_qsv",
                "-global_quality",
                q,
                "-maxrate",
                maxrate,
                "-bufsize",
                bufsize,
                "-pix_fmt",
                "nv12",
            ]);
        }
        "h264_amf" => {
            let (usage, qp, maxrate, bufsize) = match quality {
                "high" => ("quality", "18", "12M", "24M"),
                "fast" => ("speed", "25", "5M", "10M"),
                _ => ("balanced", "21", "7M", "14M"),
            };
            cmd.args([
                "-c:v", "h264_amf", "-quality", usage, "-rc", "cqp", "-qp_i", qp, "-qp_p", qp,
                "-maxrate", maxrate, "-bufsize", bufsize, "-pix_fmt", "yuv420p",
            ]);
        }
        _ => {
            let (preset, crf, maxrate, bufsize) = match quality {
                "high" => ("medium", "16", "12M", "24M"),
                "fast" => ("veryfast", "23", "5M", "10M"),
                _ => ("fast", "20", "7M", "14M"),
            };
            cmd.args([
                "-c:v", "libx264", "-preset", preset, "-crf", crf, "-maxrate", maxrate,
                "-bufsize", bufsize, "-pix_fmt", "yuv420p",
            ]);
        }
    }

    if mute && music_bed.is_none() {
        cmd.arg("-an");
    } else {
        if music_bed.is_none() && !audio_filters.is_empty() {
            cmd.args(["-af", &audio_filters.join(",")]);
        }
        // Always resample to 48 kHz stereo AAC (FFmpeg loudnorm otherwise upsamples to 96/192 kHz,
        // which Instagram Reels & TikTok reject).
        cmd.args(["-ar", "48000", "-ac", "2", "-c:a", "aac", "-b:a", "128k"]);
    }
    cmd.args(["-movflags", "+faststart"])
        .arg(tmp_out)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start FFmpeg: {e}"))?;
    let pid = child.id();
    if let Ok(mut st) = export_state().lock() {
        st.active_pid = Some(pid);
        st.encoder = encoder.to_owned();
    }
    let stderr_handle = child.stderr.take().map(|err| {
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = std::io::Read::read_to_string(&mut std::io::BufReader::new(err), &mut s);
            s
        })
    });
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if export_cancel_flag().load(Ordering::Relaxed) {
                let _ = child.kill();
                break;
            }
            if let Some(us_str) = line.strip_prefix("out_time_us=")
                && let Ok(us) = us_str.trim().parse::<u64>()
                && target_duration_us > 0
            {
                let pct = ((us.saturating_mul(100)) / target_duration_us).min(99) as u8;
                if let Ok(mut st) = export_state().lock() {
                    st.percent = pct;
                }
            }
        }
    }
    let status = child
        .wait()
        .map_err(|e| format!("Failed waiting for FFmpeg: {e}"))?;
    if let Ok(mut st) = export_state().lock()
        && st.active_pid == Some(pid)
    {
        st.active_pid = None;
    }
    let stderr_text = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    if export_cancel_flag().load(Ordering::Relaxed) {
        let _ = std::fs::remove_file(tmp_out);
        return Err("Export cancelled.".to_owned());
    }
    if !status.success() {
        let _ = std::fs::remove_file(tmp_out);
        let msg = stderr_text.trim().to_owned();
        return Err(if msg.is_empty() {
            "FFmpeg could not render this clip.".to_owned()
        } else {
            msg
        });
    }
    Ok(())
}

fn probed_duration_ms(app: &App, clip_id: &str) -> Option<u64> {
    let db = app.db.lock().ok()?;
    let raw: String = db
        .query_row(
            "SELECT probe_json FROM workspace_film_roll WHERE clip_id=?1",
            [clip_id],
            |r| r.get(0),
        )
        .optional()
        .ok()??;
    let val: Value = serde_json::from_str(&raw).ok()?;
    let dur = val["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| val["format"]["duration"].as_f64())?;
    (dur.is_finite() && dur > 0.0).then_some((dur * 1000.0).round() as u64)
}

async fn render_clip_internal(
    app: &App,
    clip_id: &str,
    input: &ExportRequest,
    manage_state: bool,
) -> Result<Value, (StatusCode, String)> {
    if !crate::workspace_media::hex(clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    if !matches!(input.framing.as_str(), "original" | "center_crop" | "blurred_fit") {
        return Err(bad("Choose original, center_crop, or blurred_fit framing."));
    }
    if !matches!(input.quality.as_str(), "fast" | "balanced" | "high") {
        return Err(bad("Choose fast, balanced, or high quality."));
    }
    if let (Some(a), Some(b)) = (input.in_ms, input.out_ms)
        && a >= b
    {
        return Err(bad("Out time must be after In time."));
    }
    let Some(source_path) = resolve_video(app, clip_id) else {
        return Err((StatusCode::NOT_FOUND, "Source video is unavailable.".into()));
    };
    let Some(ffmpeg) = find_ffmpeg() else {
        return Err(bad("FFmpeg was not found in tools/ffmpeg.exe or LIFEINVADER_FFMPEG."));
    };
    let (title, source_name): (String, String) = {
        let db = app.db.lock().map_err(internal)?;
        db.query_row(
            "SELECT f.title, COALESCE(p.name, '')
             FROM workspace_film_roll f
             LEFT JOIN profiles p ON p.id = f.source_id
             WHERE f.clip_id=?1",
            [clip_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(internal)?
        .unwrap_or_default()
    };
    let out_dir = exports_dir(app);
    std::fs::create_dir_all(&out_dir).map_err(internal)?;
    let stem = sanitize_stem(if title.is_empty() {
        source_path.file_stem().and_then(|s| s.to_str()).unwrap_or("clip")
    } else {
        &title
    });
    let short_id = &clip_id[..8];
    let file_name = format!("{stem}-{short_id}.mp4");
    let final_path = out_dir.join(&file_name);
    let tmp_path = out_dir.join(format!("{stem}-{short_id}.tmp.mp4"));

    let target_ms = match (input.in_ms, input.out_ms) {
        (Some(a), Some(b)) if b > a => b - a,
        (a, None) => probed_duration_ms(app, clip_id)
            .map(|d| d.saturating_sub(a.unwrap_or(0)))
            .unwrap_or(15_000),
        _ => 15_000,
    };

    if manage_state {
        export_cancel_flag().store(false, Ordering::Relaxed);
        if let Ok(mut st) = export_state().lock() {
            *st = ExportProgressState {
                active: true,
                clip_id: clip_id.to_owned(),
                title: if title.is_empty() { file_name.clone() } else { title.clone() },
                percent: 0,
                stage: "rendering".to_owned(),
                message: "Rendering MP4 with FFmpeg…".to_owned(),
                encoder: String::new(),
                batch_index: 0,
                batch_total: 0,
                batch_completed: 0,
                batch_failed: 0,
                active_pid: None,
            };
        }
    }

    let framing = input.framing.clone();
    let crop_pan = input.crop_pan.filter(|v| v.is_finite()).unwrap_or(0.0).clamp(-1.0, 1.0);
    let quality = input.quality.clone();
    let in_ms = input.in_ms;
    let out_ms = match (input.max_duration_s, input.out_ms) {
        (Some(max_s), Some(end)) if max_s.is_finite() && max_s >= 1.0 => {
            let cap = in_ms.unwrap_or(0).saturating_add((max_s * 1000.0).round() as u64);
            Some(end.min(cap))
        }
        (Some(max_s), None) if max_s.is_finite() && max_s >= 1.0 => {
            Some(in_ms.unwrap_or(0).saturating_add((max_s * 1000.0).round() as u64))
        }
        (_, other) => other,
    };
    let effective_target_ms = match (in_ms, out_ms) {
        (Some(a), Some(b)) if b > a => b - a,
        (None, Some(b)) if b > 0 => b,
        _ => target_ms,
    };
    let mute = input.mute;
    let normalize_audio = input.normalize_audio && !mute;
    let use_hw = input.use_hw_encode;
    let speed = input
        .speed
        .filter(|s| s.is_finite() && *s >= 0.75 && *s <= 1.5)
        .unwrap_or(1.0);
    let out_dur_s = (effective_target_ms as f64 / 1000.0) / speed;
    let effective_target_us = ((out_dur_s * 1_000_000.0).round() as u64).max(100_000);

    let mut video_filters = Vec::new();
    match input.rotate_deg.unwrap_or(0).rem_euclid(360) {
        90 => video_filters.push("transpose=1".to_owned()),
        180 => video_filters.push("hflip,vflip".to_owned()),
        270 => video_filters.push("transpose=2".to_owned()),
        _ => {}
    }
    if input.mirror_h {
        video_filters.push("hflip".to_owned());
    }
    if input.subtle_zoom {
        video_filters.push("scale=trunc(iw*1.06/2)*2:trunc(ih*1.06/2)*2,crop=iw/1.06:ih/1.06".to_owned());
    }
    if input.force_30fps {
        video_filters.push("fps=30".to_owned());
    }
    if input.vivid_filter {
        video_filters.push("eq=contrast=1.08:saturation=1.18:brightness=0.02,unsharp=3:3:0.5:3:3:0.0".to_owned());
    }
    if (speed - 1.0).abs() > 0.01 {
        video_filters.push(format!("setpts=PTS/{speed:.4}"));
    }
    if input.fade_io && out_dur_s > 1.2 {
        let fade_out_st = (out_dur_s - 0.35).max(0.1);
        video_filters.push(format!("fade=t=in:st=0:d=0.35,fade=t=out:st={fade_out_st:.3}:d=0.35"));
    }
    video_filters.extend(build_drawtext_filters(
        input.hook_text.as_deref(),
        input.watermark_text.as_deref(),
        &input.watermark_pos,
        input.outro_cta.as_deref(),
        out_dur_s,
    ));

    let temp_srt_path = if let Some(raw_srt) = input.srt_text.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let srt_file = out_dir.join(format!("sub-{short_id}-{}.srt", chrono::Utc::now().timestamp_millis()));
        if std::fs::write(&srt_file, raw_srt).is_ok() {
            let style = match input.subtitle_style.as_deref() {
                Some("clean_white") => "FontName=Arial,FontSize=20,Bold=1,PrimaryColour=&H00FFFFFF,OutlineColour=&H00000000,BorderStyle=1,Outline=2,Shadow=1,Alignment=2,MarginV=70",
                Some("vice_neon") => "FontName=Arial,FontSize=25,Bold=1,PrimaryColour=&H00FFE500,OutlineColour=&H00932EFF,BackColour=&H80120809,BorderStyle=1,Outline=3,Shadow=2,Alignment=2,MarginV=90",
                _ => "FontName=Arial,FontSize=24,Bold=1,PrimaryColour=&H0000EAFF,OutlineColour=&H00000000,BorderStyle=1,Outline=3,Shadow=1,Alignment=2,MarginV=85",
            };
            video_filters.push(format!(
                "subtitles='{}':force_style='{style}'",
                escape_ffmpeg_subtitles_path(&srt_file)
            ));
            Some(srt_file)
        } else {
            None
        }
    } else {
        None
    };

    let mut audio_filters = Vec::new();
    if !mute {
        if (speed - 1.0).abs() > 0.01 {
            audio_filters.push(format!("atempo={speed:.4}"));
        }
        match input.audio_preset.as_deref() {
            Some("voice") => audio_filters.push("highpass=f=80,lowpass=f=12000,acompressor=threshold=-18dB:ratio=3".to_owned()),
            Some("punch") => audio_filters.push("bass=g=3:f=100,treble=g=2:f=4000".to_owned()),
            Some("denoise") => audio_filters.push("afftdn=nf=-25".to_owned()),
            _ => {}
        }
        if let Some(gain) = input.audio_gain_db.filter(|g| g.is_finite() && g.abs() >= 0.25) {
            audio_filters.push(format!("volume={:.1}dB", gain.clamp(-12.0, 12.0)));
        }
        if normalize_audio {
            audio_filters.push("loudnorm=I=-14:TP=-1.5:LRA=11".to_owned());
        }
        if input.fade_io && out_dur_s > 1.2 {
            let fade_out_st = (out_dur_s - 0.35).max(0.1);
            audio_filters.push(format!("afade=t=in:st=0:d=0.35,afade=t=out:st={fade_out_st:.3}:d=0.35"));
        }
    }

    let music_bed_owned: Option<(PathBuf, f64)> = input
        .music_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "mp3" | "wav" | "m4a" | "aac" | "ogg" | "flac"))
        })
        .map(|p| (p, input.music_volume_db.filter(|v| v.is_finite()).unwrap_or(-18.0)));

    let src_clone = source_path.clone();
    let tmp_clone = tmp_path.clone();
    let final_clone = final_path.clone();
    let srt_cleanup = temp_srt_path.clone();

    let render = tokio::task::spawn_blocking(move || -> Result<(u64, String), String> {
        let mut chosen_encoder = "libx264".to_owned();
        let music_ref = music_bed_owned.as_ref().map(|(p, db)| (p.as_path(), *db));
        let res = if use_hw && let Some(hw) = cached_hw_encoder() {
            match run_ffmpeg_once(
                &ffmpeg,
                &src_clone,
                &tmp_clone,
                &framing,
                crop_pan,
                &quality,
                in_ms,
                out_ms,
                mute,
                &audio_filters,
                music_ref,
                &hw,
                &video_filters,
                effective_target_us,
            ) {
                Ok(()) => {
                    chosen_encoder = hw;
                    Ok(())
                }
                Err(e) if e == "Export cancelled." => Err(e),
                Err(_) => {
                    if let Ok(mut st) = export_state().lock() {
                        st.message = format!("{hw} unavailable for this clip — falling back to libx264…");
                        st.percent = 0;
                    }
                    chosen_encoder = "libx264 (CPU fallback)".to_owned();
                    run_ffmpeg_once(
                        &ffmpeg,
                        &src_clone,
                        &tmp_clone,
                        &framing,
                        crop_pan,
                        &quality,
                        in_ms,
                        out_ms,
                        mute,
                        &audio_filters,
                        music_ref,
                        "libx264",
                        &video_filters,
                        effective_target_us,
                    )
                }
            }
        } else {
            run_ffmpeg_once(
                &ffmpeg,
                &src_clone,
                &tmp_clone,
                &framing,
                crop_pan,
                &quality,
                in_ms,
                out_ms,
                mute,
                &audio_filters,
                music_ref,
                "libx264",
                &video_filters,
                effective_target_us,
            )
        };
        if let Some(p) = srt_cleanup {
            let _ = std::fs::remove_file(p);
        }
        res?;
        std::fs::rename(&tmp_clone, &final_clone)
            .map_err(|e| format!("Could not finalize exported MP4: {e}"))?;
        let len = std::fs::metadata(&final_clone)
            .map(|m| m.len())
            .map_err(|e| format!("Could not read exported MP4: {e}"))?;
        Ok((len, chosen_encoder))
    })
    .await
    .map_err(internal)?;

    let (bytes, encoder_used) = match render {
        Ok(v) => v,
        Err(msg) => {
            if manage_state && let Ok(mut st) = export_state().lock() {
                st.active = false;
                st.stage = if msg == "Export cancelled." {
                    "cancelled".to_owned()
                } else {
                    "error".to_owned()
                };
                st.message = msg.clone();
            }
            return Err(bad(msg));
        }
    };

    let mut caption_file = String::new();
    if input.save_caption_txt
        && let Some(text) = input.caption_text.as_deref()
        && !text.trim().is_empty()
    {
        let txt_name = format!("{stem}-{short_id}.txt");
        if std::fs::write(out_dir.join(&txt_name), text.trim()).is_ok() {
            caption_file = txt_name;
        }
    }

    let mut cover_file = String::new();
    if let Some(b64) = input.cover_png_base64.as_deref()
        && !b64.trim().is_empty()
        && let Some(png_bytes) = decode_base64(b64)
        && png_bytes.len() >= 8
        && png_bytes.len() <= 9 * 1024 * 1024
        && png_bytes.starts_with(b"\x89PNG\r\n\x1a\n")
    {
        let png_name = format!("{stem}-{short_id}-cover.png");
        if std::fs::write(out_dir.join(&png_name), &png_bytes).is_ok() {
            cover_file = png_name;
        }
    }

    let history_id = {
        let db = app.db.lock().map_err(internal)?;
        db.execute(
            "UPDATE workspace_film_roll SET exported_path=?1, review_state='exported' WHERE clip_id=?2",
            params![file_name, clip_id],
        )
        .map_err(internal)?;
        db.execute(
            "INSERT INTO workspace_export_history(
                clip_id, title, source_name, file_name, caption_file, cover_file,
                framing, quality, in_ms, out_ms, muted, normalized_audio, bytes, exported_at
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, unixepoch())",
            params![
                clip_id,
                if title.is_empty() { &file_name } else { &title },
                source_name,
                file_name,
                caption_file,
                cover_file,
                input.framing,
                input.quality,
                input.in_ms.unwrap_or(0) as i64,
                input.out_ms.unwrap_or(0) as i64,
                i64::from(input.mute),
                i64::from(normalize_audio),
                bytes as i64,
            ],
        )
        .map_err(internal)?;
        db.last_insert_rowid()
    };
    if manage_state && let Ok(mut st) = export_state().lock() {
        st.active = false;
        st.percent = 100;
        st.stage = "done".to_owned();
        st.encoder = encoder_used.clone();
        st.message = format!("Exported {file_name}");
    }
    Ok(json!({
        "clip_id": clip_id,
        "history_id": history_id,
        "file_name": file_name,
        "caption_file": if caption_file.is_empty() { None } else { Some(caption_file) },
        "cover_file": if cover_file.is_empty() { None } else { Some(cover_file) },
        "bytes": bytes,
        "encoder": encoder_used,
        "export_url": format!("/api/workspace/film-roll/{clip_id}/export"),
        "state": "exported",
    }))
}

pub(crate) async fn export_clip(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
    Json(input): Json<ExportRequest>,
) -> ApiResult<Json<Value>> {
    if export_state().lock().is_ok_and(|s| s.active) {
        return Err((
            StatusCode::CONFLICT,
            "Another export is already in progress. Wait or cancel it first.".into(),
        ));
    }
    Ok(Json(render_clip_internal(&app, &clip_id, &input, true).await?))
}

pub(crate) async fn export_progress() -> Json<Value> {
    let st = export_state()
        .lock()
        .map(|s| s.clone())
        .unwrap_or_default();
    Json(json!({
        "active": st.active,
        "clip_id": st.clip_id,
        "title": st.title,
        "percent": st.percent,
        "stage": st.stage,
        "message": st.message,
        "encoder": st.encoder,
        "batch_index": st.batch_index,
        "batch_total": st.batch_total,
        "batch_completed": st.batch_completed,
        "batch_failed": st.batch_failed,
    }))
}

pub(crate) async fn cancel_export() -> Json<Value> {
    export_cancel_flag().store(true, Ordering::Relaxed);
    let pid = export_state().lock().ok().and_then(|s| s.active_pid);
    if let Some(pid) = pid {
        tokio::task::spawn_blocking(move || kill_pid(pid)).await.ok();
    }
    if let Ok(mut st) = export_state().lock() {
        st.message = "Cancelling export…".to_owned();
    }
    Json(json!({ "cancelled": true }))
}

pub(crate) async fn export_batch(
    State(app): State<App>,
    Json(input): Json<BatchExportRequest>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    if export_state().lock().is_ok_and(|s| s.active) {
        return Err((
            StatusCode::CONFLICT,
            "An export is already in progress. Wait or cancel it first.".into(),
        ));
    }
    let targets: Vec<(String, String, String)> = {
        let db = app.db.lock().map_err(internal)?;
        if !input.clip_ids.is_empty() {
            let mut list = Vec::new();
            for id in input.clip_ids.iter().take(100) {
                if !crate::workspace_media::hex(id) {
                    continue;
                }
                if let Ok(Some(row)) = db
                    .query_row(
                        "SELECT clip_id, title, description FROM workspace_film_roll WHERE clip_id=?1",
                        [id],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
                    )
                    .optional()
                {
                    list.push(row);
                }
            }
            list
        } else {
            let want = input.state.as_deref().unwrap_or("approved");
            let mut stmt = db
                .prepare(
                    "SELECT f.clip_id, f.title, f.description
                     FROM workspace_film_roll f
                     JOIN profiles p ON p.id = f.source_id
                     WHERE p.archived = 0 AND p.role = 'source' AND f.review_state = ?1
                     ORDER BY f.added_at DESC
                     LIMIT 100",
                )
                .map_err(internal)?;
            let mapped = stmt
                .query_map([want], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })
                .map_err(internal)?;
            mapped
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(internal)?
        }
    };
    if targets.is_empty() {
        return Err(bad("No matching clips to export."));
    }
    let total = targets.len();
    export_cancel_flag().store(false, Ordering::Relaxed);
    if let Ok(mut st) = export_state().lock() {
        *st = ExportProgressState {
            active: true,
            clip_id: targets[0].0.clone(),
            title: targets[0].1.clone(),
            percent: 0,
            stage: "batch_rendering".to_owned(),
            message: format!("Exporting 1 of {total}…"),
            encoder: String::new(),
            batch_index: 1,
            batch_total: total,
            batch_completed: 0,
            batch_failed: 0,
            active_pid: None,
        };
    }
    let framing = input.framing.clone();
    let quality = input.quality.clone();
    let mute = input.mute;
    let normalize_audio = input.normalize_audio;
    let use_hw_encode = input.use_hw_encode;
    let save_caption_txt = input.save_caption_txt;
    let hook_text = input.hook_text.clone();
    let watermark_text = input.watermark_text.clone();
    let watermark_pos = input.watermark_pos.clone();
    let fade_io = input.fade_io;
    let vivid_filter = input.vivid_filter;
    let speed = input.speed;
    let app_clone = app.clone();

    tokio::spawn(async move {
        let mut completed = 0usize;
        let mut failed = 0usize;
        for (idx, (clip_id, title, desc)) in targets.into_iter().enumerate() {
            if export_cancel_flag().load(Ordering::Relaxed) {
                break;
            }
            if let Ok(mut st) = export_state().lock() {
                st.clip_id = clip_id.clone();
                st.title = title.clone();
                st.percent = 0;
                st.batch_index = idx + 1;
                st.message = format!("Exporting {} of {total}: {title}", idx + 1);
            }
            let caption_text = if save_caption_txt {
                let mut text = String::new();
                if !title.trim().is_empty() {
                    text.push_str(title.trim());
                }
                if !desc.trim().is_empty() {
                    if !text.is_empty() {
                        text.push_str("\n\n");
                    }
                    text.push_str(desc.trim());
                }
                (!text.is_empty()).then_some(text)
            } else {
                None
            };
            let req = ExportRequest {
                in_ms: None,
                out_ms: None,
                framing: framing.clone(),
                quality: quality.clone(),
                mute,
                normalize_audio,
                use_hw_encode,
                save_caption_txt,
                caption_text,
                cover_png_base64: None,
                hook_text: hook_text.clone(),
                watermark_text: watermark_text.clone(),
                watermark_pos: watermark_pos.clone(),
                fade_io,
                vivid_filter,
                speed,
                srt_text: None,
                subtitle_style: None,
                crop_pan: None,
                rotate_deg: None,
                mirror_h: false,
                subtle_zoom: false,
                outro_cta: None,
                audio_preset: None,
                audio_gain_db: None,
                music_path: None,
                music_volume_db: None,
                max_duration_s: None,
                force_30fps: false,
            };
            match render_clip_internal(&app_clone, &clip_id, &req, false).await {
                Ok(_) => completed += 1,
                Err(_) => failed += 1,
            }
            if let Ok(mut st) = export_state().lock() {
                st.batch_completed = completed;
                st.batch_failed = failed;
                st.percent = 100;
            }
        }
        let cancelled = export_cancel_flag().load(Ordering::Relaxed);
        if let Ok(mut st) = export_state().lock() {
            st.active = false;
            st.stage = if cancelled {
                "cancelled".to_owned()
            } else {
                "done".to_owned()
            };
            st.message = if cancelled {
                format!("Batch cancelled after {completed}/{total} exported.")
            } else if failed > 0 {
                format!("Batch finished: {completed} exported, {failed} failed.")
            } else {
                format!("Batch finished: {completed} clip{} exported.", if completed == 1 { "" } else { "s" })
            };
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "started": true, "total": total })),
    ))
}

pub(crate) async fn list_presets(State(app): State<App>) -> ApiResult<Json<Value>> {
    let db = app.db.lock().map_err(internal)?;
    let mut stmt = db
        .prepare(
            "SELECT id, name, title_prefix, caption_body, tags
             FROM workspace_caption_presets
             ORDER BY name COLLATE NOCASE",
        )
        .map_err(internal)?;
    let items: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "id": r.get::<_, i64>(0)?,
                "name": r.get::<_, String>(1)?,
                "title_prefix": r.get::<_, String>(2)?,
                "caption_body": r.get::<_, String>(3)?,
                "tags": r.get::<_, String>(4)?,
            }))
        })
        .map_err(internal)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(internal)?;
    Ok(Json(json!({ "presets": items })))
}

pub(crate) async fn save_preset(
    State(app): State<App>,
    Json(input): Json<CaptionPresetInput>,
) -> ApiResult<Json<Value>> {
    let name = input.name.trim();
    if name.is_empty() || name.len() > 60 {
        return Err(bad("Preset name must be 1–60 characters."));
    }
    let db = app.db.lock().map_err(internal)?;
    db.execute(
        "INSERT INTO workspace_caption_presets(name, title_prefix, caption_body, tags)
         VALUES(?1, ?2, ?3, ?4)
         ON CONFLICT(name) DO UPDATE SET
           title_prefix=excluded.title_prefix,
           caption_body=excluded.caption_body,
           tags=excluded.tags",
        params![
            name,
            input.title_prefix.trim(),
            input.caption_body.trim(),
            input.tags.trim(),
        ],
    )
    .map_err(internal)?;
    let id: i64 = db
        .query_row(
            "SELECT id FROM workspace_caption_presets WHERE name=?1",
            [name],
            |r| r.get(0),
        )
        .map_err(internal)?;
    Ok(Json(json!({ "id": id, "name": name })))
}

pub(crate) async fn delete_preset(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<StatusCode> {
    let db = app.db.lock().map_err(internal)?;
    db.execute("DELETE FROM workspace_caption_presets WHERE id=?1", [id])
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn export_history(State(app): State<App>) -> ApiResult<Json<Value>> {
    let dir = exports_dir(&app);
    let rows = {
        let db = app.db.lock().map_err(internal)?;
        let mut stmt = db
            .prepare(
                "SELECT id, clip_id, title, source_name, file_name, caption_file, cover_file,
                        framing, quality, in_ms, out_ms, muted, normalized_audio, bytes, exported_at,
                        youtube_video_id, ig_permalink, tiktok_publish_id, x_tweet_id
                 FROM workspace_export_history
                 ORDER BY exported_at DESC, id DESC
                 LIMIT 200",
            )
            .map_err(internal)?;
        let mapped = stmt
            .query_map([], |r| {
                let file_name: String = r.get(4)?;
                let caption_file: String = r.get(5)?;
                let cover_file: String = r.get(6)?;
                let yt_id: String = r.get(15)?;
                let ig_link: String = r.get(16)?;
                let tt_id: String = r.get(17)?;
                let x_id: String = r.get(18)?;
                Ok(json!({
                    "id": r.get::<_, i64>(0)?,
                    "clip_id": r.get::<_, String>(1)?,
                    "title": r.get::<_, String>(2)?,
                    "source_name": r.get::<_, String>(3)?,
                    "file_name": file_name,
                    "exists": dir.join(&file_name).is_file(),
                    "caption_file": if caption_file.is_empty() { None } else { Some(caption_file) },
                    "cover_file": if cover_file.is_empty() { None } else { Some(cover_file) },
                    "framing": r.get::<_, String>(7)?,
                    "quality": r.get::<_, String>(8)?,
                    "in_ms": r.get::<_, i64>(9)?,
                    "out_ms": r.get::<_, i64>(10)?,
                    "muted": r.get::<_, i64>(11)? != 0,
                    "normalized_audio": r.get::<_, i64>(12)? != 0,
                    "bytes": r.get::<_, i64>(13)?,
                    "exported_at": r.get::<_, i64>(14)?,
                    "youtube_video_id": if yt_id.is_empty() { None } else { Some(yt_id) },
                    "ig_permalink": if ig_link.is_empty() { None } else { Some(ig_link) },
                    "tiktok_publish_id": if tt_id.is_empty() { None } else { Some(tt_id) },
                    "x_tweet_id": if x_id.is_empty() { None } else { Some(x_id) },
                }))
            })
            .map_err(internal)?;
        mapped
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(internal)?
    };
    Ok(Json(json!({
        "exports_dir": dir.to_string_lossy(),
        "items": rows
    })))
}

pub(crate) async fn reveal_export(
    State(app): State<App>,
    AxumPath(history_id): AxumPath<i64>,
) -> ApiResult<StatusCode> {
    let file_name: Option<String> = {
        let db = app.db.lock().map_err(internal)?;
        db.query_row(
            "SELECT file_name FROM workspace_export_history WHERE id=?1",
            [history_id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(internal)?
    };
    let Some(name) = file_name
        .filter(|s| !s.is_empty() && !s.contains(['/', '\\', ':', '\0']) && !s.starts_with('.'))
    else {
        return Err((StatusCode::NOT_FOUND, "Export entry not found.".into()));
    };
    let target = exports_dir(&app).join(&name);
    if !target.is_file() {
        return Err((StatusCode::NOT_FOUND, "Exported file is no longer on disk.".into()));
    }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", target.display()))
            .spawn()
            .map_err(internal)?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R"])
            .arg(&target)
            .spawn()
            .map_err(internal)?;
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(exports_dir(&app))
            .spawn()
            .map_err(internal)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn clear_thumbnail_cache(State(app): State<App>) -> ApiResult<Json<Value>> {
    let cache_dir = app.media.join("thumbnail-cache");
    let (removed, freed) = tokio::task::spawn_blocking(move || -> (u64, u64) {
        let mut removed = 0u64;
        let mut freed = 0u64;
        let Ok(entries) = std::fs::read_dir(&cache_dir) else {
            return (0, 0);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_jpg = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jpg"));
            if !is_jpg {
                continue;
            }
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if std::fs::remove_file(&path).is_ok() {
                removed += 1;
                freed = freed.saturating_add(len);
            }
        }
        (removed, freed)
    })
    .await
    .map_err(internal)?;
    Ok(Json(json!({
        "removed_files": removed,
        "freed_bytes": freed,
        "media_untouched": true
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
    let (source_root, downloader, total, pending, approved, hold, exported, history_count) = {
        let db = app.db.lock().map_err(internal)?;
        let cfg = source::config(&db).map_err(internal)?;
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
        let history_count: i64 = db
            .query_row("SELECT COUNT(*) FROM workspace_export_history", [], |r| r.get(0))
            .unwrap_or(0);
        (cfg.root, cfg.downloader, counts.0, counts.1, counts.2, counts.3, counts.4, history_count)
    };
    let exports_path = exports_dir(&app);
    let thumb_path = app.media.join("thumbnail-cache");
    let src_root_clone = source_root.clone();
    let dl_clone = downloader.clone();
    let exp_clone = exports_path.clone();

    let probed = tokio::task::spawn_blocking(move || {
        let ffmpeg_path = find_ffmpeg();
        let ffprobe_path = find_ffprobe();
        let gallery_dl_path = find_gallery_dl();
        let ytdlp_path = (!dl_clone.is_empty() && Path::new(&dl_clone).is_file())
            .then(|| PathBuf::from(&dl_clone));
        let ffmpeg_ver = cached_ffmpeg_version();
        let hw_encoder = cached_hw_encoder();
        let ytdlp_ver = ytdlp_path.as_deref().and_then(cached_ytdlp_version);
        let (exports_bytes, exports_files) = dir_stats(&exp_clone, 2);
        let (thumb_bytes, thumb_files) = dir_stats(&thumb_path, 2);
        let (sources_bytes, sources_files) = if !src_root_clone.is_empty() {
            dir_stats(Path::new(&src_root_clone), 4)
        } else {
            (0, 0)
        };
        json!({
            "ffmpeg_ready": ffmpeg_path.is_some(),
            "ffprobe_ready": ffprobe_path.is_some(),
            "ytdlp_ready": ytdlp_path.is_some(),
            "gallery_dl_ready": gallery_dl_path.is_some(),
            "hw_encoder": hw_encoder,
            "tools": {
                "ffmpeg": { "ready": ffmpeg_path.is_some(), "path": ffmpeg_path.map(|p| p.to_string_lossy().into_owned()), "version": ffmpeg_ver },
                "ffprobe": { "ready": ffprobe_path.is_some(), "path": ffprobe_path.map(|p| p.to_string_lossy().into_owned()) },
                "ytdlp": { "ready": ytdlp_path.is_some(), "path": ytdlp_path.map(|p| p.to_string_lossy().into_owned()), "version": ytdlp_ver },
                "gallery_dl": { "ready": gallery_dl_path.is_some(), "path": gallery_dl_path.map(|p| p.to_string_lossy().into_owned()) },
            },
            "storage": {
                "sources_bytes": sources_bytes,
                "sources_files": sources_files,
                "exports_bytes": exports_bytes,
                "exports_files": exports_files,
                "thumb_cache_bytes": thumb_bytes,
                "thumb_cache_files": thumb_files,
            }
        })
    })
    .await
    .map_err(internal)?;

    Ok(Json(json!({
        "ffmpeg_ready": probed["ffmpeg_ready"],
        "ffprobe_ready": probed["ffprobe_ready"],
        "ytdlp_ready": probed["ytdlp_ready"],
        "gallery_dl_ready": probed["gallery_dl_ready"],
        "hw_encoder": probed["hw_encoder"],
        "tools": probed["tools"],
        "storage": probed["storage"],
        "source_root": source_root,
        "exports_dir": exports_path.to_string_lossy(),
        "film_roll_total": total,
        "pending_count": pending,
        "approved_count": approved,
        "hold_count": hold,
        "exported_count": exported,
        "export_history_count": history_count,
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
                "db_id": id,
                "name": name,
                "platform": platform,
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
    struct RollRow {
        clip_id: String,
        source_id: i64,
        path: String,
        title: String,
        description: String,
        saved_bytes: i64,
        review_state: String,
        exported_path: String,
        source_name: String,
        source_platform: String,
        yt_video_id: String,
        yt_publish_at: String,
        ig_media_id: String,
        ig_permalink: String,
        tt_publish_id: String,
        x_tweet_id: String,
        rating: i64,
        mood_tags: String,
        markers_json: String,
        content_sha256: String,
    }
    let entries: Vec<RollRow> = {
        let db = app.db.lock().map_err(|_| store::Error("storage_unavailable"))?;
        let mut stmt = db
            .prepare(
                "SELECT f.clip_id, f.source_id, f.path, f.title, f.description, f.bytes,
                        f.review_state, f.exported_path, p.name, p.platform,
                        f.yt_video_id, f.yt_publish_at, f.ig_media_id, f.ig_permalink,
                        f.tt_publish_id, f.x_tweet_id, f.rating, f.mood_tags, f.markers_json, f.content_sha256
                 FROM workspace_film_roll f
                 JOIN profiles p ON p.id = f.source_id
                 WHERE p.archived = 0 AND p.role = 'source'
                 ORDER BY f.added_at DESC, f.clip_id
                 LIMIT 500",
            )
            .map_err(|_| store::Error("storage_unavailable"))?;
        let mapped = stmt
            .query_map([], |r| {
                Ok(RollRow {
                    clip_id: r.get(0)?,
                    source_id: r.get(1)?,
                    path: r.get(2)?,
                    title: r.get(3)?,
                    description: r.get(4)?,
                    saved_bytes: r.get(5)?,
                    review_state: r.get(6)?,
                    exported_path: r.get(7)?,
                    source_name: r.get(8)?,
                    source_platform: r.get(9)?,
                    yt_video_id: r.get(10)?,
                    yt_publish_at: r.get(11)?,
                    ig_media_id: r.get(12)?,
                    ig_permalink: r.get(13)?,
                    tt_publish_id: r.get(14)?,
                    x_tweet_id: r.get(15)?,
                    rating: r.get(16)?,
                    mood_tags: r.get(17)?,
                    markers_json: r.get(18)?,
                    content_sha256: r.get(19)?,
                })
            })
            .map_err(|_| store::Error("storage_unavailable"))?;
        mapped
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|_| store::Error("storage_unavailable"))?
    };

    let out_dir = exports_dir(app);
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        let Ok(resolved) = source::resolve_file(app, e.source_id, &e.path, "source") else {
            continue;
        };
        let bytes = std::fs::metadata(&resolved)
            .map(|m| m.len())
            .unwrap_or(e.saved_bytes.max(0) as u64);
        let name = if e.title.is_empty() {
            e.path.rsplit(['\\', '/']).next().unwrap_or("Downloaded video").to_owned()
        } else {
            e.title
        };
        let has_export = !e.exported_path.is_empty() && out_dir.join(&e.exported_path).is_file();
        let mut row = json!({
            "kind": "original",
            "clip_id": e.clip_id,
            "source_id": e.source_id,
            "source_name": e.source_name,
            "source_platform": e.source_platform,
            "source_path": e.path,
            "name": name,
            "state": e.review_state,
            "preview_only": true,
            "preview_available": true,
            "poster_available": true,
            "preview_bytes": bytes,
            "preview_sha256": e.clip_id,
            "exported": has_export,
            "exported_file": if has_export { Some(e.exported_path) } else { None },
            "export_url": if has_export { Some(format!("/api/workspace/film-roll/{}/export", e.clip_id)) } else { None },
            "yt_video_id": if e.yt_video_id.is_empty() { None } else { Some(e.yt_video_id) },
            "yt_publish_at": if e.yt_publish_at.is_empty() { None } else { Some(e.yt_publish_at) },
            "ig_media_id": if e.ig_media_id.is_empty() { None } else { Some(e.ig_media_id) },
            "ig_permalink": if e.ig_permalink.is_empty() { None } else { Some(e.ig_permalink) },
            "tt_publish_id": if e.tt_publish_id.is_empty() { None } else { Some(e.tt_publish_id) },
            "x_tweet_id": if e.x_tweet_id.is_empty() { None } else { Some(e.x_tweet_id) },
            "rating": e.rating.clamp(0, 5),
            "mood_tags": e.mood_tags,
            "markers": serde_json::from_str::<Value>(&e.markers_json).unwrap_or_else(|_| json!([])),
            "content_sha256": if e.content_sha256.is_empty() { None } else { Some(e.content_sha256) },
            "from_sources": true,
        });
        if !e.description.is_empty() {
            row["import_metadata"] = json!({ "description": e.description });
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

pub(crate) async fn filmstrip(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
) -> Response {
    if !crate::workspace_media::hex(&clip_id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(video_path) = resolve_video(&app, &clip_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let cache_dir = app.media.join("thumbnail-cache");
    let strip_path = cache_dir.join(format!("filmstrip_{clip_id}.jpg"));
    if let Ok(bytes) = std::fs::read(&strip_path)
        && bytes.len() > 256
    {
        return (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "public, max-age=3600"),
            ],
            bytes,
        )
            .into_response();
    }
    let Some(ffmpeg) = find_ffmpeg() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let dur_ms = probed_duration_ms(&app, &clip_id).unwrap_or(16_000);
    let dur_s = (dur_ms as f64 / 1000.0).max(1.0);
    let fps = (8.0 / dur_s).clamp(0.05, 8.0);
    let out_clone = strip_path.clone();
    let ok = tokio::task::spawn_blocking(move || -> Option<Vec<u8>> {
        let _ = std::fs::create_dir_all(&cache_dir);
        let tmp = cache_dir.join(format!("filmstrip_{clip_id}.tmp.jpg"));
        let vf = format!(
            "fps={fps:.4},scale=96:54:force_original_aspect_ratio=increase,crop=96:54,tile=8x1"
        );
        let mut cmd = std::process::Command::new(&ffmpeg);
        cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
            .arg(&video_path)
            .args(["-vf", &vf, "-frames:v", "1", "-q:v", "6"])
            .arg(&tmp)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let status = cmd.status().ok()?;
        if !status.success() {
            let _ = std::fs::remove_file(&tmp);
            return None;
        }
        let bytes = std::fs::read(&tmp).ok()?;
        let _ = std::fs::rename(&tmp, &out_clone);
        Some(bytes)
    })
    .await
    .ok()
    .flatten();
    match ok {
        Some(bytes) if bytes.len() > 256 => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "public, max-age=3600"),
            ],
            bytes,
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(crate) async fn detect_silence(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let Some(video_path) = resolve_video(&app, &clip_id) else {
        return Err((StatusCode::NOT_FOUND, "Source video is unavailable.".into()));
    };
    let Some(ffmpeg) = find_ffmpeg() else {
        return Err(bad("FFmpeg was not found."));
    };
    let known_dur_ms = probed_duration_ms(&app, &clip_id);
    let result = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let mut cmd = std::process::Command::new(&ffmpeg);
        cmd.args(["-hide_banner", "-i"])
            .arg(&video_path)
            .args(["-af", "silencedetect=noise=-35dB:d=0.35", "-f", "null", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let out = cmd
            .output()
            .map_err(|e| format!("Failed to run silence detection: {e}"))?;
        let stderr = String::from_utf8_lossy(&out.stderr);
        let mut segments: Vec<(f64, Option<f64>)> = Vec::new();
        let mut parsed_dur_s: Option<f64> = known_dur_ms.map(|d| d as f64 / 1000.0);
        for line in stderr.lines() {
            if parsed_dur_s.is_none()
                && let Some(pos) = line.find("Duration:")
            {
                let rest = line[pos + 9..].trim();
                if let Some(hms) = rest.split(',').next() {
                    let parts: Vec<f64> = hms.trim().split(':').filter_map(|p| p.parse().ok()).collect();
                    if parts.len() == 3 {
                        parsed_dur_s = Some(parts[0] * 3600.0 + parts[1] * 60.0 + parts[2]);
                    }
                }
            }
            if let Some(idx) = line.find("silence_start:") {
                if let Some(num) = line[idx + 14..].split_whitespace().next().and_then(|s| s.parse::<f64>().ok()) {
                    segments.push((num.max(0.0), None));
                }
            } else if let Some(idx) = line.find("silence_end:")
                && let Some(num) = line[idx + 12..].split_whitespace().next().and_then(|s| s.parse::<f64>().ok())
                && let Some(last) = segments.last_mut()
                && last.1.is_none()
            {
                last.1 = Some(num.max(last.0));
            }
        }
        let dur_s = parsed_dur_s.unwrap_or(0.0);
        let dur_ms = (dur_s * 1000.0).round() as u64;
        let mut in_ms = 0u64;
        let mut out_ms = dur_ms;
        if let Some((s, Some(e))) = segments.first()
            && *s <= 0.25
            && *e < dur_s - 0.5
        {
            in_ms = ((*e - 0.08).max(0.0) * 1000.0).round() as u64;
        }
        if let Some((s, end_opt)) = segments.last() {
            let touches_end = end_opt.map(|e| e >= dur_s - 0.35).unwrap_or(true);
            if touches_end && *s > (in_ms as f64 / 1000.0) + 0.5 {
                out_ms = ((*s + 0.08).min(dur_s) * 1000.0).round() as u64;
            }
        }
        let seg_json: Vec<Value> = segments
            .iter()
            .map(|(s, e)| {
                json!({
                    "start_ms": (*s * 1000.0).round() as u64,
                    "end_ms": (e.unwrap_or(dur_s) * 1000.0).round() as u64
                })
            })
            .collect();
        Ok(json!({
            "duration_ms": dur_ms,
            "suggested_in_ms": in_ms,
            "suggested_out_ms": out_ms,
            "trimmed_start_ms": in_ms,
            "trimmed_end_ms": dur_ms.saturating_sub(out_ms),
            "silence_segments": seg_json
        }))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;
    Ok(Json(result))
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
        let b2 = if i + 2 < bytes.len() { bytes[i + 2] } else { 0 };
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(if i + 1 < bytes.len() { TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char } else { '=' });
        out.push(if i + 2 < bytes.len() { TABLE[(b2 & 0x3f) as usize] as char } else { '=' });
        i += 3;
    }
    out
}

pub(crate) async fn update_meta(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
    Json(input): Json<MetaUpdateRequest>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let db = app.db.lock().map_err(internal)?;
    if let Some(rating) = input.rating {
        let clamped = rating.clamp(0, 5);
        db.execute("UPDATE workspace_film_roll SET rating=?1 WHERE clip_id=?2", params![clamped, clip_id])
            .map_err(internal)?;
    }
    if let Some(mood) = input.mood_tags {
        let clean: String = mood.trim().chars().take(160).collect();
        db.execute("UPDATE workspace_film_roll SET mood_tags=?1 WHERE clip_id=?2", params![clean, clip_id])
            .map_err(internal)?;
    }
    if let Some(markers) = input.markers {
        let capped: Vec<Value> = markers.into_iter().take(40).collect();
        let raw = serde_json::to_string(&capped).unwrap_or_else(|_| "[]".to_owned());
        db.execute("UPDATE workspace_film_roll SET markers_json=?1 WHERE clip_id=?2", params![raw, clip_id])
            .map_err(internal)?;
    }
    Ok(Json(json!({ "ok": true, "clip_id": clip_id })))
}

pub(crate) async fn cover_candidates(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let Some(video_path) = resolve_video(&app, &clip_id) else {
        return Err((StatusCode::NOT_FOUND, "Source video unavailable.".into()));
    };
    let Some(ffmpeg) = find_ffmpeg() else {
        return Err(bad("FFmpeg was not found."));
    };
    let dur_ms = probed_duration_ms(&app, &clip_id).unwrap_or(15_000).max(2000);
    let cache_dir = app.media.join("thumbnail-cache");
    let candidates = tokio::task::spawn_blocking(move || -> Vec<Value> {
        let _ = std::fs::create_dir_all(&cache_dir);
        let mut out = Vec::new();
        for (idx, frac) in [0.15f64, 0.45f64, 0.78f64].into_iter().enumerate() {
            let offset_ms = ((dur_ms as f64) * frac).round() as u64;
            let seek_s = format!("{:.3}", offset_ms as f64 / 1000.0);
            let tmp = cache_dir.join(format!("cand_{clip_id}_{idx}.jpg"));
            let mut cmd = std::process::Command::new(&ffmpeg);
            cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-ss", &seek_s, "-i"])
                .arg(&video_path)
                .args([
                    "-vf",
                    "thumbnail=n=15,scale=540:960:force_original_aspect_ratio=decrease,pad=540:960:(ow-iw)/2:(oh-ih)/2:black",
                    "-frames:v",
                    "1",
                    "-q:v",
                    "4",
                ])
                .arg(&tmp)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000);
            }
            if cmd.status().is_ok_and(|s| s.success())
                && let Ok(bytes) = std::fs::read(&tmp)
                && bytes.len() > 256
            {
                out.push(json!({
                    "thumb_offset_ms": offset_ms,
                    "label": format!("{:.1}s", offset_ms as f64 / 1000.0),
                    "data_url": format!("data:image/jpeg;base64,{}", encode_base64(&bytes))
                }));
            }
            let _ = std::fs::remove_file(&tmp);
        }
        out
    })
    .await
    .map_err(internal)?;
    Ok(Json(json!({ "candidates": candidates })))
}

pub(crate) async fn split_parts(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
    Json(input): Json<SplitPartsRequest>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let Some(source_path) = resolve_video(&app, &clip_id) else {
        return Err((StatusCode::NOT_FOUND, "Source video unavailable.".into()));
    };
    let Some(ffmpeg) = find_ffmpeg() else {
        return Err(bad("FFmpeg not found."));
    };
    let dur_ms = probed_duration_ms(&app, &clip_id).unwrap_or(0);
    if dur_ms < 10_000 {
        return Err(bad("Clip is shorter than 10 seconds — no need to split."));
    }
    let part_ms = input.part_seconds.clamp(15, 180) * 1000;
    let total_parts = ((dur_ms as f64) / (part_ms as f64)).ceil() as usize;
    if total_parts <= 1 {
        return Err(bad("Clip already fits inside a single part at this length."));
    }
    let total_parts = total_parts.min(10);
    let (title, source_name): (String, String) = {
        let db = app.db.lock().map_err(internal)?;
        db.query_row(
            "SELECT f.title, COALESCE(p.name, '') FROM workspace_film_roll f LEFT JOIN profiles p ON p.id=f.source_id WHERE f.clip_id=?1",
            [&clip_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
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
    let short_id = clip_id[..8].to_owned();
    let crop_pan = input.crop_pan.filter(|v| v.is_finite()).unwrap_or(0.0).clamp(-1.0, 1.0);
    let framing = input.framing.clone();
    let quality = input.quality.clone();
    let use_hw = input.use_hw_encode;
    let normalize_audio = input.normalize_audio;
    let fade_io = input.fade_io;
    let vivid = input.vivid_filter;
    let hook_prefix = input.hook_prefix.clone().unwrap_or_default();
    let wm = input.watermark_text.clone();
    let wm_pos = input.watermark_pos.clone();
    let app_clone = app.clone();
    let clip_owned = clip_id.clone();

    let created = tokio::task::spawn_blocking(move || -> Result<Vec<Value>, String> {
        let mut results = Vec::new();
        let hw = if use_hw { cached_hw_encoder() } else { None };
        let enc = hw.as_deref().unwrap_or("libx264");
        for idx in 0..total_parts {
            let start = (idx as u64) * part_ms;
            if start >= dur_ms {
                break;
            }
            let end = ((idx as u64 + 1) * part_ms).min(dur_ms);
            if end.saturating_sub(start) < 2000 {
                break;
            }
            let part_num = idx + 1;
            let file_name = format!("{stem}-{short_id}-part{part_num}.mp4");
            let tmp_path = out_dir.join(format!("{stem}-{short_id}-part{part_num}.tmp.mp4"));
            let final_path = out_dir.join(&file_name);
            let out_dur_s = (end - start) as f64 / 1000.0;
            let mut vf = Vec::new();
            if vivid {
                vf.push("eq=contrast=1.08:saturation=1.18:brightness=0.02,unsharp=3:3:0.5:3:3:0.0".to_owned());
            }
            if fade_io && out_dur_s > 1.2 {
                let fo = (out_dur_s - 0.35).max(0.1);
                vf.push(format!("fade=t=in:st=0:d=0.35,fade=t=out:st={fo:.3}:d=0.35"));
            }
            let part_hook = if hook_prefix.trim().is_empty() {
                format!("PART {part_num}/{total_parts}")
            } else {
                format!("PART {part_num}/{total_parts} · {}", hook_prefix.trim())
            };
            vf.extend(build_drawtext_filters(Some(&part_hook), wm.as_deref(), &wm_pos, None, out_dur_s));
            let mut af = Vec::new();
            if normalize_audio {
                af.push("loudnorm=I=-14:TP=-1.5:LRA=11".to_owned());
            }
            let target_us = (end - start).max(100) * 1000;
            let res = run_ffmpeg_once(
                &ffmpeg,
                &source_path,
                &tmp_path,
                &framing,
                crop_pan,
                &quality,
                Some(start),
                Some(end),
                false,
                &af,
                None,
                enc,
                &vf,
                target_us,
            );
            if res.is_err() && enc != "libx264" {
                run_ffmpeg_once(
                    &ffmpeg,
                    &source_path,
                    &tmp_path,
                    &framing,
                    crop_pan,
                    &quality,
                    Some(start),
                    Some(end),
                    false,
                    &af,
                    None,
                    "libx264",
                    &vf,
                    target_us,
                )?;
            } else {
                res?;
            }
            std::fs::rename(&tmp_path, &final_path).map_err(|e| e.to_string())?;
            let bytes = std::fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);
            let part_title = format!("{} (Part {part_num}/{total_parts})", if title.is_empty() { &stem } else { &title });
            if let Ok(db) = app_clone.db.lock() {
                let _ = db.execute(
                    "INSERT INTO workspace_export_history(clip_id, title, source_name, file_name, framing, quality, in_ms, out_ms, muted, normalized_audio, bytes)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?10)",
                    params![clip_owned, part_title, source_name, file_name, framing, quality, start as i64, end as i64, if normalize_audio { 1 } else { 0 }, bytes as i64],
                );
            }
            results.push(json!({ "part": part_num, "file_name": file_name, "bytes": bytes, "in_ms": start, "out_ms": end }));
        }
        Ok(results)
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;

    Ok(Json(json!({ "parts": created, "count": created.len() })))
}

pub(crate) async fn zip_kit(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
    Json(input): Json<ZipKitRequest>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let (title, exported_path): (String, String) = {
        let db = app.db.lock().map_err(internal)?;
        db.query_row(
            "SELECT title, exported_path FROM workspace_film_roll WHERE clip_id=?1",
            [&clip_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(internal)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Clip not found.".into()))?
    };
    let out_dir = exports_dir(&app);
    std::fs::create_dir_all(&out_dir).map_err(internal)?;
    let video_path = if !exported_path.is_empty() && out_dir.join(&exported_path).is_file() {
        out_dir.join(&exported_path)
    } else {
        resolve_video(&app, &clip_id).ok_or_else(|| (StatusCode::NOT_FOUND, "Video file unavailable.".into()))?
    };
    let stem = sanitize_stem(if title.is_empty() { "clip" } else { &title });
    let short_id = &clip_id[..8];
    let zip_name = format!("{stem}-{short_id}-post-kit.zip");
    let zip_path = out_dir.join(&zip_name);
    let mp4_entry_name = format!("{stem}-{short_id}.mp4");
    let cap_text = input.caption_text.unwrap_or_default();
    let cover_bytes = input
        .cover_png_base64
        .as_deref()
        .and_then(decode_base64)
        .filter(|b| b.starts_with(b"\x89PNG\r\n\x1a\n") && b.len() <= 9 * 1024 * 1024);

    let (bytes, final_zip) = tokio::task::spawn_blocking(move || -> Result<(u64, PathBuf), String> {
        let mp4_bytes = std::fs::read(&video_path).map_err(|e| format!("Could not read video: {e}"))?;
        if mp4_bytes.len() > 250 * 1024 * 1024 {
            return Err("Video exceeds 250 MiB post-kit limit.".to_owned());
        }
        let mut entries: Vec<(&str, &[u8])> = vec![(&mp4_entry_name, &mp4_bytes)];
        let cap_bytes = cap_text.as_bytes();
        if !cap_text.trim().is_empty() {
            entries.push(("captions.txt", cap_bytes));
        }
        if let Some(ref cov) = cover_bytes {
            entries.push(("cover-1080x1920.png", cov));
        }
        let zip_data = crate::workspace_phase1::zip(&entries);
        std::fs::write(&zip_path, &zip_data).map_err(|e| format!("Could not write zip kit: {e}"))?;
        Ok((zip_data.len() as u64, zip_path))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;

    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", final_zip.display()))
            .spawn();
    }
    Ok(Json(json!({ "zip_name": zip_name, "bytes": bytes })))
}

pub(crate) async fn pick_music() -> ApiResult<Json<Value>> {
    #[cfg(windows)]
    {
        let picked = tokio::task::spawn_blocking(|| -> Result<Option<String>, String> {
            use std::os::windows::process::CommandExt;
            let script = r#"Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Title = 'Select Background Music Track'; $d.Filter = 'Audio Files (*.mp3;*.wav;*.m4a;*.aac;*.ogg;*.flac)|*.mp3;*.wav;*.m4a;*.aac;*.ogg;*.flac|All Files (*.*)|*.*'; if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::Out.Write($d.FileName) }"#;
            let out = std::process::Command::new("powershell")
                .args(["-NoProfile", "-STA", "-Command", script])
                .creation_flags(0x08000000)
                .output()
                .map_err(|e| format!("Could not open file picker: {e}"))?;
            let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if path.is_empty() { Ok(None) } else { Ok(Some(path)) }
        })
        .await
        .map_err(internal)?
        .map_err(bad)?;
        let name = picked
            .as_deref()
            .and_then(|p| Path::new(p).file_name().and_then(|n| n.to_str()))
            .map(str::to_owned);
        Ok(Json(json!({ "path": picked, "name": name })))
    }
    #[cfg(not(windows))]
    {
        Err(bad("Music file picker is supported on Windows builds."))
    }
}

pub(crate) struct Mp4Summary {
    pub width: u32,
    pub height: u32,
    pub duration_s: f64,
    pub video_codec: String,
    pub audio_codec: String,
    pub audio_sample_rate: u32,
}

pub(crate) fn probe_mp4_summary(path: &Path) -> Option<Mp4Summary> {
    let ffprobe = find_ffprobe()?;
    let mut cmd = std::process::Command::new(&ffprobe);
    cmd.args([
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ])
    .arg(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let parsed: Value = serde_json::from_slice(&out.stdout).ok()?;
    let streams = parsed["streams"].as_array()?;
    let v = streams.iter().find(|s| s["codec_type"].as_str() == Some("video"));
    let a = streams.iter().find(|s| s["codec_type"].as_str() == Some("audio"));
    let width = v.and_then(|s| s["width"].as_u64()).unwrap_or(0) as u32;
    let height = v.and_then(|s| s["height"].as_u64()).unwrap_or(0) as u32;
    let duration_s = parsed["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| parsed["format"]["duration"].as_f64())
        .unwrap_or(0.0);
    let video_codec = v
        .and_then(|s| s["codec_name"].as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let audio_codec = a
        .and_then(|s| s["codec_name"].as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let audio_sample_rate = a
        .and_then(|s| {
            s["sample_rate"]
                .as_str()
                .and_then(|r| r.parse::<u32>().ok())
                .or_else(|| s["sample_rate"].as_u64().map(|n| n as u32))
        })
        .unwrap_or(0);
    Some(Mp4Summary {
        width,
        height,
        duration_s,
        video_codec,
        audio_codec,
        audio_sample_rate,
    })
}

pub(crate) fn prepare_instagram_safe_mp4(
    src: &Path,
    out_tmp: &Path,
    duration_s: f64,
    fast_fallback: bool,
) -> Result<(), String> {
    let ffmpeg = find_ffmpeg().ok_or_else(|| "FFmpeg not found in tools/ffmpeg.exe.".to_owned())?;
    // Meta's rupload.facebook.com synchronously validates/transcodes on the final chunk with a ~30s timeout.
    // Keep normal uploads crisp (1080p <=35s, 720p >35s), and when fast_fallback is triggered after a
    // Meta ProcessingFailedError timeout on long/high-motion clips, scale to 540x960 @ 24fps (~6-7 MiB)
    // so Meta's transcoder finishes well under 30s.
    let dur = if duration_s.is_finite() && duration_s > 1.0 {
        duration_s
    } else {
        60.0
    };
    let target_mib = if fast_fallback { 6.5 } else { 11.2 };
    let target_kbits = target_mib * 8192.0;
    let audio_kbps: u32 = if fast_fallback || dur > 90.0 { 96 } else { 128 };
    let v_kbps = ((target_kbits / dur) - (audio_kbps as f64 + 16.0)).clamp(420.0, 4500.0) as u32;
    let max_kbps = (v_kbps * 12 / 10).max(v_kbps + 80);
    let buf_kbps = max_kbps * 2;
    let bv_str = format!("{v_kbps}k");
    let max_str = format!("{max_kbps}k");
    let buf_str = format!("{buf_kbps}k");
    let ab_str = format!("{audio_kbps}k");
    let (scale_vf, target_fps) = if fast_fallback || dur > 120.0 {
        (
            Some("scale='min(540,iw)':-2:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2"),
            "24",
        )
    } else if dur > 75.0 {
        (
            Some("scale='min(720,iw)':-2:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2"),
            "24",
        )
    } else if dur > 35.0 {
        (
            Some("scale='min(720,iw)':-2:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2"),
            "30",
        )
    } else {
        (None, "30")
    };
    let run_pass = |enc: &str| -> Result<(), String> {
        let mut cmd = std::process::Command::new(&ffmpeg);
        cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
            .arg(src);
        if let Some(vf) = scale_vf {
            cmd.args(["-vf", vf]);
        }
        match enc {
            "h264_nvenc" => {
                cmd.args([
                    "-c:v", "h264_nvenc", "-preset", "p3", "-profile:v", "main", "-rc", "vbr",
                    "-b:v", &bv_str, "-maxrate", &max_str, "-bufsize", &buf_str, "-r", target_fps,
                    "-pix_fmt", "yuv420p",
                ]);
            }
            "h264_qsv" => {
                cmd.args([
                    "-c:v", "h264_qsv", "-b:v", &bv_str, "-maxrate", &max_str, "-bufsize",
                    &buf_str, "-r", target_fps, "-pix_fmt", "nv12",
                ]);
            }
            "h264_amf" => {
                cmd.args([
                    "-c:v", "h264_amf", "-quality", "balanced", "-b:v", &bv_str, "-maxrate",
                    &max_str, "-bufsize", &buf_str, "-r", target_fps, "-pix_fmt", "yuv420p",
                ]);
            }
            _ => {
                cmd.args([
                    "-c:v", "libx264", "-preset", "veryfast", "-profile:v", "main", "-level",
                    "4.0", "-b:v", &bv_str, "-maxrate", &max_str, "-bufsize", &buf_str, "-r",
                    target_fps, "-pix_fmt", "yuv420p",
                ]);
            }
        }
        cmd.args([
            "-ar", "48000", "-ac", "2", "-c:a", "aac", "-b:a", &ab_str, "-shortest", "-movflags",
            "+faststart",
        ])
        .arg(out_tmp);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let out = cmd
            .output()
            .map_err(|e| format!("Failed to start FFmpeg Reels optimizer: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            Err(if err.is_empty() {
                "FFmpeg Reels optimizer failed.".to_owned()
            } else {
                err
            })
        }
    };
    if let Some(hw) = cached_hw_encoder() {
        if run_pass(&hw).is_ok() {
            return Ok(());
        }
    }
    run_pass("libx264")
}

#[derive(Deserialize)]
pub(crate) struct AutoCaptionRequest {
    #[serde(default)]
    seed_text: Option<String>,
    #[serde(default)]
    in_ms: Option<u64>,
    #[serde(default)]
    out_ms: Option<u64>,
}

fn fmt_srt_time(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1000;
    let rem = ms % 1000;
    format!("{h:02}:{m:02}:{s:02},{rem:03}")
}

fn find_local_whisper() -> Option<(PathBuf, PathBuf)> {
    let exe_bin = find_tool_binary("SNAPMATIC_WHISPER", "whisper-cli.exe", "whisper-cli")
        .or_else(|| find_tool_binary("SNAPMATIC_WHISPER", "whisper.exe", "whisper"))
        .or_else(|| find_tool_binary("SNAPMATIC_WHISPER", "main.exe", "main"))?;
    let mut dirs = Vec::new();
    if let Some(p) = exe_bin.parent() {
        dirs.push(p.to_path_buf());
    }
    dirs.push(PathBuf::from("tools"));
    for d in dirs {
        if let Ok(entries) = std::fs::read_dir(&d) {
            for ent in entries.flatten() {
                let p = ent.path();
                if p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("bin"))
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.to_ascii_lowercase().contains("ggml"))
                {
                    return Some((exe_bin, p));
                }
            }
        }
    }
    None
}

pub(crate) async fn viral_moment(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let Some(source_path) = resolve_video(&app, &clip_id) else {
        return Err((StatusCode::NOT_FOUND, "Source video is unavailable.".into()));
    };
    let Some(ffmpeg) = find_ffmpeg() else {
        return Err(bad("FFmpeg not found in tools/ffmpeg.exe."));
    };
    let dur_ms_hint = probed_duration_ms(&app, &clip_id).unwrap_or(0);
    let result = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let dur_ms = if dur_ms_hint > 0 {
            dur_ms_hint
        } else {
            probe_mp4_summary(&source_path)
                .map(|s| (s.duration_s * 1000.0).round() as u64)
                .unwrap_or(60_000)
        };
        let mut cmd = std::process::Command::new(&ffmpeg);
        cmd.args([
            "-hide_banner",
            "-nostats",
            "-i",
        ])
        .arg(&source_path)
        .args([
            "-vn",
            "-af",
            "aresample=16000,asetnsamples=16000,astats=metadata=1:reset=1,ametadata=print:key=lavfi.astats.Overall.RMS_level:file=-",
            "-f",
            "null",
            "-",
        ]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let out = cmd
            .output()
            .map_err(|e| format!("Failed to analyze clip audio energy: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut buckets: Vec<(u64, f64)> = Vec::new();
        let mut cur_sec = 0u64;
        for line in text.lines() {
            if let Some(rest) = line.split("pts_time:").nth(1) {
                if let Some(t) = rest.split_whitespace().next().and_then(|s| s.parse::<f64>().ok()) {
                    cur_sec = t.max(0.0).round() as u64;
                }
            } else if let Some(val_str) = line.strip_prefix("lavfi.astats.Overall.RMS_level=") {
                if let Ok(db) = val_str.trim().parse::<f64>()
                    && db.is_finite()
                {
                    let energy = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
                    buckets.push((cur_sec * 1000, energy));
                }
            }
        }
        if buckets.is_empty() {
            let cap = dur_ms.min(58_000);
            return Ok(json!({
                "duration_ms": dur_ms,
                "peak_at_ms": cap / 2,
                "suggested_in_ms": 0,
                "suggested_out_ms": cap,
                "top_peaks": []
            }));
        }
        let win_len = 48usize.min(buckets.len()).max(1);
        let mut best_idx = 0usize;
        let mut best_score = -1.0f64;
        for i in 0..=buckets.len().saturating_sub(win_len) {
            let sum: f64 = buckets[i..i + win_len].iter().map(|(_, e)| *e).sum();
            let peak: f64 = buckets[i..i + win_len]
                .iter()
                .map(|(_, e)| *e)
                .fold(0.0f64, f64::max);
            let score = (sum / win_len as f64) * 0.65 + peak * 0.35;
            if score > best_score {
                best_score = score;
                best_idx = i;
            }
        }
        let suggested_in = buckets[best_idx].0.min(dur_ms.saturating_sub(3000));
        let suggested_out = (suggested_in + 56_000).min(dur_ms);
        let peak_ms = buckets[best_idx..best_idx + win_len]
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(ms, _)| *ms)
            .unwrap_or(suggested_in);

        let mut sorted = buckets.clone();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut top_peaks: Vec<Value> = Vec::new();
        for (ms, energy) in sorted {
            if top_peaks
                .iter()
                .all(|p| p["ms"].as_u64().unwrap_or(0).abs_diff(ms) >= 6000)
            {
                top_peaks.push(json!({
                    "ms": ms,
                    "intensity": (energy * 100.0).round() as u64
                }));
                if top_peaks.len() >= 3 {
                    break;
                }
            }
        }
        Ok(json!({
            "duration_ms": dur_ms,
            "peak_at_ms": peak_ms,
            "suggested_in_ms": suggested_in,
            "suggested_out_ms": suggested_out,
            "top_peaks": top_peaks
        }))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;
    Ok(Json(result))
}

pub(crate) async fn auto_captions(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
    Json(input): Json<AutoCaptionRequest>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let Some(source_path) = resolve_video(&app, &clip_id) else {
        return Err((StatusCode::NOT_FOUND, "Source video is unavailable.".into()));
    };
    let Some(ffmpeg) = find_ffmpeg() else {
        return Err(bad("FFmpeg not found in tools/ffmpeg.exe."));
    };
    let fallback_title: String = {
        let db = app.db.lock().map_err(internal)?;
        db.query_row(
            "SELECT title FROM workspace_film_roll WHERE clip_id=?1",
            [&clip_id],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_default()
    };
    let seed = input
        .seed_text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&fallback_title)
        .to_owned();
    let in_ms = input.in_ms.unwrap_or(0);
    let out_ms_opt = input.out_ms;
    let res = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let dur_ms = probe_mp4_summary(&source_path)
            .map(|s| (s.duration_s * 1000.0).round() as u64)
            .unwrap_or(30_000);
        let end_ms = out_ms_opt
            .filter(|&e| e > in_ms)
            .unwrap_or(dur_ms)
            .min(dur_ms.max(1000));
        let clip_span = end_ms.saturating_sub(in_ms).max(1500);

        // Path A: Full local Whisper AI transcription if whisper-cli.exe + ggml-*.bin exist in tools/
        if let Some((whisper_bin, model_path)) = find_local_whisper() {
            let stamp = uuid::Uuid::new_v4().simple();
            let wav_path = std::env::temp_dir().join(format!("snapmatic-cap-{stamp}.wav"));
            let out_base = std::env::temp_dir().join(format!("snapmatic-cap-{stamp}"));
            let mut ex = std::process::Command::new(&ffmpeg);
            ex.args(["-y", "-hide_banner", "-loglevel", "error"]);
            if in_ms > 0 {
                ex.args(["-ss", &format!("{:.3}", in_ms as f64 / 1000.0)]);
            }
            ex.arg("-i").arg(&source_path);
            ex.args([
                "-t",
                &format!("{:.3}", clip_span as f64 / 1000.0),
                "-vn",
                "-ar",
                "16000",
                "-ac",
                "1",
                "-c:a",
                "pcm_s16le",
            ])
            .arg(&wav_path);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                ex.creation_flags(0x08000000);
            }
            let ex_ok = ex.status().is_ok_and(|s| s.success());
            if ex_ok {
                let mut wcmd = std::process::Command::new(&whisper_bin);
                wcmd.arg("-m")
                    .arg(&model_path)
                    .arg("-f")
                    .arg(&wav_path)
                    .args(["-osrt", "-ml", "28", "-of"])
                    .arg(&out_base);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    wcmd.creation_flags(0x08000000);
                }
                let w_ok = wcmd.status().is_ok_and(|s| s.success());
                let srt_file = PathBuf::from(format!("{}.srt", out_base.display()));
                let srt_out = if w_ok {
                    std::fs::read_to_string(&srt_file).ok()
                } else {
                    None
                };
                let _ = std::fs::remove_file(&wav_path);
                let _ = std::fs::remove_file(&srt_file);
                if let Some(raw) = srt_out
                    && !raw.trim().is_empty()
                {
                    let cues = raw.matches(" --> ").count();
                    return Ok(json!({
                        "srt": raw.trim(),
                        "engine": "whisper-local",
                        "cue_count": cues,
                        "whisper_ready": true
                    }));
                }
            }
            let _ = std::fs::remove_file(&wav_path);
        }

        // Path B: Smart CapCut-style 3-word punchy timed captions from hook/caption text
        let words: Vec<String> = seed
            .split_whitespace()
            .filter(|w| !w.starts_with('#') && !w.starts_with("http"))
            .map(|w| w.trim().to_uppercase())
            .filter(|w| !w.is_empty())
            .take(90)
            .collect();
        let chunks: Vec<String> = if words.is_empty() {
            vec![
                "WAIT FOR IT...".to_owned(),
                "WATCH WHAT HAPPENS".to_owned(),
                "FOLLOW FOR MORE GTA 6".to_owned(),
            ]
        } else {
            words
                .chunks(3)
                .map(|c| c.join(" "))
                .collect()
        };
        let n = chunks.len().max(1) as u64;
        let active_ms = clip_span.min(n * 2400).max(2000);
        let step_ms = (active_ms / n).max(700);
        let mut srt = String::new();
        for (idx, line) in chunks.iter().enumerate() {
            let st = (idx as u64) * step_ms + 250;
            let en = (st + step_ms.saturating_sub(120)).min(clip_span);
            if st >= en {
                break;
            }
            srt.push_str(&format!(
                "{}\n{} --> {}\n{}\n\n",
                idx + 1,
                fmt_srt_time(st),
                fmt_srt_time(en),
                line
            ));
        }
        Ok(json!({
            "srt": srt.trim(),
            "engine": "smart-timed-bursts",
            "cue_count": chunks.len(),
            "whisper_ready": false
        }))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;
    Ok(Json(res))
}

pub(crate) async fn import_file(
    State(app): State<App>,
    mut multipart: axum::extract::Multipart,
) -> ApiResult<Json<Value>> {
    let mut orig_name = String::new();
    let mut bytes_vec: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| bad("Invalid multipart upload."))?
    {
        if field.name() == Some("file") || bytes_vec.is_none() {
            if let Some(fname) = field.file_name() {
                orig_name = fname.to_owned();
            }
            let data = field
                .bytes()
                .await
                .map_err(|_| bad("Could not read uploaded video file (max 250 MiB)."))?;
            if data.is_empty() || data.len() > 250 * 1024 * 1024 {
                return Err(bad("Dropped video must be between 1 byte and 250 MiB."));
            }
            bytes_vec = Some(data.to_vec());
            break;
        }
    }
    let Some(data) = bytes_vec else {
        return Err(bad("Drop an .mp4, .mov, .mkv, or .webm video file."));
    };
    let ext = Path::new(&orig_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "mp4".to_owned());
    if !matches!(ext.as_str(), "mp4" | "mov" | "mkv" | "webm") {
        return Err(bad("Unsupported video format. Drop .mp4, .mov, .mkv, or .webm."));
    }
    let stem_raw = Path::new(&orig_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Dropped Clip");
    let clean_title = stem_raw.trim().chars().take(120).collect::<String>();
    let safe_stem = sanitize_stem(stem_raw);

    let (source_id, root_dir) = {
        let db = app.db.lock().map_err(internal)?;
        let cfg = source::config(&db).map_err(internal)?;
        let root = if cfg.root.trim().is_empty() {
            let fallback = app.media.join("sources");
            let _ = std::fs::create_dir_all(&fallback);
            let _ = db.execute(
                "INSERT INTO settings(key, value) VALUES('source_root', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [fallback.to_string_lossy().as_ref()],
            );
            fallback
        } else {
            PathBuf::from(&cfg.root)
        };
        let existing_id: Option<i64> = db
            .query_row(
                "SELECT id FROM profiles WHERE role='source' AND url='https://local.snapmatic/drops' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(internal)?;
        let sid = if let Some(id) = existing_id {
            let _ = db.execute(
                "UPDATE profiles SET archived=0, source_folder='Snapmatic Drops' WHERE id=?1",
                [id],
            );
            id
        } else {
            db.execute(
                "INSERT INTO profiles(name, platform, url, role, tags, source_folder) VALUES('Snapmatic Drops', 'other', 'https://local.snapmatic/drops', 'source', '[\"Local\"]', 'Snapmatic Drops')",
                [],
            )
            .map_err(internal)?;
            db.last_insert_rowid()
        };
        (sid, root)
    };

    let drops_dir = root_dir.join("Snapmatic Drops");
    std::fs::create_dir_all(&drops_dir).map_err(internal)?;
    let stamp = chrono::Utc::now().timestamp();
    let file_name = format!("{safe_stem}-{stamp}.{ext}");
    let dest = drops_dir.join(&file_name);
    let written_bytes = data.len() as u64;
    std::fs::write(&dest, &data).map_err(internal)?;

    let clip_id = stable_id(source_id, &file_name);
    {
        let db = app.db.lock().map_err(internal)?;
        db.execute(
            "INSERT OR REPLACE INTO workspace_film_roll(clip_id, source_id, path, title, description, bytes, added_at, review_state)
             VALUES(?1, ?2, ?3, ?4, '', ?5, unixepoch(), 'pending_review')",
            params![clip_id, source_id, file_name, clean_title, written_bytes as i64],
        )
        .map_err(internal)?;
    }

    Ok(Json(json!({
        "clip_id": clip_id,
        "title": clean_title,
        "file_name": file_name,
        "bytes": written_bytes
    })))
}
