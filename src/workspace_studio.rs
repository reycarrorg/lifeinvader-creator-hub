use crate::workspace_film_roll::{
    escape_drawtext, exports_dir, find_ffmpeg, find_overlay_font, probe_mp4_summary, resolve_video,
    sanitize_stem, stable_id,
};
use crate::{ApiResult, App, bad, internal, source};
use axum::{
    Json,
    extract::{Multipart, Path as AxumPath, State},
    http::header,
    response::IntoResponse,
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize, Clone, Default)]
pub(crate) struct CommentaryCard {
    #[serde(default)]
    pub(crate) start_s: f64,
    #[serde(default)]
    pub(crate) end_s: f64,
    #[serde(default)]
    pub(crate) text: String,
}

#[derive(Deserialize, Serialize, Clone, Default)]
pub(crate) struct SfxCue {
    #[serde(default)]
    pub(crate) at_s: f64,
    #[serde(default)]
    pub(crate) kind: String,
}

#[derive(Deserialize, Clone, Default)]
pub(crate) struct StudioExportOptions {
    #[serde(default)]
    pub(crate) voiceover_path: Option<String>,
    #[serde(default)]
    pub(crate) voiceover_duck_db: Option<f64>,
    #[serde(default)]
    pub(crate) voiceover_gain_db: Option<f64>,
    #[serde(default)]
    pub(crate) voice_fx: Option<String>,
    #[serde(default)]
    pub(crate) reaction_cam_path: Option<String>,
    #[serde(default)]
    pub(crate) reaction_cam_layout: Option<String>,
    #[serde(default)]
    pub(crate) freeze_at_ms: Option<u64>,
    #[serde(default)]
    pub(crate) freeze_dur_s: Option<f64>,
    #[serde(default)]
    pub(crate) freeze_callout: Option<String>,
    #[serde(default)]
    pub(crate) commentary_cards: Vec<CommentaryCard>,
    #[serde(default)]
    pub(crate) credit_handle: Option<String>,
    #[serde(default)]
    pub(crate) sfx_cues: Vec<SfxCue>,
    #[serde(default)]
    pub(crate) intro_sting_text: Option<String>,
    #[serde(default)]
    pub(crate) outro_card_text: Option<String>,
    #[serde(default)]
    pub(crate) zoom_punch_at_ms: Option<u64>,
    #[serde(default)]
    pub(crate) zoom_punch_dur_s: Option<f64>,
    #[serde(default)]
    pub(crate) zoom_punch_scale: Option<f64>,
    #[serde(default)]
    pub(crate) replay_start_ms: Option<u64>,
    #[serde(default)]
    pub(crate) replay_end_ms: Option<u64>,
    #[serde(default)]
    pub(crate) replay_label: Option<String>,
    #[serde(default)]
    pub(crate) poll_prompt: Option<String>,
    #[serde(default)]
    pub(crate) progress_bar: bool,
    #[serde(default)]
    pub(crate) countdown_at_ms: Option<u64>,
    #[serde(default)]
    pub(crate) color_grade: Option<String>,
    #[serde(default)]
    pub(crate) weazel_headline: Option<String>,
    #[serde(default)]
    pub(crate) weazel_ticker: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct TtsRequest {
    text: String,
    #[serde(default)]
    rate: Option<i32>,
    #[serde(default)]
    voice: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct BeatDetectRequest {
    #[serde(default)]
    audio_path: Option<String>,
    #[serde(default)]
    clip_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct MontageRequest {
    clip_ids: Vec<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    clip_sec: Option<f64>,
    #[serde(default)]
    beat_sync: bool,
    #[serde(default)]
    music_path: Option<String>,
    #[serde(default)]
    music_volume_db: Option<f64>,
    #[serde(default)]
    color_grade: Option<String>,
    #[serde(default)]
    progress_bar: bool,
    #[serde(default)]
    intro_sting_text: Option<String>,
    #[serde(default)]
    outro_card_text: Option<String>,
    #[serde(default)]
    credit_handle: Option<String>,
    #[serde(default)]
    weazel_headline: Option<String>,
    #[serde(default)]
    quality: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct CompareRequest {
    clip_a_id: String,
    clip_b_id: String,
    #[serde(default)]
    layout: Option<String>,
    #[serde(default)]
    label_a: Option<String>,
    #[serde(default)]
    label_b: Option<String>,
    #[serde(default)]
    title_banner: Option<String>,
    #[serde(default)]
    max_seconds: Option<f64>,
    #[serde(default)]
    color_grade: Option<String>,
    #[serde(default)]
    quality: Option<String>,
}

pub(crate) fn studio_media_dir(app: &App) -> PathBuf {
    let dir = app.media.join("studio_media");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn hide_window(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
}

fn write_wav_pcm16_stereo(path: &Path, sample_rate: u32, samples: &[(f32, f32)]) -> Result<(), String> {
    let data_bytes = (samples.len() * 4) as u32;
    let mut buf = Vec::with_capacity(44 + data_bytes as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&2u16.to_le_bytes()); // stereo
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&(sample_rate * 4).to_le_bytes());
    buf.extend_from_slice(&4u16.to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_bytes.to_le_bytes());
    for &(l, r) in samples {
        let li = (l.clamp(-1.0, 1.0) * 32760.0) as i16;
        let ri = (r.clamp(-1.0, 1.0) * 32760.0) as i16;
        buf.extend_from_slice(&li.to_le_bytes());
        buf.extend_from_slice(&ri.to_le_bytes());
    }
    std::fs::write(path, &buf).map_err(|e| format!("Could not write SFX wav: {e}"))
}

pub(crate) fn ensure_sfx_file(app: &App, kind: &str) -> Result<PathBuf, String> {
    let clean = match kind.trim().to_ascii_lowercase().as_str() {
        "whoosh" => "whoosh",
        "boom" => "boom",
        "ding" => "ding",
        "riser" => "riser",
        "scratch" => "scratch",
        _ => return Err("Unknown SFX kind.".to_owned()),
    };
    let sfx_dir = studio_media_dir(app).join("sfx");
    let _ = std::fs::create_dir_all(&sfx_dir);
    let path = sfx_dir.join(format!("{clean}.wav"));
    if path.is_file() && std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > 1000 {
        return Ok(path);
    }
    let sr = 48_000u32;
    let sr_f = sr as f32;
    let mut seed: u64 = 0x5EED_CAFE_1234_5678;
    let mut next_noise = || -> f32 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let bits = ((seed >> 33) as u32) as f32 / (u32::MAX as f32);
        bits * 2.0 - 1.0
    };
    let mut samples: Vec<(f32, f32)> = Vec::new();
    match clean {
        "whoosh" => {
            let n = (sr_f * 0.48) as usize;
            let mut lp = 0.0f32;
            for i in 0..n {
                let u = i as f32 / n as f32;
                let env = (std::f32::consts::PI * u).sin().powf(1.4);
                let alpha = 0.04 + 0.32 * (std::f32::consts::PI * u).sin();
                let raw = next_noise();
                lp += alpha * (raw - lp);
                let tone = (2.0 * std::f32::consts::PI * (140.0 + 380.0 * u) * (i as f32 / sr_f)).sin() * 0.14;
                let v = (lp * 0.72 + tone) * env * 0.85;
                samples.push((v * (1.0 - 0.15 * u), v * (0.85 + 0.15 * u)));
            }
        }
        "boom" => {
            let n = (sr_f * 0.90) as usize;
            let mut phase = 0.0f32;
            for i in 0..n {
                let t = i as f32 / sr_f;
                let u = i as f32 / n as f32;
                let freq = 34.0 + 82.0 * (-t * 9.5).exp();
                phase += 2.0 * std::f32::consts::PI * freq / sr_f;
                let sub = phase.sin();
                let click = if t < 0.02 { next_noise() * (1.0 - t / 0.02) * 0.45 } else { 0.0 };
                let env = (1.0 - (-t * 120.0).exp()) * (-t * 3.8).exp() * (1.0 - u * 0.2);
                let v = ((sub * 1.35).tanh() * 0.88 + click) * env;
                samples.push((v, v));
            }
        }
        "ding" => {
            let n = (sr_f * 0.65) as usize;
            for i in 0..n {
                let t = i as f32 / sr_f;
                let env = (1.0 - (-t * 250.0).exp()) * (-t * 6.5).exp();
                let f1 = (2.0 * std::f32::consts::PI * 1318.5 * t).sin();
                let f2 = (2.0 * std::f32::consts::PI * 2637.0 * t).sin() * 0.45;
                let f3 = (2.0 * std::f32::consts::PI * 3955.5 * t).sin() * 0.18;
                let v = (f1 + f2 + f3) * env * 0.48;
                samples.push((v, v));
            }
        }
        "riser" => {
            let n = (sr_f * 1.20) as usize;
            let mut phase = 0.0f32;
            let mut lp = 0.0f32;
            for i in 0..n {
                let u = i as f32 / n as f32;
                let freq = 180.0 * (8.5f32).powf(u);
                phase += 2.0 * std::f32::consts::PI * freq / sr_f;
                let saw = (phase / std::f32::consts::PI).rem_euclid(2.0) - 1.0;
                let raw = next_noise();
                lp += (0.04 + 0.35 * u) * (raw - lp);
                let env = u.powf(1.6) * if u > 0.94 { (1.0 - u) / 0.06 } else { 1.0 };
                let v = (saw * 0.32 + lp * 0.45) * env * 0.72;
                samples.push((v, v));
            }
        }
        _ => {
            // scratch
            let n = (sr_f * 0.38) as usize;
            let mut phase = 0.0f32;
            for i in 0..n {
                let u = i as f32 / n as f32;
                let freq = if u < 0.45 {
                    1800.0 - 1500.0 * (u / 0.45)
                } else {
                    300.0 + 1700.0 * ((u - 0.45) / 0.55)
                };
                phase += 2.0 * std::f32::consts::PI * freq / sr_f;
                let env = (std::f32::consts::PI * u).sin().powf(0.7);
                let v = ((phase.sin() * 2.2).tanh() * 0.52 + next_noise() * 0.12) * env * 0.68;
                samples.push((v, v));
            }
        }
    }
    write_wav_pcm16_stereo(&path, sr, &samples)?;
    Ok(path)
}

pub(crate) async fn stream_sfx(
    State(app): State<App>,
    AxumPath(kind): AxumPath<String>,
) -> ApiResult<impl IntoResponse> {
    let app_clone = app.clone();
    let kind_clone = kind.clone();
    let path = tokio::task::spawn_blocking(move || ensure_sfx_file(&app_clone, &kind_clone))
        .await
        .map_err(internal)?
        .map_err(bad)?;
    let bytes = std::fs::read(&path).map_err(internal)?;
    Ok((
        [
            (header::CONTENT_TYPE, "audio/wav"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        bytes,
    ))
}

pub(crate) async fn upload_studio_media(
    State(app): State<App>,
    mut multipart: Multipart,
) -> ApiResult<Json<Value>> {
    let mut kind = "voiceover".to_owned();
    let mut orig_name = String::new();
    let mut data_opt: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| bad("Invalid multipart upload."))?
    {
        let name = field.name().unwrap_or("").to_owned();
        if name == "kind" {
            if let Ok(txt) = field.text().await {
                let k = txt.trim().to_ascii_lowercase();
                if matches!(k.as_str(), "voiceover" | "reaction_cam" | "sfx") {
                    kind = k;
                }
            }
        } else if name == "file" || data_opt.is_none() {
            if let Some(fname) = field.file_name() {
                orig_name = fname.to_owned();
            }
            let bytes = field
                .bytes()
                .await
                .map_err(|_| bad("Recorded media exceeded upload limit (120 MiB)."))?;
            if bytes.is_empty() || bytes.len() > 120 * 1024 * 1024 {
                return Err(bad("Studio media must be between 1 byte and 120 MiB."));
            }
            data_opt = Some(bytes.to_vec());
        }
    }
    let Some(data) = data_opt else {
        return Err(bad("No audio or video file provided."));
    };
    let ext = Path::new(&orig_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .filter(|e| matches!(e.as_str(), "webm" | "wav" | "mp3" | "m4a" | "ogg" | "mp4" | "mov"))
        .unwrap_or_else(|| if kind == "reaction_cam" { "webm".to_owned() } else { "webm".to_owned() });

    let dir = studio_media_dir(&app);
    let id = uuid::Uuid::new_v4().simple().to_string();
    let file_name = format!("{kind}-{id}.{ext}");
    let dest = dir.join(&file_name);
    let len = data.len() as u64;
    std::fs::write(&dest, &data).map_err(internal)?;

    Ok(Json(json!({
        "id": id,
        "kind": kind,
        "file_name": file_name,
        "path": dest.to_string_lossy(),
        "bytes": len
    })))
}

pub(crate) async fn generate_tts(
    State(app): State<App>,
    Json(input): Json<TtsRequest>,
) -> ApiResult<Json<Value>> {
    let text = input.text.trim().to_owned();
    if text.is_empty() {
        return Err(bad("Enter narration script text first."));
    }
    let clean_text: String = text.chars().take(1200).collect();
    let rate = input.rate.unwrap_or(1).clamp(-5, 6);
    let voice_pref = input.voice.unwrap_or_default().trim().to_owned();
    let dir = studio_media_dir(&app);
    let id = uuid::Uuid::new_v4().simple().to_string();
    let wav_path = dir.join(format!("tts-{id}.wav"));

    let res = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let mut engine = "windows-sapi".to_owned();
        // Check optional local piper.exe + .onnx model in tools/
        let piper_bin = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|d| d.join("tools").join("piper.exe")))
            .filter(|p| p.is_file())
            .or_else(|| {
                let p = PathBuf::from("tools").join("piper.exe");
                p.is_file().then_some(p)
            });
        let piper_model = piper_bin.as_ref().and_then(|bin| {
            bin.parent().and_then(|dir| {
                std::fs::read_dir(dir).ok().and_then(|entries| {
                    entries
                        .flatten()
                        .map(|e| e.path())
                        .find(|p| p.extension().and_then(|x| x.to_str()) == Some("onnx"))
                })
            })
        });

        let mut produced = false;
        if let (Some(piper), Some(model)) = (piper_bin, piper_model) {
            let mut cmd = std::process::Command::new(&piper);
            cmd.arg("--model")
                .arg(&model)
                .arg("--output_file")
                .arg(&wav_path)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            hide_window(&mut cmd);
            if let Ok(mut child) = cmd.spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    use std::io::Write;
                    let _ = stdin.write_all(clean_text.as_bytes());
                }
                if child.wait().is_ok_and(|s| s.success()) && wav_path.is_file() {
                    engine = "piper-local".to_owned();
                    produced = true;
                }
            }
        }

        if !produced {
            #[cfg(windows)]
            {
                let txt_file = wav_path.with_extension("txt");
                std::fs::write(&txt_file, &clean_text)
                    .map_err(|e| format!("Could not write TTS script: {e}"))?;
                let ps_script = r#"
param($txtPath, $wavPath, $rateVal, $voiceHint)
Add-Type -AssemblyName System.Speech
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer
$s.Rate = [int]$rateVal
if ($voiceHint) {
  $hit = $s.GetInstalledVoices() | Where-Object { $_.Enabled -and $_.VoiceInfo.Name -like "*$voiceHint*" } | Select-Object -First 1
  if ($hit) { $s.SelectVoice($hit.VoiceInfo.Name) }
}
$s.SetOutputToWaveFile($wavPath)
$content = Get-Content -LiteralPath $txtPath -Raw -Encoding UTF8
$s.Speak($content)
$s.Dispose()
"#;
                let ps_file = wav_path.with_extension("ps1");
                let _ = std::fs::write(&ps_file, ps_script);
                let mut cmd2 = std::process::Command::new("powershell");
                cmd2.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                    .arg(&ps_file)
                    .arg(&txt_file)
                    .arg(&wav_path)
                    .arg(rate.to_string())
                    .arg(&voice_pref);
                hide_window(&mut cmd2);
                let out = cmd2
                    .output()
                    .map_err(|e| format!("Failed to invoke Windows Speech Synthesizer: {e}"))?;
                let _ = std::fs::remove_file(&txt_file);
                let _ = std::fs::remove_file(&ps_file);
                if !out.status.success() || !wav_path.is_file() {
                    let err = String::from_utf8_lossy(&out.stderr);
                    return Err(format!("Local TTS generation failed: {}", err.trim()));
                }
            }
            #[cfg(not(windows))]
            {
                return Err("Local Windows TTS is only available on Windows without Piper.".to_owned());
            }
        }

        let bytes = std::fs::metadata(&wav_path).map(|m| m.len()).unwrap_or(0);
        if bytes < 100 {
            return Err("TTS produced an empty audio file.".to_owned());
        }
        let dur_ms = probe_mp4_summary(&wav_path)
            .map(|s| (s.duration_s * 1000.0).round() as u64)
            .unwrap_or(((bytes.saturating_sub(44)) as f64 / 44100.0 * 1000.0) as u64);
        Ok(json!({
            "id": id,
            "path": wav_path.to_string_lossy(),
            "engine": engine,
            "duration_ms": dur_ms,
            "bytes": bytes
        }))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;

    Ok(Json(res))
}

pub(crate) async fn detect_beats(
    State(app): State<App>,
    Json(input): Json<BeatDetectRequest>,
) -> ApiResult<Json<Value>> {
    let target_path = if let Some(p) = input.audio_path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pb = PathBuf::from(p);
        if !pb.is_file() {
            return Err(bad("Audio file not found."));
        }
        pb
    } else if let Some(cid) = input.clip_id.as_deref() {
        if !crate::workspace_media::hex(cid) {
            return Err(bad("Invalid clip ID."));
        }
        resolve_video(&app, cid).ok_or_else(|| bad("Source clip not found."))?
    } else {
        return Err(bad("Provide a music track or select a clip for beat detection."));
    };
    let Some(ffmpeg) = find_ffmpeg() else {
        return Err(bad("FFmpeg not found in tools/ffmpeg.exe."));
    };

    let result = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let mut cmd = std::process::Command::new(&ffmpeg);
        cmd.args(["-hide_banner", "-nostats", "-t", "60", "-i"])
            .arg(&target_path)
            .args([
                "-vn",
                "-af",
                "aresample=16000,asetnsamples=1600,astats=metadata=1:reset=1,ametadata=print:key=lavfi.astats.Overall.RMS_level:file=-",
                "-f",
                "null",
                "-",
            ]);
        hide_window(&mut cmd);
        let out = cmd
            .output()
            .map_err(|e| format!("Beat analysis failed: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut frames: Vec<(u64, f64)> = Vec::new();
        let mut cur_ms = 0u64;
        for line in text.lines() {
            if let Some(rest) = line.split("pts_time:").nth(1)
                && let Some(t) = rest.split_whitespace().next().and_then(|s| s.parse::<f64>().ok())
            {
                cur_ms = (t.max(0.0) * 1000.0).round() as u64;
            } else if let Some(v) = line.strip_prefix("lavfi.astats.Overall.RMS_level=")
                && let Ok(db) = v.trim().parse::<f64>()
                && db.is_finite()
            {
                let e = ((db + 55.0) / 55.0).clamp(0.0, 1.0);
                frames.push((cur_ms, e));
            }
        }
        let mut beats_ms: Vec<u64> = Vec::new();
        for i in 2..frames.len().saturating_sub(1) {
            let prev_avg = (frames[i - 2].1 + frames[i - 1].1) * 0.5;
            let cur = frames[i].1;
            let next = frames[i + 1].1;
            if cur > prev_avg + 0.08 && cur >= next && cur > 0.30 {
                let t = frames[i].0;
                if beats_ms.last().is_none_or(|&last| t.saturating_sub(last) >= 350) {
                    beats_ms.push(t);
                }
            }
        }
        if beats_ms.len() < 4 {
            // Fallback steady 120 BPM grid (500ms)
            beats_ms = (0..40).map(|k| k * 500).collect();
        }
        let avg_gap_ms = if beats_ms.len() >= 2 {
            let mut gaps: Vec<u64> = beats_ms.windows(2).map(|w| w[1].saturating_sub(w[0])).collect();
            gaps.sort();
            gaps[gaps.len() / 2].max(250)
        } else {
            500
        };
        let bpm = (60_000.0 / avg_gap_ms as f64).round().clamp(65.0, 185.0) as u32;
        Ok(json!({
            "beats_ms": beats_ms.into_iter().take(64).collect::<Vec<_>>(),
            "bpm_est": bpm
        }))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;

    Ok(Json(result))
}

pub(crate) fn color_grade_filter(grade: Option<&str>) -> Option<&'static str> {
    match grade.map(str::trim).unwrap_or("none") {
        "vice_sunset" => Some("eq=contrast=1.10:saturation=1.28:brightness=0.01,colorbalance=rs=0.10:bs=0.08:rm=0.06:bm=0.05:rh=0.08:bh=-0.04"),
        "miami_gold" => Some("eq=contrast=1.06:saturation=1.22:brightness=0.03,colorbalance=rs=0.08:gs=0.03:bs=-0.06:rh=0.10:gh=0.05:bh=-0.08"),
        "night_heist" => Some("eq=contrast=1.16:saturation=0.92:brightness=-0.03,colorbalance=rs=-0.06:gs=0.02:bs=0.14:rm=-0.04:bm=0.08"),
        "vhs_1986" => Some("eq=contrast=1.08:saturation=1.15:gamma=1.04,colorbalance=rs=0.06:bs=0.08,noise=alls=14:allf=t+u"),
        "ultra_crisp" => Some("eq=contrast=1.07:saturation=1.12,unsharp=5:5:0.85:5:5:0.0"),
        _ => None,
    }
}

pub(crate) fn voice_fx_filter(fx: Option<&str>) -> Option<&'static str> {
    match fx.map(str::trim).unwrap_or("none") {
        "podcast" => Some("highpass=f=85,lowpass=f=13000,equalizer=f=220:width_type=o:width=1.2:g=2.5,equalizer=f=3500:width_type=o:width=1.5:g=3,acompressor=threshold=-18dB:ratio=3.5:attack=10:release=120,alimiter=limit=0.95"),
        "hype_radio" => Some("highpass=f=100,equalizer=f=2500:width_type=o:width=1.4:g=4.5,acompressor=threshold=-20dB:ratio=5:attack=5:release=80,volume=2dB"),
        "police_scanner" => Some("highpass=f=450,lowpass=f=2600,acompressor=threshold=-22dB:ratio=6"),
        "trailer_deep" => Some("asetrate=48000*0.86,aresample=48000,atempo=1.1628,bass=g=5:f=110,acompressor=threshold=-16dB:ratio=3"),
        _ => None,
    }
}

pub(crate) fn build_studio_video_filters(opts: &StudioExportOptions, out_dur_s: f64) -> Vec<String> {
    let mut filters = Vec::new();
    let font_opt = find_overlay_font()
        .map(|f| format!("fontfile='{f}':"))
        .unwrap_or_default();

    // #15 Cinematic Color Grade
    if let Some(cg) = color_grade_filter(opts.color_grade.as_deref()) {
        filters.push(cg.to_owned());
    }

    // #10 Keyframe Action Zoom / Punch-In (smooth triangle pulse around timestamp)
    if let Some(punch_ms) = opts.zoom_punch_at_ms {
        let t0 = (punch_ms as f64 / 1000.0).max(0.0);
        let dur = opts.zoom_punch_dur_s.unwrap_or(2.2).clamp(0.6, 8.0);
        let peak_extra = (opts.zoom_punch_scale.unwrap_or(1.35).clamp(1.12, 1.75)) - 1.0;
        let half = dur * 0.5;
        let t1 = t0 + dur;
        // Single-pass smooth zoompan preserving 30fps and frame dimensions
        filters.push(format!(
            "zoompan=z='if(between(in_time\\,{t0:.2}\\,{t1:.2})\\,1+{peak_extra:.3}*(1-abs(in_time-{mid:.2})/{half:.2})\\,1)':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':d=1:s=1080x1920:fps=30",
            mid = t0 + half
        ));
    }

    // #5 Creator Attribution Pill
    if let Some(credit) = opts.credit_handle.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let clean = escape_drawtext(&format!("Clip via {}", credit.chars().take(36).collect::<String>()));
        filters.push(format!(
            "drawtext={font_opt}expansion=none:text='{clean}':fontcolor=white@0.92:fontsize=max(16\\,min(w/30\\,28)):box=1:boxcolor=black@0.62:boxborderw=10:x=w*0.04:y=h*0.04"
        ));
    }

    // #5 Timed Commentary Pop-Up Cards
    for card in opts.commentary_cards.iter().take(12) {
        let txt = card.text.trim();
        if txt.is_empty() || card.end_s <= card.start_s {
            continue;
        }
        let safe = escape_drawtext(&txt.chars().take(72).collect::<String>());
        filters.push(format!(
            "drawtext={font_opt}expansion=none:enable='between(t\\,{:.2}\\,{:.2})':text='{safe}':fontcolor=#00E5FF:fontsize=max(22\\,min(w/22\\,40)):box=1:boxcolor=#090812@0.86:boxborderw=16:x=(w-text_w)/2:y=h*0.21",
            card.start_s.max(0.0),
            card.end_s
        ));
    }

    // #14 Payoff 3-2-1 Countdown
    if let Some(cd_ms) = opts.countdown_at_ms {
        let target_s = (cd_ms as f64 / 1000.0).max(3.0);
        for (idx, num) in [(3.0, "3"), (2.0, "2"), (1.0, "1")] {
            let st = (target_s - idx).max(0.0);
            let en = (st + 0.92).max(st + 0.2);
            filters.push(format!(
                "drawtext={font_opt}expansion=none:enable='between(t\\,{st:.2}\\,{en:.2})':text='{num}':fontcolor=#FF2E93:borderw=4:bordercolor=white:fontsize=max(76\\,min(w/7\\,140)):x=(w-text_w)/2:y=(h-text_h)/2"
            ));
        }
    }

    // #12 Animated Comment-Bait / Poll Prompt Banner (final 5 seconds)
    if let Some(poll) = opts.poll_prompt.as_deref().map(str::trim).filter(|s| !s.is_empty()) && out_dur_s > 2.5 {
        let safe = escape_drawtext(&poll.chars().take(54).collect::<String>());
        let st = (out_dur_s - 5.0).max(0.0);
        filters.push(format!(
            "drawtext={font_opt}expansion=none:enable='gte(t\\,{st:.2})':text='{safe}':fontcolor=#FFE600:fontsize=max(24\\,min(w/18\\,46)):box=1:boxcolor=#FF2E93@0.88:boxborderw=18:x=(w-text_w)/2:y=h*0.66"
        ));
    }

    // #17 GTA "Weazel News / Vice Breaking" Lower-Third Ticker
    if let Some(headline) = opts.weazel_headline.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let safe_head = escape_drawtext(&headline.to_uppercase().chars().take(56).collect::<String>());
        let ticker_raw = opts
            .weazel_ticker
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("WEAZEL NEWS VICE CITY LIVE  •  GTA 6 BREAKING COVERAGE  •  SUBSCRIBE FOR DAILY DROPS");
        let safe_tick = escape_drawtext(&ticker_raw.to_uppercase().chars().take(160).collect::<String>());
        // Red BREAKING bar + dark news banner + scrolling ticker
        filters.push("drawbox=x=0:y=ih*0.80:w=iw:h=42:color=#D90429@0.94:t=fill".to_owned());
        filters.push("drawbox=x=0:y=ih*0.80+42:w=iw:h=76:color=#090812@0.90:t=fill".to_owned());
        filters.push("drawbox=x=0:y=ih*0.80+118:w=iw:h=36:color=#FF9E2C@0.94:t=fill".to_owned());
        filters.push(format!(
            "drawtext={font_opt}expansion=none:text='WEAZEL NEWS  |  VICE BREAKING':fontcolor=white:fontsize=max(18\\,min(w/30\\,26)):x=w*0.04:y=h*0.80+10"
        ));
        filters.push(format!(
            "drawtext={font_opt}expansion=none:text='{safe_head}':fontcolor=#00E5FF:fontsize=max(22\\,min(w/22\\,38)):x=w*0.04:y=h*0.80+60"
        ));
        filters.push(format!(
            "drawtext={font_opt}expansion=none:text='{safe_tick}':fontcolor=#090812:fontsize=max(17\\,min(w/32\\,24)):x='w-mod(t*145\\,w+tw)':y=h*0.80+125"
        ));
    }

    filters
}

fn has_audio_stream(ffmpeg: &Path, path: &Path) -> bool {
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-i"]).arg(path);
    hide_window(&mut cmd);
    let out = cmd.output();
    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stderr);
        s.contains("Audio:")
    } else {
        true
    }
}

fn run_simple_ffmpeg(ffmpeg: &Path, args: &[String]) -> Result<(), String> {
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(args);
    hide_window(&mut cmd);
    let out = cmd
        .output()
        .map_err(|e| format!("Failed to start FFmpeg studio pass: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail = err.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
        return Err(format!("FFmpeg studio pass failed: {tail}"));
    }
    Ok(())
}

pub(crate) fn needs_studio_post_passes(opts: &StudioExportOptions) -> bool {
    opts.voiceover_path.as_deref().is_some_and(|s| !s.trim().is_empty())
        || opts.reaction_cam_path.as_deref().is_some_and(|s| !s.trim().is_empty())
        || !opts.sfx_cues.is_empty()
        || opts.progress_bar
        || opts.freeze_at_ms.is_some()
        || (opts.replay_start_ms.is_some() && opts.replay_end_ms.is_some())
        || opts.intro_sting_text.as_deref().is_some_and(|s| !s.trim().is_empty())
        || opts.outro_card_text.as_deref().is_some_and(|s| !s.trim().is_empty())
}

pub(crate) fn apply_studio_post_passes(
    app: &App,
    ffmpeg: &Path,
    mp4_path: &Path,
    opts: &StudioExportOptions,
) -> Result<(), String> {
    if !needs_studio_post_passes(opts) {
        return Ok(());
    }
    let work_dir = mp4_path.parent().unwrap_or_else(|| Path::new("."));
    let stamp = uuid::Uuid::new_v4().simple().to_string();

    // PASS 1: Composite Voiceover + Auto-Ducking (#1/#16), Reaction Cam (#2), SFX Stingers (#6), Progress Bar (#14)
    let vo_path = opts
        .voiceover_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_file());
    let cam_path = opts
        .reaction_cam_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_file());
    let mut sfx_files: Vec<(f64, PathBuf)> = Vec::new();
    for cue in opts.sfx_cues.iter().take(12) {
        if cue.at_s.is_finite() && cue.at_s >= 0.0
            && let Ok(p) = ensure_sfx_file(app, &cue.kind)
        {
            sfx_files.push((cue.at_s, p));
        }
    }

    if vo_path.is_some() || cam_path.is_some() || !sfx_files.is_empty() || opts.progress_bar {
        let dur_s = probe_mp4_summary(mp4_path)
            .map(|s| s.duration_s.max(0.5))
            .unwrap_or(15.0);
        let src_has_audio = has_audio_stream(ffmpeg, mp4_path);
        let pass1_out = work_dir.join(format!("studio-p1-{stamp}.mp4"));

        let mut args: Vec<String> = vec![
            "-y".into(),
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-i".into(),
            mp4_path.to_string_lossy().into_owned(),
        ];
        let mut next_idx = 1usize;
        let cam_idx = if let Some(ref cp) = cam_path {
            let idx = next_idx;
            next_idx += 1;
            args.push("-i".into());
            args.push(cp.to_string_lossy().into_owned());
            Some(idx)
        } else {
            None
        };
        let vo_idx = if let Some(ref vp) = vo_path {
            let idx = next_idx;
            next_idx += 1;
            args.push("-i".into());
            args.push(vp.to_string_lossy().into_owned());
            Some(idx)
        } else {
            None
        };
        let mut sfx_indices: Vec<(f64, usize)> = Vec::new();
        for (at_s, sp) in &sfx_files {
            let idx = next_idx;
            next_idx += 1;
            args.push("-i".into());
            args.push(sp.to_string_lossy().into_owned());
            sfx_indices.push((*at_s, idx));
        }
        let bar_idx = if opts.progress_bar {
            let idx = next_idx;
            args.extend([
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                format!("color=c=#FF2E93:s=1080x10:d={dur_s:.2}"),
            ]);
            Some(idx)
        } else {
            None
        };

        let mut fc: Vec<String> = Vec::new();
        let mut cur_v = "[0:v]".to_owned();

        // Reaction Cam overlay (#2)
        if let Some(ci) = cam_idx {
            let layout = opts.reaction_cam_layout.as_deref().unwrap_or("corner_tr");
            if layout == "split_top" {
                fc.push(format!(
                    "[{ci}:v]scale=1080:640:force_original_aspect_ratio=increase,crop=1080:640[camv];{cur_v}scale=1080:1280:force_original_aspect_ratio=increase,crop=1080:1280[mainv];[camv][mainv]vstack=inputs=2[vcam]"
                ));
            } else {
                let (ox, oy) = match layout {
                    "corner_tl" => ("36", "140"),
                    "corner_bl" => ("36", "H-h-240"),
                    "corner_br" => ("W-w-36", "H-h-240"),
                    _ => ("W-w-36", "140"),
                };
                fc.push(format!(
                    "[{ci}:v]scale=310:310:force_original_aspect_ratio=increase,crop=310:310,pad=320:320:5:5:#00E5FF[camv];{cur_v}[camv]overlay={ox}:{oy}:shortest=1[vcam]"
                ));
            }
            cur_v = "[vcam]".to_owned();
        }

        // Animated Vice Sunset Retention Progress Bar (#14)
        if let Some(bi) = bar_idx {
            fc.push(format!(
                "{cur_v}[{bi}:v]overlay=x='-w+w*min(1\\,t/{dur_s:.3})':y=H-10:shortest=1[vbar]"
            ));
            cur_v = "[vbar]".to_owned();
        }

        // Audio mixing: base audio + Voiceover (with Auto-Ducking #1 & Mic FX #16) + SFX (#6)
        let mut cur_a: Option<String> = None;
        if vo_idx.is_some() || !sfx_indices.is_empty() {
            let base_label = if src_has_audio {
                fc.push("[0:a]aresample=48000,aformat=channel_layouts=stereo[abase]".to_owned());
                "[abase]".to_owned()
            } else {
                fc.push(format!("anullsrc=r=48000:cl=stereo,atrim=0:{dur_s:.3}[abase]"));
                "[abase]".to_owned()
            };
            let mut mixed = base_label;

            if let Some(vi) = vo_idx {
                let gain = opts.voiceover_gain_db.unwrap_or(2.0).clamp(-12.0, 14.0);
                let duck_db = opts.voiceover_duck_db.unwrap_or(-14.0).clamp(-28.0, 0.0);
                let fx = voice_fx_filter(opts.voice_fx.as_deref())
                    .map(|f| format!(",{f}"))
                    .unwrap_or_default();
                fc.push(format!(
                    "[{vi}:a]aresample=48000,aformat=channel_layouts=stereo{fx},volume={gain:.1}dB,asplit=2[vo_mix][vo_sc]"
                ));
                if duck_db < -1.0 && src_has_audio {
                    fc.push(format!(
                        "{mixed}[vo_sc]sidechaincompress=threshold=0.025:ratio=8:attack=25:release=320:makeup=1[aduck];[aduck]volume={:.1}dB[aduck2];[aduck2][vo_mix]amix=inputs=2:duration=first:normalize=0[avo]",
                        (duck_db * 0.25).clamp(-6.0, 0.0)
                    ));
                } else {
                    fc.push(format!(
                        "{mixed}[vo_mix]amix=inputs=2:duration=first:normalize=0[avo]"
                    ));
                }
                mixed = "[avo]".to_owned();
            }

            if !sfx_indices.is_empty() {
                let mut mix_inputs = vec![mixed.clone()];
                for (k, (at_s, si)) in sfx_indices.iter().enumerate() {
                    let delay_ms = (*at_s * 1000.0).round() as u64;
                    fc.push(format!(
                        "[{si}:a]aresample=48000,aformat=channel_layouts=stereo,adelay={delay_ms}|{delay_ms},volume=0.85[sfx{k}]"
                    ));
                    mix_inputs.push(format!("[sfx{k}]"));
                }
                fc.push(format!(
                    "{}amix=inputs={}:duration=first:normalize=0[asfx]",
                    mix_inputs.join(""),
                    mix_inputs.len()
                ));
                mixed = "[asfx]".to_owned();
            }
            cur_a = Some(mixed);
        }

        if !fc.is_empty() {
            args.push("-filter_complex".into());
            args.push(fc.join(";"));
        }
        args.extend(["-map".into(), cur_v]);
        if let Some(a_lbl) = cur_a {
            args.extend(["-map".into(), a_lbl]);
        } else if src_has_audio {
            args.extend(["-map".into(), "0:a?".into()]);
        }
        args.extend([
            "-c:v".into(), "libx264".into(), "-preset".into(), "fast".into(), "-crf".into(), "20".into(),
            "-pix_fmt".into(), "yuv420p".into(),
            "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-ar".into(), "48000".into(), "-ac".into(), "2".into(),
            "-movflags".into(), "+faststart".into(),
            pass1_out.to_string_lossy().into_owned(),
        ]);
        run_simple_ffmpeg(ffmpeg, &args)?;
        std::fs::rename(&pass1_out, mp4_path).map_err(|e| format!("Failed updating studio pass 1: {e}"))?;
    }

    // PASS 2: Timeline Splices (Freeze-Frame Zoom #4, Instant Slow-Mo Replay #11, Branded Intro/Outro Cards #7)
    let want_freeze = opts.freeze_at_ms.is_some();
    let want_replay = opts.replay_start_ms.is_some() && opts.replay_end_ms.is_some();
    let want_intro = opts.intro_sting_text.as_deref().is_some_and(|s| !s.trim().is_empty());
    let want_outro = opts.outro_card_text.as_deref().is_some_and(|s| !s.trim().is_empty());

    if want_freeze || want_replay || want_intro || want_outro {
        let font_opt = find_overlay_font()
            .map(|f| format!("fontfile='{f}':"))
            .unwrap_or_default();
        let dur_s = probe_mp4_summary(mp4_path).map(|s| s.duration_s).unwrap_or(10.0);
        let has_aud = has_audio_stream(ffmpeg, mp4_path);
        let mut segs: Vec<PathBuf> = Vec::new();
        let mut cleanup: Vec<PathBuf> = Vec::new();

        // Helper to normalize a slice into a standard 1080x1920 30fps 48kHz stereo clip
        let make_card = |label: &str, sub: &str, dur: f64, bg: &str, accent: &str, out: &Path| -> Result<(), String> {
            let s1 = escape_drawtext(&label.chars().take(48).collect::<String>());
            let s2 = escape_drawtext(&sub.chars().take(54).collect::<String>());
            let vf = format!(
                "drawbox=x=56:y=ih*0.36:w=iw-112:h=8:color={accent}:t=fill,drawbox=x=56:y=ih*0.60:w=iw-112:h=8:color=#00E5FF:t=fill,drawtext={font_opt}expansion=none:text='{s1}':fontcolor=white:fontsize=56:x=(w-text_w)/2:y=h*0.43,drawtext={font_opt}expansion=none:text='{s2}':fontcolor=#00E5FF:fontsize=30:x=(w-text_w)/2:y=h*0.52,fade=t=in:st=0:d=0.25,fade=t=out:st={:.2}:d=0.25",
                (dur - 0.25).max(0.1)
            );
            let args: Vec<String> = vec![
                "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
                "-f".into(), "lavfi".into(), "-i".into(), format!("color=c={bg}:s=1080x1920:r=30:d={dur:.2}"),
                "-f".into(), "lavfi".into(), "-i".into(), format!("anullsrc=r=48000:cl=stereo:d={dur:.2}"),
                "-vf".into(), vf,
                "-c:v".into(), "libx264".into(), "-preset".into(), "fast".into(), "-crf".into(), "20".into(),
                "-pix_fmt".into(), "yuv420p".into(), "-r".into(), "30".into(),
                "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-ar".into(), "48000".into(), "-ac".into(), "2".into(),
                "-shortest".into(),
                out.to_string_lossy().into_owned(),
            ];
            run_simple_ffmpeg(ffmpeg, &args)
        };

        let extract_norm_slice = |start_s: f64, span_s: f64, extra_vf: Option<&str>, slowmo: bool, out: &Path| -> Result<(), String> {
            let mut vf = "scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2:black".to_owned();
            if slowmo {
                vf.push_str(",setpts=2.0*PTS,eq=contrast=1.12:saturation=0.65,drawbox=x=40:y=40:w=iw-80:h=ih-80:color=#00E5FF@0.75:t=8");
            }
            if let Some(ex) = extra_vf {
                vf.push(',');
                vf.push_str(ex);
            }
            let mut args: Vec<String> = vec![
                "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
                "-ss".into(), format!("{start_s:.3}"),
                "-t".into(), format!("{span_s:.3}"),
                "-i".into(), mp4_path.to_string_lossy().into_owned(),
            ];
            if !has_aud {
                args.extend([
                    "-f".into(), "lavfi".into(), "-i".into(),
                    format!("anullsrc=r=48000:cl=stereo:d={:.2}", if slowmo { span_s * 2.0 } else { span_s }),
                    "-map".into(), "0:v".into(), "-map".into(), "1:a".into(),
                ]);
            }
            args.extend(["-vf".into(), vf]);
            if has_aud && slowmo {
                args.extend(["-af".into(), "atempo=0.5".into()]);
            }
            args.extend([
                "-c:v".into(), "libx264".into(), "-preset".into(), "fast".into(), "-crf".into(), "20".into(),
                "-pix_fmt".into(), "yuv420p".into(), "-r".into(), "30".into(),
                "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-ar".into(), "48000".into(), "-ac".into(), "2".into(),
                "-shortest".into(),
                out.to_string_lossy().into_owned(),
            ]);
            run_simple_ffmpeg(ffmpeg, &args)
        };

        // #7 Intro Sting Card
        if let Some(intro_txt) = opts.intro_sting_text.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let p = work_dir.join(format!("studio-intro-{stamp}.mp4"));
            make_card(intro_txt, "SNAPMATIC  •  VICE STUDIO", 1.5, "#090812", "#FF2E93", &p)?;
            segs.push(p.clone());
            cleanup.push(p);
        }

        // Main clip with optional Freeze-Frame Zoom (#4) or Slow-Mo Replay (#11)
        if let Some(f_ms) = opts.freeze_at_ms {
            let f_s = (f_ms as f64 / 1000.0).clamp(0.3, (dur_s - 0.3).max(0.3));
            let f_dur = opts.freeze_dur_s.unwrap_or(2.0).clamp(0.8, 5.0);
            let callout = opts.freeze_callout.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("WAIT — LOOK RIGHT HERE");
            let safe_co = escape_drawtext(&callout.chars().take(48).collect::<String>());

            let p_before = work_dir.join(format!("studio-fb-{stamp}.mp4"));
            extract_norm_slice(0.0, f_s, None, false, &p_before)?;
            segs.push(p_before.clone());
            cleanup.push(p_before);

            let png_frame = work_dir.join(format!("studio-ff-{stamp}.png"));
            run_simple_ffmpeg(ffmpeg, &[
                "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
                "-ss".into(), format!("{f_s:.3}"),
                "-i".into(), mp4_path.to_string_lossy().into_owned(),
                "-vframes".into(), "1".into(),
                "-vf".into(), "scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2:black".into(),
                png_frame.to_string_lossy().into_owned(),
            ])?;
            cleanup.push(png_frame.clone());

            let p_freeze = work_dir.join(format!("studio-fz-{stamp}.mp4"));
            let frames = (f_dur * 30.0).round() as u32;
            let fz_vf = format!(
                "zoompan=z='min(1.26\\,1+0.0045*on)':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':d={frames}:s=1080x1920:fps=30,drawbox=x=210:y=680:w=660:h=560:color=#FF2E93@0.92:t=8,drawtext={font_opt}expansion=none:text='{safe_co}':fontcolor=#FFE600:fontsize=46:box=1:boxcolor=#090812@0.86:boxborderw=18:x=(w-text_w)/2:y=h*0.24"
            );
            run_simple_ffmpeg(ffmpeg, &[
                "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
                "-loop".into(), "1".into(), "-i".into(), png_frame.to_string_lossy().into_owned(),
                "-f".into(), "lavfi".into(), "-i".into(), format!("anullsrc=r=48000:cl=stereo:d={f_dur:.2}"),
                "-vf".into(), fz_vf,
                "-t".into(), format!("{f_dur:.2}"),
                "-c:v".into(), "libx264".into(), "-preset".into(), "fast".into(), "-crf".into(), "20".into(),
                "-pix_fmt".into(), "yuv420p".into(), "-r".into(), "30".into(),
                "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-ar".into(), "48000".into(), "-ac".into(), "2".into(),
                "-shortest".into(),
                p_freeze.to_string_lossy().into_owned(),
            ])?;
            segs.push(p_freeze.clone());
            cleanup.push(p_freeze);

            if dur_s > f_s + 0.25 {
                let p_after = work_dir.join(format!("studio-fa-{stamp}.mp4"));
                extract_norm_slice(f_s, dur_s - f_s, None, false, &p_after)?;
                segs.push(p_after.clone());
                cleanup.push(p_after);
            }
        } else if let (Some(rs_ms), Some(re_ms)) = (opts.replay_start_ms, opts.replay_end_ms)
            && re_ms > rs_ms + 300
        {
            let rs = (rs_ms as f64 / 1000.0).clamp(0.0, dur_s);
            let re = (re_ms as f64 / 1000.0).clamp(rs + 0.4, dur_s);
            let r_label = opts.replay_label.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("INSTANT REPLAY  0.5x");
            let safe_rl = escape_drawtext(&r_label.chars().take(42).collect::<String>());
            let replay_txt = format!(
                "drawtext={font_opt}expansion=none:text='{safe_rl}':fontcolor=#00E5FF:fontsize=42:box=1:boxcolor=#090812@0.84:boxborderw=14:x=(w-text_w)/2:y=h*0.14"
            );

            let p_part1 = work_dir.join(format!("studio-rp1-{stamp}.mp4"));
            extract_norm_slice(0.0, re, None, false, &p_part1)?;
            segs.push(p_part1.clone());
            cleanup.push(p_part1);

            let p_slow = work_dir.join(format!("studio-rps-{stamp}.mp4"));
            extract_norm_slice(rs, re - rs, Some(&replay_txt), true, &p_slow)?;
            segs.push(p_slow.clone());
            cleanup.push(p_slow);

            if dur_s > re + 0.25 {
                let p_part2 = work_dir.join(format!("studio-rp2-{stamp}.mp4"));
                extract_norm_slice(re, dur_s - re, None, false, &p_part2)?;
                segs.push(p_part2.clone());
                cleanup.push(p_part2);
            }
        } else {
            let p_main = work_dir.join(format!("studio-main-{stamp}.mp4"));
            extract_norm_slice(0.0, dur_s, None, false, &p_main)?;
            segs.push(p_main.clone());
            cleanup.push(p_main);
        }

        // #7 Outro End-Card
        if let Some(outro_txt) = opts.outro_card_text.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let p = work_dir.join(format!("studio-outro-{stamp}.mp4"));
            make_card(outro_txt, "LIKE  •  COMMENT  •  FOLLOW", 2.4, "#090812", "#00E5FF", &p)?;
            segs.push(p.clone());
            cleanup.push(p);
        }

        // Concatenate all normalized segments cleanly
        let list_path = work_dir.join(format!("studio-list-{stamp}.txt"));
        let mut list_body = String::new();
        for s in &segs {
            let escaped = s.to_string_lossy().replace('\'', "'\\''");
            list_body.push_str(&format!("file '{escaped}'\n"));
        }
        std::fs::write(&list_path, &list_body).map_err(|e| format!("Could not write concat list: {e}"))?;
        cleanup.push(list_path.clone());

        let pass2_out = work_dir.join(format!("studio-p2-{stamp}.mp4"));
        let concat_res = run_simple_ffmpeg(ffmpeg, &[
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
            "-f".into(), "concat".into(), "-safe".into(), "0".into(),
            "-i".into(), list_path.to_string_lossy().into_owned(),
            "-c:v".into(), "libx264".into(), "-preset".into(), "fast".into(), "-crf".into(), "20".into(),
            "-pix_fmt".into(), "yuv420p".into(),
            "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-ar".into(), "48000".into(), "-ac".into(), "2".into(),
            "-movflags".into(), "+faststart".into(),
            pass2_out.to_string_lossy().into_owned(),
        ]);
        for p in cleanup {
            let _ = std::fs::remove_file(p);
        }
        concat_res?;
        std::fs::rename(&pass2_out, mp4_path).map_err(|e| format!("Failed finalizing studio splice: {e}"))?;
    }

    Ok(())
}

fn register_created_clip(app: &App, file_path: &Path, title: &str) -> Result<(String, String, u64), String> {
    let bytes = std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);
    let (source_id, root_dir) = {
        let db = app.db.lock().map_err(|e| e.to_string())?;
        let cfg = source::config(&db).map_err(|e| e.to_string())?;
        let root = if cfg.root.trim().is_empty() {
            let fb = app.media.join("sources");
            let _ = std::fs::create_dir_all(&fb);
            fb
        } else {
            PathBuf::from(&cfg.root)
        };
        let existing_id: Option<i64> = db
            .query_row(
                "SELECT id FROM profiles WHERE role='source' AND url='https://local.snapmatic/creations' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let sid = if let Some(id) = existing_id {
            let _ = db.execute("UPDATE profiles SET archived=0, source_folder='Snapmatic Creations' WHERE id=?1", [id]);
            id
        } else {
            db.execute(
                "INSERT INTO profiles(name, platform, url, role, tags, source_folder) VALUES('Snapmatic Creations', 'other', 'https://local.snapmatic/creations', 'source', '[\"Original\"]', 'Snapmatic Creations')",
                [],
            )
            .map_err(|e| e.to_string())?;
            db.last_insert_rowid()
        };
        (sid, root)
    };
    let dest_dir = root_dir.join("Snapmatic Creations");
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let fname = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("creation.mp4")
        .to_owned();
    let source_copy = dest_dir.join(&fname);
    std::fs::copy(file_path, &source_copy).map_err(|e| format!("Could not save creation into Film Roll: {e}"))?;
    let clip_id = stable_id(source_id, &fname);
    {
        let db = app.db.lock().map_err(|e| e.to_string())?;
        db.execute(
            "INSERT OR REPLACE INTO workspace_film_roll(clip_id, source_id, path, title, description, bytes, added_at, review_state, exported_path)
             VALUES(?1, ?2, ?3, ?4, 'Created in Snapmatic Original Studio', ?5, unixepoch(), 'exported', ?6)",
            params![clip_id, source_id, fname, title, bytes as i64, fname],
        )
        .map_err(|e| e.to_string())?;
        let _ = db.execute(
            "INSERT INTO workspace_export_history(clip_id, title, source_name, file_name, caption_file, cover_file, framing, quality, in_ms, out_ms, muted, normalized_audio, bytes, exported_at)
             VALUES(?1, ?2, 'Snapmatic Creations', ?3, '', '', 'studio_original', 'balanced', 0, 0, 0, 1, ?4, unixepoch())",
            params![clip_id, title, fname, bytes as i64],
        );
    }
    Ok((clip_id, fname, bytes))
}

// #3 & #13 Multi-Clip Montage & Beat-Sync Auto-Cutter
pub(crate) async fn render_montage(
    State(app): State<App>,
    Json(input): Json<MontageRequest>,
) -> ApiResult<Json<Value>> {
    if input.clip_ids.len() < 2 || input.clip_ids.len() > 8 {
        return Err(bad("Select between 2 and 8 Film Roll clips for a montage."));
    }
    let Some(ffmpeg) = find_ffmpeg() else {
        return Err(bad("FFmpeg not found in tools/ffmpeg.exe."));
    };
    let mut resolved: Vec<PathBuf> = Vec::new();
    for cid in &input.clip_ids {
        if !crate::workspace_media::hex(cid) {
            return Err(bad("Invalid clip ID in montage list."));
        }
        let Some(p) = resolve_video(&app, cid) else {
            return Err(bad("One of the selected montage clips is missing on disk."));
        };
        resolved.push(p);
    }
    let out_dir = exports_dir(&app);
    let _ = std::fs::create_dir_all(&out_dir);
    let title = input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("GTA 6 Top Moments Montage")
        .to_owned();
    let stamp = chrono::Utc::now().timestamp();
    let out_name = format!("{}-{stamp}.mp4", sanitize_stem(&title));
    let out_path = out_dir.join(&out_name);
    let app_clone = app.clone();

    let (clip_id, file_name, bytes) = tokio::task::spawn_blocking(move || -> Result<(String, String, u64), String> {
        let font_opt = find_overlay_font().map(|f| format!("fontfile='{f}':")).unwrap_or_default();
        let grade_str = color_grade_filter(input.color_grade.as_deref()).map(|s| format!(",{s}")).unwrap_or_default();
        let clip_sec = input.clip_sec.unwrap_or(5.0).clamp(1.5, 25.0);

        // If beat_sync is enabled with a music track, snap cut lengths to beat intervals
        let mut beat_spans: Vec<f64> = vec![clip_sec; resolved.len()];
        if input.beat_sync
            && let Some(mp) = input.music_path.as_deref().map(PathBuf::from).filter(|p| p.is_file())
        {
            let mut bcmd = std::process::Command::new(&ffmpeg);
            bcmd.args(["-hide_banner", "-nostats", "-t", "45", "-i"]).arg(&mp).args([
                "-vn", "-af",
                "aresample=16000,asetnsamples=1600,astats=metadata=1:reset=1,ametadata=print:key=lavfi.astats.Overall.RMS_level:file=-",
                "-f", "null", "-",
            ]);
            hide_window(&mut bcmd);
            if let Ok(out) = bcmd.output() {
                let txt = String::from_utf8_lossy(&out.stdout);
                let mut cur_s = 0.0f64;
                let mut beats: Vec<f64> = Vec::new();
                for ln in txt.lines() {
                    if let Some(rest) = ln.split("pts_time:").nth(1)
                        && let Some(t) = rest.split_whitespace().next().and_then(|s| s.parse::<f64>().ok())
                    {
                        cur_s = t;
                    } else if let Some(v) = ln.strip_prefix("lavfi.astats.Overall.RMS_level=")
                        && let Ok(db) = v.trim().parse::<f64>()
                        && db > -24.0
                        && beats.last().is_none_or(|&last| cur_s - last >= (clip_sec * 0.6).max(1.8))
                    {
                        beats.push(cur_s);
                    }
                }
                for (i, w) in beats.windows(2).enumerate() {
                    if i < beat_spans.len() {
                        beat_spans[i] = (w[1] - w[0]).clamp(1.8, clip_sec.max(3.5));
                    }
                }
            }
        }

        let work_stamp = uuid::Uuid::new_v4().simple().to_string();
        let mut segs: Vec<PathBuf> = Vec::new();
        for (i, src) in resolved.iter().enumerate() {
            let seg = out_dir.join(format!("montage-seg-{work_stamp}-{i}.mp4"));
            let span = beat_spans[i];
            let label = input
                .labels
                .get(i)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("#{}", i + 1));
            let safe_lbl = escape_drawtext(&label.chars().take(40).collect::<String>());
            let has_a = has_audio_stream(&ffmpeg, src);
            let vf = format!(
                "crop='min(iw,ih*9/16)':'min(ih,iw*16/9)':'(iw-ow)/2':'(ih-oh)/2',scale=1080:1920:flags=lanczos{grade_str},drawtext={font_opt}expansion=none:enable='lt(t\\,2.2)':text='{safe_lbl}':fontcolor=#FFE600:fontsize=54:box=1:boxcolor=#FF2E93@0.86:boxborderw=18:x=(w-text_w)/2:y=h*0.14,fade=t=in:st=0:d=0.22"
            );
            let mut args: Vec<String> = vec![
                "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
                "-t".into(), format!("{span:.2}"),
                "-i".into(), src.to_string_lossy().into_owned(),
            ];
            if !has_a {
                args.extend([
                    "-f".into(), "lavfi".into(), "-i".into(), format!("anullsrc=r=48000:cl=stereo:d={span:.2}"),
                    "-map".into(), "0:v".into(), "-map".into(), "1:a".into(),
                ]);
            }
            args.extend([
                "-vf".into(), vf,
                "-c:v".into(), "libx264".into(), "-preset".into(), "fast".into(), "-crf".into(), "20".into(),
                "-pix_fmt".into(), "yuv420p".into(), "-r".into(), "30".into(),
                "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-ar".into(), "48000".into(), "-ac".into(), "2".into(),
                "-shortest".into(),
                seg.to_string_lossy().into_owned(),
            ]);
            run_simple_ffmpeg(&ffmpeg, &args)?;
            segs.push(seg);
        }

        let list_path = out_dir.join(format!("montage-list-{work_stamp}.txt"));
        let mut body = String::new();
        for s in &segs {
            body.push_str(&format!("file '{}'\n", s.to_string_lossy().replace('\'', "'\\''")));
        }
        std::fs::write(&list_path, &body).map_err(|e| format!("Failed writing montage list: {e}"))?;

        let mut concat_args: Vec<String> = vec![
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
            "-f".into(), "concat".into(), "-safe".into(), "0".into(),
            "-i".into(), list_path.to_string_lossy().into_owned(),
        ];
        if let Some(mp) = input.music_path.as_deref().map(PathBuf::from).filter(|p| p.is_file()) {
            let mdb = input.music_volume_db.unwrap_or(-14.0).clamp(-36.0, 0.0);
            concat_args.extend([
                "-stream_loop".into(), "-1".into(), "-i".into(), mp.to_string_lossy().into_owned(),
                "-filter_complex".into(),
                format!("[0:a]volume=0.85[a0];[1:a]volume={mdb:.1}dB[m];[a0][m]amix=inputs=2:duration=first:normalize=0[aout]"),
                "-map".into(), "0:v".into(), "-map".into(), "[aout]".into(), "-shortest".into(),
            ]);
        }
        let crf = if input.quality.as_deref() == Some("high") { "17" } else { "20" };
        concat_args.extend([
            "-c:v".into(), "libx264".into(), "-preset".into(), "fast".into(), "-crf".into(), crf.into(),
            "-pix_fmt".into(), "yuv420p".into(),
            "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-ar".into(), "48000".into(), "-ac".into(), "2".into(),
            "-movflags".into(), "+faststart".into(),
            out_path.to_string_lossy().into_owned(),
        ]);
        let res = run_simple_ffmpeg(&ffmpeg, &concat_args);
        for s in segs {
            let _ = std::fs::remove_file(s);
        }
        let _ = std::fs::remove_file(&list_path);
        res?;

        // Apply optional Studio overlays (Intro/Outro, Progress Bar, Credit, Weazel News)
        let extra_opts = StudioExportOptions {
            progress_bar: input.progress_bar,
            intro_sting_text: input.intro_sting_text.clone(),
            outro_card_text: input.outro_card_text.clone(),
            credit_handle: input.credit_handle.clone(),
            weazel_headline: input.weazel_headline.clone(),
            ..Default::default()
        };
        apply_studio_post_passes(&app_clone, &ffmpeg, &out_path, &extra_opts)?;
        register_created_clip(&app_clone, &out_path, &title)
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;

    Ok(Json(json!({
        "clip_id": clip_id,
        "file_name": file_name,
        "bytes": bytes,
        "export_url": format!("/api/workspace/film-roll/{clip_id}/export")
    })))
}

// #9 "GTA V vs GTA VI" Side-by-Side / Stacked Comparison Builder
pub(crate) async fn render_comparison(
    State(app): State<App>,
    Json(input): Json<CompareRequest>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&input.clip_a_id) || !crate::workspace_media::hex(&input.clip_b_id) {
        return Err(bad("Choose two valid Film Roll clips to compare."));
    }
    let Some(path_a) = resolve_video(&app, &input.clip_a_id) else {
        return Err(bad("Clip A not found on disk."));
    };
    let Some(path_b) = resolve_video(&app, &input.clip_b_id) else {
        return Err(bad("Clip B not found on disk."));
    };
    let Some(ffmpeg) = find_ffmpeg() else {
        return Err(bad("FFmpeg not found in tools/ffmpeg.exe."));
    };

    let out_dir = exports_dir(&app);
    let _ = std::fs::create_dir_all(&out_dir);
    let label_a = input.label_a.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("GTA V").to_owned();
    let label_b = input.label_b.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("GTA VI").to_owned();
    let title = input
        .title_banner
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{label_a} vs {label_b}"));
    let stamp = chrono::Utc::now().timestamp();
    let out_name = format!("{}-{stamp}.mp4", sanitize_stem(&title));
    let out_path = out_dir.join(&out_name);
    let app_clone = app.clone();

    let (clip_id, file_name, bytes) = tokio::task::spawn_blocking(move || -> Result<(String, String, u64), String> {
        let font_opt = find_overlay_font().map(|f| format!("fontfile='{f}':")).unwrap_or_default();
        let safe_a = escape_drawtext(&label_a.chars().take(28).collect::<String>());
        let safe_b = escape_drawtext(&label_b.chars().take(28).collect::<String>());
        let safe_t = escape_drawtext(&title.chars().take(48).collect::<String>());
        let max_s = input.max_seconds.unwrap_or(20.0).clamp(3.0, 58.0);
        let grade = color_grade_filter(input.color_grade.as_deref()).map(|s| format!(",{s}")).unwrap_or_default();
        let horizontal = input.layout.as_deref() == Some("horizontal");
        let has_a = has_audio_stream(&ffmpeg, &path_a);

        let fc = if horizontal {
            format!(
                "[0:v]scale=540:1920:force_original_aspect_ratio=increase,crop=540:1920,drawtext={font_opt}expansion=none:text='{safe_a}':fontcolor=white:fontsize=38:box=1:boxcolor=#FF2E93@0.88:boxborderw=14:x=(w-text_w)/2:y=h*0.14[va];[1:v]scale=540:1920:force_original_aspect_ratio=increase,crop=540:1920,drawtext={font_opt}expansion=none:text='{safe_b}':fontcolor=#090812:fontsize=38:box=1:boxcolor=#00E5FF@0.92:boxborderw=14:x=(w-text_w)/2:y=h*0.14[vb];[va][vb]hstack=inputs=2{grade},drawbox=x=538:y=0:w=4:h=ih:color=#00E5FF:t=fill,drawtext={font_opt}expansion=none:text='{safe_t}':fontcolor=#FFE600:fontsize=44:box=1:boxcolor=#090812@0.86:boxborderw=16:x=(w-text_w)/2:y=h*0.05[v]"
            )
        } else {
            format!(
                "[0:v]scale=1080:960:force_original_aspect_ratio=increase,crop=1080:960,drawtext={font_opt}expansion=none:text='{safe_a}':fontcolor=white:fontsize=42:box=1:boxcolor=#FF2E93@0.88:boxborderw=14:x=40:y=40[va];[1:v]scale=1080:960:force_original_aspect_ratio=increase,crop=1080:960,drawtext={font_opt}expansion=none:text='{safe_b}':fontcolor=#090812:fontsize=42:box=1:boxcolor=#00E5FF@0.92:boxborderw=14:x=40:y=40[vb];[va][vb]vstack=inputs=2{grade},drawbox=x=0:y=958:w=iw:h=5:color=#00E5FF:t=fill,drawtext={font_opt}expansion=none:text='{safe_t}':fontcolor=#FFE600:fontsize=44:box=1:boxcolor=#090812@0.86:boxborderw=16:x=(w-text_w)/2:y=(h-text_h)/2[v]"
            )
        };

        let crf = if input.quality.as_deref() == Some("high") { "17" } else { "20" };
        let mut args: Vec<String> = vec![
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
            "-t".into(), format!("{max_s:.2}"), "-i".into(), path_a.to_string_lossy().into_owned(),
            "-t".into(), format!("{max_s:.2}"), "-i".into(), path_b.to_string_lossy().into_owned(),
            "-filter_complex".into(), fc,
            "-map".into(), "[v]".into(),
        ];
        if has_a {
            args.extend(["-map".into(), "0:a?".into()]);
        }
        args.extend([
            "-c:v".into(), "libx264".into(), "-preset".into(), "fast".into(), "-crf".into(), crf.into(),
            "-pix_fmt".into(), "yuv420p".into(), "-r".into(), "30".into(),
            "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-ar".into(), "48000".into(), "-ac".into(), "2".into(),
            "-shortest".into(), "-movflags".into(), "+faststart".into(),
            out_path.to_string_lossy().into_owned(),
        ]);
        run_simple_ffmpeg(&ffmpeg, &args)?;
        register_created_clip(&app_clone, &out_path, &title)
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;

    Ok(Json(json!({
        "clip_id": clip_id,
        "file_name": file_name,
        "bytes": bytes,
        "export_url": format!("/api/workspace/film-roll/{clip_id}/export")
    })))
}
