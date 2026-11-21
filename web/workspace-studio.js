'use strict';
(() => {
const $ = id => document.getElementById(id);
const app = () => window.snapmaticApp;

const GRADE_CSS = {
 none: '',
 vice_sunset: 'contrast(1.1) saturate(1.28) hue-rotate(-6deg)',
 miami_gold: 'contrast(1.06) saturate(1.22) sepia(0.14) brightness(1.03)',
 night_heist: 'contrast(1.16) saturate(0.88) brightness(0.95) hue-rotate(12deg)',
 vhs_1986: 'contrast(1.1) saturate(1.18) sepia(0.08)',
 ultra_crisp: 'contrast(1.08) saturate(1.14)',
};

const SCRIPT_TEMPLATES = {
 breakdown: 'Stop scrolling — 99% of GTA 6 players completely missed this detail. Look closely at what happens right here in the background. Rockstar literally programmed realistic reactions into every NPC in Vice City. Would you try this on Day 1? Drop your take in the comments!',
 hottake: 'Unpopular opinion about GTA 6 — this single feature is going to change open-world gaming forever. Watch how smooth the physics react right here. Let me know if you think this is a W or an L!',
 didyouknow: 'Did you know you can actually pull this off in GTA 6? Most people speed right past this spot without ever noticing what triggers when you wait two seconds. Follow for daily Vice City secrets!',
 top3: 'Here are the 3 wildest GTA 6 details fans just uncovered. Number 1 is already insane, wait until you see number 3 at the end. Which one surprised you the most?',
};

const state = {
 voiceover_path: null,
 voiceover_blob_url: null,
 reaction_cam_path: null,
 freeze_at_ms: null,
 zoom_punch_at_ms: null,
 replay_start_ms: null,
 replay_end_ms: null,
 countdown_at_ms: null,
 color_grade: 'none',
 commentary_cards: [],
 sfx_cues: [],
 montage_clips: [],
};

let voRecorder = null, voChunks = [], voStream = null, voMeterRaf = null, voAudioCtx = null;
let camRecorder = null, camChunks = [], camStream = null;
let tpRunning = false, tpRaf = null, tpLastTs = 0;

function activeFxList() {
 const list = [];
 if (state.voiceover_path) list.push('Voiceover');
 if (state.reaction_cam_path) list.push('Reaction Cam');
 if (($('studio-voice-fx')?.value || 'none') !== 'none') list.push('Mic FX');
 if (state.freeze_at_ms != null) list.push('Freeze-Zoom');
 if (state.zoom_punch_at_ms != null) list.push('Action Punch');
 if (state.replay_start_ms != null && state.replay_end_ms != null) list.push('0.5× Replay');
 if (state.color_grade !== 'none') list.push('Grade:' + state.color_grade);
 if (($('studio-weazel-head')?.value || '').trim()) list.push('Weazel News');
 if (($('studio-credit-handle')?.value || '').trim()) list.push('Credit');
 if (state.commentary_cards.length) list.push(state.commentary_cards.length + ' Card' + (state.commentary_cards.length === 1 ? '' : 's'));
 if (($('studio-poll-prompt')?.value || '').trim()) list.push('Poll');
 if ($('studio-progress-bar')?.checked) list.push('Progress Bar');
 if (state.countdown_at_ms != null) list.push('3-2-1');
 if (($('studio-intro-sting')?.value || '').trim()) list.push('Intro');
 if (($('studio-outro-card')?.value || '').trim()) list.push('Outro');
 if (state.sfx_cues.length) list.push(state.sfx_cues.length + ' SFX');
 return list;
}

function syncStudioSummary() {
 const list = activeFxList();
 const badge = $('itab-create-badge');
 if (badge) {
  badge.textContent = String(list.length);
  badge.hidden = list.length === 0;
 }
 const sum = $('studio-active-summary');
 if (sum) {
  sum.textContent = list.length ? 'Active Studio FX (' + list.length + '): ' + list.join(' · ') : 'No Studio FX active yet.';
 }
 const fxOut = $('studio-fx-summary');
 if (fxOut) {
  const parts = [];
  if (state.freeze_at_ms != null) parts.push('Freeze @ ' + app().time(state.freeze_at_ms / 1000).slice(0, 5));
  if (state.zoom_punch_at_ms != null) parts.push('Punch-In @ ' + app().time(state.zoom_punch_at_ms / 1000).slice(0, 5));
  if (state.replay_start_ms != null && state.replay_end_ms != null) {
   parts.push('0.5× Replay ' + app().time(state.replay_start_ms / 1000).slice(0, 5) + '→' + app().time(state.replay_end_ms / 1000).slice(0, 5));
  } else if (state.replay_start_ms != null) {
   parts.push('Replay Start @ ' + app().time(state.replay_start_ms / 1000).slice(0, 5) + ' (set End)');
  }
  fxOut.textContent = parts.length ? parts.join(' · ') : 'Seek the video to any moment and click a button to pin FX.';
 }
 updateLiveStudioPreviews();
}

function updateLiveStudioPreviews() {
 const v = app()?.getVideo();
 if (v && state.color_grade !== 'none') {
  v.style.filter = GRADE_CSS[state.color_grade] || '';
 } else if (v && !$('edit-vivid')?.checked) {
  v.style.filter = '';
 }
 const wz = $('vf-weazel-preview');
 const head = ($('studio-weazel-head')?.value || '').trim();
 const tick = ($('studio-weazel-tick')?.value || '').trim();
 if (wz) {
  wz.hidden = !head;
  if ($('vf-weazel-head-text')) $('vf-weazel-head-text').textContent = head;
  if ($('vf-weazel-tick-text')) $('vf-weazel-tick-text').textContent = tick || 'WEAZEL NEWS VICE CITY LIVE · GTA 6 BREAKING COVERAGE';
 }
 const bar = $('vf-progress-preview');
 if (bar) {
  const show = Boolean($('studio-progress-bar')?.checked);
  bar.hidden = !show;
  if (show && v && Number.isFinite(v.duration) && v.duration > 0) {
   bar.style.width = Math.max(0, Math.min(100, (v.currentTime / v.duration) * 100)) + '%';
  }
 }
 const camEl = $('vf-cam-preview');
 if (camEl) {
  camEl.dataset.layout = $('studio-cam-layout')?.value || 'corner_tr';
 }
}

async function uploadStudioBlob(blob, kind, fileName) {
 const fd = new FormData();
 fd.append('kind', kind);
 fd.append('file', blob, fileName);
 const r = await fetch('/api/workspace/studio/upload-media', { method: 'POST', credentials: 'same-origin', body: fd });
 if (!r.ok) {
  const txt = (await r.text()).trim();
  throw Error(txt || 'Studio media upload failed.');
 }
 return r.json();
}

// #1 & #16: Mic Voiceover Recorder + RMS Meter + Auto-Ducking
function stopVoMeter() {
 if (voMeterRaf) { cancelAnimationFrame(voMeterRaf); voMeterRaf = null; }
 if (voAudioCtx) { voAudioCtx.close().catch(() => {}); voAudioCtx = null; }
 if ($('studio-vo-meter-wrap')) $('studio-vo-meter-wrap').hidden = true;
}

if ($('studio-vo-rec')) $('studio-vo-rec').onclick = async () => {
 if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === 'undefined') {
  app().note('Microphone recording is not supported in this browser context.');
  return;
 }
 try {
  voStream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
  voChunks = [];
  voRecorder = new MediaRecorder(voStream);
  voRecorder.ondataavailable = e => { if (e.data && e.data.size > 0) voChunks.push(e.data); };
  voRecorder.onstop = async () => {
   stopVoMeter();
   voStream?.getTracks().forEach(t => t.stop());
   voStream = null;
   const blob = new Blob(voChunks, { type: voRecorder.mimeType || 'audio/webm' });
   if ($('studio-vo-status')) $('studio-vo-status').textContent = 'Saving voiceover track locally…';
   try {
    const res = await uploadStudioBlob(blob, 'voiceover', 'voiceover.webm');
    state.voiceover_path = res.path;
    if (state.voiceover_blob_url) URL.revokeObjectURL(state.voiceover_blob_url);
    state.voiceover_blob_url = URL.createObjectURL(blob);
    if ($('studio-vo-audio')) { $('studio-vo-audio').src = state.voiceover_blob_url; $('studio-vo-audio').hidden = false; }
    if ($('studio-vo-clear')) $('studio-vo-clear').hidden = false;
    if ($('studio-vo-status')) $('studio-vo-status').textContent = '✓ Mic voiceover ready (' + (res.bytes / 1024).toFixed(0) + ' KiB) — auto-ducks clip on export!';
    syncStudioSummary();
    app().note('Recorded mic voiceover — ready to mix with auto-ducking on export!');
   } catch (err) {
    if ($('studio-vo-status')) $('studio-vo-status').textContent = 'Save failed: ' + err.message;
   }
  };

  // Start RMS meter
  try {
   voAudioCtx = new (window.AudioContext || window.webkitAudioContext)();
   const src = voAudioCtx.createMediaStreamSource(voStream);
   const analyser = voAudioCtx.createAnalyser();
   analyser.fftSize = 256;
   src.connect(analyser);
   const data = new Uint8Array(analyser.frequencyBinCount);
   if ($('studio-vo-meter-wrap')) $('studio-vo-meter-wrap').hidden = false;
   const tick = () => {
    analyser.getByteTimeDomainData(data);
    let sum = 0;
    for (let i = 0; i < data.length; i++) { const v = (data[i] - 128) / 128; sum += v * v; }
    const rms = Math.min(1, Math.sqrt(sum / data.length) * 3.5);
    if ($('studio-vo-meter')) $('studio-vo-meter').style.width = Math.round(rms * 100) + '%';
    voMeterRaf = requestAnimationFrame(tick);
   };
   tick();
  } catch {}

  voRecorder.start();
  $('studio-vo-rec').hidden = true;
  if ($('studio-vo-stop')) $('studio-vo-stop').hidden = false;
  if ($('studio-vo-status')) $('studio-vo-status').textContent = '● Recording mic voiceover… speak now!';
  const v = app().getVideo();
  if (v && v.readyState >= 2) { v.currentTime = (app().getEditState()?.in_ms || 0) / 1000; v.play().catch(() => {}); }
  if (!$('vf-teleprompter')?.hidden) startTeleprompterScroll();
 } catch (e) {
  app().note('Could not access microphone: ' + e.message);
 }
};

if ($('studio-vo-stop')) $('studio-vo-stop').onclick = () => {
 if (voRecorder && voRecorder.state !== 'inactive') voRecorder.stop();
 $('studio-vo-stop').hidden = true;
 if ($('studio-vo-rec')) $('studio-vo-rec').hidden = false;
 app().getVideo()?.pause();
 stopTeleprompterScroll();
};

if ($('studio-vo-clear')) $('studio-vo-clear').onclick = () => {
 state.voiceover_path = null;
 if ($('studio-vo-audio')) { $('studio-vo-audio').hidden = true; $('studio-vo-audio').removeAttribute('src'); }
 $('studio-vo-clear').hidden = true;
 if ($('studio-vo-status')) $('studio-vo-status').textContent = 'Voiceover cleared.';
 syncStudioSummary();
};

if ($('studio-vo-duck')) $('studio-vo-duck').oninput = () => {
 if ($('studio-vo-duck-val')) $('studio-vo-duck-val').textContent = $('studio-vo-duck').value + ' dB';
};
if ($('studio-vo-gain')) $('studio-vo-gain').oninput = () => {
 const g = Number($('studio-vo-gain').value) || 0;
 if ($('studio-vo-gain-val')) $('studio-vo-gain-val').textContent = (g >= 0 ? '+' : '') + g + ' dB';
};
if ($('studio-voice-fx')) $('studio-voice-fx').onchange = syncStudioSummary;

// #8 & #18: Local TTS Narration + Viewfinder Teleprompter
document.querySelectorAll('#studio-script-presets button[data-spreset]').forEach(btn => {
 btn.onclick = () => {
  const txt = SCRIPT_TEMPLATES[btn.dataset.spreset];
  if (!txt) return;
  if ($('studio-tts-script')) $('studio-tts-script').value = txt;
  if ($('vf-tp-scroll')) $('vf-tp-scroll').textContent = txt;
  app().note('Loaded "' + btn.textContent + '" commentary script template.');
 };
});

if ($('studio-tts-script')) $('studio-tts-script').addEventListener('input', () => {
 if ($('vf-tp-scroll')) $('vf-tp-scroll').textContent = $('studio-tts-script').value || 'Type your script in the Create ✨ tab…';
});
if ($('studio-tp-wpm')) $('studio-tp-wpm').oninput = () => {
 if ($('studio-tp-wpm-val')) $('studio-tp-wpm-val').textContent = $('studio-tp-wpm').value;
};

function startTeleprompterScroll() {
 const el = $('vf-tp-scroll');
 if (!el) return;
 tpRunning = true;
 tpLastTs = performance.now();
 if ($('studio-tp-play')) $('studio-tp-play').textContent = 'Ⅱ Pause Scroll';
 const step = now => {
  if (!tpRunning) return;
  const dt = Math.min(0.1, (now - tpLastTs) / 1000);
  tpLastTs = now;
  const wpm = Number($('studio-tp-wpm')?.value) || 140;
  el.scrollTop += (wpm / 140) * 28 * dt;
  tpRaf = requestAnimationFrame(step);
 };
 tpRaf = requestAnimationFrame(step);
}
function stopTeleprompterScroll() {
 tpRunning = false;
 if (tpRaf) { cancelAnimationFrame(tpRaf); tpRaf = null; }
 if ($('studio-tp-play')) $('studio-tp-play').textContent = '▶ Scroll Script';
}

if ($('studio-tp-toggle')) $('studio-tp-toggle').onclick = () => {
 const tp = $('vf-teleprompter'), playBtn = $('studio-tp-play');
 if (!tp) return;
 const show = tp.hidden;
 tp.hidden = !show;
 if (playBtn) playBtn.hidden = !show;
 $('studio-tp-toggle').textContent = show ? 'Hide Teleprompter' : '📜 Show Teleprompter';
 if (show) {
  if ($('vf-tp-scroll')) {
   $('vf-tp-scroll').textContent = ($('studio-tts-script')?.value || '').trim() || SCRIPT_TEMPLATES.breakdown;
   $('vf-tp-scroll').scrollTop = 0;
  }
 } else {
  stopTeleprompterScroll();
 }
};
if ($('studio-tp-play')) $('studio-tp-play').onclick = () => {
 if (tpRunning) stopTeleprompterScroll(); else startTeleprompterScroll();
};

function buildTimedSrtFromScript(script, durMs) {
 const words = script.trim().split(/\s+/).filter(Boolean);
 if (!words.length) return '';
 const chunks = [];
 for (let i = 0; i < words.length; i += 3) chunks.push(words.slice(i, i + 3).join(' ').toUpperCase());
 const span = Math.max(2000, Number(durMs) || chunks.length * 1400);
 const step = Math.max(550, Math.floor(span / chunks.length));
 const fmt = ms => {
  const h = String(Math.floor(ms / 3600000)).padStart(2, '0');
  const m = String(Math.floor((ms % 3600000) / 60000)).padStart(2, '0');
  const s = String(Math.floor((ms % 60000) / 1000)).padStart(2, '0');
  const r = String(ms % 1000).padStart(3, '0');
  return h + ':' + m + ':' + s + ',' + r;
 };
 return chunks.map((line, idx) => (idx + 1) + '\n' + fmt(idx * step + 100) + ' --> ' + fmt((idx + 1) * step - 60) + '\n' + line).join('\n\n');
}

if ($('studio-tts-go')) $('studio-tts-go').onclick = async () => {
 const btn = $('studio-tts-go'), out = $('studio-tts-status');
 const text = ($('studio-tts-script')?.value || '').trim();
 if (!text) { app().note('Type or choose a narration script first.'); return; }
 btn.disabled = true;
 if (out) out.textContent = 'Generating local TTS narration WAV…';
 try {
  const r = await app().get('/api/workspace/studio/tts', {
   method: 'POST', headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({ text, rate: Number($('studio-tts-rate')?.value) || 1 })
  });
  state.voiceover_path = r.path;
  if ($('studio-vo-clear')) $('studio-vo-clear').hidden = false;
  if ($('studio-vo-status')) $('studio-vo-status').textContent = '✓ Local TTS voiceover attached (' + r.engine + ' · ' + ((r.duration_ms || 0) / 1000).toFixed(1) + 's)';
  if ($('studio-tts-sync-srt')?.checked && $('edit-srt-text')) {
   $('edit-srt-text').value = buildTimedSrtFromScript(text, r.duration_ms);
   if ($('edit-srt-style')) $('edit-srt-style').value = 'vice_neon';
  }
  if (out) out.textContent = '✓ Generated ' + ((r.duration_ms || 0) / 1000).toFixed(1) + 's narration via ' + r.engine + ($('studio-tts-sync-srt')?.checked ? ' (+ synced Vice Neon captions).' : '.');
  syncStudioSummary();
  app().note('Local TTS voiceover ready (' + r.engine + ')!');
 } catch (e) {
  if (out) out.textContent = 'TTS failed: ' + e.message;
  app().note('Local TTS failed: ' + e.message);
 } finally {
  btn.disabled = false;
 }
};

// #2: Webcam Reaction Cam Recorder
async function ensureCamPreview() {
 if (camStream) return camStream;
 if (!navigator.mediaDevices?.getUserMedia) throw Error('Webcam not available in this browser context.');
 camStream = await navigator.mediaDevices.getUserMedia({ video: { width: 640, height: 640 }, audio: false });
 const preview = $('vf-cam-preview');
 if (preview) {
  preview.srcObject = camStream;
  preview.dataset.layout = $('studio-cam-layout')?.value || 'corner_tr';
  preview.hidden = false;
 }
 return camStream;
}

if ($('studio-cam-layout')) $('studio-cam-layout').onchange = updateLiveStudioPreviews;
if ($('studio-cam-start')) $('studio-cam-start').onclick = async () => {
 try {
  if (camStream) {
   camStream.getTracks().forEach(t => t.stop());
   camStream = null;
   if ($('vf-cam-preview')) { $('vf-cam-preview').hidden = true; $('vf-cam-preview').srcObject = null; }
   $('studio-cam-start').textContent = 'Preview Cam';
  } else {
   await ensureCamPreview();
   $('studio-cam-start').textContent = 'Hide Cam';
  }
 } catch (e) { app().note('Camera error: ' + e.message); }
};

if ($('studio-cam-rec')) $('studio-cam-rec').onclick = async () => {
 try {
  const stream = await ensureCamPreview();
  camChunks = [];
  camRecorder = new MediaRecorder(stream);
  camRecorder.ondataavailable = e => { if (e.data && e.data.size > 0) camChunks.push(e.data); };
  camRecorder.onstop = async () => {
   const blob = new Blob(camChunks, { type: camRecorder.mimeType || 'video/webm' });
   if ($('studio-cam-status')) $('studio-cam-status').textContent = 'Saving webcam reaction locally…';
   try {
    const res = await uploadStudioBlob(blob, 'reaction_cam', 'reaction.webm');
    state.reaction_cam_path = res.path;
    if ($('studio-cam-clear')) $('studio-cam-clear').hidden = false;
    if ($('studio-cam-status')) $('studio-cam-status').textContent = '✓ Reaction cam ready (' + (res.bytes / 1048576).toFixed(1) + ' MiB · ' + ($('studio-cam-layout')?.value || 'corner_tr') + ')';
    syncStudioSummary();
    app().note('Saved reaction cam recording — will composite on export!');
   } catch (err) {
    if ($('studio-cam-status')) $('studio-cam-status').textContent = 'Save failed: ' + err.message;
   }
  };
  camRecorder.start();
  $('studio-cam-rec').hidden = true;
  if ($('studio-cam-stop')) $('studio-cam-stop').hidden = false;
  if ($('studio-cam-status')) $('studio-cam-status').textContent = '● Recording webcam reaction…';
  const v = app().getVideo();
  if (v && v.readyState >= 2) { v.currentTime = (app().getEditState()?.in_ms || 0) / 1000; v.play().catch(() => {}); }
 } catch (e) { app().note('Reaction cam error: ' + e.message); }
};

if ($('studio-cam-stop')) $('studio-cam-stop').onclick = () => {
 if (camRecorder && camRecorder.state !== 'inactive') camRecorder.stop();
 $('studio-cam-stop').hidden = true;
 if ($('studio-cam-rec')) $('studio-cam-rec').hidden = false;
 app().getVideo()?.pause();
};
if ($('studio-cam-clear')) $('studio-cam-clear').onclick = () => {
 state.reaction_cam_path = null;
 $('studio-cam-clear').hidden = true;
 if ($('studio-cam-status')) $('studio-cam-status').textContent = 'Reaction cam cleared.';
 syncStudioSummary();
};

// #4, #10, #11: Freeze-Frame Zoom, Keyframe Action Punch-In & Slow-Mo Replay
function curPlayheadMs() {
 const v = app()?.getVideo();
 return v && Number.isFinite(v.currentTime) ? Math.round(v.currentTime * 1000) : 0;
}

if ($('studio-fz-set')) $('studio-fz-set').onclick = () => {
 state.freeze_at_ms = curPlayheadMs();
 if ($('studio-fz-clear')) $('studio-fz-clear').hidden = false;
 syncStudioSummary();
 app().note('Pinned 2s Freeze-Frame Zoom at ' + app().time(state.freeze_at_ms / 1000).slice(0, 5) + '!');
};
if ($('studio-fz-clear')) $('studio-fz-clear').onclick = () => {
 state.freeze_at_ms = null; $('studio-fz-clear').hidden = true; syncStudioSummary();
};
if ($('studio-zp-set')) $('studio-zp-set').onclick = () => {
 state.zoom_punch_at_ms = curPlayheadMs();
 if ($('studio-zp-clear')) $('studio-zp-clear').hidden = false;
 syncStudioSummary();
 app().note('Pinned Action Punch-In (' + ($('studio-zp-scale')?.value || '1.38') + '×) at ' + app().time(state.zoom_punch_at_ms / 1000).slice(0, 5) + '!');
};
if ($('studio-zp-clear')) $('studio-zp-clear').onclick = () => {
 state.zoom_punch_at_ms = null; $('studio-zp-clear').hidden = true; syncStudioSummary();
};
if ($('studio-rp-in')) $('studio-rp-in').onclick = () => {
 state.replay_start_ms = curPlayheadMs();
 if (state.replay_end_ms == null || state.replay_end_ms <= state.replay_start_ms + 300) {
  state.replay_end_ms = state.replay_start_ms + 2500;
 }
 if ($('studio-rp-clear')) $('studio-rp-clear').hidden = false;
 syncStudioSummary();
 app().note('Set 0.5× Replay Start at ' + app().time(state.replay_start_ms / 1000).slice(0, 5) + '.');
};
if ($('studio-rp-out')) $('studio-rp-out').onclick = () => {
 const ms = curPlayheadMs();
 if (state.replay_start_ms == null) state.replay_start_ms = Math.max(0, ms - 2500);
 state.replay_end_ms = Math.max(state.replay_start_ms + 400, ms);
 if ($('studio-rp-clear')) $('studio-rp-clear').hidden = false;
 syncStudioSummary();
 app().note('Set 0.5× Replay End at ' + app().time(state.replay_end_ms / 1000).slice(0, 5) + '.');
};
if ($('studio-rp-clear')) $('studio-rp-clear').onclick = () => {
 state.replay_start_ms = null; state.replay_end_ms = null; $('studio-rp-clear').hidden = true; syncStudioSummary();
};

// #5, #7, #12, #14, #15, #17: Color Grades, Weazel News, Cards, Polls, Progress Bar, Countdown & Intro/Outro
document.querySelectorAll('#studio-grade-pills button[data-grade]').forEach(btn => {
 btn.onclick = () => {
  state.color_grade = btn.dataset.grade || 'none';
  document.querySelectorAll('#studio-grade-pills button[data-grade]').forEach(b => b.setAttribute('aria-pressed', String(b === btn)));
  syncStudioSummary();
 };
});

for (const id of ['studio-weazel-head', 'studio-weazel-tick', 'studio-credit-handle', 'studio-poll-prompt', 'studio-progress-bar', 'studio-intro-sting', 'studio-outro-card']) {
 if ($(id)) $(id).addEventListener('input', syncStudioSummary);
 if ($(id)) $(id).addEventListener('change', syncStudioSummary);
}

document.querySelectorAll('#studio-poll-presets button[data-poll]').forEach(btn => {
 btn.onclick = () => {
  if ($('studio-poll-prompt')) $('studio-poll-prompt').value = btn.dataset.poll;
  syncStudioSummary();
 };
});

function renderCommentaryCards() {
 const box = $('studio-card-list');
 if (!box) return;
 box.replaceChildren(...state.commentary_cards.map((c, idx) => {
  const chip = app().node('button', app().time(c.start_s).slice(0, 5) + ' "' + c.text.slice(0, 22) + '" ×', 'marker-chip');
  chip.type = 'button'; chip.title = 'Click to remove card';
  chip.onclick = () => { state.commentary_cards.splice(idx, 1); renderCommentaryCards(); syncStudioSummary(); };
  return chip;
 }));
}

if ($('studio-card-add')) $('studio-card-add').onclick = () => {
 const text = ($('studio-card-text')?.value || '').trim();
 if (!text) { app().note('Enter commentary card text first.'); return; }
 const start_s = curPlayheadMs() / 1000;
 state.commentary_cards.push({ start_s, end_s: start_s + 3.5, text });
 if ($('studio-card-text')) $('studio-card-text').value = '';
 renderCommentaryCards();
 syncStudioSummary();
 app().note('Added 3.5s commentary card at ' + app().time(start_s).slice(0, 5) + '.');
};

if ($('studio-cd-set')) $('studio-cd-set').onclick = () => {
 state.countdown_at_ms = Math.max(3000, curPlayheadMs());
 if ($('studio-cd-clear')) $('studio-cd-clear').hidden = false;
 syncStudioSummary();
 app().note('Pinned 3-2-1 payoff countdown finishing at ' + app().time(state.countdown_at_ms / 1000).slice(0, 5) + '.');
};
if ($('studio-cd-clear')) $('studio-cd-clear').onclick = () => {
 state.countdown_at_ms = null; $('studio-cd-clear').hidden = true; syncStudioSummary();
};

// #6: Timeline SFX Stinger Rack
function renderSfxList() {
 const box = $('studio-sfx-list');
 if (!box) return;
 box.replaceChildren(...state.sfx_cues.map((c, idx) => {
  const chip = app().node('button', '🔊 ' + c.kind + ' @ ' + app().time(c.at_s).slice(0, 5) + ' ×', 'marker-chip');
  chip.type = 'button'; chip.title = 'Click to remove SFX cue';
  chip.onclick = () => { state.sfx_cues.splice(idx, 1); renderSfxList(); syncStudioSummary(); };
  return chip;
 }));
}

document.querySelectorAll('#studio-sfx-rack button[data-sfx-play]').forEach(btn => {
 btn.onclick = () => {
  const audio = new Audio('/api/workspace/studio/sfx/' + encodeURIComponent(btn.dataset.sfxPlay));
  audio.play().catch(() => {});
 };
});
document.querySelectorAll('#studio-sfx-rack button[data-sfx-drop]').forEach(btn => {
 btn.onclick = () => {
  const kind = btn.dataset.sfxDrop;
  const at_s = curPlayheadMs() / 1000;
  state.sfx_cues.push({ at_s, kind });
  state.sfx_cues.sort((a, b) => a.at_s - b.at_s);
  renderSfxList();
  syncStudioSummary();
  app().note('Dropped "' + kind + '" SFX at ' + app().time(at_s).slice(0, 5) + '.');
 };
});

// #3, #9, #13: Montage Builder (Beat-Sync) & "GTA V vs GTA VI" Comparison Studio
function renderMontageList() {
 const list = $('studio-montage-list'), go = $('studio-montage-go'), clr = $('studio-montage-clear');
 if (!list) return;
 if (go) go.disabled = state.montage_clips.length < 2;
 if (clr) clr.hidden = state.montage_clips.length === 0;
 list.replaceChildren(...state.montage_clips.map((c, idx) => {
  const li = app().node('li');
  const rm = app().node('button', '✕', 'toast-close'); rm.type = 'button'; rm.style.float = 'right';
  rm.onclick = () => { state.montage_clips.splice(idx, 1); renderMontageList(); };
  li.append(rm, '#' + (idx + 1) + ' · ' + c.name.slice(0, 34));
  return li;
 }));
}

function populateCompareDropdowns() {
 const selA = $('studio-cmp-a'), selB = $('studio-cmp-b');
 if (!selA || !selB) return;
 const clips = (app()?.getItems?.() || []).filter(r => r.from_sources);
 const prevA = selA.value, prevB = selB.value;
 const makeOpts = () => clips.map(r => {
  const txt = r.name.length > 30 ? r.name.slice(0, 29) + '…' : r.name;
  const o = app().node('option', txt);
  o.value = r.clip_id;
  o.title = r.name;
  return o;
 });
 selA.replaceChildren(...makeOpts());
 selB.replaceChildren(...makeOpts());
 if (prevA && clips.some(c => c.clip_id === prevA)) selA.value = prevA;
 if (prevB && clips.some(c => c.clip_id === prevB)) selB.value = prevB;
 else if (clips.length >= 2) selB.value = clips[1].clip_id;
}

if ($('studio-montage-add')) $('studio-montage-add').onclick = () => {
 const row = app().getSelected();
 if (!row?.from_sources) { app().note('Select a Film Roll clip first.'); return; }
 if (state.montage_clips.length >= 8) { app().note('Maximum 8 clips per montage.'); return; }
 state.montage_clips.push({ clip_id: row.clip_id, name: row.name });
 renderMontageList();
 app().note('Added "' + row.name + '" to Montage (' + state.montage_clips.length + ' clips).');
};
if ($('studio-montage-clear')) $('studio-montage-clear').onclick = () => { state.montage_clips = []; renderMontageList(); };

if ($('studio-detect-beats')) $('studio-detect-beats').onclick = async () => {
 const btn = $('studio-detect-beats'), out = $('studio-beats-status');
 const row = app().getSelected();
 const music = window.snapmaticProMusicPath || null;
 if (!music && !row?.from_sources) { app().note('Select a clip or attach a background music track first.'); return; }
 btn.disabled = true;
 if (out) out.textContent = 'Detecting BPM & audio beat transients…';
 try {
  const r = await app().get('/api/workspace/studio/detect-beats', {
   method: 'POST', headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({ audio_path: music, clip_id: music ? null : row?.clip_id })
  });
  const cnt = Array.isArray(r.beats_ms) ? r.beats_ms.length : 0;
  if (out) out.textContent = '🥁 Detected ~' + r.bpm_est + ' BPM (' + cnt + ' beat markers ready for Beat-Sync cuts).';
  app().note('Detected ~' + r.bpm_est + ' BPM (' + cnt + ' beats)!');
 } catch (e) {
  if (out) out.textContent = 'Beat detect failed: ' + e.message;
 } finally {
  btn.disabled = false;
 }
};

if ($('studio-montage-go')) $('studio-montage-go').onclick = async () => {
 const btn = $('studio-montage-go'), out = $('studio-builder-status');
 if (state.montage_clips.length < 2 || btn.disabled) return;
 btn.disabled = true;
 if (out) out.textContent = 'Rendering Multi-Clip Montage with FFmpeg…';
 try {
  const r = await app().get('/api/workspace/studio/montage', {
   method: 'POST', headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({
    clip_ids: state.montage_clips.map(c => c.clip_id),
    labels: state.montage_clips.map((_, i) => '#' + (i + 1)),
    title: ($('studio-montage-title')?.value || '').trim() || 'Top ' + state.montage_clips.length + ' GTA 6 Moments',
    clip_sec: Number($('studio-montage-sec')?.value) || 5,
    beat_sync: Boolean($('studio-montage-beatsync')?.checked),
    music_path: window.snapmaticProMusicPath || null,
    music_volume_db: Number($('edit-music-vol')?.value) || -14,
    color_grade: state.color_grade,
    progress_bar: Boolean($('studio-progress-bar')?.checked),
    intro_sting_text: ($('studio-intro-sting')?.value || '').trim() || null,
    outro_card_text: ($('studio-outro-card')?.value || '').trim() || null,
    credit_handle: ($('studio-credit-handle')?.value || '').trim() || null,
    weazel_headline: ($('studio-weazel-head')?.value || '').trim() || null,
    quality: $('edit-quality')?.value || 'balanced',
   })
  });
  if (out) out.textContent = '✓ Created Montage ' + r.file_name + ' (' + ((r.bytes || 0) / 1048576).toFixed(1) + ' MiB) & added to Film Roll!';
  await app().reload();
  app().loadExportHistory();
  const hit = app().getItems().find(x => x.clip_id === r.clip_id);
  if (hit) app().select(hit);
  app().note('Rendered Montage "' + r.file_name + '" and opened it in Viewfinder Studio!');
 } catch (e) {
  if (out) out.textContent = 'Montage failed: ' + e.message;
  app().note('Montage failed: ' + e.message);
 } finally {
  btn.disabled = state.montage_clips.length < 2;
 }
};

if ($('studio-cmp-go')) $('studio-cmp-go').onclick = async () => {
 const btn = $('studio-cmp-go'), out = $('studio-builder-status');
 const a = $('studio-cmp-a')?.value, b = $('studio-cmp-b')?.value;
 if (!a || !b) { app().note('Select two Film Roll clips to compare.'); return; }
 btn.disabled = true;
 if (out) out.textContent = 'Rendering Side-by-Side / Stacked Comparison with FFmpeg…';
 try {
  const r = await app().get('/api/workspace/studio/compare', {
   method: 'POST', headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({
    clip_a_id: a,
    clip_b_id: b,
    layout: $('studio-cmp-layout')?.value || 'vertical',
    label_a: ($('studio-cmp-label-a')?.value || '').trim() || 'GTA V',
    label_b: ($('studio-cmp-label-b')?.value || '').trim() || 'GTA VI',
    title_banner: ($('studio-cmp-title')?.value || '').trim() || null,
    max_seconds: Number($('studio-cmp-sec')?.value) || 18,
    color_grade: state.color_grade,
    quality: $('edit-quality')?.value || 'balanced',
   })
  });
  if (out) out.textContent = '✓ Created Comparison ' + r.file_name + ' (' + ((r.bytes || 0) / 1048576).toFixed(1) + ' MiB) & added to Film Roll!';
  await app().reload();
  app().loadExportHistory();
  const hit = app().getItems().find(x => x.clip_id === r.clip_id);
  if (hit) app().select(hit);
  app().note('Rendered Comparison "' + r.file_name + '" and opened it in Viewfinder Studio!');
 } catch (e) {
  if (out) out.textContent = 'Comparison failed: ' + e.message;
  app().note('Comparison failed: ' + e.message);
 } finally {
  btn.disabled = false;
 }
};

// Export Now & Reset All Studio FX
if ($('studio-pick-first')) $('studio-pick-first').onclick = () => {
 const list = app()?.getVisibleItems?.() || [];
 const first = list.find(r => r.from_sources) || list[0];
 if (first) {
  app().select(first);
  app().note('Selected "' + first.name + '" — Studio FX unlocked!');
 } else {
  app().note('No playable clips in Film Roll yet. Drop an MP4 or sync Sources first.');
 }
};

if ($('studio-export-now')) $('studio-export-now').onclick = () => {
 const row = app()?.getSelected?.();
 if (!row?.from_sources) {
  app().note('Select a Film Roll clip first before exporting.');
  return;
 }
 const tBtn = $('edit-transcode');
 if (!tBtn || tBtn.disabled) {
  app().note('Waiting for video duration to finish loading — try again in a second.');
  return;
 }
 if ($('studio-active-summary')) $('studio-active-summary').textContent = '⏳ Rendering MP4 with Studio FX…';
 tBtn.click();
};

function syncClipSelectionUI(row) {
 const ok = Boolean(row?.from_sources);
 if ($('studio-export-now')) $('studio-export-now').disabled = !ok;
 if ($('studio-no-clip-banner')) $('studio-no-clip-banner').hidden = ok;
 populateCompareDropdowns();
 syncStudioSummary();
}

window.addEventListener('snapmatic:exported', () => {
 if ($('studio-active-summary')) $('studio-active-summary').textContent = '✓ ' + ($('edit-state')?.textContent || 'Exported MP4 with Studio FX!');
});

if ($('studio-reset-all')) $('studio-reset-all').onclick = () => {
 state.voiceover_path = null;
 state.reaction_cam_path = null;
 state.freeze_at_ms = null;
 state.zoom_punch_at_ms = null;
 state.replay_start_ms = null;
 state.replay_end_ms = null;
 state.countdown_at_ms = null;
 state.color_grade = 'none';
 state.commentary_cards = [];
 state.sfx_cues = [];
 for (const id of ['studio-weazel-head', 'studio-weazel-tick', 'studio-credit-handle', 'studio-poll-prompt', 'studio-intro-sting', 'studio-outro-card', 'studio-fz-callout', 'studio-rp-label']) {
  if ($(id)) $(id).value = '';
 }
 if ($('studio-progress-bar')) $('studio-progress-bar').checked = false;
 if ($('studio-voice-fx')) $('studio-voice-fx').value = 'none';
 for (const id of ['studio-vo-clear', 'studio-cam-clear', 'studio-fz-clear', 'studio-zp-clear', 'studio-rp-clear', 'studio-cd-clear', 'studio-vo-audio']) {
  if ($(id)) $(id).hidden = true;
 }
 document.querySelectorAll('#studio-grade-pills button[data-grade]').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.grade === 'none')));
 renderCommentaryCards();
 renderSfxList();
 syncStudioSummary();
 app().note('Reset all Original Studio FX.');
};

app()?.getVideo()?.addEventListener('timeupdate', () => {
 const bar = $('vf-progress-preview'), v = app().getVideo();
 if (bar && !bar.hidden && v && Number.isFinite(v.duration) && v.duration > 0) {
  bar.style.width = Math.max(0, Math.min(100, (v.currentTime / v.duration) * 100)) + '%';
 }
});

window.addEventListener('snapmatic:clip-selected', e => {
 syncClipSelectionUI(e.detail?.row);
});

// Expose export payload merger for workspace.js
window.snapmaticStudio = {
 getExportExtras: () => ({
  voiceover_path: state.voiceover_path,
  voiceover_duck_db: Number($('studio-vo-duck')?.value) || -14,
  voiceover_gain_db: Number($('studio-vo-gain')?.value) || 2,
  voice_fx: ($('studio-voice-fx')?.value || 'none') !== 'none' ? $('studio-voice-fx').value : null,
  reaction_cam_path: state.reaction_cam_path,
  reaction_cam_layout: $('studio-cam-layout')?.value || 'corner_tr',
  freeze_at_ms: state.freeze_at_ms,
  freeze_dur_s: 2.0,
  freeze_callout: ($('studio-fz-callout')?.value || '').trim() || null,
  commentary_cards: state.commentary_cards,
  credit_handle: ($('studio-credit-handle')?.value || '').trim() || null,
  sfx_cues: state.sfx_cues,
  intro_sting_text: ($('studio-intro-sting')?.value || '').trim() || null,
  outro_card_text: ($('studio-outro-card')?.value || '').trim() || null,
  zoom_punch_at_ms: state.zoom_punch_at_ms,
  zoom_punch_dur_s: 2.2,
  zoom_punch_scale: Number($('studio-zp-scale')?.value) || 1.38,
  replay_start_ms: state.replay_start_ms,
  replay_end_ms: state.replay_end_ms,
  replay_label: ($('studio-rp-label')?.value || '').trim() || null,
  poll_prompt: ($('studio-poll-prompt')?.value || '').trim() || null,
  progress_bar: Boolean($('studio-progress-bar')?.checked),
  countdown_at_ms: state.countdown_at_ms,
  color_grade: state.color_grade !== 'none' ? state.color_grade : null,
  weazel_headline: ($('studio-weazel-head')?.value || '').trim() || null,
  weazel_ticker: ($('studio-weazel-tick')?.value || '').trim() || null,
 }),
 populateCompareDropdowns,
};

setTimeout(() => { populateCompareDropdowns(); syncClipSelectionUI(app()?.getSelected?.()); }, 1200);
renderMontageList();
syncClipSelectionUI(app()?.getSelected?.());
})();
