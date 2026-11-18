use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Deserialize)]
pub struct ScheduleInput {
    enabled: bool,
    interval_seconds: i64,
}
#[derive(Deserialize)]
pub struct BatchInput {
    profile_ids: Vec<i64>,
    #[serde(default)]
    include_photos: bool,
}
#[derive(Deserialize)]
pub struct BatchSetting {
    include: bool,
}
pub async fn save_batch_setting(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    Json(input): Json<BatchSetting>,
) -> ApiResult<StatusCode> {
    let db = app.db.lock().map_err(internal)?;
    source::source_profile(&db, id)?;
    db.execute("INSERT INTO source_preferences(profile_id,include_in_batch) VALUES(?,?) ON CONFLICT(profile_id) DO UPDATE SET include_in_batch=excluded.include_in_batch",params![id,input.include]).map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn schedules(State(app): State<App>) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().map_err(internal)?;
    let root = source::config(&db)
        .ok()
        .and_then(|c| source::checked_root(&c).ok())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut stmt=db.prepare("SELECT p.id,COALESCE(s.enabled,0),COALESCE(s.interval_seconds,3600),s.next_run, (SELECT state FROM source_queue q WHERE q.profile_id=p.id AND q.state IN ('pending','starting','running') ORDER BY q.id LIMIT 1), (SELECT state FROM source_sync_runs r WHERE r.profile_id=p.id ORDER BY r.id DESC LIMIT 1), (SELECT finished FROM source_sync_runs r WHERE r.profile_id=p.id ORDER BY r.id DESC LIMIT 1), (SELECT COUNT(*) FROM source_new_files f WHERE f.profile_id=p.id AND f.seen=0 AND f.root=?), (SELECT CASE WHEN state='error' THEN message END FROM source_queue q WHERE q.profile_id=p.id ORDER BY q.id DESC LIMIT 1), COALESCE(pref.include_in_batch,1) FROM profiles p LEFT JOIN source_schedules s ON s.profile_id=p.id LEFT JOIN source_preferences pref ON pref.profile_id=p.id WHERE p.role IN ('source','destination') ORDER BY p.id").map_err(internal)?;
    let records=stmt.query_map([root],|r|Ok(serde_json::json!({"profile_id":r.get::<_,i64>(0)?,"enabled":r.get::<_,bool>(1)?,"interval_seconds":r.get::<_,i64>(2)?,"next_run":r.get::<_,Option<i64>>(3)?,"queue_state":r.get::<_,Option<String>>(4)?,"last_state":r.get::<_,Option<String>>(5)?,"last_finished":r.get::<_,Option<String>>(6)?,"new_count":r.get::<_,i64>(7)?,"last_queue_error":r.get::<_,Option<String>>(8)?,"include_in_batch":r.get::<_,bool>(9)?}))).map_err(internal)?.collect::<Result<Vec<_>,_>>().map_err(internal)?;
    let mut s=db.prepare("SELECT batch_id, COUNT(*), SUM(state='pending' OR state='starting' OR state='running'), SUM(state='finished'), SUM(state='error' OR state='interrupted') FROM source_queue WHERE origin='batch' GROUP BY batch_id ORDER BY MIN(id) DESC LIMIT 1").map_err(internal)?;
    let batch:Option<serde_json::Value>=s.query_row([],|r|Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"total":r.get::<_,i64>(1)?,"remaining":r.get::<_,i64>(2)?,"finished":r.get::<_,i64>(3)?,"errors":r.get::<_,i64>(4)?}))).optional().map_err(internal)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(Json(
        serde_json::json!({"schedules":records,"batch":batch,"now":now}),
    ))
}
pub async fn save_schedule(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    Json(input): Json<ScheduleInput>,
) -> ApiResult<StatusCode> {
    save_schedule_for(app, id, input, "source")
}
pub async fn save_profile_schedule(
    State(app): State<App>,
    AxumPath(id): AxumPath<i64>,
    Json(input): Json<ScheduleInput>,
) -> ApiResult<StatusCode> {
    save_schedule_for(app, id, input, "destination")
}
fn save_schedule_for(app: App, id: i64, input: ScheduleInput, role: &str) -> ApiResult<StatusCode> {
    if input.enabled && !(60..=604800).contains(&input.interval_seconds) {
        return Err(bad("Choose an interval from 1 minute to 7 days."));
    }
    let mut db = app.db.lock().map_err(internal)?;
    source::account_profile(&db, id, role)?;
    let tx = db.transaction().map_err(internal)?;
    let seconds = if input.enabled {
        input.interval_seconds
    } else {
        input.interval_seconds.clamp(60, 604800)
    };
    tx.execute("INSERT INTO source_schedules(profile_id,interval_seconds,enabled,next_run) VALUES(?,?,?,CASE WHEN ? THEN unixepoch()+? ELSE NULL END) ON CONFLICT(profile_id) DO UPDATE SET interval_seconds=excluded.interval_seconds,enabled=excluded.enabled,next_run=excluded.next_run",params![id,seconds,input.enabled,input.enabled,seconds]).map_err(internal)?;
    if !input.enabled {
        tx.execute("UPDATE source_queue SET state='cancelled',message='Automatic sync disabled' WHERE profile_id=? AND origin='auto' AND state='pending'",[id]).map_err(internal)?;
    }
    tx.commit().map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn enqueue_batch(
    State(app): State<App>,
    Json(input): Json<BatchInput>,
) -> ApiResult<impl IntoResponse> {
    if input.profile_ids.is_empty() || input.profile_ids.len() > 100 {
        return Err(bad("Choose one to 100 sources."));
    }
    let mut db = app.db.lock().map_err(internal)?;
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for id in &input.profile_ids {
        if !seen.insert(*id) {
            continue;
        }
        let (platform, _, _) = source::source_profile(&db, *id)?;
        if input.include_photos && platform != "tiktok" {
            return Err(bad("Photo sync is available only for TikTok sources."));
        }
        unique.push(*id);
    }
    let tx = db.transaction().map_err(internal)?;
    let batch_id = Uuid::new_v4().to_string();
    let mut added = 0;
    let mut skipped = 0;
    let mut excluded = 0;
    for id in unique {
        let include: bool=tx.query_row("SELECT COALESCE((SELECT include_in_batch FROM source_preferences WHERE profile_id=?),1)",[id],|r|r.get(0)).map_err(internal)?;
        if !include {
            excluded += 1;
            continue;
        }
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM source_queue WHERE profile_id=? AND state IN ('pending','starting','running')) OR EXISTS(SELECT 1 FROM source_sync_runs WHERE profile_id=? AND state='running')",params![id,id],|r|r.get(0)).map_err(internal)?;
        if exists {
            skipped += 1;
            continue;
        }
        tx.execute("INSERT INTO source_queue(batch_id,profile_id,origin,state,message,created) VALUES(?,?,'batch','pending',?,unixepoch())",params![batch_id,id,if input.include_photos{"photos"}else{"videos"}]).map_err(internal)?;
        added += 1;
    }
    tx.commit().map_err(internal)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(
            serde_json::json!({"batch_id":batch_id,"added":added,"skipped":skipped,"excluded":excluded}),
        ),
    ))
}
fn tick(app: &App) -> rusqlite::Result<()> {
    let job = {
        let mut db = app.db.lock().map_err(|_| rusqlite::Error::InvalidQuery)?;
        let tx = db.transaction()?;
        tx.execute("UPDATE source_queue SET state=COALESCE((SELECT CASE WHEN r.state='finished' THEN 'finished' WHEN r.state='running' THEN 'running' ELSE 'error' END FROM source_sync_runs r WHERE r.id=source_queue.run_id),'error') WHERE state='running' AND (NOT EXISTS(SELECT 1 FROM source_sync_runs r WHERE r.id=source_queue.run_id) OR EXISTS(SELECT 1 FROM source_sync_runs r WHERE r.id=source_queue.run_id AND r.state!='running'))",[])?;
        tx.execute("INSERT INTO source_queue(batch_id,profile_id,origin,state,message,created) SELECT 'auto',s.profile_id,'auto','pending','videos',unixepoch() FROM source_schedules s WHERE s.enabled=1 AND s.next_run IS NOT NULL AND s.next_run<=unixepoch() AND NOT EXISTS(SELECT 1 FROM source_queue q WHERE q.profile_id=s.profile_id AND q.state IN ('pending','starting','running')) AND NOT EXISTS(SELECT 1 FROM source_sync_runs r WHERE r.profile_id=s.profile_id AND r.state='running')",[])?;
        tx.execute("UPDATE source_schedules SET next_run=NULL WHERE profile_id IN (SELECT profile_id FROM source_queue WHERE origin='auto' AND state='pending') AND enabled=1",[])?;
        let busy:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM source_sync_runs WHERE state='running') OR EXISTS(SELECT 1 FROM source_date_audits WHERE state='running') OR EXISTS(SELECT 1 FROM source_queue WHERE state='starting')",[],|r|r.get(0))?;
        let job: Option<(i64, i64, String, String)> = if busy {
            None
        } else {
            tx.query_row("SELECT q.id,q.profile_id,q.message,p.role FROM source_queue q JOIN profiles p ON p.id=q.profile_id WHERE q.state='pending' ORDER BY q.id LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?
        };
        if let Some((id, _, _, _)) = &job {
            tx.execute("UPDATE source_queue SET state='starting' WHERE id=?", [id])?;
        }
        tx.commit()?;
        job
    };
    if let Some((queue_id, profile_id, mode, role)) = job {
        let result = source::start_sync_for(
            app.clone(),
            profile_id,
            source::SyncInput {
                include_photos: mode == "photos",
            },
            &role,
        );
        let db = app.db.lock().map_err(|_| rusqlite::Error::InvalidQuery)?;
        match result {
            Ok(run_id) => {
                db.execute(
                    "UPDATE source_queue SET state='running',run_id=? WHERE id=?",
                    params![run_id, queue_id],
                )?;
            }
            Err((StatusCode::CONFLICT, _)) => {
                db.execute(
                    "UPDATE source_queue SET state='pending' WHERE id=?",
                    [queue_id],
                )?;
            }
            Err((_, message)) => {
                db.execute(
                    "UPDATE source_queue SET state='error',message=? WHERE id=?",
                    params![message, queue_id],
                )?;
                db.execute("UPDATE source_schedules SET next_run=unixepoch()+interval_seconds WHERE profile_id=? AND enabled=1 AND next_run IS NULL",[profile_id])?;
            }
        }
    }
    Ok(())
}
pub async fn worker(app: App) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        interval.tick().await;
        if let Err(e) = tick(&app) {
            eprintln!("Source scheduler: {e}");
        }
    }
}
