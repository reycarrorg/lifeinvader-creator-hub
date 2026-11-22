importScripts('collector.js', 'group_dm_collector.js');

const BASE_BRIDGE = 'http://127.0.0.1:3847/api/bridge';
const GROUP_DM_URL = 'https://www.tiktok.com/business-suite/messages?lang=en&from=homepage';
const DEFAULT_GROUP_NAME = "cj_4_prez's group";

let working = false;
let dmWorking = false;

async function wait(ms) { return new Promise(resolve => setTimeout(resolve, ms)); }

async function runProfileBridge() {
  if (working) return;
  working = true;
  let tabId = null;
  try {
    const { token } = await chrome.storage.local.get('token');
    if (!token) return;
    const headers = { 'x-lifeinvader-token': token };
    const resp = await fetch(BASE_BRIDGE + '/poll', { headers });
    if (!resp.ok) return;
    const { job } = await resp.json();
    if (!job) return;
    const parsed = new URL(job.url);
    if (!['https:'].includes(parsed.protocol) || !['www.instagram.com', 'instagram.com', 'www.tiktok.com', 'tiktok.com'].includes(parsed.hostname)) return;
    const tab = await chrome.tabs.create({ url: job.url, active: false });
    tabId = tab.id;
    let result = { error: 'timeout' };
    for (let i = 0; i < 20; i++) {
      await wait(1500);
      try {
        const info = await chrome.tabs.get(tabId);
        if (info.status !== 'complete') continue;
        const [out] = await chrome.scripting.executeScript({ target: { tabId }, func: readProfile, args: [job.url, job.platform] });
        result = out?.result || { error: 'unavailable' };
        if (!result.error || result.error === 'identity' || result.error === 'login') break;
      } catch {}
    }
    await fetch(BASE_BRIDGE + '/result', {
      method: 'POST',
      headers: { ...headers, 'content-type': 'application/json' },
      body: JSON.stringify({ profile_id: job.profile_id, job_id: job.id, ...result }),
    });
  } catch {} finally {
    if (tabId !== null) try { await chrome.tabs.remove(tabId); } catch {}
    working = false;
  }
}

async function findMessageTabs() {
  const [biz, std] = await Promise.all([
    chrome.tabs.query({ url: 'https://www.tiktok.com/business-suite/messages*' }),
    chrome.tabs.query({ url: 'https://www.tiktok.com/messages*' }),
  ]);
  const byId = new Map();
  for (const t of [...biz, ...std]) byId.set(t.id, t);
  return Array.from(byId.values());
}

async function ensurePinnedGroupDmTab(activate = false) {
  const tabs = await findMessageTabs();
  if (tabs.length) {
    const t = tabs[0];
    await chrome.tabs.update(t.id, { pinned: true, ...(activate ? { active: true } : {}) });
    return t;
  }
  return await chrome.tabs.create({ url: GROUP_DM_URL, pinned: true, active: activate });
}

async function submitDiscoveredLinks(links, groupName, manual = false) {
  const cfg = await chrome.storage.local.get(['dm_seen_links']);
  const seen = new Set(Array.isArray(cfg.dm_seen_links) ? cfg.dm_seen_links : []);
  const unique = Array.from(new Set(links || []));
  const fresh = unique.filter(u => !seen.has(u));
  if (!fresh.length) {
    const msg = `Synced ${groupName} (${unique.length} post${unique.length === 1 ? '' : 's'} visible · up to date).`;
    await chrome.storage.local.set({ dm_last_status: msg });
    return { status: msg, sent: 0 };
  }
  try {
    const resp = await fetch(BASE_BRIDGE + '/group-dm', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ links: fresh, mode: 'both' }),
    });
    if (resp.status === 409) {
      const msg = 'Snapmatic is busy syncing another source — retrying shortly.';
      await chrome.storage.local.set({ dm_last_status: msg });
      return { status: msg };
    }
    if (!resp.ok) {
      const txt = (await resp.text()).slice(0, 140);
      const msg = `Snapmatic returned HTTP ${resp.status}: ${txt}`;
      await chrome.storage.local.set({ dm_last_status: msg });
      return { status: msg };
    }
    for (const u of fresh) seen.add(u);
    const trimmedSeen = Array.from(seen).slice(-2000);
    const msg = `Downloading ${fresh.length} new post${fresh.length === 1 ? '' : 's'} → Film Roll!`;
    await chrome.storage.local.set({ dm_seen_links: trimmedSeen, dm_last_status: msg });
    return { status: msg, sent: fresh.length };
  } catch {
    const msg = 'Open Snapmatic app (127.0.0.1:3847) to download.';
    await chrome.storage.local.set({ dm_last_status: msg });
    return { status: msg };
  }
}

async function runGroupDmSync(manual = false) {
  if (dmWorking) return { status: 'Already syncing Group DM…' };
  dmWorking = true;
  try {
    const cfg = await chrome.storage.local.get(['dm_enabled', 'dm_group_name']);
    const enabled = cfg.dm_enabled !== false;
    if (!enabled && !manual) return { status: 'Group DM Auto-Sync is paused.' };
    const groupName = (cfg.dm_group_name || DEFAULT_GROUP_NAME).trim() || DEFAULT_GROUP_NAME;

    const tabs = await findMessageTabs();
    if (!tabs.length) {
      const msg = "Open or pin the cj_4_prez's group tab in Brave to auto-sync.";
      await chrome.storage.local.set({ dm_last_status: msg });
      return { status: msg };
    }

    const tab = tabs[0];
    if (tab.status !== 'complete') return { status: 'Waiting for TikTok Messages tab to finish loading…' };

    const results = await chrome.scripting.executeScript({
      target: { tabId: tab.id, allFrames: true },
      func: scanGroupDmLinks,
      args: [groupName, manual],
    });

    const allLinks = new Set();
    let anyMatched = false;
    let sawLogin = false;
    let sampleText = '';

    for (const r of results || []) {
      const scan = r?.result;
      if (!scan || scan.empty) continue;
      if (scan.ok && scan.matched) anyMatched = true;
      if (scan.error === 'login') sawLogin = true;
      if (scan.sample && !sampleText) sampleText = scan.sample;
      for (const u of (scan.links || [])) allLinks.add(u);
    }

    if (!anyMatched && sawLogin) {
      const msg = 'Sign into TikTok in your pinned Brave tab to sync.';
      await chrome.storage.local.set({ dm_last_status: msg });
      return { status: msg };
    }
    if (!anyMatched) {
      const msg = `Click "${groupName}" inside your pinned TikTok Messages tab.` + (sampleText ? ` (Saw: ${sampleText.slice(0, 60)})` : '');
      await chrome.storage.local.set({ dm_last_status: msg });
      return { status: msg };
    }

    return await submitDiscoveredLinks(Array.from(allLinks), groupName, manual);
  } catch {
    const msg = 'Snapmatic app not running on 127.0.0.1:3847 (open Snapmatic to sync).';
    await chrome.storage.local.set({ dm_last_status: msg });
    return { status: msg };
  } finally {
    dmWorking = false;
  }
}

chrome.runtime.onInstalled.addListener(() => {
  chrome.alarms.create('refresh', { periodInMinutes: 1 });
  chrome.alarms.create('group_dm', { periodInMinutes: 1 });
  ensurePinnedGroupDmTab(false).catch(() => {});
  runProfileBridge();
  setTimeout(() => runGroupDmSync(false), 3000);
});

chrome.runtime.onStartup.addListener(() => {
  chrome.alarms.create('refresh', { periodInMinutes: 1 });
  chrome.alarms.create('group_dm', { periodInMinutes: 1 });
  runProfileBridge();
  setTimeout(() => runGroupDmSync(false), 4000);
});

chrome.alarms.onAlarm.addListener(a => {
  if (a.name === 'refresh') runProfileBridge();
  if (a.name === 'group_dm') runGroupDmSync(false);
});

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg?.type === 'poll') { runProfileBridge(); sendResponse({ ok: true }); }
  if (msg?.type === 'sync_dm') { runGroupDmSync(true).then(sendResponse); return true; }
  if (msg?.type === 'pin_dm') { ensurePinnedGroupDmTab(true).then(() => sendResponse({ ok: true })); return true; }
  if (msg?.type === 'dm_links_found') {
    submitDiscoveredLinks(msg.links || [], msg.group || DEFAULT_GROUP_NAME, Boolean(msg.manual)).then(sendResponse);
    return true;
  }
});
