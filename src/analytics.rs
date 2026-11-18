use crate::{ApiResult, App, bad, internal};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
};
use chrono::NaiveDate;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub date: String,
    #[serde(default)]
    pub post_id: String,
    #[serde(default)]
    pub followers: Option<i64>,
    #[serde(default)]
    pub following: Option<i64>,
    #[serde(default)]
    pub views: Option<i64>,
    #[serde(default)]
    pub likes: Option<i64>,
    #[serde(default)]
    pub comments: Option<i64>,
    #[serde(default)]
    pub shares: Option<i64>,
}
fn validate(s: &Snapshot) -> ApiResult<()> {
    if NaiveDate::parse_from_str(&s.date, "%Y-%m-%d").is_err() || s.date.len() != 10 {
        return Err(bad("Use dates in YYYY-MM-DD format."));
    }
    if s.post_id.len() > 200 || s.post_id.chars().any(char::is_control) {
        return Err(bad("Invalid post ID."));
    }
    let values = [
        s.followers,
        s.following,
        s.views,
        s.likes,
        s.comments,
        s.shares,
    ];
    if values.iter().all(Option::is_none) {
        return Err(bad("Enter at least one metric per row."));
    }
    if values
        .iter()
        .flatten()
        .any(|n| *n < 0 || *n > 9_007_199_254_740_991)
    {
        return Err(bad(
            "Counts must be nonnegative whole numbers within the supported range.",
        ));
    }
    if !s.post_id.is_empty() && (s.followers.is_some() || s.following.is_some()) {
        return Err(bad(
            "Followers and following belong on account rows, not post rows.",
        ));
    }
    Ok(())
}
fn exists(db: &rusqlite::Connection, id: i64) -> ApiResult<()> {
    let role: Option<String> = db
        .query_row("SELECT role FROM profiles WHERE id=?", [id], |r| r.get(0))
        .optional()
        .map_err(internal)?;
    match role.as_deref() {
        Some("destination") => Ok(()),
        _ => Err((
            axum::http::StatusCode::NOT_FOUND,
            "Posting profile not found.".into(),
        )),
    }
}
fn save(db: &mut rusqlite::Connection, id: i64, rows: &[Snapshot], source: &str) -> ApiResult<()> {
    exists(db, id)?;
    for s in rows {
        validate(s)?;
    }
    let tx = db.transaction().map_err(internal)?;
    for s in rows {
        tx.execute("INSERT INTO analytics_snapshots (profile_id, date, post_id, followers, following, views, likes, comments, shares, source, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(profile_id,date,post_id) DO UPDATE SET followers=COALESCE(excluded.followers,followers), following=COALESCE(excluded.following,following), views=COALESCE(excluded.views,views), likes=COALESCE(excluded.likes,likes), comments=COALESCE(excluded.comments,comments), shares=COALESCE(excluded.shares,shares),source=excluded.source,updated_at=excluded.updated_at",params![id,s.date,s.post_id,s.followers,s.following,s.views,s.likes,s.comments,s.shares,source]).map_err(internal)?;
    }
    tx.commit().map_err(internal)?;
    Ok(())
}
#[derive(Serialize)]
pub struct Record {
    #[serde(flatten)]
    data: Snapshot,
    source: String,
    updated_at: String,
    approx: Vec<String>,
}
pub async fn get(State(app): State<App>, Path(id): Path<i64>) -> ApiResult<Json<Vec<Record>>> {
    let db = app.db.lock().map_err(internal)?;
    exists(&db, id)?;
    let mut stmt=db.prepare("SELECT date,post_id,followers,following,views,likes,comments,shares,source,updated_at FROM analytics_snapshots WHERE profile_id=? ORDER BY date DESC, updated_at DESC LIMIT 10000").map_err(internal)?;
    let mut result = stmt
        .query_map([id], |r| {
            Ok(Record {
                data: Snapshot {
                    date: r.get(0)?,
                    post_id: r.get(1)?,
                    followers: r.get(2)?,
                    following: r.get(3)?,
                    views: r.get(4)?,
                    likes: r.get(5)?,
                    comments: r.get(6)?,
                    shares: r.get(7)?,
                },
                source: r.get(8)?,
                updated_at: r.get(9)?,
                approx: vec![],
            })
        })
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    let mut live=db.prepare("SELECT observed_at,followers,following,likes,followers_approx,following_approx,likes_approx FROM analytics_bridge_snapshots WHERE profile_id=? ORDER BY observed_at DESC LIMIT 10000").map_err(internal)?;
    let extra = live
        .query_map([id], |r| {
            let observed: String = r.get(0)?;
            let day = chrono::DateTime::parse_from_rfc3339(&observed)
                .map(|value| {
                    value
                        .with_timezone(&chrono::Local)
                        .date_naive()
                        .format("%Y-%m-%d")
                        .to_string()
                })
                .unwrap_or_else(|_| observed.get(..10).unwrap_or("").to_string());
            Ok(Record {
                data: Snapshot {
                    date: day,
                    post_id: String::new(),
                    followers: r.get(1)?,
                    following: r.get(2)?,
                    views: None,
                    likes: r.get(3)?,
                    comments: None,
                    shares: None,
                },
                source: "Brave profile".into(),
                updated_at: observed,
                approx: [
                    ("followers", r.get::<_, bool>(4)?),
                    ("following", r.get::<_, bool>(5)?),
                    ("likes", r.get::<_, bool>(6)?),
                ]
                .into_iter()
                .filter_map(|(name, flag)| flag.then_some(name.to_owned()))
                .collect(),
            })
        })
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    result.extend(extra);
    result.sort_by(|a, b| {
        b.data
            .date
            .cmp(&a.data.date)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
    Ok(Json(result))
}
pub async fn add(
    State(app): State<App>,
    Path(id): Path<i64>,
    Json(s): Json<Snapshot>,
) -> ApiResult<axum::http::StatusCode> {
    let mut db = app.db.lock().map_err(internal)?;
    save(&mut db, id, &[s], "Manual")?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
#[derive(Serialize)]
pub struct ImportResult {
    imported: usize,
}
pub async fn import(
    State(app): State<App>,
    Path(id): Path<i64>,
    body: Bytes,
) -> ApiResult<Json<ImportResult>> {
    if body.len() > 2 * 1024 * 1024 {
        return Err(bad("CSV must be 2 MB or smaller."));
    }
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(body.as_ref());
    let headers = reader
        .headers()
        .map_err(|_| bad("Could not read CSV headers."))?
        .clone();
    let names: Vec<String> = headers
        .iter()
        .map(|h| h.trim_start_matches('\u{feff}').to_ascii_lowercase())
        .collect();
    if !names.iter().any(|h| h == "date")
        || !names.iter().any(|h| {
            [
                "followers",
                "following",
                "views",
                "likes",
                "comments",
                "shares",
            ]
            .contains(&h.as_str())
        })
    {
        return Err(bad(
            "CSV needs a date column and at least one supported metric column. See the template.",
        ));
    }
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    if sorted.len() != names.len() {
        return Err(bad("CSV has duplicate column names."));
    }
    let mut rows = Vec::new();
    for (i, rec) in reader.records().enumerate() {
        if i >= 10000 {
            return Err(bad("CSV is limited to 10,000 rows."));
        }
        let rec = rec.map_err(|_| bad(format!("Invalid CSV on row {}.", i + 2)))?;
        let field = |name: &str| -> &str {
            names
                .iter()
                .position(|h| h == name)
                .and_then(|j| rec.get(j))
                .unwrap_or("")
        };
        let number = |name: &str| -> ApiResult<Option<i64>> {
            let v = field(name);
            if v.is_empty() {
                Ok(None)
            } else {
                v.parse::<i64>()
                    .map(Some)
                    .map_err(|_| bad(format!("Invalid {} count on row {}.", name, i + 2)))
            }
        };
        let s = Snapshot {
            date: field("date").to_owned(),
            post_id: field("post_id").to_owned(),
            followers: number("followers")?,
            following: number("following")?,
            views: number("views")?,
            likes: number("likes")?,
            comments: number("comments")?,
            shares: number("shares")?,
        };
        validate(&s).map_err(|(_, e)| bad(format!("Row {}: {}", i + 2, e)))?;
        rows.push(s);
    }
    if rows.is_empty() {
        return Err(bad("CSV contains no data rows."));
    }
    let mut db = app.db.lock().map_err(internal)?;
    save(&mut db, id, &rows, "CSV import")?;
    Ok(Json(ImportResult {
        imported: rows.len(),
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn row() -> Snapshot {
        Snapshot {
            date: "2026-11-18".into(),
            post_id: String::new(),
            followers: Some(100),
            following: None,
            views: None,
            likes: None,
            comments: None,
            shares: None,
        }
    }
    #[test]
    fn validates_counts_dates_and_scope() {
        let mut s = row();
        assert!(validate(&s).is_ok());
        s.followers = Some(-1);
        assert!(validate(&s).is_err());
        s.followers = Some(1);
        s.post_id = "123".into();
        assert!(validate(&s).is_err());
        s.post_id.clear();
        s.date = "2026-02-30".into();
        assert!(validate(&s).is_err());
    }
    #[test]
    fn saves_idempotently_and_preserves_blanks() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::init(&db).unwrap();
        db.execute("INSERT INTO profiles(name,platform,url,role) VALUES ('a','tiktok','https://tiktok.com/@a','destination')",[]).unwrap();
        let s = row();
        save(&mut db, 1, &[s.clone()], "Manual").unwrap();
        let mut second = s;
        second.followers = None;
        second.likes = Some(8);
        save(&mut db, 1, &[second], "CSV import").unwrap();
        let v: (i64, i64, i64) = db
            .query_row(
                "SELECT COUNT(*),followers,likes FROM analytics_snapshots",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(v, (1, 100, 8));
    }
}
