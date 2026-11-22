const status = document.getElementById('status');
const dmStatus = document.getElementById('dm-status');
const dmEnabled = document.getElementById('dm-enabled');
const dmGroup = document.getElementById('dm-group');

chrome.storage.local.get(['token', 'dm_enabled', 'dm_group_name', 'dm_last_status']).then(x => {
  status.textContent = x.token ? 'Connected on this browser.' : 'Not connected.';
  dmEnabled.checked = x.dm_enabled !== false;
  dmGroup.value = x.dm_group_name || "cj_4_prez's group";
  if (x.dm_last_status) dmStatus.textContent = x.dm_last_status;
});

dmEnabled.onchange = () => chrome.storage.local.set({ dm_enabled: dmEnabled.checked });
dmGroup.onchange = () => chrome.storage.local.set({ dm_group_name: dmGroup.value.trim() || "cj_4_prez's group" });

document.getElementById('pin-dm').onclick = () => {
  chrome.runtime.sendMessage({ type: 'pin_dm' }, () => {
    dmStatus.textContent = "Pinned cj_4_prez's group tab in Brave!";
  });
};

document.getElementById('sync-dm').onclick = () => {
  dmStatus.textContent = 'Scanning pinned Group DM tab…';
  chrome.storage.local.set({ dm_group_name: dmGroup.value.trim() || "cj_4_prez's group" });
  chrome.runtime.sendMessage({ type: 'sync_dm' }, res => {
    dmStatus.textContent = res?.status || 'Checked Group DM.';
  });
};

document.getElementById('save').onclick = async () => {
  const value = document.getElementById('token').value.trim();
  if (!/^[0-9a-f]{64}$/.test(value)) { status.textContent = 'Enter the 64-character code from Snapmatic.'; return; }
  try {
    const r = await fetch('http://127.0.0.1:3847/api/bridge/check', { headers: { 'x-lifeinvader-token': value } });
    if (!r.ok) throw Error();
    await chrome.storage.local.set({ token: value });
    document.getElementById('token').value = '';
    status.textContent = 'Connected.';
    chrome.runtime.sendMessage({ type: 'poll' });
  } catch { status.textContent = 'Could not connect. Open Snapmatic and check the code.'; }
};

document.getElementById('forget').onclick = async () => {
  await chrome.storage.local.remove('token');
  document.getElementById('token').value = '';
  status.textContent = 'Disconnected.';
};
