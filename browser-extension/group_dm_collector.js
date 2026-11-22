// Scans ONLY visible post links inside the targeted TikTok Group DM thread.
// Supports both main frame and nested TikTok Business Suite iframes.
function scanGroupDmLinks(targetGroupName) {
  const norm = s => String(s || '').toLowerCase().replace(/[\u2018\u2019\u2032`]/g, "'").replace(/\s+/g, ' ').trim();
  const rawNeedle = norm(targetGroupName || "cj_4_prez's group");
  // Match either full group name or core handle token (e.g., "cj_4_prez") to survive ellipsis truncation
  const coreToken = rawNeedle.split(/[\s']/)[0] || rawNeedle;

  const bodyText = norm(document.body?.innerText || '');
  if (!bodyText) return { empty: true, links: [] };
  if (/log in to tiktok|sign in to tiktok/i.test(bodyText) && !bodyText.includes(coreToken)) {
    return { error: 'login', links: [] };
  }

  const matchesGroup = txt => Boolean(txt) && (txt.includes(rawNeedle) || (coreToken.length >= 4 && txt.includes(coreToken)));

  // Try clicking the target group row in the conversation list if not currently active in header
  const headerEl = document.querySelector('[class*="Header"], [class*="header"], [data-e2e*="chat-header"], [data-e2e*="nickname"], main h1, main h2, main h3');
  const headerText = norm(headerEl?.innerText || '');
  if (!matchesGroup(headerText)) {
    const candidates = Array.from(document.querySelectorAll('div, li, a, span, p')).filter(el => {
      if (el.children.length > 5) return false;
      const t = norm(el.innerText || '');
      return t && t.length <= 90 && matchesGroup(t);
    });
    if (candidates.length) {
      candidates[0].click();
    }
  }

  const refreshedRaw = document.body?.innerText || '';
  const refreshedNorm = norm(refreshedRaw);
  if (!matchesGroup(refreshedNorm)) {
    return { error: 'group_not_open', href: location.href, sample: refreshedNorm.slice(0, 120), links: [] };
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

  // 1. Anchor tags & data attributes
  for (const el of document.querySelectorAll('a[href], [data-href], [data-url]')) {
    addUrl(el.getAttribute('href') || el.getAttribute('data-href') || el.getAttribute('data-url'));
  }

  // 2. Plain text URLs pasted in messages
  const urlMatches = refreshedRaw.match(/https:\/\/[^\s"'<>]+/g) || [];
  for (const m of urlMatches) addUrl(m);

  // 3. Shared TikTok video cards inside React DM elements (extract item_id/aweme_id/author if no <a> href)
  try {
    const cards = document.querySelectorAll('[data-e2e*="chat"], [data-e2e*="message"], [class*="Message"], [class*="Video"], [class*="video"]');
    for (const el of cards) {
      for (const k of Object.keys(el)) {
        if (!k.startsWith('__reactProps') && !k.startsWith('__reactFiber')) continue;
        const seenObj = new WeakSet();
        const walk = (obj, depth) => {
          if (!obj || typeof obj !== 'object' || depth > 5 || seenObj.has(obj)) return;
          seenObj.add(obj);
          if (typeof obj.share_url === 'string') addUrl(obj.share_url);
          if (typeof obj.shareUrl === 'string') addUrl(obj.shareUrl);
          if (typeof obj.href === 'string') addUrl(obj.href);
          if (typeof obj.url === 'string') addUrl(obj.url);
          const vid = obj.itemId || obj.item_id || obj.aweme_id || obj.awemeId || obj.videoId || obj.video_id;
          const uid = obj.uniqueId || obj.unique_id || obj.authorUniqueId || obj.secUid || obj.author?.uniqueId || obj.author?.unique_id;
          if (vid && /^\d{15,22}$/.test(String(vid))) {
            const handle = (typeof uid === 'string' && /^[A-Za-z0-9._-]+$/.test(uid)) ? uid : 'tiktok';
            addUrl(`https://www.tiktok.com/@${handle}/video/${vid}`);
          }
          for (const key of Object.keys(obj)) {
            if (key === 'return' || key === 'stateNode' || key === '_owner') continue;
            const v = obj[key];
            if (v && typeof v === 'object') walk(v, depth + 1);
          }
        };
        walk(el[k], 0);
      }
    }
  } catch {}

  return { ok: true, matched: true, href: location.href, group: targetGroupName, links: Array.from(found) };
}
if (typeof module !== 'undefined') module.exports = { scanGroupDmLinks };
