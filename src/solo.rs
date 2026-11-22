use super::*;
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

pub const SOLO_URL: &str = "https://lifeinvader.local/solo-posts";
pub const SOLO_FOLDER: &str = "SOLO POSTS";
pub const GROUP_DM_URL: &str = "https://lifeinvader.local/group-dm/cj_4_prez";
pub const GROUP_DM_FOLDER: &str = "CJ4PREZ GROUP DM";

#[derive(Deserialize)]
pub struct Input {
    links: Vec<String>,
    #[serde(default)]
    mode: Option<String>,
}

fn valid_link(link: &str) -> bool {
    let Ok(uri) = link.parse::<axum::http::Uri>() else {
        return false;
    };
    if uri.scheme_str() != Some("https")
        || uri
            .authority()
            .is_none_or(|a| a.as_str().contains('@') || a.port().is_some())
    {
        return false;
    }
    let host = uri.host().unwrap_or("").to_ascii_lowercase();
    let parts: Vec<_> = uri.path().split('/').filter(|x| !x.is_empty()).collect();
    let safe = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
    };
    let safe_user = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    };
    match host.as_str() {
        "vm.tiktok.com" | "vt.tiktok.com" => parts.len() == 1 && safe(parts[0]),
        "tiktok.com" | "www.tiktok.com" | "m.tiktok.com" => {
            (parts.len() == 2 && parts[0] == "t" && safe(parts[1]))
                || (parts.len() == 3
                    && parts[0].starts_with('@')
                    && safe_user(&parts[0][1..])
                    && ["video", "photo"].contains(&parts[1])
                    && parts[2].chars().all(|c| c.is_ascii_digit())
                    && !parts[2].is_empty())
        }
        "instagram.com" | "www.instagram.com" | "m.instagram.com" => {
            (parts.len() == 2
                && ["p", "reel", "reels", "tv"].contains(&parts[0])
                && safe(parts[1]))
                || (parts.len() == 3
                    && safe_user(parts[0])
                    && ["p", "reel", "reels", "tv"].contains(&parts[1])
                    && safe(parts[2]))
        }
        "youtube.com" | "www.youtube.com" | "m.youtube.com" => {
            (parts.len() == 2 && ["shorts", "live"].contains(&parts[0]) && safe(parts[1]))
                || (parts == ["watch"]
                    && uri.query().is_some_and(|q| {
                        q.split('&').any(|p| p.strip_prefix("v=").is_some_and(safe))
                    }))
        }
        "youtu.be" => parts.len() == 1 && safe(parts[0]),
        "x.com" | "www.x.com" | "twitter.com" | "www.twitter.com" | "mobile.twitter.com" => {
            parts.len() >= 3
                && safe_user(parts[0])
                && parts[1] == "status"
                && parts[2].chars().all(|c| c.is_ascii_digit())
                && !parts[2].is_empty()
        }
        _ => false,
    }
}

fn hidden(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn run(command: &mut Command, log: &fs::File) -> std::io::Result<bool> {
    hidden(command);
    Ok(command
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log.try_clone()?))
        .status()?
        .success())
}

fn resolve_ytdlp(configured: &str) -> Option<PathBuf> {
    let cfg_path = PathBuf::from(configured);
    if cfg_path.is_absolute() && cfg_path.is_file() {
        return Some(cfg_path);
    }
    let exe_name = if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" };
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("tools").join(exe_name)));
    let local = std::env::current_dir()
        .ok()
        .map(|d| d.join("tools").join(exe_name));
    [bundled, local]
        .into_iter()
        .flatten()
        .find(|p| p.is_file())
        .and_then(|p| p.canonicalize().ok())
}

fn resolve_photo_helper(configured: &str) -> Option<PathBuf> {
    let cfg_path = PathBuf::from(configured);
    if cfg_path.is_file() {
        return Some(cfg_path);
    }
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("helpers").join("tiktok_photos.py")));
    let local = std::env::current_dir()
        .ok()
        .map(|d| d.join("helpers").join("tiktok_photos.py"));
    [bundled, local]
        .into_iter()
        .flatten()
        .find(|p| p.is_file())
}

fn collect_video_files(root: &std::path::Path, dir: &std::path::Path, out: &mut HashSet<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_video_files(root, &path, out);
        } else if path.is_file()
            && path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                matches!(e.to_ascii_lowercase().as_str(), "mp4" | "mov" | "webm" | "mkv")
            })
            && let Ok(rel) = path.strip_prefix(root)
        {
            out.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

pub async fn start(
    State(app): State<App>,
    Json(input): Json<Input>,
) -> ApiResult<impl IntoResponse> {
    start_collection(
        app,
        input,
        SOLO_URL,
        "SOLO POSTS",
        "other",
        SOLO_FOLDER,
        "[]",
    )
    .await
}

pub async fn start_group_dm(
    State(app): State<App>,
    Json(input): Json<Input>,
) -> ApiResult<impl IntoResponse> {
    start_collection(
        app,
        input,
        GROUP_DM_URL,
        "cj_4_prez's group",
        "tiktok",
        GROUP_DM_FOLDER,
        r#"["Group DM"]"#,
    )
    .await
}

async fn start_collection(
    app: App,
    input: Input,
    profile_url: &'static str,
    profile_name: &'static str,
    profile_platform: &'static str,
    folder_name: &'static str,
    profile_tags: &'static str,
) -> ApiResult<impl IntoResponse> {
    let mode = match input
        .mode
        .as_deref()
        .map(str::trim)
        .unwrap_or("both")
        .to_ascii_lowercase()
        .as_str()
    {
        "videos" => "videos".to_owned(),
        "photos" => "photos".to_owned(),
        _ => "both".to_owned(),
    };
    let want_videos = mode != "photos";
    let want_photos = mode != "videos";

    let mut unique = HashSet::new();
    let links: Vec<String> = input
        .links
        .into_iter()
        .flat_map(|line| {
            line.split_whitespace()
                .map(|s| s.trim_matches(|c: char| ",;\"'<>()[]".contains(c)).to_owned())
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .filter(|s| unique.insert(s.clone()))
        .collect();

    if links.is_empty() || links.len() > 10000 {
        return Err(bad("Paste at least 1 valid post link (up to 10,000 links)."));
    }
    if let Some(bad_url) = links.iter().find(|s| s.len() > 2048 || !valid_link(s)) {
        return Err(bad(format!(
            "Unsupported post link: {bad_url}. Use full HTTPS links to TikTok, Instagram, YouTube, or X posts."
        )));
    }

    let mut db = app.db.lock().map_err(internal)?;
    let cfg = source::config(&db).map_err(internal)?;
    let root = source::checked_root(&cfg)?;
    let Some(tool) = resolve_ytdlp(&cfg.downloader) else {
        return Err(bad(
            "yt-dlp executable not found. Check Source folders or tools/yt-dlp.exe.",
        ));
    };
    let photo_helper = resolve_photo_helper(&cfg.photo_helper);

    let busy: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM source_sync_runs WHERE state='running') OR EXISTS(SELECT 1 FROM source_date_audits WHERE state='running') OR EXISTS(SELECT 1 FROM source_queue WHERE state='starting')",
            [],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if busy {
        return Err((
            StatusCode::CONFLICT,
            "Another download or audit is running. Wait for it to finish.".into(),
        ));
    }

    let dest_preview = root.join(folder_name);
    let logname = format!("{}.log", Uuid::new_v4());
    fs::create_dir_all(&app.log_dir).map_err(internal)?;
    let mut log = fs::File::create(app.log_dir.join(&logname)).map_err(internal)?;
    writeln!(
        log,
        "{}: {} link(s) · Mode: {} · Destination: {}",
        profile_name,
        links.len(),
        mode,
        dest_preview.display()
    )
    .map_err(internal)?;

    let tx = db.transaction().map_err(internal)?;
    tx.execute(
        "INSERT INTO profiles(name,platform,url,role,tags,source_folder,archived) VALUES(?,?,?,'source',?,?,0) \
         ON CONFLICT(url) DO UPDATE SET name=excluded.name, source_folder=excluded.source_folder, archived=0",
        params![profile_name, profile_platform, profile_url, profile_tags, folder_name],
    )
    .map_err(internal)?;
    let id: i64 = tx
        .query_row("SELECT id FROM profiles WHERE url=?", [profile_url], |r| {
            r.get(0)
        })
        .map_err(internal)?;
    tx.execute(
        "INSERT INTO source_preferences(profile_id,include_in_batch) VALUES(?,0) ON CONFLICT(profile_id) DO UPDATE SET include_in_batch=0",
        [id],
    )
    .map_err(internal)?;
    tx.execute(
        "INSERT INTO source_sync_runs(profile_id,state,log_file) VALUES(?,'running',?)",
        params![id, logname],
    )
    .map_err(internal)?;
    let run_id = tx.last_insert_rowid();
    tx.commit().map_err(internal)?;
    drop(db);

    let db = app.db.clone();
    let link_count = links.len();
    let dest_str = dest_preview.to_string_lossy().into_owned();
    let mode_resp = mode.clone();

    std::thread::spawn(move || {
        let mut before_videos = HashSet::new();
        let dest_for_scan = root.join(folder_name);
        collect_video_files(&root, &dest_for_scan, &mut before_videos);

        let result = (|| -> std::io::Result<bool> {
            let dest = source::ensure_folder(&root, folder_name)?;
            let archive = dest.join("download-archive.txt");
            let output = dest.join(
                "%(uploader,uploader_id,channel,channel_id|Unknown)s/%(title).120B [%(id.-8:)s].%(ext)s",
            );
            let mut ok = true;
            for (index, url) in links.iter().enumerate() {
                writeln!(log, "__LV_BATCH__{}/{}", index, links.len())?;
                writeln!(log, "\nPost {} of {}: {}", index + 1, links.len(), url)?;
                let parsed: axum::http::Uri = url.parse().expect("validated link");
                let host = parsed.host().unwrap_or("").to_ascii_lowercase();
                let is_tiktok = host.ends_with("tiktok.com");
                let is_ig = host.ends_with("instagram.com");
                let is_ig_carousel_post = is_ig && parsed.path().contains("/p/");
                let is_tiktok_photo = is_tiktok && parsed.path().contains("/photo/");

                // 1. Photos pass (Instagram photo/carousel thumbnails & TikTok photo slideshows)
                if want_photos && is_ig {
                    let mut cmd = Command::new(&tool);
                    cmd.args([
                        "--ignore-config",
                        "--ignore-errors",
                        "--windows-filenames",
                        "--no-overwrites",
                        "--skip-download",
                        "--ignore-no-formats-error",
                        "--match-filters",
                        "!vcodec",
                        "--write-description",
                        "--write-info-json",
                        "--write-thumbnail",
                        "--output",
                        "pl_thumbnail:",
                        "--output",
                    ])
                    .arg(&output);
                    if !is_ig_carousel_post {
                        cmd.arg("--no-playlist");
                    }
                    crate::tools::configure_ytdlp(&mut cmd);
                    cmd.arg("--").arg(url);
                    ok = run(&mut cmd, &log)? && ok;
                }

                // 2. Videos pass (TikTok, Instagram Reels/Posts, YouTube, X)
                if want_videos && !is_tiktok_photo {
                    let mut cmd = Command::new(&tool);
                    cmd.args([
                        "--ignore-config",
                        "--ignore-errors",
                        "--windows-filenames",
                        "--no-overwrites",
                        "--write-description",
                        "--write-info-json",
                        "--newline",
                        "--download-archive",
                    ])
                    .arg(&archive)
                    .arg("--output")
                    .arg(&output);
                    if !is_ig_carousel_post {
                        cmd.arg("--no-playlist");
                    }
                    if is_ig {
                        cmd.arg("--ignore-no-formats-error");
                    }
                    if is_ig || is_tiktok {
                        cmd.args(["--match-filters", "vcodec != none"]);
                    }
                    crate::tools::configure_ytdlp(&mut cmd);
                    cmd.arg("--").arg(url);
                    ok = run(&mut cmd, &log)? && ok;
                }

                // 3. TikTok slideshow photos pass
                if want_photos && is_tiktok {
                    if let Some(ref helper) = photo_helper {
                        let mut cmd = Command::new(crate::tools::python_command());
                        cmd.arg(helper)
                            .arg(url)
                            .arg("--destination")
                            .arg(&dest)
                            .arg("--yt")
                            .arg(&tool);
                        let ph_ok = run(&mut cmd, &log)?;
                        if is_tiktok_photo {
                            ok = ph_ok && ok;
                        }
                    } else if is_tiktok_photo {
                        writeln!(log, "TikTok photo helper is not configured.")?;
                        ok = false;
                    }
                }
            }
            Ok(ok)
        })();
        let ok = matches!(result, Ok(true));
        if let Err(e) = result {
            let _ = writeln!(log, "Download error: {e}");
        }
        let _ = writeln!(
            log,
            "\n{}",
            if ok {
                "Finished downloading all solo post links."
            } else {
                "Finished with errors; check the log."
            }
        );
        let mut after_videos = HashSet::new();
        collect_video_files(&root, &dest_for_scan, &mut after_videos);
        if let Ok(db) = db.lock() {
            let root_str = root.to_string_lossy().into_owned();
            for rel in after_videos {
                if before_videos.contains(&rel) {
                    continue;
                }
                let _ = db.execute(
                    "INSERT INTO source_new_files(profile_id,root,path,downloaded_at,seen) VALUES(?,?,?,unixepoch(),0) \
                     ON CONFLICT(profile_id,root,path) DO UPDATE SET downloaded_at=excluded.downloaded_at,seen=0",
                    params![id, &root_str, &rel],
                );
                #[cfg(feature = "snapmatic-hud")]
                crate::workspace_film_roll::auto_add_source_video(&db, id, &rel, &root.join(&rel));
            }
            let _ = db.execute(
                "UPDATE source_sync_runs SET state=?,finished=datetime('now'),exit_code=? WHERE id=?",
                params![if ok { "finished" } else { "error" }, if ok { 0 } else { 1 }, run_id],
            );
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "id": id,
            "run_id": run_id,
            "links": link_count,
            "mode": mode_resp,
            "destination": dest_str
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_only_post_urls() {
        assert!(valid_link("https://www.tiktok.com/@x/video/123"));
        assert!(valid_link("https://www.tiktok.com/@x/photo/123?is_from_webapp=1"));
        assert!(valid_link("https://instagram.com/reel/abc_12/"));
        assert!(valid_link("https://www.instagram.com/rockstargames/p/C123abc/?igsh=xyz"));
        assert!(valid_link("https://www.instagram.com/rockstargames/reel/C123abc/"));
        assert!(valid_link("https://youtu.be/abc-12"));
        assert!(valid_link("https://vm.tiktok.com/abc123/"));
        assert!(valid_link("https://x.com/RockstarGames/status/1731813331244085531"));
        assert!(!valid_link("https://www.tiktok.com/@x"));
        assert!(!valid_link("https://instagram.com.evil.test/p/123"));
        assert!(!valid_link("https://user@instagram.com/p/123"));
    }
}
