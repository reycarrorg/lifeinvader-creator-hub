// Renders a compact floating Snapmatic status pill inside TikTok Messages and
// asks the background worker to run the MAIN-world scanner whenever messages change.
(() => {
  if (window.top !== window || window.__snapmaticDmContentLoaded) return;
  window.__snapmaticDmContentLoaded = true;

  let statusEl = null;
  let debounceTimer = null;

  const pill = document.createElement('div');
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
  btn.onclick = () => requestBgSync(true);

  pill.append(statusEl, btn);
  document.documentElement.appendChild(pill);

  function requestBgSync(manual) {
    if (manual) statusEl.textContent = '⚡ Scanning chat…';
    chrome.runtime.sendMessage({ type: manual ? 'sync_dm' : 'sync_dm_auto' }, reply => {
      if (chrome.runtime.lastError) return;
      if (reply?.status) statusEl.textContent = '⚡ ' + reply.status;
    });
  }

  function scheduleScan() {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => requestBgSync(false), 2500);
  }

  setTimeout(() => requestBgSync(false), 2500);
  setInterval(() => requestBgSync(false), 30000);

  const obs = new MutationObserver(scheduleScan);
  if (document.body) obs.observe(document.body, { childList: true, subtree: true });
})();
