use crate::workspace_film_roll::{
    find_ffmpeg, probe_mp4_summary, probed_duration_ms, resolve_video,
};
use crate::{ApiResult, App, bad, internal};
use axum::{
    Json,
    extract::{Multipart, Path as AxumPath, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Deserialize, Serialize, Clone, Default)]
pub(crate) struct CaptionWord {
    pub(crate) text: String,
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    #[serde(default)]
    pub(crate) emoji: Option<String>,
    #[serde(default)]
    pub(crate) color: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub(crate) struct CaptionStyleConfig {
    #[serde(default = "default_cap_preset")]
    pub(crate) preset: String,
    #[serde(default = "default_words_per_line")]
    pub(crate) words_per_line: usize,
    #[serde(default = "default_font_size")]
    pub(crate) font_size: u32,
    #[serde(default = "default_pos_y_pct")]
    pub(crate) pos_y_pct: u32,
    #[serde(default = "default_active_color")]
    pub(crate) active_color: String,
    #[serde(default = "default_true")]
    pub(crate) uppercase: bool,
    #[serde(default = "default_true")]
    pub(crate) pop_animation: bool,
    #[serde(default = "default_true")]
    pub(crate) show_emojis: bool,
}

fn default_cap_preset() -> String { "hormozi".to_owned() }
fn default_words_per_line() -> usize { 3 }
fn default_font_size() -> u32 { 68 }
fn default_pos_y_pct() -> u32 { 74 }
fn default_active_color() -> String { "#FFE600".to_owned() }
fn default_true() -> bool { true }

impl Default for CaptionStyleConfig {
    fn default() -> Self {
        Self {
            preset: default_cap_preset(),
            words_per_line: default_words_per_line(),
            font_size: default_font_size(),
            pos_y_pct: default_pos_y_pct(),
            active_color: default_active_color(),
            uppercase: true,
            pop_animation: true,
            show_emojis: true,
        }
    }
}

#[derive(Deserialize, Clone, Default)]
pub(crate) struct StudioExportOptions {
    // Tool 1: AutoCaption.io-style Word-by-Word ASS Karaoke Captions
    #[serde(default)]
    pub(crate) caption_words: Vec<CaptionWord>,
    #[serde(default)]
    pub(crate) caption_style: Option<CaptionStyleConfig>,
    // Tool 2: Mid-Clip Jump-Cut Silence Remover
    #[serde(default)]
    pub(crate) remove_mid_silences: bool,
    #[serde(default)]
    pub(crate) silence_threshold_db: Option<f64>,
    #[serde(default)]
    pub(crate) min_silence_s: Option<f64>,
    // Tool 4: Adobe-Podcast-style Studio Voice Polish + Clean Mic Voiceover
    #[serde(default)]
    pub(crate) studio_voice_enhance: bool,
    #[serde(default)]
    pub(crate) voiceover_path: Option<String>,
    #[serde(default)]
    pub(crate) voiceover_duck_db: Option<f64>,
    #[serde(default)]
    pub(crate) voiceover_gain_db: Option<f64>,
    // Tool 5: Pro Color Grades + Real .cube 3D LUTs
    #[serde(default)]
    pub(crate) color_grade: Option<String>,
    #[serde(default)]
    pub(crate) lut_path: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct TranscribeRequest {
    #[serde(default)]
    in_ms: Option<u64>,
    #[serde(default)]
    out_ms: Option<u64>,
    #[serde(default)]
    language: Option<String>,
}

#[derive(Clone, Default)]
struct WhisperInstallState {
    installing: bool,
    progress_pct: u8,
    stage: String,
    error: Option<String>,
}

fn whisper_install_state() -> &'static Mutex<WhisperInstallState> {
    static STATE: OnceLock<Mutex<WhisperInstallState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(WhisperInstallState::default()))
}

pub(crate) fn studio_media_dir(app: &App) -> PathBuf {
    let dir = app.media.join("studio_media");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub(crate) fn studio_luts_dir(app: &App) -> PathBuf {
    let dir = studio_media_dir(app).join("luts");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn whisper_tools_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let p = dir.join("tools").join("whisper");
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    let p = PathBuf::from("tools").join("whisper");
    let _ = std::fs::create_dir_all(&p);
    p
}

pub(crate) fn find_whisper_installation() -> Option<(PathBuf, PathBuf)> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        roots.push(dir.join("tools").join("whisper"));
        roots.push(dir.join("tools"));
    }
    roots.push(PathBuf::from("tools").join("whisper"));
    roots.push(PathBuf::from("tools"));

    let mut found_bin: Option<PathBuf> = None;
    let mut found_model: Option<PathBuf> = None;
    for dir in &roots {
        if found_bin.is_none() {
            for name in ["whisper-cli.exe", "main.exe", "whisper.exe", "whisper-cli"] {
                let p = dir.join(name);
                if p.is_file() {
                    found_bin = Some(p);
                    break;
                }
            }
            // Also check one level deep (e.g., Release/whisper-cli.exe from zip extraction)
            if found_bin.is_none()
                && let Ok(entries) = std::fs::read_dir(dir)
            {
                for ent in entries.flatten() {
                    let sub = ent.path();
                    if sub.is_dir() {
                        for name in ["whisper-cli.exe", "main.exe"] {
                            let p = sub.join(name);
                            if p.is_file() {
                                found_bin = Some(p);
                                break;
                            }
                        }
                    }
                    if found_bin.is_some() {
                        break;
                    }
                }
            }
        }
        if found_model.is_none()
            && let Ok(entries) = std::fs::read_dir(dir)
        {
            for ent in entries.flatten() {
                let p = ent.path();
                if p.is_file()
                    && let Some(name) = p.file_name().and_then(|s| s.to_str())
                    && name.starts_with("ggml-")
                    && name.ends_with(".bin")
                    && std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) > 20_000_000
                {
                    found_model = Some(p);
                    break;
                }
            }
        }
    }
    match (found_bin, found_model) {
        (Some(b), Some(m)) => Some((b, m)),
        _ => None,
    }
}

fn hide_window(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
}

fn escape_ffmpeg_filter_path(p: &Path) -> String {
    p.to_string_lossy()
        .replace('\\', "/")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

// ==========================================================================
// 1. LOCAL WHISPER AI STATUS & 1-CLICK INSTALLER
// ==========================================================================

pub(crate) async fn whisper_status() -> Json<Value> {
    let installed = find_whisper_installation();
    let st = whisper_install_state()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    let (ready, bin, model, model_mb) = match installed {
        Some((b, m)) => {
            let mb = std::fs::metadata(&m).map(|md| md.len() / 1_048_576).unwrap_or(0);
            (
                true,
                Some(b.to_string_lossy().into_owned()),
                Some(m.file_name().and_then(|s| s.to_str()).unwrap_or("ggml-base.en.bin").to_owned()),
                mb,
            )
        }
        None => (false, None, None, 0),
    };
    Json(json!({
        "ready": ready,
        "binary_path": bin,
        "model_name": model,
        "model_size_mb": model_mb,
        "installing": st.installing,
        "progress_pct": if ready { 100 } else { st.progress_pct },
        "stage": if ready && !st.installing { "Ready (Local Whisper AI)".to_owned() } else { st.stage },
        "error": st.error
    }))
}

pub(crate) async fn install_whisper() -> ApiResult<Json<Value>> {
    if find_whisper_installation().is_some() {
        return Ok(Json(json!({ "started": false, "ready": true, "message": "Local Whisper AI is already installed." })));
    }
    {
        let mut st = whisper_install_state().lock().map_err(internal)?;
        if st.installing {
            return Ok(Json(json!({ "started": false, "installing": true, "message": "Installation already in progress." })));
        }
        st.installing = true;
        st.progress_pct = 5;
        st.stage = "Fetching latest whisper.cpp Windows release…".to_owned();
        st.error = None;
    }

    tokio::spawn(async move {
        let res = run_whisper_install().await;
        if let Ok(mut st) = whisper_install_state().lock() {
            st.installing = false;
            match res {
                Ok(()) => {
                    st.progress_pct = 100;
                    st.stage = "Local Whisper AI installed and ready!".to_owned();
                    st.error = None;
                }
                Err(msg) => {
                    st.stage = "Install failed".to_owned();
                    st.error = Some(msg);
                }
            }
        }
    });

    Ok(Json(json!({ "started": true, "installing": true })))
}

fn download_url_with_progress(
    url: &str,
    dest: &Path,
    expected_bytes: u64,
    base_pct: u8,
    span_pct: u8,
    stage_prefix: &str,
) -> Result<(), String> {
    let tmp = dest.with_extension("part");
    let _ = std::fs::remove_file(&tmp);
    let mut cmd = std::process::Command::new("curl.exe");
    cmd.args([
        "-fSL",
        "--retry",
        "2",
        "-A",
        "Snapmatic-Studio/1.0",
        "-o",
    ])
    .arg(&tmp)
    .arg(url)
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::piped());
    hide_window(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed launching curl.exe for {stage_prefix}: {e}"))?;

    loop {
        if let Ok(Some(status)) = child.try_wait() {
            if !status.success() {
                let mut err_buf = String::new();
                if let Some(mut se) = child.stderr.take() {
                    use std::io::Read;
                    let _ = se.read_to_string(&mut err_buf);
                }
                let _ = std::fs::remove_file(&tmp);
                return Err(format!("Download failed for {stage_prefix}: {}", err_buf.trim()));
            }
            break;
        }
        let got = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
        if let Ok(mut st) = whisper_install_state().lock() {
            let frac = if expected_bytes > 0 {
                (got as f64 / expected_bytes as f64).clamp(0.0, 0.99)
            } else {
                0.5
            };
            st.progress_pct = base_pct + (frac * span_pct as f64).round() as u8;
            st.stage = format!("{stage_prefix} ({:.1} MB)…", got as f64 / 1_048_576.0);
        }
        std::thread::sleep(std::time::Duration::from_millis(350));
    }

    if !tmp.is_file() || std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0) < 10_000 {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("Downloaded file for {stage_prefix} was empty."));
    }
    let _ = std::fs::remove_file(dest);
    std::fs::rename(&tmp, dest)
        .map_err(|e| format!("Could not finalize downloaded file: {e}"))?;
    Ok(())
}

async fn run_whisper_install() -> Result<(), String> {
    tokio::task::spawn_blocking(|| -> Result<(), String> {
        let dir = whisper_tools_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create tools/whisper: {e}"))?;

        // 1. Download whisper-bin-x64.zip if whisper-cli.exe isn't present yet
        let has_bin = dir.join("whisper-cli.exe").is_file()
            || dir.join("Release").join("whisper-cli.exe").is_file();
        if !has_bin {
            let mut api_cmd = std::process::Command::new("curl.exe");
            api_cmd.args([
                "-fsSL",
                "-A",
                "Snapmatic-Studio/1.0",
                "https://api.github.com/repos/ggml-org/whisper.cpp/releases?per_page=10",
            ]);
            hide_window(&mut api_cmd);
            let found_asset = api_cmd
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok())
                .and_then(|v| {
                    v.as_array().and_then(|releases| {
                        for rel in releases {
                            if let Some(arr) = rel["assets"].as_array()
                                && let Some(hit) = arr
                                    .iter()
                                    .find(|a| a["name"].as_str() == Some("whisper-bin-x64.zip"))
                                    .or_else(|| {
                                        arr.iter().find(|a| {
                                            a["name"].as_str().is_some_and(|n| {
                                                n.starts_with("whisper-")
                                                    && n.ends_with("bin-x64.zip")
                                                    && !n.contains("cublas")
                                                    && !n.contains("cuda")
                                            })
                                        })
                                    })
                                && let Some(url) = hit["browser_download_url"].as_str()
                            {
                                return Some((url.to_owned(), hit["size"].as_u64().unwrap_or(5_000_000)));
                            }
                        }
                        None
                    })
                });
            let (asset_url, asset_size) = found_asset.unwrap_or_else(|| {
                (
                    "https://github.com/ggml-org/whisper.cpp/releases/download/b5907/whisper-bin-x64.zip".to_owned(),
                    5_000_000,
                )
            });

            let zip_path = dir.join("whisper-bin-x64.zip");
            download_url_with_progress(&asset_url, &zip_path, asset_size, 8, 22, "Downloading whisper-cli.exe")?;

            if let Ok(mut st) = whisper_install_state().lock() {
                st.progress_pct = 32;
                st.stage = "Extracting whisper-cli.exe…".to_owned();
            }
            let mut ex_cmd = std::process::Command::new("tar.exe");
            ex_cmd
                .arg("-xf")
                .arg(&zip_path)
                .arg("-C")
                .arg(&dir);
            hide_window(&mut ex_cmd);
            let out = ex_cmd
                .output()
                .map_err(|e| format!("Failed extracting whisper zip: {e}"))?;
            let _ = std::fs::remove_file(&zip_path);
            if !out.status.success() {
                return Err(format!(
                    "Could not extract whisper zip: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
        }

        // 2. Download ggml-base.en.bin (~147 MB) from Hugging Face if missing
        let model_path = dir.join("ggml-base.en.bin");
        if !model_path.is_file()
            || std::fs::metadata(&model_path).map(|m| m.len()).unwrap_or(0) < 100_000_000
        {
            download_url_with_progress(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
                &model_path,
                147_964_211,
                35,
                63,
                "Downloading Whisper AI Base.en model",
            )?;
        }

        if find_whisper_installation().is_none() {
            return Err("Download completed, but whisper-cli.exe or ggml-base.en.bin was not found in tools/whisper/.".to_owned());
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("Install task panicked: {e}"))?
}

// ==========================================================================
// 2. WORD-LEVEL AI TRANSCRIPTION (AUTOCAPTION.IO ENGINE)
// ==========================================================================

fn keyword_emoji(word: &str) -> Option<&'static str> {
    let clean = word
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_ascii_lowercase();
    match clean.as_str() {
        "fire" | "lit" | "insane" | "crazy" | "wild" | "heat" | "hot" => Some("🔥"),
        "dead" | "died" | "wasted" | "killed" | "rip" | "bro" => Some("💀"),
        "look" | "watch" | "secret" | "hidden" | "missed" | "spotted" => Some("👀"),
        "money" | "cash" | "rich" | "million" | "heist" | "bank" | "stacks" => Some("💰"),
        "police" | "cops" | "arrested" | "chase" | "wanted" | "lspd" => Some("🚔"),
        "car" | "drive" | "fast" | "drift" | "speed" | "race" => Some("🏎️"),
        "vice" | "miami" | "beach" | "gta" | "gta6" | "gtavi" => Some("🌴"),
        "laugh" | "funny" | "hilarious" | "lmao" | "joke" => Some("😂"),
        "win" | "best" | "goat" | "king" | "legend" => Some("👑"),
        "shocking" | "wait" | "what" | "wtf" | "no" | "stop" => Some("⚡"),
        "gun" | "shoot" | "boom" | "explosion" | "blow" => Some("💥"),
        _ => None,
    }
}

fn parse_whisper_json_words(raw: &str) -> Vec<CaptionWord> {
    let Ok(val) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    let Some(segs) = val["transcription"].as_array() else {
        return Vec::new();
    };

    let mut words: Vec<CaptionWord> = Vec::new();
    let mut cur_text = String::new();
    let mut cur_start: Option<u64> = None;
    let mut cur_end: u64 = 0;

    let flush = |words: &mut Vec<CaptionWord>, txt: &mut String, st: &mut Option<u64>, en: u64| {
        let cleaned = txt.trim().to_owned();
        if !cleaned.is_empty()
            && let Some(s) = st.take()
        {
            let e = en.max(s + 90);
            let emoji = keyword_emoji(&cleaned).map(str::to_owned);
            words.push(CaptionWord {
                text: cleaned,
                start_ms: s,
                end_ms: e,
                emoji,
                color: None,
            });
        }
        txt.clear();
    };

    for seg in segs {
        if let Some(tokens) = seg["tokens"].as_array() {
            for tok in tokens {
                let t = tok["text"].as_str().unwrap_or("");
                if t.is_empty() || t.starts_with("[_") || t.starts_with("<|") {
                    continue;
                }
                let trimmed = t.trim();
                if trimmed.is_empty() || (trimmed.starts_with('[') && trimmed.ends_with(']')) {
                    continue;
                }
                let from_ms = tok["offsets"]["from"].as_u64().unwrap_or(cur_end);
                let to_ms = tok["offsets"]["to"].as_u64().unwrap_or(from_ms + 120);

                // Merge subword BPE pieces (e.g. "Leon"+"idas", "We"+"'re", "Prison"+".") that have no leading space
                let starts_new_word = t.starts_with(' ')
                    || t.starts_with('\n')
                    || cur_text.is_empty()
                    || (from_ms > cur_end + 350 && !trimmed.chars().all(|c| c.is_ascii_punctuation()));
                if starts_new_word && !cur_text.is_empty() {
                    flush(&mut words, &mut cur_text, &mut cur_start, cur_end);
                }
                if cur_start.is_none() {
                    cur_start = Some(from_ms);
                }
                cur_text.push_str(if cur_text.is_empty() { t.trim_start() } else { t });
                cur_end = to_ms.max(cur_end);
            }
        } else if let Some(text) = seg["text"].as_str() {
            flush(&mut words, &mut cur_text, &mut cur_start, cur_end);
            let s0 = seg["offsets"]["from"].as_u64().unwrap_or(0);
            let s1 = seg["offsets"]["to"].as_u64().unwrap_or(s0 + 1500).max(s0 + 200);
            let toks: Vec<&str> = text.split_whitespace().collect();
            if !toks.is_empty() {
                let step = ((s1 - s0) / toks.len() as u64).max(100);
                for (i, w) in toks.iter().enumerate() {
                    let ws = s0 + i as u64 * step;
                    let we = (ws + step).min(s1);
                    words.push(CaptionWord {
                        text: (*w).to_owned(),
                        start_ms: ws,
                        end_ms: we.max(ws + 80),
                        emoji: keyword_emoji(w).map(str::to_owned),
                        color: None,
                    });
                }
            }
        }
    }
    flush(&mut words, &mut cur_text, &mut cur_start, cur_end);

    // Ensure strictly monotonic non-overlapping bounds
    for i in 0..words.len() {
        if i + 1 < words.len() && words[i].end_ms > words[i + 1].start_ms {
            words[i].end_ms = words[i + 1].start_ms.max(words[i].start_ms + 60);
        }
    }
    words
}

pub(crate) async fn transcribe_clip(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
    Json(input): Json<TranscribeRequest>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let Some(source_path) = resolve_video(&app, &clip_id) else {
        return Err((axum::http::StatusCode::NOT_FOUND, "Source video not found.".into()));
    };
    let Some(ffmpeg) = find_ffmpeg() else {
        return Err(bad("FFmpeg not found in tools/ffmpeg.exe."));
    };
    let Some((whisper_bin, model_path)) = find_whisper_installation() else {
        return Err(bad("Local Whisper AI is not installed yet. Click 'Install Local Whisper AI' first."));
    };
    let in_ms = input.in_ms.unwrap_or(0);
    let out_ms_opt = input.out_ms;
    let lang = input
        .language
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 8)
        .unwrap_or("en")
        .to_owned();

    let result = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let dur_ms = probe_mp4_summary(&source_path)
            .map(|s| (s.duration_s * 1000.0).round() as u64)
            .unwrap_or(60_000);
        let end_ms = out_ms_opt.filter(|&e| e > in_ms).unwrap_or(dur_ms).min(dur_ms.max(1000));
        let span_ms = end_ms.saturating_sub(in_ms).max(500);

        let stamp = uuid::Uuid::new_v4().simple();
        let wav_path = std::env::temp_dir().join(format!("snapmatic-whisper-{stamp}.wav"));
        let out_base = std::env::temp_dir().join(format!("snapmatic-whisper-{stamp}"));

        // Extract crisp 16kHz mono PCM WAV
        let mut ex = std::process::Command::new(&ffmpeg);
        ex.args(["-y", "-hide_banner", "-loglevel", "error"]);
        if in_ms > 0 {
            ex.args(["-ss", &format!("{:.3}", in_ms as f64 / 1000.0)]);
        }
        ex.arg("-i").arg(&source_path);
        ex.args([
            "-t",
            &format!("{:.3}", span_ms as f64 / 1000.0),
            "-vn",
            "-ar",
            "16000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&wav_path);
        hide_window(&mut ex);
        let ex_out = ex.output().map_err(|e| format!("Audio extraction failed: {e}"))?;
        if !ex_out.status.success() || !wav_path.is_file() {
            return Err("Could not extract audio track from this clip (clip may have no audio).".to_owned());
        }

        // Run whisper-cli with full JSON token timestamps (-ojf)
        let mut wcmd = std::process::Command::new(&whisper_bin);
        if let Some(parent) = whisper_bin.parent() {
            wcmd.current_dir(parent);
        }
        wcmd.arg("-m")
            .arg(&model_path)
            .arg("-f")
            .arg(&wav_path)
            .args(["-l", &lang, "-ojf", "-of"])
            .arg(&out_base);
        hide_window(&mut wcmd);
        let w_out = wcmd
            .output()
            .map_err(|e| format!("Failed launching whisper-cli: {e}"))?;
        let json_path = PathBuf::from(format!("{}.json", out_base.display()));
        let raw_json = if w_out.status.success() && json_path.is_file() {
            std::fs::read_to_string(&json_path).ok()
        } else {
            None
        };
        let _ = std::fs::remove_file(&wav_path);
        let _ = std::fs::remove_file(&json_path);

        let Some(raw) = raw_json else {
            let err = String::from_utf8_lossy(&w_out.stderr);
            return Err(format!("Whisper transcription failed: {}", err.lines().rev().take(3).collect::<Vec<_>>().join(" ")));
        };
        let words = parse_whisper_json_words(&raw);
        let count = words.len();
        Ok(json!({
            "words": words,
            "word_count": count,
            "duration_ms": span_ms,
            "engine": "whisper-local"
        }))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;

    Ok(Json(result))
}

// ==========================================================================
// 3. ANIMATED WORD-BY-WORD .ASS KARAOKE SUBTITLE GENERATOR
// ==========================================================================

fn hex_rgb_to_ass_bgr(hex: &str, alpha_hex: &str) -> String {
    let h = hex.trim().trim_start_matches('#');
    if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()) {
        let r = &h[0..2];
        let g = &h[2..4];
        let b = &h[4..6];
        format!("&H{alpha_hex}{b}{g}{r}").to_uppercase()
    } else {
        format!("&H{alpha_hex}00E6FF")
    }
}

fn fmt_ass_time(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1000;
    let cs = (ms % 1000) / 10;
    format!("{h}:{m:02}:{s:02}.{cs:02}")
}

fn escape_ass_text(s: &str) -> String {
    s.replace('\\', "")
        .replace('{', "(")
        .replace('}', ")")
        .replace('\r', "")
        .replace('\n', " ")
}

pub(crate) fn generate_ass_subtitles(words: &[CaptionWord], cfg: &CaptionStyleConfig) -> String {
    let wpl = cfg.words_per_line.clamp(1, 6);
    let font_sz = cfg.font_size.clamp(34, 110);
    // PlayResX=1080, PlayResY=1920 -> MarginV from bottom
    let pos_y = cfg.pos_y_pct.clamp(12, 90);
    let margin_v = ((100 - pos_y) * 1920 / 100).max(90);

    let (font_name, base_color, outline_color, back_color, border_style, outline_w, shadow_d) =
        match cfg.preset.as_str() {
            "mrbeast" => ("Impact", "&H00FFFFFF", "&H00000000", "&H90000000", 1, 6, 4),
            "vice_neon" => ("Arial Black", "&H00FFFFFF", "&H00932EFF", "&H80120809", 1, 5, 3),
            "submagic_pill" => ("Arial Black", "&H00FFFFFF", "&H00120809", "&HD0120809", 3, 14, 0),
            "minimal_clean" => ("Segoe UI", "&H00F5F3FF", "&H00090812", "&H80090812", 1, 3, 2),
            _ => ("Arial Black", "&H00FFFFFF", "&H00000000", "&H80000000", 1, 5, 3), // hormozi
        };

    let default_active_ass = hex_rgb_to_ass_bgr(&cfg.active_color, "00");

    let mut out = format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 1080\nPlayResY: 1920\nScaledBorderAndShadow: yes\n\n\
         [V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
         Style: Default,{font_name},{font_sz},{base_color},&H0000FFFF,{outline_color},{back_color},-1,0,0,0,100,100,0,0,{border_style},{outline_w},{shadow_d},2,60,60,{margin_v},1\n\n\
         [Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
    );

    // Group words into phrases respecting words_per_line, natural pauses (>450ms), and sentence ends
    let mut phrases: Vec<Vec<&CaptionWord>> = Vec::new();
    let mut cur: Vec<&CaptionWord> = Vec::new();
    for w in words {
        if w.text.trim().is_empty() {
            continue;
        }
        let pause_gap = cur
            .last()
            .is_some_and(|prev| w.start_ms.saturating_sub(prev.end_ms) > 480);
        let prev_ended_sent = cur
            .last()
            .is_some_and(|prev| prev.text.ends_with('.') || prev.text.ends_with('!') || prev.text.ends_with('?'));
        if !cur.is_empty() && (cur.len() >= wpl || pause_gap || prev_ended_sent) {
            phrases.push(std::mem::take(&mut cur));
        }
        cur.push(w);
    }
    if !cur.is_empty() {
        phrases.push(cur);
    }

    for phrase in phrases {
        for (active_idx, active_word) in phrase.iter().enumerate() {
            let st = active_word.start_ms;
            let en = if active_idx + 1 < phrase.len() {
                phrase[active_idx + 1].start_ms.max(active_word.end_ms).max(st + 70)
            } else {
                active_word.end_ms.max(st + 120)
            };
            let active_bgr = active_word
                .color
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(|c| hex_rgb_to_ass_bgr(c, "00"))
                .unwrap_or_else(|| default_active_ass.clone());

            let mut line_parts: Vec<String> = Vec::with_capacity(phrase.len());
            for (i, w) in phrase.iter().enumerate() {
                let mut txt = escape_ass_text(w.text.trim());
                if cfg.uppercase {
                    txt = txt.to_uppercase();
                }
                if i == active_idx {
                    let pop = if cfg.pop_animation {
                        r"\fscx116\fscy116\t(0,95,\fscx100\fscy100)"
                    } else {
                        ""
                    };
                    let extra_box = if cfg.preset == "submagic_pill" {
                        format!(r"\3c{active_bgr}\c&H00090812&")
                    } else {
                        format!(r"\c{active_bgr}&")
                    };
                    let emoji_sfx = if cfg.show_emojis
                        && let Some(em) = w.emoji.as_deref().filter(|s| !s.trim().is_empty())
                    {
                        format!(" {}", escape_ass_text(em))
                    } else {
                        String::new()
                    };
                    line_parts.push(format!("{{{extra_box}{pop}}}{txt}{emoji_sfx}{{\\r}}"));
                } else {
                    line_parts.push(txt);
                }
            }
            let pop_line = if cfg.pop_animation && active_idx == 0 {
                r"{\fscx92\fscy92\t(0,70,\fscx100\fscy100)}"
            } else {
                ""
            };
            out.push_str(&format!(
                "Dialogue: 0,{},{},Default,,0,0,0,,{}{}\n",
                fmt_ass_time(st),
                fmt_ass_time(en),
                pop_line,
                line_parts.join(" ")
            ));
        }
    }
    out
}

// ==========================================================================
// 4. OPUSCLIP-STYLE MULTI-HIGHLIGHT FINDER
// ==========================================================================

pub(crate) async fn find_highlights(
    State(app): State<App>,
    AxumPath(clip_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    if !crate::workspace_media::hex(&clip_id) {
        return Err(bad("Invalid clip ID."));
    }
    let Some(source_path) = resolve_video(&app, &clip_id) else {
        return Err((axum::http::StatusCode::NOT_FOUND, "Source video not found.".into()));
    };
    let Some(ffmpeg) = find_ffmpeg() else {
        return Err(bad("FFmpeg not found in tools/ffmpeg.exe."));
    };
    let dur_hint = probed_duration_ms(&app, &clip_id).unwrap_or(0);

    let res = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let dur_ms = if dur_hint > 0 {
            dur_hint
        } else {
            probe_mp4_summary(&source_path)
                .map(|s| (s.duration_s * 1000.0).round() as u64)
                .unwrap_or(60_000)
        };
        let mut cmd = std::process::Command::new(&ffmpeg);
        cmd.args(["-hide_banner", "-nostats", "-i"])
            .arg(&source_path)
            .args([
                "-vn",
                "-af",
                "aresample=16000,asetnsamples=16000,astats=metadata=1:reset=1,ametadata=print:key=lavfi.astats.Overall.RMS_level:file=-",
                "-f",
                "null",
                "-",
            ]);
        hide_window(&mut cmd);
        let out = cmd.output().map_err(|e| format!("Highlight energy scan failed: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut buckets: Vec<(u64, f64)> = Vec::new();
        let mut cur_sec = 0u64;
        for line in text.lines() {
            if let Some(rest) = line.split("pts_time:").nth(1)
                && let Some(t) = rest.split_whitespace().next().and_then(|s| s.parse::<f64>().ok())
            {
                cur_sec = t.max(0.0).round() as u64;
            } else if let Some(val_str) = line.strip_prefix("lavfi.astats.Overall.RMS_level=")
                && let Ok(db) = val_str.trim().parse::<f64>()
                && db.is_finite()
            {
                let energy = ((db + 55.0) / 55.0).clamp(0.0, 1.0);
                buckets.push((cur_sec * 1000, energy));
            }
        }
        if buckets.is_empty() {
            let cap = dur_ms.min(55_000);
            return Ok(json!({
                "duration_ms": dur_ms,
                "highlights": [{
                    "rank": 1,
                    "score": 82,
                    "start_ms": 0,
                    "end_ms": cap,
                    "peak_ms": cap / 2,
                    "duration_s": (cap as f64 / 1000.0).round() as u64,
                    "label": "Opening Highlight"
                }]
            }));
        }

        // Evaluate candidate windows (28s to 55s depending on clip length)
        let win_secs = if buckets.len() <= 45 {
            buckets.len().max(5)
        } else if buckets.len() <= 90 {
            38
        } else {
            48
        };
        let step = (win_secs / 4).max(4);
        let mut scored: Vec<(f64, u64, u64, u64)> = Vec::new();
        let mut i = 0usize;
        while i + win_secs <= buckets.len() {
            let slice = &buckets[i..i + win_secs];
            let avg = slice.iter().map(|(_, e)| *e).sum::<f64>() / win_secs as f64;
            let (peak_ms, peak_e) = slice
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .copied()
                .unwrap_or((slice[0].0, 0.0));
            // Speech-activity density (seconds with energy > 0.35)
            let active_ratio = slice.iter().filter(|(_, e)| *e > 0.35).count() as f64 / win_secs as f64;
            // Energy variance / hype dynamics
            let var = slice.iter().map(|(_, e)| (*e - avg).powi(2)).sum::<f64>() / win_secs as f64;
            let raw_score = avg * 0.42 + peak_e * 0.28 + active_ratio * 0.22 + var.sqrt() * 0.08;
            let start_ms = slice[0].0;
            let end_ms = (start_ms + (win_secs as u64) * 1000).min(dur_ms);
            scored.push((raw_score, start_ms, end_ms, peak_ms));
            i += step;
        }
        if scored.is_empty() {
            let cap = dur_ms.min(55_000);
            scored.push((0.8, 0, cap, cap / 2));
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let top_raw = scored[0].0.max(0.01);

        let mut picked: Vec<Value> = Vec::new();
        let labels = [
            "🔥 Top Viral Peak",
            "⚡ High-Energy Moment",
            "🎯 Strong Pacing Cut",
            "💎 Bonus Highlight",
        ];
        for (raw, st, en, pk) in scored {
            let overlaps = picked.iter().any(|p| {
                let ps = p["start_ms"].as_u64().unwrap_or(0);
                let pe = p["end_ms"].as_u64().unwrap_or(0);
                let ov = en.min(pe).saturating_sub(st.max(ps));
                ov > (en.saturating_sub(st)) / 2
            });
            if overlaps {
                continue;
            }
            let rank = picked.len() + 1;
            let norm_score = ((raw / top_raw) * 20.0 + 78.0).round().clamp(68.0, 99.0) as u64;
            picked.push(json!({
                "rank": rank,
                "score": norm_score,
                "start_ms": st,
                "end_ms": en,
                "peak_ms": pk,
                "duration_s": (en.saturating_sub(st) as f64 / 1000.0).round() as u64,
                "label": labels.get(rank - 1).copied().unwrap_or("Highlight")
            }));
            if picked.len() >= 4 {
                break;
            }
        }
        Ok(json!({
            "duration_ms": dur_ms,
            "highlights": picked
        }))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;

    Ok(Json(res))
}

// ==========================================================================
// 5. STUDIO MEDIA UPLOAD (CLEAN VOICEOVER + .CUBE 3D LUTS)
// ==========================================================================

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
                if matches!(k.as_str(), "voiceover" | "lut") {
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
                .map_err(|_| bad("File exceeded 120 MiB upload limit."))?;
            if bytes.is_empty() || bytes.len() > 120 * 1024 * 1024 {
                return Err(bad("Uploaded file must be between 1 byte and 120 MiB."));
            }
            data_opt = Some(bytes.to_vec());
        }
    }
    let Some(data) = data_opt else {
        return Err(bad("No file provided."));
    };
    let ext = Path::new(&orig_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| if kind == "lut" { "cube".to_owned() } else { "webm".to_owned() });

    if kind == "lut" {
        if ext != "cube" {
            return Err(bad("LUTs must be .cube files."));
        }
        let stem = Path::new(&orig_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("custom-lut")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
            .take(48)
            .collect::<String>();
        let dest = studio_luts_dir(&app).join(format!("{stem}.cube"));
        std::fs::write(&dest, &data).map_err(internal)?;
        return Ok(Json(json!({
            "kind": "lut",
            "name": format!("{stem}.cube"),
            "path": dest.to_string_lossy(),
            "bytes": data.len()
        })));
    }

    if !matches!(ext.as_str(), "webm" | "wav" | "mp3" | "m4a" | "ogg") {
        return Err(bad("Unsupported audio format."));
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let file_name = format!("vo-{id}.{ext}");
    let dest = studio_media_dir(&app).join(&file_name);
    std::fs::write(&dest, &data).map_err(internal)?;
    Ok(Json(json!({
        "id": id,
        "kind": "voiceover",
        "file_name": file_name,
        "path": dest.to_string_lossy(),
        "bytes": data.len()
    })))
}

pub(crate) async fn list_luts(State(app): State<App>) -> Json<Value> {
    let dir = studio_luts_dir(&app);
    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for ent in entries.flatten() {
            let p = ent.path();
            if p.is_file() && p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("cube")) {
                items.push(json!({
                    "name": p.file_name().and_then(|s| s.to_str()).unwrap_or("lut.cube"),
                    "path": p.to_string_lossy()
                }));
            }
        }
    }
    Json(json!({ "luts": items }))
}

// ==========================================================================
// 6. PRO COLOR GRADES, STUDIO VOICE ENHANCE & EXPORT POST-PASSES
// ==========================================================================

pub(crate) fn color_grade_filter(grade: Option<&str>) -> Option<&'static str> {
    match grade.map(str::trim).unwrap_or("none") {
        "crisp_rec709" => Some("eq=contrast=1.06:saturation=1.10:brightness=0.01,unsharp=5:5:0.65:5:5:0.0"),
        "warm_cinema" => Some("eq=contrast=1.08:saturation=1.12:brightness=0.01,colorbalance=rs=0.05:gs=0.02:bs=-0.05:rh=0.06:gh=0.03:bh=-0.05"),
        "vice_night" => Some("eq=contrast=1.10:saturation=1.20:brightness=0.0,colorbalance=rs=0.06:bs=0.09:rm=0.03:bm=0.05"),
        "hdr_gaming" => Some("eq=contrast=1.12:saturation=1.18:gamma=1.02,unsharp=3:3:0.8:3:3:0.0"),
        _ => None,
    }
}

pub(crate) fn studio_voice_enhance_filter() -> &'static str {
    "highpass=f=80,afftdn=nf=-24,equalizer=f=180:width_type=o:width=1.2:g=2.5,equalizer=f=3200:width_type=o:width=1.4:g=3.2,equalizer=f=6500:width_type=o:width=1.0:g=-2.0,acompressor=threshold=-18dB:ratio=3.2:attack=10:release=120,loudnorm=I=-14:TP=-1.5:LRA=11"
}

pub(crate) fn build_studio_video_filters(
    opts: &StudioExportOptions,
    work_dir: &Path,
) -> (Vec<String>, Option<PathBuf>) {
    let mut filters = Vec::new();

    // Tool 5: Real .cube 3D LUT OR tasteful Pro Color Grade
    if let Some(lp) = opts.lut_path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pb = PathBuf::from(lp);
        if pb.is_file() && pb.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("cube")) {
            filters.push(format!("lut3d=file='{}'", escape_ffmpeg_filter_path(&pb)));
        }
    } else if let Some(cg) = color_grade_filter(opts.color_grade.as_deref()) {
        filters.push(cg.to_owned());
    }

    // Tool 1: Burn-in animated word-by-word .ass karaoke subtitles (if silence jump-cut is OFF; if ON, applied AFTER silence jump-cut!)
    let mut temp_ass = None;
    if !opts.caption_words.is_empty() && !opts.remove_mid_silences {
        let cfg = opts.caption_style.clone().unwrap_or_default();
        let ass_content = generate_ass_subtitles(&opts.caption_words, &cfg);
        let ass_path = work_dir.join(format!("autocap-{}.ass", uuid::Uuid::new_v4().simple()));
        if std::fs::write(&ass_path, ass_content).is_ok() {
            filters.push(format!("ass='{}'", escape_ffmpeg_filter_path(&ass_path)));
            temp_ass = Some(ass_path);
        }
    }

    (filters, temp_ass)
}

fn has_audio_stream(ffmpeg: &Path, path: &Path) -> bool {
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-i"]).arg(path);
    hide_window(&mut cmd);
    cmd.output()
        .map(|o| String::from_utf8_lossy(&o.stderr).contains("Audio:"))
        .unwrap_or(true)
}

fn run_simple_ffmpeg(ffmpeg: &Path, args: &[String]) -> Result<(), String> {
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(args);
    hide_window(&mut cmd);
    let out = cmd
        .output()
        .map_err(|e| format!("FFmpeg studio pass failed to start: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail = err.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
        return Err(format!("FFmpeg studio pass failed: {tail}"));
    }
    Ok(())
}

pub(crate) fn needs_studio_post_passes(opts: &StudioExportOptions) -> bool {
    opts.remove_mid_silences
        || opts.voiceover_path.as_deref().is_some_and(|s| !s.trim().is_empty())
}

// Tool 2: Mid-Clip Jump-Cut Silence Remover + Tool 4: Clean Mic Voiceover Auto-Ducking
pub(crate) fn apply_studio_post_passes(
    ffmpeg: &Path,
    mp4_path: &Path,
    opts: &StudioExportOptions,
) -> Result<(), String> {
    if !needs_studio_post_passes(opts) {
        return Ok(());
    }
    let work_dir = mp4_path.parent().unwrap_or_else(|| Path::new("."));
    let stamp = uuid::Uuid::new_v4().simple().to_string();

    // PASS A: Mid-Clip Jump-Cut Silence Remover
    let mut kept_segments_ms: Option<Vec<(u64, u64)>> = None;
    if opts.remove_mid_silences && has_audio_stream(ffmpeg, mp4_path) {
        let dur_s = probe_mp4_summary(mp4_path).map(|s| s.duration_s).unwrap_or(0.0);
        if dur_s > 1.5 {
            let db = opts.silence_threshold_db.unwrap_or(-32.0).clamp(-50.0, -18.0);
            let min_d = opts.min_silence_s.unwrap_or(0.42).clamp(0.20, 2.0);
            let mut det = std::process::Command::new(ffmpeg);
            det.args(["-hide_banner", "-i"])
                .arg(mp4_path)
                .args(["-af", &format!("silencedetect=n={db:.1}dB:d={min_d:.2}"), "-f", "null", "-"]);
            hide_window(&mut det);
            if let Ok(out) = det.output() {
                let err = String::from_utf8_lossy(&out.stderr);
                let mut silences: Vec<(f64, f64)> = Vec::new();
                let mut cur_st: Option<f64> = None;
                for line in err.lines() {
                    if let Some(rest) = line.split("silence_start:").nth(1) {
                        cur_st = rest.split_whitespace().next().and_then(|s| s.parse::<f64>().ok());
                    } else if let Some(rest) = line.split("silence_end:").nth(1)
                        && let Some(en) = rest.split_whitespace().next().and_then(|s| s.parse::<f64>().ok())
                        && let Some(st) = cur_st.take()
                        && en > st + 0.15
                    {
                        silences.push((st, en));
                    }
                }
                if let Some(st) = cur_st {
                    if dur_s > st + 0.15 {
                        silences.push((st, dur_s));
                    }
                }
                // Convert silent spans into kept speech spans with 0.08s natural breathing pad
                let mut kept: Vec<(f64, f64)> = Vec::new();
                let mut cursor = 0.0f64;
                for (ss, se) in silences {
                    let seg_end = (ss + 0.08).min(dur_s);
                    if seg_end > cursor + 0.18 {
                        kept.push((cursor, seg_end));
                    }
                    cursor = (se - 0.08).max(seg_end);
                }
                if dur_s > cursor + 0.18 {
                    kept.push((cursor, dur_s));
                }
                if !kept.is_empty() && kept.len() <= 60 && (kept.len() > 1 || (kept[0].1 - kept[0].0) < dur_s - 0.35) {
                    let mut fc = String::new();
                    let mut concat_in = String::new();
                    for (idx, (ks, ke)) in kept.iter().enumerate() {
                        fc.push_str(&format!(
                            "[0:v]trim=start={ks:.3}:end={ke:.3},setpts=PTS-STARTPTS[v{idx}];\
                             [0:a]atrim=start={ks:.3}:end={ke:.3},asetpts=PTS-STARTPTS[a{idx}];"
                        ));
                        concat_in.push_str(&format!("[v{idx}][a{idx}]"));
                    }
                    let n = kept.len();
                    fc.push_str(&format!("{concat_in}concat=n={n}:v=1:a=1[vcat][acat]"));
                    let cut_out = work_dir.join(format!("jumpcut-{stamp}.mp4"));
                    let args: Vec<String> = vec![
                        "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
                        "-i".into(), mp4_path.to_string_lossy().into_owned(),
                        "-filter_complex".into(), fc,
                        "-map".into(), "[vcat]".into(), "-map".into(), "[acat]".into(),
                        "-c:v".into(), "libx264".into(), "-preset".into(), "fast".into(), "-crf".into(), "19".into(),
                        "-pix_fmt".into(), "yuv420p".into(),
                        "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-ar".into(), "48000".into(), "-ac".into(), "2".into(),
                        "-movflags".into(), "+faststart".into(),
                        cut_out.to_string_lossy().into_owned(),
                    ];
                    run_simple_ffmpeg(ffmpeg, &args)?;
                    std::fs::rename(&cut_out, mp4_path)
                        .map_err(|e| format!("Failed finalizing jump-cut MP4: {e}"))?;
                    kept_segments_ms = Some(
                        kept.into_iter()
                            .map(|(a, b)| ((a * 1000.0).round() as u64, (b * 1000.0).round() as u64))
                            .collect(),
                    );
                }
            }
        }

        // If captions were provided AND remove_mid_silences was ON, remap caption timestamps to the jump-cut timeline and burn .ass now!
        if !opts.caption_words.is_empty() {
            let remapped: Vec<CaptionWord> = if let Some(ref segs) = kept_segments_ms {
                let mut out_words = Vec::new();
                for w in &opts.caption_words {
                    let mid = (w.start_ms + w.end_ms) / 2;
                    let mut acc = 0u64;
                    for &(ks, ke) in segs {
                        if mid >= ks && mid <= ke {
                            let ns = acc + w.start_ms.saturating_sub(ks);
                            let ne = acc + w.end_ms.min(ke).saturating_sub(ks);
                            let mut cloned = w.clone();
                            cloned.start_ms = ns;
                            cloned.end_ms = ne.max(ns + 80);
                            out_words.push(cloned);
                            break;
                        }
                        acc += ke.saturating_sub(ks);
                    }
                }
                out_words
            } else {
                opts.caption_words.clone()
            };
            if !remapped.is_empty() {
                let cfg = opts.caption_style.clone().unwrap_or_default();
                let ass_path = work_dir.join(format!("autocap-post-{stamp}.ass"));
                if std::fs::write(&ass_path, generate_ass_subtitles(&remapped, &cfg)).is_ok() {
                    let cap_out = work_dir.join(format!("autocap-post-{stamp}.mp4"));
                    let res = run_simple_ffmpeg(ffmpeg, &[
                        "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
                        "-i".into(), mp4_path.to_string_lossy().into_owned(),
                        "-vf".into(), format!("ass='{}'", escape_ffmpeg_filter_path(&ass_path)),
                        "-c:v".into(), "libx264".into(), "-preset".into(), "fast".into(), "-crf".into(), "19".into(),
                        "-pix_fmt".into(), "yuv420p".into(),
                        "-c:a".into(), "copy".into(),
                        "-movflags".into(), "+faststart".into(),
                        cap_out.to_string_lossy().into_owned(),
                    ]);
                    let _ = std::fs::remove_file(&ass_path);
                    res?;
                    std::fs::rename(&cap_out, mp4_path)
                        .map_err(|e| format!("Failed finalizing post-cut captions: {e}"))?;
                }
            }
        }
    }

    // PASS B: Clean Mic Voiceover + Auto-Ducking
    if let Some(vp) = opts
        .voiceover_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        let src_has_audio = has_audio_stream(ffmpeg, mp4_path);
        let gain = opts.voiceover_gain_db.unwrap_or(2.0).clamp(-10.0, 12.0);
        let duck = opts.voiceover_duck_db.unwrap_or(-14.0).clamp(-26.0, 0.0);
        let vo_polish = if opts.studio_voice_enhance {
            format!(",{}", studio_voice_enhance_filter())
        } else {
            ",highpass=f=80,acompressor=threshold=-18dB:ratio=3".to_owned()
        };
        let fc = if src_has_audio && duck < -1.0 {
            format!(
                "[0:a]aresample=48000,aformat=channel_layouts=stereo[a0];\
                 [1:a]aresample=48000,aformat=channel_layouts=stereo{vo_polish},volume={gain:.1}dB,asplit=2[vo_mix][vo_sc];\
                 [a0][vo_sc]sidechaincompress=threshold=0.025:ratio=8:attack=25:release=320:makeup=1[aduck];\
                 [aduck]volume={:.1}dB[aduck2];\
                 [aduck2][vo_mix]amix=inputs=2:duration=first:normalize=0[aout]",
                (duck * 0.25).clamp(-6.0, 0.0)
            )
        } else if src_has_audio {
            format!(
                "[0:a]aresample=48000,aformat=channel_layouts=stereo[a0];\
                 [1:a]aresample=48000,aformat=channel_layouts=stereo{vo_polish},volume={gain:.1}dB[vo_mix];\
                 [a0][vo_mix]amix=inputs=2:duration=first:normalize=0[aout]"
            )
        } else {
            format!(
                "[1:a]aresample=48000,aformat=channel_layouts=stereo{vo_polish},volume={gain:.1}dB[aout]"
            )
        };
        let vo_out = work_dir.join(format!("vo-mix-{stamp}.mp4"));
        run_simple_ffmpeg(ffmpeg, &[
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
            "-i".into(), mp4_path.to_string_lossy().into_owned(),
            "-i".into(), vp.to_string_lossy().into_owned(),
            "-filter_complex".into(), fc,
            "-map".into(), "0:v".into(),
            "-map".into(), "[aout]".into(),
            "-c:v".into(), "copy".into(),
            "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-ar".into(), "48000".into(), "-ac".into(), "2".into(),
            "-shortest".into(), "-movflags".into(), "+faststart".into(),
            vo_out.to_string_lossy().into_owned(),
        ])?;
        std::fs::rename(&vo_out, mp4_path)
            .map_err(|e| format!("Failed finalizing voiceover mix: {e}"))?;
    }

    Ok(())
}
