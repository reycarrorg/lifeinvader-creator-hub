use super::{client_target, json, platform, url_encode, Value};

// Public builds can embed the issuer hostname and public app key. The operator's
// secret lives at that issuer. Existing owner installations use the OS vault.
pub(super) fn config() -> Result<Value, String> {
    let setting = |name: &str, built_in: Option<&str>| {
        std::env::var(name).ok().or_else(|| built_in.map(str::to_owned))
            .filter(|s| !s.is_empty())
    };
    let host = setting("SNAPMATIC_TIKTOK_BROKER_HOST", option_env!("SNAPMATIC_TIKTOK_BROKER_HOST"));
    let key = setting("SNAPMATIC_TIKTOK_CLIENT_KEY", option_env!("SNAPMATIC_TIKTOK_CLIENT_KEY"));
    match (host, key) {
        (None, None) => platform::vault_read(client_target("tiktok").unwrap())
            .ok_or_else(|| "Configure Snapmatic's TikTok Login Kit connection first.".into()),
        (Some(host), Some(key)) if valid_host(&host) && key.is_ascii() && key.len() <= 256 => {
            Ok(json!({"client_id": key, "broker_host": host.to_ascii_lowercase()}))
        }
        _ => Err("Configure both the HTTPS token broker hostname and the TikTok client key.".into()),
    }
}

fn valid_host(host: &str) -> bool {
    host.len() <= 253 && host.contains('.') && host.split('.').all(|label| {
        !label.is_empty() && label.len() <= 63 && !label.starts_with('-') && !label.ends_with('-')
            && label.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    })
}

pub(super) fn identity(config: &Value) -> (String, String) {
    (config["client_id"].as_str().unwrap_or("").to_owned(),
     config["broker_host"].as_str().unwrap_or("").to_owned())
}

pub(super) fn publish_scope(tokens: &Value) -> bool {
    tokens["scope"].as_str().unwrap_or("").split(|c:char| c == ',' || c.is_ascii_whitespace())
        .any(|scope| scope == "video.publish")
}

pub(super) fn exchange(client: &Value, fields: &[(&str, &str)]) -> Result<Value, String> {
    let (key, host) = identity(client);
    let response = if host.is_empty() {
        let secret = client["client_secret"].as_str().filter(|s| !s.is_empty())
            .ok_or("The operator's TikTok app credentials are incomplete.")?;
        let body = fields.iter().copied().chain([("client_key",key.as_str()),("client_secret",secret)])
            .map(|(k,v)| format!("{k}={}",url_encode(v))).collect::<Vec<_>>().join("&");
        platform::https_request("open.tiktokapis.com","POST","/v2/oauth/token/",
            &[("Content-Type","application/x-www-form-urlencoded")],body.as_bytes(),15_000)?
    } else {
        let mut payload = json!({"client_key":key});
        for &(field, value) in fields { payload[field] = json!(value); }
        platform::https_request(&host,"POST","/v1/tiktok/token",
            &[("Content-Type","application/json")],&serde_json::to_vec(&payload).map_err(|e|e.to_string())?,15_000)?
    };
    let tokens: Value = serde_json::from_slice(&response.body).map_err(|_| "Unreadable TikTok token response.")?;
    if response.status != 200 || tokens["access_token"].as_str().unwrap_or("").is_empty() {
        return Err("TikTok authorization failed. Check the app configuration and reconnect your account.".into());
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_configuration_rejects_urls_and_distinguishes_granted_scopes() {
        assert!(valid_host("auth.example.com"));
        for host in ["http://auth.example.com", "auth.example.com?x", "key@auth.example.com", "a..test"] {
            assert!(!valid_host(host));
        }
        assert!(publish_scope(&json!({"scope":"video.upload,video.publish"})));
        assert!(!publish_scope(&json!({"scope":"video.publish.extra video.upload"})));
        assert!(!publish_scope(&json!({})));
    }
}
