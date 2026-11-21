'use strict';
(() => {
const $ = id => document.getElementById(id);
const app = () => window.snapmaticApp;

let unexportedDirty = false;
let uploadPollTimer = null;
let batchUploadQueue = [];

// 1. Live Upload Progress Bar Poller (with MB/s & ETA telemetry)
function pollUploadProgress() {
 const wrap = $('upload-progress-wrap'), bar = $('upload-progress'), txt = $('upload-progress-text');
 if (!wrap) return;
 app().get('/api/oauth/upload-progress').then(p => {
  wrap.hidden = !p.active;
  if (p.active) {
   if (bar) bar.value = Number(p.percent) || 0;
   const parts = [(p.message || 'Uploading…') + ' (' + (Number(p.percent) || 0) + '%)'];
   if (Number(p.speed_mib_s) > 0.05) parts.push(Number(p.speed_mib_s).toFixed(1) + ' MiB/s');
   if (Number.isFinite(Number(p.eta_secs)) && Number(p.eta_secs) > 0) parts.push('~' + Math.round(Number(p.eta_secs)) + 's left');
   if (txt) txt.textContent = parts.join(' · ');
  }
 }).catch(() => {});
}
function ensureUploadPolling() {
 if (uploadPollTimer) return;
 uploadPollTimer = setInterval(() => {
  if (!document.hidden) pollUploadProgress();
 }, 900);
}
ensureUploadPolling();

// 2. Creator Streak & Weekly Goal Tracker
const GOAL_KEY = 'snapmatic-weekly-goal-v1';
function getWeeklyGoal() {
 try { const n = Number(localStorage.getItem(GOAL_KEY)); return n >= 1 && n <= 50 ? n : 7; } catch { return 7; }
}
function updateStreakPill() {
 const pill = $('streak-pill');
 if (!pill) return;
 app().get('/api/oauth/analytics').then(d => {
  const posts = Array.isArray(d?.posts) ? d.posts : [];
  const now = new Date();
  const weekAgo = new Date(now.getTime() - 7 * 86400000);
  const weekCount = posts.filter(p => p.published_at && new Date(p.published_at) >= weekAgo).length;
  const daySet = new Set();
  for (const p of posts) {
   if (!p.published_at) continue;
   const dt = new Date(p.published_at);
   if (!Number.isNaN(dt.getTime())) daySet.add(dt.getFullYear() + '-' + (dt.getMonth() + 1) + '-' + dt.getDate());
  }
  let streak = 0;
  for (let i = 0; i < 60; i++) {
   const d2 = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i);
   const k = d2.getFullYear() + '-' + (d2.getMonth() + 1) + '-' + d2.getDate();
   if (daySet.has(k)) streak++;
   else if (i > 0) break;
  }
  const goal = getWeeklyGoal();
  pill.textContent = '🔥 ' + streak + 'd · ' + weekCount + '/' + goal + ' wk';
  pill.title = streak + '-day streak · ' + weekCount + ' of ' + goal + ' posts this week (click to edit goal)';
 }).catch(() => { pill.textContent = '🔥 ' + getWeeklyGoal() + '/wk'; });
}
if ($('streak-pill')) {
 $('streak-pill').onclick = () => {
  const cur = getWeeklyGoal();
  const raw = window.prompt('Weekly posting goal (posts per 7 days):', String(cur));
  if (raw == null) return;
  const n = Math.max(1, Math.min(50, Math.round(Number(raw) || cur)));
  try { localStorage.setItem(GOAL_KEY, String(n)); } catch {}
  updateStreakPill();
  app().note('Updated weekly posting goal to ' + n + ' posts/week.');
 };
}

// 3. Live Viewfinder FX, Crop Pan, Rotate, Mirror & Zoom Preview
let holdOriginal = false;
function applyLiveViewfinderFx() {
 const v = app()?.getVideo();
 if (!v) return;
 const on = Boolean($('live-fx-preview')?.checked) && !holdOriginal;
 const framing = $('edit-framing')?.value || 'original';
 const panRow = $('crop-pan-row');
 if (panRow) panRow.hidden = framing !== 'center_crop' && !framing.startsWith('split_');
 const panVal = Number($('edit-crop-pan')?.value) || 0;
 if ($('crop-pan-val')) $('crop-pan-val').textContent = panVal === 0 ? 'Center' : (panVal < 0 ? 'Left ' + Math.abs(panVal) + '%' : 'Right ' + panVal + '%');
 if (!on) {
  v.style.filter = '';
  v.style.transform = '';
  v.style.objectFit = '';
  v.style.objectPosition = '';
  return;
 }
 v.style.filter = $('edit-vivid')?.checked ? 'contrast(1.08) saturate(1.18) brightness(1.02)' : '';
 if (framing === 'center_crop') {
  v.style.objectFit = 'cover';
  v.style.objectPosition = ((panVal + 100) / 2) + '% 50%';
 } else {
  v.style.objectFit = 'contain';
  v.style.objectPosition = '50% 50%';
 }
 const rot = Number($('edit-rotate')?.value) || 0;
 const mir = Boolean($('edit-mirror')?.checked);
 const zoom = Boolean($('edit-zoom')?.checked);
 const tf = [];
 if (rot) tf.push('rotate(' + rot + 'deg)');
 if (mir) tf.push('scaleX(-1)');
 if (zoom) tf.push('scale(1.06)');
 v.style.transform = tf.join(' ');
}
for (const id of ['live-fx-preview', 'edit-framing', 'edit-crop-pan', 'edit-vivid', 'edit-rotate', 'edit-mirror', 'edit-zoom', 'edit-force-30fps', 'edit-audio-preset', 'edit-audio-gain', 'edit-outro-cta']) {
 const el = $(id);
 if (!el) continue;
 const mark = () => {
  if (id !== 'live-fx-preview') { unexportedDirty = true; renderUploadReadiness(); }
  applyLiveViewfinderFx();
  if ($('edit-gain-val')) $('edit-gain-val').textContent = (Number($('edit-audio-gain')?.value) || 0) + ' dB';
 };
 el.addEventListener('input', mark);
 el.addEventListener('change', mark);
}
if ($('edit-music-vol')) $('edit-music-vol').addEventListener('input', () => {
 if ($('edit-music-vol-val')) $('edit-music-vol-val').textContent = ($('edit-music-vol').value || '-18') + ' dB';
 unexportedDirty = true;
});
if ($('live-fx-hold')) {
 const down = () => { holdOriginal = true; applyLiveViewfinderFx(); };
 const up = () => { holdOriginal = false; applyLiveViewfinderFx(); };
 $('live-fx-hold').addEventListener('pointerdown', down);
 $('live-fx-hold').addEventListener('pointerup', up);
 $('live-fx-hold').addEventListener('pointerleave', up);
}

// 4. Platform Export Presets (1-Click)
const EXPORT_PRESETS = {
 universal: { label: 'Universal 9:16', framing: 'center_crop', quality: 'balanced', norm: true, fade: true, vivid: true, speed: '1', fps30: false },
 reels: { label: 'IG Reels Safe', framing: 'center_crop', quality: 'balanced', norm: true, fade: true, vivid: false, speed: '1', fps30: true },
 shorts: { label: 'YT Shorts Max', framing: 'center_crop', quality: 'high', norm: true, fade: false, vivid: true, speed: '1', fps30: false },
 x: { label: 'X Crisp ≤140s', framing: 'original', quality: 'balanced', norm: true, fade: false, vivid: false, speed: '1', fps30: false },
};
document.querySelectorAll('#export-presets-bar button[data-epreset]').forEach(btn => {
 btn.onclick = () => {
  const p = EXPORT_PRESETS[btn.dataset.epreset];
  if (!p) return;
  if ($('edit-framing')) { $('edit-framing').value = p.framing; $('edit-framing').dispatchEvent(new Event('change')); }
  if ($('edit-quality')) $('edit-quality').value = p.quality;
  if ($('edit-norm')) $('edit-norm').checked = p.norm;
  if ($('edit-fade-io')) $('edit-fade-io').checked = p.fade;
  if ($('edit-vivid')) $('edit-vivid').checked = p.vivid;
  if ($('edit-export-speed')) $('edit-export-speed').value = p.speed;
  if ($('edit-force-30fps')) $('edit-force-30fps').checked = p.fps30;
  unexportedDirty = true;
  applyLiveViewfinderFx();
  renderUploadReadiness();
  app().note('Applied "' + p.label + '" export preset.');
 };
});

// 5. Audio Music Bed Picker
window.snapmaticProMusicPath = null;
if ($('edit-music-pick')) $('edit-music-pick').onclick = async () => {
 const btn = $('edit-music-pick'); btn.disabled = true;
 try {
  const r = await app().get('/api/workspace/film-roll/pick-music', { method: 'POST' });
  if (r.path) {
   window.snapmaticProMusicPath = r.path;
   if ($('edit-music-name')) $('edit-music-name').textContent = 'Music bed: ' + (r.name || r.path);
   if ($('edit-music-clear')) $('edit-music-clear').hidden = false;
   unexportedDirty = true;
   app().note('Attached background music: ' + (r.name || 'track'));
  }
 } catch (e) { app().note(e.message); }
 finally { btn.disabled = false; }
};
if ($('edit-music-clear')) $('edit-music-clear').onclick = () => {
 window.snapmaticProMusicPath = null;
 if ($('edit-music-name')) $('edit-music-name').textContent = 'No background music attached.';
 $('edit-music-clear').hidden = true;
};

// 6. Pro Viral Hook & Title Generator lives in workspace-studio.js

// 7. Smart Auto-Format for All Platforms
if ($('auto-format-all')) $('auto-format-all').onclick = () => {
 const row = app().getSelected();
 const rawTitle = ($('title')?.value || row?.name || 'GTA 6 Clip').trim();
 const rawCap = ($('caption')?.value || '').trim();
 const rawTags = ($('tags')?.value || '').trim() || '#GTA6 #GTAVI #RockstarGames #GTA6Clips';
 const cleanTitle = rawTitle.replace(/\s+/g, ' ').slice(0, 92);
 $('title').value = cleanTitle;
 $('tags').value = rawTags;
 if ($('draft-shorts')) { $('draft-shorts').checked = true; $('draft-shorts').dispatchEvent(new Event('change')); }
 // Extract YouTube comma-separated keyword tags
 const kwSet = new Set(
  (cleanTitle + ' ' + rawTags)
   .toLowerCase()
   .replace(/[^a-z0-9#\s]/g, ' ')
   .split(/\s+/)
   .map(w => w.replace(/^#/, ''))
   .filter(w => w.length >= 3 && !['the', 'and', 'for', 'this', 'that', 'with', 'you'].includes(w))
 );
 const ytKeywords = Array.from(kwSet).slice(0, 14).join(', ');
 if ($('draft-yt-api-tags')) {
  $('draft-yt-api-tags').value = ytKeywords;
  $('draft-yt-api-tags').dispatchEvent(new Event('input'));
 }
 // Clean X <= 280 chars with max 2 hashtags
 const xTags = (rawTags.match(/#[a-zA-Z0-9_]+/g) || []).slice(0, 2).join(' ');
 const xBase = (rawCap || cleanTitle).replace(/\s+/g, ' ');
 const maxBody = Math.max(20, 278 - xTags.length);
 let xFinal = xBase.length > maxBody ? xBase.slice(0, maxBody - 1).replace(/\s+\S*$/, '') + '…' : xBase;
 if (xTags && !xFinal.toLowerCase().includes(xTags.split(' ')[0].toLowerCase())) xFinal = (xFinal + ' ' + xTags).trim();
 if ($('draft-x')) { $('draft-x').value = xFinal.slice(0, 280); $('draft-x').dispatchEvent(new Event('input')); }
 if ($('x-upload-text')) $('x-upload-text').value = xFinal.slice(0, 280);
 app().refreshDraftEditor();
 app().renderPreflight();
 app().workflowDirty();
 renderUploadReadiness();
 app().note('Auto-formatted captions, #Shorts, YouTube keywords & 280-char X post!');
};

// Sync X inline upload box <-> Captions X draft
if ($('x-upload-text')) $('x-upload-text').addEventListener('input', () => {
 if ($('draft-x')) { $('draft-x').value = $('x-upload-text').value; $('draft-x').dispatchEvent(new Event('input')); }
});
if ($('draft-x')) $('draft-x').addEventListener('input', () => {
 if ($('x-upload-text') && document.activeElement !== $('x-upload-text')) $('x-upload-text').value = $('draft-x').value;
});
// Sync IG upload Share-to-Feed <-> draft checkbox
if ($('ig-upload-share')) $('ig-upload-share').onchange = () => {
 if ($('draft-ig-share')) { $('draft-ig-share').checked = $('ig-upload-share').checked; $('draft-ig-share').dispatchEvent(new Event('change')); }
};
// IG Cover Frame Scrubber
if ($('ig-thumb-slider')) $('ig-thumb-slider').addEventListener('input', () => {
 const ms = Number($('ig-thumb-slider').value) || 0;
 if ($('ig-thumb-val')) $('ig-thumb-val').textContent = (ms / 1000).toFixed(1) + 's';
 const v = app().getVideo();
 if (v && Number.isFinite(v.duration) && v.duration > 0) v.currentTime = Math.min(v.duration, ms / 1000);
 const pdSt = app().getPDState();
 if (pdSt) pdSt.cover = { frame_ms: ms, thumb_offset_ms: ms };
});

// 8. YouTube Quick-Schedule Buttons
function toLocalInputStr(dt) {
 const p = n => String(n).padStart(2, '0');
 return dt.getFullYear() + '-' + p(dt.getMonth() + 1) + '-' + p(dt.getDate()) + 'T' + p(dt.getHours()) + ':' + p(dt.getMinutes());
}
document.querySelectorAll('#yt-quick-sched button[data-qsched]').forEach(btn => {
 btn.onclick = async () => {
  const mode = btn.dataset.qsched;
  const input = $('yt-draft-schedule');
  if (!input) return;
  if (mode === 'clear') {
   input.value = '';
   input.dispatchEvent(new Event('change'));
   return;
  }
  const now = new Date();
  let dt = new Date(now);
  if (mode === 'today18') {
   dt.setHours(18, 0, 0, 0);
   if (dt <= now) dt.setDate(dt.getDate() + 1);
  } else if (mode === 'tmrw12') {
   dt.setDate(dt.getDate() + 1); dt.setHours(12, 0, 0, 0);
  } else if (mode === 'tmrw18') {
   dt.setDate(dt.getDate() + 1); dt.setHours(18, 0, 0, 0);
  } else if (mode === 'nextopen') {
   await app().loadCalendar();
   const busy = new Set();
   for (const it of app().getCalendarItems() || []) {
    const d = new Date(it.date);
    if (!Number.isNaN(d.getTime())) busy.add(d.getFullYear() + '-' + (d.getMonth() + 1) + '-' + d.getDate());
   }
   dt = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 18, 0, 0, 0);
   for (let i = 0; i < 30; i++) {
    const k = dt.getFullYear() + '-' + (dt.getMonth() + 1) + '-' + dt.getDate();
    if (!busy.has(k)) break;
    dt.setDate(dt.getDate() + 1);
   }
  }
  input.value = toLocalInputStr(dt);
  input.dispatchEvent(new Event('change'));
  app().note('Scheduled YouTube draft for ' + dt.toLocaleString() + '.');
 };
});

// 9. Upload Readiness & Pre-Upload Summary + Auto-Fix / Reels <=89s Variant
function renderUploadReadiness() {
 const grid = $('upload-readiness'), warn = $('upload-unexported-warn'), fixBtn = $('upload-autofix-btn'), shortsBtn = $('upload-shorts-cut-btn'), reelsBtn = $('upload-reels-cut-btn'), sumBox = $('upload-summary-preview');
 const row = app()?.getSelected();
 if (!grid) return;
 if (!row?.from_sources) {
  grid.textContent = 'Select a Film Roll clip to check Shorts, Reels, TikTok & X readiness.';
  if (warn) warn.hidden = true;
  if (fixBtn) fixBtn.hidden = true;
  if (shortsBtn) shortsBtn.hidden = true;
  if (reelsBtn) reelsBtn.hidden = true;
  return;
 }
 const v = app().getVideo();
 const es = app().getEditState();
 const durS = es && es.out_ms > es.in_ms ? (es.out_ms - es.in_ms) / 1000 : (Number.isFinite(v?.duration) ? v.duration : 0);
 const w = v?.videoWidth || 0, h = v?.videoHeight || 0;
 const vertical = h > w || Boolean(row.exported) || ($('edit-framing') ? $('edit-framing').value !== 'original' : false);
 const facts = app().getMediaFacts()?.[row.preview_sha256];
 const vCodec = (facts?.streams?.find(s => s.codec_type === 'video')?.codec_name || '').toLowerCase();
 const h264Ok = row.exported || !vCodec || vCodec === 'h264';
 const sizeMiB = (row.preview_bytes || 0) / 1048576;

 const checks = [
  { id: 'youtube', name: 'YouTube Shorts', ok: durS <= 60 && vertical && sizeMiB <= 250, warn: durS > 60 && durS <= 180 && vertical && sizeMiB <= 250, msg: durS > 180 ? 'Over 180s (regular video)' : !vertical ? 'Landscape (crop to 9:16 for Shorts)' : durS > 60 ? '3-min Short (' + durS.toFixed(0) + 's) — trim ≤59s if Content ID music puts it in Videos' : 'Guaranteed Short (' + durS.toFixed(1) + 's · 9:16 · #Shorts)' },
  { id: 'instagram', name: 'Instagram Reels', ok: durS <= 90 && vertical && h264Ok, warn: durS > 90 && durS <= 180, msg: !h264Ok ? 'Codec ' + vCodec + ' — auto-optimized on upload' : durS > 90 ? 'Over 90s (' + durS.toFixed(0) + 's) — recommend ≤89s (auto-optimized 48 kHz)' : !vertical ? 'Crop to 9:16 for Reels' : 'Ready (' + durS.toFixed(1) + 's · H.264)' },
  { id: 'tiktok', name: 'TikTok Inbox', ok: durS <= 600 && h264Ok, msg: !h264Ok ? 'Export H.264 MP4 recommended' : 'Ready (' + durS.toFixed(1) + 's)' },
  { id: 'x', name: 'X Video', ok: durS <= 140 && h264Ok, msg: durS > 140 ? 'Over 140s X free-tier limit (' + durS.toFixed(0) + 's)' : 'Ready (' + durS.toFixed(1) + 's · ' + app().xPostText().length + '/280 chars)' },
 ];
 grid.replaceChildren(...checks.map(c => {
  const cls = c.ok ? 'ok' : c.warn ? 'warn' : 'fail';
  const el = app().node('div', undefined, 'ready-pill ' + cls);
  const head = app().node('strong');
  head.append(app().brandIcon(c.id), ' ' + c.name + (c.ok ? ' ✓' : c.warn ? ' ⚠' : ' ✕'));
  el.append(head, app().node('span', c.msg, 'muted'));
  return el;
 }));
 const hasPendingEdits = unexportedDirty || Boolean(es?.dirty);
 if (warn) warn.hidden = !hasPendingEdits;
 if (fixBtn) fixBtn.hidden = !(!h264Ok || !vertical || hasPendingEdits);
 if (shortsBtn) shortsBtn.hidden = !(durS > 60);
 if (reelsBtn) reelsBtn.hidden = !(durS > 90);
 if (sumBox) {
  const fileLabel = row.exported ? ('Exported MP4 (' + row.exported_file + ')') : ('Original source (' + row.name + ')');
  sumBox.textContent = 'File: ' + fileLabel + ' · Title: "' + (($('title')?.value || row.name).slice(0, 60)) + '" · X: ' + app().xPostText().length + '/280 chars';
 }
}
if ($('upload-autofix-btn')) $('upload-autofix-btn').onclick = () => {
 if ($('edit-framing') && $('edit-framing').value === 'original') $('edit-framing').value = 'center_crop';
 if ($('edit-norm')) $('edit-norm').checked = true;
 if ($('edit-force-30fps')) $('edit-force-30fps').checked = true;
 app().setInspectorTab('export');
 $('edit-transcode')?.click();
};
if ($('upload-shorts-cut-btn')) $('upload-shorts-cut-btn').onclick = async () => {
 const row = app().getSelected(), es = app().getEditState();
 if (!row?.from_sources || !es) return;
 const start = es.in_ms || 0;
 const end = Math.min(es.duration_ms, start + 59000);
 app().setTrim('out', end);
 if ($('edit-framing') && $('edit-framing').value === 'original') $('edit-framing').value = 'center_crop';
 if ($('yt-draft-shorts')) $('yt-draft-shorts').checked = true;
 app().note('Trimmed clip to guaranteed ≤59s YouTube Short and starting export…');
 app().setInspectorTab('export');
 $('edit-transcode')?.click();
};
if ($('upload-reels-cut-btn')) $('upload-reels-cut-btn').onclick = async () => {
 const row = app().getSelected(), es = app().getEditState();
 if (!row?.from_sources || !es) return;
 const start = es.in_ms || 0;
 const end = Math.min(es.duration_ms, start + 89000);
 app().setTrim('out', end);
 if ($('edit-framing') && $('edit-framing').value === 'original') $('edit-framing').value = 'center_crop';
 if ($('edit-force-30fps')) $('edit-force-30fps').checked = true;
 app().note('Trimmed clip to Reels-safe 89s and starting export…');
 app().setInspectorTab('export');
 $('edit-transcode')?.click();
};

// 10. Duplicate SHA-256 Check + Cloud Upload Receipts Table
async function checkDuplicateHash(row) {
 const box = $('dup-hash-warning');
 if (!box) return;
 if (!row?.from_sources) { box.hidden = true; return; }
 try {
  const r = await app().get('/api/oauth/duplicate-check/' + encodeURIComponent(row.clip_id));
  const matches = Array.isArray(r.matches) ? r.matches : [];
  if (!matches.length) { box.hidden = true; return; }
  box.hidden = false;
  const lines = matches.slice(0, 4).map(m => {
   const d = app().node('div');
   const dt = m.uploaded_at ? new Date(m.uploaded_at * 1000).toLocaleDateString() : '';
   d.append(app().brandIcon(m.provider), ' ' + m.provider.toUpperCase() + ' (' + (m.status || 'uploaded') + (dt ? ' · ' + dt : '') + ') ');
   if (m.remote_url) {
    const a = app().node('a', 'Open ↗', 'oauth-link'); a.href = m.remote_url; a.target = '_blank'; a.rel = 'noopener noreferrer';
    d.append(a);
   }
   return d;
  });
  box.replaceChildren(app().node('strong', '⚠ Duplicate File SHA-256 Guard — exact content previously uploaded:'), ...lines);
 } catch { box.hidden = true; }
}
async function loadUploadReceipts() {
 const tbody = $('upload-receipts-rows'), notice = $('upload-receipts-notice');
 if (!tbody) return;
 try {
  const d = await app().get('/api/oauth/receipts');
  const rows = Array.isArray(d.items) ? d.items : [];
  tbody.replaceChildren(...rows.map(r => {
   const tr = app().node('tr');
   const pTd = app().node('td'); pTd.append(app().brandIcon(r.provider), ' ' + r.provider.toUpperCase());
   const linkTd = app().node('td');
   if (r.remote_url) {
    const a = app().node('a', 'Open ↗', 'oauth-link'); a.href = r.remote_url; a.target = '_blank'; a.rel = 'noopener noreferrer';
    linkTd.append(a);
   } else {
    linkTd.textContent = r.remote_id || '—';
   }
   const stText = [r.status, r.privacy, r.scheduled_at ? 'sched ' + new Date(r.scheduled_at).toLocaleDateString() : null].filter(Boolean).join(' · ');
   tr.append(
    pTd,
    app().node('td', r.title || r.clip_id.slice(0, 10)),
    app().node('td', stText),
    app().node('td', r.content_sha256 ? r.content_sha256.slice(0, 12) + '…' : '—', 'hash'),
    app().node('td', r.uploaded_at ? new Date(r.uploaded_at * 1000).toLocaleString() : '—'),
    linkTd
   );
   return tr;
  }));
  if (notice) notice.textContent = rows.length ? rows.length + ' cloud upload receipt' + (rows.length === 1 ? '' : 's') + ' tracked by SHA-256' : 'No cloud uploads recorded yet.';
 } catch { if (notice) notice.textContent = 'Could not load upload receipts.'; }
}
window.addEventListener('snapmatic:ledger-opened', loadUploadReceipts);

// 11. Batch Multi-Clip Upload Queue
function renderBatchUploadQueue() {
 const list = $('batch-queue-list'), cnt = $('batch-queue-count'), runBtn = $('batch-queue-run'), clrBtn = $('batch-queue-clear');
 if (!list) return;
 if (cnt) cnt.textContent = String(batchUploadQueue.length);
 if (runBtn) runBtn.disabled = batchUploadQueue.length === 0;
 if (clrBtn) clrBtn.hidden = batchUploadQueue.length === 0;
 list.replaceChildren(...batchUploadQueue.map((item, idx) => {
  const li = app().node('li');
  const rm = app().node('button', '✕', 'toast-close'); rm.type = 'button'; rm.style.float = 'right';
  rm.onclick = () => { batchUploadQueue.splice(idx, 1); renderBatchUploadQueue(); };
  li.append(rm, (idx + 1) + '. ' + item.title.slice(0, 36) + ' → [' + item.targets.join(', ').toUpperCase() + ']');
  return li;
 }));
}
if ($('batch-queue-add')) $('batch-queue-add').onclick = () => {
 const row = app().getSelected();
 if (!row?.from_sources) return;
 const targets = [];
 if ($('ship-chk-yt')?.checked && !$('ship-chk-yt').disabled) targets.push('youtube');
 if ($('ship-chk-ig')?.checked && !$('ship-chk-ig').disabled) targets.push('instagram');
 if ($('ship-chk-tt')?.checked && !$('ship-chk-tt').disabled) targets.push('tiktok');
 if ($('ship-chk-x')?.checked && !$('ship-chk-x').disabled) targets.push('x');
 if (!targets.length) { app().note('Check at least one platform above before adding to the batch queue.'); return; }
 const common = { title: $('title').value.trim() || row.name, caption: $('caption').value.trim(), tags: $('tags').value.trim() };
 const mats = app().pd.materialize(common, app().getPDState()?.overrides || {}, app().getPDState() || {});
 batchUploadQueue.push({
  clip_id: row.clip_id,
  exported: Boolean(row.exported),
  title: common.title,
  yt_desc: mats.youtube,
  yt_tags: $('draft-yt-api-tags')?.value || common.tags,
  yt_cat: $('yt-draft-category')?.value || '20',
  ig_cap: mats.instagram,
  ig_share: Boolean($('ig-upload-share')?.checked),
  ig_publish: $('ship-ig-mode')?.value === 'publish',
  x_text: app().xPostText(),
  targets,
 });
 renderBatchUploadQueue();
 app().note('Added "' + common.title + '" to Batch Upload Queue (' + batchUploadQueue.length + ' queued).');
};
if ($('batch-queue-clear')) $('batch-queue-clear').onclick = () => { batchUploadQueue = []; renderBatchUploadQueue(); };
if ($('batch-queue-run')) $('batch-queue-run').onclick = async () => {
 if (!batchUploadQueue.length) return;
 const hasLive = batchUploadQueue.some(q => (q.targets.includes('instagram') && q.ig_publish) || q.targets.includes('x'));
 if (hasLive && !window.confirm('Batch queue includes LIVE Instagram or X posts. Continue?')) return;
 const btn = $('batch-queue-run'); btn.disabled = true;
 const spaceYt = Boolean($('batch-space-yt')?.checked);
 const baseDate = new Date(); baseDate.setDate(baseDate.getDate() + 1); baseDate.setHours(18, 0, 0, 0);
 let done = 0, failed = 0, ytIdx = 0;
 const itemsCopy = batchUploadQueue.slice();
 for (const item of itemsCopy) {
  for (const t of item.targets) {
   try {
    if (t === 'youtube') {
     let publish_at = null;
     if (spaceYt) {
      const d = new Date(baseDate.getTime() + ytIdx * 86400000);
      publish_at = d.toISOString();
      ytIdx++;
     }
     await app().get('/api/oauth/youtube/upload-draft/' + encodeURIComponent(item.clip_id), {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title: item.title, description: item.yt_desc, tags: item.yt_tags, category_id: item.yt_cat, as_short: Boolean($('yt-draft-shorts') ? $('yt-draft-shorts').checked : true), privacy: 'private', publish_at, use_source: !item.exported })
     });
    } else if (t === 'instagram') {
     await app().get('/api/oauth/instagram/upload-reel/' + encodeURIComponent(item.clip_id), {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ caption: item.ig_cap, share_to_feed: item.ig_share, use_source: !item.exported, publish: item.ig_publish })
     });
    } else if (t === 'tiktok') {
     await app().get('/api/oauth/tiktok/upload-draft/' + encodeURIComponent(item.clip_id), {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ use_source: !item.exported })
     });
    } else if (t === 'x') {
     await app().get('/api/oauth/x/post-video/' + encodeURIComponent(item.clip_id), {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ text: item.x_text, use_source: !item.exported, confirm_publish: true })
     });
    }
    done++;
   } catch { failed++; }
  }
 }
 batchUploadQueue = [];
 renderBatchUploadQueue();
 await app().reload(); app().loadExportHistory(); loadUploadReceipts(); updateStreakPill();
 app().note('Batch upload finished: ' + done + ' succeeded' + (failed ? ', ' + failed + ' failed' : '') + '.');
};

// 12. Auto-Pick 3 Best Cover Frames + Cover Thumbnail Designer Canvas
if ($('cover-auto-btn')) $('cover-auto-btn').onclick = async () => {
 const row = app().getSelected(), btn = $('cover-auto-btn'), box = $('cover-candidates');
 if (!row?.from_sources || btn.disabled) return;
 btn.disabled = true; btn.textContent = 'Scanning frames…';
 try {
  const r = await app().get('/api/workspace/film-roll/' + encodeURIComponent(row.clip_id) + '/cover-candidates', { method: 'POST' });
  const list = Array.isArray(r.candidates) ? r.candidates : [];
  if (!list.length) { app().note('Could not extract candidate frames.'); return; }
  box.hidden = false;
  box.replaceChildren(...list.map(c => {
   const b = app().node('button', undefined, 'cover-cand-btn'); b.type = 'button';
   const im = app().node('img'); im.src = c.data_url; im.alt = 'Cover ' + c.label;
   b.append(im, app().node('small', c.label));
   b.onclick = async () => {
    const imgObj = new Image();
    imgObj.onload = () => {
     const cv = document.createElement('canvas'); cv.width = 1080; cv.height = 1920;
     const ctx = cv.getContext('2d');
     ctx.fillStyle = '#000'; ctx.fillRect(0, 0, 1080, 1920);
     ctx.drawImage(imgObj, 0, 0, 1080, 1920);
     cv.toBlob(blob => {
      if (!blob) return;
      app().setCoverBlob(blob, c.thumb_offset_ms, c.thumb_offset_ms);
      if ($('cover-design-apply')) $('cover-design-apply').disabled = false;
      app().note('Selected cover frame at ' + c.label + ' (ready for YouTube & Instagram).');
     }, 'image/png');
    };
    imgObj.src = c.data_url;
   };
   return b;
  }));
 } catch (e) { app().note(e.message); }
 finally { btn.disabled = false; btn.textContent = 'Auto-Pick 3 Best Covers'; }
};

if ($('cover-design-apply')) $('cover-design-apply').onclick = async () => {
 const url = app().getCoverUrl();
 if (!url) { app().note('Capture or auto-pick a cover frame first.'); return; }
 const headline = ($('cover-design-title')?.value || $('title')?.value || '').trim().toUpperCase().slice(0, 50);
 const vignette = Boolean($('cover-design-vignette')?.checked);
 const border = Boolean($('cover-design-border')?.checked);
 const img = new Image();
 img.onload = () => {
  const cv = document.createElement('canvas'); cv.width = 1080; cv.height = 1920;
  const ctx = cv.getContext('2d');
  ctx.drawImage(img, 0, 0, 1080, 1920);
  if (vignette) {
   const g = ctx.createLinearGradient(0, 0, 0, 1920);
   g.addColorStop(0, 'rgba(0,0,0,0.45)');
   g.addColorStop(0.28, 'rgba(0,0,0,0)');
   g.addColorStop(0.68, 'rgba(0,0,0,0)');
   g.addColorStop(1, 'rgba(0,0,0,0.72)');
   ctx.fillStyle = g; ctx.fillRect(0, 0, 1080, 1920);
  }
  if (border) {
   ctx.strokeStyle = '#29B6F6'; ctx.lineWidth = 24;
   ctx.strokeRect(12, 12, 1056, 1896);
  }
  if (headline) {
   ctx.font = '900 84px Impact, Arial Black, sans-serif';
   ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
   const words = headline.split(/\s+/);
   const lines = []; let cur = '';
   for (const w of words) {
    const test = cur ? cur + ' ' + w : w;
    if (ctx.measureText(test).width > 920 && cur) { lines.push(cur); cur = w; } else cur = test;
   }
   if (cur) lines.push(cur);
   const startY = 1480 - (lines.length * 48);
   lines.slice(0, 3).forEach((ln, i) => {
    const y = startY + i * 102;
    ctx.lineWidth = 18; ctx.strokeStyle = '#000'; ctx.strokeText(ln, 540, y);
    ctx.fillStyle = '#FFE600'; ctx.fillText(ln, 540, y);
   });
  }
  cv.toBlob(blob => {
   if (!blob) return;
   const curCov = app().getPDState()?.cover;
   app().setCoverBlob(blob, curCov?.frame_ms || 0, curCov?.thumb_offset_ms || 0);
   app().note('Stamped custom Cover Thumbnail PNG (1080×1920)!');
  }, 'image/png');
 };
 img.src = url;
};

// 13. Multi-Part Splitter + Zip Post Kit
if ($('split-parts-go')) $('split-parts-go').onclick = async () => {
 const row = app().getSelected(), btn = $('split-parts-go'), out = $('split-parts-result');
 if (!row?.from_sources || btn.disabled) return;
 btn.disabled = true; if (out) out.textContent = 'Splitting & rendering vertical parts with FFmpeg…';
 try {
  const res = await app().get('/api/workspace/film-roll/' + encodeURIComponent(row.clip_id) + '/split-parts', {
   method: 'POST', headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({
    part_seconds: Number($('split-part-sec')?.value) || 55,
    framing: $('edit-framing')?.value || 'center_crop',
    quality: $('edit-quality')?.value || 'balanced',
    use_hw_encode: Boolean($('edit-hw')?.checked),
    crop_pan: (Number($('edit-crop-pan')?.value) || 0) / 100,
    hook_prefix: ($('split-part-hook')?.value || '').trim() || null,
    watermark_text: ($('edit-watermark')?.value || '').trim() || null,
    watermark_pos: $('edit-wm-pos')?.value || 'bottom_right',
    normalize_audio: Boolean($('edit-norm')?.checked),
    fade_io: Boolean($('edit-fade-io')?.checked),
    vivid_filter: Boolean($('edit-vivid')?.checked),
   })
  });
  const msg = 'Exported ' + res.count + ' parts (' + res.parts.map(p => p.file_name).join(', ') + ').';
  if (out) out.textContent = msg;
  app().note(msg, [{ label: 'Open Folder', onClick: () => $('edit-open-exports')?.click() }]);
  app().loadExportHistory(); app().refreshLocalStatus();
 } catch (e) { if (out) out.textContent = 'Split failed: ' + e.message; app().note(e.message); }
 finally { btn.disabled = false; }
};

if ($('export-zip-kit')) $('export-zip-kit').onclick = async () => {
 const row = app().getSelected(), btn = $('export-zip-kit');
 if (!row?.from_sources || btn.disabled) return;
 btn.disabled = true;
 try {
  const cov = await app().coverBase64();
  const r = await app().get('/api/workspace/film-roll/' + encodeURIComponent(row.clip_id) + '/zip-kit', {
   method: 'POST', headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({ caption_text: app().buildCaptionPackText(), cover_png_base64: cov })
  });
  app().note('Created Post Kit ' + r.zip_name + ' (' + (r.bytes / 1048576).toFixed(1) + ' MiB) & opened in Explorer!');
 } catch (e) { app().note('Zip kit failed: ' + e.message); }
 finally { btn.disabled = false; }
};

// 14. Star Ratings, Mood Tags & Clickable Timeline Markers
function renderClipRatingAndMarkers(row) {
 const stars = document.querySelectorAll('#rating-stars button[data-star]');
 const r = Number(row?.rating || 0);
 stars.forEach(b => b.classList.toggle('on', Number(b.dataset.star) <= r));
 const moods = new Set(String(row?.mood_tags || '').split(',').map(s => s.trim()).filter(Boolean));
 document.querySelectorAll('#mood-pills button[data-mood]').forEach(b => b.setAttribute('aria-pressed', String(moods.has(b.dataset.mood))));

 const pins = $('trim-markers'), list = $('marker-list');
 const markers = Array.isArray(row?.markers) ? row.markers : [];
 const es = app().getEditState();
 const dur = es?.duration_ms || ((app().getVideo()?.duration || 0) * 1000) || 1;
 if (pins) {
  pins.replaceChildren(...markers.map(m => {
   const pin = app().node('span', undefined, 'trim-marker-pin');
   pin.style.left = Math.max(0, Math.min(100, ((Number(m.ms) || 0) / dur) * 100)) + '%';
   pin.title = app().time((Number(m.ms) || 0) / 1000) + ' · ' + (m.label || 'Marker') + ' (click to jump)';
   pin.onclick = e => {
    e.stopPropagation();
    const v = app().getVideo();
    if (v && Number.isFinite(v.duration)) v.currentTime = Math.min(v.duration, (Number(m.ms) || 0) / 1000);
   };
   return pin;
  }));
 }
 if (list) {
  list.replaceChildren(...markers.map((m, idx) => {
   const chip = app().node('button', app().time((Number(m.ms) || 0) / 1000).slice(0, 5) + ' ' + (m.label || 'Pin') + ' ×', 'marker-chip');
   chip.type = 'button'; chip.title = 'Click to jump · Shift+Click to delete marker';
   chip.onclick = async e => {
    if (e.shiftKey) {
     row.markers.splice(idx, 1);
     await saveClipMeta(row, { markers: row.markers });
     renderClipRatingAndMarkers(row);
    } else {
     const v = app().getVideo();
     if (v && Number.isFinite(v.duration)) v.currentTime = Math.min(v.duration, (Number(m.ms) || 0) / 1000);
    }
   };
   return chip;
  }));
 }
}
async function saveClipMeta(row, payload) {
 if (!row?.from_sources) return;
 try {
  await app().get('/api/workspace/film-roll/' + encodeURIComponent(row.clip_id) + '/meta', {
   method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload)
  });
 } catch {}
}
document.querySelectorAll('#rating-stars button[data-star]').forEach(btn => {
 btn.onclick = async () => {
  const row = app().getSelected(); if (!row?.from_sources) return;
  const val = Number(btn.dataset.star);
  row.rating = row.rating === val ? 0 : val;
  renderClipRatingAndMarkers(row); app().renderRoll(true);
  await saveClipMeta(row, { rating: row.rating });
  app().note(row.rating ? 'Rated ' + '★'.repeat(row.rating) : 'Cleared rating.');
 };
});
document.querySelectorAll('#mood-pills button[data-mood]').forEach(btn => {
 btn.onclick = async () => {
  const row = app().getSelected(); if (!row?.from_sources) return;
  const set = new Set(String(row.mood_tags || '').split(',').map(s => s.trim()).filter(Boolean));
  if (set.has(btn.dataset.mood)) set.delete(btn.dataset.mood); else set.add(btn.dataset.mood);
  row.mood_tags = Array.from(set).join(', ');
  renderClipRatingAndMarkers(row);
  await saveClipMeta(row, { mood_tags: row.mood_tags });
 };
});
if ($('marker-add')) $('marker-add').onclick = async () => {
 const row = app().getSelected(), v = app().getVideo();
 if (!row?.from_sources || !v || !Number.isFinite(v.currentTime)) return;
 const ms = Math.round(v.currentTime * 1000);
 const label = ($('marker-label')?.value || '').trim() || 'Marker';
 if (!Array.isArray(row.markers)) row.markers = [];
 row.markers.push({ ms, label });
 row.markers.sort((a, b) => (a.ms || 0) - (b.ms || 0));
 if ($('marker-label')) $('marker-label').value = '';
 renderClipRatingAndMarkers(row);
 await saveClipMeta(row, { markers: row.markers });
 app().note('Pinned marker "' + label + '" at ' + app().time(ms / 1000) + '.');
};

// 15. Speed-Triage Keyboard Shortcuts (1-5 stars, A approve, H hold, N/P next/prev)
document.addEventListener('keydown', e => {
 if (e.ctrlKey || e.metaKey || e.altKey || e.repeat) return;
 if (e.target.closest('input,textarea,select,[contenteditable]') || document.querySelector('dialog[open]')) return;
 const row = app()?.getSelected();
 if ($('studio')?.hidden) return;
 if (/^[1-5]$/.test(e.key) && row?.from_sources) {
  e.preventDefault();
  document.querySelector('#rating-stars button[data-star="' + e.key + '"]')?.click();
  return;
 }
 const k = e.key.toLowerCase();
 if (k === 'a' && row?.from_sources && !$('approve')?.disabled) { e.preventDefault(); $('approve').click(); return; }
 if (k === 'h' && row?.from_sources && !$('hold-clip')?.disabled) { e.preventDefault(); $('hold-clip').click(); return; }
 if ((k === 'n' || k === 'p')) {
  const vis = app().getVisibleItems();
  if (!vis.length) return;
  const idx = vis.findIndex(x => x.clip_id === row?.clip_id);
  const next = vis[k === 'n' ? Math.min(vis.length - 1, idx + 1) : Math.max(0, idx - 1)];
  if (next && next.clip_id !== row?.clip_id) { e.preventDefault(); app().select(next); }
 }
});

// 16. Brand Kits & Settings Export / Import (.json, zero credentials)
const KITS_KEY = 'snapmatic-brand-kits-v1';
function readKits() { try { return JSON.parse(localStorage.getItem(KITS_KEY) || '[]') || []; } catch { return []; } }
function writeKits(list) { try { localStorage.setItem(KITS_KEY, JSON.stringify(list)); } catch {} }
function renderBrandKits() {
 const sel = $('brand-kit-select'), appBtn = $('brand-kit-apply'), delBtn = $('brand-kit-delete');
 if (!sel) return;
 const kits = readKits();
 if (!kits.length) {
  const o = app().node('option', 'No saved Brand Kits'); o.value = ''; sel.replaceChildren(o);
  if (appBtn) appBtn.disabled = true; if (delBtn) delBtn.disabled = true;
  return;
 }
 sel.replaceChildren(...kits.map((k, i) => { const o = app().node('option', k.name); o.value = String(i); return o; }));
 if (appBtn) appBtn.disabled = false; if (delBtn) delBtn.disabled = false;
}
if ($('brand-kit-save')) $('brand-kit-save').onclick = () => {
 const name = ($('brand-kit-name')?.value || '').trim();
 if (!name) { app().note('Enter a Brand Kit name first.'); return; }
 const kits = readKits().filter(k => k.name.toLowerCase() !== name.toLowerCase());
 kits.push({
  name,
  watermark: $('edit-watermark')?.value || '',
  wm_pos: $('edit-wm-pos')?.value || 'bottom_right',
  outro_cta: $('edit-outro-cta')?.value || '',
  yt_category: $('yt-draft-category')?.value || '20',
  framing: $('edit-framing')?.value || 'center_crop',
  quality: $('edit-quality')?.value || 'balanced',
  vivid: Boolean($('edit-vivid')?.checked),
  fade_io: Boolean($('edit-fade-io')?.checked),
  norm: Boolean($('edit-norm')?.checked),
  tags: $('tags')?.value || '',
 });
 writeKits(kits);
 if ($('brand-kit-name')) $('brand-kit-name').value = '';
 renderBrandKits();
 if ($('brand-kit-msg')) $('brand-kit-msg').textContent = 'Saved Brand Kit "' + name + '".';
 app().note('Saved Brand Kit "' + name + '".');
};
if ($('brand-kit-apply')) $('brand-kit-apply').onclick = () => {
 const kit = readKits()[Number($('brand-kit-select')?.value)];
 if (!kit) return;
 if ($('edit-watermark')) $('edit-watermark').value = kit.watermark || '';
 if ($('edit-wm-pos')) $('edit-wm-pos').value = kit.wm_pos || 'bottom_right';
 if ($('edit-outro-cta')) $('edit-outro-cta').value = kit.outro_cta || '';
 if ($('yt-draft-category')) $('yt-draft-category').value = kit.yt_category || '20';
 if ($('edit-framing')) $('edit-framing').value = kit.framing || 'center_crop';
 if ($('edit-quality')) $('edit-quality').value = kit.quality || 'balanced';
 if ($('edit-vivid')) $('edit-vivid').checked = Boolean(kit.vivid);
 if ($('edit-fade-io')) $('edit-fade-io').checked = Boolean(kit.fade_io);
 if ($('edit-norm')) $('edit-norm').checked = Boolean(kit.norm);
 if (kit.tags && $('tags')) $('tags').value = kit.tags;
 app().updateOverlayPreview(); applyLiveViewfinderFx();
 if ($('brand-kit-msg')) $('brand-kit-msg').textContent = 'Applied Brand Kit "' + kit.name + '".';
 app().note('Applied Brand Kit "' + kit.name + '"!');
};
if ($('brand-kit-delete')) $('brand-kit-delete').onclick = () => {
 const idx = Number($('brand-kit-select')?.value);
 const kits = readKits();
 if (!Number.isInteger(idx) || !kits[idx]) return;
 kits.splice(idx, 1); writeKits(kits); renderBrandKits();
};
if ($('settings-export-btn')) $('settings-export-btn').onclick = () => {
 const dump = {
  schema: 'snapmatic-settings-backup-v1',
  exported_at: new Date().toISOString(),
  theme: localStorage.getItem('lv-theme') || 'dark',
  weekly_goal: getWeeklyGoal(),
  export_prefs: JSON.parse(localStorage.getItem('snapmatic-export-prefs-v1') || '{}'),
  brand_kits: readKits(),
 };
 const blob = new Blob([JSON.stringify(dump, null, 2)], { type: 'application/json' });
 const a = document.createElement('a');
 a.href = URL.createObjectURL(blob); a.download = 'snapmatic-settings-backup.json'; a.click();
 setTimeout(() => URL.revokeObjectURL(a.href), 1500);
 app().note('Exported workspace settings & Brand Kits (no OAuth secrets included).');
};
if ($('settings-import-file')) $('settings-import-file').onchange = async e => {
 const f = e.currentTarget.files?.[0]; if (!f) return;
 try {
  const data = JSON.parse(await f.text());
  if (data.schema !== 'snapmatic-settings-backup-v1') throw Error('Unrecognized backup format.');
  if (Array.isArray(data.brand_kits)) writeKits(data.brand_kits);
  if (data.export_prefs) localStorage.setItem('snapmatic-export-prefs-v1', JSON.stringify(data.export_prefs));
  if (data.weekly_goal) localStorage.setItem(GOAL_KEY, String(data.weekly_goal));
  renderBrandKits(); updateStreakPill();
  app().note('Imported settings & Brand Kits! Refresh to apply all export defaults.');
 } catch (err) { app().note('Import failed: ' + err.message); }
};

// Clip Selection & Export Events
window.addEventListener('snapmatic:clip-selected', e => {
 const row = e.detail?.row;
 unexportedDirty = false;
 const ok = Boolean(row?.from_sources);
 for (const id of ['edit-rotate', 'edit-mirror', 'edit-zoom', 'edit-force-30fps', 'split-parts-go', 'export-zip-kit', 'cover-auto-btn', 'batch-queue-add']) {
  if ($(id)) $(id).disabled = !ok;
 }
 if ($('cover-candidates')) { $('cover-candidates').hidden = true; $('cover-candidates').replaceChildren(); }
 if ($('cover-design-apply')) $('cover-design-apply').disabled = !app().getCoverUrl();
 if ($('x-upload-text')) $('x-upload-text').value = app().xPostText();
 const sl = $('ig-thumb-slider');
 if (sl) { sl.disabled = !ok; sl.value = '0'; if ($('ig-thumb-val')) $('ig-thumb-val').textContent = '0.0s'; }
 renderClipRatingAndMarkers(row);
 renderUploadReadiness();
 applyLiveViewfinderFx();
 checkDuplicateHash(row);
});
window.addEventListener('snapmatic:exported', () => {
 unexportedDirty = false;
 renderUploadReadiness();
});
app()?.getVideo()?.addEventListener('loadedmetadata', () => {
 const v = app().getVideo(), sl = $('ig-thumb-slider');
 if (sl && Number.isFinite(v.duration) && v.duration > 0) sl.max = String(Math.floor(v.duration * 1000));
 renderUploadReadiness();
 renderClipRatingAndMarkers(app().getSelected());
});

renderBrandKits();
renderBatchUploadQueue();
updateStreakPill();

// 17. Clickable 59s YT & 89s IG Sweet-Spot Lines on #trim-track
function bindTrimLimitSnap(id, secs, label) {
 const el = $(id);
 if (!el) return;
 el.addEventListener('click', e => {
  e.stopPropagation();
  const es = app()?.getEditState();
  if (!es || !es.duration_ms) return;
  const targetOut = Math.min(es.duration_ms, (es.in_ms || 0) + secs * 1000);
  if (targetOut <= (es.in_ms || 0) + 500) return;
  app().setTrim('out', targetOut);
  app().workflowDirty();
  unexportedDirty = true;
  renderUploadReadiness();
  app().note('Snapped Out handle to ' + label + ' (' + app().time(targetOut / 1000) + ').');
 });
}
bindTrimLimitSnap('trim-limit-59', 59, '59s YouTube Short sweet-spot');
bindTrimLimitSnap('trim-limit-89', 89, '89s Instagram Reels sweet-spot');

// 18. Auto "Viral Moment" Audio Peak Finder (FFmpeg RMS Scan)
if ($('trim-viral-moment')) $('trim-viral-moment').onclick = async () => {
 const row = app().getSelected(), es = app().getEditState(), btn = $('trim-viral-moment'), out = $('silence-status');
 if (!row?.from_sources || !es || btn.disabled) return;
 btn.disabled = true;
 if (out) out.textContent = 'Scanning audio energy for hype/reaction peaks…';
 try {
  const r = await app().get('/api/workspace/film-roll/' + encodeURIComponent(row.clip_id) + '/viral-moment', { method: 'POST' });
  const nextIn = Math.max(0, Math.min(es.duration_ms - 500, Number(r.suggested_in_ms) || 0));
  const nextOut = Math.max(nextIn + 500, Math.min(es.duration_ms, Number(r.suggested_out_ms) || es.duration_ms));
  app().setTrim('in', nextIn);
  app().setTrim('out', nextOut);
  app().workflowDirty();
  unexportedDirty = true;
  renderUploadReadiness();
  const v = app().getVideo();
  if (v && Number.isFinite(v.duration) && Number.isFinite(Number(r.peak_at_ms))) {
   v.currentTime = Math.max(0, Math.min(v.duration, Number(r.peak_at_ms) / 1000));
  }
  const peaks = Array.isArray(r.top_peaks) ? r.top_peaks.slice(0, 3).map(p => app().time((Number(p.ms ?? p.at_ms) || 0) / 1000).slice(0, 5)).join(', ') : '';
  const msg = 'Viral window: ' + app().time(nextIn / 1000).slice(0, 5) + ' → ' + app().time(nextOut / 1000).slice(0, 5) + (peaks ? ' · Top peaks: ' + peaks : '');
  if (out) out.textContent = msg;
  app().note('⚡ ' + msg);
 } catch (e) {
  if (out) out.textContent = 'Viral moment scan failed: ' + e.message;
  app().note('Viral moment scan failed: ' + e.message);
 } finally {
  app().renderNvenc();
 }
};

// 19. 1-Click Local AI / Smart Auto-Captions Generator
if ($('edit-auto-captions')) $('edit-auto-captions').onclick = async () => {
 const row = app().getSelected(), es = app().getEditState(), btn = $('edit-auto-captions'), out = $('auto-captions-status');
 if (!row?.from_sources || btn.disabled) return;
 btn.disabled = true;
 if (out) out.textContent = 'Generating timed captions (checking local Whisper model)…';
 try {
  const seed = ($('edit-hook')?.value || '').trim() || ($('caption')?.value || '').trim() || ($('title')?.value || '').trim() || row.name;
  const r = await app().get('/api/workspace/film-roll/' + encodeURIComponent(row.clip_id) + '/auto-captions', {
   method: 'POST',
   headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify({
    seed_text: seed,
    in_ms: es?.in_ms || 0,
    out_ms: es?.out_ms || es?.duration_ms || null
   })
  });
  if ($('edit-srt-text')) $('edit-srt-text').value = r.srt || '';
  if ($('edit-srt-style')) $('edit-srt-style').value = 'vice_neon';
  unexportedDirty = true;
  renderUploadReadiness();
  const modeLabel = r.engine === 'whisper-local' ? 'Local Whisper AI' : 'Smart 3-word Vice bursts (drop whisper-cli.exe + ggml-*.bin in tools/ for speech-to-text)';
  if (out) out.textContent = '✓ Generated ' + (r.cue_count || 0) + ' cues via ' + modeLabel + '.';
  app().note('Auto-Captions ready (' + (r.cue_count || 0) + ' cues · GTA 6 Vice Neon Pop style).');
 } catch (e) {
  if (out) out.textContent = 'Auto-captions failed: ' + e.message;
  app().note('Auto-captions failed: ' + e.message);
 } finally {
  app().renderNvenc();
 }
};

// 20. Drag-and-Drop Local Video Intake Straight Into the Viewfinder (+ Drop MP4 button)
async function importLocalVideoFile(file) {
 if (!file) return;
 const ext = (file.name.split('.').pop() || '').toLowerCase();
 if (!['mp4', 'mov', 'mkv', 'webm'].includes(ext)) {
  app().note('Supported video formats: .mp4, .mov, .mkv, .webm');
  return;
 }
 app().note('Importing "' + file.name + '" into Film Roll…');
 try {
  const fd = new FormData();
  fd.append('file', file, file.name);
  const resp = await fetch('/api/workspace/film-roll/import-file', {
   method: 'POST',
   credentials: 'same-origin',
   body: fd
  });
  if (!resp.ok) {
   const txt = (await resp.text()).trim();
   let msg = txt || 'Import failed.';
   try { const j = JSON.parse(txt); msg = j.reason || j.error || msg; } catch {}
   throw Error(msg);
  }
  const r = await resp.json();
  await app().reload();
  app().refreshLocalStatus();
  const hit = app().getItems().find(x => x.clip_id === r.clip_id);
  if (hit) {
   app().showView('studio', true);
   app().select(hit);
  }
  app().note('Imported "' + (r.title || file.name) + '" (' + ((r.bytes || 0) / 1048576).toFixed(1) + ' MiB) into Film Roll!');
 } catch (e) {
  app().note('Video import failed: ' + e.message);
 }
}

if ($('roll-drop-input')) {
 $('roll-drop-input').onchange = e => {
  const f = e.currentTarget.files?.[0];
  e.currentTarget.value = '';
  if (f) importLocalVideoFile(f);
 };
}

const dropZone = $('vf-drop-zone'), dropOverlay = $('vf-drop-overlay');
if (dropZone) {
 let dragDepth = 0;
 const setDragging = on => {
  dropZone.classList.toggle('drag-over', on);
  if (dropOverlay) dropOverlay.hidden = !on;
 };
 dropZone.addEventListener('dragenter', e => {
  if (!e.dataTransfer?.types?.includes('Files')) return;
  e.preventDefault();
  dragDepth++;
  setDragging(true);
 });
 dropZone.addEventListener('dragover', e => {
  if (!e.dataTransfer?.types?.includes('Files')) return;
  e.preventDefault();
  e.dataTransfer.dropEffect = 'copy';
 });
 dropZone.addEventListener('dragleave', () => {
  dragDepth = Math.max(0, dragDepth - 1);
  if (dragDepth === 0) setDragging(false);
 });
 dropZone.addEventListener('drop', e => {
  e.preventDefault();
  dragDepth = 0;
  setDragging(false);
  const f = e.dataTransfer?.files?.[0];
  if (f) importLocalVideoFile(f);
 });
}

// 21. 1-Click "Ship Recipe" Macro (Auto-Format -> Auto-Export -> Multi-Ship)
if ($('ship-recipe-go')) $('ship-recipe-go').onclick = async () => {
 const row = app().getSelected(), btn = $('ship-recipe-go'), st = $('ship-recipe-status');
 if (!row?.from_sources || btn.disabled) return;
 btn.disabled = true;
 try {
  if ($('recipe-autoformat')?.checked) {
   if (st) st.textContent = '1/3 Auto-formatting captions, #Shorts & X post…';
   $('auto-format-all')?.click();
  }
  const needExport = Boolean($('recipe-autoexport')?.checked) && (!row.exported || unexportedDirty);
  if (needExport) {
   const v = app().getVideo();
   if (v && v.videoWidth > v.videoHeight && $('edit-framing')?.value === 'original') {
    $('edit-framing').value = 'center_crop';
    $('edit-framing').dispatchEvent(new Event('change'));
   }
   if (st) st.textContent = '2/3 Exporting optimized 9:16 H.264 MP4…';
   const exportedOk = await new Promise(resolve => {
    let settled = false;
    const onDone = ev => {
     if (ev.detail?.clip_id === row.clip_id && !settled) { settled = true; cleanup(); resolve(true); }
    };
    const pollBtn = setInterval(() => {
     const b = $('edit-transcode');
     if (!settled && b && !b.disabled && ($('edit-state')?.textContent || '').startsWith('Export failed')) {
      settled = true; cleanup(); resolve(false);
     }
    }, 500);
    const timeout = setTimeout(() => { if (!settled) { settled = true; cleanup(); resolve(false); } }, 240000);
    function cleanup() { window.removeEventListener('snapmatic:exported', onDone); clearInterval(pollBtn); clearTimeout(timeout); }
    window.addEventListener('snapmatic:exported', onDone);
    $('edit-transcode')?.click();
   });
   if (!exportedOk) {
    if (st) st.textContent = 'Export did not finish — review Edit & Export tab before shipping.';
    return;
   }
  }
  if (st) st.textContent = '3/3 Shipping to selected platforms…';
  const res = await app().runMultiShip();
  if (st) {
   if (res?.cancelled) st.textContent = 'Ship cancelled before live publish.';
   else st.textContent = '✓ Ship Recipe done: ' + (res?.okCount || 0) + '/' + (res?.total || 0) + ' platform(s) succeeded.';
  }
 } finally {
  app().syncYoutubeDraftButton();
 }
};

// 22. Smart Growth Coach in Channel Insights
function renderGrowthCoach(data) {
 const body = $('insights-coach-body');
 if (!body) return;
 const posts = Array.isArray(data?.posts) ? data.posts : [];
 if (!posts.length) {
  body.replaceChildren(
   app().node('div', 'Sync live YouTube, Instagram & TikTok posts to unlock personalized best-time, hashtag, and engagement coaching.', 'muted')
  );
  return;
 }
 const dayNames = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
 const slotMap = new Map();
 const tagMap = new Map();
 const platMap = new Map();
 let viewsSum = 0, engNum = 0, engDen = 0;

 for (const p of posts) {
  const views = Number(p.views) || 0;
  const likes = Number(p.likes) || 0;
  const comments = Number(p.comments) || 0;
  const score = views + likes * 12 + comments * 25;
  viewsSum += views;
  if (views > 0) { engNum += (likes + comments); engDen += views; }

  const pCur = platMap.get(p.provider) || { provider: p.provider, count: 0, views: 0, likes: 0 };
  pCur.count++; pCur.views += views; pCur.likes += likes;
  platMap.set(p.provider, pCur);

  if (p.published_at) {
   const dt = new Date(p.published_at);
   if (!Number.isNaN(dt.getTime())) {
    const slotKey = dayNames[dt.getDay()] + ' @ ' + dt.toLocaleTimeString([], { hour: 'numeric' });
    const sCur = slotMap.get(slotKey) || { key: slotKey, count: 0, score: 0 };
    sCur.count++; sCur.score += score;
    slotMap.set(slotKey, sCur);
   }
  }
  const tags = ((p.title || '') + ' ' + (p.caption || '')).match(/#[a-zA-Z0-9_]+/g) || [];
  const seen = new Set();
  for (const rawTag of tags) {
   const k = rawTag.toLowerCase();
   if (seen.has(k)) continue;
   seen.add(k);
   const tCur = tagMap.get(k) || { tag: k, count: 0, views: 0, score: 0 };
   tCur.count++; tCur.views += views; tCur.score += score;
   tagMap.set(k, tCur);
  }
 }

 const bestSlot = Array.from(slotMap.values()).sort((a, b) => (b.score / b.count) - (a.score / a.count))[0];
 const bestTags = Array.from(tagMap.values()).sort((a, b) => (b.score / b.count) - (a.score / a.count)).slice(0, 3);
 const leaderPlat = Array.from(platMap.values()).sort((a, b) => (b.views / Math.max(1, b.count)) - (a.views / Math.max(1, a.count)))[0];
 const engRate = engDen > 0 ? ((engNum / engDen) * 100).toFixed(2) : null;
 const avgViews = Math.round(viewsSum / posts.length);

 const tip = (eyebrow, headline, detail) => {
  const card = app().node('div', undefined, 'coach-tip');
  card.append(app().node('small', eyebrow), app().node('strong', headline), app().node('p', detail));
  return card;
 };

 const platPretty = { youtube: 'YouTube Shorts', instagram: 'Instagram Reels', tiktok: 'TikTok' };
 body.replaceChildren(
  tip(
   'Peak Posting Window',
   bestSlot ? bestSlot.key : 'Evenings (6 PM)',
   bestSlot ? 'Highest average engagement score across your synced drops (' + bestSlot.count + ' post' + (bestSlot.count === 1 ? '' : 's') + ' in this slot).' : 'Schedule more drops with timestamps to lock in your peak hour.'
  ),
  tip(
   'Winning Hashtags',
   bestTags.length ? bestTags.map(t => t.tag).join(' ') : '#gta6 #gtavi #rockstargames',
   bestTags.length ? 'Top tags by avg reach (' + Math.round(bestTags[0].views / bestTags[0].count).toLocaleString() + ' avg views on ' + bestTags[0].tag + ').' : 'Pair 3–5 focused GTA 6 tags in Studio Captions to track tag lift.'
  ),
  tip(
   'Engagement & Reach',
   engRate ? engRate + '% eng · ' + avgViews.toLocaleString() + ' avg views' : avgViews.toLocaleString() + ' avg views',
   leaderPlat ? (platPretty[leaderPlat.provider] || leaderPlat.provider) + ' leads your per-post reach (' + Math.round(leaderPlat.views / leaderPlat.count).toLocaleString() + ' views/post).' : 'Cross-post every clip to Shorts, Reels & TikTok.'
  ),
  tip(
   'Next GTA 6 Drop Playbook',
   'Keep Shorts ≤59s · Reels ≤89s',
   'Use ⚡ Find Viral Moment + Auto-Captions (Vice Neon Pop) in Studio, then hit 🔥 Run Ship Recipe to auto-format, export 9:16, and ship in 1 click.'
  )
 );
}
window.addEventListener('snapmatic:insights-rendered', e => renderGrowthCoach(e.detail?.data));
if (app()?.getAnalyticsData?.()) renderGrowthCoach(app().getAnalyticsData());
})();
