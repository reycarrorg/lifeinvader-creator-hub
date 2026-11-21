'use strict';
(() => {
const $ = id => document.getElementById(id);
const app = () => window.snapmaticApp;

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

const DEFAULT_CAPTION_STYLE = {
 preset: 'hormozi',
 words_per_line: 3,
 font_size: 68,
 pos_y_pct: 74,
 active_color: '#FFE600',
 uppercase: true,
 pop_animation: true,
 show_emojis: true,
};

const state = {
 words: [],
 autoPlan: null,
 lastAudit: null,
 opusConfigured: false,
 opusTemplateId: '',
 submagicConfigured: false,
 submagicTemplate: 'Hormozi 2',
 submagicBrolls: true,
 submagicZooms: true,
 busy: false,
 activeKeyProvider: null,
 runAfterSave: false,
};

let captionRaf = null;
let lastRenderedWordKey = '';
let cloudPollTimer = null;
let exportPollTimer = null;
let auditSeq = 0;

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
// 60FPS LIVE VIEWFINDER KARAOKE PREVIEW
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
 const cfg = DEFAULT_CAPTION_STYLE;
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
 const renderKey = phraseIdx + '|' + activeWordIdx;
 if (!force && renderKey === lastRenderedWordKey) return;
 lastRenderedWordKey = renderKey;

 layer.hidden = false;
 layer.dataset.preset = cfg.preset;
 layer.style.top = cfg.pos_y_pct + '%';
 const vfW = $('viewfinder')?.clientWidth || 275;
 const scaledPx = Math.max(11, Math.round((cfg.font_size / 1080) * vfW * 1.12));

 layer.replaceChildren(...activePhrase.map(item => {
  const isAct = item.idx === activeWordIdx;
  let txt = item.word.text.trim().toUpperCase();
  if (isAct && item.word.emoji) txt += ' ' + item.word.emoji;
  const sp = app().node('span', txt, 'vf-autocap-word' + (isAct ? ' active' : ''));
  sp.style.fontSize = scaledPx + 'px';
  if (isAct) sp.style.color = item.word.color || cfg.active_color;
  return sp;
 }));
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

// ==========================================================================
// UI HELPERS & PROGRESS BAR
// ==========================================================================
function setProgress(show, pct, text, allowCancel = false) {
 const wrap = $('ac3-progress-wrap'), bar = $('ac3-progress-bar'), out = $('ac3-progress-text'), cBtn = $('ac3-cancel-btn');
 if (wrap) wrap.hidden = !show;
 if (bar && Number.isFinite(pct)) bar.value = Math.max(0, Math.min(100, pct));
 if (out && text !== undefined) out.textContent = text;
 if (cBtn) cBtn.hidden = !allowCancel;
}

function syncButtons() {
 const row = app()?.getSelected?.();
 const hasClip = Boolean(row?.from_sources);
 const locked = !hasClip || state.busy;
 if ($('studio-no-clip-banner')) $('studio-no-clip-banner').hidden = hasClip;
 if ($('btn-auto-snapmatic')) $('btn-auto-snapmatic').disabled = locked;
 if ($('btn-auto-opus')) $('btn-auto-opus').disabled = locked;
 if ($('btn-auto-submagic')) $('btn-auto-submagic').disabled = locked;

 const oBadge = $('opus-key-badge');
 if (oBadge) {
  oBadge.textContent = state.opusConfigured ? 'READY' : 'SET KEY';
  oBadge.className = 'ac3-pill ' + (state.opusConfigured ? 'ac3-pill-ready' : 'ac3-pill-warn');
 }
 const sBadge = $('submagic-key-badge');
 if (sBadge) {
  sBadge.textContent = state.submagicConfigured ? 'READY' : 'SET KEY';
  sBadge.className = 'ac3-pill ' + (state.submagicConfigured ? 'ac3-pill-ready' : 'ac3-pill-warn');
 }
 if ($('hook-gen-from-transcript')) $('hook-gen-from-transcript').hidden = state.words.length === 0;
 renderLiveCaptions(true);
}

// ==========================================================================
// AUTOMATIC TIKTOK ORIGINAL CONTENT GUIDELINES AUDITOR
// ==========================================================================
function renderAudit(data) {
 const scoreEl = $('tt-audit-score');
 const verdictEl = $('tt-audit-verdict');
 const targetEl = $('tt-audit-target');
 const listEl = $('tt-audit-list');
 if (!scoreEl || !listEl) return;

 if (!data) {
  scoreEl.textContent = '--';
  scoreEl.className = 'tt-score-pill';
  if (verdictEl) verdictEl.hidden = true;
  if (targetEl) targetEl.textContent = 'Select a clip — checks automatically.';
  listEl.replaceChildren();
  return;
 }

 const sc = Number(data.score) || 0;
 const cls = sc >= 82 ? 'pass' : sc >= 55 ? 'warn' : 'fail';
 scoreEl.textContent = sc + '/100';
 scoreEl.className = 'tt-score-pill ' + cls;

 if (verdictEl) {
  verdictEl.hidden = false;
  verdictEl.textContent = data.verdict || '';
  verdictEl.className = 'tt-verdict-tag ' + cls;
 }
 if (targetEl) {
  const kind = data.target === 'export' ? 'Exported Render' : 'Raw Source Clip';
  const dim = data.width && data.height ? ` · ${data.width}×${data.height}` : '';
  targetEl.textContent = `Audited: ${kind}${dim}`;
 }

 const badge = $('itab-create-badge');
 if (badge) {
  badge.textContent = String(sc);
  badge.hidden = false;
 }

 const checks = Array.isArray(data.checks) ? data.checks : [];
 listEl.replaceChildren(...checks.map(chk => {
  const li = app().node('li', undefined, 'tt-audit-item ' + (chk.status || 'warn'));
  const icon = chk.status === 'pass' ? '✓' : chk.status === 'fail' ? '✕' : '!';
  const ic = app().node('span', icon, 'tt-chk-icon ' + (chk.status || 'warn'));
  const body = app().node('div', undefined, 'tt-chk-body');
  body.append(app().node('strong', chk.label || ''), app().node('small', chk.detail || ''));
  li.append(ic, body);
  return li;
 }));
}

async function runTikTokAudit(clipId, target = 'auto') {
 if (!clipId) {
  state.lastAudit = null;
  renderAudit(null);
  return null;
 }
 const seq = ++auditSeq;
 if ($('tt-audit-target')) $('tt-audit-target').textContent = 'Checking TikTok Original Content Guidelines…';
 if ($('tt-audit-score')) {
  $('tt-audit-score').textContent = '…';
  $('tt-audit-score').className = 'tt-score-pill';
 }
 try {
  const data = await app().get('/api/workspace/studio/tiktok-audit/' + encodeURIComponent(clipId), {
   method: 'POST',
   headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({ target }),
  });
  if (seq !== auditSeq) return data;
  state.lastAudit = data;
  renderAudit(data);
  return data;
 } catch (e) {
  if (seq === auditSeq && $('tt-audit-target')) {
   $('tt-audit-target').textContent = 'Audit unavailable: ' + e.message;
  }
  return null;
 }
}

// ==========================================================================
// BUTTON 1: ⚡ SNAPMATIC AUTO-CREATE (LOCAL 1-CLICK ORIGINAL CONTENT PIPELINE)
// ==========================================================================
function stopExportPoll() {
 if (exportPollTimer) { clearInterval(exportPollTimer); exportPollTimer = null; }
}

async function runSnapmaticAutoCreate() {
 const row = app()?.getSelected?.();
 if (!row?.from_sources || state.busy) return;
 state.busy = true;
 syncButtons();
 if ($('studio-active-summary')) $('studio-active-summary').textContent = '';

 try {
  // Step 1: Study raw source clip against TikTok Original Content Guidelines
  setProgress(true, 10, 'Step 1/4: Studying clip against TikTok Original Content Guidelines…');
  const audit = await runTikTokAudit(row.clip_id, 'source');
  const plan = audit?.auto_fix_plan || {
   pre_crop: null,
   detail_restore: true,
   dynamic_zoom: true,
   remove_mid_silences: true,
   studio_voice_enhance: true,
   color_grade: 'crisp_rec709',
   add_captions: true,
  };

  // Step 2: Generate Pro Word-by-Word Karaoke Captions via Whisper AI
  let words = loadClipCaptions(row.clip_id);
  if (plan.add_captions !== false && !words.length) {
   setProgress(true, 28, 'Step 2/4: Generating Pro Word-by-Word Karaoke Captions (Whisper AI)…');
   try {
    const es = app()?.getEditState?.();
    const tr = await app().get('/api/workspace/studio/transcribe/' + encodeURIComponent(row.clip_id), {
     method: 'POST',
     headers: { 'Content-Type': 'application/json' },
     body: JSON.stringify({
      in_ms: es?.in_ms || 0,
      out_ms: es?.out_ms || null,
      language: 'en',
     }),
    });
    if (Array.isArray(tr.words) && tr.words.length) {
     words = tr.words;
     saveClipCaptions(row.clip_id, words);
    }
   } catch (e) {
    // Continue gracefully even if clip has no clear speech
    console.warn('Auto-Create captions skipped:', e.message);
   }
  }
  state.words = words;
  renderLiveCaptions(true);

  // Auto-seed a punchy title if empty
  if (words.length && $('title') && !$('title').value.trim()) {
   const hookSnippet = words.slice(0, 7).map(w => w.text).join(' ').replace(/[.!?]+$/, '');
   if (hookSnippet.length >= 6) {
    $('title').value = hookSnippet.slice(0, 80);
    app()?.refreshDraftEditor?.();
   }
  }

  // Build automatic studio plan
  const studioPlan = {
   caption_words: words,
   caption_style: words.length ? DEFAULT_CAPTION_STYLE : null,
   pre_crop: plan.pre_crop || null,
   detail_restore: true,
   dynamic_zoom: true,
   remove_mid_silences: Boolean(plan.remove_mid_silences),
   silence_threshold_db: -32,
   min_silence_s: 0.42,
   studio_voice_enhance: Boolean(plan.studio_voice_enhance),
   color_grade: plan.color_grade || 'crisp_rec709',
  };
  state.autoPlan = studioPlan;

  // Step 3: Render transformed 1080x1920 High-Bitrate MP4 with scrubbed metadata
  setProgress(true, 45, 'Step 3/4: Rendering transformed 1080p MP4 (reframe + polish + pacing + captions)…');
  const es = app()?.getEditState?.();
  stopExportPoll();
  exportPollTimer = setInterval(async () => {
   try {
    const p = await app().get('/api/workspace/film-roll/export-progress');
    if (p?.active) {
     const mapped = 45 + Math.round((Number(p.percent) || 0) * 0.47);
     setProgress(true, mapped, p.message || `Rendering 1080p MP4 (${p.percent || 0}%)…`);
    }
   } catch {}
  }, 600);

  const exportBody = {
   framing: 'center_crop',
   quality: 'high',
   in_ms: es?.in_ms || 0,
   out_ms: es?.out_ms || null,
   mute: false,
   normalize_audio: true,
   use_hw_encoder: true,
   save_caption_txt: false,
   title: ($('title')?.value || row.name || '').trim(),
   caption_text: ($('caption')?.value || '').trim(),
   ...studioPlan,
  };

  const res = await app().get('/api/workspace/film-roll/' + encodeURIComponent(row.clip_id) + '/export', {
   method: 'POST',
   headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify(exportBody),
  });
  stopExportPoll();

  row.exported = true;
  row.exported_file = res.file_name;
  row.export_url = res.export_url;
  row.state = 'exported';

  // Step 4: Automatically verify the finished MP4 against TikTok Guidelines!
  setProgress(true, 95, 'Step 4/4: Verifying finished MP4 against TikTok Original Content Guidelines…');
  const finalAudit = await runTikTokAudit(row.clip_id, 'export');
  setProgress(false, 100, '');

  const scoreTxt = finalAudit ? ` · TikTok Score: ${finalAudit.score}/100` : '';
  const summary = `✓ Snapmatic Auto-Create exported ${res.file_name}${scoreTxt}`;
  if ($('studio-active-summary')) $('studio-active-summary').textContent = summary;
  if ($('edit-state')) $('edit-state').textContent = summary;

  window.dispatchEvent(new CustomEvent('snapmatic:exported', { detail: { clip_id: row.clip_id } }));
  await app()?.reload?.();
  app()?.note?.(summary, [
   { label: 'Upload →', onClick: () => document.querySelector('[data-itab="upload"]')?.click() },
  ]);
 } catch (e) {
  stopExportPoll();
  setProgress(false, 0, '');
  const msg = '✕ Snapmatic Auto-Create failed: ' + e.message;
  if ($('studio-active-summary')) $('studio-active-summary').textContent = msg;
  app()?.note?.(msg);
 } finally {
  state.busy = false;
  syncButtons();
 }
}

// ==========================================================================
// BUTTONS 2 & 3: 🟣 OPUSCLIP (OPUS.PRO) & ✨ SUBMAGIC CLOUD AI
// ==========================================================================
async function refreshProvidersStatus(checkCompletion = false) {
 try {
  const s = await app().get('/api/workspace/studio/providers/status');
  state.opusConfigured = Boolean(s.opus_configured);
  state.opusTemplateId = s.opus_template_id || '';
  state.submagicConfigured = Boolean(s.submagic_configured);
  state.submagicTemplate = s.submagic_template || 'Hormozi 2';
  state.submagicBrolls = s.submagic_brolls !== false;
  state.submagicZooms = s.submagic_zooms !== false;

  const job = s.job || {};
  if (job.active) {
   state.busy = true;
   setProgress(true, Number(job.percent) || 10, job.message || (job.provider + ' processing…'), true);
   if (!cloudPollTimer) cloudPollTimer = setInterval(() => refreshProvidersStatus(true), 1400);
  } else if (checkCompletion && cloudPollTimer) {
   clearInterval(cloudPollTimer);
   cloudPollTimer = null;
   state.busy = false;
   setProgress(false, 100, '', false);
   if (job.stage === 'done' && job.result) {
    const cid = job.clip_id || app()?.getSelected?.()?.clip_id;
    const finalAudit = cid ? await runTikTokAudit(cid, 'export') : null;
    const scoreTxt = finalAudit ? ` · TikTok Score: ${finalAudit.score}/100` : '';
    const msg = `✓ ${job.provider} ready: ${job.result.file_name}${scoreTxt}`;
    if ($('studio-active-summary')) $('studio-active-summary').textContent = msg;
    if ($('edit-state')) $('edit-state').textContent = msg;
    await app()?.reload?.();
    app()?.note?.(msg, [
     { label: 'Upload →', onClick: () => document.querySelector('[data-itab="upload"]')?.click() },
    ]);
   } else if (job.error) {
    const msg = `✕ ${job.provider}: ${job.error}`;
    if ($('studio-active-summary')) $('studio-active-summary').textContent = msg;
    app()?.note?.(msg);
   }
  }
  syncButtons();
 } catch {}
}

function openProviderKeyDialog(provider, runAfterSave = false) {
 const dlg = $('provider-key-dialog');
 if (!dlg) return;
 state.activeKeyProvider = provider;
 state.runAfterSave = runAfterSave;
 if ($('pk-error')) $('pk-error').textContent = '';
 if ($('pk-api-key')) $('pk-api-key').value = '';

 const isOpus = provider === 'opus';
 const configured = isOpus ? state.opusConfigured : state.submagicConfigured;
 if ($('pk-title')) $('pk-title').textContent = isOpus ? '🟣 Connect OpusClip (opus.pro)' : '✨ Connect Submagic AI';
 if ($('pk-desc')) {
  $('pk-desc').textContent = isOpus
   ? 'Paste your OpusClip API key (from opus.pro dashboard → API). Stored securely in Windows Credential Manager.'
   : 'Paste your Submagic API key (from app.submagic.co → API). Stored securely in Windows Credential Manager.';
 }
 if ($('pk-opus-fields')) $('pk-opus-fields').hidden = !isOpus;
 if ($('pk-submagic-fields')) $('pk-submagic-fields').hidden = isOpus;
 if (isOpus && $('pk-opus-tpl')) $('pk-opus-tpl').value = state.opusTemplateId;
 if (!isOpus) {
  if ($('pk-sub-tpl')) $('pk-sub-tpl').value = state.submagicTemplate || 'Hormozi 2';
  if ($('pk-sub-brolls')) $('pk-sub-brolls').checked = state.submagicBrolls;
  if ($('pk-sub-zooms')) $('pk-sub-zooms').checked = state.submagicZooms;
 }
 if ($('pk-api-key')) {
  $('pk-api-key').placeholder = configured ? 'Leave blank to keep saved key, or paste new key…' : 'Paste your API key…';
 }
 if ($('pk-remove')) $('pk-remove').hidden = !configured;
 if ($('pk-save')) $('pk-save').textContent = runAfterSave ? 'Save Key & Run' : 'Save Key';
 dlg.showModal();
 $('pk-api-key')?.focus();
}

async function runCloudProvider(provider) {
 const row = app()?.getSelected?.();
 if (!row?.from_sources || state.busy) return;
 const configured = provider === 'opus' ? state.opusConfigured : state.submagicConfigured;
 if (!configured) {
  openProviderKeyDialog(provider, true);
  return;
 }
 state.busy = true;
 syncButtons();
 const label = provider === 'opus' ? 'OpusClip' : 'Submagic';
 setProgress(true, 8, `Starting ${label} cloud AI job…`, true);
 try {
  await app().get(`/api/workspace/studio/${provider}/run/` + encodeURIComponent(row.clip_id), {
   method: 'POST',
  });
  if (!cloudPollTimer) cloudPollTimer = setInterval(() => refreshProvidersStatus(true), 1400);
 } catch (e) {
  state.busy = false;
  setProgress(false, 0, '', false);
  syncButtons();
  const msg = `✕ ${label}: ${e.message}`;
  if ($('studio-active-summary')) $('studio-active-summary').textContent = msg;
  app()?.note?.(msg);
 }
}

if ($('btn-auto-snapmatic')) $('btn-auto-snapmatic').onclick = runSnapmaticAutoCreate;
if ($('btn-auto-opus')) $('btn-auto-opus').onclick = () => runCloudProvider('opus');
if ($('btn-auto-submagic')) $('btn-auto-submagic').onclick = () => runCloudProvider('submagic');
if ($('btn-opus-key')) $('btn-opus-key').onclick = () => openProviderKeyDialog('opus', false);
if ($('btn-submagic-key')) $('btn-submagic-key').onclick = () => openProviderKeyDialog('submagic', false);

if ($('ac3-cancel-btn')) $('ac3-cancel-btn').onclick = async () => {
 try { await app().get('/api/workspace/studio/providers/cancel', { method: 'POST' }); } catch {}
};

if ($('pk-close')) $('pk-close').onclick = () => $('provider-key-dialog')?.close();

if ($('pk-remove')) $('pk-remove').onclick = async () => {
 if (!state.activeKeyProvider) return;
 try {
  await app().get('/api/workspace/studio/providers/key', {
   method: 'POST',
   headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({ provider: state.activeKeyProvider, api_key: '' }),
  });
  await refreshProvidersStatus(false);
  $('provider-key-dialog')?.close();
  app()?.note?.('API key removed from Windows Credential Manager.');
 } catch (e) {
  if ($('pk-error')) $('pk-error').textContent = e.message;
 }
};

if ($('provider-key-form')) $('provider-key-form').onsubmit = async e => {
 e.preventDefault();
 const prov = state.activeKeyProvider;
 if (!prov) return;
 const rawKey = ($('pk-api-key')?.value || '').trim();
 const already = prov === 'opus' ? state.opusConfigured : state.submagicConfigured;
 if (!rawKey && !already) {
  if ($('pk-error')) $('pk-error').textContent = 'Paste your API key to continue.';
  return;
 }
 try {
  const payload = {
   provider: prov,
   api_key: rawKey,
   keep_existing_key: !rawKey && already,
  };
  if (prov === 'opus') {
   payload.brand_template_id = ($('pk-opus-tpl')?.value || '').trim();
  } else {
   payload.template_name = ($('pk-sub-tpl')?.value || 'Hormozi 2').trim();
   payload.magic_brolls = Boolean($('pk-sub-brolls')?.checked);
   payload.magic_zooms = Boolean($('pk-sub-zooms')?.checked);
  }
  await app().get('/api/workspace/studio/providers/key', {
   method: 'POST',
   headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify(payload),
  });
  await refreshProvidersStatus(false);
  $('provider-key-dialog')?.close();
  app()?.note?.((prov === 'opus' ? 'OpusClip' : 'Submagic') + ' API key saved securely!');
  if (state.runAfterSave) {
   state.runAfterSave = false;
   runCloudProvider(prov);
  }
 } catch (err) {
  if ($('pk-error')) $('pk-error').textContent = err.message;
 }
};

if ($('studio-pick-first')) $('studio-pick-first').onclick = () => {
 const first = document.querySelector('#queue-list .clip-row, #queue-list button[data-clip-id]');
 if (first) first.click();
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
 ],
 shock: [
  { tag: '98 CTR', fn: t => 'NO WAY this actually happened in ' + t + ' 💀' },
  { tag: '96 CTR', fn: t => 'I still can’t believe ' + t + ' is real…' },
  { tag: '95 CTR', fn: t => 'Wait… HOW did ' + t + ' just pull that off?! ⚡' },
  { tag: '94 CTR', fn: t => 'This ' + t + ' moment broke my brain 🤯' },
  { tag: '93 CTR', fn: t => 'Everything went wrong in ' + t + ' in 0.5 seconds' },
  { tag: '92 CTR', fn: t => 'Bro really thought nobody would notice ' + t },
 ],
 gta6: [
  { tag: '99 CTR', fn: t => 'Rockstar actually put THIS detail in ' + t + ' 🌴' },
  { tag: '97 CTR', fn: t => 'GTA 6 realism just peaked with ' + t + ' 🔥' },
  { tag: '96 CTR', fn: t => '99% of GTA fans missed this ' + t + ' Easter egg 👀' },
  { tag: '95 CTR', fn: t => 'Why ' + t + ' proves GTA 6 is 10 years ahead' },
  { tag: '94 CTR', fn: t => 'Vice City is going to be INSANE because of ' + t },
  { tag: '92 CTR', fn: t => 'Pause at 0:03 — did Rockstar really sneak ' + t + ' in?!' },
 ],
 funny: [
  { tag: '97 CTR', fn: t => 'I actually lost it when ' + t + ' happened 😂' },
  { tag: '95 CTR', fn: t => 'Who let ' + t + ' cook like this?! 💀' },
  { tag: '94 CTR', fn: t => 'Trying to stay serious during ' + t + ' (IMPOSSIBLE)' },
  { tag: '93 CTR', fn: t => 'Instant karma hit ' + t + ' at 100 mph 😂' },
  { tag: '92 CTR', fn: t => 'Bro had ONE job in ' + t + ' and fumbled it' },
  { tag: '90 CTR', fn: t => 'Zero survival instincts detected in ' + t + ' 💀' },
 ],
 debate: [
  { tag: '97 CTR', fn: t => 'Unpopular opinion: ' + t + ' is overrated (here’s why)' },
  { tag: '96 CTR', fn: t => 'Am I the only one who thinks ' + t + ' is crazy?' },
  { tag: '95 CTR', fn: t => 'Be honest — would you actually try ' + t + '? 👇' },
  { tag: '94 CTR', fn: t => 'Rate this ' + t + ' moment 1–10 in the comments' },
  { tag: '92 CTR', fn: t => 'W or L? Look what happened with ' + t },
  { tag: '90 CTR', fn: t => 'Tell me I’m wrong about ' + t + '… I’ll wait' },
 ],
 story: [
  { tag: '97 CTR', fn: t => 'POV: 5 seconds into ' + t + ' and everything goes sideways' },
  { tag: '95 CTR', fn: t => 'The wildest ' + t + ' story you’ll hear all week' },
  { tag: '94 CTR', fn: t => 'How ' + t + ' went from 0 to 100 real quick' },
  { tag: '93 CTR', fn: t => 'I tried ' + t + ' so you don’t have to…' },
  { tag: '91 CTR', fn: t => 'Watch how fast ' + t + ' turned into pure chaos' },
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
 return raw && raw.length >= 3 ? raw.slice(0, 48) : 'this GTA 6 clip';
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
};

// ==========================================================================
// LIFECYCLE EVENTS
// ==========================================================================
window.addEventListener('snapmatic:clip-selected', e => {
 const row = e.detail?.row;
 stopCaptionRaf();
 state.words = row?.clip_id ? loadClipCaptions(row.clip_id) : [];
 state.autoPlan = null;
 if ($('hook-gen-topic')) $('hook-gen-topic').value = '';
 if ($('studio-active-summary')) $('studio-active-summary').textContent = '';
 syncButtons();
 renderGeneratedHooks();
 if (row?.from_sources) runTikTokAudit(row.clip_id, 'auto');
 else renderAudit(null);
});

window.addEventListener('snapmatic:exported', e => {
 const cid = e.detail?.clip_id || app()?.getSelected?.()?.clip_id;
 if (cid && !state.busy) runTikTokAudit(cid, 'export');
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
  ...(state.autoPlan || {}),
  caption_words: state.words,
  caption_style: state.words.length ? DEFAULT_CAPTION_STYLE : null,
 }),
};

refreshProvidersStatus(false);
renderGeneratedHooks();
syncButtons();
setTimeout(() => {
 const row = app()?.getSelected?.();
 syncButtons();
 if (row?.from_sources && !state.lastAudit) runTikTokAudit(row.clip_id, 'auto');
}, 900);
})();
