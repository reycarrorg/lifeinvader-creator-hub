use crate::{ApiResult, App, bad, internal, source};
use axum::{Json, extract::State, http::StatusCode};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

type Index = HashMap<String, Option<String>>;
fn valid_id(s: &str) -> bool {
    (17..=20).contains(&s.len()) && s.bytes().all(|c| c.is_ascii_digit())
}
fn archive_index(root: &Path) -> Index {
    let path = root.join("download-archive.txt");
    let Ok(meta) = fs::metadata(&path) else {
        return Index::new();
    };
    if meta.len() > 64 * 1024 * 1024 {
        return Index::new();
    }
    let Ok(file) = File::open(path) else {
        return Index::new();
    };
    let mut index = Index::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let mut parts = line.split_whitespace();
        let (Some(platform), Some(id)) = (parts.next(), parts.next()) else {
            continue;
        };
        if !platform.eq_ignore_ascii_case("tiktok") || !valid_id(id) {
            continue;
        }
        let key = id[id.len() - 8..].to_owned();
        index
            .entry(key)
            .and_modify(|prev| {
                if prev.as_deref() != Some(id) {
                    *prev = None
                }
            })
            .or_insert_with(|| Some(id.to_owned()));
    }
    index
}
fn filename_id(name: &str, index: &Index) -> Option<String> {
    let stem = Path::new(name).file_stem()?.to_str()?;
    let prefix = stem.strip_suffix(']')?;
    let val = prefix.rsplit_once('[')?.1;
    if valid_id(val) {
        return Some(val.to_owned());
    }
    if val.len() == 8 && val.bytes().all(|b| b.is_ascii_digit()) {
        return index.get(val).and_then(Clone::clone);
    }
    None
}
fn id_date(id: &str) -> Option<String> {
    if !valid_id(id) {
        return None;
    }
    let seconds = (id.parse::<u64>().ok()? >> 32) as i64;
    if seconds < 1_472_688_000 || seconds > Utc::now().timestamp() + 86400 {
        return None;
    }
    Some(
        DateTime::from_timestamp(seconds, 0)?
            .date_naive()
            .format("%Y-%m-%d")
            .to_string(),
    )
}
#[derive(Default, Clone)]
struct Saved {
    post_id: String,
    date: String,
    kind: String,
    title: String,
    description: String,
    url: String,
    size: i64,
    modified: i64,
}
fn saved(
    db: &rusqlite::Connection,
    id: i64,
    root: &Path,
) -> rusqlite::Result<HashMap<String, Saved>> {
    let mut q=db.prepare("SELECT path,post_id,posted_date,date_kind,title,description,original_url,file_size,file_modified FROM source_date_results WHERE profile_id=? AND root=?")?;
    let rows = q
        .query_map(params![id, root.to_string_lossy().as_ref()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Saved {
                    post_id: r.get(1)?,
                    date: r.get(2)?,
                    kind: r.get(3)?,
                    title: r.get(4)?,
                    description: r.get(5)?,
                    url: r.get(6)?,
                    size: r.get(7)?,
                    modified: r.get(8)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}
fn apply(f: &mut source::SourceFile, platform: &str, index: &Index, stored: Option<&Saved>) {
    if let Some(s) = stored.filter(|s| s.size == f.size as i64 && s.modified == f.modified as i64) {
        if f.post_id.is_none() && !s.post_id.is_empty() {
            f.post_id = Some(s.post_id.clone())
        }
        if f.posted_date.is_none() && !s.date.is_empty() {
            f.posted_date = Some(s.date.clone());
            f.posted_date_kind = Some(s.kind.clone());
        }
        if f.title.is_none() && !s.title.is_empty() {
            f.title = Some(s.title.clone())
        }
        if f.description.is_none() && !s.description.is_empty() {
            f.description = Some(s.description.clone())
        }
        if f.original_url.is_none() && !s.url.is_empty() {
            f.original_url = Some(s.url.clone())
        }
    }
    if platform == "tiktok" {
        let id = f
            .post_id
            .as_deref()
            .filter(|v| valid_id(v))
            .map(str::to_owned)
            .or_else(|| filename_id(&f.name, index));
        if f.post_id.is_none() {
            f.post_id = id.clone()
        }
        if f.posted_date.is_none() {
            if let Some(date) = id.as_deref().and_then(id_date) {
                f.posted_date = Some(date);
                f.posted_date_kind = Some("estimated_id".into());
            }
        }
    }
}
pub(crate) fn enrich(
    db: &rusqlite::Connection,
    id: i64,
    root: &Path,
    platform: &str,
    files: &mut [source::SourceFile],
) -> rusqlite::Result<()> {
    let existing = saved(db, id, root)?;
    let index = if platform == "tiktok" {
        archive_index(root)
    } else {
        Index::new()
    };
    for f in files {
        apply(f, platform, &index, existing.get(&f.path));
    }
    Ok(())
}
pub(crate) fn resolve(
    db: &rusqlite::Connection,
    id: i64,
    root: &Path,
    platform: &str,
    path: &str,
    name: &str,
    post_id: &mut Option<String>,
    date: &mut Option<String>,
    kind: &mut Option<String>,
) -> rusqlite::Result<()> {
    let entry = saved(db, id, root)?.remove(path);
    let metadata = std::fs::metadata(root.join(path)).ok();
    let size = metadata.as_ref().map(|m| m.len() as i64).unwrap_or(-1);
    let modified = metadata
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(-1);
    if let Some(e) = entry.filter(|e| e.size == size && e.modified == modified) {
        if post_id.is_none() && !e.post_id.is_empty() {
            *post_id = Some(e.post_id)
        }
        if date.is_none() && !e.date.is_empty() {
            *date = Some(e.date);
            *kind = Some(e.kind)
        }
    }
    if platform == "tiktok" {
        let id = post_id
            .as_deref()
            .filter(|v| valid_id(v))
            .map(str::to_owned)
            .or_else(|| filename_id(name, &archive_index(root)));
        if post_id.is_none() {
            *post_id = id.clone()
        }
        if date.is_none() {
            if let Some(d) = id.as_deref().and_then(id_date) {
                *date = Some(d);
                *kind = Some("estimated_id".into());
            }
        }
    }
    Ok(())
}
#[derive(Deserialize)]
pub struct Input {
    profile_ids: Vec<i64>,
    #[serde(default)]
    online: bool,
}
pub async fn status(State(app): State<App>) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().map_err(internal)?;
    let result=db.query_row("SELECT id,state,online,processed,total,verified,estimated,unknown,failed,cancel_requested,message,started,finished FROM source_date_audits ORDER BY id DESC LIMIT 1",[],|r|Ok(serde_json::json!({"id":r.get::<_,i64>(0)?,"state":r.get::<_,String>(1)?,"online":r.get::<_,bool>(2)?,"processed":r.get::<_,i64>(3)?,"total":r.get::<_,i64>(4)?,"verified":r.get::<_,i64>(5)?,"estimated":r.get::<_,i64>(6)?,"unknown":r.get::<_,i64>(7)?,"failed":r.get::<_,i64>(8)?,"cancel_requested":r.get::<_,bool>(9)?,"message":r.get::<_,String>(10)?,"started":r.get::<_,String>(11)?,"finished":r.get::<_,Option<String>>(12)?}))).optional().map_err(internal)?;
    Ok(Json(serde_json::json!({"run":result})))
}
pub async fn cancel(State(app): State<App>) -> ApiResult<StatusCode> {
    let db = app.db.lock().map_err(internal)?;
    let n=db.execute("UPDATE source_date_audits SET cancel_requested=1,message='Stopping after the current check…' WHERE state='running'",[]).map_err(internal)?;
    if n == 0 {
        return Err(bad("No date audit is running."));
    }
    Ok(StatusCode::NO_CONTENT)
}
pub async fn start(
    State(app): State<App>,
    Json(input): Json<Input>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    if input.profile_ids.is_empty() || input.profile_ids.len() > 100 {
        return Err(bad("Choose one to 100 sources."));
    }
    let mut ids = input.profile_ids;
    ids.sort();
    ids.dedup();
    let mut db = app.db.lock().map_err(internal)?;
    let cfg = source::config(&db).map_err(internal)?;
    let root = source::checked_root(&cfg)?;
    let tool = PathBuf::from(&cfg.downloader);
    if input.online && (!tool.is_absolute() || !tool.is_file()) {
        return Err(bad(
            "Choose yt-dlp in Source folders before checking online.",
        ));
    }
    let mut selected = Vec::new();
    for id in ids {
        let (platform, url, folder) = source::source_profile(&db, id)?;
        selected.push((id, platform, url, folder));
    }
    let tx = db.transaction().map_err(internal)?;
    let busy:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM source_sync_runs WHERE state='running') OR EXISTS(SELECT 1 FROM source_date_audits WHERE state='running') OR EXISTS(SELECT 1 FROM source_queue WHERE state IN ('pending','starting','running'))",[],|r|r.get(0)).map_err(internal)?;
    if busy {
        return Err((
            StatusCode::CONFLICT,
            "Wait for sync or the current date audit to finish.".into(),
        ));
    }
    tx.execute("INSERT INTO source_date_audits(state,online,message) VALUES('running',?,'Scanning local media…')",[input.online]).map_err(internal)?;
    let run = tx.last_insert_rowid();
    tx.commit().map_err(internal)?;
    drop(db);
    thread::spawn(move || {
        if let Err(e) = run_audit(&app, run, &root, &tool, &selected, input.online) {
            if let Ok(db) = app.db.lock() {
                let _=db.execute("UPDATE source_date_audits SET state='error',finished=datetime('now'),message=? WHERE id=?",params![format!("Audit could not finish: {e}"),run]);
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({"id":run}))))
}
fn cancelled(app: &App, run: i64) -> bool {
    app.db
        .lock()
        .ok()
        .and_then(|db| {
            db.query_row(
                "SELECT cancel_requested FROM source_date_audits WHERE id=?",
                [run],
                |r| r.get(0),
            )
            .ok()
        })
        .unwrap_or(true)
}
fn post_url(platform: &str, account: &str, id: &str, original: Option<&str>) -> Option<String> {
    if let Some(url) = original {
        if let Ok(uri) = url.parse::<axum::http::Uri>() {
            let h = uri.host().unwrap_or("").to_ascii_lowercase();
            let valid = uri.scheme_str() == Some("https")
                && match platform {
                    "tiktok" => {
                        ["tiktok.com", "www.tiktok.com", "m.tiktok.com"].contains(&h.as_str())
                    }
                    "instagram" => ["instagram.com", "www.instagram.com"].contains(&h.as_str()),
                    "youtube" => {
                        ["youtube.com", "www.youtube.com", "m.youtube.com"].contains(&h.as_str())
                    }
                    _ => false,
                };
            if valid
                && (uri.path().split('/').any(|p| p == id)
                    || uri
                        .query()
                        .is_some_and(|q| q.split('&').any(|p| p == format!("v={id}"))))
            {
                return Some(url.to_owned());
            }
        }
    }
    match platform {
        "tiktok" if valid_id(id) => Some(format!("{}/video/{id}", account.trim_end_matches('/'))),
        "instagram"
            if (5..=40).contains(&id.len())
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') =>
        {
            Some(format!("https://www.instagram.com/p/{id}/"))
        }
        "youtube"
            if (6..=30).contains(&id.len())
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') =>
        {
            Some(format!("https://www.youtube.com/watch?v={id}"))
        }
        _ => None,
    }
}
fn lookup(app: &App, run: i64, tool: &Path, url: &str, id: &str) -> Option<String> {
    fs::create_dir_all(&app.log_dir).ok()?;
    let tmp = app
        .log_dir
        .join(format!("date-audit-{}.tmp", Uuid::new_v4()));
    let out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .ok()?;
    let mut cmd = Command::new(tool);
    cmd.args([
        "--ignore-config",
        "--skip-download",
        "--no-playlist",
        "--no-warnings",
        "--socket-timeout",
        "8",
        "--retries",
        "0",
        "--extractor-retries",
        "0",
        "--print",
        "%(id)s\t%(upload_date)s\t%(timestamp)s",
        "--",
        url,
    ])
    .stdout(Stdio::from(out))
    .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let result = (|| {
        let mut child = cmd.spawn().ok()?;
        let start = Instant::now();
        let exit = loop {
            if let Ok(Some(status)) = child.try_wait() {
                break Some(status);
            }
            if start.elapsed() > Duration::from_secs(20) || cancelled(app, run) {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            thread::sleep(Duration::from_millis(150));
        };
        if !exit?.success() {
            return None;
        }
        let mut text = String::new();
        File::open(&tmp)
            .ok()?
            .take(16385)
            .read_to_string(&mut text)
            .ok()?;
        if text.len() > 16384 {
            return None;
        }
        let row = text.lines().next()?;
        let mut fields = row.split('\t');
        if fields.next()?.trim() != id {
            return None;
        }
        let raw = fields.next()?.trim();
        NaiveDate::parse_from_str(raw, "%Y%m%d")
            .ok()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .or_else(|| {
                fields
                    .next()?
                    .trim()
                    .parse::<i64>()
                    .ok()
                    .and_then(|n| DateTime::from_timestamp(n, 0))
                    .map(|d| d.date_naive().format("%Y-%m-%d").to_string())
            })
    })();
    let _ = fs::remove_file(tmp);
    result
}
fn run_audit(
    app: &App,
    run: i64,
    root: &Path,
    tool: &Path,
    sources: &[(i64, String, String, String)],
    online: bool,
) -> Result<(), String> {
    let mut jobs = Vec::new();
    let mut unavailable = 0;
    'sources: for (id, platform, url, folder) in sources {
        if cancelled(app, run) {
            break;
        }
        let dirs = source::allowed_dirs(root, folder);
        if dirs.is_empty() {
            unavailable += 1;
            continue;
        }
        let mut files = Vec::new();
        for d in dirs {
            if cancelled(app, run) {
                break 'sources;
            }
            if source::gather(root, &d, 0, &mut files).is_err() {
                unavailable += 1;
                continue 'sources;
            }
        }
        let db = app.db.lock().map_err(|_| "Database is busy".to_string())?;
        enrich(&db, *id, root, platform, &mut files).map_err(|e| e.to_string())?;
        drop(db);
        for f in files {
            jobs.push((*id, platform.clone(), url.clone(), f));
        }
    }
    {
        let db = app.db.lock().map_err(|_| "Database is busy".to_string())?;
        db.execute(
            "UPDATE source_date_audits SET total=?,message=? WHERE id=?",
            params![
                jobs.len() as i64,
                format!(
                    "Checking {} files; {} source folders unavailable.",
                    jobs.len(),
                    unavailable
                ),
                run
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    let mut cache: HashMap<(String, String), Option<String>> = HashMap::new();
    let (mut verified, mut estimated, mut unknown, mut failed) = (0, 0, 0, 0);
    for (pos, (pid, platform, account, mut file)) in jobs.into_iter().enumerate() {
        if cancelled(app, run) {
            break;
        }
        let mut tried = false;
        if online
            && file.posted_date_kind.as_deref() != Some("metadata")
            && file.posted_date_kind.as_deref() != Some("verified")
        {
            if let Some(post) = file.post_id.as_deref() {
                if let Some(url) = post_url(&platform, &account, post, file.original_url.as_deref())
                {
                    tried = true;
                    let key = (platform.clone(), post.to_owned());
                    let result = if let Some(v) = cache.get(&key) {
                        v.clone()
                    } else {
                        let found = lookup(app, run, tool, &url, post);
                        cache.insert(key, found.clone());
                        thread::sleep(Duration::from_millis(500));
                        found
                    };
                    if let Some(date) = result {
                        file.posted_date = Some(date);
                        file.posted_date_kind = Some("verified".into());
                        if file.original_url.is_none() {
                            file.original_url = Some(url)
                        }
                    } else {
                        failed += 1;
                    }
                }
            }
        }
        match file.posted_date_kind.as_deref() {
            Some("metadata" | "verified") => verified += 1,
            Some("estimated_id") => estimated += 1,
            _ => unknown += 1,
        };
        let db = app.db.lock().map_err(|_| "Database is busy".to_string())?;
        db.execute("INSERT INTO source_date_results(profile_id,root,path,post_id,posted_date,date_kind,title,description,original_url,file_size,file_modified,checked_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,datetime('now')) ON CONFLICT(profile_id,root,path) DO UPDATE SET post_id=excluded.post_id,posted_date=excluded.posted_date,date_kind=excluded.date_kind,title=excluded.title,description=excluded.description,original_url=excluded.original_url,file_size=excluded.file_size,file_modified=excluded.file_modified,checked_at=excluded.checked_at",params![pid,root.to_string_lossy().as_ref(),file.path,file.post_id.unwrap_or_default(),file.posted_date.clone().unwrap_or_default(),file.posted_date_kind.clone().unwrap_or_default(),file.title.unwrap_or_default(),file.description.unwrap_or_default(),file.original_url.unwrap_or_default(),file.size as i64,file.modified as i64]).map_err(|e|e.to_string())?;
        if let Some(date) = file.posted_date {
            db.execute("UPDATE posting_queue SET posted_date=?,posted_date_kind=? WHERE source_id=? AND root=? AND path=? AND (posted_date='' OR posted_date_kind='estimated_id')",params![date,file.posted_date_kind.unwrap_or_default(),pid,root.to_string_lossy().as_ref(),file.path]).map_err(|e|e.to_string())?;
        }
        db.execute("UPDATE source_date_audits SET processed=?,verified=?,estimated=?,unknown=?,failed=?,message=? WHERE id=?",params![(pos+1) as i64,verified,estimated,unknown,failed,if tried{"Checking platform metadata…"}else{"Checking local metadata…"},run]).map_err(|e|e.to_string())?;
    }
    let was_cancelled = cancelled(app, run);
    let db = app.db.lock().map_err(|_| "Database is busy".to_string())?;
    db.execute(
        "UPDATE source_date_audits SET state=?,finished=datetime('now'),message=? WHERE id=?",
        params![
            if was_cancelled {
                "cancelled"
            } else {
                "finished"
            },
            if was_cancelled {
                "Audit stopped. Completed results were kept.".to_string()
            } else if unavailable > 0 {
                format!("Date audit complete; {unavailable} source folders unavailable.")
            } else {
                "Date audit complete.".to_string()
            },
            run
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn id_timestamp_is_estimate_and_invalid_ids_are_rejected() {
        assert_eq!(
            id_date("7262913527093005611").as_deref(),
            Some("2023-08-03")
        );
        assert!(id_date("12345678").is_none());
        assert!(id_date("99999999999999999999").is_none());
    }
    #[test]
    fn ambiguous_suffix_is_not_guessed() {
        let mut x = Index::new();
        x.insert("12345678".into(), None);
        assert!(filename_id("a [12345678].mp4", &x).is_none());
        x.insert("12345678".into(), Some("72629135212345678".into()));
        assert_eq!(
            filename_id("a [12345678].mp4", &x).as_deref(),
            Some("72629135212345678")
        );
    }
}
