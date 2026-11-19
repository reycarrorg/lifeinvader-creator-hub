use super::*;
use axum::{
    extract::{Query, Request},
    response::Response,
};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    process::{Command, Stdio},
    time::UNIX_EPOCH,
};
use tower::ServiceExt;
use tower_http::services::ServeFile;

#[derive(Serialize)]
pub struct SourceConfig {
    pub(crate) root: String,
    pub(crate) downloader: String,
    pub(crate) photo_helper: String,
}
#[derive(Deserialize)]
pub struct ConfigInput {
    root: String,
    downloader: String,
    photo_helper: String,
}
#[derive(Serialize)]
pub(crate) struct SourceFile {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) size: u64,
    pub(crate) modified: u64,
    created: Option<u64>,
    pub(crate) posted_date: Option<String>,
    pub(crate) posted_date_kind: Option<String>,
    pub(crate) post_id: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) original_url: Option<String>,
    pub(crate) original_account: Option<String>,
    pub(crate) original_channel_url: Option<String>,
    pub(crate) original_platform: Option<String>,
    pub(crate) duration_seconds: Option<f64>,
    asset_id: Option<String>,
    downloaded_at: Option<i64>,
    is_new: bool,
}
#[derive(Serialize)]
struct Run {
    id: i64,
    started: String,
    finished: Option<String>,
    state: String,
    exit_code: Option<i64>,
    log: String,
}
#[derive(Deserialize)]
pub struct FileQuery {
    path: String,
}
#[derive(Deserialize)]
pub struct SyncInput {
    pub(crate) include_photos: bool,
}

fn setting(db: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    db.query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
        r.get(0)
    })
    .optional()
}
pub(crate) fn config(db: &Connection) -> rusqlite::Result<SourceConfig> {
    let root = std::env::current_dir().unwrap_or_default();
    let tool = root.join("tools").join(if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    });
    let photo = root.join("helpers").join("tiktok_photos.py");
    Ok(SourceConfig {
        root: setting(db, "source_root")?.unwrap_or_default(),
        downloader: setting(db, "source_downloader")?.unwrap_or_else(|| {
            if tool.is_file() {
                tool.to_string_lossy().into_owned()
            } else {
                String::new()
            }
        }),
        photo_helper: setting(db, "source_photo_helper")?.unwrap_or_else(|| {
            if photo.is_file() {
                photo.to_string_lossy().into_owned()
            } else {
                String::new()
            }
        }),
    })
}
pub async fn get_config(State(app): State<App>) -> ApiResult<Json<SourceConfig>> {
    let db = app.db.lock().map_err(internal)?;
    Ok(Json(config(&db).map_err(internal)?))
}
pub async fn save_config(
    State(app): State<App>,
    Json(c): Json<ConfigInput>,
) -> ApiResult<StatusCode> {
    let root = Path::new(c.root.trim());
    if !root.is_absolute() || !root.is_dir() {
        return Err(bad("Choose an existing folder for your source library."));
    }
    for (value, label) in [(&c.downloader, "yt-dlp"), (&c.photo_helper, "photo helper")] {
        if !value.trim().is_empty()
            && (!Path::new(value.trim()).is_absolute() || !Path::new(value.trim()).is_file())
        {
            return Err(bad(format!(
                "The {label} path must be an existing file, or left empty."
            )));
        }
    }
    let mut db = app.db.lock().map_err(internal)?;
    let busy: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM source_sync_runs WHERE state='running') OR EXISTS(SELECT 1 FROM source_date_audits WHERE state='running')",
            [],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if busy {
        return Err((
            StatusCode::CONFLICT,
            "Wait for the current sync or date audit to finish before changing folders.".into(),
        ));
    }
    let tx = db.transaction().map_err(internal)?;
    for (key, val) in [
        ("source_root", c.root.trim()),
        ("source_downloader", c.downloader.trim()),
        ("source_photo_helper", c.photo_helper.trim()),
    ] {
        tx.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,val]).map_err(internal)?;
    }
    tx.commit().map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn browse_folder() -> ApiResult<Json<serde_json::Value>> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let result=tokio::task::spawn_blocking(||Command::new("powershell.exe").creation_flags(0x08000000).args(["-NoProfile","-STA","-Command","[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new(); Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = 'Choose your source media folder'; if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::Write($d.SelectedPath) }"]).output()).await.map_err(internal)?.map_err(internal)?;
        if !result.status.success() {
            return Err(internal("Could not open the folder picker"));
        }
        return Ok(Json(
            serde_json::json!({"path":String::from_utf8_lossy(&result.stdout).trim()}),
        ));
    }
    #[cfg(not(windows))]
    {
        Err(bad("Enter the full folder path on this platform."))
    }
}
pub async fn discover(State(app): State<App>) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().map_err(internal)?;
    let root = checked_root(&config(&db).map_err(internal)?)?;
    let file = root.join("TikTok Profile Links.txt");
    if !file.is_file() {
        return Ok(Json(
            serde_json::json!({"sources":[],"message":"No TikTok Profile Links.txt was found in this folder."}),
        ));
    }
    let metadata = fs::metadata(&file).map_err(internal)?;
    if metadata.len() > 1024 * 1024 {
        return Err(bad("The profile list is too large to read."));
    }
    let text = fs::read_to_string(file).map_err(internal)?;
    let mut found = Vec::new();
    for line in text.lines() {
        let Some(link) = line.split_whitespace().next() else {
            continue;
        };
        let Ok(uri) = link.parse::<axum::http::Uri>() else {
            continue;
        };
        if uri.scheme_str() != Some("https")
            || !uri.host().is_some_and(|h| {
                h.eq_ignore_ascii_case("www.tiktok.com") || h.eq_ignore_ascii_case("tiktok.com")
            })
        {
            continue;
        }
        let parts: Vec<&str> = uri.path().split('/').filter(|x| !x.is_empty()).collect();
        if parts.len() != 1 || !parts[0].starts_with('@') {
            continue;
        }
        let name = parts[0].trim_start_matches('@');
        if !safe_folder_name(name)
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        {
            continue;
        }
        let url = format!("https://www.tiktok.com/@{name}");
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM profiles WHERE url=?)",
                [&url],
                |r| r.get(0),
            )
            .map_err(internal)?;
        if !exists
            && !found.iter().any(|v: &serde_json::Value| {
                v["url"]
                    .as_str()
                    .is_some_and(|u| u.eq_ignore_ascii_case(&url))
            })
        {
            found.push(serde_json::json!({"name":name,"url":url,"folder_exists":root.join(name).is_dir(),"note":line.split_once(" - ").map(|(_,n)|n.trim()).unwrap_or("")}));
        }
    }
    Ok(Json(serde_json::json!({"sources":found,"message":""})))
}
pub fn safe_folder_name(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.len() <= 160
        && !s
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        && !s.ends_with(['.', ' '])
}
pub(crate) fn source_profile(db: &Connection, id: i64) -> ApiResult<(String, String, String)> {
    account_profile(db, id, "source")
}
pub(crate) fn account_profile(
    db: &Connection,
    id: i64,
    expected_role: &str,
) -> ApiResult<(String, String, String)> {
    let profile: Option<(String, String, String, String)> = db
        .query_row(
            "SELECT platform,url,role,source_folder FROM profiles WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(internal)?;
    let Some((platform, url, role, custom)) = profile else {
        return Err((StatusCode::NOT_FOUND, "Source not found.".into()));
    };
    if role != expected_role {
        return Err(bad("This account is not in this section."));
    }
    if expected_role == "source" && url == crate::solo::SOLO_URL {
        return Ok(("other".into(), url, "Solo Posts/Downloads".into()));
    }
    let parsed: axum::http::Uri = url.parse().map_err(bad)?;
    let host = parsed.host().unwrap_or("").to_ascii_lowercase();
    let host_ok = match platform.as_str() {
        "tiktok" => ["tiktok.com", "www.tiktok.com", "m.tiktok.com"].contains(&host.as_str()),
        "instagram" => ["instagram.com", "www.instagram.com"].contains(&host.as_str()),
        "youtube" => ["youtube.com", "www.youtube.com", "m.youtube.com"].contains(&host.as_str()),
        _ => false,
    };
    if !host_ok {
        return Err(bad(
            "Sync supports TikTok, Instagram and YouTube account links.",
        ));
    }
    let parts: Vec<&str> = parsed.path().split('/').filter(|x| !x.is_empty()).collect();
    let handle = if platform == "youtube" {
        match parts.as_slice() {
            [name] if name.starts_with('@') => name.trim_start_matches('@'),
            [kind, name] if ["channel", "user", "c"].contains(kind) => *name,
            _ => return Err(bad("Use a YouTube channel or @handle link.")),
        }
    } else {
        if parts.len() != 1 {
            return Err(bad(
                "Use a source profile link rather than an individual post link.",
            ));
        }
        parts[0].trim_start_matches('@')
    };
    if !safe_folder_name(handle)
        || !handle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        return Err(bad("The profile link needs a valid account name."));
    }
    let folder = if custom.is_empty() {
        handle.to_owned()
    } else {
        custom
    };
    if !safe_folder_name(&folder) {
        return Err(bad("The source folder name is invalid."));
    }
    let folder = if expected_role == "destination" {
        format!("_My Profiles/{platform}/profile-{id}")
    } else {
        folder
    };
    Ok((platform, url, folder))
}
pub(crate) fn checked_root(c: &SourceConfig) -> ApiResult<PathBuf> {
    let root = PathBuf::from(&c.root);
    if !root.is_absolute() || !root.is_dir() {
        return Err(bad(
            "Choose an existing source folder first. The app will not switch to a different folder automatically.",
        ));
    }
    root.canonicalize().map_err(internal)
}
fn gallery_dl_dir() -> PathBuf {
    std::env::var_os("LIFEINVADER_GALLERY_DL")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok().and_then(|p| {
                p.parent()
                    .map(|p| p.join("tools").join("gallery-dl-python"))
            })
        })
        .unwrap_or_default()
}
pub(crate) fn allowed_dirs(root: &Path, folder: &str) -> Vec<PathBuf> {
    [root.join(folder), root.join("Photos").join(folder)]
        .into_iter()
        .filter_map(|p| p.canonicalize().ok())
        .filter(|p| p.starts_with(root) && p.is_dir())
        .collect()
}
pub(crate) fn post_metadata(
    path: &Path,
) -> Option<(
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<f64>,
)> {
    let mut json_name = path.as_os_str().to_os_string();
    json_name.push(".json");
    let info = path.with_extension("info.json");
    let sidecar = if info.is_file() {
        info
    } else {
        PathBuf::from(json_name)
    };
    let metadata = fs::symlink_metadata(&sidecar).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&fs::read(sidecar).ok()?).ok()?;
    let date = value
        .get("upload_date")
        .and_then(|v| v.as_str())
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y%m%d").ok())
        .map(|d| d.format("%Y-%m-%d").to_string())
        .or_else(|| {
            value
                .get("date")
                .and_then(|v| v.as_str())
                .and_then(|s| s.get(..10))
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                .map(|d| d.format("%Y-%m-%d").to_string())
        })
        .or_else(|| {
            ["timestamp", "create_time", "createTime"]
                .iter()
                .find_map(|key| {
                    let value = value.get(*key)?;
                    let seconds = value
                        .as_i64()
                        .or_else(|| value.as_str()?.parse::<i64>().ok())?;
                    if !(946_684_800..=chrono::Utc::now().timestamp() + 86_400).contains(&seconds) {
                        return None;
                    }
                    chrono::DateTime::from_timestamp(seconds, 0)
                        .map(|d| d.date_naive().format("%Y-%m-%d").to_string())
                })
        });
    let as_id = |v: &serde_json::Value| -> Option<String> {
        let s = if let Some(s) = v.as_str() {
            s.to_owned()
        } else if let Some(n) = v.as_u64() {
            n.to_string()
        } else {
            return None;
        };
        if s.is_empty() || s.len() > 128 {
            None
        } else {
            Some(s)
        }
    };
    let id = value
        .get("post_shortcode")
        .and_then(as_id)
        .or_else(|| value.get("id").and_then(as_id))
        .or_else(|| value.get("post_id").and_then(as_id));
    let asset_id = value.get("media_id").and_then(as_id).or_else(|| id.clone());
    let text = |key: &str, max: usize| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(max).collect::<String>())
            .filter(|s| !s.trim().is_empty())
    };
    let title = text("title", 500);
    let description = text("description", 10000);
    let url = text("webpage_url", 2000).filter(|s| s.starts_with("https://"));
    let duration = value
        .get("duration")
        .and_then(|v| v.as_f64())
        .filter(|n| n.is_finite() && *n >= 0.0 && *n <= 604800.0);
    Some((date, id, asset_id, title, description, url, duration))
}
pub(crate) fn original_account(path: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let info = path.with_extension("info.json");
    let metadata = match fs::symlink_metadata(&info) {
        Ok(m) if m.is_file() && !m.file_type().is_symlink() && m.len() <= 1024 * 1024 => m,
        _ => return (None, None, None),
    };
    let _ = metadata;
    let value: serde_json::Value = match fs::read(info)
        .ok()
        .and_then(|v| serde_json::from_slice(&v).ok())
    {
        Some(v) => v,
        None => return (None, None, None),
    };
    let text = |keys: &[&str]| {
        keys.iter().find_map(|k| {
            value
                .get(*k)
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.chars().take(200).collect::<String>())
        })
    };
    let account = text(&["uploader_id", "channel", "uploader"]);
    let channel = text(&["channel_url", "uploader_url"]).filter(|s| s.starts_with("https://"));
    let platform = text(&["extractor_key", "extractor"])
        .map(|s| s.to_ascii_lowercase())
        .and_then(|s| {
            if s.contains("tiktok") {
                Some("tiktok".into())
            } else if s.contains("instagram") {
                Some("instagram".into())
            } else if s.contains("youtube") {
                Some("youtube".into())
            } else {
                None
            }
        });
    (account, channel, platform)
}
pub(crate) fn gather(
    root: &Path,
    dir: &Path,
    depth: u8,
    files: &mut Vec<SourceFile>,
) -> std::io::Result<()> {
    if depth > 3 {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue;
        }
        let path = entry.path();
        if ft.is_dir() {
            if entry.file_name() == ".lifeinvader-edits" { continue; }
            gather(root, &path, depth + 1, files)?;
            continue;
        }
        if !ft.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let kind = match ext.as_str() {
            "mp4" | "webm" | "mov" | "mkv" => "video",
            "jpg" | "jpeg" | "png" | "webp" | "gif" | "avif" => "image",
            _ => continue,
        };
        let metadata = entry.metadata()?;
        let relative = path.strip_prefix(root).map_err(std::io::Error::other)?;
        let Some(relative) = relative.to_str() else {
            continue;
        };
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let (posted_date, post_id, asset_id, title, description, original_url, duration_seconds) =
            post_metadata(&path).unwrap_or_default();
        let (original_account, original_channel_url, original_platform) = original_account(&path);
        files.push(SourceFile {
            path: relative.replace('\\', "/"),
            name: name.to_string(),
            kind: kind.into(),
            size: metadata.len(),
            modified: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|x| x.as_secs())
                .unwrap_or(0),
            created: metadata
                .created()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|v| v.as_secs()),
            posted_date_kind: posted_date.as_ref().map(|_| "metadata".to_owned()),
            posted_date,
            post_id,
            title,
            description,
            original_url,
            original_account,
            original_channel_url,
            original_platform,
            duration_seconds,
            asset_id,
            downloaded_at: None,
            is_new: false,
        });
    }
    Ok(())
}
fn last_run(db: &Connection, id: i64, log_dir: &Path) -> rusqlite::Result<Option<Run>> {
    let entry:Option<(i64,String,Option<String>,String,Option<i64>,String)>=db.query_row("SELECT id,started,finished,state,exit_code,log_file FROM source_sync_runs WHERE profile_id=? ORDER BY id DESC LIMIT 1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
    Ok(
        entry.map(|(id, started, finished, state, exit_code, name)| {
            let mut bytes = Vec::new();
            if let Ok(mut f) = fs::File::open(log_dir.join(name)) {
                let _ = f.seek(SeekFrom::End(-32768));
                let _ = f.read_to_end(&mut bytes);
            }
            Run {
                id,
                started,
                finished,
                state,
                exit_code,
                log: String::from_utf8_lossy(&bytes).into_owned(),
            }
        }),
    )
}
pub async fn status(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    status_for(app, id, "source")
}
pub async fn profile_status(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    status_for(app, id, "destination")
}
fn status_for(app: App, id: i64, role: &str) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().map_err(internal)?;
    account_profile(&db, id, role)?;
    let run = last_run(&db, id, &app.log_dir).map_err(internal)?;
    Ok(Json(serde_json::json!({"last_run":run})))
}
pub async fn mark_seen(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<StatusCode> {
    mark_seen_for(app, id, "source")
}
pub async fn profile_seen(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<StatusCode> {
    mark_seen_for(app, id, "destination")
}
fn mark_seen_for(app: App, id: i64, role: &str) -> ApiResult<StatusCode> {
    let db = app.db.lock().map_err(internal)?;
    account_profile(&db, id, role)?;
    let root = checked_root(&config(&db).map_err(internal)?)?;
    db.execute(
        "UPDATE source_new_files SET seen=1 WHERE profile_id=? AND root=?",
        params![id, root.to_string_lossy().as_ref()],
    )
    .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
fn account_folders(app:&App,id:i64,role:&str)->ApiResult<Vec<PathBuf>> {
 let db=app.db.lock().map_err(internal)?;let (_,_,folder)=account_profile(&db,id,role)?;let root=checked_root(&config(&db).map_err(internal)?)?;Ok(allowed_dirs(&root,&folder))
}
fn display_folder(path:&Path)->String {let text=path.to_string_lossy();if let Some(rest)=text.strip_prefix(r"\\?\UNC\"){format!(r"\\{}",rest)}else{text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()}}
fn folder_response(app:&App,id:i64,role:&str)->ApiResult<Json<serde_json::Value>> {let paths=account_folders(app,id,role)?;Ok(Json(serde_json::json!({"paths":paths.iter().map(|p|display_folder(p)).collect::<Vec<_>>() }))) }
pub async fn source_folder_location(State(app):State<App>,AxumPath(id):AxumPath<i64>)->ApiResult<Json<serde_json::Value>>{folder_response(&app,id,"source")}
pub async fn profile_folder_location(State(app):State<App>,AxumPath(id):AxumPath<i64>)->ApiResult<Json<serde_json::Value>>{folder_response(&app,id,"destination")}
fn show_folder(app:&App,id:i64,role:&str)->ApiResult<StatusCode>{let paths=account_folders(app,id,role)?;let path=paths.first().ok_or_else(||(StatusCode::NOT_FOUND,"This account folder does not exist yet. Check Folder settings or sync it first.".into()))?;
 #[cfg(windows)] let mut command=std::process::Command::new("explorer.exe");
 #[cfg(target_os="macos")] let mut command=std::process::Command::new("open");
 #[cfg(all(not(windows),not(target_os="macos")))] let mut command=std::process::Command::new("xdg-open");
 command.arg(display_folder(path)).spawn().map_err(|_|bad("Could not open the folder in your file manager."))?;Ok(StatusCode::NO_CONTENT)}
pub async fn open_source_folder(State(app):State<App>,AxumPath(id):AxumPath<i64>)->ApiResult<StatusCode>{show_folder(&app,id,"source")}
pub async fn open_profile_folder(State(app):State<App>,AxumPath(id):AxumPath<i64>)->ApiResult<StatusCode>{show_folder(&app,id,"destination")}
pub async fn overview(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    overview_for(app, id, "source")
}
pub async fn profile_overview(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    overview_for(app, id, "destination")
}
fn overview_for(app: App, id: i64, role: &str) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().map_err(internal)?;
    let (platform, url, folder) = account_profile(&db, id, role)?;
    let cfg = config(&db).map_err(internal)?;
    let last = last_run(&db, id, &app.log_dir).map_err(internal)?;
    drop(db);
    let root = match checked_root(&cfg) {
        Ok(r) => r,
        Err(_) => {
            return Ok(Json(
                serde_json::json!({"configured":false,"root":cfg.root,"folder":folder,"files":[],"last_run":last,"platform":platform,"url":url,"photos_supported":if role=="destination" && platform=="instagram" {gallery_dl_dir().join("gallery_dl").is_dir()} else {Path::new(&cfg.photo_helper).is_file()}}),
            ));
        }
    };
    let mut files = Vec::new();
    for d in allowed_dirs(&root, &folder) {
        gather(&root, &d, 0, &mut files).map_err(internal)?;
    }
    let db = app.db.lock().map_err(internal)?;
    if role == "source" {
        crate::date_audit::enrich(&db, id, &root, &platform, &mut files).map_err(internal)?;
    }
    let mut stmt = db
        .prepare(
            "SELECT path,downloaded_at,seen FROM source_new_files WHERE profile_id=? AND root=?",
        )
        .map_err(internal)?;
    let entries = stmt
        .query_map(params![id, root.to_string_lossy().as_ref()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, bool>(2)?,
            ))
        })
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    let records: HashMap<String, (i64, bool)> =
        entries.into_iter().map(|(p, t, s)| (p, (t, s))).collect();
    for f in &mut files {
        if let Some((downloaded, seen)) = records.get(&f.path) {
            f.downloaded_at = Some(*downloaded);
            let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
            f.is_new = !*seen && *downloaded > now-3600;
        }
    }
    files.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(Json(
        serde_json::json!({"configured":true,"root":cfg.root,"folder":folder,"files":files,"last_run":last,"platform":platform,"url":url,"photos_supported":if role=="destination" && platform=="instagram" {gallery_dl_dir().join("gallery_dl").is_dir()} else {Path::new(&cfg.photo_helper).is_file()}}),
    ))
}
pub async fn summary(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let Json(value) = overview(State(app), AxumPath(id)).await?;
    let empty = Vec::new();
    let files = value
        .get("files")
        .and_then(|x| x.as_array())
        .unwrap_or(&empty);
    let videos = files.iter().filter(|x| x["kind"] == "video").count();
    let photos = files.iter().filter(|x| x["kind"] == "image").count();
    Ok(Json(
        serde_json::json!({"configured":value["configured"],"videos":videos,"photos":photos,"new_count":files.iter().filter(|x|x["is_new"]==true).count(),"last_run":value["last_run"]}),
    ))
}
pub async fn file(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    Query(q): Query<FileQuery>,
    request: Request,
) -> ApiResult<Response> {
    file_for(app, id, q, request, "source").await
}
pub async fn profile_file(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    Query(q): Query<FileQuery>,
    request: Request,
) -> ApiResult<Response> {
    file_for(app, id, q, request, "destination").await
}
async fn file_for(
    app: App,
    id: i64,
    q: FileQuery,
    request: Request,
    role: &str,
) -> ApiResult<Response> {
    let target = resolve_file(&app, id, &q.path, role)?;
    let response = ServeFile::new(target)
        .oneshot(request)
        .await
        .map_err(internal)?;
    Ok(response.into_response())
}
pub(crate) fn resolve_file(app: &App, id: i64, path: &str, role: &str) -> ApiResult<PathBuf> {
    let (folder, root) = {
        let db = app.db.lock().map_err(internal)?;
        let (_, _, folder) = account_profile(&db, id, role)?;
        let root = checked_root(&config(&db).map_err(internal)?)?;
        (folder, root)
    };
    let relative = Path::new(path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(bad("Invalid file path."));
    }
    let target = root
        .join(relative)
        .canonicalize()
        .map_err(|_| (StatusCode::NOT_FOUND, "File not found.".into()))?;
    if !target.is_file()
        || !allowed_dirs(&root, &folder)
            .iter()
            .any(|d| target.starts_with(d))
    {
        return Err((StatusCode::FORBIDDEN, "File is outside this source.".into()));
    }
    let ext = target
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ![
        "mp4", "webm", "mov", "mkv", "jpg", "jpeg", "png", "webp", "gif", "avif",
    ]
    .contains(&ext.as_str())
    {
        return Err((StatusCode::FORBIDDEN, "Unsupported file.".into()));
    }
    Ok(target)
}
pub async fn thumbnail(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    Query(q): Query<FileQuery>,
) -> ApiResult<impl IntoResponse> {
    thumbnail_for(app, id, q, "source").await
}
pub async fn profile_thumbnail(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    Query(q): Query<FileQuery>,
) -> ApiResult<impl IntoResponse> {
    thumbnail_for(app, id, q, "destination").await
}
async fn thumbnail_for(
    app: App,
    id: i64,
    q: FileQuery,
    role: &str,
) -> ApiResult<impl IntoResponse> {
    use std::hash::{Hash, Hasher};
    let target = resolve_file(&app, id, &q.path, role)?;
    let ext = target
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !["mp4", "webm", "mov", "mkv"].contains(&ext.as_str()) {
        return Err(bad("Not a video."));
    }
    let metadata = fs::metadata(&target).map_err(internal)?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    target.hash(&mut hash);
    metadata.len().hash(&mut hash);
    metadata.modified().ok().hash(&mut hash);
    let cache = app.media.join("thumbnail-cache");
    let output = cache.join(format!("{:016x}.jpg", hash.finish()));
    let ffmpeg = std::env::var_os("LIFEINVADER_FFMPEG")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok().and_then(|p| {
                p.parent().map(|p| {
                    p.join("tools").join(if cfg!(windows) {
                        "ffmpeg.exe"
                    } else {
                        "ffmpeg"
                    })
                })
            })
        })
        .unwrap_or_default();
    let bytes = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
        static THUMB_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = THUMB_LOCK
            .lock()
            .map_err(|_| std::io::Error::other("Preview lock unavailable"))?;
        if let Ok(bytes) = fs::read(&output) {
            if !bytes.is_empty() {
                return Ok(bytes);
            }
        }
        fs::create_dir_all(&cache)?;
        let tmp = cache.join(format!("{}.jpg", Uuid::new_v4()));
        for offset in ["0.5", "0"] {
            let mut command = Command::new(&ffmpeg);
            command
                .args(["-hide_banner", "-loglevel", "error", "-ss", offset, "-i"])
                .arg(&target)
                .args([
                    "-frames:v",
                    "1",
                    "-vf",
                    "scale=480:480:force_original_aspect_ratio=decrease",
                    "-y",
                ])
                .arg(&tmp)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            let status = command.status()?;
            if status.success() {
                if let Ok(bytes) = fs::read(&tmp) {
                    if !bytes.is_empty() {
                        let _ = fs::rename(&tmp, &output);
                        return Ok(bytes);
                    }
                }
            }
            let _ = fs::remove_file(&tmp);
        }
        Err(std::io::Error::other("Unable to decode video"))
    })
    .await
    .map_err(internal)?
    .map_err(|_| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Preview unavailable".into(),
        )
    })?;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "image/jpeg"),
            (axum::http::header::CACHE_CONTROL, "private, max-age=86400"),
        ],
        bytes,
    ))
}
pub async fn sync(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    Json(input): Json<SyncInput>,
) -> ApiResult<impl IntoResponse> {
    sync_for(app, id, input, "source")
}
pub async fn profile_sync(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    Json(input): Json<SyncInput>,
) -> ApiResult<impl IntoResponse> {
    sync_for(app, id, input, "destination")
}
fn sync_for(app: App, id: i64, input: SyncInput, role: &str) -> ApiResult<impl IntoResponse> {
    {
        let db = app.db.lock().map_err(internal)?;
        let queued:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM source_queue WHERE profile_id=? AND state IN ('pending','starting','running'))",[id],|r|r.get(0)).map_err(internal)?;
        if queued {
            return Err((
                StatusCode::CONFLICT,
                "This source is already queued for sync.".into(),
            ));
        }
    }
    let run_id = start_sync_for(app, id, input, role)?;
    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({"id":run_id}))))
}
pub(crate) fn ensure_folder(root: &Path, folder: &str) -> std::io::Result<PathBuf> {
    let mut path = root.to_path_buf();
    for component in Path::new(folder).components() {
        let std::path::Component::Normal(part) = component else {
            return Err(std::io::Error::other("Invalid folder"));
        };
        path.push(part);
        match fs::create_dir(&path) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e),
        }
        if !path.canonicalize()?.starts_with(root) || !path.is_dir() {
            return Err(std::io::Error::other("Folder is outside the library"));
        }
    }
    Ok(path)
}
pub(crate) fn start_sync_for(app: App, id: i64, input: SyncInput, role: &str) -> ApiResult<i64> {
    let mut db = app.db.lock().map_err(internal)?;
    let (platform, url, folder) = account_profile(&db, id, role)?;
    let archived: bool = db.query_row("SELECT archived FROM profiles WHERE id=?", [id], |r| r.get(0)).map_err(internal)?;
    if archived { return Err(bad("Restore this account before syncing it.")); }
    let paused_start: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM source_queue q WHERE q.profile_id=? AND q.state='starting' AND q.origin IN ('auto','batch') AND (NOT COALESCE((SELECT enabled FROM source_schedules WHERE profile_id=q.profile_id),0) OR COALESCE((SELECT dead FROM source_preferences WHERE profile_id=q.profile_id),0)))",[id],|r|r.get(0)).map_err(internal)?;
    if paused_start { return Err(bad("Queued sync cancelled because auto sync is off or the source is marked dead.")); }

    if url == crate::solo::SOLO_URL {
        return Err(bad(
            "Solo posts cannot be synced. Add individual links instead.",
        ));
    }
    let is_profile = role == "destination";
    let cfg = config(&db).map_err(internal)?;
    let selection: Option<(bool, String, String, String)> = if !is_profile {
        db.query_row(
            "SELECT pending,allowed,excluded,mode FROM source_selection WHERE profile_id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(internal)?
    } else {
        None
    };
    let needs_selection: bool = !is_profile
        && db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
                [format!("source_needs_selection_{id}")],
                |r| r.get(0),
            )
            .map_err(internal)?;
    if needs_selection && selection.is_none() {
        return Err(bad("Choose which posts to download before the first sync."));
    }
    let mut selection_pending = false;
    let mut selection_filter = String::from("vcodec != none");
    if let Some((pending, allowed, excluded, mode)) = selection {
        let allowed: Vec<String> = serde_json::from_str(&allowed).map_err(internal)?;
        let excluded: Vec<String> = serde_json::from_str(&excluded).map_err(internal)?;
        if input.include_photos && (!excluded.is_empty() || mode == "before") {
            return Err(bad(
                "Photo sync is unavailable for a source with excluded posts; it could bypass your selection.",
            ));
        }
        selection_pending = pending;
        let limit_to_allowed = (pending && !excluded.is_empty()) || mode == "before";
        let ids = if limit_to_allowed {
            &allowed
        } else {
            &excluded
        };
        if !ids.is_empty() {
            if ids.iter().any(|s| {
                s.is_empty()
                    || !s
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
            }) {
                return Err(bad("The saved selection contains an invalid post ID."));
            }
            let regex = ids.join("|");
            if regex.len() > 24000 {
                return Err(bad(
                    "This selection is too large to sync safely. No posts were downloaded.",
                ));
            }
            selection_filter.push_str(if limit_to_allowed {
                " & id ~= '^(?:"
            } else {
                " & id !~= '^(?:"
            });
            selection_filter.push_str(&regex);
            selection_filter.push_str(")$'");
        }
    }
    let root = checked_root(&cfg)?;
    let tool = PathBuf::from(&cfg.downloader);
    if !tool.is_absolute() || !tool.is_file() {
        return Err(bad(
            "Choose the existing yt-dlp executable in Source settings.",
        ));
    }
    if input.include_photos {
        if platform == "tiktok" && !Path::new(&cfg.photo_helper).is_file() {
            return Err(bad(
                "TikTok photos need the existing photo helper; check Source settings.",
            ));
        }
        if platform == "instagram" && is_profile && !gallery_dl_dir().join("gallery_dl").is_dir() {
            return Err(bad("Instagram photo downloader is missing."));
        }
        if platform != "tiktok" && !(platform == "instagram" && is_profile) {
            return Err(bad("Photo sync is unavailable for this account."));
        }
    }
    let handle = url
        .parse::<axum::http::Uri>()
        .ok()
        .and_then(|u| {
            u.path()
                .split('/')
                .filter(|s| !s.is_empty())
                .next()
                .map(|s| s.trim_start_matches('@').to_owned())
        })
        .unwrap_or_default();
    if input.include_photos
        && platform == "tiktok"
        && !is_profile
        && !folder.eq_ignore_ascii_case(&handle)
    {
        return Err(bad(
            "The photo helper uses the account's own folder name. Clear the custom folder override before including photos.",
        ));
    }
    if root.join(&folder).exists() {
        let canonical = root.join(&folder).canonicalize().map_err(internal)?;
        if !canonical.starts_with(&root) || !canonical.is_dir() {
            return Err(bad("The source folder is outside the selected root."));
        }
    }
    let logname = format!("{}.log", Uuid::new_v4());
    fs::create_dir_all(&app.log_dir).map_err(internal)?;
    let logpath = app.log_dir.join(&logname);
    let mut log = fs::File::create(&logpath).map_err(internal)?;
    writeln!(
        log,
        "{}: {url}\nFolder: {}\nVideo archive: {}\nAlready-archived video IDs are skipped. Previews use local files.\n",
        if is_profile { "Profile" } else { "Source" },
        root.join(&folder).display(),
        if is_profile {root.join(&folder).join("download-archive.txt")} else {root.join("download-archive.txt")}.display()
    )
    .map_err(internal)?;
    let tx = db.transaction().map_err(internal)?;
    let busy: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM source_sync_runs WHERE state='running') OR EXISTS(SELECT 1 FROM source_date_audits WHERE state='running')",
            [],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if busy {
        drop(tx);
        let _ = fs::remove_file(logpath);
        return Err((
            StatusCode::CONFLICT,
            "A sync or date audit is already running. Wait until it finishes.".into(),
        ));
    }
    tx.execute(
        "INSERT INTO source_sync_runs(profile_id,state,log_file) VALUES(?,'running',?)",
        params![id, logname],
    )
    .map_err(internal)?;
    let run_id = tx.last_insert_rowid();
    tx.commit().map_err(internal)?;
    let fallback_links: Vec<String> = if platform == "instagram" {
        match db.prepare("SELECT url FROM analytics_bridge_links WHERE profile_id=? ORDER BY captured_at DESC LIMIT 50") {
            Ok(mut stmt) => stmt.query_map([id], |r| r.get(0)).ok().map(|rows| rows.filter_map(Result::ok).collect()).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let recovery_seed: Option<String> = if is_profile && platform == "tiktok" {
        db.query_row(
            "SELECT value FROM settings WHERE key=?",
            [format!("profile_recovery_seed_{id}")],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?
    } else {
        None
    };
    drop(db);
    let db = app.db.clone();
    let photo_helper = cfg.photo_helper;
    let gallery_dir = gallery_dl_dir();
    std::thread::spawn(move || {
        let mut before = Vec::new();
        let before_ok = allowed_dirs(&root, &folder)
            .iter()
            .all(|d| gather(&root, d, 0, &mut before).is_ok());
        let before: std::collections::HashSet<String> = before
            .into_iter()
            .filter(|f| f.kind == "video")
            .map(|f| f.path)
            .collect();
        let mut used_partial_fallback = false;
        let result = (|| -> std::io::Result<i32> {
            if !before_ok {
                return Err(std::io::Error::other(
                    "Could not read the source folder before syncing",
                ));
            }
            if !root.is_dir() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Source folder is unavailable",
                ));
            }
            let account_dir = ensure_folder(&root, &folder)?;
            let output = account_dir.join(if is_profile {
                "%(title).120B [%(id)s].%(ext)s"
            } else {
                "%(title).120B [%(id.-8:)s].%(ext)s"
            });
            let make_command = |target: &str| {
                let mut command = Command::new(&tool);
                command
                    .args([
                        "--ignore-config",
                        "--ignore-errors",
                        "--windows-filenames",
                        "--no-overwrites",
                        "--write-description",
                        "--write-info-json",
                        "--color",
                        "never",
                        "--newline",
                        "--progress",
                        "--progress-delta",
                        "1",
                        "--progress-template",
                        "download:__LV_PROGRESS__%(progress._percent_str)s",
                        "--match-filters",
                    ])
                    .arg(&selection_filter)
                    .arg("--download-archive")
                    .arg(if is_profile {
                        account_dir.join("download-archive.txt")
                    } else {
                        root.join("download-archive.txt")
                    })
                    .arg("--output")
                    .arg(&output);
                crate::tools::configure_ytdlp(&mut command);
                command.arg("--").arg(target);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x08000000);
                }
                command
            };
            let status = make_command(&url)
                .stdout(Stdio::from(log.try_clone()?))
                .stderr(Stdio::from(log.try_clone()?))
                .status()?;
            let mut code = status.code().unwrap_or(1);
            let mut effective_url = url.clone();
            if code != 0
                && let Some(seed) = recovery_seed
                    .as_ref()
                    .filter(|s| crate::recovery::valid_seed(&url, s))
            {
                writeln!(
                    log,
                    "\nTrying a verified account ID using the saved post link..."
                )?;
                let mut probe = Command::new(&tool);
                probe
                    .args([
                        "--ignore-config",
                        "--skip-download",
                        "--no-warnings",
                        "--no-playlist",
                        "--socket-timeout",
                        "10",
                        "--retries",
                        "0",
                        "--print",
                        "uploader",
                        "--print",
                        "channel_id",
                    ]);
                crate::tools::configure_ytdlp(&mut probe);
                probe.arg("--").arg(seed);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    probe.creation_flags(0x08000000);
                }
                let result = probe.stderr(Stdio::from(log.try_clone()?)).output()?;
                let output = String::from_utf8_lossy(&result.stdout);
                let lines: Vec<_> = output
                    .lines()
                    .map(str::trim)
                    .filter(|x| !x.is_empty())
                    .collect();
                if result.status.success()
                    && lines.len() == 2
                    && lines[0]
                        .trim_start_matches('@')
                        .eq_ignore_ascii_case(&handle)
                    && (10..=200).contains(&lines[1].len())
                    && lines[1]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
                {
                    effective_url = format!("tiktokuser:{}", lines[1]);
                    writeln!(
                        log,
                        "Account verified. Retrying profile sync with the recovered ID."
                    )?;
                    let status = make_command(&effective_url)
                        .stdout(Stdio::from(log.try_clone()?))
                        .stderr(Stdio::from(log.try_clone()?))
                        .status()?;
                    code = status.code().unwrap_or(1);
                } else {
                    writeln!(
                        log,
                        "Could not verify the saved post belongs to this profile; original error remains."
                    )?;
                }
            }
            if code != 0 && !fallback_links.is_empty() {
                used_partial_fallback = true;
                writeln!(
                    log,
                    "\nProfile scan failed. Trying {} previously visible Instagram post links captured by Brave; older posts may be missing.",
                    fallback_links.len()
                )?;
                let mut retry = Command::new(&tool);
                retry
                    .args([
                        "--ignore-config",
                        "--ignore-errors",
                        "--ignore-no-formats-error",
                        "--windows-filenames",
                        "--no-overwrites",
                        "--write-description",
                        "--write-info-json",
                        "--color",
                        "never",
                        "--newline",
                        "--progress",
                        "--progress-delta",
                        "1",
                        "--progress-template",
                        "download:__LV_PROGRESS__%(progress._percent_str)s",
                        "--match-filters",
                    ])
                    .arg(&selection_filter)
                    .arg("--download-archive")
                    .arg(if is_profile {
                        account_dir.join("download-archive.txt")
                    } else {
                        root.join("download-archive.txt")
                    })
                    .arg("--output")
                    .arg(&output);
                crate::tools::configure_ytdlp(&mut retry);
                retry.arg("--").args(&fallback_links);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    retry.creation_flags(0x08000000);
                }
                let status = retry
                    .stdout(Stdio::from(log.try_clone()?))
                    .stderr(Stdio::from(log.try_clone()?))
                    .status()?;
                code = status.code().unwrap_or(1);
            }
            if input.include_photos && platform == "tiktok" {
                writeln!(log, "\nChecking TikTok photo posts...")?;
                let mut cmd = Command::new("python");
                cmd.arg(photo_helper)
                    .arg(&effective_url)
                    .arg("--destination")
                    .arg(if is_profile { &account_dir } else { &root })
                    .arg("--yt")
                    .arg(&tool);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(0x08000000);
                }
                let result = cmd
                    .stdout(Stdio::from(log.try_clone()?))
                    .stderr(Stdio::from(log.try_clone()?))
                    .status()?;
                if !result.success() {
                    code = result.code().unwrap_or(1);
                }
            }
            if input.include_photos && platform == "instagram" && is_profile {
                writeln!(log, "\nChecking Instagram photo posts...")?;
                let mut cmd = Command::new("python");
                cmd.arg("-s")
                    .arg("-m")
                    .arg("gallery_dl")
                    .env("PYTHONPATH", &gallery_dir)
                    .args([
                        "--config-ignore",
                        "--write-metadata",
                        "--filter",
                        "extension.lower() in ('jpg','jpeg','png','webp','avif')",
                        "--download-archive",
                    ])
                    .arg(account_dir.join("instagram-photo-archive.sqlite3"))
                    .arg("--directory")
                    .arg(&account_dir)
                    .arg("--")
                    .arg(&url);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(0x08000000);
                }
                let result = cmd
                    .stdout(Stdio::from(log.try_clone()?))
                    .stderr(Stdio::from(log.try_clone()?))
                    .status()?;
                let mut photo_code = result.code().unwrap_or(1);
                if photo_code != 0 && !fallback_links.is_empty() {
                    let photos: Vec<_> = fallback_links
                        .iter()
                        .filter(|link| link.contains("/p/"))
                        .collect();
                    if !photos.is_empty() {
                        used_partial_fallback = true;
                        writeln!(
                            log,
                            "\nProfile photo scan failed. Trying {} previously visible Instagram posts; coverage is partial.",
                            photos.len()
                        )?;
                        let mut retry = Command::new("python");
                        retry
                            .arg("-s")
                            .arg("-m")
                            .arg("gallery_dl")
                            .env("PYTHONPATH", &gallery_dir)
                            .args([
                                "--config-ignore",
                                "--write-metadata",
                                "--filter",
                                "extension.lower() in ('jpg','jpeg','png','webp','avif')",
                                "--download-archive",
                            ])
                            .arg(account_dir.join("instagram-photo-archive.sqlite3"))
                            .arg("--directory")
                            .arg(&account_dir)
                            .arg("--")
                            .args(photos);
                        #[cfg(windows)]
                        {
                            use std::os::windows::process::CommandExt;
                            retry.creation_flags(0x08000000);
                        }
                        photo_code = retry
                            .stdout(Stdio::from(log.try_clone()?))
                            .stderr(Stdio::from(log.try_clone()?))
                            .status()?
                            .code()
                            .unwrap_or(1);
                    }
                }
                if photo_code != 0 {
                    code = photo_code;
                }
            }
            Ok(code)
        })();
        let code = match result {
            Ok(c) => c,
            Err(e) => {
                let _ = writeln!(log, "\nCould not finish sync: {e}");
                1
            }
        };
        let state = if code != 0 {
            "error"
        } else if used_partial_fallback {
            "partial"
        } else {
            "finished"
        };
        let _ = writeln!(
            log,
            "\nSync {state} (exit code {code}). Platform restrictions or unavailable posts can limit results."
        );
        let mut after = Vec::new();
        let after_ok = before_ok
            && allowed_dirs(&root, &folder)
                .iter()
                .all(|d| gather(&root, d, 0, &mut after).is_ok());
        if let Ok(db) = db.lock() {
            if after_ok {
                for f in after.into_iter().filter(|f| f.size > 0) {
                    if is_profile {
                        if let (Some(post_id), Some(asset_id)) = (&f.post_id, &f.asset_id) {
                            let _=db.execute("INSERT INTO profile_download_files(profile_id,root,platform,post_id,asset_id,path) VALUES(?,?,?,?,?,?) ON CONFLICT(profile_id,root,platform,asset_id) DO UPDATE SET post_id=excluded.post_id,path=excluded.path",params![id,root.to_string_lossy().as_ref(),platform,post_id,asset_id,f.path]);
                        }
                        if let Some(post_id) = &f.post_id
                            && f.kind == "video"
                        {
                            let _=db.execute("INSERT INTO profile_downloads(profile_id,root,platform,post_id,path,posted_date) VALUES(?,?,?,?,?,?) ON CONFLICT(profile_id,root,platform,post_id) DO UPDATE SET path=excluded.path,posted_date=excluded.posted_date",params![id,root.to_string_lossy().as_ref(),platform,post_id,f.path,f.posted_date]);
                        }
                    }
                    if f.kind != "video" {
                        continue;
                    }
                    if before.contains(&f.path) {
                        continue;
                    }
                    let _=db.execute("INSERT INTO source_new_files(profile_id,root,path,downloaded_at,seen) VALUES(?,?,?,unixepoch(),0) ON CONFLICT(profile_id,root,path) DO UPDATE SET downloaded_at=excluded.downloaded_at,seen=0",params![id,root.to_string_lossy().as_ref(),f.path]);
                }
            }
            if code == 0 && selection_pending {
                let _ = db.execute(
                    "UPDATE source_selection SET pending=0 WHERE profile_id=?",
                    [id],
                );
            }
            let _=db.execute("UPDATE source_sync_runs SET state=?,exit_code=?,finished=datetime('now') WHERE id=?",params![state,code,run_id]);
            let _=db.execute("UPDATE source_schedules SET next_run=unixepoch()+interval_seconds WHERE profile_id=? AND enabled=1",[id]);
        }
    });
    Ok(run_id)
}
