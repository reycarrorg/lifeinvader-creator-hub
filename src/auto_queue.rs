use crate::{ApiResult, App, bad, internal, queue, source};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Local;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Filters {
    pub category: String,
    pub source_id: Option<i64>,
    pub kind: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub from: String,
    pub to: String,
    pub search: String,
    pub unknown: bool,
    pub estimated: bool,
}
#[derive(Deserialize)]
pub struct Input {
    enabled: bool,
    interval_seconds: i64,
    max_pending: i64,
    filters: Filters,
    #[serde(default)]
    instructions: String,
}
#[derive(Serialize)]
pub struct Rule {
    profile_id: i64,
    enabled: bool,
    interval_seconds: i64,
    max_pending: i64,
    filters: Filters,
    next_run: Option<i64>,
    last_result: String,
    instructions: String,
}
fn valid(f: &Filters) -> ApiResult<()> {
    if !["", "video", "image"].contains(&f.kind.as_str())
        || f.category.len() > 200
        || f.search.len() > 500
        || f.source_id.is_some_and(|x| x <= 0)
    {
        return Err(bad("Invalid source filters."));
    }
    if f.min
        .is_some_and(|n| !n.is_finite() || n < 0.0 || n > 604800.0)
        || f.max
            .is_some_and(|n| !n.is_finite() || n < 0.0 || n > 604800.0)
        || matches!((f.min,f.max),(Some(a),Some(b)) if a>b)
    {
        return Err(bad("Check the length range."));
    }
    for d in [&f.from, &f.to] {
        if !d.is_empty()
            && (d.len() != 10 || chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err())
        {
            return Err(bad("Use a valid date."));
        }
    }
    if !f.from.is_empty() && !f.to.is_empty() && f.from > f.to {
        return Err(bad("Start date must be before end date."));
    }
    Ok(())
}
pub async fn list(State(app): State<App>) -> ApiResult<Json<Vec<Rule>>> {
    let db = app.db.lock().map_err(internal)?;
    let mut s=db.prepare("SELECT r.profile_id,r.enabled,r.interval_seconds,r.max_pending,r.filters,r.next_run,r.last_result,r.instructions FROM auto_queue_rules r JOIN profiles p ON p.id=r.profile_id WHERE p.role='destination' AND p.archived=0").map_err(internal)?;
    let rows = s
        .query_map([], |r| {
            let raw: String = r.get(4)?;
            Ok(Rule {
                profile_id: r.get(0)?,
                enabled: r.get(1)?,
                interval_seconds: r.get(2)?,
                max_pending: r.get(3)?,
                filters: serde_json::from_str(&raw).unwrap_or_default(),
                next_run: r.get(5)?,
                last_result: r.get(6)?,
                instructions: r.get(7)?,
            })
        })
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    Ok(Json(rows))
}
pub async fn save(
    State(app): State<App>,
    Path(id): Path<i64>,
    Json(input): Json<Input>,
) -> ApiResult<StatusCode> {
    valid(&input.filters)?;
    if input.instructions.len() > 4000 {
        return Err(bad("Posting instructions must be under 4000 bytes."));
    }
    if !(60..=2_592_000).contains(&input.interval_seconds)
        || !(1..=100).contains(&input.max_pending)
    {
        return Err(bad(
            "Choose an interval between 1 minute and 30 days, and a queue limit between 1 and 100.",
        ));
    }
    let db = app.db.lock().map_err(internal)?;
    let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM profiles WHERE id=? AND role='destination' AND archived=0)",[id],|r|r.get(0)).map_err(internal)?;
    if !exists {
        return Err((StatusCode::NOT_FOUND, "Profile not found.".into()));
    }
    let filters = serde_json::to_string(&input.filters).map_err(internal)?;
    db.execute("INSERT INTO auto_queue_rules(profile_id,enabled,interval_seconds,max_pending,filters,next_run,last_result,instructions) VALUES(?,?,?,?,?,CASE WHEN ? THEN unixepoch()+? ELSE NULL END,'',?) ON CONFLICT(profile_id) DO UPDATE SET enabled=excluded.enabled,interval_seconds=excluded.interval_seconds,max_pending=excluded.max_pending,filters=excluded.filters,next_run=excluded.next_run,last_result='',instructions=excluded.instructions,generation=auto_queue_rules.generation+1",params![id,input.enabled,input.interval_seconds,input.max_pending,filters,input.enabled,input.interval_seconds,input.instructions]).map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
fn text<'a>(v: &'a serde_json::Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}
fn platform(p: &str, solo: bool, f: &serde_json::Value) -> String {
    if solo {
        let x = text(f, "original_platform");
        if x.is_empty() {
            "other".into()
        } else {
            x.into()
        }
    } else {
        p.into()
    }
}
async fn run_rule(
    app: &App,
    id: i64,
    filters: &Filters,
    max: i64,
    raw: &str,
    next_run: i64,
    generation: i64,
) -> Result<String, String> {
    let sources = {
        let db = app
            .db
            .lock()
            .map_err(|_| "Database unavailable".to_string())?;
        let mut stmt=db.prepare("SELECT id,name,platform,url,tags FROM profiles WHERE role='source' AND archived=0 ORDER BY id").map_err(|e|e.to_string())?;
        let result = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        result
    };
    let axum::Json(queue_items) = queue::list(State(app.clone()), Path(id))
        .await
        .map_err(|e| e.1)?;
    if queue_items.iter().filter(|q| q.state == "queued").count() >= max as usize {
        return Ok("Queue limit reached; waiting for a status update.".into());
    }
    let mut candidates = Vec::<(i64, String, String, String)>::new();
    let mut unavailable = 0;
    for (sid, name, plat, url, tags) in sources {
        let solo = url == crate::solo::SOLO_URL;
        let tag_list: Vec<String> = serde_json::from_str(&tags).unwrap_or_default();
        if filters.source_id.is_some_and(|x| x != sid)
            || (!filters.category.is_empty()
                && if filters.category == "__lifeinvader,solo_posts__" {
                    !solo
                } else {
                    !tag_list
                        .iter()
                        .any(|x| x.eq_ignore_ascii_case(&filters.category))
                })
        {
            continue;
        }
        let axum::Json(o) = match source::overview(State(app.clone()), Path(sid)).await {
            Ok(o) => o,
            Err(_) => {
                unavailable += 1;
                continue;
            }
        };
        if o["configured"] != true {
            unavailable += 1;
            continue;
        }
        let files = o["files"].as_array().cloned().unwrap_or_default();
        let videos: std::collections::HashSet<(String, String)> = files
            .iter()
            .filter(|f| text(f, "kind") == "video" && !text(f, "post_id").is_empty())
            .map(|f| {
                (
                    text(f, "path")
                        .rsplit_once('.')
                        .map(|x| x.0)
                        .unwrap_or("")
                        .into(),
                    text(f, "post_id").into(),
                )
            })
            .collect();
        for f in &files {
            let kind = text(f, "kind");
            let path = text(f, "path");
            let pid = text(f, "post_id");
            if solo
                && kind == "image"
                && !pid.is_empty()
                && videos.contains(&(
                    path.rsplit_once('.').map(|x| x.0).unwrap_or("").into(),
                    pid.into(),
                ))
            {
                continue;
            }
            if !filters.kind.is_empty() && filters.kind != kind {
                continue;
            }
            if filters.min.is_some() || filters.max.is_some() {
                let Some(d) = f.get("duration_seconds").and_then(|v| v.as_f64()) else {
                    continue;
                };
                if kind != "video"
                    || filters.min.is_some_and(|n| d < n)
                    || filters.max.is_some_and(|n| d > n)
                {
                    continue;
                }
            }
            let date = text(f, "posted_date");
            if !filters.estimated && text(f, "posted_date_kind") == "estimated_id" {
                continue;
            }
            if (!filters.from.is_empty() || !filters.to.is_empty())
                && date.is_empty()
                && !filters.unknown
            {
                continue;
            }
            if !date.is_empty()
                && ((!filters.from.is_empty() && date < filters.from.as_str())
                    || (!filters.to.is_empty() && date > filters.to.as_str()))
            {
                continue;
            }
            let query = filters.search.trim().to_lowercase();
            if !query.is_empty()
                && !format!(
                    "{} {} {} {} {}",
                    text(f, "name"),
                    text(f, "title"),
                    text(f, "description"),
                    text(f, "original_account"),
                    name
                )
                .to_lowercase()
                .contains(&query)
            {
                continue;
            }
            let p = platform(&plat, solo, f);
            if queue_items.iter().any(|q| {
                (q.source_id == sid && q.path == path)
                    || (!pid.is_empty()
                        && q.post_id == pid
                        && (q.platform == p || q.source_id == sid))
            }) {
                continue;
            }
            candidates.push((sid, path.into(), p, pid.into()));
        }
    }
    if candidates.is_empty() {
        return Ok(if unavailable > 0 {
            "No matching posts; some sources could not be opened."
        } else {
            "No matching unqueued posts."
        }
        .into());
    }
    let start = (uuid::Uuid::new_v4().as_u128() % candidates.len() as u128) as usize;
    candidates.rotate_left(start);
    for (sid, path, _, _) in candidates {
        let scheduled = Local::now().format("%Y-%m-%dT%H:%M").to_string();
        let input = queue::Add {
            source_id: sid,
            path,
            scheduled_for: scheduled,
            edit_id: String::new(),
            caption: String::new(),
            auto_guard: Some((raw.to_string(), next_run, max, generation)),
        };
        match queue::add(State(app.clone()), Path(id), Json(input)).await {
            Ok(_) => {
                return Ok(if unavailable > 0 {
                    "Added a post; some sources could not be opened."
                } else {
                    "Added a post to the queue."
                }
                .into());
            }
            Err((StatusCode::CONFLICT, _)) => continue,
            Err(e) => return Err(e.1),
        }
    }
    Ok("No matching unqueued posts.".into())
}
pub async fn worker(app: App) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let due = {
            let Ok(db) = app.db.lock() else { continue };
            let Ok(mut s)=db.prepare("SELECT r.profile_id,r.filters,r.max_pending,r.generation FROM auto_queue_rules r JOIN profiles p ON p.id=r.profile_id WHERE r.enabled=1 AND r.next_run<=unixepoch() AND p.archived=0 AND p.role='destination'") else{continue};
            let rows = s
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                })
                .and_then(|x| x.collect::<Result<Vec<_>, _>>())
                .unwrap_or_default();
            rows
        };
        for (id, raw, max, generation) in due {
            let claimed = {
                let Ok(db) = app.db.lock() else { continue };
                db.query_row("UPDATE auto_queue_rules SET next_run=unixepoch()+interval_seconds,last_result='Checking for matching posts…' WHERE profile_id=? AND enabled=1 AND next_run<=unixepoch() AND filters=? AND generation=? RETURNING next_run",params![id,raw,generation],|r|r.get::<_,i64>(0)).optional().ok().flatten()
            };
            let Some(next_run) = claimed else {
                continue;
            };
            let result = match serde_json::from_str::<Filters>(&raw) {
                Ok(f) if valid(&f).is_ok() => run_rule(&app, id, &f, max, &raw, next_run, generation)
                    .await
                    .unwrap_or_else(|e| format!("Could not queue a post: {e}")),
                _ => "Invalid saved filters; update your auto queue settings.".into(),
            };
            if let Ok(db) = app.db.lock() {
                let _=db.execute("UPDATE auto_queue_rules SET last_result=? WHERE profile_id=? AND enabled=1 AND filters=? AND generation=?",params![result,id,raw,generation]);
            }
        }
    }
}
