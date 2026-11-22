// Scans ONLY visible post links inside the targeted TikTok Group DM thread.
// Never reads cookies, passwords, or unrelated conversations.
function scanGroupDmLinks(targetGroupName) {
  const needle = String(targetGroupName || 'cj_4_prez').trim().toLowerCase();
  const bodyText = (document.body?.innerText || '').toLowerCase();
  if (/log in to tiktok|sign in to tiktok/i.test(bodyText) && !bodyText.includes(needle)) {
    return { error: 'login', links: [] };
  }

  // Try clicking the target group row in the left conversation list if not currently active
  const headerText = (
    document.querySelector('[class*="Header"], [class*="header"], [data-e2e*="chat-header"], main h1, main h2, main h3')?.innerText || ''
  ).toLowerCase();
  if (needle && !headerText.includes(needle)) {
    const candidates = Array.from(document.querySelectorAll('div, li, a, span, p')).filter(el => {
      if (el.children.length > 4) return false;
      const t = (el.innerText || '').trim().toLowerCase();
      return t && t.length <= 80 && t.includes(needle);
    });
    if (candidates.length) {
      candidates[0].click();
    }
  }

  // Verify the target group is visible on the page before collecting links
  const refreshedText = (document.body?.innerText || '');
  if (needle && !refreshedText.toLowerCase().includes(needle)) {
    return { error: 'group_not_open', links: [] };
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

  // 1. Anchor tags & clickable shared video cards in chat
  for (const el of document.querySelectorAll('a[href], [data-href], [data-url]')) {
    addUrl(el.getAttribute('href') || el.getAttribute('data-href') || el.getAttribute('data-url'));
  }

  // 2. Plain text URLs pasted in messages
  const urlMatches = refreshedText.match(/https:\/\/[^\s"'<>]+/g) || [];
  for (const m of urlMatches) addUrl(m);

  return { ok: true, group: targetGroupName, links: Array.from(found) };
}
if (typeof module !== 'undefined') module.exports = { scanGroupDmLinks };
