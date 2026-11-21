'use strict';
(() => {
const $ = id => document.getElementById(id);
const app = () => window.snapmaticApp;

let unexportedDirty = false;
let uploadPollTimer = null;

// 1. On-Demand Live Upload Progress Poller (Zero idle polling!)
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
  } else {
   stopUploadPoll();
  }
 }).catch(() => {});
}
function startUploadPoll() {
 if (uploadPollTimer) return;
 pollUploadProgress();
 uploadPollTimer = setInterval(() => {
  if (!document.hidden) pollUploadProgress();
 }, 750);
}
function stopUploadPoll() {
 if (uploadPollTimer) { clearInterval(uploadPollTimer); uploadPollTimer = null; }
}
// One-time check on load in case an upload was already running
setTimeout(pollUploadProgress, 800);

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

// 3. Upload Readiness + Auto-Fix / <=59s Short / <=89s Reel Quick Export
function renderUploadReadiness() {
 const grid = $('upload-readiness'), warn = $('upload-unexported-warn'), fixBtn = $('upload-autofix-btn'), shortsBtn = $('upload-shorts-cut-btn'), reelsBtn = $('upload-reels-cut-btn');
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
 const vertical = h > w || Boolean(row.exported);
 const facts = app().getMediaFacts()?.[row.preview_sha256];
 const vCodec = (facts?.streams?.find(s => s.codec_type === 'video')?.codec_name || '').toLowerCase();
 const h264Ok = row.exported || !vCodec || vCodec === 'h264';
 const sizeMiB = (row.preview_bytes || 0) / 1048576;

 const checks = [
  { id: 'youtube', name: 'YouTube Shorts', ok: durS <= 60 && vertical && sizeMiB <= 250, warn: durS > 60 && durS <= 180 && vertical && sizeMiB <= 250, msg: durS > 180 ? 'Over 180s (regular video)' : !vertical ? 'Landscape — Export MP4 auto-crops 9:16' : durS > 60 ? '3-min Short (' + durS.toFixed(0) + 's) — trim ≤59s for guaranteed Shorts' : 'Guaranteed Short (' + durS.toFixed(1) + 's · 9:16)' },
  { id: 'instagram', name: 'Instagram Reels', ok: durS <= 90 && vertical && h264Ok, warn: durS > 90 && durS <= 180, msg: !h264Ok ? 'Codec ' + vCodec + ' — auto-optimized on export' : durS > 90 ? 'Over 90s (' + durS.toFixed(0) + 's) — recommend ≤89s' : !vertical ? 'Export MP4 auto-crops 9:16' : 'Ready (' + durS.toFixed(1) + 's · H.264)' },
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
}
if ($('upload-autofix-btn')) $('upload-autofix-btn').onclick = () => $('edit-transcode')?.click();
if ($('upload-shorts-cut-btn')) $('upload-shorts-cut-btn').onclick = () => {
 const row = app().getSelected(), es = app().getEditState();
 if (!row?.from_sources || !es) return;
 app().setTrim('out', Math.min(es.duration_ms, (es.in_ms || 0) + 59000));
 app().note('Trimmed clip to guaranteed ≤59s YouTube Short and starting export…');
 $('edit-transcode')?.click();
};
if ($('upload-reels-cut-btn')) $('upload-reels-cut-btn').onclick = () => {
 const row = app().getSelected(), es = app().getEditState();
 if (!row?.from_sources || !es) return;
 app().setTrim('out', Math.min(es.duration_ms, (es.in_ms || 0) + 89000));
 app().note('Trimmed clip to Reels-safe 89s and starting export…');
 $('edit-transcode')?.click();
};

// 4. Duplicate SHA-256 Guard + Cloud Upload Receipts Table
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

// 5. Speed-Triage Keyboard Shortcuts (A approve, H hold, N/P next/prev)
document.addEventListener('keydown', e => {
 if (e.ctrlKey || e.metaKey || e.altKey || e.repeat) return;
 if (e.target.closest('input,textarea,select,[contenteditable]') || document.querySelector('dialog[open]')) return;
 const row = app()?.getSelected();
 if ($('studio')?.hidden) return;
 const k = e.key.toLowerCase();
 if (k === 'a' && row?.from_sources && !$('approve')?.disabled) { e.preventDefault(); $('approve').click(); return; }
 if (k === 'h' && row?.from_sources && !$('hold-clip')?.disabled) { e.preventDefault(); $('hold-clip').click(); return; }
 if (k === 'n' || k === 'p') {
  const vis = app().getVisibleItems();
  if (!vis.length) return;
  const idx = vis.findIndex(x => x.clip_id === row?.clip_id);
  const next = vis[k === 'n' ? Math.min(vis.length - 1, idx + 1) : Math.max(0, idx - 1)];
  if (next && next.clip_id !== row?.clip_id) { e.preventDefault(); app().select(next); }
 }
});

// 6. Brand Kits & Settings Export / Import (.json, zero credentials)
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
 kits.push({ name, tags: $('tags')?.value || '' });
 writeKits(kits);
 if ($('brand-kit-name')) $('brand-kit-name').value = '';
 renderBrandKits();
 if ($('brand-kit-msg')) $('brand-kit-msg').textContent = 'Saved Brand Kit "' + name + '".';
 app().note('Saved Brand Kit "' + name + '".');
};
if ($('brand-kit-apply')) $('brand-kit-apply').onclick = () => {
 const kit = readKits()[Number($('brand-kit-select')?.value)];
 if (!kit) return;
 if (kit.tags && $('tags')) { $('tags').value = kit.tags; app().refreshDraftEditor(); app().workflowDirty(); }
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
  if (data.weekly_goal) localStorage.setItem(GOAL_KEY, String(data.weekly_goal));
  renderBrandKits(); updateStreakPill();
  app().note('Imported settings & Brand Kits!');
 } catch (err) { app().note('Import failed: ' + err.message); }
};

// 7. Clickable 59s YT & 89s IG Sweet-Spot Lines on #trim-track
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

// 8. Drag-and-Drop Local Video Intake Into Viewfinder (+ Drop MP4 button)
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
  const resp = await fetch('/api/workspace/film-roll/import-file', { method: 'POST', credentials: 'same-origin', body: fd });
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
  if (hit) { app().showView('studio', true); app().select(hit); }
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
 dropZone.addEventListener('dragenter', e => { if (!e.dataTransfer?.types?.includes('Files')) return; e.preventDefault(); dragDepth++; setDragging(true); });
 dropZone.addEventListener('dragover', e => { if (!e.dataTransfer?.types?.includes('Files')) return; e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; });
 dropZone.addEventListener('dragleave', () => { dragDepth = Math.max(0, dragDepth - 1); if (dragDepth === 0) setDragging(false); });
 dropZone.addEventListener('drop', e => { e.preventDefault(); dragDepth = 0; setDragging(false); const f = e.dataTransfer?.files?.[0]; if (f) importLocalVideoFile(f); });
}

// 9. 1-Click Ship Recipe Macro (Auto-Format -> Auto-Export -> Multi-Ship)
if ($('ship-recipe-go')) $('ship-recipe-go').onclick = async () => {
 const row = app().getSelected(), btn = $('ship-recipe-go'), st = $('ship-recipe-status');
 if (!row?.from_sources || btn.disabled) return;
 btn.disabled = true;
 try {
  if ($('recipe-autoformat')?.checked) {
   if (st) st.textContent = '1/3 Auto-formatting title & hashtags…';
   const rawTitle = ($('title')?.value || row.name || 'GTA 6 Clip').trim().replace(/\s+/g, ' ').slice(0, 92);
   if ($('title')) $('title').value = rawTitle;
   if ($('tags') && !$('tags').value.trim()) $('tags').value = '#GTA6 #GTAVI #RockstarGames #Shorts';
   app().refreshDraftEditor();
   app().workflowDirty();
  }
  const needExport = Boolean($('recipe-autoexport')?.checked) && (!row.exported || unexportedDirty);
  if (needExport) {
   if (st) st.textContent = '2/3 Exporting optimized 9:16 H.264 MP4…';
   const exportedOk = await new Promise(resolve => {
    let settled = false;
    const onDone = ev => { if (ev.detail?.clip_id === row.clip_id && !settled) { settled = true; cleanup(); resolve(true); } };
    const pollBtn = setInterval(() => {
     const b = $('edit-transcode');
     if (!settled && b && !b.disabled && ($('edit-state')?.textContent || '').startsWith('Export failed')) { settled = true; cleanup(); resolve(false); }
    }, 500);
    const timeout = setTimeout(() => { if (!settled) { settled = true; cleanup(); resolve(false); } }, 240000);
    function cleanup() { window.removeEventListener('snapmatic:exported', onDone); clearInterval(pollBtn); clearTimeout(timeout); }
    window.addEventListener('snapmatic:exported', onDone);
    $('edit-transcode')?.click();
   });
   if (!exportedOk) {
    if (st) st.textContent = 'Export did not finish — check Export MP4 bar below.';
    return;
   }
  }
  if (st) st.textContent = '3/3 Shipping to selected platforms…';
  startUploadPoll();
  const res = await app().runMultiShip();
  if (st) {
   if (res?.cancelled) st.textContent = 'Ship cancelled before live publish.';
   else st.textContent = '✓ Ship Recipe done: ' + (res?.okCount || 0) + '/' + (res?.total || 0) + ' platform(s) succeeded.';
  }
 } finally {
  stopUploadPoll();
  app().syncYoutubeDraftButton();
 }
};

// 10. Smart Growth Coach in Channel Insights
function renderGrowthCoach(data) {
 const body = $('insights-coach-body');
 if (!body) return;
 const posts = Array.isArray(data?.posts) ? data.posts : [];
 if (!posts.length) {
  body.replaceChildren(app().node('div', 'Sync live YouTube, Instagram & TikTok posts to unlock personalized best-time, hashtag, and engagement coaching.', 'muted'));
  return;
 }
 const dayNames = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
 const slotMap = new Map(), tagMap = new Map(), platMap = new Map();
 let viewsSum = 0, engNum = 0, engDen = 0;

 for (const p of posts) {
  const views = Number(p.views) || 0, likes = Number(p.likes) || 0, comments = Number(p.comments) || 0;
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
  tip('Peak Posting Window', bestSlot ? bestSlot.key : 'Evenings (6 PM)', bestSlot ? 'Highest average engagement across your synced drops (' + bestSlot.count + ' post' + (bestSlot.count === 1 ? '' : 's') + ' in this slot).' : 'Schedule more drops to lock in your peak hour.'),
  tip('Winning Hashtags', bestTags.length ? bestTags.map(t => t.tag).join(' ') : '#gta6 #gtavi #rockstargames', bestTags.length ? 'Top tags by avg reach (' + Math.round(bestTags[0].views / bestTags[0].count).toLocaleString() + ' avg views on ' + bestTags[0].tag + ').' : 'Pair 3–5 focused tags in Captions to track tag lift.'),
  tip('Engagement & Reach', engRate ? engRate + '% eng · ' + avgViews.toLocaleString() + ' avg views' : avgViews.toLocaleString() + ' avg views', leaderPlat ? (platPretty[leaderPlat.provider] || leaderPlat.provider) + ' leads your per-post reach (' + Math.round(leaderPlat.views / leaderPlat.count).toLocaleString() + ' views/post).' : 'Cross-post every clip to Shorts, Reels & TikTok.'),
  tip('Next Drop Playbook', 'Keep Shorts ≤59s · Reels ≤89s', 'Generate DTW AutoCaptions in Create ✨, pick a CTR hook in Captions, then hit 🔥 Ship Drop in Upload.')
 );
}

window.addEventListener('snapmatic:clip-selected', e => {
 unexportedDirty = false;
 renderUploadReadiness();
 checkDuplicateHash(e.detail?.row);
});
window.addEventListener('snapmatic:exported', () => {
 unexportedDirty = false;
 renderUploadReadiness();
});
app()?.getVideo()?.addEventListener('loadedmetadata', renderUploadReadiness);
window.addEventListener('snapmatic:insights-rendered', e => renderGrowthCoach(e.detail?.data));
if (app()?.getAnalyticsData?.()) renderGrowthCoach(app().getAnalyticsData());

window.snapmaticPro = { startUploadPoll, stopUploadPoll, renderUploadReadiness };

renderBrandKits();
updateStreakPill();
})();
