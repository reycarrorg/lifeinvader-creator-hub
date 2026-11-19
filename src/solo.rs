use super::*;
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

pub const SOLO_URL: &str = "https://lifeinvader.local/solo-posts";
#[derive(Deserialize)]
pub struct Input {
    links: Vec<String>,
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
    match host.as_str() {
        "vm.tiktok.com" | "vt.tiktok.com" => parts.len() == 1 && safe(parts[0]),
        "tiktok.com" | "www.tiktok.com" | "m.tiktok.com" => {
            (parts.len() == 2 && parts[0] == "t" && safe(parts[1]))
                || (parts.len() == 3
                    && parts[0].starts_with('@')
                    && parts[0][1..]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
                    && ["video", "photo"].contains(&parts[1])
                    && parts[2].chars().all(|c| c.is_ascii_digit())
                    && !parts[2].is_empty())
        }
        "instagram.com" | "www.instagram.com" => {
            parts.len() == 2 && ["p", "reel", "reels", "tv"].contains(&parts[0]) && safe(parts[1])
        }
        "youtube.com" | "www.youtube.com" | "m.youtube.com" => {
            (parts.len() == 2 && ["shorts", "live"].contains(&parts[0]) && safe(parts[1]))
                || (parts == ["watch"]
                    && uri.query().is_some_and(|q| {
                        q.split('&').any(|p| p.strip_prefix("v=").is_some_and(safe))
                    }))
        }
        "youtu.be" => parts.len() == 1 && safe(parts[0]),
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
pub async fn start(
    State(app): State<App>,
    Json(input): Json<Input>,
) -> ApiResult<impl IntoResponse> {
    let mut unique = HashSet::new();
    let links: Vec<String> = input
        .links
        .into_iter()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .filter(|s| unique.insert(s.clone()))
        .collect();
    if links.is_empty() || links.len() > 10000 {
        return Err(bad("Enter between 1 and 10,000 post links."));
    }
    if links.iter().any(|s| s.len() > 2048 || !valid_link(s)) {
        return Err(bad(
            "Use full HTTPS links to individual TikTok, Instagram or YouTube posts, one per line.",
        ));
    }
    let mut db = app.db.lock().map_err(internal)?;
    let cfg = source::config(&db).map_err(internal)?;
    let root = source::checked_root(&cfg)?;
    let tool = std::path::PathBuf::from(&cfg.downloader);
    if !tool.is_absolute() || !tool.is_file() {
        return Err(bad(
            "Choose an existing yt-dlp executable in Source folders.",
        ));
    }
    let busy:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM source_sync_runs WHERE state='running') OR EXISTS(SELECT 1 FROM source_date_audits WHERE state='running') OR EXISTS(SELECT 1 FROM source_queue WHERE state='starting')",[],|r|r.get(0)).map_err(internal)?;
    if busy {
        return Err((
            StatusCode::CONFLICT,
            "Another download or audit is running. Wait for it to finish.".into(),
        ));
    }
    let logname = format!("{}.log", Uuid::new_v4());
    fs::create_dir_all(&app.log_dir).map_err(internal)?;
    let mut log = fs::File::create(app.log_dir.join(&logname)).map_err(internal)?;
    writeln!(
        log,
        "Individual posts: {} links. Already archived videos are skipped.",
        links.len()
    )
    .map_err(internal)?;
    let tx = db.transaction().map_err(internal)?;
    tx.execute("INSERT OR IGNORE INTO profiles(name,platform,url,role,tags,source_folder) VALUES('Solo Posts','other',?,'source','[]','')",[SOLO_URL]).map_err(internal)?;
    let id: i64 = tx
        .query_row("SELECT id FROM profiles WHERE url=?", [SOLO_URL], |r| {
            r.get(0)
        })
        .map_err(internal)?;
    tx.execute("INSERT INTO source_preferences(profile_id,include_in_batch) VALUES(?,0) ON CONFLICT(profile_id) DO UPDATE SET include_in_batch=0",[id]).map_err(internal)?;
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
    std::thread::spawn(move || {
        let result = (|| -> std::io::Result<bool> {
            let dest = source::ensure_folder(&root, "Solo Posts/Downloads")?;
            let archive = dest.join("download-archive.txt");
            let output=dest.join("%(uploader,uploader_id,channel,channel_id|Unknown)s/%(title).120B [%(id.-8:)s].%(ext)s");
            let mut ok = true;
            for (index, url) in links.iter().enumerate() {
                writeln!(log, "__LV_BATCH__{}/{}", index, links.len())?;
                writeln!(log, "\nPost {} of {}: {}", index + 1, links.len(), url)?;
                let parsed: axum::http::Uri = url.parse().expect("validated link");
                let host = parsed.host().unwrap_or("");
                let is_tiktok = host.ends_with("tiktok.com");
                let is_ig = host.ends_with("instagram.com");
                let photo = is_tiktok && parsed.path().contains("/photo/");
                if is_ig {
                    let mut cmd = Command::new(&tool);
                    cmd.args([
                        "--ignore-config",
                        "--ignore-errors",
                        "--windows-filenames",
                        "--no-overwrites",
                        "--no-playlist",
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
                    crate::tools::configure_ytdlp(&mut cmd);
                    cmd.arg("--").arg(url);
                    ok = run(&mut cmd, &log)? && ok;
                }
                if !photo {
                    let mut cmd = Command::new(&tool);
                    cmd.args([
                        "--ignore-config",
                        "--ignore-errors",
                        "--windows-filenames",
                        "--no-overwrites",
                        "--no-playlist",
                        "--write-description",
                        "--write-info-json",
                        "--newline",
                        "--download-archive",
                    ])
                    .arg(&archive)
                    .arg("--output")
                    .arg(&output);
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
                if is_tiktok {
                    if Path::new(&cfg.photo_helper).is_file() {
                        let mut cmd = Command::new("python");
                        cmd.arg(&cfg.photo_helper)
                            .arg(url)
                            .arg("--destination")
                            .arg(&dest)
                            .arg("--yt")
                            .arg(&tool);
                        ok = run(&mut cmd, &log)? && ok;
                    } else if photo {
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
                "Finished processing links."
            } else {
                "Finished with errors; check the log."
            }
        );
        if let Ok(db) = db.lock() {
            let _=db.execute("UPDATE source_sync_runs SET state=?,finished=datetime('now'),exit_code=? WHERE id=?",params![if ok {"finished"} else {"error"},if ok {0} else {1},run_id]);
        }
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"id":id,"run_id":run_id,"links":link_count})),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_only_post_urls() {
        assert!(valid_link("https://www.tiktok.com/@x/video/123"));
        assert!(valid_link("https://instagram.com/reel/abc_12/"));
        assert!(valid_link("https://youtu.be/abc-12"));
        assert!(valid_link("https://vm.tiktok.com/abc123/"));
        assert!(!valid_link("https://www.tiktok.com/@x"));
        assert!(!valid_link("https://instagram.com.evil.test/p/123"));
        assert!(!valid_link("https://user@instagram.com/p/123"));
    }
}
