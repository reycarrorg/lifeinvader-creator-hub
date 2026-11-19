use crate::{ApiResult, App, bad, internal, source};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Input {
    post_url: String,
}
pub(crate) fn valid_seed(profile_url: &str, post_url: &str) -> bool {
    let Ok(profile) = profile_url.parse::<axum::http::Uri>() else {
        return false;
    };
    let Ok(post) = post_url.parse::<axum::http::Uri>() else {
        return false;
    };
    if post.scheme_str() != Some("https")
        || post
            .authority()
            .is_none_or(|a| a.as_str().contains('@') || a.port().is_some())
        || !["tiktok.com", "www.tiktok.com", "m.tiktok.com"].contains(&post.host().unwrap_or(""))
    {
        return false;
    }
    let account = profile
        .path()
        .split('/')
        .filter(|v| !v.is_empty())
        .next()
        .unwrap_or("");
    let parts: Vec<_> = post.path().split('/').filter(|v| !v.is_empty()).collect();
    parts.len() == 3
        && parts[0].eq_ignore_ascii_case(account)
        && ["video", "photo"].contains(&parts[1])
        && !parts[2].is_empty()
        && parts[2].chars().all(|c| c.is_ascii_digit())
}
pub async fn get(
    State(app): State<App>,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().map_err(internal)?;
    let (platform, _, _) = source::account_profile(&db, id, "destination")?;
    if platform != "tiktok" {
        return Err(bad("Recovery links are available for TikTok profiles."));
    }
    let seed: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key=?",
            [format!("profile_recovery_seed_{id}")],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?;
    Ok(Json(
        serde_json::json!({"post_url":seed.unwrap_or_default()}),
    ))
}
pub async fn save(
    State(app): State<App>,
    Path(id): Path<i64>,
    Json(input): Json<Input>,
) -> ApiResult<StatusCode> {
    let db = app.db.lock().map_err(internal)?;
    let (platform, url, _) = source::account_profile(&db, id, "destination")?;
    if platform != "tiktok" {
        return Err(bad("Recovery links are available for TikTok profiles."));
    }
    let seed = input.post_url.trim();
    if !seed.is_empty() && (seed.len() > 2048 || !valid_seed(&url, seed)) {
        return Err(bad(
            "Use a full TikTok video or photo link from this same account.",
        ));
    }
    let key = format!("profile_recovery_seed_{id}");
    if seed.is_empty() {
        db.execute("DELETE FROM settings WHERE key=?", [key])
            .map_err(internal)?;
    } else {
        db.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,seed]).map_err(internal)?;
    }
    Ok(StatusCode::NO_CONTENT)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checks_account_and_host() {
        let p = "https://www.tiktok.com/@example";
        assert!(valid_seed(p, "https://www.tiktok.com/@example/video/12345"));
        assert!(!valid_seed(p, "https://www.tiktok.com/@other/video/12345"));
        assert!(!valid_seed(
            p,
            "https://tiktok.com.evil.test/@example/video/12345"
        ));
    }
}
