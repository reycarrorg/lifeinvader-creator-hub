// Scans visible post links & TikTok shared-video cards inside the open Group DM thread.
// Supports main frame, nested iframes, curly quotes, and React IM message JSON payloads.
function scanGroupDmLinks(targetGroupName, allowAnyOpenChat = false) {
  const norm = s => String(s || '').toLowerCase().replace(/[\u2018\u2019\u2032`]/g, "'").replace(/\s+/g, ' ').trim();
  const compact = s => norm(s).replace(/[^a-z0-9]/g, '');
  const rawNeedle = norm(targetGroupName || "cj_4_prez's group");
  const coreToken = rawNeedle.split(/[\s']/)[0] || rawNeedle;
  const compactCore = compact(coreToken);

  const bodyRaw = document.body?.innerText || '';
  const bodyNorm = norm(bodyRaw);
  if (!bodyNorm) return { empty: true, links: [] };

  const matchesGroup = txt => {
    if (!txt) return false;
    const n = norm(txt);
    if (n.includes(rawNeedle)) return true;
    if (coreToken.length >= 4 && n.includes(coreToken)) return true;
    const c = compact(n);
    return compactCore.length >= 4 && c.includes(compactCore);
  };

  if (/log in to tiktok|sign in to tiktok/i.test(bodyNorm) && !matchesGroup(bodyNorm)) {
    return { error: 'login', links: [] };
  }

  // Try clicking the target group row in the conversation list if it's in the sidebar but not yet active
  const headerEl = document.querySelector(
    '[data-e2e*="chat-header"], [data-e2e*="nickname"], [class*="ChatHeader"], [class*="chat-header"], [class*="Header"] h1, [class*="Header"] h2, main h1, main h2'
  );
  const headerText = headerEl?.innerText || '';
  if (!allowAnyOpenChat && !matchesGroup(headerText)) {
    const candidates = Array.from(document.querySelectorAll('div, li, a, span, p')).filter(el => {
      if (el.children.length > 5) return false;
      const t = (el.innerText || '').trim();
      return t && t.length <= 90 && matchesGroup(t);
    });
    if (candidates.length) {
      try { candidates[0].click(); } catch {}
    }
  }

  const refreshedRaw = document.body?.innerText || '';
  const matched = allowAnyOpenChat || matchesGroup(refreshedRaw);
  if (!matched) {
    return {
      error: 'group_not_open',
      href: location.href,
      sample: norm(refreshedRaw).slice(0, 120),
      links: [],
    };
  }

  const found = new Set();
  const postPattern = /^https:\/\/(?:(?:www\.|m\.)?tiktok\.com\/(?:@[A-Za-z0-9._-]+\/(?:video|photo)\/\d+|t\/[A-Za-z0-9_-]+)|(?:vm|vt)\.tiktok\.com\/[A-Za-z0-9_-]+|(?:www\.|m\.)?instagram\.com\/(?:[A-Za-z0-9._-]+\/)?(?:p|reel|reels|tv)\/[A-Za-z0-9_-]+|(?:www\.|m\.)?youtube\.com\/(?:shorts|live)\/[A-Za-z0-9_-]+|youtu\.be\/[A-Za-z0-9_-]+|(?:www\.|mobile\.)?(?:x|twitter)\.com\/[A-Za-z0-9._-]+\/status\/\d+)/i;

  function addUrl(raw) {
    if (!raw) return;
    let cleaned = String(raw).trim().replace(/[,;)"'<>\]\s]+$/g, '');
    if (cleaned.startsWith('/@') || cleaned.startsWith('/t/')) {
      cleaned = 'https://www.tiktok.com' + cleaned;
    }
    try {
      const u = new URL(cleaned);
      const base = u.origin + u.pathname.replace(/\/+$/, '') + (u.hostname.includes('youtube.com') && u.pathname === '/watch' ? u.search : '');
      if (postPattern.test(base)) found.add(base);
    } catch {}
  }

  function addVideoId(vid, uid, isPhoto = false) {
    const s = String(vid || '').trim();
    if (!/^\d{15,22}$/.test(s)) return;
    const handle = (typeof uid === 'string' && /^[A-Za-z0-9._-]{2,40}$/.test(uid.replace(/^@/, '')))
      ? uid.replace(/^@/, '')
      : 'tiktok';
    const kind = isPhoto ? 'photo' : 'video';
    addUrl(`https://www.tiktok.com/@${handle}/${kind}/${s}`);
  }

  function scanStringForIdsOrUrls(str) {
    if (typeof str !== 'string' || str.length < 10 || str.length > 20000) return;
    const urls = str.match(/https:\/\/[^\s"'<>\\]+/g);
    if (urls) for (const u of urls) addUrl(u);
    if (str.includes('itemId') || str.includes('item_id') || str.includes('aweme_id') || str.includes('awemeId') || str.includes('video_id')) {
      try {
        const parsed = JSON.parse(str);
        if (parsed && typeof parsed === 'object') {
          const vid = parsed.itemId || parsed.item_id || parsed.aweme_id || parsed.awemeId || parsed.video_id || parsed.videoId;
          const uid = parsed.uid_name || parsed.unique_id || parsed.uniqueId || parsed.author_name || parsed.nickname || parsed.author?.unique_id || parsed.author?.uniqueId;
          const isPhoto = parsed.aweme_type === 150 || parsed.media_type === 'photo';
          if (vid) addVideoId(vid, uid, isPhoto);
          if (parsed.share_url) addUrl(parsed.share_url);
          if (parsed.url) addUrl(parsed.url);
        }
      } catch {
        const m = str.match(/"(?:aweme_id|awemeId|item_id|itemId|video_id|videoId)"\s*:\s*"?(\d{15,22})"?/g);
        if (m) {
          for (const hit of m) {
            const idm = hit.match(/(\d{15,22})/);
            if (idm) addVideoId(idm[1], null, false);
          }
        }
      }
    }
  }

  // 1. Anchor tags & data attributes
  for (const el of document.querySelectorAll('a[href], [data-href], [data-url], [data-item-id], [data-video-id]')) {
    addUrl(el.getAttribute('href') || el.getAttribute('data-href') || el.getAttribute('data-url'));
    addVideoId(el.getAttribute('data-item-id') || el.getAttribute('data-video-id'), null, false);
  }

  // 2. Plain text URLs pasted in messages
  const urlMatches = refreshedRaw.match(/https:\/\/[^\s"'<>]+/g) || [];
  for (const m of urlMatches) addUrl(m);

  // 3. Shared TikTok video cards inside React DM elements (including JSON message content!)
  try {
    const nodes = document.querySelectorAll(
      '[data-e2e*="chat"], [data-e2e*="message"], [data-e2e*="video"], [class*="Message"], [class*="message"], [class*="Video"], [class*="Card"], [class*="Item"]'
    );
    const maxNodes = Math.min(nodes.length, 400);
    for (let i = 0; i < maxNodes; i++) {
      const el = nodes[i];
      for (const k of Object.keys(el)) {
        if (!k.startsWith('__reactProps') && !k.startsWith('__reactFiber')) continue;
        const seenObj = new WeakSet();
        const walk = (obj, depth) => {
          if (!obj || depth > 6) return;
          if (typeof obj === 'string') { scanStringForIdsOrUrls(obj); return; }
          if (typeof obj !== 'object' || seenObj.has(obj)) return;
          seenObj.add(obj);
          if (typeof obj.share_url === 'string') addUrl(obj.share_url);
          if (typeof obj.shareUrl === 'string') addUrl(obj.shareUrl);
          if (typeof obj.href === 'string') addUrl(obj.href);
          if (typeof obj.url === 'string') addUrl(obj.url);
          if (typeof obj.content === 'string') scanStringForIdsOrUrls(obj.content);
          const vid = obj.itemId || obj.item_id || obj.aweme_id || obj.awemeId || obj.videoId || obj.video_id;
          const uid = obj.uniqueId || obj.unique_id || obj.authorUniqueId || obj.author?.uniqueId || obj.author?.unique_id;
          if (vid) addVideoId(vid, uid, obj.aweme_type === 150);
          for (const key of Object.keys(obj)) {
            if (key === 'return' || key === 'stateNode' || key === '_owner' || key === 'alternate' || key === 'sibling') continue;
            const v = obj[key];
            if (v && (typeof v === 'object' || typeof v === 'string')) walk(v, depth + 1);
          }
        };
        walk(el[k], 0);
      }
    }
  } catch {}

  return { ok: true, matched: true, href: location.href, group: targetGroupName, links: Array.from(found) };
}
if (typeof module !== 'undefined') module.exports = { scanGroupDmLinks };
