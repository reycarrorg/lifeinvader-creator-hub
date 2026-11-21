use crate::{ApiResult, App, bad, internal};
use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Clone)]
struct PendingOAuth {
    provider: String,
    code_verifier: String,
    redirect_uri: String,
    created_at: i64,
}

fn pending_map() -> &'static Mutex<HashMap<String, PendingOAuth>> {
    static MAP: OnceLock<Mutex<HashMap<String, PendingOAuth>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn init(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_oauth_accounts (
            provider TEXT PRIMARY KEY,
            status TEXT NOT NULL DEFAULT 'disconnected',
            account_id TEXT NOT NULL DEFAULT '',
            account_name TEXT NOT NULL DEFAULT '',
            handle TEXT NOT NULL DEFAULT '',
            avatar_url TEXT NOT NULL DEFAULT '',
            subscribers INTEGER,
            media_count INTEGER,
            expires_at INTEGER,
            last_checked_at INTEGER,
            last_error TEXT NOT NULL DEFAULT '',
            extra_json TEXT NOT NULL DEFAULT '{}'
        );
        CREATE TABLE IF NOT EXISTS workspace_channel_posts (
            provider TEXT NOT NULL,
            post_id TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            caption TEXT NOT NULL DEFAULT '',
            permalink TEXT NOT NULL DEFAULT '',
            thumbnail_url TEXT NOT NULL DEFAULT '',
            published_at TEXT NOT NULL DEFAULT '',
            views INTEGER NOT NULL DEFAULT 0,
            likes INTEGER NOT NULL DEFAULT 0,
            comments INTEGER NOT NULL DEFAULT 0,
            media_type TEXT NOT NULL DEFAULT '',
            synced_at INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY (provider, post_id)
        );",
    )?;
    let cols: Vec<String> = {
        let mut stmt = db.prepare("PRAGMA table_info(workspace_export_history)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if !cols.is_empty() && !cols.iter().any(|c| c == "youtube_video_id") {
        let _ = db.execute(
            "ALTER TABLE workspace_export_history ADD COLUMN youtube_video_id TEXT NOT NULL DEFAULT ''",
            [],
        );
    }
    Ok(())
}

fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn base64url(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
        let b2 = if i + 2 < bytes.len() { bytes[i + 2] } else { 0 };
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        }
        if i + 2 < bytes.len() {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        }
        i += 3;
    }
    out
}

fn random_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64url(&digest)
}

fn port_from_headers(headers: &HeaderMap) -> u16 {
    headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.rsplit_once(':'))
        .and_then(|(_, p)| p.parse::<u16>().ok())
        .unwrap_or(3847)
}

fn redirect_uri_for(headers: &HeaderMap, provider: &str) -> String {
    let port = port_from_headers(headers);
    if provider == "instagram" {
        // Meta rejects http://127.0.0.1 with "not a secure connection", but allows http://localhost in Development mode.
        format!("http://localhost:{port}/api/oauth/instagram/callback")
    } else {
        format!("http://127.0.0.1:{port}/api/oauth/{provider}/callback")
    }
}

fn open_browser_url(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", url])
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Could not open browser: {e}"))?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Could not open browser: {e}"))?;
        Ok(())
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Could not open browser: {e}"))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Secure Windows Credential Manager vault + WinHTTP HTTPS client
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod platform {
    use serde_json::Value;
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Networking::WinHttp::*;
    use windows_sys::Win32::Security::Credentials::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub(super) fn vault_read(target: &str) -> Option<Value> {
        let name = wide(target);
        let mut ptr: *mut CREDENTIALW = null_mut();
        if unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut ptr) } == 0 || ptr.is_null() {
            return None;
        }
        let result = unsafe {
            let cred = &*ptr;
            if cred.CredentialBlob.is_null() || cred.CredentialBlobSize == 0 || cred.CredentialBlobSize > 2560 {
                None
            } else {
                let slice = std::slice::from_raw_parts(cred.CredentialBlob, cred.CredentialBlobSize as usize);
                serde_json::from_slice::<Value>(slice).ok()
            }
        };
        unsafe { CredFree(ptr.cast()) };
        result
    }

    pub(super) fn vault_write(target: &str, value: &Value) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        if bytes.is_empty() || bytes.len() > 2560 {
            return Err("Credential payload exceeds Windows Credential Manager limit.".to_owned());
        }
        let mut name = wide(target);
        let cred = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: name.as_mut_ptr(),
            CredentialBlobSize: bytes.len() as u32,
            CredentialBlob: bytes.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        if unsafe { CredWriteW(&cred, 0) } == 0 {
            return Err("Windows Credential Manager refused to store this credential.".to_owned());
        }
        Ok(())
    }

    pub(super) fn vault_delete(target: &str) {
        let name = wide(target);
        unsafe {
            CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0);
        }
    }

    struct WinH(*mut c_void);
    impl WinH {
        fn new(h: *mut c_void) -> Result<Self, String> {
            if h.is_null() {
                Err("Could not initialize Windows HTTPS transport.".to_owned())
            } else {
                Ok(Self(h))
            }
        }
    }
    impl Drop for WinH {
        fn drop(&mut self) {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }

    pub(super) struct HttpResponse {
        pub status: u32,
        pub body: Vec<u8>,
        pub location: Option<String>,
    }

    pub(super) fn https_request(
        host: &str,
        method: &str,
        path_and_query: &str,
        extra_headers: &[(&str, &str)],
        body: &[u8],
        timeout_ms: i32,
    ) -> Result<HttpResponse, String> {
        let session = WinH::new(unsafe {
            WinHttpOpen(
                wide("Snapmatic/1.0").as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                null(),
                null(),
                0,
            )
        })?;
        let _ = unsafe { WinHttpSetTimeouts(session.0, 10_000, 10_000, timeout_ms, timeout_ms) };
        let conn = WinH::new(unsafe { WinHttpConnect(session.0, wide(host).as_ptr(), 443, 0) })?;
        let req = WinH::new(unsafe {
            WinHttpOpenRequest(
                conn.0,
                wide(method).as_ptr(),
                wide(path_and_query).as_ptr(),
                null(),
                null(),
                null(),
                WINHTTP_FLAG_SECURE,
            )
        })?;
        let no_redirect = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
        let _ = unsafe {
            WinHttpSetOption(
                req.0,
                WINHTTP_OPTION_REDIRECT_POLICY,
                (&no_redirect as *const u32).cast(),
                4,
            )
        };
        let mut header_block = String::from("Accept: application/json\r\n");
        for (k, v) in extra_headers {
            header_block.push_str(k);
            header_block.push_str(": ");
            header_block.push_str(v);
            header_block.push_str("\r\n");
        }
        let hdr_w = wide(&header_block);
        let data_ptr = if body.is_empty() { null() } else { body.as_ptr().cast() };
        let data_len = u32::try_from(body.len()).map_err(|_| "Request body too large.".to_owned())?;
        if unsafe {
            WinHttpSendRequest(
                req.0,
                hdr_w.as_ptr(),
                (hdr_w.len() - 1) as u32,
                data_ptr,
                data_len,
                data_len,
                0,
            )
        } == 0
        {
            return Err(format!("HTTPS request to {host} failed."));
        }
        if unsafe { WinHttpReceiveResponse(req.0, null_mut()) } == 0 {
            return Err(format!("No HTTPS response from {host}."));
        }
        let mut status = 0u32;
        let mut status_len = 4u32;
        if unsafe {
            WinHttpQueryHeaders(
                req.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                (&mut status as *mut u32).cast(),
                &mut status_len,
                null_mut(),
            )
        } == 0
        {
            return Err("Could not read HTTPS status code.".to_owned());
        }
        let mut loc_len = 0u32;
        let _ = unsafe {
            WinHttpQueryHeaders(
                req.0,
                WINHTTP_QUERY_LOCATION,
                null(),
                null_mut(),
                &mut loc_len,
                null_mut(),
            )
        };
        let location = if loc_len > 2 && loc_len <= 16384 {
            let mut buf = vec![0u16; (loc_len as usize) / 2 + 1];
            let mut actual = loc_len;
            if unsafe {
                WinHttpQueryHeaders(
                    req.0,
                    WINHTTP_QUERY_LOCATION,
                    null(),
                    buf.as_mut_ptr().cast(),
                    &mut actual,
                    null_mut(),
                )
            } != 0
            {
                let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                Some(String::from_utf16_lossy(&buf[..end]))
            } else {
                None
            }
        } else {
            None
        };
        let mut out = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let mut n = 0u32;
            if unsafe { WinHttpReadData(req.0, chunk.as_mut_ptr().cast(), chunk.len() as u32, &mut n) } == 0 {
                return Err("Failed while reading HTTPS response body.".to_owned());
            }
            if n == 0 {
                break;
            }
            if out.len() + (n as usize) > 1024 * 1024 {
                return Err("HTTPS response exceeded 1 MiB safety limit.".to_owned());
            }
            out.extend_from_slice(&chunk[..n as usize]);
        }
        Ok(HttpResponse { status, body: out, location })
    }
}

#[cfg(not(windows))]
mod platform {
    use serde_json::Value;
    pub(super) struct HttpResponse {
        pub status: u32,
        pub body: Vec<u8>,
        pub location: Option<String>,
    }
    pub(super) fn vault_read(_target: &str) -> Option<Value> {
        None
    }
    pub(super) fn vault_write(_target: &str, _value: &Value) -> Result<(), String> {
        Err("Native credential storage is only enabled on Windows builds.".to_owned())
    }
    pub(super) fn vault_delete(_target: &str) {}
    pub(super) fn https_request(
        _host: &str,
        _method: &str,
        _path_and_query: &str,
        _extra_headers: &[(&str, &str)],
        _body: &[u8],
        _timeout_ms: i32,
    ) -> Result<HttpResponse, String> {
        Err("Native HTTPS OAuth transport is only enabled on Windows builds.".to_owned())
    }
}

fn client_target(provider: &str) -> Option<&'static str> {
    match provider {
        "youtube" => Some("Snapmatic/OAuth/YouTube/Client"),
        "instagram" => Some("Snapmatic/OAuth/Instagram/Client"),
        _ => None,
    }
}

fn token_target(provider: &str) -> Option<&'static str> {
    match provider {
        "youtube" => Some("Snapmatic/OAuth/YouTube/Tokens"),
        "instagram" => Some("Snapmatic/OAuth/Instagram/Tokens"),
        _ => None,
    }
}

fn save_account_row(
    app: &App,
    provider: &str,
    status: &str,
    account_id: &str,
    account_name: &str,
    handle: &str,
    avatar_url: &str,
    subscribers: Option<i64>,
    media_count: Option<i64>,
    expires_at: Option<i64>,
    last_error: &str,
) -> Result<(), String> {
    let db = app.db.lock().map_err(|_| "Database unavailable".to_owned())?;
    db.execute(
        "INSERT INTO workspace_oauth_accounts(
            provider, status, account_id, account_name, handle, avatar_url,
            subscribers, media_count, expires_at, last_checked_at, last_error
         ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(provider) DO UPDATE SET
            status=excluded.status,
            account_id=excluded.account_id,
            account_name=excluded.account_name,
            handle=excluded.handle,
            avatar_url=excluded.avatar_url,
            subscribers=excluded.subscribers,
            media_count=excluded.media_count,
            expires_at=excluded.expires_at,
            last_checked_at=excluded.last_checked_at,
            last_error=excluded.last_error",
        params![
            provider,
            status,
            account_id,
            account_name,
            handle,
            avatar_url,
            subscribers,
            media_count,
            expires_at,
            now_unix(),
            last_error,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn ensure_destination_profile(app: &App, platform_name: &str, name: &str, url: &str) {
    if name.trim().is_empty() || url.trim().is_empty() {
        return;
    }
    if let Ok(db) = app.db.lock() {
        let _ = db.execute(
            "INSERT OR IGNORE INTO profiles(name, platform, url, role, tags) VALUES(?1, ?2, ?3, 'destination', '[]')",
            params![name.trim(), platform_name, url.trim()],
        );
    }
}

// ---------------------------------------------------------------------------
// API Request Payloads
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct ConfigInput {
    #[serde(default)]
    client_json: Option<String>,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    client_secret: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct DisconnectInput {
    #[serde(default)]
    clear_config: bool,
}

#[derive(Deserialize)]
pub(crate) struct CallbackQuery {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct YoutubeDraftUploadInput {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tags: Option<String>,
    #[serde(default)]
    privacy: Option<String>,
    #[serde(default)]
    publish_at: Option<String>,
    #[serde(default)]
    thumbnail_png_base64: Option<String>,
    #[serde(default)]
    use_source: bool,
}

#[derive(Deserialize)]
pub(crate) struct InstagramTokenInput {
    access_token: String,
}

#[derive(Deserialize)]
pub(crate) struct InstagramReelUploadInput {
    #[serde(default)]
    caption: Option<String>,
    #[serde(default = "default_true")]
    share_to_feed: bool,
    #[serde(default)]
    thumb_offset_ms: Option<u64>,
    #[serde(default)]
    use_source: bool,
    #[serde(default)]
    publish: bool,
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Route Handlers
// ---------------------------------------------------------------------------

pub(crate) async fn status(State(app): State<App>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let mut rows: HashMap<String, Value> = HashMap::new();
    {
        let db = app.db.lock().map_err(internal)?;
        let mut stmt = db
            .prepare(
                "SELECT provider, status, account_id, account_name, handle, avatar_url,
                        subscribers, media_count, expires_at, last_checked_at, last_error
                 FROM workspace_oauth_accounts",
            )
            .map_err(internal)?;
        let mapped = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    json!({
                        "status": r.get::<_, String>(1)?,
                        "account_id": r.get::<_, String>(2)?,
                        "account_name": r.get::<_, String>(3)?,
                        "handle": r.get::<_, String>(4)?,
                        "avatar_url": r.get::<_, String>(5)?,
                        "subscribers": r.get::<_, Option<i64>>(6)?,
                        "media_count": r.get::<_, Option<i64>>(7)?,
                        "expires_at": r.get::<_, Option<i64>>(8)?,
                        "last_checked_at": r.get::<_, Option<i64>>(9)?,
                        "last_error": r.get::<_, String>(10)?,
                    }),
                ))
            })
            .map_err(internal)?;
        for (k, v) in mapped.flatten() {
            rows.insert(k, v);
        }
    }

    let yt_client = platform::vault_read(client_target("youtube").unwrap());
    let yt_tokens = platform::vault_read(token_target("youtube").unwrap());
    let ig_client = platform::vault_read(client_target("instagram").unwrap());
    let ig_tokens = platform::vault_read(token_target("instagram").unwrap());

    let build_provider = |id: &str, client: Option<Value>, tokens: Option<Value>| -> Value {
        let meta = rows.get(id);
        let has_client = client.is_some();
        let has_tokens = tokens.is_some();
        let db_status = meta
            .and_then(|m| m["status"].as_str())
            .unwrap_or("disconnected");
        let connected = has_tokens && db_status == "connected";
        let client_id_preview = client
            .as_ref()
            .and_then(|c| c["client_id"].as_str())
            .map(|s| {
                if s.len() > 20 {
                    format!("{}…{}", &s[..10], &s[s.len() - 8..])
                } else {
                    s.to_owned()
                }
            });
        json!({
            "provider": id,
            "client_configured": has_client,
            "client_id_preview": client_id_preview,
            "connected": connected,
            "status": if connected { "connected" } else if has_client { "configured" } else { "unconfigured" },
            "account_id": meta.and_then(|m| m["account_id"].as_str()).unwrap_or(""),
            "account_name": meta.and_then(|m| m["account_name"].as_str()).unwrap_or(""),
            "handle": meta.and_then(|m| m["handle"].as_str()).unwrap_or(""),
            "subscribers": meta.and_then(|m| m["subscribers"].as_i64()),
            "media_count": meta.and_then(|m| m["media_count"].as_i64()),
            "expires_at": meta.and_then(|m| m["expires_at"].as_i64()),
            "last_checked_at": meta.and_then(|m| m["last_checked_at"].as_i64()),
            "last_error": meta.and_then(|m| m["last_error"].as_str()).unwrap_or(""),
            "redirect_uri": redirect_uri_for(&headers, id),
        })
    };

    Ok(Json(json!({
        "youtube": build_provider("youtube", yt_client, yt_tokens),
        "instagram": build_provider("instagram", ig_client, ig_tokens),
        "tiktok": {
            "provider": "tiktok",
            "status": "manual_export",
            "connected": false,
            "note": "TikTok uses local MP4 + caption pack export (public auto-posting disabled)."
        },
        "public_posting_enabled": false
    })))
}

pub(crate) async fn save_config(
    AxumPath(provider): AxumPath<String>,
    Json(input): Json<ConfigInput>,
) -> ApiResult<Json<Value>> {
    let Some(target) = client_target(&provider) else {
        return Err(bad("Choose youtube or instagram."));
    };
    let (client_id, client_secret) = if let Some(raw) = input
        .client_json
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let parsed: Value = serde_json::from_str(raw.strip_prefix('\u{feff}').unwrap_or(raw))
            .map_err(|_| bad("Invalid OAuth client JSON."))?;
        let node = parsed
            .get("installed")
            .or_else(|| parsed.get("web"))
            .unwrap_or(&parsed);
        let id = node["client_id"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| bad("OAuth JSON is missing client_id."))?
            .to_owned();
        let secret = node["client_secret"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| bad("OAuth JSON is missing client_secret."))?
            .to_owned();
        (id, secret)
    } else {
        let id = input
            .client_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| bad("Client ID / App ID is required."))?
            .to_owned();
        let secret = input
            .client_secret
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| bad("Client Secret / App Secret is required."))?
            .to_owned();
        (id, secret)
    };

    if provider == "youtube" && !client_id.ends_with(".apps.googleusercontent.com") {
        return Err(bad(
            "YouTube Client ID should end with .apps.googleusercontent.com (from Google Cloud OAuth credentials).",
        ));
    }
    if client_id.len() > 256 || client_secret.len() > 256 {
        return Err(bad("Client ID or secret is unexpectedly long."));
    }

    platform::vault_write(
        target,
        &json!({ "client_id": client_id, "client_secret": client_secret }),
    )
    .map_err(bad)?;

    Ok(Json(json!({
        "provider": provider,
        "client_configured": true
    })))
}

pub(crate) async fn start_oauth(
    AxumPath(provider): AxumPath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let Some(target) = client_target(&provider) else {
        return Err(bad("Choose youtube or instagram."));
    };
    let client = platform::vault_read(target)
        .ok_or_else(|| bad("Save your OAuth Client ID and Secret first."))?;
    let client_id = client["client_id"]
        .as_str()
        .ok_or_else(|| bad("Saved OAuth client is incomplete."))?;

    let state = random_token();
    let verifier = random_token();
    let challenge = pkce_challenge(&verifier);
    let redirect_uri = redirect_uri_for(&headers, &provider);

    {
        let mut map = pending_map().lock().map_err(internal)?;
        let cutoff = now_unix() - 600;
        map.retain(|_, v| v.created_at >= cutoff);
        map.insert(
            state.clone(),
            PendingOAuth {
                provider: provider.clone(),
                code_verifier: verifier,
                redirect_uri: redirect_uri.clone(),
                created_at: now_unix(),
            },
        );
    }

    let auth_url = match provider.as_str() {
        "youtube" => format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&state={}&code_challenge={}&code_challenge_method=S256",
            url_encode(client_id),
            url_encode(&redirect_uri),
            url_encode("https://www.googleapis.com/auth/youtube.readonly https://www.googleapis.com/auth/youtube.upload"),
            url_encode(&state),
            url_encode(&challenge),
        ),
        "instagram" => format!(
            "https://www.facebook.com/v21.0/dialog/oauth?client_id={}&redirect_uri={}&response_type=code&auth_type=rerequest&scope={}&state={}",
            url_encode(client_id),
            url_encode(&redirect_uri),
            url_encode("instagram_basic,pages_show_list,pages_read_engagement,instagram_content_publish,business_management"),
            url_encode(&state),
        ),
        _ => return Err(bad("Unsupported provider.")),
    };

    let launched = open_browser_url(&auth_url).is_ok();
    Ok(Json(json!({
        "provider": provider,
        "auth_url": auth_url,
        "browser_opened": launched,
        "redirect_uri": redirect_uri
    })))
}

fn callback_html(ok: bool, title: &str, detail: &str) -> Html<String> {
    let color = if ok { "#29B6F6" } else { "#FF5252" };
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    Html(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Snapmatic OAuth</title>
<style>body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#0B0D10;color:#F3F5F8;font:15px/1.5 system-ui,sans-serif}}
.card{{max-width:460px;padding:28px;border-radius:12px;background:#13171D;border:1px solid #1E242D;text-align:center}}
h1{{margin:0 0 10px;color:{color};font-size:22px}}p{{color:#9aa3b2;margin:0 0 18px}}
a{{display:inline-block;padding:9px 16px;border-radius:7px;border:1px solid #29B6F6;color:#F3F5F8;text-decoration:none}}</style></head>
<body><div class="card"><h1>{}</h1><p>{}</p><a id="ret" href="/workspace?view=system">Return to Snapmatic</a></div>
<script>try{{var r=document.getElementById('ret');if(location.hostname==='localhost')r.href='http://127.0.0.1:'+location.port+'/workspace?view=system';window.opener&&window.opener.postMessage({{type:'snapmatic:oauth-updated'}},'*');}}catch{{}}</script></body></html>"#,
        esc(title),
        esc(detail)
    ))
}

fn fetch_youtube_channel(access_token: &str) -> Result<(String, String, String, Option<i64>, Option<i64>), String> {
    let auth = format!("Bearer {access_token}");
    let resp = platform::https_request(
        "www.googleapis.com",
        "GET",
        "/youtube/v3/channels?part=snippet,statistics&mine=true",
        &[("Authorization", &auth)],
        &[],
        15_000,
    )?;
    let parsed: Value = serde_json::from_slice(&resp.body)
        .map_err(|_| "Unreadable response from YouTube API.".to_owned())?;
    if resp.status != 200 {
        let msg = parsed["error"]["message"]
            .as_str()
            .unwrap_or("YouTube API rejected the request.");
        return Err(msg.to_owned());
    }
    let ch = parsed["items"]
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| "No YouTube channel found on this Google account.".to_owned())?;
    let id = ch["id"].as_str().unwrap_or("").to_owned();
    let title = ch["snippet"]["title"].as_str().unwrap_or("YouTube Channel").to_owned();
    let handle = ch["snippet"]["customUrl"].as_str().unwrap_or("").to_owned();
    let subs = ch["statistics"]["subscriberCount"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok());
    let vids = ch["statistics"]["videoCount"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok());
    Ok((id, title, handle, subs, vids))
}

fn refresh_youtube_access_token() -> Result<(String, i64), String> {
    let client = platform::vault_read(client_target("youtube").unwrap())
        .ok_or_else(|| "YouTube OAuth client not configured.".to_owned())?;
    let tokens = platform::vault_read(token_target("youtube").unwrap())
        .ok_or_else(|| "YouTube is not connected.".to_owned())?;
    let expires_at = tokens["expires_at"].as_i64().unwrap_or(0);
    if expires_at > now_unix() + 120
        && let Some(tok) = tokens["access_token"].as_str().filter(|s| !s.is_empty())
    {
        return Ok((tok.to_owned(), expires_at));
    }
    let refresh = tokens["refresh_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Missing YouTube refresh token. Please reconnect YouTube.".to_owned())?;
    let cid = client["client_id"].as_str().unwrap_or("");
    let csec = client["client_secret"].as_str().unwrap_or("");
    let body = format!(
        "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
        url_encode(cid),
        url_encode(csec),
        url_encode(refresh),
    );
    let resp = platform::https_request(
        "oauth2.googleapis.com",
        "POST",
        "/token",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        body.as_bytes(),
        15_000,
    )?;
    let parsed: Value = serde_json::from_slice(&resp.body)
        .map_err(|_| "Unreadable response from Google OAuth token endpoint.".to_owned())?;
    if resp.status != 200 {
        let msg = parsed["error_description"]
            .as_str()
            .or_else(|| parsed["error"].as_str())
            .unwrap_or("Google rejected the token refresh.");
        return Err(msg.to_owned());
    }
    let new_access = parsed["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Google token refresh did not return an access_token.".to_owned())?
        .to_owned();
    let exp_in = parsed["expires_in"].as_i64().unwrap_or(3600);
    let new_exp = now_unix() + exp_in;
    platform::vault_write(
        token_target("youtube").unwrap(),
        &json!({
            "access_token": new_access,
            "refresh_token": refresh,
            "expires_at": new_exp,
        }),
    )?;
    Ok((new_access, new_exp))
}

fn parse_ig_node(ig: &Value) -> Option<(String, String, String, Option<i64>, Option<i64>)> {
    let id = ig["id"].as_str().filter(|s| !s.is_empty())?;
    let username = ig["username"].as_str().unwrap_or("").to_owned();
    let name = ig["name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(if username.is_empty() { "Instagram Account" } else { &username })
        .to_owned();
    let handle = if username.is_empty() {
        String::new()
    } else {
        format!("@{username}")
    };
    let followers = ig["followers_count"].as_i64();
    let media = ig["media_count"].as_i64();
    Some((id.to_owned(), name, handle, followers, media))
}

fn fetch_instagram_business_account(access_token: &str) -> Result<(String, String, String, Option<i64>, Option<i64>), String> {
    let path = format!(
        "/v21.0/me/accounts?fields=id,name,instagram_business_account%7Bid,username,name,followers_count,media_count%7D,connected_instagram_account%7Bid,username,name,followers_count,media_count%7D&access_token={}",
        url_encode(access_token)
    );
    let resp = platform::https_request("graph.facebook.com", "GET", &path, &[], &[], 15_000)?;
    let parsed: Value = serde_json::from_slice(&resp.body)
        .map_err(|_| "Unreadable response from Meta Graph API.".to_owned())?;
    if resp.status != 200 {
        let msg = parsed["error"]["message"]
            .as_str()
            .unwrap_or("Meta Graph API rejected the request.");
        return Err(msg.to_owned());
    }
    let pages = parsed["data"].as_array().cloned().unwrap_or_default();
    let mut page_names: Vec<String> = Vec::new();
    for page in &pages {
        if let Some(pname) = page["name"].as_str().filter(|s| !s.is_empty()) {
            page_names.push(pname.to_owned());
        }
        if let Some(found) = page
            .get("instagram_business_account")
            .and_then(parse_ig_node)
            .or_else(|| page.get("connected_instagram_account").and_then(parse_ig_node))
        {
            return Ok(found);
        }
    }

    // Fallback: check Meta Business Portfolios (owned_pages / owned_instagram_accounts / instagram_accounts)
    let biz_path = format!(
        "/v21.0/me/businesses?fields=id,name,owned_pages%7Bid,name,instagram_business_account%7Bid,username,name,followers_count,media_count%7D,connected_instagram_account%7Bid,username,name,followers_count,media_count%7D%7D,owned_instagram_accounts%7Bid,username,name,followers_count,media_count%7D,instagram_accounts%7Bid,username,name,followers_count,media_count%7D&access_token={}",
        url_encode(access_token)
    );
    if let Ok(bresp) = platform::https_request("graph.facebook.com", "GET", &biz_path, &[], &[], 15_000)
        && bresp.status == 200
        && let Ok(bparsed) = serde_json::from_slice::<Value>(&bresp.body)
        && let Some(businesses) = bparsed["data"].as_array()
    {
        for biz in businesses {
            if let Some(opages) = biz["owned_pages"]["data"].as_array() {
                for page in opages {
                    if let Some(pname) = page["name"].as_str().filter(|s| !s.is_empty())
                        && !page_names.iter().any(|x| x == pname)
                    {
                        page_names.push(pname.to_owned());
                    }
                    if let Some(found) = page
                        .get("instagram_business_account")
                        .and_then(parse_ig_node)
                        .or_else(|| page.get("connected_instagram_account").and_then(parse_ig_node))
                    {
                        return Ok(found);
                    }
                }
            }
            for key in ["owned_instagram_accounts", "instagram_accounts"] {
                if let Some(igs) = biz[key]["data"].as_array() {
                    for ig in igs {
                        if let Some(found) = parse_ig_node(ig) {
                            return Ok(found);
                        }
                    }
                }
            }
        }
    }

    if page_names.is_empty() {
        Err("Facebook login succeeded, but Meta returned 0 Facebook Pages. Make sure you check your Facebook Page (e.g. CJ 4 Prez) and Business Portfolio on the Facebook permission screen.".to_owned())
    } else {
        Err(format!(
            "Facebook login succeeded and found Page(s): {}, but none returned a linked Instagram account. Make sure you check both your Facebook Page and Instagram account on the Facebook permission screen.",
            page_names.join(", ")
        ))
    }
}

pub(crate) async fn callback(
    State(app): State<App>,
    AxumPath(provider): AxumPath<String>,
    Query(q): Query<CallbackQuery>,
) -> impl IntoResponse {
    if let Some(err) = q.error {
        let desc = q.error_description.unwrap_or(err);
        return callback_html(false, "Connection Cancelled", &desc);
    }
    let (Some(code), Some(state)) = (q.code, q.state) else {
        return callback_html(false, "Missing OAuth Parameters", "Authorization code or state was missing.");
    };
    let pending = {
        let Ok(mut map) = pending_map().lock() else {
            return callback_html(false, "Internal Error", "OAuth session state unavailable.");
        };
        map.remove(&state)
    };
    let Some(pending) = pending else {
        return callback_html(false, "Expired or Invalid Session", "Please click Connect again inside Snapmatic.");
    };
    if pending.provider != provider || now_unix() - pending.created_at > 600 {
        return callback_html(false, "Session Expired", "Please restart the connection inside Snapmatic.");
    }
    let Some(c_target) = client_target(&provider) else {
        return callback_html(false, "Unsupported Provider", "Unknown provider.");
    };
    let Some(t_target) = token_target(&provider) else {
        return callback_html(false, "Unsupported Provider", "Unknown provider.");
    };
    let Some(client) = platform::vault_read(c_target) else {
        return callback_html(false, "Missing Client Config", "Save your OAuth Client ID and Secret first.");
    };
    let client_id = client["client_id"].as_str().unwrap_or("").to_owned();
    let client_secret = client["client_secret"].as_str().unwrap_or("").to_owned();

    let app_clone = app.clone();
    let provider_clone = provider.clone();
    let res = tokio::task::spawn_blocking(move || -> Result<String, String> {
        if provider_clone == "youtube" {
            let body = format!(
                "code={}&client_id={}&client_secret={}&redirect_uri={}&grant_type=authorization_code&code_verifier={}",
                url_encode(&code),
                url_encode(&client_id),
                url_encode(&client_secret),
                url_encode(&pending.redirect_uri),
                url_encode(&pending.code_verifier),
            );
            let resp = platform::https_request(
                "oauth2.googleapis.com",
                "POST",
                "/token",
                &[("Content-Type", "application/x-www-form-urlencoded")],
                body.as_bytes(),
                15_000,
            )?;
            let parsed: Value = serde_json::from_slice(&resp.body)
                .map_err(|_| "Unreadable response from Google OAuth.".to_owned())?;
            if resp.status != 200 {
                let msg = parsed["error_description"]
                    .as_str()
                    .or_else(|| parsed["error"].as_str())
                    .unwrap_or("Google rejected the authorization code.");
                return Err(msg.to_owned());
            }
            let access = parsed["access_token"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "Missing access_token from Google.".to_owned())?
                .to_owned();
            let refresh = parsed["refresh_token"]
                .as_str()
                .unwrap_or("")
                .to_owned();
            let exp_at = now_unix() + parsed["expires_in"].as_i64().unwrap_or(3600);
            platform::vault_write(
                t_target,
                &json!({
                    "access_token": access,
                    "refresh_token": refresh,
                    "expires_at": exp_at,
                }),
            )?;
            let (ch_id, ch_name, ch_handle, subs, vids) = fetch_youtube_channel(&access)?;
            save_account_row(
                &app_clone,
                "youtube",
                "connected",
                &ch_id,
                &ch_name,
                &ch_handle,
                "",
                subs,
                vids,
                Some(exp_at),
                "",
            )?;
            let ch_url = if ch_handle.starts_with('@') {
                format!("https://www.youtube.com/{ch_handle}")
            } else {
                format!("https://www.youtube.com/channel/{ch_id}")
            };
            ensure_destination_profile(&app_clone, "youtube", &ch_name, &ch_url);
            Ok(format!("Connected YouTube channel {ch_name} ({ch_handle})."))
        } else {
            let path = format!(
                "/v21.0/oauth/access_token?client_id={}&redirect_uri={}&client_secret={}&code={}",
                url_encode(&client_id),
                url_encode(&pending.redirect_uri),
                url_encode(&client_secret),
                url_encode(&code),
            );
            let resp = platform::https_request("graph.facebook.com", "GET", &path, &[], &[], 15_000)?;
            let parsed: Value = serde_json::from_slice(&resp.body)
                .map_err(|_| "Unreadable response from Meta OAuth.".to_owned())?;
            if resp.status != 200 {
                let msg = parsed["error"]["message"]
                    .as_str()
                    .unwrap_or("Meta rejected the authorization code.");
                return Err(msg.to_owned());
            }
            let short_tok = parsed["access_token"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "Missing access_token from Meta.".to_owned())?
                .to_owned();
            // Exchange for 60-day long-lived token
            let ll_path = format!(
                "/v21.0/oauth/access_token?grant_type=fb_exchange_token&client_id={}&client_secret={}&fb_exchange_token={}",
                url_encode(&client_id),
                url_encode(&client_secret),
                url_encode(&short_tok),
            );
            let (final_tok, exp_at) = match platform::https_request("graph.facebook.com", "GET", &ll_path, &[], &[], 15_000) {
                Ok(r) if r.status == 200 => {
                    let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
                    let tok = v["access_token"].as_str().unwrap_or(&short_tok).to_owned();
                    let exp = now_unix() + v["expires_in"].as_i64().unwrap_or(5_184_000);
                    (tok, exp)
                }
                _ => (short_tok, now_unix() + 3600),
            };
            platform::vault_write(
                t_target,
                &json!({ "access_token": final_tok, "expires_at": exp_at }),
            )?;
            let (ig_id, ig_name, ig_handle, followers, media) = fetch_instagram_business_account(&final_tok)?;
            save_account_row(
                &app_clone,
                "instagram",
                "connected",
                &ig_id,
                &ig_name,
                &ig_handle,
                "",
                followers,
                media,
                Some(exp_at),
                "",
            )?;
            let ig_url = format!("https://www.instagram.com/{}/", ig_handle.trim_start_matches('@'));
            ensure_destination_profile(&app_clone, "instagram", &ig_name, &ig_url);
            Ok(format!("Connected Instagram account {ig_name} ({ig_handle})."))
        }
    })
    .await;

    match res {
        Ok(Ok(msg)) => callback_html(true, "Account Connected!", &format!("{msg} You can close this tab and return to Snapmatic.")),
        Ok(Err(err)) => {
            let _ = save_account_row(&app, &provider, "error", "", "", "", "", None, None, None, &err);
            callback_html(false, "Connection Failed", &err)
        }
        Err(_) => callback_html(false, "Connection Failed", "Background OAuth exchange failed."),
    }
}

pub(crate) async fn refresh(
    State(app): State<App>,
    AxumPath(provider): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    if !matches!(provider.as_str(), "youtube" | "instagram") {
        return Err(bad("Choose youtube or instagram."));
    }
    let app_clone = app.clone();
    let prov = provider.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        if prov == "youtube" {
            let (tok, exp) = refresh_youtube_access_token()?;
            let (id, name, handle, subs, vids) = fetch_youtube_channel(&tok)?;
            save_account_row(&app_clone, "youtube", "connected", &id, &name, &handle, "", subs, vids, Some(exp), "")?;
            Ok(json!({ "provider": "youtube", "connected": true, "account_name": name, "handle": handle, "subscribers": subs, "media_count": vids, "expires_at": exp }))
        } else {
            let tokens = platform::vault_read(token_target("instagram").unwrap())
                .ok_or_else(|| "Instagram is not connected.".to_owned())?;
            let tok = tokens["access_token"].as_str().ok_or_else(|| "Missing Instagram token.".to_owned())?;
            let exp = tokens["expires_at"].as_i64();
            let (id, name, handle, followers, media) = fetch_instagram_business_account(tok)?;
            save_account_row(&app_clone, "instagram", "connected", &id, &name, &handle, "", followers, media, exp, "")?;
            Ok(json!({ "provider": "instagram", "connected": true, "account_name": name, "handle": handle, "subscribers": followers, "media_count": media, "expires_at": exp }))
        }
    })
    .await
    .map_err(internal)?;

    match result {
        Ok(v) => Ok(Json(v)),
        Err(msg) => {
            let _ = save_account_row(&app, &provider, "error", "", "", "", "", None, None, None, &msg);
            Err(bad(msg))
        }
    }
}

pub(crate) async fn disconnect(
    State(app): State<App>,
    AxumPath(provider): AxumPath<String>,
    Json(input): Json<DisconnectInput>,
) -> ApiResult<Json<Value>> {
    let Some(t_target) = token_target(&provider) else {
        return Err(bad("Choose youtube or instagram."));
    };
    platform::vault_delete(t_target);
    if input.clear_config
        && let Some(c_target) = client_target(&provider)
    {
        platform::vault_delete(c_target);
    }
    let db = app.db.lock().map_err(internal)?;
    db.execute("DELETE FROM workspace_oauth_accounts WHERE provider=?1", [&provider])
        .map_err(internal)?;
    Ok(Json(json!({ "provider": provider, "disconnected": true })))
}

fn sync_posting_queue_url(app: &App, clip_id: &str, platform_name: &str, post_url: &str) {
    let Some((source_id, source_path)) = crate::workspace_film_roll::resolve_source_ref(app, clip_id) else {
        return;
    };
    if let Ok(db) = app.db.lock() {
        let _ = db.execute(
            "UPDATE posting_queue
             SET state='posted', post_url=?1, updated_at=datetime('now')
             WHERE source_id=?2 AND path=?3
               AND profile_id IN (SELECT id FROM profiles WHERE role='destination' AND platform=?4)",
            params![post_url, source_id, source_path, platform_name],
        );
    }
}

fn record_analytics_snapshot(
    app: &App,
    platform_name: &str,
    followers: Option<i64>,
    views: Option<i64>,
    likes: Option<i64>,
    comments: Option<i64>,
) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let now_str = chrono::Utc::now().to_rfc3339();
    if let Ok(db) = app.db.lock()
        && let Ok(Some(pid)) = db
            .query_row(
                "SELECT id FROM profiles WHERE role='destination' AND archived=0 AND platform=?1 ORDER BY id LIMIT 1",
                [platform_name],
                |r| r.get::<_, i64>(0),
            )
            .optional()
    {
        let _ = db.execute(
            "INSERT INTO analytics_snapshots(profile_id, date, post_id, followers, views, likes, comments, source, updated_at)
             VALUES(?1, ?2, '', ?3, ?4, ?5, ?6, 'oauth_api', ?7)
             ON CONFLICT(profile_id, date, post_id) DO UPDATE SET
               followers=COALESCE(excluded.followers, analytics_snapshots.followers),
               views=COALESCE(excluded.views, analytics_snapshots.views),
               likes=COALESCE(excluded.likes, analytics_snapshots.likes),
               comments=COALESCE(excluded.comments, analytics_snapshots.comments),
               source='oauth_api',
               updated_at=excluded.updated_at",
            params![pid, today, followers, views, likes, comments, now_str],
        );
    }
}

fn resolve_upload_file(app: &App, clip_id: &str, use_source: bool) -> ApiResult<(std::path::PathBuf, String)> {
    let db = app.db.lock().map_err(internal)?;
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT title, exported_path FROM workspace_film_roll WHERE clip_id=?1",
            [clip_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(internal)?;
    let Some((title, exported_path)) = row else {
        return Err((StatusCode::NOT_FOUND, "Clip not found in Film Roll.".into()));
    };
    drop(db);
    let path = if !use_source
        && !exported_path.is_empty()
        && !exported_path.contains(['/', '\\', ':', '\0'])
        && !exported_path.starts_with('.')
    {
        let p = crate::workspace_film_roll::exports_dir(app).join(&exported_path);
        if p.is_file() {
            p
        } else {
            crate::workspace_film_roll::resolve_video(app, clip_id)
                .ok_or_else(|| (StatusCode::NOT_FOUND, "Video file unavailable.".into()))?
        }
    } else {
        crate::workspace_film_roll::resolve_video(app, clip_id)
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Video file unavailable.".into()))?
    };
    Ok((path, title))
}

pub(crate) async fn upload_youtube_private_draft(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
    Json(input): Json<YoutubeDraftUploadInput>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let (file_path, fallback_title) = resolve_upload_file(&app, &clip_id, input.use_source)?;
    let meta = std::fs::metadata(&file_path).map_err(internal)?;
    if meta.len() == 0 || meta.len() > 250 * 1024 * 1024 {
        return Err(bad("Video must be between 1 byte and 250 MiB for draft upload."));
    }

    let publish_at = input
        .publish_at
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.to_rfc3339())
                .map_err(|_| bad("Scheduled publish time must be a valid ISO timestamp."))
        })
        .transpose()?;

    // Server-enforced PRIVATE or UNLISTED only. Scheduled publish requires privacyStatus="private".
    let privacy = if publish_at.is_some() {
        "private".to_owned()
    } else {
        match input.privacy.as_deref() {
            Some("unlisted") => "unlisted".to_owned(),
            _ => "private".to_owned(),
        }
    };

    let thumb_bytes = input
        .thumbnail_png_base64
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(crate::workspace_film_roll::decode_base64)
        .filter(|b| b.len() >= 8 && b.len() <= 2 * 1024 * 1024 && b.starts_with(b"\x89PNG\r\n\x1a\n"));

    let title = input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(if fallback_title.is_empty() { "Snapmatic Draft" } else { &fallback_title })
        .chars()
        .take(100)
        .collect::<String>();
    let description = input
        .description
        .as_deref()
        .unwrap_or("")
        .chars()
        .take(4900)
        .collect::<String>();
    let tags: Vec<String> = input
        .tags
        .as_deref()
        .unwrap_or("")
        .split([',', ' '])
        .map(|s| s.trim().trim_start_matches('#').to_owned())
        .filter(|s| !s.is_empty())
        .take(30)
        .collect();
    let privacy_clone = privacy.clone();
    let publish_at_clone = publish_at.clone();

    let (video_id, thumb_uploaded, thumb_note) = tokio::task::spawn_blocking(move || -> Result<(String, bool, Option<String>), String> {
        let (access_token, _) = refresh_youtube_access_token()?;
        let video_bytes = std::fs::read(&file_path).map_err(|e| format!("Could not read MP4: {e}"))?;
        let auth = format!("Bearer {access_token}");
        let len_str = video_bytes.len().to_string();
        let mut status_obj = json!({
            "privacyStatus": privacy_clone,
            "selfDeclaredMadeForKids": false
        });
        if let Some(when) = publish_at_clone {
            status_obj["publishAt"] = json!(when);
        }
        let metadata = json!({
            "snippet": {
                "title": title,
                "description": description,
                "tags": tags,
                "categoryId": "20"
            },
            "status": status_obj
        });
        let meta_bytes = serde_json::to_vec(&metadata).map_err(|e| e.to_string())?;
        let init_resp = platform::https_request(
            "www.googleapis.com",
            "POST",
            "/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status",
            &[
                ("Authorization", &auth),
                ("Content-Type", "application/json; charset=UTF-8"),
                ("X-Upload-Content-Length", &len_str),
                ("X-Upload-Content-Type", "video/mp4"),
            ],
            &meta_bytes,
            20_000,
        )?;
        if init_resp.status != 200 {
            let err_val: Value = serde_json::from_slice(&init_resp.body).unwrap_or_default();
            let msg = err_val["error"]["message"]
                .as_str()
                .unwrap_or("YouTube rejected the draft upload initialization.");
            return Err(msg.to_owned());
        }
        let loc = init_resp
            .location
            .ok_or_else(|| "YouTube did not return an upload session URL.".to_owned())?;
        let upload_path = loc
            .strip_prefix("https://www.googleapis.com")
            .ok_or_else(|| "Unexpected YouTube upload host.".to_owned())?;
        let put_resp = platform::https_request(
            "www.googleapis.com",
            "PUT",
            upload_path,
            &[
                ("Authorization", &auth),
                ("Content-Type", "video/mp4"),
            ],
            &video_bytes,
            180_000,
        )?;
        let body_val: Value = serde_json::from_slice(&put_resp.body).unwrap_or_default();
        if put_resp.status != 200 && put_resp.status != 201 {
            let msg = body_val["error"]["message"]
                .as_str()
                .unwrap_or("YouTube failed to receive the video upload.");
            return Err(msg.to_owned());
        }
        let video_id = body_val["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "YouTube upload succeeded but returned no video ID.".to_owned())?
            .to_owned();

        let mut thumb_ok = false;
        let mut thumb_err = None;
        if let Some(png) = thumb_bytes {
            let tpath = format!("/upload/youtube/v3/thumbnails/set?videoId={}&uploadType=media", url_encode(&video_id));
            match platform::https_request(
                "www.googleapis.com",
                "POST",
                &tpath,
                &[("Authorization", &auth), ("Content-Type", "image/png")],
                &png,
                30_000,
            ) {
                Ok(r) if r.status == 200 => thumb_ok = true,
                Ok(r) => {
                    let ev: Value = serde_json::from_slice(&r.body).unwrap_or_default();
                    thumb_err = Some(
                        ev["error"]["message"]
                            .as_str()
                            .unwrap_or("Channel may require phone verification for custom thumbnails.")
                            .to_owned(),
                    );
                }
                Err(e) => thumb_err = Some(e),
            }
        }
        Ok((video_id, thumb_ok, thumb_err))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;

    let studio_url = format!("https://studio.youtube.com/video/{video_id}/edit");
    if let Ok(db) = app.db.lock() {
        let _ = db.execute(
            "UPDATE workspace_film_roll SET yt_video_id=?1 WHERE clip_id=?2",
            params![video_id, clip_id],
        );
        let _ = db.execute(
            "UPDATE workspace_export_history SET youtube_video_id=?1 WHERE id=(SELECT id FROM workspace_export_history WHERE clip_id=?2 ORDER BY id DESC LIMIT 1)",
            params![video_id, clip_id],
        );
    }
    sync_posting_queue_url(&app, &clip_id, "youtube", &studio_url);
    Ok(Json(json!({
        "video_id": video_id,
        "privacy": privacy,
        "publish_at": publish_at,
        "thumbnail_uploaded": thumb_uploaded,
        "thumbnail_note": thumb_note,
        "studio_url": studio_url
    })))
}

pub(crate) async fn upload_instagram_reel(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
    Json(input): Json<InstagramReelUploadInput>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let (file_path, _) = resolve_upload_file(&app, &clip_id, input.use_source)?;
    let meta = std::fs::metadata(&file_path).map_err(internal)?;
    if meta.len() == 0 || meta.len() > 250 * 1024 * 1024 {
        return Err(bad("Reel video must be between 1 byte and 250 MiB."));
    }
    let ig_id: String = {
        let db = app.db.lock().map_err(internal)?;
        db.query_row(
            "SELECT account_id FROM workspace_oauth_accounts WHERE provider='instagram' AND status='connected'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(internal)?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("Connect Instagram in Connections & System first."))?
    };
    let tokens = platform::vault_read(token_target("instagram").unwrap())
        .ok_or_else(|| bad("Instagram token missing. Reconnect Instagram."))?;
    let access_token = tokens["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("Instagram token missing. Reconnect Instagram."))?
        .to_owned();

    let caption = input
        .caption
        .as_deref()
        .unwrap_or("")
        .chars()
        .take(2200)
        .collect::<String>();
    let share_to_feed = input.share_to_feed;
    let thumb_offset = input.thumb_offset_ms;
    let do_publish = input.publish;

    let result = tokio::task::spawn_blocking(move || -> Result<(String, Option<String>, Option<String>), String> {
        let video_bytes = std::fs::read(&file_path).map_err(|e| format!("Could not read MP4: {e}"))?;
        let file_size = video_bytes.len().to_string();

        // Step 1: Create resumable Reels container on Graph API
        let mut form = format!(
            "media_type=REELS&upload_type=resumable&share_to_feed={}&access_token={}",
            if share_to_feed { "true" } else { "false" },
            url_encode(&access_token),
        );
        if !caption.trim().is_empty() {
            form.push_str("&caption=");
            form.push_str(&url_encode(caption.trim()));
        }
        if let Some(off) = thumb_offset {
            form.push_str("&thumb_offset=");
            form.push_str(&off.to_string());
        }
        let init_path = format!("/v21.0/{}/media", url_encode(&ig_id));
        let init_resp = platform::https_request(
            "graph.facebook.com",
            "POST",
            &init_path,
            &[("Content-Type", "application/x-www-form-urlencoded")],
            form.as_bytes(),
            20_000,
        )?;
        let init_val: Value = serde_json::from_slice(&init_resp.body).unwrap_or_default();
        if init_resp.status != 200 {
            let msg = init_val["error"]["message"]
                .as_str()
                .unwrap_or("Instagram rejected Reel container creation.");
            return Err(msg.to_owned());
        }
        let container_id = init_val["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "Instagram did not return a container ID.".to_owned())?
            .to_owned();
        let upload_uri = init_val["uri"]
            .as_str()
            .filter(|s| s.starts_with("https://rupload.facebook.com/"))
            .ok_or_else(|| "Instagram did not return a rupload.facebook.com upload URI.".to_owned())?;
        let upload_path = upload_uri
            .strip_prefix("https://rupload.facebook.com")
            .ok_or_else(|| "Invalid rupload URI.".to_owned())?;

        // Step 2: Upload raw MP4 bytes directly to rupload.facebook.com (no cloud hosting required!)
        let oauth_hdr = format!("OAuth {access_token}");
        let up_resp = platform::https_request(
            "rupload.facebook.com",
            "POST",
            upload_path,
            &[
                ("Authorization", &oauth_hdr),
                ("offset", "0"),
                ("file_size", &file_size),
                ("Content-Type", "application/octet-stream"),
            ],
            &video_bytes,
            180_000,
        )?;
        if up_resp.status != 200 {
            let ev: Value = serde_json::from_slice(&up_resp.body).unwrap_or_default();
            let msg = ev["debug_info"]["message"]
                .as_str()
                .or_else(|| ev["error"]["message"].as_str())
                .unwrap_or("Instagram rupload video transfer failed.");
            return Err(msg.to_owned());
        }

        // Step 3: Poll container processing until FINISHED (up to 60s)
        let status_path = format!(
            "/v21.0/{}?fields=status_code,status&access_token={}",
            url_encode(&container_id),
            url_encode(&access_token),
        );
        let mut finished = false;
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let sresp = platform::https_request("graph.facebook.com", "GET", &status_path, &[], &[], 15_000)?;
            let sval: Value = serde_json::from_slice(&sresp.body).unwrap_or_default();
            match sval["status_code"].as_str() {
                Some("FINISHED") => {
                    finished = true;
                    break;
                }
                Some("ERROR") | Some("EXPIRED") => {
                    let msg = sval["status"].as_str().unwrap_or("Instagram video processing failed.");
                    return Err(msg.to_owned());
                }
                _ => {}
            }
        }
        if !finished {
            return Err("Instagram is still processing this video container. Try again in a moment.".to_owned());
        }

        if !do_publish {
            return Ok((container_id, None, None));
        }

        // Step 4: Publish finished container (only when user explicitly confirmed publish=true)
        let pub_path = format!("/v21.0/{}/media_publish", url_encode(&ig_id));
        let pub_body = format!(
            "creation_id={}&access_token={}",
            url_encode(&container_id),
            url_encode(&access_token),
        );
        let pub_resp = platform::https_request(
            "graph.facebook.com",
            "POST",
            &pub_path,
            &[("Content-Type", "application/x-www-form-urlencoded")],
            pub_body.as_bytes(),
            30_000,
        )?;
        let pub_val: Value = serde_json::from_slice(&pub_resp.body).unwrap_or_default();
        if pub_resp.status != 200 {
            let msg = pub_val["error"]["message"]
                .as_str()
                .unwrap_or("Instagram rejected Reel publish.");
            return Err(msg.to_owned());
        }
        let media_id = pub_val["id"].as_str().map(str::to_owned);
        let mut permalink = None;
        if let Some(mid) = media_id.as_deref() {
            let p_path = format!(
                "/v21.0/{}?fields=permalink&access_token={}",
                url_encode(mid),
                url_encode(&access_token),
            );
            if let Ok(pr) = platform::https_request("graph.facebook.com", "GET", &p_path, &[], &[], 10_000)
                && pr.status == 200
                && let Ok(pv) = serde_json::from_slice::<Value>(&pr.body)
            {
                permalink = pv["permalink"].as_str().map(str::to_owned);
            }
        }
        Ok((container_id, media_id, permalink))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;

    let (container_id, media_id, permalink) = result;
    if let Some(mid) = media_id.as_deref() {
        let link = permalink.as_deref().unwrap_or("");
        if let Ok(db) = app.db.lock() {
            let _ = db.execute(
                "UPDATE workspace_film_roll SET ig_media_id=?1, ig_permalink=?2 WHERE clip_id=?3",
                params![mid, link, clip_id],
            );
            let _ = db.execute(
                "UPDATE workspace_export_history SET ig_permalink=?1 WHERE id=(SELECT id FROM workspace_export_history WHERE clip_id=?2 ORDER BY id DESC LIMIT 1)",
                params![link, clip_id],
            );
        }
        if !link.is_empty() {
            sync_posting_queue_url(&app, &clip_id, "instagram", link);
        }
    }
    Ok(Json(json!({
        "container_id": container_id,
        "published": media_id.is_some(),
        "media_id": media_id,
        "permalink": permalink
    })))
}

pub(crate) async fn extend_instagram_token(State(app): State<App>) -> ApiResult<Json<Value>> {
    let client = platform::vault_read(client_target("instagram").unwrap())
        .ok_or_else(|| bad("Instagram App ID & Secret are not configured."))?;
    let tokens = platform::vault_read(token_target("instagram").unwrap())
        .ok_or_else(|| bad("Instagram is not connected."))?;
    let cid = client["client_id"].as_str().unwrap_or("").to_owned();
    let csec = client["client_secret"].as_str().unwrap_or("").to_owned();
    let cur = tokens["access_token"].as_str().unwrap_or("").to_owned();
    if cid.is_empty() || csec.is_empty() || cur.is_empty() {
        return Err(bad("Missing Instagram credentials or token."));
    }
    let app_clone = app.clone();
    let new_exp = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let ll_path = format!(
            "/v21.0/oauth/access_token?grant_type=fb_exchange_token&client_id={}&client_secret={}&fb_exchange_token={}",
            url_encode(&cid),
            url_encode(&csec),
            url_encode(&cur),
        );
        let resp = platform::https_request("graph.facebook.com", "GET", &ll_path, &[], &[], 15_000)?;
        let parsed: Value = serde_json::from_slice(&resp.body)
            .map_err(|_| "Unreadable response from Meta token exchange.".to_owned())?;
        if resp.status != 200 {
            let msg = parsed["error"]["message"]
                .as_str()
                .unwrap_or("Meta rejected token extension.");
            return Err(msg.to_owned());
        }
        let next_tok = parsed["access_token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "Meta did not return a refreshed token.".to_owned())?
            .to_owned();
        let exp = now_unix() + parsed["expires_in"].as_i64().unwrap_or(5_184_000);
        platform::vault_write(
            token_target("instagram").unwrap(),
            &json!({ "access_token": next_tok, "expires_at": exp }),
        )?;
        if let Ok(db) = app_clone.db.lock() {
            let _ = db.execute(
                "UPDATE workspace_oauth_accounts SET expires_at=?1, last_checked_at=?2, last_error='' WHERE provider='instagram'",
                params![exp, now_unix()],
            );
        }
        Ok(exp)
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;
    Ok(Json(json!({ "provider": "instagram", "expires_at": new_exp })))
}

pub(crate) async fn sync_analytics(State(app): State<App>) -> ApiResult<Json<Value>> {
    let app_clone = app.clone();
    let report = tokio::task::spawn_blocking(move || -> Value {
        let mut yt_synced = 0usize;
        let mut yt_err: Option<String> = None;
        let mut ig_synced = 0usize;
        let mut ig_err: Option<String> = None;

        // 1. Sync YouTube channel stats + last 25 uploads
        if let Ok((tok, exp)) = refresh_youtube_access_token() {
            let auth = format!("Bearer {tok}");
            match platform::https_request(
                "www.googleapis.com",
                "GET",
                "/youtube/v3/channels?part=snippet,statistics,contentDetails&mine=true",
                &[("Authorization", &auth)],
                &[],
                15_000,
            ) {
                Ok(r) if r.status == 200 => {
                    if let Ok(v) = serde_json::from_slice::<Value>(&r.body)
                        && let Some(ch) = v["items"].as_array().and_then(|a| a.first())
                    {
                        let ch_id = ch["id"].as_str().unwrap_or("");
                        let ch_name = ch["snippet"]["title"].as_str().unwrap_or("YouTube");
                        let ch_handle = ch["snippet"]["customUrl"].as_str().unwrap_or("");
                        let subs = ch["statistics"]["subscriberCount"].as_str().and_then(|s| s.parse::<i64>().ok());
                        let total_views = ch["statistics"]["viewCount"].as_str().and_then(|s| s.parse::<i64>().ok());
                        let vid_count = ch["statistics"]["videoCount"].as_str().and_then(|s| s.parse::<i64>().ok());
                        let _ = save_account_row(&app_clone, "youtube", "connected", ch_id, ch_name, ch_handle, "", subs, vid_count, Some(exp), "");

                        let mut sum_likes = 0i64;
                        let mut sum_comments = 0i64;
                        if let Some(uploads) = ch["contentDetails"]["relatedPlaylists"]["uploads"].as_str() {
                            let pl_path = format!(
                                "/youtube/v3/playlistItems?part=contentDetails&maxResults=25&playlistId={}",
                                url_encode(uploads)
                            );
                            if let Ok(plr) = platform::https_request("www.googleapis.com", "GET", &pl_path, &[("Authorization", &auth)], &[], 15_000)
                                && plr.status == 200
                                && let Ok(plv) = serde_json::from_slice::<Value>(&plr.body)
                            {
                                let ids: Vec<String> = plv["items"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(|x| x["contentDetails"]["videoId"].as_str().map(str::to_owned))
                                    .collect();
                                if !ids.is_empty() {
                                    let vpath = format!(
                                        "/youtube/v3/videos?part=snippet,statistics&id={}",
                                        url_encode(&ids.join(","))
                                    );
                                    if let Ok(vr) = platform::https_request("www.googleapis.com", "GET", &vpath, &[("Authorization", &auth)], &[], 15_000)
                                        && vr.status == 200
                                        && let Ok(vv) = serde_json::from_slice::<Value>(&vr.body)
                                        && let Some(items) = vv["items"].as_array()
                                        && let Ok(db) = app_clone.db.lock()
                                    {
                                        for item in items {
                                            let pid = item["id"].as_str().unwrap_or("");
                                            if pid.is_empty() { continue; }
                                            let title = item["snippet"]["title"].as_str().unwrap_or("");
                                            let desc = item["snippet"]["description"].as_str().unwrap_or("");
                                            let pub_at = item["snippet"]["publishedAt"].as_str().unwrap_or("");
                                            let thumb = item["snippet"]["thumbnails"]["medium"]["url"]
                                                .as_str()
                                                .or_else(|| item["snippet"]["thumbnails"]["default"]["url"].as_str())
                                                .unwrap_or("");
                                            let views = item["statistics"]["viewCount"].as_str().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                                            let likes = item["statistics"]["likeCount"].as_str().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                                            let comments = item["statistics"]["commentCount"].as_str().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                                            sum_likes += likes;
                                            sum_comments += comments;
                                            let link = format!("https://www.youtube.com/watch?v={pid}");
                                            let _ = db.execute(
                                                "INSERT INTO workspace_channel_posts(provider, post_id, title, caption, permalink, thumbnail_url, published_at, views, likes, comments, media_type, synced_at)
                                                 VALUES('youtube', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'VIDEO', unixepoch())
                                                 ON CONFLICT(provider, post_id) DO UPDATE SET
                                                   title=excluded.title, caption=excluded.caption, thumbnail_url=excluded.thumbnail_url,
                                                   published_at=excluded.published_at, views=excluded.views, likes=excluded.likes,
                                                   comments=excluded.comments, synced_at=unixepoch()",
                                                params![pid, title, desc, link, thumb, pub_at, views, likes, comments],
                                            );
                                            yt_synced += 1;
                                        }
                                    }
                                }
                            }
                        }
                        record_analytics_snapshot(&app_clone, "youtube", subs, total_views, Some(sum_likes), Some(sum_comments));
                    }
                }
                Ok(_) => yt_err = Some("YouTube API rejected analytics sync.".to_owned()),
                Err(e) => yt_err = Some(e),
            }
        }

        // 2. Sync Instagram @account stats + last 25 media posts/Reels
        if let Some(tokens) = platform::vault_read(token_target("instagram").unwrap())
            && let Some(tok) = tokens["access_token"].as_str().filter(|s| !s.is_empty())
        {
            let exp = tokens["expires_at"].as_i64();
            match fetch_instagram_business_account(tok) {
                Ok((ig_id, ig_name, ig_handle, followers, media_count)) => {
                    let _ = save_account_row(&app_clone, "instagram", "connected", &ig_id, &ig_name, &ig_handle, "", followers, media_count, exp, "");
                    let mpath = format!(
                        "/v21.0/{}/media?fields=id,caption,media_type,permalink,thumbnail_url,media_url,timestamp,like_count,comments_count&limit=25&access_token={}",
                        url_encode(&ig_id),
                        url_encode(tok),
                    );
                    let mut sum_likes = 0i64;
                    let mut sum_comments = 0i64;
                    if let Ok(mr) = platform::https_request("graph.facebook.com", "GET", &mpath, &[], &[], 15_000)
                        && mr.status == 200
                        && let Ok(mv) = serde_json::from_slice::<Value>(&mr.body)
                        && let Some(items) = mv["data"].as_array()
                        && let Ok(db) = app_clone.db.lock()
                    {
                        for item in items {
                            let pid = item["id"].as_str().unwrap_or("");
                            if pid.is_empty() { continue; }
                            let cap = item["caption"].as_str().unwrap_or("");
                            let title: String = cap.lines().next().unwrap_or("Instagram Post").chars().take(90).collect();
                            let link = item["permalink"].as_str().unwrap_or("");
                            let thumb = item["thumbnail_url"]
                                .as_str()
                                .or_else(|| item["media_url"].as_str())
                                .unwrap_or("");
                            let ts = item["timestamp"].as_str().unwrap_or("");
                            let likes = item["like_count"].as_i64().unwrap_or(0);
                            let comments = item["comments_count"].as_i64().unwrap_or(0);
                            let mtype = item["media_type"].as_str().unwrap_or("VIDEO");
                            sum_likes += likes;
                            sum_comments += comments;
                            let _ = db.execute(
                                "INSERT INTO workspace_channel_posts(provider, post_id, title, caption, permalink, thumbnail_url, published_at, views, likes, comments, media_type, synced_at)
                                 VALUES('instagram', ?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9, unixepoch())
                                 ON CONFLICT(provider, post_id) DO UPDATE SET
                                   title=excluded.title, caption=excluded.caption, permalink=excluded.permalink,
                                   thumbnail_url=excluded.thumbnail_url, published_at=excluded.published_at,
                                   likes=excluded.likes, comments=excluded.comments, media_type=excluded.media_type, synced_at=unixepoch()",
                                params![pid, title, cap, link, thumb, ts, likes, comments, mtype],
                            );
                            ig_synced += 1;
                        }
                    }
                    record_analytics_snapshot(&app_clone, "instagram", followers, None, Some(sum_likes), Some(sum_comments));
                }
                Err(e) => ig_err = Some(e),
            }
        }

        json!({
            "youtube_synced": yt_synced,
            "youtube_error": yt_err,
            "instagram_synced": ig_synced,
            "instagram_error": ig_err,
            "synced_at": now_unix()
        })
    })
    .await
    .map_err(internal)?;
    Ok(Json(report))
}

pub(crate) async fn get_analytics(State(app): State<App>) -> ApiResult<Json<Value>> {
    let db = app.db.lock().map_err(internal)?;
    let mut accounts = json!({});
    {
        let mut stmt = db
            .prepare(
                "SELECT provider, status, account_name, handle, subscribers, media_count, expires_at, last_checked_at
                 FROM workspace_oauth_accounts",
            )
            .map_err(internal)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    json!({
                        "status": r.get::<_, String>(1)?,
                        "account_name": r.get::<_, String>(2)?,
                        "handle": r.get::<_, String>(3)?,
                        "subscribers": r.get::<_, Option<i64>>(4)?,
                        "media_count": r.get::<_, Option<i64>>(5)?,
                        "expires_at": r.get::<_, Option<i64>>(6)?,
                        "last_checked_at": r.get::<_, Option<i64>>(7)?,
                    }),
                ))
            })
            .map_err(internal)?;
        for (k, v) in rows.flatten() {
            accounts[k] = v;
        }
    }
    let posts: Vec<Value> = {
        let mut stmt = db
            .prepare(
                "SELECT provider, post_id, title, caption, permalink, thumbnail_url,
                        published_at, views, likes, comments, media_type, synced_at
                 FROM workspace_channel_posts
                 ORDER BY published_at DESC, synced_at DESC
                 LIMIT 100",
            )
            .map_err(internal)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({
                    "provider": r.get::<_, String>(0)?,
                    "post_id": r.get::<_, String>(1)?,
                    "title": r.get::<_, String>(2)?,
                    "caption": r.get::<_, String>(3)?,
                    "permalink": r.get::<_, String>(4)?,
                    "thumbnail_url": r.get::<_, String>(5)?,
                    "published_at": r.get::<_, String>(6)?,
                    "views": r.get::<_, i64>(7)?,
                    "likes": r.get::<_, i64>(8)?,
                    "comments": r.get::<_, i64>(9)?,
                    "media_type": r.get::<_, String>(10)?,
                    "synced_at": r.get::<_, i64>(11)?,
                }))
            })
            .map_err(internal)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(internal)?
    };
    Ok(Json(json!({ "accounts": accounts, "posts": posts })))
}

pub(crate) async fn connect_instagram_token(
    State(app): State<App>,
    Json(input): Json<InstagramTokenInput>,
) -> ApiResult<Json<Value>> {
    let raw_token = input.access_token.trim().to_owned();
    if raw_token.is_empty() || raw_token.len() > 2048 || raw_token.chars().any(char::is_whitespace) {
        return Err(bad("Paste a valid Meta access token (no spaces)."));
    }
    let client = platform::vault_read(client_target("instagram").unwrap());
    let app_clone = app.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        // If App ID + Secret are saved, upgrade short-lived Graph API Explorer token -> 60-day long-lived token.
        let (final_tok, exp_at) = if let Some(c) = client
            && let (Some(cid), Some(csec)) = (c["client_id"].as_str(), c["client_secret"].as_str())
            && !cid.is_empty()
            && !csec.is_empty()
        {
            let ll_path = format!(
                "/v21.0/oauth/access_token?grant_type=fb_exchange_token&client_id={}&client_secret={}&fb_exchange_token={}",
                url_encode(cid),
                url_encode(csec),
                url_encode(&raw_token),
            );
            match platform::https_request("graph.facebook.com", "GET", &ll_path, &[], &[], 15_000) {
                Ok(r) if r.status == 200 => {
                    let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
                    let tok = v["access_token"].as_str().unwrap_or(&raw_token).to_owned();
                    let exp = now_unix() + v["expires_in"].as_i64().unwrap_or(5_184_000);
                    (tok, exp)
                }
                _ => (raw_token, now_unix() + 3600),
            }
        } else {
            (raw_token, now_unix() + 3600)
        };
        let (ig_id, ig_name, ig_handle, followers, media) = fetch_instagram_business_account(&final_tok)?;
        platform::vault_write(
            token_target("instagram").unwrap(),
            &json!({ "access_token": final_tok, "expires_at": exp_at }),
        )?;
        save_account_row(
            &app_clone,
            "instagram",
            "connected",
            &ig_id,
            &ig_name,
            &ig_handle,
            "",
            followers,
            media,
            Some(exp_at),
            "",
        )?;
        let ig_url = format!(
            "https://www.instagram.com/{}/",
            ig_handle.trim_start_matches('@')
        );
        ensure_destination_profile(&app_clone, "instagram", &ig_name, &ig_url);
        Ok(json!({
            "provider": "instagram",
            "connected": true,
            "account_name": ig_name,
            "handle": ig_handle,
            "subscribers": followers,
            "media_count": media,
            "expires_at": exp_at
        }))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;
    Ok(Json(result))
}
