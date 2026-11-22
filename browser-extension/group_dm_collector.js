// Scans visible post links & TikTok shared-video cards inside the open Group DM thread.
// Executed in Chrome MV3 world: "MAIN" so React __reactFiber / __reactProps on shared video cards are readable!
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

  const matched = allowAnyOpenChat || matchesGroup(bodyRaw);
  if (!matched) {
    return { error: 'group_not_open', href: location.href, sample: bodyNorm.slice(0, 100), links: [] };
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
    addUrl(`https://www.tiktok.com/@${handle}/${isPhoto ? 'photo' : 'video'}/${s}`);
  }

  function scanString(str) {
    if (typeof str !== 'string' || str.length < 10 || str.length > 30000) return;
    const urls = str.match(/https:\/\/[^\s"'<>\\]+/g);
    if (urls) for (const u of urls) addUrl(u);
    if (str.includes('itemId') || str.includes('item_id') || str.includes('aweme_id') || str.includes('awemeId') || str.includes('video_id') || str.includes('videoId')) {
      try {
        const p = JSON.parse(str);
        if (p && typeof p === 'object') {
          const vid = p.itemId || p.item_id || p.aweme_id || p.awemeId || p.video_id || p.videoId || p.video?.id;
          const uid = p.uid_name || p.unique_id || p.uniqueId || p.author_name || p.author?.unique_id || p.author?.uniqueId || p.user?.uniqueId;
          const isPhoto = p.aweme_type === 150 || p.media_type === 'photo' || Boolean(p.imagePost);
          if (vid) addVideoId(vid, uid, isPhoto);
          if (p.share_url) addUrl(p.share_url);
          if (p.shareUrl) addUrl(p.shareUrl);
          if (p.url) addUrl(p.url);
          if (p.content && typeof p.content === 'string') scanString(p.content);
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
  const anchors = document.querySelectorAll('a[href], [data-href], [data-url], [data-item-id], [data-video-id]');
  for (const el of anchors) {
    addUrl(el.getAttribute('href') || el.getAttribute('data-href') || el.getAttribute('data-url'));
    addVideoId(el.getAttribute('data-item-id') || el.getAttribute('data-video-id'), null, false);
  }

  // 2. Plain text URLs pasted in messages
  const urlMatches = bodyRaw.match(/https:\/\/[^\s"'<>]+/g) || [];
  for (const m of urlMatches) addUrl(m);

  // 3. Walk MAIN-world React props & fibers across all visible chat elements
  let reactNodes = 0;
  try {
    const allEls = document.querySelectorAll('div, a, article, section, li, span');
    const seenObj = new WeakSet();
    const limit = Math.min(allEls.length, 1500);
    const walk = (obj, depth) => {
      if (!obj || depth > 6) return;
      if (typeof obj === 'string') { scanString(obj); return; }
      if (typeof obj !== 'object' || seenObj.has(obj)) return;
      seenObj.add(obj);
      if (typeof obj.share_url === 'string') addUrl(obj.share_url);
      if (typeof obj.shareUrl === 'string') addUrl(obj.shareUrl);
      if (typeof obj.href === 'string') addUrl(obj.href);
      if (typeof obj.url === 'string') addUrl(obj.url);
      if (typeof obj.content === 'string') scanString(obj.content);
      if (typeof obj.rawContent === 'string') scanString(obj.rawContent);
      const vid = obj.itemId || obj.item_id || obj.aweme_id || obj.awemeId || obj.videoId || obj.video_id;
      const uid = obj.uniqueId || obj.unique_id || obj.authorUniqueId || obj.author?.uniqueId || obj.author?.unique_id || obj.user?.uniqueId;
      if (vid) addVideoId(vid, uid, obj.aweme_type === 150 || Boolean(obj.imagePost));
      const keys = Object.keys(obj);
      for (let j = 0; j < keys.length && j < 40; j++) {
        const key = keys[j];
        if (key === 'return' || key === 'stateNode' || key === '_owner' || key === 'alternate' || key === 'sibling' || key === 'children') continue;
        const v = obj[key];
        if (v && (typeof v === 'object' || typeof v === 'string')) walk(v, depth + 1);
      }
    };
    for (let i = 0; i < limit; i++) {
      const el = allEls[i];
      for (const k of Object.keys(el)) {
        if (k.startsWith('__reactProps') || k.startsWith('__reactFiber')) {
          reactNodes++;
          walk(el[k], 0);
        }
      }
    }
  } catch {}

  return {
    ok: true,
    matched: true,
    href: location.href,
    group: targetGroupName,
    links: Array.from(found),
    diag: { anchors: anchors.length, reactNodes },
  };
}
if (typeof module !== 'undefined') module.exports = { scanGroupDmLinks };
