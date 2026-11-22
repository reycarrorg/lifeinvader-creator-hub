// Runs automatically inside TikTok Business Suite Messages / TikTok Messages.
// Watches for shared posts in real time and renders a compact floating Snapmatic status pill.
(() => {
  if (window.__snapmaticDmContentLoaded) return;
  window.__snapmaticDmContentLoaded = true;

  let pill = null;
  let statusEl = null;
  let debounceTimer = null;
  let lastCount = -1;

  function ensurePill() {
    if (window.top !== window || pill) return;
    pill = document.createElement('div');
    pill.id = 'snapmatic-dm-pill';
    pill.style.cssText = [
      'position:fixed', 'right:16px', 'bottom:16px', 'z-index:2147483647',
      'display:flex', 'align-items:center', 'gap:8px',
      'padding:8px 12px', 'border-radius:999px',
      'background:rgba(15,12,30,0.94)', 'color:#f5f3ff',
      'border:1px solid rgba(0,229,255,0.55)',
      'box-shadow:0 8px 24px rgba(0,0,0,0.45)',
      'font:600 12px system-ui,Segoe UI,sans-serif',
      'backdrop-filter:blur(8px)',
    ].join(';');

    statusEl = document.createElement('span');
    statusEl.textContent = '⚡ Snapmatic Auto-Sync ON';

    const btn = document.createElement('button');
    btn.type = 'button';
    btn.textContent = '⬇ Sync Chat Now';
    btn.style.cssText = [
      'border:0', 'border-radius:999px', 'padding:5px 10px', 'cursor:pointer',
      'background:linear-gradient(135deg,#ff2e93,#00e5ff)', 'color:#fff',
      'font:700 11px system-ui,Segoe UI,sans-serif',
    ].join(';');
    btn.onclick = () => triggerSync(true);

    pill.append(statusEl, btn);
    document.documentElement.appendChild(pill);
  }

  function updatePill(text) {
    if (statusEl && text) statusEl.textContent = '⚡ ' + text;
  }

  async function triggerSync(forceAnyOpen = false) {
    try {
      const cfg = await chrome.storage.local.get(['dm_enabled', 'dm_group_name']);
      if (cfg.dm_enabled === false && !forceAnyOpen) return;
      const groupName = (cfg.dm_group_name || "cj_4_prez's group").trim();
      const res = scanGroupDmLinks(groupName, forceAnyOpen);
      if (res?.ok && Array.isArray(res.links) && res.links.length > 0) {
        if (forceAnyOpen || res.links.length !== lastCount) {
          lastCount = res.links.length;
          chrome.runtime.sendMessage(
            { type: 'dm_links_found', links: res.links, group: groupName, manual: forceAnyOpen },
            reply => {
              if (chrome.runtime.lastError) return;
              if (reply?.status) updatePill(reply.status);
            }
          );
        }
      } else if (forceAnyOpen) {
        updatePill(res?.error === 'login' ? 'Sign into TikTok first' : 'No shared post links found in this view (scroll up to load older posts)');
      }
    } catch {}
  }

  function scheduleScan() {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => triggerSync(false), 1800);
  }

  ensurePill();
  setTimeout(() => triggerSync(false), 2500);
  setInterval(() => triggerSync(false), 25000);

  const obs = new MutationObserver(scheduleScan);
  if (document.body) obs.observe(document.body, { childList: true, subtree: true });
})();
