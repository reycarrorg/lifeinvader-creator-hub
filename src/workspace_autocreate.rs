use crate::workspace_film_roll::{
    exports_dir, find_ffmpeg, register_external_export, resolve_exported_video, resolve_video,
};
use crate::{ApiResult, App, bad, internal};
use axum::{
    Json,
    extract::{Path as AxumPath, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

const OPUS_VAULT_TARGET: &str = "Snapmatic/Studio/OpusPro";
const SUBMAGIC_VAULT_TARGET: &str = "Snapmatic/Studio/Submagic";

fn hide_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

fn find_ffprobe(ffmpeg: &Path) -> PathBuf {
    let probe_name = if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" };
    if let Some(dir) = ffmpeg.parent() {
        let sib = dir.join(probe_name);
        if sib.is_file() {
            return sib;
        }
    }
    PathBuf::from(probe_name)
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct OpusCreds {
    api_key: String,
    #[serde(default)]
    brand_template_id: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct SubmagicCreds {
    api_key: String,
    #[serde(default = "default_submagic_template")]
    template_name: String,
    #[serde(default = "default_true")]
    magic_brolls: bool,
    #[serde(default = "default_true")]
    magic_zooms: bool,
}

fn default_submagic_template() -> String {
    "Hormozi 2".to_owned()
}
fn default_true() -> bool {
    true
}

fn load_opus_creds() -> Option<OpusCreds> {
    let raw = crate::workspace_oauth::studio_vault_read(OPUS_VAULT_TARGET)?;
    let creds: OpusCreds = serde_json::from_value(raw).ok()?;
    (!creds.api_key.trim().is_empty()).then_some(creds)
}

fn load_submagic_creds() -> Option<SubmagicCreds> {
    let raw = crate::workspace_oauth::studio_vault_read(SUBMAGIC_VAULT_TARGET)?;
    let creds: SubmagicCreds = serde_json::from_value(raw).ok()?;
    (!creds.api_key.trim().is_empty()).then_some(creds)
}

#[derive(Serialize, Clone, Default)]
struct CloudJobState {
    active: bool,
    provider: String,
    clip_id: String,
    stage: String,
    percent: u8,
    message: String,
    error: Option<String>,
    result: Option<Value>,
}

fn cloud_job_state() -> &'static Mutex<CloudJobState> {
    static STATE: OnceLock<Mutex<CloudJobState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(CloudJobState::default()))
}

fn cloud_cancel_flag() -> &'static AtomicBool {
    static CANCEL: OnceLock<AtomicBool> = OnceLock::new();
    CANCEL.get_or_init(|| AtomicBool::new(false))
}

fn set_job_progress(percent: u8, stage: &str, message: impl Into<String>) {
    if let Ok(mut st) = cloud_job_state().lock() {
        st.percent = percent;
        st.stage = stage.to_owned();
        st.message = message.into();
    }
}

#[derive(Deserialize, Default)]
pub(crate) struct ProviderKeyRequest {
    provider: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    keep_existing_key: bool,
    #[serde(default)]
    brand_template_id: Option<String>,
    #[serde(default)]
    template_name: Option<String>,
    #[serde(default)]
    magic_brolls: Option<bool>,
    #[serde(default)]
    magic_zooms: Option<bool>,
}

pub(crate) async fn providers_status() -> ApiResult<Json<Value>> {
    let opus = load_opus_creds();
    let sub = load_submagic_creds();
    let job = cloud_job_state()
        .lock()
        .map(|s| s.clone())
        .unwrap_or_default();
    Ok(Json(json!({
        "opus_configured": opus.is_some(),
        "opus_template_id": opus.as_ref().map(|c| c.brand_template_id.clone()).unwrap_or_default(),
        "submagic_configured": sub.is_some(),
        "submagic_template": sub.as_ref().map(|c| c.template_name.clone()).unwrap_or_else(default_submagic_template),
        "submagic_brolls": sub.as_ref().map(|c| c.magic_brolls).unwrap_or(true),
        "submagic_zooms": sub.as_ref().map(|c| c.magic_zooms).unwrap_or(true),
        "job": job,
    })))
}

pub(crate) async fn save_provider_key(
    Json(input): Json<ProviderKeyRequest>,
) -> ApiResult<Json<Value>> {
    let prov = input.provider.trim().to_ascii_lowercase();
    let raw_key = input.api_key.trim().to_owned();
    match prov.as_str() {
        "opus" => {
            let final_key = if raw_key.is_empty() && input.keep_existing_key {
                load_opus_creds().map(|c| c.api_key).unwrap_or_default()
            } else {
                raw_key
            };
            if final_key.is_empty() {
                crate::workspace_oauth::studio_vault_delete(OPUS_VAULT_TARGET);
                return Ok(Json(json!({ "provider": "opus", "configured": false })));
            }
            let payload = json!({
                "api_key": final_key,
                "brand_template_id": input.brand_template_id.unwrap_or_default().trim(),
            });
            crate::workspace_oauth::studio_vault_write(OPUS_VAULT_TARGET, &payload)
                .map_err(bad)?;
            Ok(Json(json!({ "provider": "opus", "configured": true })))
        }
        "submagic" => {
            let existing = load_submagic_creds();
            let final_key = if raw_key.is_empty() && input.keep_existing_key {
                existing.as_ref().map(|c| c.api_key.clone()).unwrap_or_default()
            } else {
                raw_key
            };
            if final_key.is_empty() {
                crate::workspace_oauth::studio_vault_delete(SUBMAGIC_VAULT_TARGET);
                return Ok(Json(json!({ "provider": "submagic", "configured": false })));
            }
            let tpl = input
                .template_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(default_submagic_template);
            let payload = json!({
                "api_key": final_key,
                "template_name": tpl,
                "magic_brolls": input.magic_brolls.unwrap_or(true),
                "magic_zooms": input.magic_zooms.unwrap_or(true),
            });
            crate::workspace_oauth::studio_vault_write(SUBMAGIC_VAULT_TARGET, &payload)
                .map_err(bad)?;
            Ok(Json(json!({ "provider": "submagic", "configured": true })))
        }
        _ => Err(bad("Unsupported provider. Use 'opus' or 'submagic'.")),
    }
}

pub(crate) async fn cancel_provider_job() -> ApiResult<Json<Value>> {
    cloud_cancel_flag().store(true, Ordering::SeqCst);
    if let Ok(mut st) = cloud_job_state().lock()
        && st.active
    {
        st.message = "Cancelling cloud AI job…".to_owned();
    }
    Ok(Json(json!({ "cancelled": true })))
}

// ============================================================================
// AUTOMATIC TIKTOK ORIGINAL CONTENT GUIDELINES AUDIT ENGINE
// ============================================================================

#[derive(Deserialize, Default)]
pub(crate) struct AuditRequest {
    #[serde(default)]
    target: Option<String>, // "source" | "export" | None (auto: export if exists, else source)
}

#[derive(Serialize)]
struct GuidelineCheck {
    id: &'static str,
    label: &'static str,
    status: &'static str, // "pass" | "warn" | "fail"
    detail: String,
    auto_fixable: bool,
}

fn parse_fps(raw: &str) -> f64 {
    if let Some((a, b)) = raw.split_once('/') {
        let num = a.trim().parse::<f64>().unwrap_or(0.0);
        let den = b.trim().parse::<f64>().unwrap_or(1.0);
        if den > 0.0 { num / den } else { 0.0 }
    } else {
        raw.trim().parse::<f64>().unwrap_or(0.0)
    }
}

fn detect_suspicious_tags(tags: &serde_json::Map<String, Value>) -> Vec<String> {
    const SUSPECT: &[&str] = &[
        "tiktok",
        "capcut",
        "instagram",
        "snaptik",
        "ssstik",
        "musical.ly",
        "bytedance",
        "douyin",
        "yt-dlp",
        "savetik",
        "reel",
        "inshot",
        "vn video",
    ];
    let mut hits = Vec::new();
    for (k, v) in tags {
        let kl = k.to_ascii_lowercase();
        let vs = v.as_str().unwrap_or("").to_ascii_lowercase();
        if vs.is_empty() {
            continue;
        }
        if matches!(kl.as_str(), "comment" | "purl" | "description" | "synopsis") && vs.len() > 2 {
            hits.push(format!("{k} tag present"));
            continue;
        }
        if let Some(m) = SUSPECT.iter().find(|s| vs.contains(**s)) {
            hits.push(format!("{k} contains '{m}'"));
        }
    }
    hits.sort();
    hits.dedup();
    hits
}

fn probe_strip_luma_sat(ffmpeg: &Path, path: &Path, seek_s: f64, y_expr: &str, h_expr: &str) -> Option<(f64, f64)> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "info",
        "-ss",
        &format!("{seek_s:.2}"),
        "-i",
    ])
    .arg(path)
    .args([
        "-frames:v",
        "1",
        "-vf",
        &format!("crop=iw:{h_expr}:0:{y_expr},signalstats,metadata=mode=print:file=-"),
        "-f",
        "null",
        "-",
    ])
    .stdin(Stdio::null());
    hide_window(&mut cmd);
    let out = cmd.output().ok()?;
    let txt = String::from_utf8_lossy(&out.stdout);
    let mut yavg: Option<f64> = None;
    let mut satavg: Option<f64> = None;
    for line in txt.lines() {
        if let Some(rest) = line.split("lavfi.signalstats.YAVG=").nth(1) {
            yavg = rest.trim().parse::<f64>().ok();
        } else if let Some(rest) = line.split("lavfi.signalstats.SATAVG=").nth(1) {
            satavg = rest.trim().parse::<f64>().ok();
        }
    }
    Some((yavg?, satavg.unwrap_or(0.0)))
}

fn detect_crop_and_headers(
    ffmpeg: &Path,
    path: &Path,
    width: u32,
    height: u32,
    duration_s: f64,
) -> (Option<String>, Option<String>) {
    if width < 200 || height < 200 {
        return (None, None);
    }
    let seek_s = (duration_s * 0.25).clamp(0.2, 8.0);

    // 1. Check for bright white baked-in rival caption header (top 18%) or footer (bottom 15%)
    let top_white = probe_strip_luma_sat(ffmpeg, path, seek_s, "0", "floor(ih*0.18)")
        .is_some_and(|(y, sat)| y >= 215.0 && sat <= 20.0);
    let bot_white = probe_strip_luma_sat(ffmpeg, path, seek_s, "floor(ih*0.85)", "floor(ih*0.15)")
        .is_some_and(|(y, sat)| y >= 215.0 && sat <= 20.0);

    if top_white || bot_white {
        let top_cut = if top_white { ((height as f64 * 0.20).round() as u32) & !1 } else { 0 };
        let bot_cut = if bot_white { ((height as f64 * 0.16).round() as u32) & !1 } else { 0 };
        let new_h = height.saturating_sub(top_cut + bot_cut) & !1;
        if new_h >= (height / 2).max(200) {
            let reason = match (top_white, bot_white) {
                (true, true) => "Detected baked-in white caption bars at top & bottom".to_owned(),
                (true, false) => "Detected baked-in white rival caption header at top".to_owned(),
                _ => "Detected baked-in white caption footer at bottom".to_owned(),
            };
            return (Some(format!("{width}:{new_h}:0:{top_cut}")), Some(reason));
        }
    }

    // 2. Check for black letterbox / pillarbox bars using FFmpeg cropdetect
    let mut cmd = Command::new(ffmpeg);
    cmd.args([
        "-hide_banner",
        "-ss",
        &format!("{seek_s:.2}"),
        "-t",
        "2.5",
        "-i",
    ])
    .arg(path)
    .args(["-vf", "cropdetect=24:16:0", "-f", "null", "-"])
    .stdin(Stdio::null());
    hide_window(&mut cmd);
    if let Ok(out) = cmd.output() {
        let err = String::from_utf8_lossy(&out.stderr);
        if let Some(last_crop) = err
            .lines()
            .rev()
            .find_map(|l| l.split("crop=").nth(1))
            .and_then(|s| s.split_whitespace().next())
        {
            let parts: Vec<u32> = last_crop
                .split(':')
                .filter_map(|p| p.parse::<u32>().ok())
                .collect();
            if parts.len() == 4 {
                let (cw, ch, cx, cy) = (parts[0], parts[1], parts[2], parts[3]);
                let trim_w = width.saturating_sub(cw);
                let trim_h = height.saturating_sub(ch);
                let significant = (trim_h as f64 / height as f64) >= 0.08
                    || (trim_w as f64 / width as f64) >= 0.08;
                let safe_size = cw >= (width / 2).max(200) && ch >= (height / 2).max(200);
                if significant && safe_size {
                    let ew = cw & !1;
                    let eh = ch & !1;
                    return (
                        Some(format!("{ew}:{eh}:{cx}:{cy}")),
                        Some(format!(
                            "Detected black letterbox/pillarbox bars (active frame {ew}×{eh})"
                        )),
                    );
                }
            }
        }
    }

    (None, None)
}

fn scan_audio_metrics(ffmpeg: &Path, path: &Path, duration_s: f64) -> (Option<f64>, f64) {
    let scan_dur = duration_s.clamp(1.0, 90.0);
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-t", &format!("{scan_dur:.1}"), "-i"])
        .arg(path)
        .args([
            "-vn",
            "-af",
            "volumedetect,silencedetect=n=-32dB:d=0.45",
            "-f",
            "null",
            "-",
        ])
        .stdin(Stdio::null());
    hide_window(&mut cmd);
    let Ok(out) = cmd.output() else {
        return (None, 0.0);
    };
    let err = String::from_utf8_lossy(&out.stderr);
    let mut mean_db: Option<f64> = None;
    let mut silence_total = 0.0f64;
    for line in err.lines() {
        if let Some(rest) = line.split("mean_volume:").nth(1) {
            mean_db = rest.split_whitespace().next().and_then(|s| s.parse::<f64>().ok());
        } else if let Some(rest) = line.split("silence_duration:").nth(1)
            && let Some(d) = rest.split_whitespace().next().and_then(|s| s.parse::<f64>().ok())
        {
            silence_total += d;
        }
    }
    let ratio = if scan_dur > 0.5 {
        (silence_total / scan_dur).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (mean_db, ratio)
}

fn scan_freeze_seconds(ffmpeg: &Path, path: &Path, duration_s: f64) -> f64 {
    let scan_dur = duration_s.clamp(1.0, 45.0);
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-t", &format!("{scan_dur:.1}"), "-i"])
        .arg(path)
        .args(["-an", "-vf", "freezedetect=n=-50dB:d=2.2", "-f", "null", "-"])
        .stdin(Stdio::null());
    hide_window(&mut cmd);
    let Ok(out) = cmd.output() else {
        return 0.0;
    };
    let err = String::from_utf8_lossy(&out.stderr);
    let mut frozen = 0.0f64;
    for line in err.lines() {
        if let Some(rest) = line.split("freeze_duration:").nth(1)
            && let Some(d) = rest.split_whitespace().next().and_then(|s| s.parse::<f64>().ok())
        {
            frozen += d;
        }
    }
    frozen
}

pub(crate) async fn tiktok_audit(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
    body: Option<Json<AuditRequest>>,
) -> ApiResult<Json<Value>> {
    let req = body.map(|b| b.0).unwrap_or_default();
    let ffmpeg = find_ffmpeg().ok_or_else(|| bad("FFmpeg not found. Install FFmpeg to run TikTok Guidelines Audit."))?;
    let requested = req.target.as_deref().map(str::trim).unwrap_or("auto");

    let (target_path, is_export) = match requested {
        "source" => (
            resolve_video(&app, &clip_id).ok_or_else(|| bad("Clip source video not found."))?,
            false,
        ),
        "export" => {
            if let Some(exp) = resolve_exported_video(&app, &clip_id) {
                (exp, true)
            } else {
                (
                    resolve_video(&app, &clip_id).ok_or_else(|| bad("Clip video not found."))?,
                    false,
                )
            }
        }
        _ => {
            if let Some(exp) = resolve_exported_video(&app, &clip_id) {
                (exp, true)
            } else {
                (
                    resolve_video(&app, &clip_id).ok_or_else(|| bad("Clip video not found."))?,
                    false,
                )
            }
        }
    };

    let result = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let ffprobe = find_ffprobe(&ffmpeg);
        let mut pcmd = Command::new(&ffprobe);
        pcmd.args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(&target_path)
        .stdin(Stdio::null());
        hide_window(&mut pcmd);
        let pout = pcmd
            .output()
            .map_err(|e| format!("ffprobe failed to start: {e}"))?;
        let probed: Value = serde_json::from_slice(&pout.stdout).unwrap_or_else(|_| json!({}));

        let streams = probed["streams"].as_array().cloned().unwrap_or_default();
        let vstream = streams
            .iter()
            .find(|s| s["codec_type"].as_str() == Some("video"));
        let has_audio = streams
            .iter()
            .any(|s| s["codec_type"].as_str() == Some("audio"));

        let width = vstream.and_then(|s| s["width"].as_u64()).unwrap_or(0) as u32;
        let height = vstream.and_then(|s| s["height"].as_u64()).unwrap_or(0) as u32;
        let fps = vstream
            .and_then(|s| s["avg_frame_rate"].as_str().or_else(|| s["r_frame_rate"].as_str()))
            .map(parse_fps)
            .unwrap_or(0.0);
        let duration_s = probed["format"]["duration"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let bitrate_bps = probed["format"]["bit_rate"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let bitrate_mbps = bitrate_bps / 1_000_000.0;
        let tags = probed["format"]["tags"]
            .as_object()
            .cloned()
            .unwrap_or_default();

        let (suggested_crop, crop_reason) =
            detect_crop_and_headers(&ffmpeg, &target_path, width, height, duration_s);
        let (mean_db, silence_ratio) = if has_audio {
            scan_audio_metrics(&ffmpeg, &target_path, duration_s)
        } else {
            (None, 1.0)
        };
        let frozen_s = scan_freeze_seconds(&ffmpeg, &target_path, duration_s);
        let suspect_tags = detect_suspicious_tags(&tags);

        let mut checks: Vec<GuidelineCheck> = Vec::new();
        let mut score: i32 = 0;

        // 1. Resolution, Aspect Ratio (9:16) & Bitrate Quality (20 pts)
        let is_vertical = height > width && width > 0 && ((width as f64 / height as f64) - (9.0 / 16.0)).abs() < 0.08;
        let is_1080p = width >= 1080 && height >= 1920;
        let is_720p = width >= 720 && height >= 1280;
        let good_bitrate = bitrate_mbps >= 3.0;
        if is_vertical && is_1080p && good_bitrate {
            score += 20;
            checks.push(GuidelineCheck {
                id: "quality",
                label: "1080p Vertical HD Quality",
                status: "pass",
                detail: format!("{width}×{height} · {fps:.0}fps · {bitrate_mbps:.1} Mbps"),
                auto_fixable: false,
            });
        } else if is_vertical && is_720p {
            score += 12;
            checks.push(GuidelineCheck {
                id: "quality",
                label: "1080p Vertical HD Quality",
                status: "warn",
                detail: format!("{width}×{height} ({bitrate_mbps:.1} Mbps) — Auto-Create upscales & restores crisp 1080p"),
                auto_fixable: true,
            });
        } else {
            score += 4;
            checks.push(GuidelineCheck {
                id: "quality",
                label: "1080p Vertical HD Quality",
                status: "fail",
                detail: format!("{width}×{height} ({bitrate_mbps:.1} Mbps) — low-res or non-9:16 triggers TikTok quality filter"),
                auto_fixable: true,
            });
        }

        // 2. Rival Caption Bars / Letterboxing Check (20 pts)
        if suggested_crop.is_none() {
            score += 20;
            checks.push(GuidelineCheck {
                id: "framing",
                label: "Zero Rival Bars / Letterboxing",
                status: "pass",
                detail: "Clean full-bleed frame — no letterbox bars or baked-in white caption headers detected".to_owned(),
                auto_fixable: false,
            });
        } else {
            score += 4;
            checks.push(GuidelineCheck {
                id: "framing",
                label: "Zero Rival Bars / Letterboxing",
                status: "fail",
                detail: crop_reason.unwrap_or_else(|| "Detected bars or rival caption headers".to_owned()),
                auto_fixable: true,
            });
        }

        // 3. Metadata & Third-Party App Hygiene (15 pts)
        if suspect_tags.is_empty() {
            score += 15;
            checks.push(GuidelineCheck {
                id: "metadata",
                label: "Clean Container Metadata",
                status: "pass",
                detail: "No TikTok, CapCut, Instagram, or downloader metadata signatures found".to_owned(),
                auto_fixable: false,
            });
        } else {
            score += 3;
            checks.push(GuidelineCheck {
                id: "metadata",
                label: "Clean Container Metadata",
                status: "fail",
                detail: format!("Found third-party tags ({}) — scrubbed automatically on export", suspect_tags.join(", ")),
                auto_fixable: true,
            });
        }

        // 4. Audio Loudness & Dead Air Check (15 pts)
        if !has_audio {
            score += 3;
            checks.push(GuidelineCheck {
                id: "audio",
                label: "Audio Polish & Dead-Air Pacing",
                status: "warn",
                detail: "No audio stream detected — silent clips have lower FYP retention".to_owned(),
                auto_fixable: false,
            });
        } else if silence_ratio > 0.25 {
            score += 7;
            checks.push(GuidelineCheck {
                id: "audio",
                label: "Audio Polish & Dead-Air Pacing",
                status: "warn",
                detail: format!("{:.0}% dead air detected (mean {:.1} dB) — Auto-Create jump-cuts silence & masters to -14 LUFS", silence_ratio * 100.0, mean_db.unwrap_or(-20.0)),
                auto_fixable: true,
            });
        } else if let Some(db) = mean_db && db < -24.0 {
            score += 9;
            checks.push(GuidelineCheck {
                id: "audio",
                label: "Audio Polish & Dead-Air Pacing",
                status: "warn",
                detail: format!("Quiet audio ({db:.1} dB) — Auto-Create boosts & masters to -14 LUFS"),
                auto_fixable: true,
            });
        } else {
            score += 15;
            checks.push(GuidelineCheck {
                id: "audio",
                label: "Audio Polish & Dead-Air Pacing",
                status: "pass",
                detail: format!("Clean audio ({:.1} dB · {:.0}% silence)", mean_db.unwrap_or(-14.0), silence_ratio * 100.0),
                auto_fixable: false,
            });
        }

        // 5. Dynamic Motion & Length Gate (15 pts)
        if duration_s < 5.0 {
            score += 3;
            checks.push(GuidelineCheck {
                id: "pacing",
                label: "Length & Dynamic Motion",
                status: "fail",
                detail: format!("{duration_s:.1}s is under TikTok's 5s FYP minimum length"),
                auto_fixable: false,
            });
        } else if frozen_s >= 2.2 {
            score += 6;
            checks.push(GuidelineCheck {
                id: "pacing",
                label: "Length & Dynamic Motion",
                status: "warn",
                detail: format!("{frozen_s:.1}s static/frozen frames detected — Auto-Create adds dynamic zoom motion"),
                auto_fixable: true,
            });
        } else {
            score += 15;
            checks.push(GuidelineCheck {
                id: "pacing",
                label: "Length & Dynamic Motion",
                status: "pass",
                detail: format!("{duration_s:.1}s active video — no static/frozen stretches"),
                auto_fixable: false,
            });
        }

        // 6. Transformative Edit Status (15 pts)
        if is_export {
            score += 15;
            checks.push(GuidelineCheck {
                id: "originality",
                label: "Transformative Edit Output",
                status: "pass",
                detail: "Rendered transformed export (reframed, metadata scrubbed, mastered)".to_owned(),
                auto_fixable: false,
            });
        } else {
            score += 0;
            checks.push(GuidelineCheck {
                id: "originality",
                label: "Transformative Edit Output",
                status: "fail",
                detail: "Raw untouched source clip — run Snapmatic, OpusClip, or Submagic to transform before posting".to_owned(),
                auto_fixable: true,
            });
        }

        let score = score.clamp(0, 100);
        let verdict = if score >= 82 {
            "PASS · TikTok FYP Ready"
        } else if score >= 55 {
            "NEEDS AUTO-FIX"
        } else {
            "HIGH RISK · Likely Restricted"
        };

        Ok(json!({
            "target": if is_export { "export" } else { "source" },
            "file_name": target_path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
            "score": score,
            "verdict": verdict,
            "width": width,
            "height": height,
            "fps": fps,
            "duration_s": duration_s,
            "bitrate_mbps": (bitrate_mbps * 10.0).round() / 10.0,
            "has_audio": has_audio,
            "silence_ratio": silence_ratio,
            "frozen_s": frozen_s,
            "suggested_crop": suggested_crop,
            "checks": checks,
            "auto_fix_plan": {
                "pre_crop": suggested_crop,
                "detail_restore": !is_1080p || !good_bitrate,
                "dynamic_zoom": true,
                "remove_mid_silences": has_audio && silence_ratio > 0.12,
                "studio_voice_enhance": has_audio,
                "color_grade": "crisp_rec709",
                "add_captions": has_audio,
            }
        }))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;

    Ok(Json(result))
}

// ============================================================================
// CLOUD AI HELPERS (OPUS.PRO & SUBMAGIC.CO)
// ============================================================================

struct TempFileGuard(PathBuf);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn plain_win_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_owned()
}

fn stage_clean_upload(app: &App, src: &Path) -> Result<TempFileGuard, String> {
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("mp4");
    let staged = exports_dir(app).join(format!(
        "cloud-upload-{}.{ext}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::copy(src, &staged)
        .map_err(|e| format!("Could not prepare clean clip copy for upload: {e}"))?;
    Ok(TempFileGuard(staged))
}

fn curl_json(args: &[String]) -> Result<(u16, Value, String), String> {
    let mut cmd = Command::new("curl.exe");
    cmd.arg("-sS")
        .arg("-g")
        .arg("--max-time")
        .arg("180")
        .arg("-w")
        .arg("\n__SNAP_HTTP_CODE__:%{http_code}")
        .args(args)
        .stdin(Stdio::null());
    hide_window(&mut cmd);
    let out = cmd
        .output()
        .map_err(|e| format!("Failed launching curl.exe: {e}"))?;
    let raw = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, code) = if let Some((b, c)) = raw.rsplit_once("\n__SNAP_HTTP_CODE__:") {
        (b.trim().to_owned(), c.trim().parse::<u16>().unwrap_or(0))
    } else {
        (raw.trim().to_owned(), 0)
    };
    if !out.status.success() && code == 0 {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(format!("Network request failed: {err}"));
    }
    let parsed = serde_json::from_str::<Value>(&body).unwrap_or(Value::Null);
    Ok((code, parsed, body))
}

fn download_cloud_mp4(url: &str, dest: &Path, auth_header: Option<&str>) -> Result<(), String> {
    let tmp = dest.with_extension("cloud.part");
    let _ = std::fs::remove_file(&tmp);
    let mut cmd = Command::new("curl.exe");
    cmd.args(["-fSL", "-g", "--retry", "2", "--max-time", "300"]);
    if let Some(h) = auth_header {
        cmd.args(["-H", h]);
    }
    cmd.arg("-o").arg(&tmp).arg(url).stdin(Stdio::null());
    hide_window(&mut cmd);
    let out = cmd
        .output()
        .map_err(|e| format!("Failed downloading rendered video: {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "Could not download rendered MP4: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    if std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0) < 16_000 {
        let _ = std::fs::remove_file(&tmp);
        return Err("Downloaded cloud render was empty or invalid.".to_owned());
    }
    // Scrub third-party metadata losslessly so TikTok Guidelines pass cleanly
    if let Some(ffmpeg) = find_ffmpeg() {
        let clean = dest.with_extension("clean.mp4");
        let mut fcmd = Command::new(&ffmpeg);
        fcmd.args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
            .arg(&tmp)
            .args(["-c", "copy", "-map_metadata", "-1", "-movflags", "+faststart"])
            .arg(&clean)
            .stdin(Stdio::null());
        hide_window(&mut fcmd);
        if fcmd.status().is_ok_and(|s| s.success()) && clean.is_file() {
            let _ = std::fs::remove_file(&tmp);
            let _ = std::fs::remove_file(dest);
            std::fs::rename(&clean, dest).map_err(|e| format!("Could not finalize MP4: {e}"))?;
            return Ok(());
        }
        let _ = std::fs::remove_file(&clean);
    }
    let _ = std::fs::remove_file(dest);
    std::fs::rename(&tmp, dest).map_err(|e| format!("Could not finalize MP4: {e}"))?;
    Ok(())
}

fn find_first_http_url(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if s.starts_with("https://") || s.starts_with("http://") => {
            Some(s.clone())
        }
        Value::Array(arr) => arr.iter().find_map(find_first_http_url),
        Value::Object(map) => {
            for key in [
                "directUrl",
                "downloadUrl",
                "uriForExport",
                "videoUrl",
                "mp4Url",
                "url",
            ] {
                if let Some(s) = map.get(key).and_then(Value::as_str)
                    && (s.starts_with("https://") || s.starts_with("http://"))
                {
                    return Some(s.to_owned());
                }
            }
            for key in ["exports", "data", "clips", "items", "result"] {
                if let Some(sub) = map.get(key)
                    && let Some(hit) = find_first_http_url(sub)
                {
                    return Some(hit);
                }
            }
            None
        }
        _ => None,
    }
}

fn start_cloud_guard(provider: &str, clip_id: &str, msg: &str) -> Result<(), (axum::http::StatusCode, String)> {
    let mut st = cloud_job_state().lock().map_err(internal)?;
    if st.active {
        return Err(bad(format!(
            "A {} job is already running. Cancel it or wait for it to finish.",
            st.provider
        )));
    }
    cloud_cancel_flag().store(false, Ordering::SeqCst);
    *st = CloudJobState {
        active: true,
        provider: provider.to_owned(),
        clip_id: clip_id.to_owned(),
        stage: "uploading".to_owned(),
        percent: 8,
        message: msg.to_owned(),
        error: None,
        result: None,
    };
    Ok(())
}

fn finish_cloud_job(res: Result<Value, String>) {
    if let Ok(mut st) = cloud_job_state().lock() {
        st.active = false;
        match res {
            Ok(v) => {
                st.percent = 100;
                st.stage = "done".to_owned();
                st.message = format!(
                    "✓ {} finished: {}",
                    st.provider,
                    v["file_name"].as_str().unwrap_or("Export ready")
                );
                st.error = None;
                st.result = Some(v);
            }
            Err(err) => {
                st.stage = if err.contains("Cancelled") {
                    "cancelled".to_owned()
                } else {
                    "error".to_owned()
                };
                st.message = err.clone();
                st.error = Some(err);
                st.result = None;
            }
        }
    }
}

// ============================================================================
// OPUS.PRO OFFICIAL CLOUD API WORKFLOW
// ============================================================================

pub(crate) async fn run_opus(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let creds = load_opus_creds().ok_or_else(|| {
        bad("Opus.pro API key not set. Click OpusClip to add your Opus.pro API key first.")
    })?;
    let src_path = resolve_video(&app, &clip_id).ok_or_else(|| bad("Clip video file not found."))?;
    start_cloud_guard("OpusClip", &clip_id, "Requesting Opus.pro upload session…")?;

    let app_bg = app.clone();
    let clip_bg = clip_id.clone();
    tokio::spawn(async move {
        let res = tokio::task::spawn_blocking(move || -> Result<Value, String> {
            let auth = format!("Authorization: Bearer {}", creds.api_key.trim());

            // Step 1: Request pre-signed upload link
            let (code, data, raw) = curl_json(&[
                "-X".into(),
                "POST".into(),
                "https://api.opus.pro/api/upload-links".into(),
                "-H".into(),
                "Accept: application/json".into(),
                "-H".into(),
                "Content-Type: application/json".into(),
                "-H".into(),
                auth.clone(),
                "-d".into(),
                r#"{"video":{"usecase":"LocalUpload"}}"#.into(),
            ])?;
            if code >= 400 || code == 0 {
                return Err(format!("Opus.pro auth/upload-link error (HTTP {code}): {}", raw.chars().take(240).collect::<String>()));
            }
            let gcs_url = data["url"]
                .as_str()
                .ok_or_else(|| format!("Opus.pro did not return an upload URL: {raw}"))?
                .to_owned();
            let upload_id = data["uploadId"]
                .as_str()
                .ok_or_else(|| format!("Opus.pro did not return an uploadId: {raw}"))?
                .to_owned();

            if cloud_cancel_flag().load(Ordering::SeqCst) {
                return Err("Cancelled by user.".to_owned());
            }

            // Step 2: Initiate GCS resumable upload session
            set_job_progress(20, "uploading", "Uploading clip to Opus.pro…");
            let staged_guard = stage_clean_upload(&app_bg, &src_path)?;
            let staged_str = plain_win_path(&staged_guard.0);
            let hdr_file = exports_dir(&app_bg).join(format!("opus-hdr-{}.txt", uuid::Uuid::new_v4().simple()));
            let mut init_cmd = Command::new("curl.exe");
            init_cmd
                .args([
                    "-sS",
                    "-g",
                    "-X",
                    "POST",
                    "-H",
                    "x-goog-resumable: start",
                    "-H",
                    "Content-Type: video/mp4",
                    "-H",
                    "Content-Length: 0",
                    "-D",
                ])
                .arg(&hdr_file)
                .arg("-o")
                .arg(if cfg!(windows) { "NUL" } else { "/dev/null" })
                .arg(&gcs_url)
                .stdin(Stdio::null());
            hide_window(&mut init_cmd);
            let _ = init_cmd.status();
            let location = std::fs::read_to_string(&hdr_file)
                .ok()
                .and_then(|h| {
                    h.lines().find_map(|l| {
                        let lower = l.to_ascii_lowercase();
                        lower
                            .strip_prefix("location:")
                            .map(|_| l.split_once(':').map(|(_, v)| v.trim().to_owned()))
                            .flatten()
                    })
                })
                .unwrap_or_else(|| gcs_url.clone());
            let _ = std::fs::remove_file(&hdr_file);

            // Step 3: Upload file bytes from clean ASCII staging path (--globoff prevents [id] glob errors)
            let mut put_cmd = Command::new("curl.exe");
            put_cmd
                .args([
                    "-fSL",
                    "-g",
                    "--max-time",
                    "300",
                    "-X",
                    "PUT",
                    "-H",
                    "Content-Type: video/mp4",
                    "-T",
                ])
                .arg(&staged_str)
                .arg(&location)
                .stdin(Stdio::null());
            hide_window(&mut put_cmd);
            let put_out = put_cmd
                .output()
                .map_err(|e| format!("Opus.pro file upload failed: {e}"))?;
            drop(staged_guard);
            if !put_out.status.success() {
                return Err(format!(
                    "Opus.pro upload failed: {}",
                    String::from_utf8_lossy(&put_out.stderr).trim()
                ));
            }

            if cloud_cancel_flag().load(Ordering::SeqCst) {
                return Err("Cancelled by user.".to_owned());
            }

            // Step 4: Create clip project
            set_job_progress(40, "processing", "Opus.pro AI is creating your viral edit…");
            let mut body_obj = json!({ "videoUrl": upload_id });
            if !creds.brand_template_id.trim().is_empty() {
                body_obj["brandTemplateId"] = json!(creds.brand_template_id.trim());
            }
            let (code, proj, raw) = curl_json(&[
                "-X".into(),
                "POST".into(),
                "https://api.opus.pro/api/clip-projects".into(),
                "-H".into(),
                "Accept: application/json".into(),
                "-H".into(),
                "Content-Type: application/json".into(),
                "-H".into(),
                auth.clone(),
                "-d".into(),
                body_obj.to_string(),
            ])?;
            if code >= 400 || code == 0 {
                return Err(format!(
                    "Opus.pro create project failed (HTTP {code}): {}",
                    raw.chars().take(260).collect::<String>()
                ));
            }
            let project_id = proj["projectId"]
                .as_str()
                .or_else(|| proj["id"].as_str())
                .or_else(|| proj["data"]["projectId"].as_str())
                .or_else(|| proj["data"]["id"].as_str())
                .ok_or_else(|| format!("Opus.pro did not return a projectId: {raw}"))?
                .to_owned();

            // Step 5: Poll until exportable clips are ready
            let mut download_url: Option<String> = None;
            for attempt in 1..=120 {
                if cloud_cancel_flag().load(Ordering::SeqCst) {
                    return Err("Cancelled by user.".to_owned());
                }
                std::thread::sleep(std::time::Duration::from_secs(5));
                let pct = (42 + (attempt * 45 / 120)).min(88) as u8;
                set_job_progress(
                    pct,
                    "processing",
                    format!("Opus.pro AI processing ({}s)…", attempt * 5),
                );

                // Check project status
                if let Ok((pcode, pdata, _)) = curl_json(&[
                    format!("https://api.opus.pro/api/clip-projects/{project_id}"),
                    "-H".into(),
                    "Accept: application/json".into(),
                    "-H".into(),
                    auth.clone(),
                ]) && pcode < 400
                {
                    let st = pdata["stage"]
                        .as_str()
                        .or_else(|| pdata["status"].as_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if st.contains("fail") || st.contains("error") {
                        return Err(format!("Opus.pro processing failed: {pdata}"));
                    }
                    if let Some(u) = find_first_http_url(&pdata)
                        && (st.contains("done") || st.contains("complet") || st.contains("ready"))
                    {
                        download_url = Some(u);
                        break;
                    }
                }

                // Query exportable clips
                if let Ok((ccode, cdata, _)) = curl_json(&[
                    format!("https://api.opus.pro/api/exportable-clips?q=findByProjectId&projectId={project_id}"),
                    "-H".into(),
                    "Accept: application/json".into(),
                    "-H".into(),
                    auth.clone(),
                ]) && ccode < 400
                    && let Some(u) = find_first_http_url(&cdata)
                {
                    download_url = Some(u);
                    break;
                }
            }

            let final_url = download_url.ok_or_else(|| {
                "Opus.pro timed out or returned no clips (note: OpusClip works best on clips over 45–60s).".to_owned()
            })?;

            // Step 6: Download finished render and register in Snapmatic
            set_job_progress(92, "downloading", "Downloading Opus.pro render & scrubbing metadata…");
            let stem = src_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("clip")
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
                .take(40)
                .collect::<String>();
            let short = &uuid::Uuid::new_v4().simple().to_string()[..8];
            let file_name = format!("{stem}-opus-{short}.mp4");
            let dest = exports_dir(&app_bg).join(&file_name);
            download_cloud_mp4(&final_url, &dest, None)?;
            let bytes = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            register_external_export(&app_bg, &clip_bg, &file_name, "OpusClip (opus.pro)", bytes)
                .map_err(|(_, msg)| msg)
        })
        .await
        .unwrap_or_else(|e| Err(format!("Opus.pro worker error: {e}")));
        finish_cloud_job(res);
    });

    Ok(Json(json!({ "started": true, "provider": "opus" })))
}

// ============================================================================
// SUBMAGIC.CO OFFICIAL CLOUD API WORKFLOW
// ============================================================================

pub(crate) async fn run_submagic(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let creds = load_submagic_creds().ok_or_else(|| {
        bad("Submagic API key not set. Click Submagic to add your Submagic API key first.")
    })?;
    let src_path = resolve_video(&app, &clip_id).ok_or_else(|| bad("Clip video file not found."))?;
    start_cloud_guard("Submagic", &clip_id, "Uploading clip to Submagic AI…")?;

    let app_bg = app.clone();
    let clip_bg = clip_id.clone();
    tokio::spawn(async move {
        let res = tokio::task::spawn_blocking(move || -> Result<Value, String> {
            let key_hdr = format!("x-api-key: {}", creds.api_key.trim());
            let title = src_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Snapmatic Clip")
                .chars()
                .take(60)
                .collect::<String>();
            let staged_guard = stage_clean_upload(&app_bg, &src_path)?;
            let staged_str = plain_win_path(&staged_guard.0);
            let file_arg = format!("file=@{staged_str};type=video/mp4");
            let tpl = if creds.template_name.trim().is_empty() {
                default_submagic_template()
            } else {
                creds.template_name.trim().to_owned()
            };

            set_job_progress(18, "uploading", "Uploading clip to Submagic AI (Magic B-Roll + Zooms + Captions)…");
            let (code, created, raw) = curl_json(&[
                "-X".into(),
                "POST".into(),
                "https://api.submagic.co/v1/projects/upload".into(),
                "-H".into(),
                key_hdr.clone(),
                "-F".into(),
                file_arg,
                "--form-string".into(),
                format!("title={title}"),
                "--form-string".into(),
                "language=en".into(),
                "--form-string".into(),
                format!("templateName={tpl}"),
                "--form-string".into(),
                format!("magicBrolls={}", if creds.magic_brolls { "true" } else { "false" }),
                "--form-string".into(),
                format!("magicZooms={}", if creds.magic_zooms { "true" } else { "false" }),
            ])?;
            drop(staged_guard);
            if code >= 400 || code == 0 {
                return Err(format!(
                    "Submagic upload failed (HTTP {code}): {}",
                    raw.chars().take(260).collect::<String>()
                ));
            }
            let proj_id = created["id"]
                .as_str()
                .or_else(|| created["projectId"].as_str())
                .or_else(|| created["data"]["id"].as_str())
                .ok_or_else(|| format!("Submagic did not return a project id: {raw}"))?
                .to_owned();

            // Poll GET /v1/projects/{id}
            let mut download_url: Option<String> = None;
            for attempt in 1..=150 {
                if cloud_cancel_flag().load(Ordering::SeqCst) {
                    return Err("Cancelled by user.".to_owned());
                }
                std::thread::sleep(std::time::Duration::from_secs(4));
                let pct = (30 + (attempt * 58 / 150)).min(88) as u8;
                set_job_progress(
                    pct,
                    "processing",
                    format!("Submagic rendering Magic B-Roll, Zooms & Captions ({}s)…", attempt * 4),
                );
                let (pcode, pdata, praw) = curl_json(&[
                    format!("https://api.submagic.co/v1/projects/{proj_id}"),
                    "-H".into(),
                    key_hdr.clone(),
                ])?;
                if pcode >= 400 {
                    return Err(format!(
                        "Submagic status error (HTTP {pcode}): {}",
                        praw.chars().take(220).collect::<String>()
                    ));
                }
                let st = pdata["status"]
                    .as_str()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if st == "failed" || st == "error" {
                    return Err(format!("Submagic processing failed: {pdata}"));
                }
                if st == "completed" || st == "done" || st == "exported" {
                    if let Some(u) = find_first_http_url(&pdata) {
                        download_url = Some(u);
                        break;
                    }
                }
            }

            let final_url = download_url.ok_or_else(|| {
                "Submagic timed out waiting for the rendered download URL.".to_owned()
            })?;

            set_job_progress(92, "downloading", "Downloading Submagic render & scrubbing metadata…");
            let stem = src_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("clip")
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
                .take(40)
                .collect::<String>();
            let short = &uuid::Uuid::new_v4().simple().to_string()[..8];
            let file_name = format!("{stem}-submagic-{short}.mp4");
            let dest = exports_dir(&app_bg).join(&file_name);
            download_cloud_mp4(&final_url, &dest, None)?;
            let bytes = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            register_external_export(&app_bg, &clip_bg, &file_name, "Submagic AI", bytes)
                .map_err(|(_, msg)| msg)
        })
        .await
        .unwrap_or_else(|e| Err(format!("Submagic worker error: {e}")));
        finish_cloud_job(res);
    });

    Ok(Json(json!({ "started": true, "provider": "submagic" })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_third_party_metadata_tags() {
        let mut clean = serde_json::Map::new();
        clean.insert("major_brand".into(), json!("isom"));
        assert!(detect_suspicious_tags(&clean).is_empty());

        let mut dirty = serde_json::Map::new();
        dirty.insert("encoder".into(), json!("CapCut 3.2"));
        dirty.insert("comment".into(), json!("vid:v09044g40000"));
        let hits = detect_suspicious_tags(&dirty);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn parses_fps_ratios_and_extracts_cloud_urls() {
        assert!((parse_fps("30000/1001") - 29.97).abs() < 0.02);
        assert_eq!(parse_fps("60"), 60.0);
        let sub_resp = json!({
            "id": "abc",
            "status": "completed",
            "exports": [{"directUrl": "https://cdn.example.com/out.mp4"}]
        });
        assert_eq!(
            find_first_http_url(&sub_resp).as_deref(),
            Some("https://cdn.example.com/out.mp4")
        );
    }
}
