// Only visible counters on a configured profile page are returned. No cookies,
// messages, or page HTML are sent to the local app.
function readProfile(expectedUrl, platform) {
  const expected = new URL(expectedUrl), actual = new URL(location.href);
  const host = x => x.replace(/^www\./,'').toLowerCase();
  const path = x => x.replace(/\/+$/,'').toLowerCase();
  if(host(expected.hostname)!==host(actual.hostname)||path(expected.pathname)!==path(actual.pathname))return {error:'identity'};
  const text = (document.querySelector('main header') || document.querySelector('header') || document.querySelector('main') || document.body)?.innerText?.slice(0,6000) || '';
  if(/log in to (instagram|tiktok)|sign in to (instagram|tiktok)|account (is )?private|couldn.t find this account/i.test(text))return {error:'login'};
  function parse(raw){
    if(!raw)return null;const cleaned=String(raw).trim().replace(/,/g,'');const m=cleaned.match(/^(\d+(?:\.\d+)?)\s*([kmb])?$/i);if(!m)return null;
    const mult={k:1e3,m:1e6,b:1e9}[String(m[2]||'').toLowerCase()]||1;const n=Math.round(Number(m[1])*mult);
    return Number.isSafeInteger(n)?{value:n,approx:!!m[2]}:null;
  }
  const byLabel=label=>{const m=text.match(new RegExp('([0-9][0-9,.]*\\s*[KMB]?)\\s*'+label+'\\b','i'));return parse(m?.[1]);};
  const get=(selector,label)=>parse(document.querySelector(selector)?.textContent)||byLabel(label);
  const followers=get('[data-e2e="followers-count"]','followers');
  const following=get('[data-e2e="following-count"]','following');
  const likes=platform==='tiktok'?get('[data-e2e="likes-count"]','likes'):null;
  if(!followers&&!following&&!likes)return {error:'unavailable'};
  const links=[];
  if(platform==='instagram') for(const a of document.querySelectorAll('main a[href]')){
    try{const u=new URL(a.href),parts=u.pathname.split('/').filter(Boolean),handle=expected.pathname.split('/').filter(Boolean)[0];if(host(u.hostname)==='instagram.com'&&parts.length===3&&parts[0].toLowerCase()===handle.toLowerCase()&&['p','reel'].includes(parts[1])&&/^[A-Za-z0-9_-]{5,40}$/.test(parts[2]))links.push(u.href);if(links.length>=50)break;}catch{}
  }
  return {post_links:[...new Set(links)],followers:followers?.value??null,following:following?.value??null,likes:likes?.value??null,followers_approx:followers?.approx||false,following_approx:following?.approx||false,likes_approx:likes?.approx||false};
}
if (typeof module !== 'undefined') module.exports={readProfile};
