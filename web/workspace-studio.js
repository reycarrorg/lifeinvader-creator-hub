'use strict';
(() => {
const $ = id => document.getElementById(id);
const app = () => window.snapmaticApp;

const TPL_KEY = 'snapmatic-autocap-templates-v1';
const GRADE_CSS = {
 none: '',
 crisp_rec709: 'contrast(1.06) saturate(1.10) brightness(1.01)',
 warm_cinema: 'contrast(1.08) saturate(1.12) sepia(0.10) brightness(1.01)',
 vice_night: 'contrast(1.10) saturate(1.20) hue-rotate(-5deg)',
 hdr_gaming: 'contrast(1.12) saturate(1.18)',
};

const state = {
 whisperReady: false,
 words: [], // [{ text, start_ms, end_ms, emoji, color }]
 selectedWordIdx: -1,
 preset: 'hormozi',
 activeColor: '#FFE600',
 colorGrade: 'none',
 lutPath: '',
 voiceoverPath: null,
 voiceoverBlobUrl: null,
};

let whisperPollTimer = null;
let voRecorder = null, voChunks = [], voStream = null, voMeterRaf = null, voAudioCtx = null;

function getCaptionStyle() {
 return {
  preset: state.preset,
  words_per_line: Number($('ac-wpl')?.value) || 3,
  font_size: Number($('ac-size')?.value) || 68,
  pos_y_pct: Number($('ac-posy')?.value) || 74,
  active_color: state.activeColor,
  uppercase: Boolean($('ac-uppercase')?.checked),
  pop_animation: Boolean($('ac-pop')?.checked),
  show_emojis: Boolean($('ac-emojis')?.checked),
 };
}

function buildPhrases(words, wpl) {
 const phrases = [];
 let cur = [];
 for (let i = 0; i < words.length; i++) {
  const w = words[i];
  if (!w?.text?.trim()) continue;
  const prev = cur.length ? cur[cur.length - 1].word : null;
  const pauseGap = prev && (w.start_ms - prev.end_ms) > 480;
  const endSent = prev && /[.!?]$/.test(prev.text.trim());
  if (cur.length && (cur.length >= wpl || pauseGap || endSent)) {
   phrases.push(cur);
   cur = [];
  }
  cur.push({ idx: i, word: w });
 }
 if (cur.length) phrases.push(cur);
 return phrases;
}

// ==========================================================================
// LIVE WYSIWYG VIEWFINDER AUTOCAPTION PREVIEW
// ==========================================================================
function renderLiveCaptions() {
 const layer = $('vf-autocap-layer');
 if (!layer) return;
 if (!state.words.length) {
  layer.hidden = true;
  layer.replaceChildren();
  return;
 }
 const cfg = getCaptionStyle();
 const v = app()?.getVideo();
 const es = app()?.getEditState();
 const trimInMs = es?.in_ms || 0;
 const curMs = v && Number.isFinite(v.currentTime) ? Math.max(0, Math.round(v.currentTime * 1000) - trimInMs) : 0;

 const phrases = buildPhrases(state.words, cfg.words_per_line);
 // Find active phrase (or preview first phrase when paused at 0s so styling changes are immediately visible!)
 let activePhrase = phrases.find(ph => curMs >= ph[0].word.start_ms && curMs <= ph[ph.length - 1].word.end_ms + 250);
 let previewMode = false;
 if (!activePhrase && (!v || v.paused)) {
  // Show closest phrase or first phrase for live WYSIWYG styling
  activePhrase = phrases.find(ph => ph[ph.length - 1].word.end_ms >= curMs) || phrases[0];
  previewMode = true;
 }
 if (!activePhrase) {
  layer.hidden = true;
  return;
 }

 layer.hidden = false;
 layer.dataset.preset = cfg.preset;
 layer.style.top = cfg.pos_y_pct + '%';
 // Scale 1080p font size down proportionally to the viewfinder width (~275px)
 const vfW = $('viewfinder')?.clientWidth || 275;
 const scaledPx = Math.max(11, Math.round((cfg.font_size / 1080) * vfW * 1.12));

 let activeWordIdx = activePhrase[0].idx;
 if (!previewMode) {
  for (const item of activePhrase) {
   if (curMs >= item.word.start_ms) activeWordIdx = item.idx;
  }
 }

 layer.replaceChildren(...activePhrase.map(item => {
  const isAct = item.idx === activeWordIdx;
  let txt = item.word.text.trim();
  if (cfg.uppercase) txt = txt.toUpperCase();
  if (isAct && cfg.show_emojis && item.word.emoji) txt += ' ' + item.word.emoji;
  const sp = app().node('span', txt, 'vf-autocap-word' + (isAct && cfg.pop_animation ? ' active' : ''));
  sp.style.fontSize = scaledPx + 'px';
  const col = item.word.color || cfg.active_color;
  if (isAct) {
   if (cfg.preset === 'submagic_pill') {
    sp.style.backgroundColor = col;
    sp.style.color = '#090812';
   } else {
    sp.style.color = col;
   }
  }
  return sp;
 }));

 // Highlight active speaking chip in word editor
 document.querySelectorAll('#ac-word-chips .ac-word-chip.speaking').forEach(el => el.classList.remove('speaking'));
 const speakingChip = document.querySelector('#ac-word-chips .ac-word-chip[data-widx="' + activeWordIdx + '"]');
 if (speakingChip) speakingChip.classList.add('speaking');
}

function updateColorPreview() {
 const v = app()?.getVideo();
 if (!v) return;
 if (state.colorGrade !== 'none') v.style.filter = GRADE_CSS[state.colorGrade] || '';
 else if (!$('edit-vivid')?.checked) v.style.filter = '';
}

function activeFxList() {
 const list = [];
 if (state.words.length) list.push('AutoCaptions (' + state.words.length + 'w)');
 if ($('ac-jumpcut-on')?.checked) list.push('Jump-Cut');
 if ($('ac-voice-polish')?.checked) list.push('Voice Polish');
 if (state.voiceoverPath) list.push('Voiceover');
 if (state.lutPath) list.push('3D LUT');
 else if (state.colorGrade !== 'none') list.push('Grade');
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
  sum.textContent = list.length ? 'Active Pro Studio (' + list.length + '): ' + list.join(' · ') : 'No Pro Studio effects active yet.';
 }
 renderLiveCaptions();
 updateColorPreview();
}

// ==========================================================================
// TOOL 1: WHISPER AI STATUS, 1-CLICK INSTALLER & WORD-BY-WORD TRANSCRIBER
// ==========================================================================
async function refreshWhisperStatus() {
 try {
  const s = await app().get('/api/workspace/studio/whisper/status');
  state.whisperReady = Boolean(s.ready);
  const badge = $('ac-whisper-badge'), instBtn = $('ac-whisper-install');
  const pWrap = $('ac-install-progress-wrap'), pBar = $('ac-install-progress'), pTxt = $('ac-install-progress-text');

  if (s.ready) {
   if (badge) { badge.textContent = 'WHISPER AI READY (' + (s.model_name || 'base.en') + ')'; badge.className = 'flag ready'; }
   if (instBtn) instBtn.hidden = true;
   if (pWrap) pWrap.hidden = true;
   if (whisperPollTimer) { clearInterval(whisperPollTimer); whisperPollTimer = null; }
  } else if (s.installing) {
   if (badge) { badge.textContent = 'INSTALLING WHISPER AI…'; badge.className = 'flag warn'; }
   if (instBtn) instBtn.hidden = true;
   if (pWrap) pWrap.hidden = false;
   if (pBar) pBar.value = Number(s.progress_pct) || 5;
   if (pTxt) pTxt.textContent = s.stage || 'Downloading…';
   if (!whisperPollTimer) whisperPollTimer = setInterval(refreshWhisperStatus, 900);
  } else {
   if (badge) { badge.textContent = s.error ? 'WHISPER INSTALL ERROR' : 'WHISPER AI NOT INSTALLED'; badge.className = 'flag warn'; }
   if (instBtn) { instBtn.hidden = false; instBtn.disabled = false; }
   if (pWrap) pWrap.hidden = true;
   if (s.error && $('ac-transcribe-status')) $('ac-transcribe-status').textContent = 'Install error: ' + s.error;
   if (whisperPollTimer) { clearInterval(whisperPollTimer); whisperPollTimer = null; }
  }
  syncClipSelectionUI(app()?.getSelected?.());
 } catch {}
}

if ($('ac-whisper-install')) $('ac-whisper-install').onclick = async () => {
 const btn = $('ac-whisper-install');
 btn.disabled = true;
 try {
  await app().get('/api/workspace/studio/whisper/install', { method: 'POST' });
  app().note('Downloading Local Whisper AI (whisper-cli + base.en model)…');
  refreshWhisperStatus();
 } catch (e) {
  app().note('Whisper install failed: ' + e.message);
  btn.disabled = false;
 }
};

function renderWordEditor() {
 const wrap = $('ac-editor-wrap'), chips = $('ac-word-chips'), cnt = $('ac-word-count'), clr = $('ac-clear-words');
 if (!wrap || !chips) return;
 if (!state.words.length) {
  wrap.hidden = true;
  if (clr) clr.hidden = true;
  if ($('ac-word-edit-bar')) $('ac-word-edit-bar').hidden = true;
  return;
 }
 wrap.hidden = false;
 if (clr) clr.hidden = false;
 if (cnt) cnt.textContent = state.words.length + ' spoken word' + (state.words.length === 1 ? '' : 's');
 chips.replaceChildren(...state.words.map((w, idx) => {
  const b = app().node('button', w.text + (w.emoji ? ' ' + w.emoji : ''), 'ac-word-chip' + (idx === state.selectedWordIdx ? ' selected' : ''));
  b.type = 'button';
  b.dataset.widx = String(idx);
  b.title = app().time(w.start_ms / 1000).slice(0, 8) + ' — click to seek & edit';
  b.onclick = () => {
   state.selectedWordIdx = idx;
   const es = app()?.getEditState();
   const v = app()?.getVideo();
   if (v && Number.isFinite(v.duration)) {
    v.currentTime = Math.min(v.duration, ((es?.in_ms || 0) + w.start_ms) / 1000);
   }
   if ($('ac-word-edit-bar')) $('ac-word-edit-bar').hidden = false;
   if ($('ac-word-input')) { $('ac-word-input').value = w.text; $('ac-word-input').focus(); }
   if ($('ac-word-emoji')) $('ac-word-emoji').value = w.emoji || '';
   renderWordEditor();
   renderLiveCaptions();
  };
  return b;
 }));
}

if ($('ac-word-save')) $('ac-word-save').onclick = () => {
 const w = state.words[state.selectedWordIdx];
 if (!w) return;
 const nextTxt = ($('ac-word-input')?.value || '').trim();
 if (!nextTxt) { state.words.splice(state.selectedWordIdx, 1); state.selectedWordIdx = -1; }
 else { w.text = nextTxt; w.emoji = ($('ac-word-emoji')?.value || '').trim() || null; }
 renderWordEditor();
 syncStudioSummary();
};
if ($('ac-word-input')) $('ac-word-input').addEventListener('keydown', e => { if (e.key === 'Enter') { e.preventDefault(); $('ac-word-save')?.click(); } });
if ($('ac-word-del')) $('ac-word-del').onclick = () => {
 if (state.selectedWordIdx < 0 || !state.words[state.selectedWordIdx]) return;
 state.words.splice(state.selectedWordIdx, 1);
 state.selectedWordIdx = -1;
 if ($('ac-word-edit-bar')) $('ac-word-edit-bar').hidden = true;
 renderWordEditor();
 syncStudioSummary();
};
if ($('ac-clear-words')) $('ac-clear-words').onclick = () => {
 state.words = [];
 state.selectedWordIdx = -1;
 renderWordEditor();
 syncStudioSummary();
 if ($('ac-transcribe-status')) $('ac-transcribe-status').textContent = 'Cleared word captions.';
};

if ($('ac-transcribe-go')) $('ac-transcribe-go').onclick = async () => {
 const row = app()?.getSelected?.(), btn = $('ac-transcribe-go'), out = $('ac-transcribe-status');
 if (!row?.from_sources || btn.disabled) return;
 if (!state.whisperReady) {
  app().note('Click "Install Local Whisper AI (142 MB)" first — takes ~20 seconds once.');
  return;
 }
 btn.disabled = true;
 if (out) out.textContent = 'Transcribing exact word timestamps with Local Whisper AI…';
 try {
  const es = app().getEditState();
  const r = await app().get('/api/workspace/studio/transcribe/' + encodeURIComponent(row.clip_id), {
   method: 'POST', headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({ in_ms: es?.in_ms || 0, out_ms: es?.out_ms || null, language: 'en' })
  });
  state.words = Array.isArray(r.words) ? r.words : [];
  state.selectedWordIdx = -1;
  renderWordEditor();
  syncStudioSummary();
  if (state.words.length) {
   if (out) out.textContent = '✓ Transcribed ' + state.words.length + ' spoken words with exact karaoke timing! Press Play to preview live.';
   app().note('✨ Auto-generated ' + state.words.length + ' animated karaoke caption words!');
  } else {
   if (out) out.textContent = 'No speech detected in this clip segment.';
   app().note('No speech detected in this clip.');
  }
 } catch (e) {
  if (out) out.textContent = 'Transcription failed: ' + e.message;
  app().note('Transcription failed: ' + e.message);
 } finally {
  syncClipSelectionUI(app()?.getSelected?.());
 }
};

// Style preset cards, colors, sliders & checkboxes
document.querySelectorAll('#ac-style-grid button[data-ac-preset]').forEach(btn => {
 btn.onclick = () => {
  state.preset = btn.dataset.acPreset || 'hormozi';
  document.querySelectorAll('#ac-style-grid button[data-ac-preset]').forEach(b => b.setAttribute('aria-pressed', String(b === btn)));
  syncStudioSummary();
 };
});
document.querySelectorAll('#ac-color-row button[data-ac-color]').forEach(btn => {
 btn.onclick = () => {
  state.activeColor = btn.dataset.acColor || '#FFE600';
  document.querySelectorAll('#ac-color-row button[data-ac-color]').forEach(b => b.setAttribute('aria-pressed', String(b === btn)));
  syncStudioSummary();
 };
});
for (const id of ['ac-wpl', 'ac-size', 'ac-posy', 'ac-uppercase', 'ac-pop', 'ac-emojis', 'ac-jumpcut-on', 'ac-voice-polish']) {
 const el = $(id);
 if (!el) continue;
 const onChg = () => {
  if ($('ac-size-val') && $('ac-size')) $('ac-size-val').textContent = $('ac-size').value + 'px';
  if ($('ac-posy-val') && $('ac-posy')) $('ac-posy-val').textContent = $('ac-posy').value + '%';
  syncStudioSummary();
 };
 el.addEventListener('input', onChg);
 el.addEventListener('change', onChg);
}

// ==========================================================================
// TOOL 3: OPUSCLIP-STYLE MULTI-HIGHLIGHT FINDER
// ==========================================================================
if ($('ac-highlights-go')) $('ac-highlights-go').onclick = async () => {
 const row = app()?.getSelected?.(), btn = $('ac-highlights-go'), out = $('ac-highlights-status'), box = $('ac-highlights-list');
 if (!row?.from_sources || btn.disabled) return;
 btn.disabled = true;
 if (out) out.textContent = 'Scanning audio dynamics & speech activity for top viral moments…';
 try {
  const r = await app().get('/api/workspace/studio/highlights/' + encodeURIComponent(row.clip_id), { method: 'POST' });
  const list = Array.isArray(r.highlights) ? r.highlights : [];
  if (out) out.textContent = list.length ? 'Found ' + list.length + ' ranked viral highlight moment' + (list.length === 1 ? '' : 's') + ':' : 'No highlights found.';
  if (box) {
   box.replaceChildren(...list.map(h => {
    const card = app().node('div', undefined, 'ac-hl-card');
    const meta = app().node('div', undefined, 'ac-hl-meta');
    meta.append(
     app().node('strong', h.label + ' · ' + h.score + '/99'),
     app().node('small', app().time(h.start_ms / 1000).slice(0, 5) + ' → ' + app().time(h.end_ms / 1000).slice(0, 5) + ' (' + h.duration_s + 's)')
    );
    const loadBtn = app().node('button', 'Load & Trim', 'mini-btn');
    loadBtn.type = 'button';
    loadBtn.onclick = () => {
     const es = app().getEditState();
     if (es) {
      app().setTrim('in', Math.min(es.duration_ms - 500, h.start_ms));
      app().setTrim('out', Math.min(es.duration_ms, h.end_ms));
      app().workflowDirty();
     }
     const v = app().getVideo();
     if (v && Number.isFinite(v.duration)) v.currentTime = Math.min(v.duration, h.start_ms / 1000);
     app().note('Loaded ' + h.label + ' (' + app().time(h.start_ms / 1000).slice(0, 5) + ' → ' + app().time(h.end_ms / 1000).slice(0, 5) + ').');
    };
    card.append(meta, loadBtn);
    return card;
   }));
  }
 } catch (e) {
  if (out) out.textContent = 'Highlight scan failed: ' + e.message;
 } finally {
  syncClipSelectionUI(app()?.getSelected?.());
 }
};

// ==========================================================================
// TOOL 4: CLEAN MIC VOICEOVER RECORDER + AUTO-DUCKING
// ==========================================================================
async function uploadStudioMedia(blob, kind, fileName) {
 const fd = new FormData();
 fd.append('kind', kind);
 fd.append('file', blob, fileName);
 const r = await fetch('/api/workspace/studio/upload-media', { method: 'POST', credentials: 'same-origin', body: fd });
 if (!r.ok) throw Error((await r.text()).trim() || 'Upload failed.');
 return r.json();
}

function stopVoMeter() {
 if (voMeterRaf) { cancelAnimationFrame(voMeterRaf); voMeterRaf = null; }
 if (voAudioCtx) { voAudioCtx.close().catch(() => {}); voAudioCtx = null; }
 if ($('studio-vo-meter-wrap')) $('studio-vo-meter-wrap').hidden = true;
}

if ($('studio-vo-rec')) $('studio-vo-rec').onclick = async () => {
 if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === 'undefined') {
  app().note('Microphone recording is unavailable in this browser context.');
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
   if ($('studio-vo-status')) $('studio-vo-status').textContent = 'Saving voiceover locally…';
   try {
    const res = await uploadStudioMedia(blob, 'voiceover', 'voiceover.webm');
    state.voiceoverPath = res.path;
    if (state.voiceoverBlobUrl) URL.revokeObjectURL(state.voiceoverBlobUrl);
    state.voiceoverBlobUrl = URL.createObjectURL(blob);
    if ($('studio-vo-audio')) { $('studio-vo-audio').src = state.voiceoverBlobUrl; $('studio-vo-audio').hidden = false; }
    if ($('studio-vo-clear')) $('studio-vo-clear').hidden = false;
    if ($('studio-vo-status')) $('studio-vo-status').textContent = '✓ Voiceover ready (' + (res.bytes / 1024).toFixed(0) + ' KiB) — auto-ducks clip on export.';
    syncStudioSummary();
   } catch (err) {
    if ($('studio-vo-status')) $('studio-vo-status').textContent = 'Save failed: ' + err.message;
   }
  };
  try {
   voAudioCtx = new (window.AudioContext || window.webkitAudioContext)();
   const src = voAudioCtx.createMediaStreamSource(voStream);
   const an = voAudioCtx.createAnalyser(); an.fftSize = 256; src.connect(an);
   const buf = new Uint8Array(an.frequencyBinCount);
   if ($('studio-vo-meter-wrap')) $('studio-vo-meter-wrap').hidden = false;
   const tick = () => {
    an.getByteTimeDomainData(buf);
    let s = 0; for (let i = 0; i < buf.length; i++) { const d = (buf[i] - 128) / 128; s += d * d; }
    if ($('studio-vo-meter')) $('studio-vo-meter').style.width = Math.min(100, Math.round(Math.sqrt(s / buf.length) * 350)) + '%';
    voMeterRaf = requestAnimationFrame(tick);
   };
   tick();
  } catch {}
  voRecorder.start();
  $('studio-vo-rec').hidden = true;
  if ($('studio-vo-stop')) $('studio-vo-stop').hidden = false;
  if ($('studio-vo-status')) $('studio-vo-status').textContent = '● Recording voiceover…';
  const v = app().getVideo();
  if (v && v.readyState >= 2) { v.currentTime = (app().getEditState()?.in_ms || 0) / 1000; v.play().catch(() => {}); }
 } catch (e) { app().note('Mic error: ' + e.message); }
};
if ($('studio-vo-stop')) $('studio-vo-stop').onclick = () => {
 if (voRecorder && voRecorder.state !== 'inactive') voRecorder.stop();
 $('studio-vo-stop').hidden = true;
 if ($('studio-vo-rec')) $('studio-vo-rec').hidden = false;
 app().getVideo()?.pause();
};
if ($('studio-vo-clear')) $('studio-vo-clear').onclick = () => {
 state.voiceoverPath = null;
 if ($('studio-vo-audio')) { $('studio-vo-audio').hidden = true; $('studio-vo-audio').removeAttribute('src'); }
 $('studio-vo-clear').hidden = true;
 if ($('studio-vo-status')) $('studio-vo-status').textContent = 'Voiceover cleared.';
 syncStudioSummary();
};
if ($('studio-vo-duck')) $('studio-vo-duck').oninput = () => { if ($('studio-vo-duck-val')) $('studio-vo-duck-val').textContent = $('studio-vo-duck').value + ' dB'; };
if ($('studio-vo-gain')) $('studio-vo-gain').oninput = () => { const g = Number($('studio-vo-gain').value) || 0; if ($('studio-vo-gain-val')) $('studio-vo-gain-val').textContent = (g >= 0 ? '+' : '') + g + ' dB'; };

// ==========================================================================
// TOOL 5: PRO COLOR GRADES & .CUBE 3D LUTS
// ==========================================================================
document.querySelectorAll('#studio-grade-pills button[data-grade]').forEach(btn => {
 btn.onclick = () => {
  state.colorGrade = btn.dataset.grade || 'none';
  document.querySelectorAll('#studio-grade-pills button[data-grade]').forEach(b => b.setAttribute('aria-pressed', String(b === btn)));
  syncStudioSummary();
 };
});

async function loadLuts(selectPath) {
 const sel = $('ac-lut-select');
 if (!sel) return;
 try {
  const r = await app().get('/api/workspace/studio/luts');
  const luts = Array.isArray(r.luts) ? r.luts : [];
  const prev = selectPath ?? sel.value;
  const opts = [app().node('option', 'None (use preset above)')]; opts[0].value = '';
  for (const l of luts) { const o = app().node('option', l.name); o.value = l.path; opts.push(o); }
  sel.replaceChildren(...opts);
  if (prev && luts.some(l => l.path === prev)) sel.value = prev;
  state.lutPath = sel.value || '';
  syncStudioSummary();
 } catch {}
}
if ($('ac-lut-select')) $('ac-lut-select').onchange = () => { state.lutPath = $('ac-lut-select').value || ''; syncStudioSummary(); };
if ($('ac-lut-upload')) $('ac-lut-upload').onchange = async e => {
 const f = e.currentTarget.files?.[0]; e.currentTarget.value = '';
 if (!f) return;
 try {
  const res = await uploadStudioMedia(f, 'lut', f.name);
  if ($('ac-lut-status')) $('ac-lut-status').textContent = '✓ Loaded 3D LUT: ' + res.name;
  await loadLuts(res.path);
  app().note('Loaded .cube LUT: ' + res.name);
 } catch (err) {
  if ($('ac-lut-status')) $('ac-lut-status').textContent = 'LUT upload failed: ' + err.message;
 }
};

// ==========================================================================
// TOOL 6: REUSABLE 1-CLICK CREATOR STYLE TEMPLATES
// ==========================================================================
function readTemplates() { try { return JSON.parse(localStorage.getItem(TPL_KEY) || '[]') || []; } catch { return []; } }
function writeTemplates(list) { try { localStorage.setItem(TPL_KEY, JSON.stringify(list)); } catch {} }
function renderTemplates() {
 const sel = $('ac-tpl-select'), ap = $('ac-tpl-apply'), dl = $('ac-tpl-del');
 if (!sel) return;
 const list = readTemplates();
 if (!list.length) {
  const o = app().node('option', 'No saved templates'); o.value = ''; sel.replaceChildren(o);
  if (ap) ap.disabled = true; if (dl) dl.disabled = true;
  return;
 }
 sel.replaceChildren(...list.map((t, i) => { const o = app().node('option', t.name); o.value = String(i); return o; }));
 if (ap) ap.disabled = false; if (dl) dl.disabled = false;
}
if ($('ac-tpl-save')) $('ac-tpl-save').onclick = () => {
 const name = ($('ac-tpl-name')?.value || '').trim();
 if (!name) { app().note('Enter a template name first.'); return; }
 const list = readTemplates().filter(t => t.name.toLowerCase() !== name.toLowerCase());
 list.push({
  name,
  caption: getCaptionStyle(),
  jumpcut: Boolean($('ac-jumpcut-on')?.checked),
  voicePolish: Boolean($('ac-voice-polish')?.checked),
  colorGrade: state.colorGrade,
  lutPath: state.lutPath,
 });
 writeTemplates(list);
 if ($('ac-tpl-name')) $('ac-tpl-name').value = '';
 renderTemplates();
 app().note('Saved Style Template "' + name + '"!');
};
if ($('ac-tpl-apply')) $('ac-tpl-apply').onclick = () => {
 const tpl = readTemplates()[Number($('ac-tpl-select')?.value)];
 if (!tpl) return;
 const c = tpl.caption || {};
 if (c.preset) {
  state.preset = c.preset;
  document.querySelectorAll('#ac-style-grid button[data-ac-preset]').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.acPreset === c.preset)));
 }
 if (c.active_color) {
  state.activeColor = c.active_color;
  document.querySelectorAll('#ac-color-row button[data-ac-color]').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.acColor === c.active_color)));
 }
 if ($('ac-wpl') && c.words_per_line) $('ac-wpl').value = String(c.words_per_line);
 if ($('ac-size') && c.font_size) { $('ac-size').value = String(c.font_size); if ($('ac-size-val')) $('ac-size-val').textContent = c.font_size + 'px'; }
 if ($('ac-posy') && c.pos_y_pct) { $('ac-posy').value = String(c.pos_y_pct); if ($('ac-posy-val')) $('ac-posy-val').textContent = c.pos_y_pct + '%'; }
 if ($('ac-uppercase') && typeof c.uppercase === 'boolean') $('ac-uppercase').checked = c.uppercase;
 if ($('ac-pop') && typeof c.pop_animation === 'boolean') $('ac-pop').checked = c.pop_animation;
 if ($('ac-emojis') && typeof c.show_emojis === 'boolean') $('ac-emojis').checked = c.show_emojis;
 if ($('ac-jumpcut-on') && typeof tpl.jumpcut === 'boolean') $('ac-jumpcut-on').checked = tpl.jumpcut;
 if ($('ac-voice-polish') && typeof tpl.voicePolish === 'boolean') $('ac-voice-polish').checked = tpl.voicePolish;
 if (tpl.colorGrade) {
  state.colorGrade = tpl.colorGrade;
  document.querySelectorAll('#studio-grade-pills button[data-grade]').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.grade === tpl.colorGrade)));
 }
 if ($('ac-lut-select') && tpl.lutPath != null) { $('ac-lut-select').value = tpl.lutPath; state.lutPath = $('ac-lut-select').value || ''; }
 syncStudioSummary();
 app().note('Applied Style Template "' + tpl.name + '"!');
};
if ($('ac-tpl-del')) $('ac-tpl-del').onclick = () => {
 const idx = Number($('ac-tpl-select')?.value);
 const list = readTemplates();
 if (!Number.isInteger(idx) || !list[idx]) return;
 list.splice(idx, 1); writeTemplates(list); renderTemplates();
};

// ==========================================================================
// SELECTION, EXPORT & RESET WIRING
// ==========================================================================
if ($('studio-pick-first')) $('studio-pick-first').onclick = () => {
 const list = app()?.getVisibleItems?.() || [];
 const first = list.find(r => r.from_sources) || list[0];
 if (first) { app().select(first); app().note('Selected "' + first.name + '" — ready for AutoCaption!'); }
 else app().note('No playable clips in Film Roll yet.');
};

if ($('studio-export-now')) $('studio-export-now').onclick = () => {
 const row = app()?.getSelected?.();
 if (!row?.from_sources) { app().note('Select a Film Roll clip first.'); return; }
 const tBtn = $('edit-transcode');
 if (!tBtn || tBtn.disabled) { app().note('Waiting for video duration to load — try again in a second.'); return; }
 if ($('studio-active-summary')) $('studio-active-summary').textContent = '⏳ Rendering MP4 with Pro Studio…';
 tBtn.click();
};

if ($('studio-reset-all')) $('studio-reset-all').onclick = () => {
 state.words = [];
 state.selectedWordIdx = -1;
 state.colorGrade = 'none';
 state.lutPath = '';
 state.voiceoverPath = null;
 if ($('ac-jumpcut-on')) $('ac-jumpcut-on').checked = false;
 if ($('ac-voice-polish')) $('ac-voice-polish').checked = false;
 if ($('ac-lut-select')) $('ac-lut-select').value = '';
 if ($('studio-vo-audio')) { $('studio-vo-audio').hidden = true; $('studio-vo-audio').removeAttribute('src'); }
 if ($('studio-vo-clear')) $('studio-vo-clear').hidden = true;
 if ($('ac-highlights-list')) $('ac-highlights-list').replaceChildren();
 document.querySelectorAll('#studio-grade-pills button[data-grade]').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.grade === 'none')));
 renderWordEditor();
 syncStudioSummary();
 app().note('Reset Pro Studio settings.');
};

function syncClipSelectionUI(row) {
 const ok = Boolean(row?.from_sources);
 if ($('studio-export-now')) $('studio-export-now').disabled = !ok;
 if ($('studio-no-clip-banner')) $('studio-no-clip-banner').hidden = ok;
 if ($('ac-transcribe-go')) $('ac-transcribe-go').disabled = !ok;
 if ($('ac-highlights-go')) $('ac-highlights-go').disabled = !ok;
 syncStudioSummary();
}

window.addEventListener('snapmatic:clip-selected', e => {
 state.words = [];
 state.selectedWordIdx = -1;
 if ($('ac-highlights-list')) $('ac-highlights-list').replaceChildren();
 if ($('ac-highlights-status')) $('ac-highlights-status').textContent = '';
 renderWordEditor();
 syncClipSelectionUI(e.detail?.row);
});

window.addEventListener('snapmatic:exported', () => {
 if ($('studio-active-summary')) $('studio-active-summary').textContent = '✓ ' + ($('edit-state')?.textContent || 'Exported MP4 with Pro Studio!');
});

for (const ev of ['timeupdate', 'seeked', 'play', 'pause']) {
 app()?.getVideo()?.addEventListener(ev, renderLiveCaptions);
}

window.snapmaticStudio = {
 getExportExtras: () => ({
  caption_words: state.words,
  caption_style: state.words.length ? getCaptionStyle() : null,
  remove_mid_silences: Boolean($('ac-jumpcut-on')?.checked),
  silence_threshold_db: Number($('ac-jumpcut-db')?.value) || -32,
  min_silence_s: Number($('ac-jumpcut-min')?.value) || 0.42,
  studio_voice_enhance: Boolean($('ac-voice-polish')?.checked),
  voiceover_path: state.voiceoverPath,
  voiceover_duck_db: Number($('studio-vo-duck')?.value) || -14,
  voiceover_gain_db: Number($('studio-vo-gain')?.value) || 2,
  color_grade: state.colorGrade !== 'none' ? state.colorGrade : null,
  lut_path: state.lutPath || null,
 }),
};

renderTemplates();
refreshWhisperStatus();
loadLuts();
syncClipSelectionUI(app()?.getSelected?.());
setTimeout(() => syncClipSelectionUI(app()?.getSelected?.()), 1000);
})();
