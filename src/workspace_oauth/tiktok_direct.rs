use super::{direct_tiktok_identity, parse_tiktok_upload_url, platform, tiktok_direct_receipts as receipts};
use crate::{ApiResult, App, bad, internal, workspace_film_roll as film};
use axum::{
    Json, body::Body,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs::File, io::{Read, Seek}, path::PathBuf};
use tower::ServiceExt;
use tower_http::services::ServeFile;

#[derive(Deserialize)]
pub(crate) struct AssetQuery {
    use_source: bool,
    sha256: Option<String>,
    account_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Post {
    use_source: bool,
    account_id: String,
    sha256: String,
    title: String,
    privacy_level: String,
    allow_comment: bool,
    allow_duet: bool,
    allow_stitch: bool,
    disclose_commercial: bool,
    brand_organic_toggle: bool,
    brand_content_toggle: bool,
    is_aigc: bool,
    consent: bool,
}

#[derive(Deserialize, Serialize)]
struct Creator {
    creator_username: String,
    creator_nickname: String,
    #[serde(default)]
    creator_avatar_url: String,
    privacy_level_options: Vec<String>,
    comment_disabled: bool,
    duet_disabled: bool,
    stitch_disabled: bool,
    max_video_post_duration_sec: u32,
}

fn validate(post: &Post, creator: &Creator, duration_ms: u64) -> ApiResult<()> {
    if !post.consent { return Err(bad("Confirm the TikTok posting agreement before publishing.")); }
    if post.title.encode_utf16().count() > 2200 || post.title.contains('\0') {
        return Err(bad("TikTok text must fit within 2,200 UTF-16 units and cannot contain a null character."));
    }
    if !creator.privacy_level_options.contains(&post.privacy_level) {
        return Err(bad("Choose a visibility currently offered for this account."));
    }
    if (post.allow_comment && creator.comment_disabled) || (post.allow_duet && creator.duet_disabled)
        || (post.allow_stitch && creator.stitch_disabled) {
        return Err(bad("The account's interaction settings changed. Review the post again."));
    }
    if post.disclose_commercial != (post.brand_content_toggle || post.brand_organic_toggle) {
        return Err(bad("Select the kind of commercial content being disclosed."));
    }
    if post.brand_content_toggle && !matches!(post.privacy_level.as_str(), "PUBLIC_TO_EVERYONE" | "MUTUAL_FOLLOW_FRIENDS") {
        return Err(bad("A paid partnership requires Everyone or Friends visibility."));
    }
    if duration_ms == 0 || duration_ms > u64::from(creator.max_video_post_duration_sec) * 1000 {
        return Err(bad("The selected video exceeds this account's duration limit or has no readable duration."));
    }
    Ok(())
}

fn same_origin(headers: &HeaderMap) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else { return false };
    let allowed = host.strip_prefix("127.0.0.1:").or_else(|| host.strip_prefix("localhost:"))
        .is_some_and(|port| port.parse::<u16>().is_ok());
    allowed && headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) == Some(format!("http://{host}").as_str())
}

// Resolve the exact variant the creator selected. Never substitute a source for a missing export.
fn asset_path(app: &App, clip_id: &str, source: bool) -> ApiResult<PathBuf> {
    if !crate::workspace_media::hex(clip_id) { return Err(bad("Invalid clip ID.")); }
    if source {
        return film::resolve_video(app, clip_id).ok_or((StatusCode::NOT_FOUND, "Source video is unavailable.".into()));
    }
    let db = app.db.lock().map_err(internal)?;
    let name: Option<String> = db.query_row(
        "SELECT exported_path FROM workspace_film_roll WHERE clip_id=?1", [clip_id], |r| r.get(0),
    ).optional().map_err(internal)?;
    let name = name.filter(|s| !s.is_empty() && !s.starts_with('.') && !s.contains(['/', '\\', ':', '\0']))
        .ok_or((StatusCode::NOT_FOUND, "Selected export is unavailable. Export the clip again.".into()))?;
    Ok(film::exports_dir(app).join(name))
}

struct Asset {
    file: File,
    bytes: u64,
    sha: String,
    mime: &'static str,
}

fn open_asset(path: &std::path::Path) -> ApiResult<Asset> {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    let mime = match ext.as_str() {
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        _ => return Err(bad("Choose an MP4, MOV, or WebM video, or export an MP4 first.")),
    };
    let mut options = File::options();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1); // FILE_SHARE_READ: keep immutable while probing, hashing, and uploading.
    }
    let mut file = options.open(path).map_err(|_| bad("Selected video is unavailable or being changed."))?;
    let bytes = file.metadata().map_err(internal)?.len();
    if !(1..=4 * 1024 * 1024 * 1024u64).contains(&bytes) { return Err(bad("TikTok accepts files up to 4 GB.")); }
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest).map_err(internal)?;
    file.rewind().map_err(internal)?;
    Ok(Asset { file, bytes, sha: format!("{:x}", digest.finalize()), mime })
}

fn duration(path: &std::path::Path) -> ApiResult<u64> {
    let meta = film::probe_mp4_summary(path).filter(|m| {
        m.width > 0 && m.height > 0 && m.duration_s.is_finite() && m.duration_s > 0.0
    }).ok_or_else(|| bad("Could not read this video's duration. Verify that FFprobe is available."))?;
    Ok((meta.duration_s * 1000.0).ceil() as u64)
}

fn tiktok_json(token: &str, path: &str, body: &Value) -> Result<Value, String> {
    let auth = format!("Bearer {token}");
    let response = platform::https_request(
        "open.tiktokapis.com", "POST", path,
        &[("Authorization", &auth), ("Content-Type", "application/json; charset=UTF-8")],
        &serde_json::to_vec(body).map_err(|e| e.to_string())?, 20_000,
    )?;
    let payload: Value = serde_json::from_slice(&response.body)
        .map_err(|_| format!("TikTok returned an unreadable response (HTTP {}).", response.status))?;
    let code = payload["error"]["code"].as_str().unwrap_or("missing_status");
    if response.status != 200 || code != "ok" {
        let message = payload["error"]["message"].as_str().unwrap_or("TikTok rejected the request.");
        return Err(format!("TikTok: {message} ({code}, HTTP {}).", response.status));
    }
    Ok(payload["data"].clone())
}

fn current_creator(token: &str) -> ApiResult<Creator> {
    let data = tiktok_json(token, "/v2/post/publish/creator_info/query/", &json!({})).map_err(bad)?;
    let creator: Creator = serde_json::from_value(data).map_err(|_| bad("TikTok returned incomplete creator settings."))?;
    if creator.creator_nickname.trim().is_empty() || creator.privacy_level_options.is_empty() || creator.max_video_post_duration_sec == 0 {
        return Err(bad("TikTok is not currently offering publishing options for this creator."));
    }
    Ok(creator)
}

pub(crate) async fn creator(State(app): State<App>, Path(id): Path<String>, Query(q): Query<AssetQuery>) -> ApiResult<Json<Value>> {
    tokio::task::spawn_blocking(move || {
        let path = asset_path(&app, &id, q.use_source)?;
        let (token, account) = direct_tiktok_identity().map_err(bad)?;
        let creator = current_creator(&token)?;
        let asset = open_asset(&path)?;
        let mut result = serde_json::to_value(creator).map_err(internal)?;
        result["account_id"] = json!(account);
        result["duration_ms"] = json!(duration(&path)?);
        result["preview_url"] = json!(format!("/api/oauth/tiktok/direct/preview/{id}?use_source={}&sha256={}", q.use_source, asset.sha));
        result["sha256"] = json!(asset.sha);
        Ok(Json(result))
    }).await.map_err(internal)?
}

pub(crate) async fn preview(State(app): State<App>, Path(id): Path<String>, Query(q): Query<AssetQuery>, req: Request) -> ApiResult<Response> {
    let (path, guard) = tokio::task::spawn_blocking(move || {
        let path = asset_path(&app, &id, q.use_source)?;
        let guard = open_asset(&path)?;
        if q.sha256.as_deref() != Some(&guard.sha) {
            return Err((StatusCode::CONFLICT, "Video changed. Reopen TikTok review.".into()));
        }
        Ok((path, guard))
    }).await.map_err(internal)??;
    let response = ServeFile::new(path).oneshot(req).await.unwrap_or_else(|never| match never {}).into_response();
    let (mut parts, body) = response.into_parts();
    parts.headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    parts.headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    // Keep the reviewed file read-locked until the browser finishes this range.
    let stream = body.into_data_stream().map(move |result| {
        let _pin = &guard;
        result
    });
    Ok(Response::from_parts(parts, Body::from_stream(stream)))
}

#[derive(Copy, Clone)]
struct Chunks { size: u64, count: u64 }

impl Chunks {
    fn for_video(bytes: u64) -> Self {
        let size = if bytes <= 64 * 1024 * 1024 { bytes } else { 16 * 1024 * 1024 };
        Self { size, count: bytes / size }
    }
}

fn initialization(post: &Post, bytes: u64, chunks: Chunks) -> Value {
    json!({
        "post_info": {
            "title": post.title, "privacy_level": post.privacy_level,
            "disable_comment": !post.allow_comment, "disable_duet": !post.allow_duet,
            "disable_stitch": !post.allow_stitch, "brand_organic_toggle": post.brand_organic_toggle,
            "brand_content_toggle": post.brand_content_toggle, "is_aigc": post.is_aigc
        },
        "source_info": {
            "source": "FILE_UPLOAD", "video_size": bytes,
            "chunk_size": chunks.size, "total_chunk_count": chunks.count
        }
    })
}

fn send_chunks(
    reader: &mut impl Read, bytes: u64, chunks: Chunks,
    mut put: impl FnMut(&str, &[u8]) -> Result<(), String>,
) -> Result<(), String> {
    let mut buffer = Vec::new();
    for index in 0..chunks.count {
        let start = index * chunks.size;
        let end = if index + 1 == chunks.count { bytes } else { start + chunks.size };
        buffer.resize((end - start) as usize, 0);
        reader.read_exact(&mut buffer).map_err(|_| "Could not read the selected video.".to_owned())?;
        put(&format!("bytes {start}-{}/{bytes}", end - 1), &buffer)?;
    }
    Ok(())
}

fn view(receipt: Option<receipts::Receipt>) -> Json<Value> {
    let Some(r) = receipt else { return Json(json!({"state":"none", "privacy_level":null})) };
    let state = match r.status.as_str() {
        "PUBLISH_COMPLETE" => "complete",
        "FAILED" => "failed",
        "UNKNOWN" => "needs_review",
        "RESERVED" | "UPLOADING" | "PROCESSING" => "pending",
        _ => "needs_review",
    };
    let message = if state == "complete" && r.privacy_level.as_deref() == Some("PUBLIC_TO_EVERYONE") {
        if r.post_id.is_some() {
            "TikTok confirmed the post is publicly available."
        } else {
            "TikTok confirmed posting for Everyone. Public availability is awaiting confirmation."
        }
    } else {
        &r.detail
    };
    Json(json!({"state":state, "publish_id":r.publish_id, "message":message, "post_id":r.post_id, "privacy_level":r.privacy_level}))
}

fn latest(app: &App, account: &str, sha: &str) -> ApiResult<Option<receipts::Receipt>> {
    let db = app.db.lock().map_err(internal)?;
    receipts::lookup(&db, account, sha).map_err(internal)
}

fn initialize_upload(token: &str, post: &Post, asset: &Asset, chunks: Chunks) -> Result<(String,String,String), String> {
    let data = tiktok_json(token, "/v2/post/publish/video/init/", &initialization(post, asset.bytes, chunks))?;
    let id = data["publish_id"].as_str().filter(|s| !s.is_empty() && s.len() <= 64)
        .ok_or("TikTok did not return a valid publishing ID.")?;
    let url = data["upload_url"].as_str().ok_or("TikTok did not return a file upload URL.")?;
    let (host,path) = parse_tiktok_upload_url(url)?;
    Ok((id.to_owned(),host,path))
}

pub(crate) async fn publish(State(app): State<App>, Path(id): Path<String>, headers: HeaderMap, Json(post): Json<Post>) -> ApiResult<Json<Value>> {
    if !same_origin(&headers) { return Err((StatusCode::FORBIDDEN, "Direct Post requires the local Snapmatic page.".into())); }
    tokio::task::spawn_blocking(move || {
        let (token, account) = direct_tiktok_identity().map_err(bad)?;
        if post.account_id != account { return Err((StatusCode::CONFLICT, "TikTok account changed. Review again.".into())); }
        let path = asset_path(&app, &id, post.use_source)?;
        let mut asset = open_asset(&path)?;
        if asset.sha != post.sha256 { return Err((StatusCode::CONFLICT, "Video changed. Review again.".into())); }
        validate(&post, &current_creator(&token)?, duration(&path)?)?;
        let reservation = {
            let db = app.db.lock().map_err(internal)?;
            receipts::reserve(&db, &account, &asset.sha, &id, &post.privacy_level).map_err(internal)?
        };
        let attempt = match reservation {
            receipts::Reservation::Existing(r) => return Ok(view(Some(r))),
            receipts::Reservation::New(r) => r.attempt,
        };
        let chunks = Chunks::for_video(asset.bytes);
        let (publish_id, host, upload_path) = match initialize_upload(&token, &post, &asset, chunks) {
            Ok(session) => session,
            Err(e) => {
                // FILE_UPLOAD cannot publish until a video is transferred.
                let db = app.db.lock().map_err(internal)?;
                receipts::set_state(&db, &attempt, receipts::State::Failed, &e, None).map_err(internal)?;
                return Ok(view(receipts::lookup(&db, &account, &asset.sha).map_err(internal)?));
            }
        };
        {
            let db = app.db.lock().map_err(internal)?;
            if !receipts::record_id(&db, &attempt, &publish_id).map_err(internal)? {
                return Err((StatusCode::CONFLICT, "The upload attempt changed before transfer. Refresh its status.".into()));
            }
        }
        let mime = asset.mime;
        let uploaded = send_chunks(&mut asset.file, asset.bytes, chunks, |range, chunk| {
            let response = platform::https_request(
                &host, "PUT", &upload_path, &[("Content-Type", mime), ("Content-Range", range)], chunk, 180_000,
            )?;
            if matches!(response.status, 200|201|206) { Ok(()) } else {
                Err(format!("TikTok file transfer returned HTTP {}. Check post status before retrying.", response.status))
            }
        });
        let db = app.db.lock().map_err(internal)?;
        let (state, message) = match uploaded {
            Ok(()) => (receipts::State::Processing, "Video transferred. TikTok is processing the post.".to_owned()),
            Err(message) => (receipts::State::Ambiguous, message),
        };
        receipts::set_state(&db, &attempt, state, &message, None).map_err(internal)?;
        Ok(view(receipts::lookup(&db, &account, &asset.sha).map_err(internal)?))
    }).await.map_err(internal)?
}

fn remote_state(data: &Value) -> (receipts::State, &str, Option<String>) {
    match data["status"].as_str() {
        Some("PUBLISH_COMPLETE") => {
            // TikTok's wire field is intentionally spelled "publicaly".
            let id = data["publicaly_available_post_id"].get(0)
                .and_then(|v| v.as_u64().map(|x| x.to_string()).or_else(|| v.as_str().map(str::to_owned)));
            (receipts::State::Complete, "TikTok confirmed publication.", id)
        }
        Some("FAILED") => (receipts::State::Failed, data["fail_reason"].as_str().unwrap_or("TikTok reported a failed post."), None),
        Some("PROCESSING_UPLOAD") => (receipts::State::Processing, "TikTok is processing the upload.", None),
        _ => (receipts::State::Ambiguous, "TikTok returned an unexpected post status. Check again.", None),
    }
}

pub(crate) async fn status(State(app): State<App>, Path(id): Path<String>, Query(q): Query<AssetQuery>) -> ApiResult<Json<Value>> {
    tokio::task::spawn_blocking(move || {
        if !crate::workspace_media::hex(&id) { return Err(bad("Invalid clip ID.")); }
        let (token, account) = direct_tiktok_identity().map_err(bad)?;
        if q.account_id.is_some_and(|expected| expected != account) {
            return Err((StatusCode::CONFLICT, "TikTok account changed. Review again.".into()));
        }
        let sha = match q.sha256 {
            Some(sha) if crate::workspace_media::hex(&sha) => sha,
            Some(_) => return Err(bad("Invalid video fingerprint.")),
            None => open_asset(&asset_path(&app, &id, q.use_source)?)?.sha,
        };
        {
            let db = app.db.lock().map_err(internal)?;
            receipts::recover_unstarted(&db, &account, &sha).map_err(internal)?;
        }
        let Some(r) = latest(&app, &account, &sha)? else { return Ok(view(None)); };
        if r.status == "FAILED" || (r.status == "PUBLISH_COMPLETE"
            && (r.post_id.is_some() || r.privacy_level.as_deref().is_some_and(|p| p != "PUBLIC_TO_EVERYONE")))
            || r.publish_id.is_none() {
            return Ok(view(Some(r)));
        }
        // Do not submit another post while querying/recovering a previous one.
        let remote = tiktok_json(&token, "/v2/post/publish/status/fetch/", &json!({"publish_id":r.publish_id}));
        if let Ok(data) = remote {
            let (state, message, post_id) = remote_state(&data);
            let db = app.db.lock().map_err(internal)?;
            receipts::set_state(&db, &r.attempt, state, message, post_id.as_deref()).map_err(internal)?;
        } else if !matches!(r.status.as_str(), "PUBLISH_COMPLETE" | "FAILED") {
            let mut result = view(Some(r));
            result.0["message"] = json!("TikTok status is temporarily unavailable. Refresh later.");
            return Ok(result);
        }
        Ok(view(latest(&app, &account, &sha)?))
    }).await.map_err(internal)?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Creator, Post) {
        let creator = Creator {
            creator_username: "owner".into(), creator_nickname: "Creator".into(), creator_avatar_url: "".into(),
            privacy_level_options: vec!["SELF_ONLY".into(), "PUBLIC_TO_EVERYONE".into()],
            comment_disabled: true, duet_disabled: false, stitch_disabled: false, max_video_post_duration_sec: 30,
        };
        let post = Post {
            use_source: false, account_id: "owner-id".into(), sha256: "e".repeat(64),
            title: "The turning point\n\nWatch the minimap 🗺️ #gaming".into(), privacy_level: "SELF_ONLY".into(),
            allow_comment: false, allow_duet: true, allow_stitch: false, disclose_commercial: false,
            brand_organic_toggle: false, brand_content_toggle: false, is_aigc: false, consent: true,
        };
        (creator, post)
    }

    #[test]
    fn text_and_creator_settings_survive_initialization_and_mock_file_transfer() {
        let (creator, post) = fixture();
        validate(&post, &creator, 30_000).unwrap();
        let chunks = Chunks::for_video(7);
        let init = initialization(&post, 7, chunks);
        assert_eq!(init["post_info"]["title"], post.title);
        assert_eq!(init["post_info"]["privacy_level"], "SELF_ONLY");
        assert_eq!(init["post_info"]["disable_duet"], false);
        assert_eq!(init["post_info"]["disable_comment"], true);
        assert_eq!(init["source_info"], json!({"source":"FILE_UPLOAD","video_size":7,"chunk_size":7,"total_chunk_count":1}));
        let mut sent = Vec::new();
        send_chunks(&mut &b"example"[..], 7, chunks, |range, bytes| {
            sent.push((range.to_owned(), bytes.to_vec()));
            Ok(())
        }).unwrap();
        assert_eq!(sent, vec![("bytes 0-6/7".to_owned(), b"example".to_vec())]);
    }

    #[test]
    fn final_chunk_includes_remainder_and_failed_transfer_does_not_resume() {
        let mut seen = Vec::new();
        send_chunks(&mut &b"abcdefg"[..], 7, Chunks{size:3,count:2}, |range, bytes| {
            seen.push((range.to_owned(), bytes.to_vec())); Ok(())
        }).unwrap();
        assert_eq!(seen, vec![("bytes 0-2/7".into(), b"abc".to_vec()), ("bytes 3-6/7".into(), b"defg".to_vec())]);
        let mut calls = 0;
        let result = send_chunks(&mut &b"abcdefg"[..], 7, Chunks{size:3,count:2}, |_,_| {
            calls += 1; Err("connection lost".into())
        });
        assert!(result.is_err());
        assert_eq!(calls, 1);
        let large = Chunks::for_video(64*1024*1024 + 1);
        assert!(large.count > 1 && large.count <= 1000);
    }

    #[test]
    fn text_limit_counts_utf16_and_rejects_incompatible_consent_and_settings() {
        let (creator, mut post) = fixture();
        post.title = "🚀".repeat(1100);
        assert!(validate(&post, &creator, 1).is_ok());
        post.title.push('x');
        assert!(validate(&post, &creator, 1).is_err());
        post.title.clear();
        assert!(validate(&post, &creator, 30_001).is_err());
        post.allow_comment = true;
        assert!(validate(&post, &creator, 1).is_err());
        post.allow_comment = false;
        post.disclose_commercial = true;
        assert!(validate(&post, &creator, 1).is_err());
        post.brand_content_toggle = true;
        assert!(validate(&post, &creator, 1).is_err());
        post.privacy_level = "PUBLIC_TO_EVERYONE".into();
        assert!(validate(&post, &creator, 1).is_ok());
        post.consent = false;
        assert!(validate(&post, &creator, 1).is_err());
    }

    #[test]
    fn origin_and_status_are_explicit() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:3847"));
        assert!(!same_origin(&headers));
        headers.insert(header::ORIGIN, HeaderValue::from_static("http://localhost:3847"));
        assert!(!same_origin(&headers));
        headers.insert(header::ORIGIN, HeaderValue::from_static("http://127.0.0.1:3847"));
        assert!(same_origin(&headers));
        assert!(matches!(remote_state(&json!({"status":"SEND_TO_USER_INBOX"})).0, receipts::State::Ambiguous));
        assert_eq!(remote_state(&json!({"status":"PUBLISH_COMPLETE","publicaly_available_post_id":[9007199254740993u64]})).2, Some("9007199254740993".to_owned()));
    }
}
