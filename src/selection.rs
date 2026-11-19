use super::*;
use std::{
    path::Path,
    process::{Command, Stdio},
};

#[derive(Serialize, Deserialize)]
struct Post {
    id: String,
    title: String,
    thumbnail: String,
    date: String,
}
#[derive(Serialize, Deserialize)]
struct Preview {
    token: String,
    posts: Vec<Post>,
}
#[derive(Deserialize)]
pub struct Choice {
    token: String,
    mode: String,
    cutoff: usize,
}
fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
}
fn json_field(v: &serde_json::Value, name: &str, max: usize) -> String {
    v.get(name)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .chars()
        .take(max)
        .collect()
}

pub async fn preview(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let (platform, url, tool) = {
        let db = app.db.lock().map_err(internal)?;
        let (platform, url, _) = source::source_profile(&db, id)?;
        if url == crate::solo::SOLO_URL {
            return Err(bad("Solo posts have no account feed."));
        }
        let cfg = source::config(&db).map_err(internal)?;
        (platform, url, cfg.downloader)
    };
    if !Path::new(&tool).is_absolute() || !Path::new(&tool).is_file() {
        return Err(bad(
            "Choose an existing yt-dlp executable in Source folders.",
        ));
    }
    let result = tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new(tool);
        cmd.args([
            "--ignore-config",
            "--flat-playlist",
            "--skip-download",
            "--dump-json",
            "--no-warnings",
            "--socket-timeout",
            "10",
            "--retries",
            "0",
            "--playlist-end",
            "1501",
            "--",
        ])
        .arg(url);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        cmd.stderr(Stdio::piped()).output()
    })
    .await
    .map_err(internal)?
    .map_err(internal)?;
    if !result.status.success() {
        let message = String::from_utf8_lossy(&result.stderr);
        return Err(bad(format!(
            "Couldn't load this account's posts. {}",
            message
                .lines()
                .rev()
                .find(|l| l.contains("ERROR:"))
                .unwrap_or("The platform may restrict account access.")
        )));
    }
    let mut posts = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut raw_count = 0usize;
    for line in result.stdout.split(|b| *b == b'\n') {
        if line.is_empty() {
            continue;
        }
        raw_count += 1;
        if raw_count > 1500 {
            return Err(bad(
                "This account has more than 1,500 posts. Selection is not yet safe for lists this large; no downloads were started.",
            ));
        }
        let v: serde_json::Value = serde_json::from_slice(line)
            .map_err(|_| bad("The downloader returned an incomplete post list."))?;
        let pid = json_field(&v, "id", 100);
        if !safe_id(&pid) {
            return Err(bad(
                "The platform returned a post without a usable ID; no selection was saved.",
            ));
        }
        if !seen.insert(pid.clone()) {
            continue;
        }
        let mut thumb = json_field(&v, "thumbnail", 2000);
        if thumb.is_empty() {
            thumb = v
                .get("thumbnails")
                .and_then(|x| x.as_array())
                .and_then(|a| a.last())
                .map(|x| json_field(x, "url", 2000))
                .unwrap_or_default();
        }
        if !thumb.starts_with("https://") {
            thumb.clear();
        }
        let raw_date = json_field(&v, "upload_date", 8);
        let date = chrono::NaiveDate::parse_from_str(&raw_date, "%Y%m%d")
            .ok()
            .map(|d| d.to_string())
            .unwrap_or_default();
        posts.push(Post {
            id: pid,
            title: json_field(&v, "title", 300),
            thumbnail: thumb,
            date,
        });
    }
    if posts.is_empty() {
        return Err(bad(
            "No posts were returned. The account may be empty, private, or restricted.",
        ));
    }
    if posts.len() > 1500 {
        return Err(bad(
            "This account has more than 1,500 posts. Selection is not yet safe for lists this large; no downloads were started.",
        ));
    }
    let preview = Preview {
        token: Uuid::new_v4().to_string(),
        posts,
    };
    let encoded = serde_json::to_string(&preview).map_err(internal)?;
    let db = app.db.lock().map_err(internal)?;
    db.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("source_preview_{id}"),encoded]).map_err(internal)?;
    Ok(Json(
        serde_json::json!({"preview":preview,"platform":platform}),
    ))
}
pub async fn choose(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    Json(input): Json<Choice>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut db = app.db.lock().map_err(internal)?;
    source::source_profile(&db, id)?;
    let value: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key=?",
            [format!("source_preview_{id}")],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?;
    if input.token.is_empty() && input.mode == "all" {
        let needs: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
                [format!("source_needs_selection_{id}")],
                |r| r.get(0),
            )
            .map_err(internal)?;
        if !needs {
            return Err(bad("Preview the source before changing its selection."));
        }
        let tx = db.transaction().map_err(internal)?;
        tx.execute("INSERT OR REPLACE INTO source_selection(profile_id,pending,allowed,excluded,mode) VALUES(?,0,'[]','[]','all')",[id]).map_err(internal)?;
        tx.execute(
            "DELETE FROM settings WHERE key=?",
            [format!("source_needs_selection_{id}")],
        )
        .map_err(internal)?;
        tx.commit().map_err(internal)?;
        return Ok(Json(serde_json::json!({"selected":null,"excluded":0})));
    }
    let preview: Preview =
        serde_json::from_str(&value.ok_or_else(|| bad("Preview expired; load it again."))?)
            .map_err(internal)?;
    if preview.token != input.token
        || input.cutoff >= preview.posts.len()
        || !["all", "before", "after"].contains(&input.mode.as_str())
    {
        return Err(bad("Selection changed; preview the posts again."));
    }
    let busy:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM source_sync_runs WHERE profile_id=? AND state='running') OR EXISTS(SELECT 1 FROM source_queue WHERE profile_id=? AND state IN ('starting','pending','running'))",params![id,id],|r|r.get(0)).map_err(internal)?;
    if busy {
        return Err((
            StatusCode::CONFLICT,
            "Wait for the current sync to finish.".into(),
        ));
    }
    let mut allowed = Vec::new();
    let mut excluded = Vec::new();
    // Posts are shown in the order returned by the platform, newest first when supplied that way.
    for (i, p) in preview.posts.iter().enumerate() {
        if input.mode == "all"
            || (input.mode == "after" && i <= input.cutoff)
            || (input.mode == "before" && i >= input.cutoff)
        {
            allowed.push(p.id.clone());
        } else {
            excluded.push(p.id.clone());
        }
    }
    if allowed.join("|").len() > 24000 || excluded.join("|").len() > 24000 {
        return Err(bad(
            "This selection is too large to sync safely. No selection was saved.",
        ));
    }
    let tx = db.transaction().map_err(internal)?;
    tx.execute("INSERT INTO source_selection(profile_id,pending,allowed,excluded,mode) VALUES(?,1,?,?,?) ON CONFLICT(profile_id) DO UPDATE SET pending=1,allowed=excluded.allowed,excluded=excluded.excluded,mode=excluded.mode",params![id,serde_json::to_string(&allowed).map_err(internal)?,serde_json::to_string(&excluded).map_err(internal)?,input.mode]).map_err(internal)?;
    tx.execute(
        "DELETE FROM settings WHERE key=?",
        [format!("source_needs_selection_{id}")],
    )
    .map_err(internal)?;
    tx.commit().map_err(internal)?;
    Ok(Json(
        serde_json::json!({"selected":allowed.len(),"excluded":excluded.len()}),
    ))
}
