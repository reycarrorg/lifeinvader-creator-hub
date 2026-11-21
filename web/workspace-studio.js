'use strict';
(() => {
const $ = id => document.getElementById(id);
const app = () => window.snapmaticApp;

const TPL_KEY = 'snapmatic-autocap-templates-v1';
const CAP_CACHE_KEY = 'snapmatic-autocap-clips-v1';
function readCapCache() { try { return JSON.parse(localStorage.getItem(CAP_CACHE_KEY) || '{}') || {}; } catch { return {}; } }
function saveClipCaptions(clipId, words) {
 if (!clipId) return;
 try {
  const map = readCapCache();
  if (!Array.isArray(words) || !words.length) delete map[clipId];
  else map[clipId] = { words, ts: Date.now() };
  const keys = Object.keys(map).sort((a, b) => (map[b]?.ts || 0) - (map[a]?.ts || 0));
  for (const k of keys.slice(25)) delete map[k];
  localStorage.setItem(CAP_CACHE_KEY, JSON.stringify(map));
 } catch {}
}
function loadClipCaptions(clipId) {
 if (!clipId) return [];
 const entry = readCapCache()[clipId];
 return Array.isArray(entry?.words) ? entry.words : [];
}
const GRADE_CSS = {
 none: '',
 crisp_rec709: 'contrast(1.06) saturate(1.10) brightness(1.01)',
 warm_cinema: 'contrast(1.08) saturate(1.12) sepia(0.10) brightness(1.01)',
 vice_night: 'contrast(1.10) saturate(1.20) hue-rotate(-5deg)',
 hdr_gaming: 'contrast(1.12) saturate(1.18)',
};

const state = {
 whisperReady: false,
 hasSmall: false,
 cloudConfigured: false,
 cloudProvider: null,
 words: [], // [{ text, start_ms, end_ms, emoji, color }]
 preset: 'hormozi',
 activeColor: '#FFE600',
 colorGrade: 'none',
 lutPath: '',
 voiceoverPath: null,
 voiceoverBlobUrl: null,
};

let whisperPollTimer = null;
let captionRaf = null;
let lastRenderedWordKey = '';
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

function fmtShortTs(ms) {
 const s = Math.max(0, ms) / 1000;
 const m = Math.floor(s / 60);
 const rem = (s % 60).toFixed(1).padStart(4, '0');
 return String(m).padStart(2, '0') + ':' + rem;
}

// ==========================================================================
// 60FPS LIVE WYSIWYG VIEWFINDER AUTOCAPTION PREVIEW
// ==========================================================================
function renderLiveCaptions(force = false) {
 const layer = $('vf-autocap-layer');
 if (!layer) return;
 if (!state.words.length) {
  layer.hidden = true;
  layer.replaceChildren();
  lastRenderedWordKey = '';
  return;
 }
 const cfg = getCaptionStyle();
 const v = app()?.getVideo();
 const es = app()?.getEditState();
 const trimInMs = es?.in_ms || 0;
 const curMs = v && Number.isFinite(v.currentTime) ? Math.max(0, Math.round(v.currentTime * 1000) - trimInMs) : 0;

 const phrases = buildPhrases(state.words, cfg.words_per_line);
 let phraseIdx = phrases.findIndex(ph => curMs >= ph[0].word.start_ms && curMs <= ph[ph.length - 1].word.end_ms + 240);
 let previewMode = false;
 if (phraseIdx < 0 && (!v || v.paused)) {
  phraseIdx = phrases.findIndex(ph => ph[ph.length - 1].word.end_ms >= curMs);
  if (phraseIdx < 0) phraseIdx = 0;
  previewMode = true;
 }
 if (phraseIdx < 0 || !phrases[phraseIdx]) {
  layer.hidden = true;
  lastRenderedWordKey = 'hidden';
  return;
 }

 const activePhrase = phrases[phraseIdx];
 let activeWordIdx = activePhrase[0].idx;
 if (!previewMode) {
  for (const item of activePhrase) {
   if (curMs >= item.word.start_ms) activeWordIdx = item.idx;
  }
 }

 const renderKey = [phraseIdx, activeWordIdx, cfg.preset, cfg.active_color, cfg.font_size, cfg.pos_y_pct, cfg.uppercase, cfg.pop_animation, cfg.show_emojis, cfg.words_per_line].join('|');
 if (!force && renderKey === lastRenderedWordKey) return;
 lastRenderedWordKey = renderKey;

 layer.hidden = false;
 layer.dataset.preset = cfg.preset;
 layer.style.top = cfg.pos_y_pct + '%';
 const vfW = $('viewfinder')?.clientWidth || 275;
 const scaledPx = Math.max(11, Math.round((cfg.font_size / 1080) * vfW * 1.12));

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

 // Highlight active timed line in the Pro Transcript Editor
 const rows = document.querySelectorAll('#ac-lines-list .ac-line-row');
 rows.forEach((rowEl, idx) => rowEl.classList.toggle('speaking', idx === phraseIdx));
}

function startCaptionRaf() {
 if (captionRaf !== null) return;
 const tick = () => {
  captionRaf = null;
  const v = app()?.getVideo();
  if (!v || v.paused || v.ended || !state.words.length) return;
  renderLiveCaptions(false);
  captionRaf = requestAnimationFrame(tick);
 };
 captionRaf = requestAnimationFrame(tick);
}

function stopCaptionRaf() {
 if (captionRaf !== null) {
  cancelAnimationFrame(captionRaf);
  captionRaf = null;
 }
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
 if ($('hook-gen-from-transcript')) $('hook-gen-from-transcript').hidden = state.words.length === 0;
 renderLiveCaptions(true);
 updateColorPreview();
}

// ==========================================================================
// TOOL 1: PRO WHISPER AI STATUS, SMALL.EN UPGRADE, CLOUD TURBO & TRANSCRIBER
// ==========================================================================
async function refreshWhisperStatus() {
 try {
  const s = await app().get('/api/workspace/studio/whisper/status');
  state.whisperReady = Boolean(s.ready);
  state.hasSmall = Boolean(s.has_small);
  state.cloudConfigured = Boolean(s.cloud_configured);
  state.cloudProvider = s.cloud_provider || null;
  const badge = $('ac-whisper-badge'), instBtn = $('ac-whisper-install');
  const pWrap = $('ac-install-progress-wrap'), pBar = $('ac-install-progress'), pTxt = $('ac-install-progress-text');

  if ($('ac-cloud-clear')) $('ac-cloud-clear').hidden = !state.cloudConfigured;
  if ($('ac-cloud-status')) {
   $('ac-cloud-status').textContent = state.cloudConfigured
    ? '✓ Cloud Turbo active (' + String(state.cloudProvider || 'groq').toUpperCase() + ' Whisper Large-v3-Turbo · saved in Windows Vault).'
    : 'Not configured — runs 100% locally via Whisper AI + DTW.';
  }

  if (s.installing) {
   if (badge) { badge.textContent = 'INSTALLING PRO MODEL…'; badge.className = 'flag warn'; }
   if (instBtn) instBtn.hidden = true;
   if (pWrap) pWrap.hidden = false;
   if (pBar) pBar.value = Number(s.progress_pct) || 5;
   if (pTxt) pTxt.textContent = s.stage || 'Downloading…';
   if (!whisperPollTimer) whisperPollTimer = setInterval(refreshWhisperStatus, 900);
  } else if (state.cloudConfigured) {
   if (badge) { badge.textContent = '⚡ CLOUD TURBO (' + String(state.cloudProvider || 'GROQ').toUpperCase() + ')'; badge.className = 'flag ready'; }
   if (instBtn) instBtn.hidden = true;
   if (pWrap) pWrap.hidden = true;
   if (whisperPollTimer) { clearInterval(whisperPollTimer); whisperPollTimer = null; }
  } else if (s.ready && s.has_small) {
   if (badge) { badge.textContent = 'PRO WHISPER READY (' + (s.model_tier || 'small.en') + ' · DTW)'; badge.className = 'flag ready'; }
   if (instBtn) instBtn.hidden = true;
   if (pWrap) pWrap.hidden = true;
   if (whisperPollTimer) { clearInterval(whisperPollTimer); whisperPollTimer = null; }
  } else if (s.ready) {
   if (badge) { badge.textContent = 'WHISPER (' + (s.model_tier || 'base.en') + ' · DTW)'; badge.className = 'flag warn'; }
   if (instBtn) { instBtn.hidden = false; instBtn.disabled = false; instBtn.textContent = '⬇ Upgrade to Pro Model (488 MB)'; }
   if (pWrap) pWrap.hidden = true;
   if (whisperPollTimer) { clearInterval(whisperPollTimer); whisperPollTimer = null; }
  } else {
   if (badge) { badge.textContent = s.error ? 'WHISPER INSTALL ERROR' : 'WHISPER AI NOT INSTALLED'; badge.className = 'flag warn'; }
   if (instBtn) { instBtn.hidden = false; instBtn.disabled = false; instBtn.textContent = '⬇ Install Pro Whisper AI (488 MB)'; }
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
  await app().get('/api/workspace/studio/whisper/install', {
   method: 'POST',
   headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({ model: 'small.en' })
  });
  app().note('Downloading Pro Whisper AI Small.en model (488 MB) for high-accuracy word detection…');
  refreshWhisperStatus();
 } catch (e) {
  app().note('Whisper install failed: ' + e.message);
  btn.disabled = false;
 }
};

if ($('ac-cloud-save')) $('ac-cloud-save').onclick = async () => {
 const provider = $('ac-cloud-provider')?.value || 'groq';
 const api_key = ($('ac-cloud-key')?.value || '').trim();
 if (!api_key) { app().note('Paste your Groq or OpenAI API key first.'); return; }
 const btn = $('ac-cloud-save');
 btn.disabled = true;
 try {
  await app().get('/api/workspace/studio/cloud-key', {
   method: 'POST',
   headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({ provider, api_key })
  });
  if ($('ac-cloud-key')) $('ac-cloud-key').value = '';
  app().note('Saved ' + provider.toUpperCase() + ' Whisper Turbo key securely in Windows Credential Manager!');
  await refreshWhisperStatus();
 } catch (e) {
  app().note('Could not save key: ' + e.message);
 } finally {
  btn.disabled = false;
 }
};

if ($('ac-cloud-clear')) $('ac-cloud-clear').onclick = async () => {
 try {
  await app().get('/api/workspace/studio/cloud-key', {
   method: 'POST',
   headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({ provider: 'groq', api_key: '' })
  });
  app().note('Removed Cloud Whisper key (switched back to Local Whisper AI).');
  await refreshWhisperStatus();
 } catch (e) {
  app().note('Could not remove key: ' + e.message);
 }
};

// ==========================================================================
// PRO LINE-BY-LINE TIMED TRANSCRIPT EDITOR (AutoCaption.io / TikTokCaptions.io style)
// ==========================================================================
function renderTranscriptLines() {
 const wrap = $('ac-editor-wrap'), list = $('ac-lines-list'), cnt = $('ac-word-count'), clr = $('ac-clear-words'), rescan = $('ac-rescan-words');
 if (!wrap || !list) return;
 if (!state.words.length) {
  wrap.hidden = true;
  if (clr) clr.hidden = true;
  if (rescan) rescan.hidden = true;
  list.replaceChildren();
  return;
 }
 wrap.hidden = false;
 if (clr) clr.hidden = false;
 if (rescan) rescan.hidden = false;
 const cfg = getCaptionStyle();
 const phrases = buildPhrases(state.words, cfg.words_per_line);
 if (cnt) cnt.textContent = phrases.length + ' caption line' + (phrases.length === 1 ? '' : 's') + ' · ' + state.words.length + ' words';

 list.replaceChildren(...phrases.map(ph => {
  const firstIdx = ph[0].idx;
  const lastIdx = ph[ph.length - 1].idx;
  const startMs = ph[0].word.start_ms;
  const endMs = ph[ph.length - 1].word.end_ms;
  const lineText = ph.map(x => x.word.text).join(' ');
  const lineEmoji = ph.find(x => x.word.emoji)?.word.emoji || '';

  const row = app().node('div', undefined, 'ac-line-row');
  const tsBtn = app().node('button', fmtShortTs(startMs), 'ac-line-ts');
  tsBtn.type = 'button';
  tsBtn.title = 'Click to seek video to ' + fmtShortTs(startMs);
  tsBtn.onclick = () => {
   const es = app()?.getEditState();
   const v = app()?.getVideo();
   if (v && Number.isFinite(v.duration)) {
    v.currentTime = Math.min(v.duration, ((es?.in_ms || 0) + startMs) / 1000);
   }
   renderLiveCaptions(true);
  };

  const txtIn = document.createElement('input');
  txtIn.type = 'text';
  txtIn.className = 'ac-line-input';
  txtIn.value = lineText;
  txtIn.setAttribute('aria-label', 'Edit caption line at ' + fmtShortTs(startMs));

  const emIn = document.createElement('input');
  emIn.type = 'text';
  emIn.className = 'ac-line-emoji';
  emIn.maxLength = 6;
  emIn.placeholder = '✨';
  emIn.value = lineEmoji;
  emIn.title = 'Optional highlight emoji for this line';

  const commitLine = () => {
   const nextTokens = txtIn.value.trim().split(/\s+/).filter(Boolean);
   const nextEmoji = emIn.value.trim() || null;
   const spanCount = lastIdx - firstIdx + 1;
   if (!nextTokens.length) {
    state.words.splice(firstIdx, spanCount);
   } else if (nextTokens.length === spanCount) {
    nextTokens.forEach((tok, i) => {
     state.words[firstIdx + i].text = tok;
     state.words[firstIdx + i].emoji = i === nextTokens.length - 1 ? nextEmoji : null;
    });
   } else {
    const totalChars = nextTokens.reduce((acc, s) => acc + Math.max(2, s.length), 0);
    const totalMs = Math.max(200, endMs - startMs);
    let cursor = startMs;
    const rebuilt = nextTokens.map((tok, i) => {
     const share = Math.max(70, Math.round((Math.max(2, tok.length) / totalChars) * totalMs));
     const we = i === nextTokens.length - 1 ? Math.max(cursor + 80, endMs) : Math.min(endMs, cursor + share);
     const item = { text: tok, start_ms: cursor, end_ms: we, emoji: i === nextTokens.length - 1 ? nextEmoji : null, color: null };
     cursor = we;
     return item;
    });
    state.words.splice(firstIdx, spanCount, ...rebuilt);
   }
   saveClipCaptions(app()?.getSelected?.()?.clip_id, state.words);
   renderTranscriptLines();
   syncStudioSummary();
  };

  txtIn.addEventListener('change', commitLine);
  emIn.addEventListener('change', commitLine);
  txtIn.addEventListener('keydown', e => { if (e.key === 'Enter') { e.preventDefault(); txtIn.blur(); } });

  const delBtn = app().node('button', '✕', 'ac-line-del');
  delBtn.type = 'button';
  delBtn.title = 'Remove this caption line';
  delBtn.onclick = () => {
   state.words.splice(firstIdx, lastIdx - firstIdx + 1);
   saveClipCaptions(app()?.getSelected?.()?.clip_id, state.words);
   renderTranscriptLines();
   syncStudioSummary();
  };

  row.append(tsBtn, txtIn, emIn, delBtn);
  return row;
 }));
}

if ($('ac-clear-words')) $('ac-clear-words').onclick = () => {
 state.words = [];
 saveClipCaptions(app()?.getSelected?.()?.clip_id, []);
 stopCaptionRaf();
 renderTranscriptLines();
 syncStudioSummary();
 if ($('ac-transcribe-status')) $('ac-transcribe-status').textContent = 'Cleared word captions.';
};

async function runTranscribe(forceRefresh = false) {
 const row = app()?.getSelected?.(), btn = $('ac-transcribe-go'), rescan = $('ac-rescan-words'), out = $('ac-transcribe-status');
 if (!row?.from_sources || btn?.disabled) return;
 if (!state.whisperReady && !state.cloudConfigured) {
  app().note('Click "Install Pro Whisper AI (488 MB)" first, or add a free Groq key.');
  return;
 }
 if (btn) btn.disabled = true;
 if (rescan) rescan.disabled = true;
 const modeDesc = state.cloudConfigured ? 'Cloud Whisper Large-v3-Turbo' : ('Local Whisper ' + (state.hasSmall ? 'Small.en + DTW' : 'Base.en + DTW'));
 if (out) out.textContent = (forceRefresh ? 'Re-scanning vocals & aligning words with ' : 'Isolating vocals & aligning spoken words with ') + modeDesc + '…';
 try {
  const es = app().getEditState();
  const r = await app().get('/api/workspace/studio/transcribe/' + encodeURIComponent(row.clip_id), {
   method: 'POST', headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({ in_ms: es?.in_ms || 0, out_ms: es?.out_ms || null, language: 'en', engine: 'auto', force_refresh: Boolean(forceRefresh) })
  });
  state.words = Array.isArray(r.words) ? r.words : [];
  saveClipCaptions(row.clip_id, state.words);
  renderTranscriptLines();
  syncStudioSummary();
  if (state.words.length) {
   const eng = (r.engine || modeDesc) + (r.cached ? ' · instant cache' : '');
   if (out) out.textContent = '✓ Aligned ' + state.words.length + ' spoken words (' + eng + ')! Press Play to preview 60fps karaoke.';
   app().note('✨ Auto-generated ' + state.words.length + ' word-aligned captions (' + eng + ')!');
  } else {
   if (out) out.textContent = 'No speech detected in this clip segment.';
   app().note('No speech detected in this clip.');
  }
 } catch (e) {
  if (out) out.textContent = 'Transcription failed: ' + e.message;
  app().note('Transcription failed: ' + e.message);
 } finally {
  if (rescan) rescan.disabled = false;
  syncClipSelectionUI(app()?.getSelected?.());
 }
}

if ($('ac-transcribe-go')) $('ac-transcribe-go').onclick = () => runTranscribe(false);
if ($('ac-rescan-words')) $('ac-rescan-words').onclick = () => runTranscribe(true);

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
  if (id === 'ac-wpl' && state.words.length) renderTranscriptLines();
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
 stopCaptionRaf();
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
 renderTranscriptLines();
 syncStudioSummary();
 app().note('Reset Pro Studio settings.');
};

// ==========================================================================
// PRO VIRAL HOOK & TITLE GENERATOR (CAPTIONS TAB)
// ==========================================================================
let hookGenSeed = 0;
const HOOK_TEMPLATES = {
 curiosity: [
  { tag: '98 CTR', fn: t => '99% of people completely missed this about ' + t + ' 👀' },
  { tag: '96 CTR', fn: t => 'Nobody is talking about what actually happened with ' + t },
  { tag: '95 CTR', fn: t => 'Wait until the last 3 seconds of ' + t + '…' },
  { tag: '94 CTR', fn: t => 'The hidden detail inside ' + t + ' changes everything' },
  { tag: '93 CTR', fn: t => 'Stop scrolling — you need to see this ' + t + ' breakdown' },
  { tag: '92 CTR', fn: t => 'Did you catch the secret in ' + t + '? Most people didn’t' },
  { tag: '91 CTR', fn: t => 'Here is the real reason ' + t + ' is blowing up right now' },
  { tag: '90 CTR', fn: t => 'Watch closely — ' + t + ' is NOT what you think' },
 ],
 shock: [
  { tag: '98 CTR', fn: t => 'NO WAY this actually happened in ' + t + ' 💀' },
  { tag: '96 CTR', fn: t => 'I still can’t believe ' + t + ' is real…' },
  { tag: '95 CTR', fn: t => 'Wait… HOW did ' + t + ' just pull that off?! ⚡' },
  { tag: '94 CTR', fn: t => 'This ' + t + ' moment broke my brain 🤯' },
  { tag: '93 CTR', fn: t => 'Everything went wrong in ' + t + ' in 0.5 seconds' },
  { tag: '92 CTR', fn: t => 'I paused ' + t + ' at the exact wrong frame 💀' },
  { tag: '91 CTR', fn: t => 'Bro really thought nobody would notice ' + t },
  { tag: '90 CTR', fn: t => 'My jaw dropped when ' + t + ' did THIS' },
 ],
 gta6: [
  { tag: '99 CTR', fn: t => 'Rockstar actually put THIS detail in ' + t + ' 🌴' },
  { tag: '97 CTR', fn: t => 'GTA 6 realism just peaked with ' + t + ' 🔥' },
  { tag: '96 CTR', fn: t => '99% of GTA fans missed this ' + t + ' Easter egg 👀' },
  { tag: '95 CTR', fn: t => 'Why ' + t + ' proves GTA 6 is 10 years ahead' },
  { tag: '94 CTR', fn: t => 'This ' + t + ' leak / detail just confirmed everything' },
  { tag: '93 CTR', fn: t => 'Vice City is going to be INSANE because of ' + t },
  { tag: '92 CTR', fn: t => 'Pause at 0:03 — did Rockstar really sneak ' + t + ' in?!' },
  { tag: '91 CTR', fn: t => 'Every GTA 6 player needs to know about ' + t },
 ],
 funny: [
  { tag: '97 CTR', fn: t => 'I actually lost it when ' + t + ' happened 😂' },
  { tag: '95 CTR', fn: t => 'Who let ' + t + ' cook like this?! 💀' },
  { tag: '94 CTR', fn: t => 'Trying to stay serious during ' + t + ' (IMPOSSIBLE)' },
  { tag: '93 CTR', fn: t => 'Instant karma hit ' + t + ' at 100 mph 😂' },
  { tag: '92 CTR', fn: t => 'Bro had ONE job in ' + t + ' and fumbled it' },
  { tag: '91 CTR', fn: t => 'Average 5 seconds of ' + t + ' pure chaos' },
  { tag: '90 CTR', fn: t => 'Zero survival instincts detected in ' + t + ' 💀' },
  { tag: '89 CTR', fn: t => 'Why is ' + t + ' funnier every time it loops?!' },
 ],
 debate: [
  { tag: '97 CTR', fn: t => 'Unpopular opinion: ' + t + ' is overrated (here’s why)' },
  { tag: '96 CTR', fn: t => 'Am I the only one who thinks ' + t + ' is crazy?' },
  { tag: '95 CTR', fn: t => 'Be honest — would you actually try ' + t + '? 👇' },
  { tag: '94 CTR', fn: t => 'Everyone gets ' + t + ' completely wrong' },
  { tag: '93 CTR', fn: t => 'Hot take: ' + t + ' just ruined every other game for me' },
  { tag: '92 CTR', fn: t => 'Rate this ' + t + ' moment 1–10 in the comments' },
  { tag: '91 CTR', fn: t => 'W or L? Look what happened with ' + t },
  { tag: '90 CTR', fn: t => 'Tell me I’m wrong about ' + t + '… I’ll wait' },
 ],
 story: [
  { tag: '97 CTR', fn: t => 'POV: 5 seconds into ' + t + ' and everything goes sideways' },
  { tag: '95 CTR', fn: t => 'The wildest ' + t + ' story you’ll hear all week' },
  { tag: '94 CTR', fn: t => 'How ' + t + ' went from 0 to 100 real quick' },
  { tag: '93 CTR', fn: t => 'I tried ' + t + ' so you don’t have to…' },
  { tag: '92 CTR', fn: t => 'Day 1 of testing ' + t + ' — did NOT expect this' },
  { tag: '91 CTR', fn: t => 'Watch how fast ' + t + ' turned into pure chaos' },
  { tag: '90 CTR', fn: t => 'This is why you NEVER underestimate ' + t },
  { tag: '89 CTR', fn: t => 'Wait for the plot twist in ' + t + '…' },
 ],
};

function cleanTopicFromClip() {
 const manual = ($('hook-gen-topic')?.value || '').trim();
 if (manual) return manual;
 const row = app()?.getSelected?.();
 const raw = ($('title')?.value || row?.name || '').trim()
  .replace(/\.[^.]+$/, '')
  .replace(/[_-]+/g, ' ')
  .replace(/#\S+/g, '')
  .replace(/\s+/g, ' ')
  .trim();
 if (raw && raw.length >= 3) return raw.slice(0, 48);
 return 'this GTA 6 clip';
}

function renderGeneratedHooks(seedTopic) {
 const box = $('hook-gen-results');
 if (!box) return;
 const vibe = $('hook-gen-vibe')?.value || 'curiosity';
 const topic = (seedTopic || cleanTopicFromClip()).trim();
 const pool = HOOK_TEMPLATES[vibe] || HOOK_TEMPLATES.curiosity;
 const rotated = pool.map((_, i) => pool[(i + hookGenSeed) % pool.length]).slice(0, 6);
 hookGenSeed = (hookGenSeed + 2) % pool.length;

 box.replaceChildren(...rotated.map(item => {
  const text = item.fn(topic);
  const card = app().node('div', undefined, 'hook-gen-item');
  const top = app().node('div', undefined, 'hook-gen-top');
  top.append(app().node('span', item.tag, 'hook-gen-score'), app().node('span', text, 'hook-gen-text'));
  const acts = app().node('div', undefined, 'hook-gen-actions');
  const bTitle = app().node('button', 'Use Title', 'mini-btn');
  bTitle.type = 'button';
  bTitle.onclick = () => {
   if ($('title')) $('title').value = text.slice(0, 100);
   app().refreshDraftEditor();
   app().workflowDirty();
   app().note('Applied generated hook to Title!');
  };
  const bCap = app().node('button', 'Use Caption', 'mini-btn');
  bCap.type = 'button';
  bCap.onclick = () => {
   if ($('caption')) {
    const cur = $('caption').value.trim();
    $('caption').value = cur ? text + '\n\n' + cur : text;
   }
   app().refreshDraftEditor();
   app().workflowDirty();
   app().note('Added generated hook to Caption!');
  };
  acts.append(bTitle, bCap);
  card.append(top, acts);
  return card;
 }));
}

if ($('hook-gen-go')) $('hook-gen-go').onclick = () => renderGeneratedHooks();
if ($('hook-gen-vibe')) $('hook-gen-vibe').onchange = () => renderGeneratedHooks();
if ($('hook-gen-topic')) $('hook-gen-topic').addEventListener('keydown', e => { if (e.key === 'Enter') { e.preventDefault(); renderGeneratedHooks(); } });
if ($('hook-gen-from-transcript')) $('hook-gen-from-transcript').onclick = () => {
 if (!state.words.length) return;
 const snippet = state.words.slice(0, 7).map(w => w.text).join(' ').replace(/[.!?]+$/, '');
 if ($('hook-gen-topic')) $('hook-gen-topic').value = snippet.slice(0, 60);
 renderGeneratedHooks(snippet);
 app().note('Seeded Hook Generator from your spoken transcript!');
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
 const row = e.detail?.row;
 stopCaptionRaf();
 state.words = row?.clip_id ? loadClipCaptions(row.clip_id) : [];
 if ($('ac-highlights-list')) $('ac-highlights-list').replaceChildren();
 if ($('ac-highlights-status')) $('ac-highlights-status').textContent = '';
 if ($('hook-gen-topic')) $('hook-gen-topic').value = '';
 if ($('ac-transcribe-status')) {
  $('ac-transcribe-status').textContent = state.words.length
   ? '✓ Restored ' + state.words.length + ' saved caption words for this clip.'
   : 'Uses vocal-isolated Whisper AI + DTW word alignment for tight karaoke sync.';
 }
 renderTranscriptLines();
 syncClipSelectionUI(row);
 renderGeneratedHooks();
});

window.addEventListener('snapmatic:exported', () => {
 if ($('studio-active-summary')) $('studio-active-summary').textContent = '✓ ' + ($('edit-state')?.textContent || 'Exported MP4 with Pro Studio!');
});

const vidEl = app()?.getVideo();
if (vidEl) {
 vidEl.addEventListener('play', () => { renderLiveCaptions(true); startCaptionRaf(); });
 vidEl.addEventListener('pause', () => { stopCaptionRaf(); renderLiveCaptions(true); });
 vidEl.addEventListener('ended', () => { stopCaptionRaf(); renderLiveCaptions(true); });
 vidEl.addEventListener('emptied', stopCaptionRaf);
 vidEl.addEventListener('seeked', () => renderLiveCaptions(true));
 vidEl.addEventListener('timeupdate', () => { if (vidEl.paused) renderLiveCaptions(false); });
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
renderGeneratedHooks();
syncClipSelectionUI(app()?.getSelected?.());
setTimeout(() => syncClipSelectionUI(app()?.getSelected?.()), 1000);
})();
