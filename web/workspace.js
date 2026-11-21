'use strict';
(() => {
const $=id=>document.getElementById(id);
const video=$('video'), seek=$('seek'), play=$('play'), roll=$('roll');
const labels={pending_review:'Pending review',approved:'Approved',hold:'On hold',exported:'Exported',test_hold:'Private test hold',duplicate_hold:'Duplicate hold',QUEUED:'Queued',PROCESSING:'Processing',READY_FOR_REVIEW:'Ready for review',HELD:'Held',FAILED:'Failed',POSTED:'Posted',BACKED_UP:'Posted · backup verified'};
let mediaFacts=Object.create(null);
function renderPreflight(){
 const box=$('platform-preflight');if(!selected){return;}
 const entry=mediaFacts[selected.preview_sha256];
 const pinned=Number(entry?.format?.size)===selected.preview_bytes?entry:null;
 const rows=window.snapmaticPreflight(pinned,$('title').value,$('caption').value,$('tags').value,selected.kind==='original'&&selected.state==='pending_review');
 box.replaceChildren(...Object.entries(rows).map(([id,list])=>{
  const detail=node('details'),summary=node('summary',providerNames[id]+' · '+(list.some(x=>x.state==='FAIL')?'FAIL':list.some(x=>x.state==='NEEDS H.264 TRANSCODE')?'NEEDS H.264 TRANSCODE':list.some(x=>x.state==='UNKNOWN'&&x.group!=='Inbox capacity')?'NOT VERIFIED':list.some(x=>x.state==='WARN')?'WARN':'CHECKED'));
  const ul=node('ul');list.forEach(x=>ul.append(node('li',x.state+' · '+x.group+': '+x.label,x.state==='FAIL'?'hold':x.state==='WARN'||x.state==='UNKNOWN'||x.state==='NEEDS H.264 TRANSCODE'?'warn':'muted')));
  detail.append(summary,ul);return detail;
 }));
}
let channelProfiles=[], scope='all', localStatus=null;
function syncIframeThemes(mode){
 for(const id of ['sources','profiles']){
  const f=$(id)?.querySelector('iframe');
  try{f?.contentWindow?.postMessage({type:'snapmatic:theme',mode},location.origin);}catch{}
 }
}
function applyShellTheme(mode){
 const light=mode==='light';
 document.documentElement.dataset.theme=light?'light':'dark';
 try{localStorage.setItem('lv-theme',light?'light':'dark');}catch{}
 const btn=$('theme-toggle');
 if(btn){btn.textContent=light?'☾ Dark':'☀ Light';btn.setAttribute('aria-pressed',String(light));}
 syncIframeThemes(light?'light':'dark');
}
applyShellTheme((()=>{try{return localStorage.getItem('lv-theme')||'dark';}catch{return 'dark';}})());
if($('theme-toggle'))$('theme-toggle').onclick=()=>applyShellTheme(document.documentElement.dataset.theme==='light'?'dark':'light');
window.addEventListener('storage',e=>{if(e.key==='lv-theme'&&(e.newValue==='dark'||e.newValue==='light'))applyShellTheme(e.newValue);});

function activeProfile(){return scope==='all'||scope==='demo-channel'||channelProfiles.some(p=>p.profile_id===scope&&p.configured);}
function fmtBytes(b){
 if(!Number.isFinite(b)||b<0)return '0 B';
 if(b<1024)return b+' B';
 if(b<1048576)return (b/1024).toFixed(1)+' KiB';
 if(b<1073741824)return (b/1048576).toFixed(1)+' MiB';
 return (b/1073741824).toFixed(2)+' GiB';
}
function renderLocalDiagnostics(s){
 const toolsBox=$('system-tools');
 if(toolsBox&&s.tools){
  const list=[
   ['FFmpeg',s.tools.ffmpeg],
   ['FFprobe',s.tools.ffprobe],
   ['yt-dlp',s.tools.ytdlp],
   ['gallery-dl',s.tools.gallery_dl],
   ['Hardware Video Encoder',{ready:Boolean(s.hw_encoder),version:s.hw_encoder?s.hw_encoder+' (auto CPU fallback enabled)':'CPU libx264 (no GPU encoder detected)'}],
  ];
  toolsBox.replaceChildren(...list.map(([label,info])=>{
   const el=node('div',undefined,'kv');
   el.append(node('strong',label),node('span',info?.ready?'READY':'MISSING',info?.ready?'verified':'warn'));
   if(info?.version)el.append(node('div',info.version,'muted'));
   if(info?.path)el.append(node('div',info.path,'muted'));
   return el;
  }));
 }
 const storeBox=$('system-storage');
 if(storeBox&&s.storage){
  const st=s.storage;
  const rows=[
   ['Downloaded Sources',fmtBytes(st.sources_bytes)+' · '+(st.sources_files||0)+' files'],
   ['Local Exports',fmtBytes(st.exports_bytes)+' · '+(st.exports_files||0)+' files'],
   ['Thumbnail Cache',fmtBytes(st.thumb_cache_bytes)+' · '+(st.thumb_cache_files||0)+' cached JPEGs'],
   ['Film Roll Queue',(s.film_roll_total||0)+' total · '+(s.pending_count||0)+' pending · '+(s.approved_count||0)+' approved · '+(s.exported_count||0)+' exported'],
  ];
  storeBox.replaceChildren(...rows.map(([k,v])=>{
   const el=node('div',undefined,'kv');el.append(node('strong',k),node('span',v));return el;
  }));
 }
}
function refreshLocalStatus(){
 return get('/api/workspace/local-status').then(s=>{
  localStatus=s;
  const pill=$('sm-tools-pill');
  if(pill)pill.textContent=(s.ffmpeg_ready&&s.ytdlp_ready)?'FFmpeg + yt-dlp Ready':(s.ffmpeg_ready?'FFmpeg Ready · yt-dlp Missing':s.ytdlp_ready?'yt-dlp Ready · FFmpeg Missing':'Tools Not Configured');
  if($('sm-live-nvenc'))$('sm-live-nvenc').textContent='FFmpeg · '+(s.ffmpeg_ready?'ready':'missing');
  if($('sm-live-source'))$('sm-live-source').textContent='yt-dlp · '+(s.ytdlp_ready?'ready':'missing');
  if($('sm-live-queue'))$('sm-live-queue').textContent='Film Roll · '+s.film_roll_total+' clip'+(s.film_roll_total===1?'':'s')+(s.exported_count?' · '+s.exported_count+' exported':'');
  renderLocalDiagnostics(s);
  renderNvenc();
 }).catch(()=>{});
}
function renderQueueProfiles(){
 const sel=$('queue-profile'),btn=$('queue-send');
 if(!sel)return;
 const dests=channelProfiles.filter(p=>Number.isInteger(p.db_id));
 const prev=sel.value;
 const opts=[node('option',dests.length?'Choose a posting profile…':'No posting profiles yet (add in Profiles tab)')];
 opts[0].value='';
 for(const p of dests){const o=node('option',p.channel_alias);o.value=String(p.db_id);o.dataset.platform=p.platform||'';opts.push(o);}
 sel.replaceChildren(...opts);
 if(prev&&dests.some(p=>String(p.db_id)===prev))sel.value=prev;
 else if(dests.length===1)sel.value=String(dests[0].db_id);
 if(btn)btn.disabled=!selected?.from_sources||!sel.value;
}
if($('queue-profile'))$('queue-profile').onchange=()=>{if($('queue-send'))$('queue-send').disabled=!selected?.from_sources||!$('queue-profile').value;};
if($('queue-send'))$('queue-send').onclick=async()=>{
 const row=selected,sel=$('queue-profile'),btn=$('queue-send'),out=$('queue-result');
 if(!row?.from_sources||!sel?.value||btn.disabled)return;
 const pid=Number(sel.value),plat=sel.selectedOptions[0]?.dataset.platform||'';
 const common={title:$('title').value,caption:$('caption').value,tags:$('tags').value};
 const mats=pd.materialize(common,pdState?.overrides||{},pdState||{});
 const caption=(plat&&mats[plat])||[common.title,common.caption,common.tags].filter(Boolean).join('\n\n');
 btn.disabled=true;
 try{
  await get('/api/profiles/'+encodeURIComponent(pid)+'/queue',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source_id:row.source_id,path:row.source_path,caption})});
  const label=sel.selectedOptions[0]?.textContent||'profile';
  if(out)out.textContent='Queued in '+label+'. Open Profiles to review or post manually.';
  note('Sent "'+row.name+'" to '+label+' queue.');
 }catch(e){if(out)out.textContent='Queue failed: '+e.message;note(e.message);}
 finally{btn.disabled=!selected?.from_sources||!sel.value;}
};
function refreshScope(){
 phase5.clear();p5LoadLabels();
 renderProviders();if(indexReady)renderRoll();if(activeView==='gallery')renderGallery();if(activeView==='ledger')renderLedger();
 if(!activeProfile()){saveDraft();selected=null;pdState=null;editState=null;generation++;cancelFrameWatch();stopVideo();refreshDraftEditor();updateTrim();$('approve').disabled=true;mediaEmpty('CHANNEL NOT CONFIGURED','NO CREDENTIAL OR SOURCE ALIAS VERIFIED');}
}
$('channel-scope').onchange=()=>{scope=$('channel-scope').value;sessionStorage.setItem('snapmatic-profile',scope);refreshScope();};
function loadChannelProfiles(){
 return get('/api/workspace/channel-profiles').then(d=>{
  channelProfiles=d.profiles;
  const saved=sessionStorage.getItem('snapmatic-profile');if(channelProfiles.some(p=>p.profile_id===saved))scope=saved;
  $('channel-scope').replaceChildren(...channelProfiles.map(p=>{const o=node('option',p.channel_alias+(p.configured?'':' (not configured)'));o.value=p.profile_id;return o;}));
  $('channel-scope').value=scope;renderQueueProfiles();refreshScope();
 }).catch(()=>{});
}
loadChannelProfiles();

let oauthStatus=null, analyticsData=null, analyticsLoad=null, igArmTimer=null, xArmTimer=null;
function brandIcon(prov){
 const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');
 svg.setAttribute('viewBox','0 0 24 24');svg.setAttribute('class','brand-ico');svg.setAttribute('aria-hidden','true');
 const p=document.createElementNS('http://www.w3.org/2000/svg','path');
 if(prov==='youtube'){
  p.setAttribute('fill','#FF3B3B');
  p.setAttribute('d','M23.5 6.2a3 3 0 0 0-2.1-2.1C19.5 3.5 12 3.5 12 3.5s-7.5 0-9.4.6A3 3 0 0 0 .5 6.2 31.4 31.4 0 0 0 0 12a31.4 31.4 0 0 0 .5 5.8 3 3 0 0 0 2.1 2.1c1.9.6 9.4.6 9.4.6s7.5 0 9.4-.6a3 3 0 0 0 2.1-2.1A31.4 31.4 0 0 0 24 12a31.4 31.4 0 0 0-.5-5.8zM9.6 15.6V8.4l6.3 3.6-6.3 3.6z');
 }else if(prov==='instagram'){
  p.setAttribute('fill','#E1306C');
  p.setAttribute('d','M7.8 2h8.4A5.8 5.8 0 0 1 22 7.8v8.4a5.8 5.8 0 0 1-5.8 5.8H7.8A5.8 5.8 0 0 1 2 16.2V7.8A5.8 5.8 0 0 1 7.8 2zm8.4 2H7.8A3.8 3.8 0 0 0 4 7.8v8.4A3.8 3.8 0 0 0 7.8 20h8.4a3.8 3.8 0 0 0 3.8-3.8V7.8A3.8 3.8 0 0 0 16.2 4zM12 7a5 5 0 1 1 0 10 5 5 0 0 1 0-10zm0 2a3 3 0 1 0 0 6 3 3 0 0 0 0-6zm5.25-3.5a1.25 1.25 0 1 1 0 2.5 1.25 1.25 0 0 1 0-2.5z');
 }else if(prov==='tiktok'){
  p.setAttribute('fill','#25F4EE');
  p.setAttribute('d','M19.59 6.69a4.83 4.83 0 0 1-3.77-4.25V2h-3.45v13.67a2.89 2.89 0 0 1-5.2 1.74 2.89 2.89 0 0 1 2.31-4.64 2.93 2.93 0 0 1 .88.13V9.4a6.84 6.84 0 0 0-1-.05A6.33 6.33 0 0 0 5 20.1a6.34 6.34 0 0 0 10.86-4.43v-7a8.16 8.16 0 0 0 4.77 1.52v-3.4a4.85 4.85 0 0 1-1-.1z');
 }else{
  p.setAttribute('fill','currentColor');
  p.setAttribute('d','M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z');
 }
 svg.append(p);
 return svg;
}
function accountAvatar(prov,url){
 if(!url)return brandIcon(prov);
 const img=node('img',undefined,'acct-avatar');
 img.src=url;img.alt='';img.referrerPolicy='no-referrer';
 img.onerror=()=>img.replaceWith(brandIcon(prov));
 return img;
}
function setInspectorTab(tab){
 if(!['captions','export','upload','checks'].includes(tab))tab='captions';
 document.querySelectorAll('#inspector-tabs button[data-itab]').forEach(b=>b.setAttribute('aria-selected',String(b.dataset.itab===tab)));
 document.querySelectorAll('.itab-pane[data-itab-pane]').forEach(p=>{p.hidden=p.dataset.itabPane!==tab;});
 try{localStorage.setItem('snapmatic-itab-v1',tab);}catch{}
}
document.querySelectorAll('#inspector-tabs button[data-itab]').forEach(b=>{b.onclick=()=>setInspectorTab(b.dataset.itab);});
setInspectorTab((()=>{try{return localStorage.getItem('snapmatic-itab-v1')||'captions';}catch{return 'captions';}})());

function igDaysLeft(info){
 if(!info?.connected||!Number.isSafeInteger(info.expires_at)||info.expires_at<=0)return null;
 return (info.expires_at-(Date.now()/1000))/86400;
}
function disarmIgPublish(){
 if(igArmTimer){clearTimeout(igArmTimer);igArmTimer=null;}
 const arm=$('ig-publish-arm'),conf=$('ig-publish-confirm');
 if(conf)conf.hidden=true;
 if(arm)arm.hidden=false;
 if(xArmTimer){clearTimeout(xArmTimer);xArmTimer=null;}
 const xArm=$('x-post-arm'),xConf=$('x-post-confirm');
 if(xConf)xConf.hidden=true;
 if(xArm)xArm.hidden=false;
}
function renderShipBadges(row){
 if(!row?.yt_video_id&&!row?.ig_media_id&&!row?.tt_publish_id&&!row?.x_tweet_id)return null;
 const wrap=node('span',undefined,'ship-badges');
 if(row.yt_video_id)wrap.append(node('span','YT ✓','ship-badge ship-yt'));
 if(row.ig_media_id)wrap.append(node('span','IG ✓','ship-badge ship-ig'));
 if(row.tt_publish_id)wrap.append(node('span','TT ✓','ship-badge ship-tt'));
 if(row.x_tweet_id)wrap.append(node('span','X ✓','ship-badge ship-x'));
 return wrap;
}
function renderClipShippedBanner(row){
 const box=$('clip-shipped-links'),tabBadge=$('itab-upload-badge');
 const count=[row?.yt_video_id,row?.ig_media_id,row?.tt_publish_id,row?.x_tweet_id].filter(Boolean).length;
 if(tabBadge){tabBadge.textContent=count+'✓';tabBadge.hidden=count===0;}
 if(!box)return;
 if(!row||count===0){box.hidden=true;box.replaceChildren();return;}
 box.hidden=false;
 const parts=[node('strong','Already uploaded:')];
 if(row.yt_video_id){
  const a=node('a','YouTube Studio ('+row.yt_video_id+') ↗','oauth-link');
  a.href='https://studio.youtube.com/video/'+encodeURIComponent(row.yt_video_id)+'/edit';a.target='_blank';a.rel='noopener noreferrer';
  parts.push(node('span','YT ✓','ship-badge ship-yt'),a);
 }
 if(row.ig_media_id){
  parts.push(node('span','IG ✓','ship-badge ship-ig'));
  if(row.ig_permalink){
   const a=node('a','Instagram Reel ↗','oauth-link');a.href=row.ig_permalink;a.target='_blank';a.rel='noopener noreferrer';
   parts.push(a);
  }else{
   parts.push(node('span','Media '+row.ig_media_id,'muted'));
  }
 }
 if(row.tt_publish_id){
  parts.push(node('span','TT ✓','ship-badge ship-tt'),node('span','Sent to TikTok Inbox','muted'));
 }
 if(row.x_tweet_id){
  const a=node('a','X Post ↗','oauth-link');
  a.href='https://x.com/i/web/status/'+encodeURIComponent(row.x_tweet_id);a.target='_blank';a.rel='noopener noreferrer';
  parts.push(node('span','X ✓','ship-badge ship-x'),a);
 }
 box.replaceChildren(...parts);
}
function xPostText(){
 const custom=($('draft-x')?.value||'').trim();
 if(custom)return custom.slice(0,280);
 const parts=[$('title')?.value.trim(),$('caption')?.value.trim(),$('tags')?.value.trim()].filter(Boolean);
 return parts.join(' ').slice(0,280);
}
function syncYoutubeDraftButton(){
 const btn=$('yt-upload-draft'),out=$('yt-upload-result');
 const ytConn=Boolean(oauthStatus?.youtube?.connected);
 const sched=$('yt-draft-schedule'),priv=$('yt-draft-privacy');
 if(sched&&priv){
  if(sched.value){priv.value='private';priv.disabled=true;}
  else priv.disabled=!ytConn||!selected?.from_sources;
 }
 if(btn)btn.disabled=!ytConn||!selected?.from_sources;
 if(out&&!out.dataset.busy){
  const mode=sched?.value?'scheduled Private':(priv?.value==='unlisted'?'Unlisted':'Private');
  out.textContent=!ytConn?'Connect YouTube in Connections & System to upload drafts (Private or Unlisted only).':selected?.from_sources?(selected.yt_video_id?'Already uploaded to YouTube ('+selected.yt_video_id+'). Uploading again creates a separate draft.':'Uploads '+(selected.exported?'your exported MP4':'this source clip')+' as a '+mode+' draft in YouTube Studio.'):'Select a Film Roll clip to upload a draft.';
 }
 const igTest=$('ig-upload-container'),igArm=$('ig-publish-arm'),igOut=$('ig-upload-result');
 const igConn=Boolean(oauthStatus?.instagram?.connected);
 if(igTest)igTest.disabled=!igConn||!selected?.from_sources;
 if(igArm)igArm.disabled=!igConn||!selected?.from_sources;
 if(igOut&&!igOut.dataset.busy){
  igOut.textContent=!igConn?'Connect Instagram in Connections & System to upload Reels directly.':selected?.from_sources?(selected.ig_media_id?'Already published to Instagram ('+selected.ig_media_id+').':'Ready to upload '+(selected.exported?'your exported MP4':'this source clip')+' directly to '+(oauthStatus.instagram.handle||'Instagram')+'.'):'Select a Film Roll clip to upload a Reel.';
 }
 const ttBtn=$('tt-upload-draft'),ttOut=$('tt-upload-result');
 const ttConn=Boolean(oauthStatus?.tiktok?.connected);
 if(ttBtn)ttBtn.disabled=!ttConn||!selected?.from_sources;
 if(ttOut&&!ttOut.dataset.busy){
  ttOut.textContent=!ttConn?'Connect TikTok in Connections & System to send drafts straight to your TikTok Inbox.':selected?.from_sources?(selected.tt_publish_id?'Already sent to TikTok Inbox ('+selected.tt_publish_id+').':'Sends '+(selected.exported?'your exported MP4':'this source clip')+' to your TikTok phone app Inbox as a draft.'):'Select a Film Roll clip to send to TikTok Inbox.';
 }
 const xArm=$('x-post-arm'),xOut=$('x-upload-result');
 const xConn=Boolean(oauthStatus?.x?.connected);
 if(xArm)xArm.disabled=!xConn||!selected?.from_sources;
 if(xOut&&!xOut.dataset.busy){
  xOut.textContent=!xConn?'Connect X in Connections & System to post video clips directly.':selected?.from_sources?(selected.x_tweet_id?'Already posted to X ('+selected.x_tweet_id+').':'Ready to upload '+(selected.exported?'your exported MP4':'this source clip')+' to '+(oauthStatus.x.handle||'X')+'.'):'Select a Film Roll clip to post to X.';
 }
 for(const [id,on] of [['ship-chk-yt',ytConn],['ship-chk-ig',igConn],['ship-chk-tt',ttConn],['ship-chk-x',xConn]]){
  const el=$(id);if(el){el.disabled=!on||!selected?.from_sources;if(!on)el.checked=false;}
 }
 const anyShip=['ship-chk-yt','ship-chk-ig','ship-chk-tt','ship-chk-x'].some(id=>$(id)?.checked&&!$(id)?.disabled);
 if($('multi-ship-go'))$('multi-ship-go').disabled=!selected?.from_sources||!anyShip;
 renderClipShippedBanner(selected);
 updateMockOverlay();
}
for(const id of ['ship-chk-yt','ship-chk-ig','ship-chk-tt','ship-chk-x']){if($(id))$(id).onchange=syncYoutubeDraftButton;}
if($('yt-draft-schedule'))$('yt-draft-schedule').onchange=syncYoutubeDraftButton;
if($('yt-draft-privacy'))$('yt-draft-privacy').onchange=syncYoutubeDraftButton;
function renderOAuthCard(prefix,info,label){
 const badge=$(prefix+'-oauth-badge'),meta=$(prefix+'-oauth-meta'),conn=$(prefix+'-connect'),titleEl=$(prefix+'-oauth-title');
 const ref=$(prefix+'-refresh'),disc=$(prefix+'-disconnect'),ext=$(prefix+'-extend'),uri=$(prefix+'-redirect-uri'),setup=$(prefix+'-oauth-setup');
 if(!badge||!info)return;
 const provKey=prefix==='yt'?'youtube':prefix==='ig'?'instagram':prefix==='tt'?'tiktok':'x';
 if(titleEl)titleEl.replaceChildren(accountAvatar(provKey,info.connected?info.avatar_url:''),label);
 if(uri&&info.redirect_uri)uri.textContent=info.redirect_uri;
 if(info.connected){
  const days=prefix==='ig'?igDaysLeft(info):null;
  const expiring=days!==null&&days<7;
  badge.textContent=expiring?(days<=0?'TOKEN EXPIRED':'EXPIRES IN '+Math.max(1,Math.ceil(days))+'D'):'CONNECTED';
  badge.className='flag '+(expiring?'warn':'ready');
  const parts=[info.account_name,info.handle].filter(Boolean).join(' · ');
  const stats=[];
  if(Number.isSafeInteger(info.subscribers))stats.push(info.subscribers.toLocaleString()+' '+(prefix==='yt'?'subscribers':'followers'));
  if(Number.isSafeInteger(info.media_count))stats.push(info.media_count.toLocaleString()+' '+(prefix==='yt'?'videos':'posts'));
  meta.textContent=(parts||label+' connected')+(stats.length?' ('+stats.join(' · ')+')':'')+(info.expires_at?' · Token valid until '+new Date(info.expires_at*1000).toLocaleString():'');
  if(conn){conn.textContent='Reconnect '+label;conn.disabled=false;}
  if(ref)ref.hidden=false;
  if(disc)disc.hidden=false;
  if(ext){ext.hidden=false;ext.classList.toggle('extend-highlight',expiring);}
  if(setup&&!setup.dataset.touched)setup.open=false;
 }else if(info.client_configured){
  badge.textContent='READY TO CONNECT';badge.className='flag warn';
  meta.textContent='Credentials saved ('+(info.client_id_preview||'configured')+'). Click Connect '+label+' to sign in.'+(info.last_error?' Last error: '+info.last_error:'');
  if(conn){conn.textContent='Connect '+label;conn.disabled=false;}
  if(ref)ref.hidden=true;
  if(disc)disc.hidden=true;
  if(ext)ext.hidden=true;
 }else{
  badge.textContent='NOT CONFIGURED';badge.className='flag warn';
  meta.textContent='Save your OAuth credentials below, then click Connect '+label+'.'+(info.last_error?' Last error: '+info.last_error:'');
  if(conn){conn.textContent='Connect '+label;conn.disabled=true;}
  if(ref)ref.hidden=true;
  if(disc)disc.hidden=true;
  if(ext)ext.hidden=true;
 }
}
function refreshOAuthStatus(){
 return get('/api/oauth/status').then(s=>{
  oauthStatus=s;
  renderOAuthCard('yt',s.youtube,'YouTube');
  renderOAuthCard('ig',s.instagram,'Instagram');
  renderOAuthCard('tt',s.tiktok,'TikTok');
  renderOAuthCard('x',s.x,'X (Twitter)');
  renderProviders();
  syncYoutubeDraftButton();
 }).catch(()=>{});
}
if($('ig-extend'))$('ig-extend').onclick=async()=>{
 const btn=$('ig-extend');btn.disabled=true;
 try{
  const r=await get('/api/oauth/instagram/extend',{method:'POST'});
  note('Extended Instagram token until '+new Date(r.expires_at*1000).toLocaleDateString()+'.');
  await refreshOAuthStatus();
 }catch(e){note('Token extend failed: '+e.message);}
 finally{btn.disabled=false;}
};
for(const [prefix,prov] of [['yt','youtube'],['ig','instagram'],['tt','tiktok'],['x','x']]){
 if($(prefix+'-oauth-setup'))$(prefix+'-oauth-setup').addEventListener('toggle',e=>{e.currentTarget.dataset.touched='1';});
 if($(prefix+'-copy-uri'))$(prefix+'-copy-uri').onclick=async()=>{
  const text=$(prefix+'-redirect-uri')?.textContent||'';
  try{await navigator.clipboard.writeText(text);note('Copied redirect URI.');}catch{note(text);}
 };
 if($(prefix+'-save-config'))$(prefix+'-save-config').onclick=async()=>{
  const btn=$(prefix+'-save-config'),out=$(prefix+'-config-msg');btn.disabled=true;
  try{
   const body={
    client_json:prefix==='yt'?($(prefix+'-client-json')?.value||''):'',
    client_id:$(prefix+'-client-id')?.value||'',
    client_secret:$(prefix+'-client-secret')?.value||'',
   };
   await get('/api/oauth/'+prov+'/config',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
   if($(prefix+'-client-json'))$(prefix+'-client-json').value='';
   if($(prefix+'-client-secret'))$(prefix+'-client-secret').value='';
   if(out)out.textContent='Saved securely in Windows Credential Manager. Click Connect above.';
   note('Saved '+prov+' OAuth credentials.');
   await refreshOAuthStatus();
  }catch(e){if(out)out.textContent='Error: '+e.message;note(e.message);}
  finally{btn.disabled=false;}
 };
 if($(prefix+'-connect'))$(prefix+'-connect').onclick=async()=>{
  const btn=$(prefix+'-connect'),link=$(prefix+'-auth-link');btn.disabled=true;
  try{
   const r=await get('/api/oauth/'+prov+'/start',{method:'POST'});
   if(link&&r.auth_url){link.href=r.auth_url;link.hidden=false;}
   note(r.browser_opened?'Opened sign-in in your browser. Complete login there to finish connecting.':'Click the sign-in link to connect your account.');
  }catch(e){note(e.message);}
  finally{btn.disabled=false;}
 };
 if($(prefix+'-refresh'))$(prefix+'-refresh').onclick=async()=>{
  const btn=$(prefix+'-refresh');btn.disabled=true;
  try{
   await get('/api/oauth/'+prov+'/refresh',{method:'POST'});
   note('Refreshed '+prov+' account status.');
   await refreshOAuthStatus();loadChannelProfiles();
  }catch(e){note('Refresh failed: '+e.message);await refreshOAuthStatus();}
  finally{btn.disabled=false;}
 };
 if($(prefix+'-disconnect'))$(prefix+'-disconnect').onclick=async()=>{
  const btn=$(prefix+'-disconnect');btn.disabled=true;
  try{
   await get('/api/oauth/'+prov+'/disconnect',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({clear_config:false})});
   note('Disconnected '+prov+' account.');
   await refreshOAuthStatus();
  }catch(e){note(e.message);}
  finally{btn.disabled=false;}
 };
}
if($('tt-sys-exports'))$('tt-sys-exports').onclick=()=>{$('edit-open-exports')?.click();};
if($('ig-token-connect'))$('ig-token-connect').onclick=async()=>{
 const tokEl=$('ig-direct-token'),btn=$('ig-token-connect'),out=$('ig-config-msg');
 const access_token=(tokEl?.value||'').trim();
 if(!access_token){if(out)out.textContent='Paste a Meta access token first.';tokEl?.focus();return;}
 btn.disabled=true;if(out)out.textContent='Verifying token & fetching Instagram Business/Creator account…';
 try{
  const r=await get('/api/oauth/instagram/token',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({access_token})});
  if(tokEl)tokEl.value='';
  if(out)out.textContent='Connected Instagram '+(r.handle||r.account_name)+' (stored in Windows Credential Manager).';
  note('Connected Instagram '+(r.handle||r.account_name)+'!');
  await refreshOAuthStatus();loadChannelProfiles();
 }catch(e){if(out)out.textContent='Error: '+e.message;note(e.message);}
 finally{btn.disabled=false;}
};
if($('yt-upload-draft'))$('yt-upload-draft').onclick=async()=>{
 const row=selected,btn=$('yt-upload-draft'),out=$('yt-upload-result');
 if(!row?.from_sources||btn.disabled)return;
 const schedRaw=($('yt-draft-schedule')?.value||'').trim();
 let publish_at=null;
 if(schedRaw){
  const dt=new Date(schedRaw);
  if(Number.isNaN(dt.getTime())){note('Enter a valid schedule date and time.');return;}
  publish_at=dt.toISOString();
 }
 const privacy=publish_at?'private':($('yt-draft-privacy')?.value||'private');
 const wantThumb=Boolean($('yt-draft-thumb')?.checked&&coverUrl);
 btn.disabled=true;if(out){out.dataset.busy='1';out.textContent='Uploading '+privacy.toUpperCase()+' draft to YouTube Studio'+(wantThumb?' (with cover thumbnail)…':'…');}
 try{
  const common={title:$('title').value.trim()||row.name,caption:$('caption').value.trim(),tags:$('tags').value.trim()};
  const mats=pd.materialize(common,pdState?.overrides||{},pdState||{});
  const thumbData=wantThumb?await coverBase64():null;
  const res=await get('/api/oauth/youtube/upload-draft/'+encodeURIComponent(row.clip_id),{
   method:'POST',headers:{'Content-Type':'application/json'},
   body:JSON.stringify({
    title:common.title,
    description:mats.youtube,
    tags:$('draft-yt-api-tags')?.value||common.tags,
    use_source:!row.exported,
    privacy,
    publish_at,
    category_id:$('yt-draft-category')?.value||'20',
    made_for_kids:Boolean($('yt-draft-kids')?.checked),
    as_short:Boolean($('yt-draft-shorts')?$('yt-draft-shorts').checked:true),
    thumbnail_png_base64:thumbData
   })
  });
  row.yt_video_id=res.video_id;
  if(out){
   delete out.dataset.busy;
   const a=node('a','Open Draft in YouTube Studio ↗');a.href=res.studio_url;a.target='_blank';a.rel='noopener noreferrer';a.className='oauth-link';
   const extra=(res.as_short?' · #Shorts':'')+(res.publish_at?' · Scheduled':'')+(res.thumbnail_uploaded?' · Thumbnail set':res.thumbnail_note?' · Thumbnail skipped: '+res.thumbnail_note:'')+(res.shorts_note?' — '+res.shorts_note:'');
   out.replaceChildren('Uploaded '+String(res.privacy||'private').toUpperCase()+' draft ('+res.video_id+')'+extra+'. ',a);
  }
  note('Uploaded '+String(res.privacy||'private').toUpperCase()+' draft to YouTube Studio ('+res.video_id+').');
  renderClipShippedBanner(row);reload();loadExportHistory();
 }catch(e){
  if(out){delete out.dataset.busy;out.textContent='Upload failed: '+e.message;}
  note('YouTube upload failed: '+e.message);
 }finally{syncYoutubeDraftButton();}
};
async function runInstagramUpload(publish){
 const row=selected,testBtn=$('ig-upload-container'),armBtn=$('ig-publish-arm'),confBtn=$('ig-publish-confirm'),out=$('ig-upload-result');
 if(!row?.from_sources)return;
 disarmIgPublish();
 if(testBtn)testBtn.disabled=true;
 if(armBtn)armBtn.disabled=true;
 if(confBtn)confBtn.disabled=true;
 if(out){out.dataset.busy='1';out.textContent=publish?'Uploading MP4 to Instagram & publishing Reel (waiting for Meta video processing)…':'Uploading unpublished test container to Instagram (waiting for Meta video processing)…';}
 try{
  const common={title:$('title').value.trim()||row.name,caption:$('caption').value.trim(),tags:$('tags').value.trim()};
  const mats=pd.materialize(common,pdState?.overrides||{},pdState||{});
  const res=await get('/api/oauth/instagram/upload-reel/'+encodeURIComponent(row.clip_id),{
   method:'POST',headers:{'Content-Type':'application/json'},
   body:JSON.stringify({
    caption:mats.instagram,
    share_to_feed:Boolean($('ig-upload-share')?$('ig-upload-share').checked:$('draft-ig-share')?.checked),
    thumb_offset_ms:pdState?.cover?.thumb_offset_ms??(Number($('ig-thumb-slider')?.value)||null),
    use_source:!row.exported,
    publish:Boolean(publish)
   })
  });
  if(res.published&&res.media_id){
   row.ig_media_id=res.media_id;
   row.ig_permalink=res.permalink||'';
   if(out){
    delete out.dataset.busy;
    if(res.permalink){
     const a=node('a','Open Reel on Instagram ↗');a.href=res.permalink;a.target='_blank';a.rel='noopener noreferrer';a.className='oauth-link';
     out.replaceChildren('Published Reel ('+res.media_id+'). ',a);
    }else{
     out.textContent='Published Reel ('+res.media_id+').';
    }
   }
   note('Published Instagram Reel ('+res.media_id+')!');
   renderClipShippedBanner(row);reload();loadExportHistory();
  }else{
   if(out){delete out.dataset.busy;out.textContent='Unpublished test container FINISHED (ID: '+res.container_id+'). Not published to feed.';}
   note('Uploaded unpublished Instagram test container ('+res.container_id+').');
  }
 }catch(e){
  if(out){delete out.dataset.busy;out.textContent='Instagram upload failed: '+e.message;}
  note('Instagram upload failed: '+e.message);
 }finally{
  if(confBtn)confBtn.disabled=false;
  syncYoutubeDraftButton();
 }
}
if($('ig-upload-container'))$('ig-upload-container').onclick=()=>runInstagramUpload(false);
if($('ig-publish-arm'))$('ig-publish-arm').onclick=()=>{
 if($('ig-publish-arm').disabled)return;
 $('ig-publish-arm').hidden=true;
 if($('ig-publish-confirm')){$('ig-publish-confirm').hidden=false;$('ig-publish-confirm').focus();}
 if(igArmTimer)clearTimeout(igArmTimer);
 igArmTimer=setTimeout(disarmIgPublish,10000);
};
if($('ig-publish-confirm'))$('ig-publish-confirm').onclick=()=>runInstagramUpload(true);
if($('tt-upload-draft'))$('tt-upload-draft').onclick=async()=>{
 const row=selected,btn=$('tt-upload-draft'),out=$('tt-upload-result');
 if(!row?.from_sources||btn.disabled)return;
 btn.disabled=true;
 if(out){out.dataset.busy='1';out.textContent='Uploading MP4 to your TikTok Inbox (may take a few seconds)…';}
 try{
  const res=await get('/api/oauth/tiktok/upload-draft/'+encodeURIComponent(row.clip_id),{
   method:'POST',headers:{'Content-Type':'application/json'},
   body:JSON.stringify({use_source:!row.exported})
  });
  row.tt_publish_id=res.publish_id;
  if(out){
   delete out.dataset.busy;
   out.textContent='Sent to TikTok Inbox ('+(res.status||'PROCESSING')+' · ID '+res.publish_id+'). Open your TikTok phone app Inbox notification to add audio & publish!';
  }
  if($('tt-autocopy')?.checked){
   try{await navigator.clipboard.writeText(ttText());}catch{}
  }
  note('Sent draft to TikTok Inbox!'+(($('tt-autocopy')?.checked)?' Caption copied to clipboard!':'')+' Check your TikTok phone app notifications.');
  renderClipShippedBanner(row);reload();loadExportHistory();
 }catch(e){
  if(out){delete out.dataset.busy;out.textContent='TikTok upload failed: '+e.message;}
  note('TikTok upload failed: '+e.message);
 }finally{syncYoutubeDraftButton();}
};
if($('x-copy'))$('x-copy').onclick=async()=>{
 try{await navigator.clipboard.writeText(xPostText());note('Copied X post text ('+xPostText().length+'/280).');}
 catch{note('Clipboard unavailable.');}
};
if($('draft-x'))$('draft-x').addEventListener('input',()=>{
 const len=$('draft-x').value.length;
 if($('draft-x-count')){$('draft-x-count').textContent=len+'/280 characters';$('draft-x-count').className=len>280?'warn':'muted';}
});
if($('x-post-arm'))$('x-post-arm').onclick=()=>{
 if($('x-post-arm').disabled)return;
 $('x-post-arm').hidden=true;
 if($('x-post-confirm')){$('x-post-confirm').hidden=false;$('x-post-confirm').focus();}
 if(xArmTimer)clearTimeout(xArmTimer);
 xArmTimer=setTimeout(disarmIgPublish,10000);
};
async function runXVideoPost(){
 const row=selected,arm=$('x-post-arm'),conf=$('x-post-confirm'),out=$('x-upload-result');
 if(!row?.from_sources)return;
 disarmIgPublish();
 if(arm)arm.disabled=true;
 if(conf)conf.disabled=true;
 if(out){out.dataset.busy='1';out.textContent='Uploading MP4 & publishing to X…';}
 try{
  const res=await get('/api/oauth/x/post-video/'+encodeURIComponent(row.clip_id),{
   method:'POST',headers:{'Content-Type':'application/json'},
   body:JSON.stringify({text:xPostText(),use_source:!row.exported,confirm_publish:true})
  });
  row.x_tweet_id=res.tweet_id;
  if(out){
   delete out.dataset.busy;
   const a=node('a','Open X Post ↗','oauth-link');a.href=res.post_url;a.target='_blank';a.rel='noopener noreferrer';
   out.replaceChildren('Published to X ('+res.tweet_id+'). ',a);
  }
  note('Published video to X!',[{label:'Open Post ↗',onClick:()=>window.open(res.post_url,'_blank','noopener')}]);
  renderClipShippedBanner(row);reload();loadExportHistory();
 }catch(e){
  if(out){delete out.dataset.busy;out.textContent='X upload failed: '+e.message;}
  note('X upload failed: '+e.message);
 }finally{if(conf)conf.disabled=false;syncYoutubeDraftButton();}
}
if($('x-post-confirm'))$('x-post-confirm').onclick=runXVideoPost;

if($('multi-ship-go'))$('multi-ship-go').onclick=async()=>{
 const row=selected,btn=$('multi-ship-go'),log=$('multi-ship-log');
 if(!row?.from_sources||btn.disabled)return;
 const targets=[];
 if($('ship-chk-yt')?.checked&&!$('ship-chk-yt').disabled)targets.push('youtube');
 if($('ship-chk-ig')?.checked&&!$('ship-chk-ig').disabled)targets.push('instagram');
 if($('ship-chk-tt')?.checked&&!$('ship-chk-tt').disabled)targets.push('tiktok');
 if($('ship-chk-x')?.checked&&!$('ship-chk-x').disabled)targets.push('x');
 if(!targets.length)return;
 const igLive=targets.includes('instagram')&&$('ship-ig-mode')?.value==='publish';
 const xLive=targets.includes('x');
 if((igLive||xLive)&&!window.confirm('This will publish LIVE to '+[igLive?'Instagram':null,xLive?'X':null].filter(Boolean).join(' & ')+'. Continue?'))return;
 btn.disabled=true;
 if(log){log.hidden=false;log.replaceChildren();}
 const addRow=(txt,cls)=>{const li=node('li',txt,cls);if(log)log.append(li);return li;};
 const common={title:$('title').value.trim()||row.name,caption:$('caption').value.trim(),tags:$('tags').value.trim()};
 const mats=pd.materialize(common,pdState?.overrides||{},pdState||{});
 let okCount=0;
 for(const t of targets){
  if(t==='youtube'){
   const li=addRow('⏳ Uploading YouTube draft…');
   try{
    const schedRaw=($('yt-draft-schedule')?.value||'').trim();
    const publish_at=schedRaw&&!Number.isNaN(new Date(schedRaw).getTime())?new Date(schedRaw).toISOString():null;
    const privacy=publish_at?'private':($('yt-draft-privacy')?.value||'private');
    const thumbData=Boolean($('yt-draft-thumb')?.checked&&coverUrl)?await coverBase64():null;
    const r=await get('/api/oauth/youtube/upload-draft/'+encodeURIComponent(row.clip_id),{
     method:'POST',headers:{'Content-Type':'application/json'},
     body:JSON.stringify({
      title:common.title,
      description:mats.youtube,
      tags:$('draft-yt-api-tags')?.value||common.tags,
      category_id:$('yt-draft-category')?.value||'20',
      made_for_kids:Boolean($('yt-draft-kids')?.checked),
      as_short:Boolean($('yt-draft-shorts')?$('yt-draft-shorts').checked:true),
      use_source:!row.exported,
      privacy,
      publish_at,
      thumbnail_png_base64:thumbData
     })
    });
    row.yt_video_id=r.video_id;okCount++;
    li.className='ok';li.textContent='✓ YouTube '+String(r.privacy||'private').toUpperCase()+(r.as_short?' Short':'')+' draft ('+r.video_id+')'+(r.shorts_note?' — '+r.shorts_note:'');
   }catch(e){li.className='err';li.textContent='✕ YouTube: '+e.message;}
  }else if(t==='instagram'){
   const li=addRow(igLive?'⏳ Publishing Instagram Reel…':'⏳ Uploading Instagram Test Container…');
   try{
    const r=await get('/api/oauth/instagram/upload-reel/'+encodeURIComponent(row.clip_id),{
     method:'POST',headers:{'Content-Type':'application/json'},
     body:JSON.stringify({caption:mats.instagram,share_to_feed:Boolean($('ig-upload-share')?$('ig-upload-share').checked:$('draft-ig-share')?.checked),thumb_offset_ms:pdState?.cover?.thumb_offset_ms??(Number($('ig-thumb-slider')?.value)||null),use_source:!row.exported,publish:igLive})
    });
    if(r.published&&r.media_id){row.ig_media_id=r.media_id;row.ig_permalink=r.permalink||'';}
    okCount++;
    li.className='ok';li.textContent=r.published?'✓ Instagram Reel published ('+r.media_id+')':'✓ Instagram Test Container ready ('+r.container_id+')';
   }catch(e){li.className='err';li.textContent='✕ Instagram: '+e.message;}
  }else if(t==='tiktok'){
   const li=addRow('⏳ Sending TikTok Inbox draft…');
   try{
    const r=await get('/api/oauth/tiktok/upload-draft/'+encodeURIComponent(row.clip_id),{
     method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({use_source:!row.exported})
    });
    row.tt_publish_id=r.publish_id;okCount++;
    li.className='ok';li.textContent='✓ TikTok Inbox draft ('+(r.status||'SENT')+')';
   }catch(e){li.className='err';li.textContent='✕ TikTok: '+e.message;}
  }else if(t==='x'){
   const li=addRow('⏳ Publishing video to X…');
   try{
    const r=await get('/api/oauth/x/post-video/'+encodeURIComponent(row.clip_id),{
     method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({text:xPostText(),use_source:!row.exported,confirm_publish:true})
    });
    row.x_tweet_id=r.tweet_id;okCount++;
    li.className='ok';li.textContent='✓ X Post published ('+r.tweet_id+')';
   }catch(e){li.className='err';li.textContent='✕ X: '+e.message;}
  }
 }
 note('Multi-platform ship finished: '+okCount+'/'+targets.length+' succeeded.');
 renderClipShippedBanner(row);await reload();loadExportHistory();syncYoutubeDraftButton();
};

function extractHashtags(text){
 const matches=String(text||'').match(/#[\p{L}\p{N}_]+/gu)||[];
 return Array.from(new Set(matches.map(s=>s.toLowerCase()))).join(' ');
}
function renderInsightsCharts(posts,snapshots){
 const platBox=$('insights-chart-platforms'),growthBox=$('insights-chart-growth'),tagBox=$('insights-hashtags'),timeBox=$('insights-best-times');
 if(platBox){
  const stats=[
   {id:'youtube',cls:'yt',label:'YouTube',views:0,likes:0,comments:0,count:0},
   {id:'instagram',cls:'ig',label:'Instagram',views:0,likes:0,comments:0,count:0},
   {id:'tiktok',cls:'tt',label:'TikTok',views:0,likes:0,comments:0,count:0},
  ];
  for(const p of posts){
   const s=stats.find(x=>x.id===p.provider);if(!s)continue;
   s.count++;s.views+=Number(p.views)||0;s.likes+=Number(p.likes)||0;s.comments+=Number(p.comments)||0;
  }
  const maxScore=Math.max(1,...stats.map(s=>s.views||s.likes*10||s.count));
  platBox.replaceChildren(...stats.map(s=>{
   const row=node('div',undefined,'bar-row');
   const meta=node('div',undefined,'bar-meta');
   const left=node('span');left.append(brandIcon(s.id),' '+s.label+' ('+s.count+')');
   const right=node('span',(s.views?s.views.toLocaleString()+' views · ':'')+s.likes.toLocaleString()+' likes · '+s.comments.toLocaleString()+' comments','muted');
   meta.append(left,right);
   const track=node('div',undefined,'bar-track');
   const fill=node('div',undefined,'bar-fill '+s.cls);
   const score=s.views||s.likes*10||s.count;
   fill.style.width=(s.count?Math.max(6,Math.round((score/maxScore)*100)):0)+'%';
   track.append(fill);row.append(meta,track);return row;
  }));
 }
 if(growthBox){
  const metric=$('growth-metric')?.value||'followers';
  const rows=Array.isArray(snapshots)?snapshots.filter(s=>s[metric]!=null):[];
  if(!rows.length){
   growthBox.replaceChildren(node('p','Sync analytics on multiple days to plot your follower and view growth curve here.','muted'));
  }else{
   const dates=Array.from(new Set(rows.map(r=>r.date))).sort();
   const byProv={youtube:[],instagram:[],tiktok:[]};
   for(const r of rows){if(byProv[r.provider])byProv[r.provider].push(r);}
   const vals=rows.map(r=>Number(r[metric])||0);
   const maxV=Math.max(1,...vals),minV=Math.min(0,...vals);
   const W=360,H=145,padL=34,padR=10,padT=12,padB=24;
   const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');
   svg.setAttribute('viewBox',`0 0 ${W} ${H}`);svg.setAttribute('class','chart-svg');
   const mk=(tag,attrs)=>{const el=document.createElementNS('http://www.w3.org/2000/svg',tag);for(const [k,v] of Object.entries(attrs))el.setAttribute(k,String(v));return el;};
   for(let i=0;i<=3;i++){
    const y=padT+((H-padT-padB)*i/3);
    svg.append(mk('line',{x1:padL,y1:y,x2:W-padR,y2:y,stroke:'rgba(255,255,255,0.08)','stroke-width':1}));
    const v=Math.round(maxV-((maxV-minV)*i/3));
    const t=mk('text',{x:padL-4,y:y+3,'text-anchor':'end',fill:'#8b95a5','font-size':9});t.textContent=v.toLocaleString();svg.append(t);
   }
   const colors={youtube:'#ff4d4d',instagram:'#e1306c',tiktok:'#25f4ee'};
   for(const [prov,list] of Object.entries(byProv)){
    if(!list.length)continue;
    const pts=list.map(r=>{
     const xi=dates.indexOf(r.date);
     const x=dates.length<=1?(padL+(W-padL-padR)/2):(padL+(W-padL-padR)*(xi/(dates.length-1)));
     const y=padT+(H-padT-padB)*(1-(((Number(r[metric])||0)-minV)/(maxV-minV||1)));
     return [x,y,r];
    });
    if(pts.length>1){
     svg.append(mk('polyline',{fill:'none',stroke:colors[prov],'stroke-width':2.2,points:pts.map(p=>p[0].toFixed(1)+','+p[1].toFixed(1)).join(' ')}));
    }
    for(const [x,y,r] of pts){
     const c=mk('circle',{cx:x,cy:y,r:3.5,fill:colors[prov]});
     const title=document.createElementNS('http://www.w3.org/2000/svg','title');
     title.textContent=prov+' · '+r.date+': '+(Number(r[metric])||0).toLocaleString();
     c.append(title);svg.append(c);
    }
   }
   const firstLbl=mk('text',{x:padL,y:H-6,fill:'#8b95a5','font-size':9});firstLbl.textContent=dates[0];svg.append(firstLbl);
   if(dates.length>1){const lastLbl=mk('text',{x:W-padR,y:H-6,'text-anchor':'end',fill:'#8b95a5','font-size':9});lastLbl.textContent=dates[dates.length-1];svg.append(lastLbl);}
   growthBox.replaceChildren(svg);
  }
 }
 if(tagBox){
  const map=new Map();
  for(const p of posts){
   const tags=extractHashtags((p.title||'')+' '+(p.caption||'')).split(/\s+/).filter(Boolean);
   const score=(Number(p.views)||0)+(Number(p.likes)||0)*10+(Number(p.comments)||0)*20;
   for(const t of tags){
    const cur=map.get(t)||{tag:t,uses:0,score:0,views:0};
    cur.uses++;cur.score+=score;cur.views+=Number(p.views)||0;
    map.set(t,cur);
   }
  }
  const top=Array.from(map.values()).sort((a,b)=>(b.score/b.uses)-(a.score/a.uses)).slice(0,12);
  if(!top.length){
   tagBox.replaceChildren(node('span','No hashtags found in synced posts yet.','muted'));
  }else{
   tagBox.replaceChildren(...top.map(t=>{
    const b=node('button',undefined,'hashtag-pill');b.type='button';
    b.append(t.tag+' ',node('small',Math.round(t.views/t.uses).toLocaleString()+' avg'));
    b.title='Click to add '+t.tag+' to Viewfinder Studio hashtags';
    b.onclick=()=>{
     $('tags').value=pd.mergeTags($('tags').value,t.tag);
     pdVersion++;refreshDraftEditor();renderPreflight();workflowDirty();
     note('Added '+t.tag+' to Studio hashtags.',[{label:'Open Studio',onClick:()=>{showView('studio',true);setInspectorTab('captions');}}]);
    };
    return b;
   }));
  }
 }
 if(timeBox){
  const dayNames=['Sun','Mon','Tue','Wed','Thu','Fri','Sat'];
  const buckets=new Map();
  for(const p of posts){
   if(!p.published_at)continue;
   const dt=new Date(p.published_at);if(Number.isNaN(dt.getTime()))continue;
   const key=dayNames[dt.getDay()]+' '+dt.toLocaleTimeString([],{hour:'numeric'});
   const cur=buckets.get(key)||{key,count:0,score:0};
   cur.count++;cur.score+=(Number(p.views)||0)+(Number(p.likes)||0)*10+1;
   buckets.set(key,cur);
  }
  const best=Array.from(buckets.values()).sort((a,b)=>(b.score/b.count)-(a.score/a.count)).slice(0,4);
  if(best.length){
   timeBox.replaceChildren(node('span','Top posting slots: '),...best.map(x=>node('span',x.key,'time-chip')));
  }else{
   timeBox.replaceChildren(node('span','Publish timestamps will highlight best posting hours here.','muted'));
  }
 }
}
if($('growth-metric'))$('growth-metric').onchange=()=>renderInsights();

function renderInsights(){
 const kpis=$('insights-kpis'),tbody=$('insights-rows'),empty=$('insights-empty');
 if(!kpis||!tbody)return;
 const acc=analyticsData?.accounts||{};
 const posts=Array.isArray(analyticsData?.posts)?analyticsData.posts:[];
 const snapshots=Array.isArray(analyticsData?.snapshots)?analyticsData.snapshots:[];
 const yt=acc.youtube, ig=acc.instagram, tt=acc.tiktok;
 const totalViews=posts.reduce((s,p)=>s+(Number(p.views)||0),0);
 const totalLikes=posts.reduce((s,p)=>s+(Number(p.likes)||0),0);
 const totalComments=posts.reduce((s,p)=>s+(Number(p.comments)||0),0);
 const makeCard=(prov,label,avatarUrl,val,sub)=>{
  const c=node('div',undefined,'kpi-card');
  const head=node('div',undefined,'kpi-head');
  const idWrap=node('span',undefined,'kpi-id');
  if(prov)idWrap.append(accountAvatar(prov,avatarUrl));
  idWrap.append(node('small',label));
  head.append(idWrap);
  c.append(head,node('strong',val),node('span',sub));
  return c;
 };
 kpis.replaceChildren(
  makeCard('youtube','YouTube',yt?.avatar_url,yt?.status==='connected'?(Number.isSafeInteger(yt.subscribers)?yt.subscribers.toLocaleString()+' subs':'Connected'):'Not connected',yt?.status==='connected'?([yt.account_name,yt.handle,Number.isSafeInteger(yt.media_count)?yt.media_count.toLocaleString()+' videos':null].filter(Boolean).join(' · ')||'Synced'):'Connect in Connections & System'),
  makeCard('instagram','Instagram',ig?.avatar_url,ig?.status==='connected'?(Number.isSafeInteger(ig.subscribers)?ig.subscribers.toLocaleString()+' followers':'Connected'):'Not connected',ig?.status==='connected'?([ig.handle||ig.account_name,Number.isSafeInteger(ig.media_count)?ig.media_count.toLocaleString()+' posts':null].filter(Boolean).join(' · ')||'Synced'):'Connect in Connections & System'),
  makeCard('tiktok','TikTok',tt?.avatar_url,tt?.status==='connected'?(Number.isSafeInteger(tt.subscribers)?tt.subscribers.toLocaleString()+' followers':'Connected'):'Not connected',tt?.status==='connected'?([tt.handle||tt.account_name,Number.isSafeInteger(tt.media_count)?tt.media_count.toLocaleString()+' videos':null].filter(Boolean).join(' · ')||'Synced'):'Connect in Connections & System'),
  makeCard(null,'Recent Views ('+posts.length+' posts)','',totalViews.toLocaleString(),totalLikes.toLocaleString()+' likes · '+totalComments.toLocaleString()+' comments')
 );
 renderInsightsCharts(posts,snapshots);
 const prov=$('insights-provider')?.value||'all';
 const sort=$('insights-sort')?.value||'recent';
 const q=($('insights-search')?.value||'').trim().toLocaleLowerCase();
 const list=posts.filter(p=>{
  if(prov!=='all'&&p.provider!==prov)return false;
  if(!q)return true;
  return [p.title,p.caption,p.provider,p.media_type].filter(Boolean).join(' ').toLocaleLowerCase().includes(q);
 }).slice();
 if(sort==='views')list.sort((a,b)=>(b.views||0)-(a.views||0));
 else if(sort==='likes')list.sort((a,b)=>(b.likes||0)-(a.likes||0));
 else if(sort==='comments')list.sort((a,b)=>(b.comments||0)-(a.comments||0));
 else list.sort((a,b)=>String(b.published_at||'').localeCompare(String(a.published_at||'')));
 tbody.replaceChildren(...list.map(p=>{
  const tr=node('tr');
  const postTd=node('td'),wrap=node('div',undefined,'insight-post');
  if(p.thumbnail_url){const im=node('img',undefined,'insight-thumb');im.src=p.thumbnail_url;im.alt='';im.loading='lazy';im.referrerPolicy='no-referrer';wrap.append(im);}
  const cap=node('div',undefined,'insight-caption');
  const titleText=p.title||'(Untitled)';
  cap.append(node('strong',titleText));
  if(p.caption&&p.caption!==p.title)cap.append(node('small',p.caption.slice(0,140)));
  wrap.append(cap);postTd.append(wrap);
  const platTd=node('td');
  const platName=p.provider==='youtube'?'YouTube':p.provider==='tiktok'?'TikTok':'Instagram';
  const platCls=p.provider==='youtube'?'ship-yt':p.provider==='tiktok'?'ship-tt':'ship-ig';
  const badge=node('span',undefined,'ship-badge '+platCls);badge.append(brandIcon(p.provider),' '+platName);
  platTd.append(badge);
  const pubStr=p.published_at?new Date(p.published_at).toLocaleDateString():'—';
  const actTd=node('td'),actWrap=node('div',undefined,'exp-actions');
  if(p.permalink){
   const a=node('a','Open ↗','oauth-link');a.href=p.permalink;a.target='_blank';a.rel='noopener noreferrer';
   actWrap.append(a);
  }
  const tags=extractHashtags((p.title||'')+' '+(p.caption||''));
  if(tags){
   const reuse=node('button','Use hashtags in Studio');reuse.type='button';reuse.title=tags;
   reuse.onclick=()=>{
    $('tags').value=pd.mergeTags($('tags').value,tags);
    pdVersion++;refreshDraftEditor();renderPreflight();workflowDirty();
    showView('studio',true);setInspectorTab('captions');
    note('Added winning hashtags to Studio caption desk.');
   };
   actWrap.append(reuse);
  }
  actTd.append(actWrap);
  tr.append(postTd,platTd,node('td',p.provider==='youtube'||p.views>0?Number(p.views||0).toLocaleString():'—'),node('td',Number(p.likes||0).toLocaleString()),node('td',Number(p.comments||0).toLocaleString()),node('td',pubStr),actTd);
  return tr;
 }));
 if(empty)empty.textContent=list.length?list.length+' synced post'+(list.length===1?'':'s')+' shown':posts.length?'No synced posts match this filter.':'No synced posts yet. Click Sync Live Analytics to pull recent YouTube uploads, Instagram posts, and TikTok videos.';
}
let calendarItems=[], calMonth=new Date(), calSelectedKey=null;
function dateKeyLocal(dt){return dt.getFullYear()+'-'+String(dt.getMonth()+1).padStart(2,'0')+'-'+String(dt.getDate()).padStart(2,'0');}
function itemDateKey(raw){
 if(!raw)return null;
 const dt=new Date(raw);if(!Number.isNaN(dt.getTime()))return dateKeyLocal(dt);
 const m=String(raw).match(/^(\d{4}-\d{2}-\d{2})/);
 return m?m[1]:null;
}
function renderCalDetail(key,grouped){
 const box=$('cal-day-detail');if(!box)return;
 const list=grouped.get(key)||[];
 const h=node('h3',key+' ('+list.length+' item'+(list.length===1?'':'s')+')');
 if(!list.length){
  box.replaceChildren(h,node('p','No published, scheduled, or queued items on this date.','muted'));
  return;
 }
 box.replaceChildren(h,...list.map(it=>{
  const card=node('div',undefined,'cal-item');
  const head=node('div',undefined,'cal-item-head');
  const left=node('span');left.append(brandIcon(it.provider),' '+(providerNames[it.provider]||it.provider));
  const kindLabel=it.kind==='published'?'Published':it.kind==='scheduled'?'Scheduled':'Queued';
  head.append(left,node('span',kindLabel,'ship-badge '+(it.kind==='published'?'ship-tt':it.kind==='scheduled'?'ship-yt':'ship-ig')));
  card.append(head,node('strong',it.title||'(Untitled)'));
  if(it.views||it.likes)card.append(node('small',(it.views?Number(it.views).toLocaleString()+' views · ':'')+Number(it.likes||0).toLocaleString()+' likes','muted'));
  if(it.permalink){
   const a=node('a','Open post ↗','oauth-link');a.href=it.permalink;a.target='_blank';a.rel='noopener noreferrer';
   card.append(a);
  }
  return card;
 }));
}
function renderCalendar(){
 const grid=$('cal-grid'),lbl=$('cal-month-label');
 if(!grid||!lbl)return;
 const prov=$('cal-provider')?.value||'all';
 const y=calMonth.getFullYear(),m=calMonth.getMonth();
 lbl.textContent=new Date(y,m,1).toLocaleDateString([],{month:'long',year:'numeric'});
 const grouped=new Map();
 for(const it of calendarItems){
  if(prov!=='all'&&it.provider!==prov)continue;
  const k=itemDateKey(it.date);if(!k)continue;
  if(!grouped.has(k))grouped.set(k,[]);
  grouped.get(k).push(it);
 }
 const todayKey=dateKeyLocal(new Date());
 if(!calSelectedKey)calSelectedKey=todayKey;
 const first=new Date(y,m,1);
 const start=new Date(y,m,1-first.getDay());
 const cells=[];
 for(let i=0;i<42;i++){
  const d=new Date(start.getFullYear(),start.getMonth(),start.getDate()+i);
  const k=dateKeyLocal(d);
  const list=grouped.get(k)||[];
  const cell=node('button',undefined,'cal-cell'+(d.getMonth()!==m?' outside':'')+(k===todayKey?' today':'')+(k===calSelectedKey?' selected':''));
  cell.type='button';
  const num=node('div',undefined,'cal-day-num');num.append(node('span',String(d.getDate())));
  if(list.length)num.append(node('small',String(list.length),'muted'));
  const chips=node('div',undefined,'cal-chips');
  for(const it of list.slice(0,3)){
   const c=node('span',(it.provider||'').slice(0,2).toUpperCase()+' · '+(it.title||'Clip'),'cal-chip '+(it.kind==='published'?'pub':it.kind==='scheduled'?'sched':'qd'));
   chips.append(c);
  }
  if(list.length>3)chips.append(node('small','+'+(list.length-3)+' more','muted'));
  cell.append(num,chips);
  cell.onclick=()=>{calSelectedKey=k;renderCalendar();};
  cells.push(cell);
 }
 grid.replaceChildren(...cells);
 renderCalDetail(calSelectedKey,grouped);
}
function loadCalendar(){
 return get('/api/oauth/calendar').then(d=>{
  calendarItems=Array.isArray(d.items)?d.items:[];
  renderCalendar();
 }).catch(()=>{});
}
if($('cal-prev'))$('cal-prev').onclick=()=>{calMonth=new Date(calMonth.getFullYear(),calMonth.getMonth()-1,1);renderCalendar();};
if($('cal-next'))$('cal-next').onclick=()=>{calMonth=new Date(calMonth.getFullYear(),calMonth.getMonth()+1,1);renderCalendar();};
if($('cal-today'))$('cal-today').onclick=()=>{calMonth=new Date();calSelectedKey=dateKeyLocal(new Date());renderCalendar();};
if($('cal-provider'))$('cal-provider').onchange=renderCalendar;

let insightsAutoSynced=false;
function loadInsights(allowAutoSync=false){
 if(analyticsLoad)return analyticsLoad;
 analyticsLoad=get('/api/oauth/analytics').then(d=>{
  analyticsData=d;renderInsights();
  if(allowAutoSync&&!insightsAutoSynced){
   const posts=Array.isArray(d?.posts)?d.posts:[];
   const connectedProvs=['youtube','instagram','tiktok'].filter(p=>oauthStatus?.[p]?.connected||d?.accounts?.[p]?.status==='connected');
   const missingPosts=connectedProvs.some(p=>!posts.some(x=>x.provider===p));
   if(missingPosts){insightsAutoSynced=true;setTimeout(()=>syncInsights(),0);}
  }
 }).catch(()=>{if($('insights-status'))$('insights-status').textContent='Could not load analytics.';}).finally(()=>{analyticsLoad=null;});
 return analyticsLoad;
}
async function syncInsights(){
 const btn=$('insights-sync'),out=$('insights-status');
 if(btn)btn.disabled=true;
 if(out)out.textContent='Syncing live stats from YouTube, Instagram & TikTok…';
 try{
  const r=await get('/api/oauth/analytics/sync',{method:'POST'});
  const notes=[];
  if(r.youtube_synced>0)notes.push('YouTube: '+r.youtube_synced+' videos');
  else if(r.youtube_error)notes.push('YouTube error: '+r.youtube_error);
  if(r.instagram_synced>0)notes.push('Instagram: '+r.instagram_synced+' posts');
  else if(r.instagram_error)notes.push('Instagram error: '+r.instagram_error);
  if(r.tiktok_synced>0)notes.push('TikTok: '+r.tiktok_synced+' videos');
  else if(r.tiktok_error)notes.push('TikTok error: '+r.tiktok_error);
  const msg=notes.length?'Synced '+notes.join(' · '):'Sync complete (connect YouTube, Instagram, or TikTok to pull posts).';
  if(out)out.textContent=msg;
  note(msg);
  await Promise.all([loadInsights(),refreshOAuthStatus(),loadChannelProfiles()]);
 }catch(e){
  if(out)out.textContent='Sync failed: '+e.message;
  note('Analytics sync failed: '+e.message);
 }finally{if(btn)btn.disabled=false;}
}
if($('insights-sync'))$('insights-sync').onclick=syncInsights;
for(const id of ['insights-provider','insights-sort']){if($(id))$(id).onchange=renderInsights;}
if($('insights-search'))$('insights-search').oninput=renderInsights;
$('export-diagnostics').onclick=async()=>{
 const b=$('export-diagnostics'),out=$('diagnostics-result');b.disabled=true;
 try{const r=await get('/api/workspace/diagnostics',{method:'POST'});out.textContent='Saved '+r.file+' to '+r.location+'. Nothing shared.';}catch(e){out.textContent=e.message;}finally{b.disabled=false;}
};
const providerNames={youtube:'YouTube',instagram:'Instagram',tiktok:'TikTok',x:'X'};
const guides={tiktok:['10% 18% 23% 7%','TikTok guide','18%','31%'],reels:['11% 16% 24% 7%','Reels guide','16%','32%'],shorts:['9% 18% 22% 7%','Shorts guide','18%','29%'],x:['8% 6% 18% 6%','X video guide','6%','18%']};
const drafts=new Map(), durations=new Map(), dimensions=new Map(), inspected=new Map([['tiktok',true],['instagram',true],['youtube',true]]);
let items=[],providers=[],selected=null,filter='all',galleryFilter='all',generation=0,refreshing=false,ledgerRows=[],exportHistoryRows=[],systemData=null,activeView='studio',indexReady=false,ledgerReady=false,indexLoad=null,ledgerLoad=null;
function node(tag,text,cls){const e=document.createElement(tag);if(text!==undefined)e.textContent=text;if(cls)e.className=cls;return e;}
async function get(url,opts){const r=await fetch(url,{cache:'no-store',credentials:'same-origin',redirect:'error',...opts});if(!r.ok){let reason='The local Hub did not accept this request.';try{const txt=(await r.text()).trim();if(txt){try{const j=JSON.parse(txt);reason=j.reason||j.error||j.message||txt;}catch{reason=txt;}}}catch{}throw Error(reason);}return r.json();}
for(const id of ['title','caption','tags'])$(id).addEventListener('input',renderPreflight);
get('/api/workspace/media-facts').then(d=>{mediaFacts=d;renderPreflight();updateCoverState();if(selected&&!editState)resetEdit(selected);}).catch(()=>{});
const pd=window.snapmaticDraft, ed=window.snapmaticEdit;
let editState=null;const editSessions=new Map();
const phase3Clip='d471e12c49d298ef7276d8b147160636f16da76ad3b71fe43727516f3f292707e';
let nvenc=null,nvencLoading=null,derivedMode=false;
const requestStable=x=>JSON.stringify(x,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.keys(v).sort().map(k=>[k,v[k]])):v);
function approvedSeed(){
 if(!editState||selected?.clip_id!==phase3Clip||!nvenc?.request||editState.planSeeded||editState.dirty)return;
 const a=nvenc.request;editState.duration_ms=20800;editState.in_ms=a.in_ms;editState.out_ms=a.out_ms;
 editState.framing={center:'center_crop',blur:'blurred_fit',dual_zone:'dual_zone'}[a.framing.mode]||'center_crop';
 editState.facecam=a.framing.face?.map((v,i)=>100*v/[1440,2560,1440,2560][i])||[];
 editState.gameplay=a.framing.game?.map((v,i)=>100*v/[1440,2560,1440,2560][i])||[];
 editState.edits={...a.edits};editState.planSeeded=true;
}
function requestFromEditor(){
 if(!editState||!nvenc?.request||selected?.clip_id!==phase3Clip||derivedMode)return null;
 const mode=editState.framing;
 let framing=mode==='center_crop'?{mode:'center'}:mode==='blurred_fit'?{mode:'blur'}:null;
 if(mode==='dual_zone'){
   const raw=ed.framing(mode,editState.facecam,editState.gameplay);
   if(!raw)return null;
   const px=v=>v.map((n,i)=>Math.floor(n*[1440,2560,1440,2560][i]/200)*2);
   framing={mode:'dual_zone',face:px(raw.facecam),game:px(raw.gameplay)};
 }
 if(!framing||!ed.trim(editState.in_ms,editState.out_ms,editState.duration_ms))return null;
 return {...nvenc.request,in_ms:editState.in_ms,out_ms:editState.out_ms,framing,edits:editState.edits||{}};
}
let exportPollTimer=null, exportJobActive=false, lastBatchActive=false;
function updateExportProgressUI(p){
 exportJobActive=Boolean(p?.active);
 const wrap=$('export-progress-wrap'),bar=$('export-progress'),txt=$('export-progress-text');
 const gBarWrap=$('gallery-batch-bar'),gBar=$('gallery-batch-progress'),gTxt=$('gallery-batch-text');
 const isBatch=Number(p?.batch_total)>0;
 if(wrap){
  wrap.hidden=!exportJobActive;
  if(bar)bar.value=Number(p?.percent)||0;
  if(txt)txt.textContent=(Number(p?.percent)||0)+'%'+(p?.encoder?' · '+p.encoder:'');
 }
 if(gBarWrap){
  gBarWrap.hidden=!(exportJobActive&&isBatch);
  if(isBatch){
   const overall=Math.min(100,Math.round((((Math.max(0,(p.batch_index||1)-1))*100)+(Number(p.percent)||0))/(p.batch_total||1)));
   if(gBar)gBar.value=overall;
   if(gTxt)gTxt.textContent=(p.message||'Exporting…')+' ('+overall+'%)';
  }
 }
 if(lastBatchActive&&!exportJobActive){
  lastBatchActive=false;
  if(p?.message)note(p.message);
  reload();refreshLocalStatus();loadExportHistory();
 }else if(exportJobActive&&isBatch){
  lastBatchActive=true;
 }
 if($('gallery-export-approved'))$('gallery-export-approved').disabled=exportJobActive||items.filter(r=>r.state==='approved').length===0;
}
function stopExportPoll(){if(exportPollTimer){clearInterval(exportPollTimer);exportPollTimer=null;}}
function startExportPoll(){
 if(exportPollTimer)return;
 const tick=()=>get('/api/workspace/film-roll/export-progress').then(p=>{
  updateExportProgressUI(p);
  if(!p.active)stopExportPoll();
 }).catch(()=>{});
 tick();
 exportPollTimer=setInterval(tick,350);
}
async function cancelActiveExport(){
 try{await fetch('/api/workspace/film-roll/export-cancel',{method:'POST',credentials:'same-origin'});note('Cancelling export…');}catch{}
}
if($('export-cancel'))$('export-cancel').onclick=cancelActiveExport;
if($('gallery-batch-cancel'))$('gallery-batch-cancel').onclick=cancelActiveExport;

const EXPORT_PREFS_KEY='snapmatic-export-prefs-v1';
function readExportPrefs(){try{return JSON.parse(localStorage.getItem(EXPORT_PREFS_KEY)||'{}')||{};}catch{return {};}}
function saveExportPrefs(){
 try{
  localStorage.setItem(EXPORT_PREFS_KEY,JSON.stringify({
   framing:$('edit-framing')?.value||'original',
   quality:$('edit-quality')?.value||'balanced',
   normalize_audio:Boolean($('edit-norm')?.checked),
   use_hw_encode:Boolean($('edit-hw')?.checked),
   save_caption_txt:Boolean($('edit-pack')?.checked),
   watermark_text:$('edit-watermark')?.value||'',
   watermark_pos:$('edit-wm-pos')?.value||'bottom_right',
   fade_io:Boolean($('edit-fade-io')?.checked),
   vivid_filter:Boolean($('edit-vivid')?.checked),
   export_speed:$('edit-export-speed')?.value||'1',
  }));
 }catch{}
}
function applyExportPrefs(){
 const p=readExportPrefs();
 if($('edit-quality')&&['balanced','high','fast'].includes(p.quality))$('edit-quality').value=p.quality;
 if($('edit-norm')&&typeof p.normalize_audio==='boolean')$('edit-norm').checked=p.normalize_audio;
 if($('edit-hw')&&typeof p.use_hw_encode==='boolean')$('edit-hw').checked=p.use_hw_encode;
 if($('edit-pack')&&typeof p.save_caption_txt==='boolean')$('edit-pack').checked=p.save_caption_txt;
 if($('edit-watermark')&&typeof p.watermark_text==='string')$('edit-watermark').value=p.watermark_text;
 if($('edit-wm-pos')&&['bottom_right','bottom_left','top_right','top_left'].includes(p.watermark_pos))$('edit-wm-pos').value=p.watermark_pos;
 if($('edit-fade-io')&&typeof p.fade_io==='boolean')$('edit-fade-io').checked=p.fade_io;
 if($('edit-vivid')&&typeof p.vivid_filter==='boolean')$('edit-vivid').checked=p.vivid_filter;
 if($('edit-export-speed')&&['1','1.1','1.25'].includes(String(p.export_speed)))$('edit-export-speed').value=String(p.export_speed);
}
function updateOverlayPreview(){
 const hook=$('edit-hook')?.value.trim()||'';
 const wm=$('edit-watermark')?.value.trim()||'';
 const pos=$('edit-wm-pos')?.value||'bottom_right';
 const hEl=$('vf-hook-overlay'), wEl=$('vf-wm-overlay');
 if(hEl){hEl.textContent=hook;hEl.hidden=!hook;}
 if(wEl){wEl.textContent=wm;wEl.dataset.pos=pos;wEl.hidden=!wm;}
}
applyExportPrefs();
updateOverlayPreview();
for(const id of ['edit-hook','edit-watermark']){if($(id))$(id).addEventListener('input',()=>{updateOverlayPreview();if(id==='edit-watermark')saveExportPrefs();});}
if($('edit-wm-pos'))$('edit-wm-pos').onchange=()=>{updateOverlayPreview();saveExportPrefs();};
for(const id of ['edit-quality','edit-norm','edit-hw','edit-pack','edit-fade-io','edit-vivid','edit-export-speed']){if($(id))$(id).addEventListener('change',saveExportPrefs);}
if($('edit-srt-file'))$('edit-srt-file').onchange=async e=>{
 const file=e.currentTarget.files?.[0];if(!file)return;
 try{
  const txt=await file.text();
  if($('edit-srt-text'))$('edit-srt-text').value=txt;
  note('Loaded subtitle file: '+file.name);
 }catch{note('Could not read .srt file.');}
};

function renderNvenc(){
 const isSourceClip=Boolean(selected?.from_sources);
 const target=selected?.clip_id===phase3Clip,b=$('edit-transcode'),open=$('edit-open-derived'),mute=$('edit-mute');
 const qual=$('edit-quality'),norm=$('edit-norm'),pack=$('edit-pack'),hw=$('edit-hw'),hwLbl=$('edit-hw-label');
 const hwName=localStatus?.hw_encoder||null;
 if(hwLbl)hwLbl.textContent=hwName?'Hardware encode ('+hwName+')':'Hardware encode (CPU libx264 only)';
 if(isSourceClip){
  const hasDuration=Boolean(editState&&editState.duration_ms>0);
  const ffmpegOk=localStatus?localStatus.ffmpeg_ready:true;
  if(mute)mute.disabled=!hasDuration;
  if(qual)qual.disabled=!hasDuration;
  if(norm)norm.disabled=!hasDuration||Boolean(mute?.checked);
  if(pack)pack.disabled=!hasDuration;
  if($('edit-fade-io'))$('edit-fade-io').disabled=!hasDuration;
  if($('edit-vivid'))$('edit-vivid').disabled=!hasDuration;
  if($('edit-export-speed'))$('edit-export-speed').disabled=!hasDuration;
  if($('trim-detect-silence'))$('trim-detect-silence').disabled=!hasDuration||!ffmpegOk;
  if(hw){hw.disabled=!hasDuration||!hwName;if(!hwName)hw.checked=false;}
  if($('queue-send'))$('queue-send').disabled=!$('queue-profile')?.value;
  b.textContent='Export MP4 (FFmpeg)';
  b.disabled=!hasDuration||!ffmpegOk||exportJobActive;
  open.hidden=!selected.exported;
  open.textContent=derivedMode?'Back to original':'Play exported MP4';
  $('edit-state').textContent=!ffmpegOk?'FFmpeg not found in tools/ffmpeg.exe.':selected.exported?'Exported locally: '+(selected.exported_file||'ready')+'. Original untouched.':'Set optional trim, framing, overlay & quality, then export locally. Original stays untouched.';
  return;
 }
 if(mute)mute.disabled=true;
 if(qual)qual.disabled=true;
 if(norm)norm.disabled=true;
 if(pack)pack.disabled=true;
 if($('edit-fade-io'))$('edit-fade-io').disabled=true;
 if($('edit-vivid'))$('edit-vivid').disabled=true;
 if($('edit-export-speed'))$('edit-export-speed').disabled=true;
 if($('trim-detect-silence'))$('trim-detect-silence').disabled=true;
 if(hw)hw.disabled=true;
 if($('queue-send'))$('queue-send').disabled=true;
 const r=target?requestFromEditor():null;
 const matches=r&&requestStable(r)===requestStable(nvenc.request);
 b.disabled=!target||!nvenc?.can_dispatch||!matches;
 const audio=target?nvenc?.receipt?.audio:null;
 $('edit-lufs').textContent=audio?.status==='measured'?Number(audio.integrated_lufs).toFixed(2)+' LUFS':audio?.status==='silence'?'Measured silence':'Not measured';
 $('edit-peak').textContent=audio?.status==='measured'?Number(audio.true_peak_dbtp).toFixed(2)+' dBTP':audio?.status==='silence'?'Measured silence':'Not measured';
 open.hidden=!(target&&nvenc?.stage==='ready'&&/^[a-f0-9]{64}$/.test(nvenc?.job_id||''));
 $('edit-state').textContent=!target?'Select a Film Roll clip to trim and export locally.':
   nvenc?.can_dispatch?(matches?'Exact reviewed source, trim and framing. GPU readiness is rechecked before encoding.':'Edits differ from the independently reviewed request. Export is held.'):
   (ed.stageLabels[nvenc?.stage]||(nvenc?.reason==='preview_plan_unconfigured'?'No independently verified local worker plan.':'Local preview not available'))+
   (nvenc?.failure?' · '+nvenc.failure:'');
}
if($('edit-mute'))$('edit-mute').onchange=()=>renderNvenc();
function showTranscript(){
 const t=$('edit-transcript');
 const segments=selected?.clip_id===phase3Clip&&nvenc?.request?.transcript_sha256?
   ed.grounded(nvenc.segments,editState?.duration_ms):null;
 t.replaceChildren();
 if(!segments){$('edit-transcript-state').textContent='No verified transcript loaded. This does not mean no speech.';return;}
 $('edit-transcript-state').textContent=segments.length+' grounded speech segments · timestamps are immutable';
 for(const s of segments){
   const label=node('label',time(s.start_ms/1000)+' to '+time(s.end_ms/1000));
   const input=node('textarea');input.rows=2;input.maxLength=300;input.value=editState?.edits?.[s.id]??s.text;
   input.addEventListener('input',()=>{
       if(!editState)return;editState.dirty=true;
       const e=editState.edits||(editState.edits={});
       if(input.value===s.text)delete e[s.id];else e[s.id]=input.value;
       renderNvenc();
   });label.append(input);t.append(label);
 }
}
function loadNvenc(){
 if(nvencLoading)return nvencLoading;
 nvencLoading=get('/api/workspace/local-preview').then(v=>{
   nvenc=v;
   if(selected?.clip_id===phase3Clip){approvedSeed();updateTrim();showTranscript();}
   renderNvenc();if(activeView==='gallery')renderGallery();phase5Panel();
 }).catch(()=>{}).finally(()=>{nvencLoading=null;});return nvencLoading;
}
async function coverBase64(){
 if(!coverUrl)return null;
 try{
  const blob=await fetch(coverUrl).then(r=>r.blob());
  return await new Promise((ok,no)=>{const fr=new FileReader();fr.onload=()=>ok(String(fr.result||''));fr.onerror=()=>no();fr.readAsDataURL(blob);});
 }catch{return null;}
}
function buildCaptionPackText(){
 const common={title:$('title').value.trim(),caption:$('caption').value.trim(),tags:$('tags').value.trim()};
 const mats=pd.materialize(common,pdState?.overrides||{},pdState||{});
 const parts=[];
 if(common.title)parts.push('TITLE:\n'+common.title);
 parts.push('YOUTUBE SHORTS:\n'+mats.youtube);
 parts.push('INSTAGRAM REELS:\n'+mats.instagram);
 parts.push('TIKTOK:\n'+mats.tiktok);
 return parts.join('\n\n---\n\n');
}
if($('copy-all-captions'))$('copy-all-captions').onclick=async()=>{
 try{await navigator.clipboard.writeText(buildCaptionPackText());note('Copied all platform captions (YouTube, Instagram & TikTok).');}
 catch{note('Clipboard unavailable. Select and copy manually.');}
};
$('edit-transcode').onclick=async()=>{
 const button=$('edit-transcode');
 if(selected?.from_sources){
  const row=selected;
  if(button.disabled||!editState)return;
  button.disabled=true;$('edit-state').textContent='Rendering local MP4 with FFmpeg…';
  startExportPoll();
  try{
   const savePack=Boolean($('edit-pack')?.checked);
   const body={
    in_ms:editState.in_ms||0,
    out_ms:editState.out_ms||editState.duration_ms,
    framing:$('edit-framing').value||'original',
    crop_pan:(Number($('edit-crop-pan')?.value)||0)/100,
    quality:$('edit-quality')?.value||'balanced',
    mute:Boolean($('edit-mute')?.checked),
    normalize_audio:Boolean($('edit-norm')?.checked)&&!Boolean($('edit-mute')?.checked),
    use_hw_encode:Boolean($('edit-hw')?.checked),
    save_caption_txt:savePack,
    caption_text:savePack?buildCaptionPackText():null,
    cover_png_base64:savePack?await coverBase64():null,
    hook_text:($('edit-hook')?.value||'').trim()||null,
    outro_cta:($('edit-outro-cta')?.value||'').trim()||null,
    watermark_text:($('edit-watermark')?.value||'').trim()||null,
    watermark_pos:$('edit-wm-pos')?.value||'bottom_right',
    fade_io:Boolean($('edit-fade-io')?.checked),
    vivid_filter:Boolean($('edit-vivid')?.checked),
    speed:Number($('edit-export-speed')?.value)||1,
    rotate_deg:Number($('edit-rotate')?.value)||0,
    mirror_h:Boolean($('edit-mirror')?.checked),
    subtle_zoom:Boolean($('edit-zoom')?.checked),
    force_30fps:Boolean($('edit-force-30fps')?.checked),
    audio_preset:$('edit-audio-preset')?.value||'none',
    audio_gain_db:Number($('edit-audio-gain')?.value)||0,
    music_path:window.snapmaticProMusicPath||null,
    music_volume_db:Number($('edit-music-vol')?.value)||-18,
    srt_text:($('edit-srt-text')?.value||'').trim()||null,
    subtitle_style:$('edit-srt-style')?.value||'bold_yellow',
   };
   const res=await get('/api/workspace/film-roll/'+encodeURIComponent(row.clip_id)+'/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
   row.exported=true;row.exported_file=res.file_name;row.export_url=res.export_url;row.state='exported';
   if(editState)editState.dirty=false;
   window.dispatchEvent(new CustomEvent('snapmatic:exported',{detail:{clip_id:row.clip_id}}));
   const extras=[res.caption_file,res.cover_file].filter(Boolean);
   const packNote=extras.length?' (+ '+extras.join(', ')+')':'';
   const encNote=res.encoder?' · '+res.encoder:'';
   $('edit-state').textContent='Exported '+res.file_name+packNote+' ('+(res.bytes/1048576).toFixed(1)+' MiB'+encNote+'). Original untouched.';
   note('Exported '+res.file_name+packNote+encNote+'.',[
    {label:'Open Folder',onClick:()=>$('edit-open-exports')?.click()},
    {label:'Upload →',onClick:()=>setInspectorTab('upload')}
   ]);
   await reload();refreshLocalStatus();loadExportHistory();renderNvenc();phase5Panel();workFlow();
  }catch(e){$('edit-state').textContent='Export failed: '+e.message;note('Export: '+e.message);}
  finally{stopExportPoll();updateExportProgressUI({active:false});renderNvenc();}
  return;
 }
 const r=requestFromEditor();
 if(button.disabled||!r)return;
 button.disabled=true;
 try{const result=await get('/api/workspace/local-preview',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(r)});
   if(result.publishing_enabled!==false)throw Error('Unexpected publishing status');
   nvenc={...nvenc,...result,can_dispatch:false};renderNvenc();
 }catch(e){$('edit-state').textContent='Held: '+e.message;}
};
$('edit-open-derived').onclick=()=>{
 if($('edit-open-derived').hidden||!selected)return;
 if(selected.from_sources&&selected.export_url){
  if(derivedMode){select(selected);return;}
  generation++;cancelFrameWatch();stopVideo();derivedMode=true;
  video.src=selected.export_url+'?t='+Date.now();video.load();
  $('inspect-state').textContent='Playing exported MP4 · '+selected.exported_file;
  note('Playing exported MP4 (original untouched).');
  renderNvenc();phase5Panel();
  return;
 }
 if(!nvenc||selected?.clip_id!==phase3Clip)return;
 generation++;cancelFrameWatch();stopVideo();derivedMode=true;editState=null;updateTrim();pdState=null;refreshDraftEditor();
 video.src='/api/workspace/local-preview/'+encodeURIComponent(nvenc.job_id)+'/video';video.load();
 $('inspect-state').textContent='Verified H.264 local derivative only · original unchanged';
 renderNvenc();
};
if($('edit-open-exports'))$('edit-open-exports').onclick=async()=>{try{const r=await fetch('/api/workspace/film-roll/open-exports',{method:'POST',credentials:'same-origin'});if(!r.ok)throw Error();note('Opened local exports folder.');}catch{note('Could not open exports folder.');}};

function knownDuration(row){
 if(!row)return null;
 if(row.from_sources){
  const d=durations.get(row.clip_id)||(selected?.clip_id===row.clip_id&&Number.isFinite(video.duration)&&video.duration>0?video.duration:null);
  return d&&d>0?Math.ceil(d*1000):null;
 }
 const f=mediaFacts[row?.preview_sha256]?.format,d=Number(f?.duration);
 return row&&Number(f?.size)===row.preview_bytes&&d>0&&Number.isFinite(d)?Math.ceil(d*1000):null;
}
function resetEdit(row){
 const savedFraming=readExportPrefs().framing;
 const defFraming=row?.from_sources&&['original','center_crop','blurred_fit'].includes(savedFraming)?savedFraming:(row?.from_sources?'original':'center_crop');
 const d=row?.clip_id===phase3Clip?20800:knownDuration(row),key=row?.clip_id+':'+row?.preview_sha256;
 editState=d?(editSessions.get(key)||{clip_id:row.clip_id,media_sha256:row.preview_sha256,duration_ms:d,in_ms:0,out_ms:d,framing:defFraming,facecam:[],gameplay:[]}):null;
 if(editState)editSessions.set(key,editState);approvedSeed();
 $('edit-framing').value=editState?.framing||defFraming;$('edit-zones').hidden=$('edit-framing').value!=='dual_zone';
 if($('edit-mute'))$('edit-mute').checked=false;
 for(const name of ['facecam','gameplay'])document.querySelectorAll('#edit-'+name+' input').forEach((x,i)=>x.value=editState?.[name]?.[i]??'');
 showTranscript();updateTrim();renderNvenc();
}
function updateTrim(){
 const enabled=Boolean(editState),d=editState?.duration_ms||0;
 for(const id of ['trim-in','trim-out']){
  const a=$(id),ms=id==='trim-in'?(editState?.in_ms||0):(editState?.out_ms||0);
  a.disabled=!enabled;a.max=String(d);a.value=String(ms);a.setAttribute('aria-valuetext',time(ms/1000));
 }
 $('trim-in-here').disabled=!enabled||video.readyState<2;$('trim-out-here').disabled=!enabled||video.readyState<2;
 $('edit-framing').disabled=!enabled;
 $('trim-status').textContent=enabled?'Source '+time(editState.in_ms/1000)+' to '+time(editState.out_ms/1000)+' · original untouched':'No verified source duration';
 const vis=$('trim-visual');
 if(vis){
  vis.hidden=!(enabled&&d>0);
  if(enabled&&d>0){
   const pIn=Math.max(0,Math.min(100,(editState.in_ms/d)*100));
   const pOut=Math.max(pIn,Math.min(100,(editState.out_ms/d)*100));
   const rng=$('trim-range'),hIn=$('trim-handle-in'),hOut=$('trim-handle-out');
   if(rng){rng.style.left=pIn+'%';rng.style.width=(pOut-pIn)+'%';}
   if(hIn)hIn.style.left=pIn+'%';
   if(hOut)hOut.style.left=pOut+'%';
   if($('trim-visual-range'))$('trim-visual-range').textContent=time(editState.in_ms/1000)+' → '+time(editState.out_ms/1000);
   if($('trim-visual-dur'))$('trim-visual-dur').textContent='Clip: '+time(Math.max(0,editState.out_ms-editState.in_ms)/1000);
  }
 }
}
function bindTrimDrag(handleId,which){
 const h=$(handleId),track=$('trim-track');
 if(!h||!track)return;
 h.addEventListener('pointerdown',e=>{
  if(!editState||editState.duration_ms<=0)return;
  e.preventDefault();h.setPointerCapture(e.pointerId);
  const move=ev=>{
   const rect=track.getBoundingClientRect();
   if(rect.width<=0)return;
   const ratio=Math.max(0,Math.min(1,(ev.clientX-rect.left)/rect.width));
   const ms=Math.round(ratio*editState.duration_ms);
   setTrim(which,ms);
   if(Number.isFinite(video.duration)&&video.duration>0)video.currentTime=Math.max(0,Math.min(video.duration,ms/1000));
  };
  const up=()=>{h.removeEventListener('pointermove',move);h.removeEventListener('pointerup',up);h.removeEventListener('pointercancel',up);workflowDirty();};
  h.addEventListener('pointermove',move);h.addEventListener('pointerup',up);h.addEventListener('pointercancel',up);
 });
}
bindTrimDrag('trim-handle-in','in');
bindTrimDrag('trim-handle-out','out');
if($('trim-track'))$('trim-track').addEventListener('pointerdown',e=>{
 if(e.target.closest('.trim-handle')||!editState||editState.duration_ms<=0)return;
 const rect=$('trim-track').getBoundingClientRect();
 if(rect.width<=0)return;
 const ratio=Math.max(0,Math.min(1,(e.clientX-rect.left)/rect.width));
 if(Number.isFinite(video.duration)&&video.duration>0)video.currentTime=ratio*video.duration;
});
if($('trim-detect-silence'))$('trim-detect-silence').onclick=async()=>{
 const row=selected,btn=$('trim-detect-silence'),out=$('silence-status');
 if(!row?.from_sources||!editState||btn.disabled)return;
 btn.disabled=true;if(out)out.textContent='Analyzing audio track for dead air…';
 try{
  const r=await get('/api/workspace/film-roll/'+encodeURIComponent(row.clip_id)+'/detect-silence',{method:'POST'});
  const trimmedStart=Number(r.trimmed_start_ms)||0, trimmedEnd=Number(r.trimmed_end_ms)||0;
  if(trimmedStart<100&&trimmedEnd<100){
   if(out)out.textContent='Audio starts and ends cleanly — no leading/trailing silence detected.';
   note('No leading or trailing dead air detected.');
  }else{
   const nextIn=Math.max(0,Math.min(editState.duration_ms-200,Number(r.suggested_in_ms)||0));
   const nextOut=Math.max(nextIn+200,Math.min(editState.duration_ms,Number(r.suggested_out_ms)||editState.duration_ms));
   const msg='Suggested trim: cut '+(trimmedStart/1000).toFixed(2)+'s start, '+(trimmedEnd/1000).toFixed(2)+'s end.';
   if(out)out.textContent=msg;
   setTrim('in',nextIn);setTrim('out',nextOut);workflowDirty();
   note('Auto-trimmed dead air ('+time(nextIn/1000)+' → '+time(nextOut/1000)+').');
  }
 }catch(e){if(out)out.textContent='Silence detect failed: '+e.message;note(e.message);}
 finally{renderNvenc();}
};
function setTrim(which,at){
 if(!editState)return;
 const t=ed.trim(which==='in'?at:editState.in_ms,which==='out'?at:editState.out_ms,editState.duration_ms);
 if(!t){updateTrim();$('trim-status').textContent='Invalid range: In must be before Out, inside this source.';return;}
 editState.dirty=true;Object.assign(editState,t);updateTrim();renderNvenc();
 if(pdState?.cover){
   const c=pdState.cover,next=pd.mapCover((c.frame_ms+0.5)/1000,t.in_ms,t.out_ms,editState.duration_ms/1000);
   if(!next){pdState.cover=null;if(coverUrl){URL.revokeObjectURL(coverUrl);coverUrl=null;}$('cover-image').hidden=true;$('cover-download').hidden=true;$('cover-offset').textContent='Previous cover is outside trim; recapture before export.';}
   else{pdState.cover=next;updateCoverState();}
 }
}
for(const which of ['in','out']){
 $('trim-'+which).oninput=e=>setTrim(which,Number(e.currentTarget.value));
 $('trim-'+which+'-here').onclick=()=>{
  if(video.readyState>=2&&Number.isFinite(video.currentTime))setTrim(which,Math.floor(video.currentTime*1000));
 };
}
$('edit-framing').onchange=()=>{if(editState){editState.framing=$('edit-framing').value;editState.dirty=true;}$('edit-zones').hidden=$('edit-framing').value!=='dual_zone';saveExportPrefs();renderNvenc();};
for(const name of ['facecam','gameplay'])document.querySelectorAll('#edit-'+name+' input').forEach((x,i)=>x.addEventListener('input',()=>{if(editState){editState[name][i]=x.value===''?null:Number(x.value);editState.dirty=true;}renderNvenc();}));
document.addEventListener('keydown',e=>{
 if(activeView!=='studio'||e.ctrlKey||e.altKey||e.metaKey||e.repeat||e.target.closest('input,textarea,select,[contenteditable]'))return;
 const k=e.key.toLowerCase(),b=k==='i'?$('trim-in-here'):k==='o'?$('trim-out-here'):null;
 if(b&&!b.disabled){e.preventDefault();b.click();}
});
const pdCache=new Map();
let pdState=null,pdVersion=0, frameToken=null, frameRequest=null,coverUrl=null;
const emptyPD=()=>({overrides:{youtube:null,instagram:null,tiktok:null},shorts:false,trial:false,cover:null,share_to_feed:false,youtube_api_tags:""});
function refreshDraftEditor(){
 const state=pdState, ok=Boolean(state);
 $('platform-draft-save').disabled=!ok;$('draft-preset').disabled=!ok;
 $('draft-shorts').disabled=!ok;$('draft-trial').disabled=!ok;
 $('draft-ig-share').disabled=!ok;$('draft-ig-share').checked=Boolean(state?.share_to_feed);
 const ytTags=$('draft-yt-api-tags');ytTags.disabled=!ok;if(document.activeElement!==ytTags)ytTags.value=state?.youtube_api_tags||'';
 const tagCost=pd.youtubeTagCount(state?.youtube_api_tags||'');
 const tagOut=$('draft-yt-api-count');tagOut.textContent=tagCost+'/500 YouTube API tag characters (commas and space-tag quotes count)';tagOut.className=tagCost>500?'warn':'muted';
 const hasVideo=video.readyState>=1&&video.videoWidth>0&&video.videoHeight>0;
 const duration=hasVideo?video.duration:NaN;
 $('draft-yt-shorts').textContent=pd.youtubeShorts(duration,video.videoWidth,video.videoHeight);
 $('draft-ig-placement').hidden=!(duration>90);
 refreshTikTokExport();
 if(tagCost>500)$('platform-draft-save').disabled=true;
 $('draft-shorts').checked=Boolean(state?.shorts);$('draft-trial').checked=Boolean(state?.trial);
 const common={title:$('title').value,caption:$('caption').value,tags:$('tags').value};
 const rows=pd.materialize(common,state?.overrides||{},state||{});
 $('draft-yt-title').textContent=pd.charCount(common.title)+'/100 title characters | category 20 (Gaming) | made for kids: No';
 for(const key of pd.keys){
   const edit=$('draft-'+key), use=$('draft-'+key+'-override');
   use.disabled=!ok;use.checked=state?.overrides[key]!=null;edit.disabled=!ok||!use.checked;
   const now=use.checked?state.overrides[key]:rows[key];if(edit.value!==now&&document.activeElement!==edit)edit.value=now;
   const output=rows[key];
   const label=key==='youtube'?pd.byteCount(output)+'/5000 UTF-8 description bytes':
     key==='instagram'?pd.charCount(output)+'/2200 characters · '+pd.hashtagCount(output)+'/30 hashtags':
     output.length+'/2200 UTF-16 units (editor aid, not sent to Inbox)';
   const over=key==='youtube'?pd.byteCount(output)>5000||pd.charCount(common.title)>100:
      key==='instagram'?pd.charCount(output)>2200||pd.hashtagCount(output)>30:output.length>2200;
   const count=$('draft-'+key+'-count');count.textContent=label;count.className=over?'warn':'muted';
 }
 $('draft-yt-audio').hidden=!(video.duration>60);$('draft-ig-discovery').hidden=!(video.duration>180);
 if($('draft-x')&&document.activeElement!==$('draft-x')&&!$('draft-x').value){
  const xt=xPostText();
  if($('draft-x-count')){$('draft-x-count').textContent=xt.length+'/280 characters (auto from caption)';$('draft-x-count').className='muted';}
 }
 updateMockOverlay();
 updateCoverState();
}
for(const key of pd.keys){
 const edit=$('draft-'+key),use=$('draft-'+key+'-override');
 use.onchange=()=>{if(!pdState)return;pdVersion++;pdState.overrides[key]=use.checked?$('caption').value:null;refreshDraftEditor();};
 edit.addEventListener('input',()=>{if(pdState&&use.checked){pdVersion++;pdState.overrides[key]=edit.value;refreshDraftEditor();}});
}
for(const id of ['title','caption','tags'])$(id).addEventListener('input',()=>{pdVersion++;refreshDraftEditor();});
$('draft-shorts').onchange=()=>{if(pdState){pdVersion++;pdState.shorts=$('draft-shorts').checked;refreshDraftEditor();}};
$('draft-trial').onchange=()=>{if(pdState){pdVersion++;pdState.trial=$('draft-trial').checked;refreshDraftEditor();}};
$('draft-ig-share').onchange=()=>{if(pdState){pdVersion++;pdState.share_to_feed=$('draft-ig-share').checked;refreshDraftEditor();}};
$('draft-yt-api-tags').oninput=()=>{if(pdState){pdVersion++;pdState.youtube_api_tags=$('draft-yt-api-tags').value;refreshDraftEditor();}};
let captionPresets=[];
function renderPresetSelect(selectId){
 const sel=$('preset-select'),del=$('preset-delete');
 if(!sel)return;
 const prev=selectId!=null?String(selectId):sel.value;
 if(!captionPresets.length){
  const o=node('option','No saved presets');o.value='';sel.replaceChildren(o);
  if(del)del.disabled=true;
  return;
 }
 sel.replaceChildren(...captionPresets.map(p=>{const o=node('option',p.name);o.value=String(p.id);return o;}));
 if(prev&&captionPresets.some(p=>String(p.id)===prev))sel.value=prev;
 if(del)del.disabled=!sel.value;
}
function loadPresets(selectId){
 return get('/api/workspace/caption-presets').then(d=>{
  captionPresets=Array.isArray(d.presets)?d.presets:[];
  renderPresetSelect(selectId);
 }).catch(()=>{});
}
if($('preset-select'))$('preset-select').onchange=()=>{if($('preset-delete'))$('preset-delete').disabled=!$('preset-select').value;};
$('draft-preset').onclick=()=>{
 if(!pdState)return;
 const selId=Number($('preset-select')?.value);
 const preset=captionPresets.find(p=>p.id===selId);
 pdVersion++;
 if(preset){
  if(preset.title_prefix&&!$('title').value.startsWith(preset.title_prefix))$('title').value=(preset.title_prefix+' '+$('title').value).trim();
  if(preset.caption_body){
   const cur=$('caption').value.trim();
   $('caption').value=cur?(cur.includes(preset.caption_body)?cur:cur+'\n\n'+preset.caption_body):preset.caption_body;
  }
  if(preset.tags)$('tags').value=pd.mergeTags($('tags').value,preset.tags);
  note('Applied preset "'+preset.name+'".');
 }else{
  $('tags').value=pd.mergeTags($('tags').value,pd.preset);
 }
 refreshDraftEditor();renderPreflight();workflowDirty();
};
if($('preset-save'))$('preset-save').onclick=async()=>{
 const nameEl=$('preset-name'),btn=$('preset-save');
 const name=(nameEl?.value||'').trim();
 if(!name){note('Enter a preset name first.');nameEl?.focus();return;}
 btn.disabled=true;
 try{
  const r=await get('/api/workspace/caption-presets',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({name,title_prefix:'',caption_body:$('caption').value.trim(),tags:$('tags').value.trim()})});
  if(nameEl)nameEl.value='';
  await loadPresets(r.id);
  note('Saved caption preset "'+r.name+'".');
 }catch(e){note(e.message);}
 finally{btn.disabled=false;}
};
if($('preset-delete'))$('preset-delete').onclick=async()=>{
 const sel=$('preset-select'),btn=$('preset-delete');
 if(!sel?.value)return;
 btn.disabled=true;
 try{
  await fetch('/api/workspace/caption-presets/'+encodeURIComponent(sel.value),{method:'DELETE',credentials:'same-origin'});
  await loadPresets();
  note('Deleted caption preset.');
 }catch(e){note(e.message);}
 finally{if(btn)btn.disabled=!sel.value;}
};
function draftPath(row){return '/api/clips/'+encodeURIComponent(row.clip_id)+'/platform-draft';}
function pdKey(row){return row.clip_id+':'+row.preview_sha256;}
function loadPlatformDraft(row,turn){
 pdState=row.preview_available&&/^[a-f0-9]{64}$/.test(row.preview_sha256||'')?(pdCache.get(pdKey(row))||emptyPD()):null;
 $('platform-draft-result').textContent=pdState?'Local only. Not uploaded.':'This clip has no verified editable preview.';
 refreshDraftEditor();
 if(!pdState)return;
 const version=pdVersion;
 get(draftPath(row)).then(r=>{
   if(turn!==generation||pdVersion!==version||pdCache.has(pdKey(row))||!r.draft)return;
   if(r.draft.media_sha256!==row.preview_sha256)return;
   const d=r.draft;pdState=d;$('title').value=d.title;$('caption').value=d.caption;$('tags').value=d.tags;refreshDraftEditor();renderPreflight();
 }).catch(()=>{if(turn===generation)$('platform-draft-result').textContent='Local draft not available. Unsaved edits remain here.';});
}
$('platform-draft-save').onclick=async()=>{
 if(!selected||!pdState)return;
 const row=selected,turn=generation;
 const saved={media_sha256:row.preview_sha256,title:$('title').value,caption:$('caption').value,tags:$('tags').value,
    overrides:pdState.overrides,shorts:pdState.shorts,trial:pdState.trial,cover:pdState.cover,share_to_feed:!!pdState.share_to_feed,youtube_api_tags:pdState.youtube_api_tags||""};
 const b=$('platform-draft-save');b.disabled=true;
 try{await get(draftPath(row),{method:'PUT',headers:{'Content-Type':'application/json'},body:JSON.stringify(saved)});if(turn===generation)$('platform-draft-result').textContent='Saved local draft. Nothing uploaded.';}
 catch(e){if(turn===generation)$('platform-draft-result').textContent=e.message;}
 finally{if(turn===generation)refreshDraftEditor();}
};
function updateCoverState(){
 const exact=mediaFacts[selected?.preview_sha256]?.format;
 const real=selected&&(selected.from_sources||ttSHA===selected.preview_sha256?(Number.isFinite(video.duration)&&video.duration>0?video.duration:null):Number(exact?.size)===selected.preview_bytes?Number(exact.duration):null);
 const hasFrame=selected?.from_sources?(video.readyState>=2&&!video.seeking):Boolean(frameToken&&frameToken.turn===generation&&!video.seeking&&video.readyState>=2);
 $('cover-shutter').disabled=!pdState||!hasFrame||!real;
 const prior=pdState?.cover;
 $('cover-offset').textContent=prior?'Cover frame '+prior.frame_ms+' ms · Instagram thumb offset '+prior.thumb_offset_ms+' ms':'Cover not captured';
}
function cancelFrameWatch(){
 if(frameRequest!=null)video.cancelVideoFrameCallback?.(frameRequest);
 frameRequest=null;frameToken=null;
 if(coverUrl){URL.revokeObjectURL(coverUrl);coverUrl=null;}
 $('cover-image').hidden=true;$('cover-image').removeAttribute('src');$('cover-download').hidden=true;
}
function watchFrames(row,turn){
 if(frameRequest!=null||!video.requestVideoFrameCallback)return;
 const cb=(_,meta)=>{
  frameRequest=null;
  if(turn!==generation||selected?.clip_id!==row.clip_id)return;
  if(!video.seeking&&Number.isFinite(meta.mediaTime)&&video.currentSrc===new URL(endpoint(row,'stream'),location.origin).href){
   const newlyKnown=!frameToken;frameToken={turn,seconds:meta.mediaTime};if(newlyKnown)updateCoverState();
  }
  frameRequest=video.requestVideoFrameCallback(cb);
 };
 frameRequest=video.requestVideoFrameCallback(cb);
}
for(const event of ['loadeddata','canplay','playing','seeked'])video.addEventListener(event,()=>{
 if(!derivedMode&&selected?.preview_available)watchFrames(selected,generation);
 updateCoverState();
});
video.addEventListener('seeking',()=>{frameToken=null;updateCoverState();});
$('cover-shutter').onclick=()=>{
 const row=selected,token=frameToken||(row?.from_sources&&video.readyState>=2?{turn:generation,seconds:video.currentTime||0}:null);
 if($('cover-shutter').disabled||!row||!pdState||!token)return;
 const f=row.from_sources?{duration:video.duration}:(mediaFacts[row.preview_sha256]?.format||{duration:ttSHA===row.preview_sha256?video.duration:null});
 const mark=pd.mapCover(token.seconds,editState?.in_ms,editState?.out_ms,Number(f.duration));
 if(!mark){$('cover-offset').textContent='Frame outside the verified trim; no file created.';return;}
 video.pause();const w=video.videoWidth,h=video.videoHeight;if(!w||!h)return;
 const canvas=document.createElement('canvas');canvas.width=1080;canvas.height=1920;
 try{
   const ctx=canvas.getContext('2d');if(!ctx)throw Error();
   const scale=Math.min(1080/w,1920/h);ctx.fillStyle='#000';ctx.fillRect(0,0,1080,1920);ctx.drawImage(video,(1080-w*scale)/2,(1920-h*scale)/2,w*scale,h*scale);
   canvas.toBlob(blob=>{
    if(generation!==token.turn)return;
    if(!blob||blob.type!=='image/png'||blob.size>9*1024*1024){$('cover-offset').textContent='Cover export could not be produced.';return;}
    if(coverUrl)URL.revokeObjectURL(coverUrl);coverUrl=URL.createObjectURL(blob);
    pdVersion++;pdState.cover=mark;
    const image=$('cover-image'), link=$('cover-download');
    image.src=coverUrl;image.hidden=false;
    link.href=coverUrl;link.download='snapmatic-cover-'+row.clip_id.slice(0,12)+'-'+mark.frame_ms+'ms.png';link.hidden=false;
    updateCoverState();workflowDirty();workflowCover(workflowKey(row),{blob,frame_ms:mark.frame_ms}).catch(()=>{});
   },'image/png');
 }catch{$('cover-offset').textContent='Same-origin frame capture unavailable; no file exported.';}
};

let ttCheck=null,ttKey='',ttSHA=null;
const ttText=()=>pd.materialize({title:$('title').value,caption:$('caption').value,tags:$('tags').value},pdState?.overrides||{},pdState||{}).tiktok;
function refreshTikTokExport(){
 const row=selected, id=row?.clip_id, k=JSON.stringify([generation,id,row?.preview_sha256,activeProfile()]);
 if(k===ttKey)return;ttKey=k;ttSHA=null;$('tt-export').disabled=true;
 $('draft-tt-capacity').textContent=!row?'Select a clip.':row.from_sources?(row.exported?'Exported locally ('+(row.exported_file||'ready')+'). Copy caption above or open exports folder.':'Use Export Local MP4 below to render for manual TikTok upload.'):row.kind!=='rendered'?'Preview only — not export eligible.':'Held pending exact export verification.';
 if(!activeProfile()||row?.kind!=='rendered'||row.state!=='READY_FOR_REVIEW')return;
 ttCheck=get('/api/workspace/tiktok/export/'+encodeURIComponent(id)).then(x=>{
  if(ttKey!==k||!x.ready||x.sha256!==row.preview_sha256)return;
  ttSHA=x.sha256;$('draft-tt-capacity').textContent='EXPORT READY · verified local MP4';$('tt-export').disabled=false;updateCoverState();workFlow();
 }).catch(()=>{});
}
$('tt-copy').onclick=async()=>{try{await navigator.clipboard.writeText(ttText());$('tt-result').textContent='Caption and hashtags copied.';}catch{$('tt-result').textContent='Clipboard unavailable. Copy the editor text manually.';}};
$('tt-folder').onclick=async()=>{try{const r=await fetch('/api/workspace/film-roll/open-exports',{method:'POST',credentials:'same-origin'});if(!r.ok)throw Error();$('tt-result').textContent='Opened the local export folder.';}catch{$('tt-result').textContent='Could not open the local export folder.';}};
$('tt-export').onclick=async()=>{
 const id=selected?.clip_id, key=ttKey, sha256=ttSHA, b=$('tt-export');
 if(!id||!sha256||b.disabled)return;b.disabled=true;
 try{
  let cover_png_hex=null;
  if(coverUrl&&pdState?.cover&&video.currentSrc===new URL(endpoint(selected,'stream'),location.origin).href){
   const blob=await fetch(coverUrl).then(r=>r.blob());
   if(blob.size>2*1024*1024)throw Error('Captured cover exceeds 2 MiB; no export was created.');
   cover_png_hex=Array.from(new Uint8Array(await blob.arrayBuffer()),v=>v.toString(16).padStart(2,'0')).join('');
  }else throw Error('Capture a cover from this exact render before export.');
  if(key!==ttKey)return;
  const r=await fetch('/api/workspace/tiktok/export/'+encodeURIComponent(id),{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({sha256,caption:ttText(),cover_png_hex})});
  if(!r.ok)throw Error('Exact local export held by server.');
  const item=await r.json();$('tt-result').textContent='Created '+item.file+' locally. Nothing was uploaded.';
 }catch(e){$('tt-result').textContent=e.message;}finally{if(key===ttKey)b.disabled=!ttSHA;}
};
const wfUndo=new Map();
let wfTimer,wfApplying=false,wfSeek=null,wfLastTime=0;
function workflowKey(row){return row?.preview_available&&/^[a-f0-9]{64}$/.test(row.preview_sha256||'')?row.clip_id+':'+row.preview_sha256:null;}
function workflowRead(key){try{return JSON.parse(localStorage.getItem('snapmatic-workflow-v1:'+scope+':'+key));}catch{return null;}}
function workflowWrite(key,value){try{localStorage.setItem('snapmatic-workflow-v1:'+scope+':'+key,JSON.stringify(value));return true;}catch{return false;}}
function workflowSnapshot(){return {title:$('title').value,caption:$('caption').value,tags:$('tags').value,in_ms:editState?.in_ms,out_ms:editState?.out_ms,cover:pdState?.cover||null};}
async function workflowCover(key,record){
 if(!key||!window.indexedDB)return null;
 const db=await new Promise((ok,no)=>{
  const t=indexedDB.open('snapmatic-workflow-v1',1);
  t.onupgradeneeded=()=>t.result.createObjectStore('covers');
  t.onsuccess=()=>ok(t.result);t.onerror=()=>no(t.error);
 });
 try{return await new Promise((ok,no)=>{
  const tx=db.transaction('covers',record?'readwrite':'readonly');
  const table=tx.objectStore('covers');
  const req=record?table.put(record,scope+':'+key):table.get(scope+':'+key);
  tx.oncomplete=()=>ok(req.result);tx.onabort=tx.onerror=()=>no(tx.error);
 });}finally{db.close();}
}
function workflowApply(row,override){
 const key=workflowKey(row);if(!key)return;
 const d=override||workflowRead(key);
 if(!override){const last=workflowRead('last');wfSeek=last?.id===row.clip_id&&last?.sha===row.preview_sha256?{turn:generation,ms:last.ms}:null;}
 if(!d){wfUndo.set(scope+':'+key,workflowSnapshot());return;}
 wfApplying=true;
 for(const [id,cap] of [['title',150],['caption',10000],['tags',1000]])if(typeof d[id]==='string'&&d[id].length<=cap)$(id).value=d[id];
 if(editState){const t=ed.trim(d.in_ms,d.out_ms,editState.duration_ms);if(t){Object.assign(editState,t);editState.dirty=true;updateTrim();}}
 if(pdState&&d.cover&&Number.isSafeInteger(d.cover.frame_ms)&&d.cover.frame_ms>=0)pdState.cover=d.cover;
  if(pdState?.cover){
  const turn=generation,ms=pdState.cover.frame_ms;
  workflowCover(key).then(saved=>{
   if(turn!==generation||pdState?.cover?.frame_ms!==ms||saved?.frame_ms!==ms||!(saved.blob instanceof Blob))return;
   if(coverUrl)URL.revokeObjectURL(coverUrl);
   coverUrl=URL.createObjectURL(saved.blob);
   const img=$('cover-image'),link=$('cover-download');
   img.src=coverUrl;img.hidden=false;link.href=coverUrl;link.download='cover.png';link.hidden=false;
  }).catch(()=>{});
 }else if(override){if(coverUrl)URL.revokeObjectURL(coverUrl);coverUrl=null;$('cover-image').hidden=true;$('cover-download').hidden=true;}
 pdVersion++;refreshDraftEditor();updateCoverState();wfApplying=false;
 if(!override)wfUndo.set(scope+':'+key,workflowSnapshot());
}
function workflowSave(){
 const key=workflowKey(selected);if(!key||wfApplying||!activeProfile())return;
 const d=workflowSnapshot();$('wf-saved').textContent=workflowWrite(key,d)?'Draft saved on this PC.':'Draft could not be saved.';
}
function workflowFlush(){if(wfTimer==null)return;clearTimeout(wfTimer);wfTimer=null;workflowSave();}
function workflowDirty(){
 const key=workflowKey(selected);if(!key||wfApplying)return;
 clearTimeout(wfTimer);const turn=generation;
 $('wf-saved').textContent='Saving local draft…';
 wfTimer=setTimeout(()=>{wfTimer=null;if(turn===generation)workflowSave();},320);
 $('wf-undo').disabled=!wfUndo.has(scope+':'+key);
}
for(const id of ['title','caption','tags','trim-in','trim-out'])$(id).addEventListener('input',workflowDirty);
for(const id of ['trim-in-here','trim-out-here'])$(id).addEventListener('click',workflowDirty);
function workFlow(){
 const r=selected, b=$('wf-primary'),h=$('wf-stage'),d=$('wf-reason');
 const pending=items.filter(x=>x.state==='pending_review').length,approved=items.filter(x=>x.state==='approved').length,exported=items.filter(x=>x.state==='exported'||x.exported).length;
 $('wf-count').textContent=pending+' pending · '+approved+' approved · '+exported+' exported';
 if(r?.from_sources){
  const canExport=!$('edit-transcode').disabled;
  h.textContent=r.state==='exported'?'Exported':r.state==='approved'?'Approved':r.state==='hold'?'Hold':'Review';
  b.textContent=r.exported?'Re-export MP4':canExport?'Export MP4 (FFmpeg)':'Loading video…';
  b.disabled=!canExport;
  b.onclick=()=>$('edit-transcode').click();
  d.textContent=r.exported?'Exported locally ('+(r.exported_file||'ready')+'). You can replay it, open the exports folder, or advance to the next clip.':'Trim, crop, edit captions, approve, or export this clip locally. Original file is never modified.';
  $('wf-next').disabled=!workflowKey(r);
  return;
 }
 const ready=Boolean(r&&ttSHA&&r.preview_sha256===ttSHA),review=r?.kind==='rendered'&&r.state==='READY_FOR_REVIEW'&&r.review==='PENDING';
 const renderer=r?.clip_id===phase3Clip&&!$('edit-transcode').disabled;
 const hold=/hold|HELD|FAILED/i.test(r?.state||'');
 h.textContent=ready?'Export':hold?'Hold':review?'Review':r?'Render':'Review';
 b.textContent=ready?'Create manual export':review?'Approve local review':renderer?'Render verified local preview':hold?'On hold':r?'Render not authorized':'Choose a clip';
 b.disabled=!(ready||review||renderer);
 b.onclick=()=>{if(ready)$('tt-export').click();else if(review)$('approve').click();else if(renderer)$('edit-transcode').click();};
 d.textContent=hold?'On hold.':ready?'Verified MP4. A local package never posts.':review?'Approve the exact existing render for local review.':renderer?'The existing controller will recheck the sealed render plan.':r?'Select a Film Roll clip from Sources to edit and export locally.':'Select a clip.';
 $('wf-next').disabled=!workflowKey(r);
}
$('wf-next').onclick=()=>{workflowSave();const i=items.findIndex(x=>x.clip_id===selected?.clip_id);const next=items.slice(i+1).find(x=>x.preview_available&&shown(x));if(next)select(next);else note('No more playable clips in this filter.');};
document.addEventListener('keydown',e=>{
 if(activeView!=='studio'||!e.ctrlKey||e.shiftKey||e.altKey||e.metaKey||e.repeat||e.key.toLowerCase()!=='z'||e.target.closest('input,textarea,select,[contenteditable],[role="textbox"]')||document.querySelector('dialog[open]')||$('wf-undo').disabled)return;
 e.preventDefault();$('wf-undo').click();
});
$('wf-undo').onclick=()=>{const key=workflowKey(selected);const d=wfUndo.get(scope+':'+key);if(d){workflowApply(selected,d);workflowSave();$('wf-undo').disabled=true;}};
video.addEventListener('loadedmetadata',()=>{if(wfSeek?.turn===generation&&Number.isFinite(video.duration)&&Number.isFinite(wfSeek.ms)){video.currentTime=Math.max(0,Math.min(wfSeek.ms/1000,video.duration-.05));}wfSeek=null;});
video.addEventListener('timeupdate',()=>{const row=selected,now=Date.now();if(!workflowKey(row)||derivedMode||!activeProfile()||now-wfLastTime<900||video.readyState<2)return;wfLastTime=now;workflowWrite('last',{id:row.clip_id,sha:row.preview_sha256,ms:Math.round(video.currentTime*1000)});});
let toastTimer=null;
function note(t,actions=[]){
 $('message').textContent=t;
 const box=$('sm-toast');
 if(!box)return;
 if(!t){box.hidden=true;return;}
 const parts=[node('span',t)];
 const actWrap=node('span',undefined,'toast-actions');
 if(Array.isArray(actions)&&actions.length){
  for(const act of actions){
   const b=node('button',act.label,'toast-act-btn');b.type='button';
   b.onclick=()=>{box.hidden=true;clearTimeout(toastTimer);act.onClick?.();};
   actWrap.append(b);
  }
 }
 const close=node('button','×','toast-close');close.type='button';close.ariaLabel='Dismiss notification';
 close.onclick=()=>{box.hidden=true;clearTimeout(toastTimer);};
 actWrap.append(close);
 parts.push(actWrap);
 box.replaceChildren(...parts);
 box.hidden=false;
 clearTimeout(toastTimer);
 toastTimer=setTimeout(()=>{box.hidden=true;},actions.length?7000:3800);
}
function tone(row){return /hold|FAILED|HELD/i.test(row.state)?'hold':row.state==='pending_review'||row.review==='PENDING'?'warn':'ready';}
function shown(row,mode=filter){
 if(mode==='pending')return row.state==='pending_review'||(row.state==='READY_FOR_REVIEW'&&row.review==='PENDING');
 if(mode==='approved')return row.state==='approved';
 if(mode==='exported')return row.state==='exported'||row.exported===true;
 if(mode==='unposted')return !row.yt_video_id&&!row.ig_media_id&&!row.tt_publish_id&&!row.x_tweet_id;
 if(mode==='toprated')return Number(row.rating||0)>=4;
 if(mode==='ready')return row.state==='READY_FOR_REVIEW'||row.state==='approved'||row.state==='exported';
 if(mode==='workflow-render')return row.state==='pending_review'&&!row.preview_only;
 if(mode==='workflow-export')return row.manual_export_ready===true||row.exported===true;
 if(mode==='no-speech')return row.caption_status==='no_speech';
 if(mode==='hold')return ['hold','test_hold','duplicate_hold','HELD','FAILED'].includes(row.state);
 return true;
}
function time(s){if(!Number.isFinite(s))return '--:--.---';const ms=Math.max(0,Math.floor(s*1000));return String(Math.floor(ms/60000)).padStart(2,'0')+':'+String(Math.floor(ms/1000)%60).padStart(2,'0')+'.'+String(ms%1000).padStart(3,'0');}
function updateTime(){
 seek.value=String(video.currentTime||0);
 $('time').textContent=time(video.currentTime)+' / '+time(video.duration);
 const ph=$('trim-playhead');
 if(ph&&Number.isFinite(video.duration)&&video.duration>0){
  ph.style.left=Math.max(0,Math.min(100,(video.currentTime/video.duration)*100))+'%';
 }
}
function updateMockOverlay(){
 const el=$('vf-mock-ui');if(!el)return;
 const on=Boolean($('mock-ui-toggle')?.checked);
 el.hidden=!on;
 if(!on)return;
 const p=$('platform-zone')?.value||'tiktok';
 el.dataset.platform=p;
 const topLabels={tiktok:'Following  ·  For You',reels:'Reels',shorts:'Shorts',x:'X · Video'};
 if($('vf-mock-top'))$('vf-mock-top').textContent=topLabels[p]||'Preview';
 const provKey=p==='reels'?'instagram':p==='shorts'?'youtube':p==='x'?'x':'tiktok';
 const handle=oauthStatus?.[provKey]?.handle||oauthStatus?.tiktok?.handle||oauthStatus?.instagram?.handle||'@creator';
 if($('vf-mock-handle'))$('vf-mock-handle').textContent=handle;
 const capText=[$('caption')?.value.trim()||$('title')?.value.trim(),$('tags')?.value.trim()].filter(Boolean).join(' ')||'Your caption & hashtags appear here…';
 if($('vf-mock-caption'))$('vf-mock-caption').textContent=capText;
}
function updateGuides(){
 $('grid').hidden=!$('grid-toggle').checked;$('zones').hidden=!$('safe-toggle').checked;
 const [ratios,name,right,bottom]=guides[$('platform-zone').value]||guides.tiktok;$('zones').style.setProperty('--zone',ratios);$('zones').style.setProperty('--right',right);$('zones').style.setProperty('--bottom',bottom);$('zone-label').textContent=name;
 updateMockOverlay();
}
function showView(mode,push=false){
 if(!['studio','gallery','insights','calendar','ledger','system','profiles','sources'].includes(mode))mode='studio';
 activeView=mode;
 for(const id of ['studio','gallery','insights','calendar','ledger','system','profiles','sources'])$(id).hidden=id!==mode;
 document.querySelectorAll('.view-nav [data-view]').forEach(a=>{if(a.dataset.view===mode)a.setAttribute('aria-current','page');else a.removeAttribute('aria-current');});
 document.querySelectorAll('.utility-strip [data-shell-link]').forEach(a=>{if(a.dataset.shellLink===mode)a.setAttribute('aria-current','page');else a.removeAttribute('aria-current');});
 if(push&&new URLSearchParams(location.search).get('view')!==mode)history.pushState(null,'','/workspace?view='+mode);
 if(mode!=='studio')video.pause();
 if(mode==='profiles'||mode==='sources'){const frame=$(mode).querySelector('iframe');if(frame&&!frame.hasAttribute('src')){frame.onload=()=>syncIframeThemes(document.documentElement.dataset.theme==='light'?'light':'dark');frame.src=frame.dataset.embed;}else{syncIframeThemes(document.documentElement.dataset.theme==='light'?'light':'dark');}}
 if(mode==='studio')refreshLocalStatus();
 if(mode==='gallery'){renderGallery();inspectMetadata();refreshLocalStatus();if(indexReady)reload();}
 if(mode==='insights'){loadInsights(true);refreshOAuthStatus();}
 if(mode==='calendar'){loadCalendar();}
 if(mode==='ledger'){renderExportHistory();loadExportHistory();renderLedger();if(ledgerReady)loadLedger();window.dispatchEvent(new CustomEvent('snapmatic:ledger-opened'));}
 if(mode==='system'){renderSystem();updatePhase4();cacheUsage();refreshLocalStatus();refreshOAuthStatus();}
}
let gallerySource='all', gallerySort='recent';
function renderGallery(){
 if(!indexReady)return;
 const srcSel=$('gallery-source-filter');
 if(srcSel){
  const sources=Array.from(new Set(items.map(r=>r.source_name).filter(Boolean))).sort((a,b)=>a.localeCompare(b));
  const opts=[node('option','All sources')];opts[0].value='all';
  for(const s of sources){const o=node('option',s);o.value=s;opts.push(o);}
  srcSel.replaceChildren(...opts);
  if(gallerySource!=='all'&&!sources.includes(gallerySource))gallerySource='all';
  srcSel.value=gallerySource;
 }
 const needle=$('gallery-search').value.trim().toLocaleLowerCase();
 const visible=(activeProfile()?items:[]).filter(r=>{
  if(!shown(r,galleryFilter))return false;
  if(gallerySource!=='all'&&(r.source_name||'')!==gallerySource)return false;
  if(!needle)return true;
  return [r.name,r.source_name,r.source_platform,r.import_metadata?.description].filter(Boolean).join(' ').toLocaleLowerCase().includes(needle);
 }).slice();
 if(gallerySort==='title')visible.sort((a,b)=>(a.name||'').localeCompare(b.name||''));
 else if(gallerySort==='largest')visible.sort((a,b)=>(b.preview_bytes||0)-(a.preview_bytes||0));
 else if(gallerySort==='rating')visible.sort((a,b)=>(Number(b.rating||0)-Number(a.rating||0))||(a.name||'').localeCompare(b.name||''));
 const pending=items.filter(r=>r.state==='pending_review').length;
 const approved=items.filter(r=>r.state==='approved').length;
 const exported=items.filter(r=>r.state==='exported'||r.exported===true).length;
 $('gallery-count').textContent=visible.length+' shown · '+pending+' pending · '+approved+' approved · '+exported+' exported';
 const clearBtn=$('gallery-clear-approved');
 if(clearBtn)clearBtn.disabled=approved===0;
 const expAllBtn=$('gallery-export-approved');
 if(expAllBtn)expAllBtn.disabled=approved===0||exportJobActive;
 $('gallery-items').replaceChildren(...visible.map(r=>{
  const b=node('button',undefined,'gallery-card');b.type='button';b.dataset.clip=r.clip_id;
  if(r.poster_available){const i=node('img');i.alt='';i.src=endpoint(r,'poster');i.loading='lazy';i.onerror=()=>i.replaceWith(node('span','POSTER HELD','gallery-placeholder'));b.append(i);}
  else b.append(node('span',r.state==='test_hold'?'PRIVATE TEST HOLD':'NO POSTER','gallery-placeholder'));
  const starStr=Number(r.rating||0)>0?' · '+'★'.repeat(Number(r.rating)):'';
  b.append(node('strong',r.name),node('small',(labels[r.state]||r.state)+(r.source_name?' · '+r.source_name:'')+(r.exported?' · MP4':'')+starStr,tone(r)));
  const ship=renderShipBadges(r);if(ship)b.append(ship);
  const m=node('small',(dimensions.get(r.clip_id)||'Resolution unmeasured')+' · '+(durations.has(r.clip_id)?time(durations.get(r.clip_id)):'--:--'),'dimensions');
  if(r.import_metadata?.description){const summary=node('small',r.import_metadata.description,'import-description');summary.title=r.import_metadata.description;b.append(summary);} b.append(m);b.onclick=()=>{showView('studio',true);select(r);$('viewfinder').focus();};
  if(r.from_sources){
   const wrap=node('div',undefined,'gallery-card-wrap');wrap.append(b);
   const rm=node('button','Remove from Film Roll','gallery-remove-btn');rm.type='button';rm.title='Remove from Film Roll Gallery (keeps file on disk)';
   rm.onclick=async()=>{rm.disabled=true;try{await fetch('/api/workspace/film-roll/'+encodeURIComponent(r.clip_id),{method:'DELETE',credentials:'same-origin',redirect:'error'});if(selected?.clip_id===r.clip_id){selected=null;stopVideo();}await reload();refreshLocalStatus();}catch{rm.disabled=false;}};
   wrap.append(rm);return wrap;
  }
  if(r.clip_id!==phase3Clip)return b;
  const host=node('div',undefined,'gallery-export');host.append(b);const tool=node('button','Transcode to H.264 (NVENC)');tool.type='button';tool.disabled=!nvenc?.can_dispatch;
  tool.onclick=()=>{showView('studio',true);select(r);$('edit-transcode').focus();};host.append(tool);return host;
 }));
}
let readingMetadata=false;
async function inspectMetadata(){
 if(readingMetadata)return;
 readingMetadata=true;
 const probe=document.createElement('video');probe.preload='metadata';probe.muted=true;
 try {
  for(const r of items.filter(r=>r.preview_available&&!durations.has(r.clip_id))){
   if(activeView!=='gallery')break;
   await new Promise(resolve=>{
    const end=()=>{clearTimeout(timer);probe.removeEventListener('loadedmetadata',ok);probe.removeEventListener('error',end);resolve();};
    const ok=()=>{if(Number.isFinite(probe.duration)&&probe.duration>0){durations.set(r.clip_id,probe.duration);dimensions.set(r.clip_id,probe.videoWidth+'×'+probe.videoHeight);}end();};
    const timer=setTimeout(end,4500);probe.addEventListener('loadedmetadata',ok);probe.addEventListener('error',end);
    probe.src=endpoint(r,'stream');probe.load();
   });
   const label=Array.from($('gallery-items').querySelectorAll('.gallery-card')).find(b=>b.dataset.clip===r.clip_id)?.querySelector('.dimensions');
   if(label&&durations.has(r.clip_id))label.textContent=dimensions.get(r.clip_id)+' · '+time(durations.get(r.clip_id));
  }
 }finally{probe.removeAttribute('src');probe.load();readingMetadata=false;}
}
function studioLink(id){
 const td=node('td');if(typeof id!=='string'||!/^[-_A-Za-z0-9]{11}$/.test(id)){td.textContent='Not recorded';return td;}
 const a=node('a',id);a.href='https://studio.youtube.com/video/'+id+'/edit';a.target='_blank';a.rel='noopener noreferrer';td.append(a);
 const copy=node('button','Copy ID');copy.type='button';copy.onclick=async()=>{try{await navigator.clipboard.writeText(id);$('ledger-notice').textContent='Copied YouTube ID.';}catch{$('ledger-notice').textContent='Copy unavailable. The selectable YouTube ID is '+id+'.';}};td.append(copy);return td;
}
function restoreActions(row){
 const td=node('td');
 if(row.job_id!=='00000000-0000-4000-8000-000000000001'||!row.backup_restore_verified)return td;
 for(const [kind,label] of [['video','Open Backup Folder (video)'],['receipt','Show receipt']]){
  const b=node('button',label);b.type='button';b.onclick=async()=>{
   b.disabled=true;
   try{const r=await fetch('/api/workspace/restore/'+kind,{method:'POST',credentials:'same-origin',redirect:'error'});
    if(!r.ok)throw Error('The exact backup could not be verified.');$('ledger-notice').textContent='Opened Explorer for the verified '+kind+'.';
   }catch(e){$('ledger-notice').textContent=e.message;}finally{b.disabled=false;}
  };td.append(b);
 }
 return td;
}
let exportHistoryLoad=null;
function loadExportHistory(){
 if(exportHistoryLoad)return exportHistoryLoad;
 exportHistoryLoad=get('/api/workspace/export-history').then(d=>{
  exportHistoryRows=Array.isArray(d.items)?d.items:[];
  renderExportHistory();
 }).catch(()=>{if($('export-history-notice'))$('export-history-notice').textContent='Local export history unavailable.';}).finally(()=>{exportHistoryLoad=null;});
 return exportHistoryLoad;
}
function renderExportHistory(){
 const tbody=$('export-history-rows'),notice=$('export-history-notice');
 if(!tbody)return;
 const word=($('ledger-search')?.value||'').trim().toLocaleLowerCase();
 const rows=exportHistoryRows.filter(r=>!word||[r.title,r.source_name,r.file_name,r.framing,r.quality,r.caption_file,r.cover_file].filter(Boolean).join(' ').toLocaleLowerCase().includes(word));
 tbody.replaceChildren(...rows.map(r=>{
  const tr=node('tr');
  const titleTd=node('td');titleTd.append(node('strong',r.title||r.file_name),node('div',(r.source_name?r.source_name+' · ':'')+r.file_name,'muted'));
  const trimStr=r.out_ms>r.in_ms?(time(r.in_ms/1000)+' → '+time(r.out_ms/1000)):'Full clip';
  const flags=[r.framing,r.quality,r.muted?'muted':null,r.normalized_audio?'-14 LUFS':null].filter(Boolean).join(' · ');
  const setTd=node('td');setTd.append(node('div',flags),node('div',trimStr,'muted'));
  const files=[r.exists?'MP4':'MP4 (moved)',r.caption_file?'.txt':null,r.cover_file?'cover .png':null].filter(Boolean).join(' · ');
  const actTd=node('td');const actWrap=node('div',undefined,'exp-actions');
  const rev=node('button','Reveal in Explorer');rev.type='button';rev.disabled=!r.exists;
  rev.onclick=async()=>{rev.disabled=true;try{const res=await fetch('/api/workspace/film-roll/exports/'+encodeURIComponent(r.id)+'/reveal',{method:'POST',credentials:'same-origin'});if(!res.ok)throw Error();note('Opened Explorer for '+r.file_name);}catch{note('Could not reveal '+r.file_name);}finally{rev.disabled=!r.exists;}};
  actWrap.append(rev);
  const clipMatch=items.find(x=>x.clip_id===r.clip_id);
  if(clipMatch){
   const openBtn=node('button','Open in Studio');openBtn.type='button';
   openBtn.onclick=()=>{showView('studio',true);select(clipMatch);$('viewfinder').focus();};
   actWrap.append(openBtn);
  }
  if(r.youtube_video_id){
   const ytA=node('a','YouTube Studio ↗','oauth-link');
   ytA.href='https://studio.youtube.com/video/'+encodeURIComponent(r.youtube_video_id)+'/edit';ytA.target='_blank';ytA.rel='noopener noreferrer';
   actWrap.append(ytA);
  }
  if(r.ig_permalink){
   const igA=node('a','Instagram Reel ↗','oauth-link');
   igA.href=r.ig_permalink;igA.target='_blank';igA.rel='noopener noreferrer';
   actWrap.append(igA);
  }
  if(r.tiktok_publish_id){
   actWrap.append(node('span','TT Inbox ✓','ship-badge ship-tt'));
  }
  if(r.x_tweet_id){
   const xA=node('a','X Post ↗','oauth-link');
   xA.href='https://x.com/i/web/status/'+encodeURIComponent(r.x_tweet_id);xA.target='_blank';xA.rel='noopener noreferrer';
   actWrap.append(xA);
  }
  actTd.append(actWrap);
  tr.append(titleTd,setTd,node('td',files,r.exists?'verified':'warn'),node('td',fmtBytes(r.bytes)),node('td',healthTime(r.exported_at)),actTd);
  return tr;
 }));
 if(notice)notice.textContent=rows.length?rows.length+' local export'+(rows.length===1?'':'s')+' recorded · original files untouched':'No local MP4 exports yet. Export any Film Roll clip in Viewfinder Studio to see it here.';
}
if($('ledger-open-exports'))$('ledger-open-exports').onclick=()=>{$('edit-open-exports')?.click();};
if($('sys-open-exports'))$('sys-open-exports').onclick=()=>{$('edit-open-exports')?.click();};
if($('clear-thumb-cache'))$('clear-thumb-cache').onclick=async()=>{
 const b=$('clear-thumb-cache'),out=$('thumb-cache-result');b.disabled=true;
 try{
  const r=await get('/api/workspace/thumbnail-cache/clear',{method:'POST'});
  const msg='Cleared '+r.removed_files+' cached preview'+(r.removed_files===1?'':'s')+' ('+fmtBytes(r.freed_bytes)+' freed). Media untouched.';
  if(out)out.textContent=msg;note(msg);refreshLocalStatus();
 }catch(e){if(out)out.textContent='Could not clear thumbnail cache: '+e.message;}
 finally{b.disabled=false;}
};
function renderLedger(){
 renderExportHistory();
 if(!ledgerReady)return;
 const word=$('ledger-search').value.trim().toLocaleLowerCase();
 const records=(activeProfile()?ledgerRows:[]).filter(r=>!word||Object.values(r).join(' ').toLocaleLowerCase().includes(word));
 if($('ledger-count'))$('ledger-count').textContent=String(records.length);
 $('ledger-records').replaceChildren(...records.map(r=>{
   const tr=node('tr');
   const title=node('td');title.append(node('strong',r.name),node('div',labels[r.state]||r.state,'muted'));
   const restore=node('td',r.backup_restore_verified?'Verified restore':'Not verified',r.backup_restore_verified?'verified':'warn');
   const ig=typeof r.instagram_unpublished_container_id==='string'&&/^\d{10,24}$/.test(r.instagram_unpublished_container_id)?r.instagram_unpublished_container_id:'Not recorded';
   tr.append(title,node('td',r.source_sha256||'-','hash'),node('td',r.render_sha256||'-','hash'),restore,
     studioLink(r.youtube_private_id),node('td',ig),restoreActions(r));
   return tr;
 }));
 $('ledger-notice').textContent=records.length+' matching receipt'+(records.length===1?'':'s')+' · public posting disabled';
}
function healthTime(t){return Number.isSafeInteger(t)&&t>0?new Date(t*1000).toLocaleString():'Not verified';}
function deadline(el,label,t,checked){
 const known=Number.isSafeInteger(t)&&t>0&&Number.isSafeInteger(checked)&&checked>0;
 const days=known?Math.max(0,(t-checked)/86400):null;
 el.append(node('div',label+': '+healthTime(t)+(days===null?'':t<=checked?' (expired)':' ('+(days<1?Math.max(1,Math.ceil(days*1440))+' min':days.toFixed(1)+' days')+' remaining)'),known&&days<=14?'warn':'muted'));
}
function sizeLabel(bytes){
 return Number.isSafeInteger(bytes)&&bytes>=0?new Intl.NumberFormat(undefined,{maximumFractionDigits:2}).format(bytes/1048576)+' MiB':'Not verified';
}
let phase4Read=null;
function updatePhase4(){
 if(phase4Read)return phase4Read;
 phase4Read=get('/api/workspace/intake').then(d=>{
  const status=$('phase4-source'),watch=d.watcher?.state||'unknown';
  const readable={held_unconfigured:'Not configured',read_only_no_observer_authority:'Read-only sidecar; observer not authorized',not_started:'Not started',
        observer_starting:'Starting bounded observer',fixture_verified:'Fixture observer finished',observed_read_only:'Read-only metadata observation recorded',
        checking_pending_only:'Checking for pending review',pending_only_checked:d.admission_enabled?'Active · pending review only':'Pending-only: source unavailable'};
  status.textContent=(d.fixture?'Isolated fixture only · ':'')+(readable[watch]||'Held: '+watch)+
    ' · '+String(d.source_state||'unknown').replaceAll('_',' ')+(d.pending_review==null?'':' · '+d.pending_review+' pending (local)');
  const byId=new Map((d.platforms||[]).map(p=>[p.platform,p]));
  $('phase4-platforms').replaceChildren(...['youtube','instagram','tiktok'].map(id=>{
    const p=byId.get(id),line=node('div',undefined,'kv');
    const spacing=p?.minimum_spacing_secs, base=d.cadence_default?.minimum_spacing_secs;
    line.append(node('strong',providerNames[id]),node('span',Number.isSafeInteger(spacing)?
       (spacing/3600).toFixed(2).replace(/\.00$/,'')+' h configured spacing':
       Number.isSafeInteger(base)?(base/3600)+' h default · not active':'Spacing not verified'));
    line.append(node('div','Next local due: '+healthTime(p?.next_due_at_unix)+' · posting held','muted'));
    return line;
  }));
 }).catch(()=>{$('phase4-source').textContent='Local intake state not verified';}).finally(()=>{phase4Read=null;});
 return phase4Read;
}
function cacheUsage(){
 return get('/api/workspace/cache').then(d=>{$('phase4-cache-size').textContent=sizeLabel(d.managed_logical_bytes)+' managed logical bytes · '+sizeLabel(d.protected_derivative_bytes)+' protected derivative';})
 .catch(()=>{$('phase4-cache-size').textContent='Cache size not verified';});
}
$('phase4-cache-preview').onclick=async()=>{
 const b=$('phase4-cache-preview'),out=$('phase4-cache-result');b.disabled=true;
 try{
   const d=await get('/api/workspace/cache-preview');
   if(d.deletion_enabled!==false)throw Error('Preview rejected');
   out.textContent=d.eligible_count+' eligible. '+d.reason+' Nothing removed.';
 }catch(e){out.textContent='Preview held: '+e.message;}
 finally{b.disabled=false;}
};

function renderSystem(){
 $('system-providers').replaceChildren(...['youtube','instagram','tiktok'].map(id=>{
  const p=(activeProfile()?providers:[]).find(r=>r.provider===id),el=node('div',undefined,'kv');
  if(id==='tiktok'){el.append(node('strong','TikTok'),node('span','MANUAL EXPORT'),node('div','No TikTok account connection, Inbox, or automatic posting. Export is available locally from Viewfinder Studio.','muted'));return el;}
  const named=providerNames[id];
 const authReason=id==='youtube'?({
   refresh_attempt_held_unknown:'A renewal was attempted, but its outcome was not verified. Automatic retry is held.',
   invalid_grant:'Google explicitly rejected the refresh grant. Reconnection is required.',
   provider_rejected:'The request was rejected; exact cause was not verified.',
   transport_unconfirmed:'The request outcome was not verified. Automatic retry is held.',
   verification_unconfirmed:'Renewal could not be verified. Automatic retry is held.'
 })[p?.reason]:null;
  const health=p?.health||'unknown';
  el.append(node('strong',named),node('span',health.toUpperCase()+(id==='youtube'&&p?.automatic_refresh_configured?' · auto-refresh configured':id==='instagram'?' · live transport held':'')));
  const historic=ledgerRows.find(x=>x.instagram_unpublished_container_id==='10000000000000002');
  if(id==='instagram'&&historic)el.append(node('div','Historical test container: FINISHED, unpublished. This is not a current credential check.','muted'));
    if(authReason)el.append(node('div',authReason,'muted'));
 el.append(node('div','Credential: '+({google_oauth_user:"Google OAuth desktop user",facebook_page:"Facebook Page",tiktok_oauth_user:"TikTok OAuth user"}[p?.credential_type]||(health==='unconfigured'?'Not connected':health==='retired'?'Retired':'Not verified')),'muted'));
  el.append(node('div','Last verified refresh attempt: '+healthTime(p?.last_refresh_at_unix),'muted'));
  deadline(el,'Access expiry',p?.access_expires_at_unix,p?.checked_at_unix);
  deadline(el,'Refresh credential expiry',p?.refresh_expires_at_unix,p?.checked_at_unix);
  deadline(el,'Data access expiry',p?.data_access_expires_at_unix,p?.checked_at_unix);
  return el;
 }));
 $('system-paths').replaceChildren(...[['Local media folder',activeProfile()?systemData?.source_path:null],['Exports folder',activeProfile()?systemData?.backup_path:null]].map(([k,v])=>{
  const el=node('div',undefined,'kv');el.append(node('strong',k),node('span',v||'Unavailable'));return el;
 }));
}
function renderShellProfile(){
 const profile=channelProfiles.find(p=>p.configured&&p.profile_id===scope);
 if($('sm-active-profile'))$('sm-active-profile').textContent=profile?profile.channel_alias+' · active local profile':'All Channels';
 if($('sm-active-health'))$('sm-active-health').textContent=profile?['youtube','instagram','tiktok'].map(id=>providerNames[id]+' '+(id==='tiktok'?'Manual Export':providers.find(p=>p.provider===id)?.health||'not verified')).join(' · '):'Local workspace ready.';
}
function renderProviders(){
 renderShellProfile();
 $('credentials').replaceChildren(...['youtube','instagram','tiktok','x'].map(id=>{
  const oa=oauthStatus?.[id];
  if(oa?.connected){
   const tag=oa.handle||oa.account_name||'';
   const days=id==='instagram'?igDaysLeft(oa):null;
   const expiring=days!==null&&days<7;
   const statusWord=expiring?(days<=0?'TOKEN EXPIRED':'EXPIRES IN '+Math.max(1,Math.ceil(days))+'D'):'CONNECTED';
   const badge=node('a',undefined,expiring?'warn':'ready');
   badge.append(accountAvatar(id,oa.avatar_url),providerNames[id].toUpperCase()+(tag?' '+tag:'')+' · '+statusWord);
   badge.title=providerNames[id]+' connected ('+tag+')'+(expiring?' · Extend your 60-day token in Connections & System!':' · Public auto-posting OFF');
   badge.href='/workspace?view=system';badge.onclick=e=>{e.preventDefault();showView('system',true);};return badge;
  }
  const p=(activeProfile()?providers:[]).find(x=>x.provider===id), health=p?.health||'unknown';
  const label=oa?.client_configured?'READY TO CONNECT':(health==='ready'?'READY':id==='tiktok'?'MANUAL / CONNECT':'CONNECT');
  const badge=node('a',undefined,health==='ready'?'ready':oa?.client_configured?'warn':'');
  badge.append(brandIcon(id),providerNames[id].toUpperCase()+' · '+label);
  badge.title='Click to connect your '+providerNames[id]+' account in Connections & System';
  badge.href='/workspace?view=system';badge.onclick=e=>{e.preventDefault();showView('system',true);};return badge;
 }));
 $('platforms').replaceChildren(...['tiktok','instagram','youtube'].map(id=>{
  const l=node('label'),i=node('input');i.type='checkbox';i.checked=inspected.get(id);i.onchange=()=>{inspected.set(id,i.checked);if(i.checked){$('platform-zone').value={instagram:'reels',youtube:'shorts',tiktok:'tiktok'}[id];updateGuides();}};
  const oa=oauthStatus?.[id];
  const p=(activeProfile()?providers:[]).find(x=>x.provider===id),health=p?.health||'unknown';
  const statusText=oa?.connected?(id==='tiktok'?'CONNECTED · INBOX DRAFTS':'CONNECTED · PRIVATE DRAFTS ONLY'):id==='tiktok'?'MANUAL EXPORT / CONNECT':health==='ready'?'AUTH READY · POST HELD':'NOT CONNECTED';
  l.append(i,node('span',providerNames[id]),node('small',statusText));return l;
 }));
 renderSystem();
}
function renderRoll(restore=false){
 const q=($('roll-search')?.value||'').trim().toLocaleLowerCase();
 const visible=(activeProfile()?items:[]).filter(r=>{
  if(!shown(r))return false;
  if(!q)return true;
  return [r.name,r.source_name,r.source_platform,r.import_metadata?.description].filter(Boolean).join(' ').toLocaleLowerCase().includes(q);
 });
 const pending=activeProfile()?items.filter(j=>j.state==='pending_review').length:0;
 const approved=activeProfile()?items.filter(j=>j.state==='approved').length:0;
 const exported=activeProfile()?items.filter(j=>j.state==='exported'||j.exported===true).length:0;
 $('roll-count').textContent=visible.length+' shown · '+pending+' pending · '+approved+' approved · '+exported+' exported';
 const active=restore?document.activeElement?.dataset.clip:null;
 roll.replaceChildren(...visible.map((row,i)=>{
  const b=node('button',undefined,'clip');b.type='button';b.dataset.clip=row.clip_id;b.setAttribute('aria-pressed',String(selected?.clip_id===row.clip_id));
  let thumb;
  if(row.poster_available){thumb=node('img',undefined,'thumb');thumb.alt='';thumb.loading='lazy';thumb.src=endpoint(row,'poster');thumb.onerror=()=>thumb.replaceWith(node('span',String(i+1).padStart(2,'0'),'thumb'));}
  else thumb=node('span',String(i+1).padStart(2,'0'),'thumb');
  const starStr=Number(row.rating||0)>0?' · '+'★'.repeat(Number(row.rating)):'';
  const words=node('span');words.append(node('strong',row.name),node('small',(labels[row.state]||row.state)+(row.source_name?' · '+row.source_name:'')+starStr+(row.caption_status==='no_speech'?' · No speech':'')+(durations.has(row.clip_id)?' · '+time(durations.get(row.clip_id)):' · --:--'),tone(row)));const ship=renderShipBadges(row);if(ship)words.append(ship);b.append(thumb,words);
  const eligible=phase5Visible().some(r=>r.clip_id===row.clip_id);const decision=p5Labels.get(p5Key(row));if(decision)words.append(node('small','LOCAL LABEL · '+decision));b.classList.toggle('p5-picked',phase5.batch.has(row.clip_id));b.onclick=e=>{if(e.shiftKey&&eligible){phase5.range(phase5Visible().map(r=>r.clip_id),row.clip_id);renderRoll(true);}else {if(eligible)phase5.setAnchor(row.clip_id);select(row);}};return b;
 }));
 phase5Tray();if(!visible.length)roll.append(node('p','No clips match this filter.','muted'));
 workFlow();if(active)Array.from(roll.querySelectorAll('.clip')).find(b=>b.dataset.clip===active)?.focus();
}
function syncReviewButtons(row){
 const appBtn=$('approve'), holdBtn=$('hold-clip');
 const icon=appBtn.querySelector('img');
 const setAppText=txt=>{if(icon){appBtn.replaceChildren(icon,' '+txt);}else{appBtn.textContent=txt;}};
 if(!row){
  appBtn.disabled=true;setAppText('Approve');
  if(holdBtn){holdBtn.disabled=true;holdBtn.textContent='Hold';}
  return;
 }
 if(row.from_sources){
  appBtn.disabled=false;
  setAppText(row.state==='approved'?'Approved ✓':'Approve');
  if(holdBtn){holdBtn.disabled=false;holdBtn.textContent=row.state==='hold'?'On Hold ✓':'Hold';}
  return;
 }
 appBtn.disabled=!(row.kind==='rendered'&&row.state==='READY_FOR_REVIEW'&&row.review==='PENDING');
 setAppText('Approve');
 if(holdBtn){holdBtn.disabled=true;holdBtn.textContent='Hold';}
}
function endpoint(row,action){return '/api/clips/'+encodeURIComponent(row.clip_id)+'/'+(action==='stream'?'video':action);}
function mediaEmpty(label,detail){$('empty').hidden=false;$('empty').querySelector('span').textContent=label;$('empty').querySelector('small').textContent=detail;}
function stopVideo(){video.pause();video.removeAttribute('src');video.removeAttribute('poster');video.load();seek.disabled=true;play.disabled=true;$('frame').textContent='—';$('bitrate').textContent='—';updateTime();}
function applyProbedTelemetry(row){
 const facts=mediaFacts[row?.preview_sha256];
 if(!facts?.streams)return;
 const v=facts.streams.find(s=>s.codec_type==='video');
 const a=facts.streams.find(s=>s.codec_type==='audio');
 const [p,q]=(v?.avg_frame_rate||'').split('/').map(Number);
 const fps=p>0&&q>0?(p/q).toFixed(2).replace(/\.00$/,''):null;
 const parts=[v?.codec_name?.toUpperCase(),fps?fps+' fps':null,a?.codec_name?.toUpperCase()].filter(Boolean);
 if(parts.length)$('codec').textContent=parts.join(' · ');
}
function probeFilmRoll(row,turn){
 if(!row?.from_sources)return;
 if(mediaFacts[row.preview_sha256]){applyProbedTelemetry(row);renderPreflight();phase5Panel();return;}
 get('/api/workspace/film-roll/'+encodeURIComponent(row.clip_id)+'/probe').then(d=>{
  if(d?.facts){
   mediaFacts[row.preview_sha256]=d.facts;
   if(turn===generation&&selected?.clip_id===row.clip_id){
    applyProbedTelemetry(row);renderPreflight();phase5Panel();
   }
  }
 }).catch(()=>{});
}
function saveDraft(){if(selected){drafts.set(selected.clip_id,{title:$('title').value,caption:$('caption').value,tags:$('tags').value});if(pdState)pdCache.set(pdKey(selected),pdState);}}
function select(row){workflowFlush();saveDraft();disarmIgPublish();selected=row;derivedMode=false;$('p5-loop').checked=false;generation++;cancelFrameWatch();stopVideo();note('');renderRoll(true);
 $('stage-state').textContent=(labels[row.state]||row.state).toUpperCase();$('stage-state').className='flag '+tone(row);
 $('inspect-description').textContent=row.import_metadata?.description||'';$('inspect-description').hidden=!row.import_metadata?.description;
 $('inspect-state').textContent=row.name+(row.source_name?' · '+row.source_name:'')+(row.preview_only&&!row.from_sources?' · Preview only':'');
 const d=drafts.get(row.clip_id)||{};$('title').value=d.title??row.name;$('caption').value=d.caption??(row.import_metadata?.description||'');$('tags').value=d.tags??'';
 resetEdit(row);loadPlatformDraft(row,generation);workflowApply(row);renderPreflight();$('digest').textContent=row.preview_sha256||'Not verified';$('digest').title=row.preview_sha256||'';
 $('codec').textContent=row.codec||(row.from_sources?'Probing…':'Not recorded');$('encoder').textContent=row.encoder||(row.exported?'FFmpeg (exported)':'Original source');$('receipt').textContent=row.from_sources?(row.exported?'Exported locally':'Film Roll source'):row.kind==='original'?(row.preview_available?'Pinned preview':'No render receipt'):(row.preview_available?'Verified output':'Not verified');
 if(row.from_sources)probeFilmRoll(row,generation);
 const fsEl=$('trim-filmstrip');
 if(fsEl){fsEl.style.backgroundImage=row.from_sources?'url("/api/workspace/film-roll/'+encodeURIComponent(row.clip_id)+'/filmstrip")':'none';}
 if($('silence-status'))$('silence-status').textContent='';
 if($('multi-ship-log')){$('multi-ship-log').hidden=true;$('multi-ship-log').replaceChildren();}
 if($('draft-x'))$('draft-x').value='';
 syncReviewButtons(row);
 renderQueueProfiles();
 syncYoutubeDraftButton();
 $('drive').textContent=row.from_sources?(row.exported?'EXPORT READY':'LOCAL SOURCE'):row.state==='BACKED_UP'?'BACKUP VERIFIED':row.kind==='original'?'SOURCE INDEXED':'BACKUP NOT VERIFIED';$('drive').className='flag '+(row.state==='BACKED_UP'||row.exported?'ready':'warn');
 $('drive-detail').textContent=row.from_sources?(row.exported?'Exported MP4 saved to local exports folder. Original file untouched.':'Linked directly to downloaded file in Sources (original untouched).'):row.state==='BACKED_UP'?'Controller recorded a verified restored-media receipt.':'This is not a backup receipt.';
 if(row.preview_available){mediaEmpty('LOADING PREVIEW','READ-ONLY PLAYBACK');if(row.poster_available)video.poster=endpoint(row,'poster');video.src=endpoint(row,'stream');video.load();}
 else mediaEmpty(row.state==='test_hold'?'PRIVATE TEST HOLD':'PREVIEW UNAVAILABLE',row.preview_only?'PREVIEW ONLY · NOT FOR POSTING':'NO VERIFIED PLAYABLE FILE');
 phase5Panel();workFlow();
 window.dispatchEvent(new CustomEvent('snapmatic:clip-selected',{detail:{row}}));
}
if($('sm-speed'))$('sm-speed').onchange=()=>{video.playbackRate=Number($('sm-speed').value)||1;};
video.addEventListener('loadedmetadata',()=>{if(!selected)return;if($('sm-speed'))video.playbackRate=Number($('sm-speed').value)||1;const d=video.duration,w=video.videoWidth,h=video.videoHeight;if(Number.isFinite(d)&&d>0&&durations.get(selected.clip_id)!==d){durations.set(selected.clip_id,d);dimensions.set(selected.clip_id,w+'×'+h);renderRoll(true);}$('frame').textContent=w&&h?w+' × '+h:'—';if(Number.isFinite(d)&&d>0){seek.max=String(d);seek.disabled=false;play.disabled=false;}$('bitrate').textContent=Number.isFinite(d)&&d>0&&selected.preview_bytes?(selected.preview_bytes*8/d/1000).toFixed(0)+' kb/s (avg)':'—';$('empty').hidden=true;updateTime();if(selected.from_sources&&!derivedMode&&Number.isFinite(d)&&d>0&&(!editState||editState.duration_ms!==Math.round(d*1000))){resetEdit(selected);}refreshDraftEditor();updateTrim();updateCoverState();renderNvenc();phase5Panel();workFlow();});
video.addEventListener('timeupdate',updateTime);video.addEventListener('error',()=>{if(video.hasAttribute('src')){play.disabled=true;seek.disabled=true;mediaEmpty('PREVIEW HELD','THE LOCAL FILE DID NOT LOAD');}});
video.addEventListener('play',()=>{play.textContent='Ⅱ';play.setAttribute('aria-label','Pause clip');});video.addEventListener('pause',()=>{play.textContent='▶';play.setAttribute('aria-label','Play clip');});seek.oninput=()=>{if(Number.isFinite(video.duration))video.currentTime=Number(seek.value);};
function togglePlay(){if(play.disabled)return;if(video.paused)video.play().catch(()=>note('Playback could not start.'));else video.pause();}
play.onclick=togglePlay;
document.querySelectorAll('#filters button').forEach(b=>b.onclick=()=>{filter=b.dataset.filter;document.querySelectorAll('#filters button').forEach(x=>x.setAttribute('aria-pressed',String(x===b)));renderRoll();});
roll.addEventListener('keydown',e=>{if(e.shiftKey&&e.key==='Enter'&&e.target.closest('.clip')){const id=e.target.closest('.clip').dataset.clip;e.preventDefault();phase5.range(phase5Visible().map(r=>r.clip_id),id);renderRoll(true);return;}if(!['ArrowUp','ArrowDown'].includes(e.key)||!e.target.closest('.clip'))return;const buttons=Array.from(roll.querySelectorAll('.clip'));const i=buttons.indexOf(e.target.closest('.clip'));const next=buttons[Math.max(0,Math.min(buttons.length-1,i+(e.key==='ArrowUp'?-1:1)))];if(next){e.preventDefault();next.focus();next.click();}});
$('viewfinder').addEventListener('keydown',e=>{
 if(e.key===' '){e.preventDefault();togglePlay();}
 if(['ArrowLeft','ArrowRight'].includes(e.key)&&Number.isFinite(video.duration)){e.preventDefault();video.currentTime=Math.min(video.duration,Math.max(0,video.currentTime+(e.key==='ArrowLeft'?-1:1)));}
 if(e.key.toLowerCase()==='g'){e.preventDefault();const next=!$('grid-toggle').checked||!$('safe-toggle').checked;$('grid-toggle').checked=next;$('safe-toggle').checked=next;updateGuides();}
});
['grid-toggle','safe-toggle','mock-ui-toggle','platform-zone'].forEach(id=>{if($(id))$(id).onchange=updateGuides;});
document.addEventListener('keydown',e=>{if(e.ctrlKey&&e.key==='Enter'&&!e.target.closest('input, textarea, select')&&!$('approve').disabled){e.preventDefault();$('approve').click();}});
async function setFilmRollState(row,nextState,msg){
 const appBtn=$('approve'),holdBtn=$('hold-clip');
 appBtn.disabled=true;if(holdBtn)holdBtn.disabled=true;
 try{
  await get('/api/workspace/film-roll/'+encodeURIComponent(row.clip_id)+'/state',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({state:nextState})});
  row.state=nextState;
  $('stage-state').textContent=(labels[row.state]||row.state).toUpperCase();
  $('stage-state').className='flag '+tone(row);
  note(msg);
  await reload();refreshLocalStatus();
 }catch(e){note(e.message);await reload();}
}
$('approve').onclick=async()=>{
 const j=selected;if(!j||$('approve').disabled)return;
 if(j.from_sources){
  const nextState=j.state==='approved'?'pending_review':'approved';
  return setFilmRollState(j,nextState,nextState==='approved'?'Approved locally. Nothing was posted.':'Moved back to Pending Review.');
 }
 $('approve').disabled=true;
 try{await get('/api/controller/jobs/'+encodeURIComponent(j.id)+'/review',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({revision:j.revision,action:'approve'})});note('Approved for local review. Nothing was posted.');await reload();}catch(e){note(e.message);await reload();}
};
if($('hold-clip'))$('hold-clip').onclick=async()=>{
 const j=selected;if(!j||!j.from_sources||$('hold-clip').disabled)return;
 const nextState=j.state==='hold'?'pending_review':'hold';
 return setFilmRollState(j,nextState,nextState==='hold'?'Marked On Hold.':'Moved back to Pending Review.');
};
function reload(){
 if(indexLoad)return indexLoad;
 indexLoad=get('/api/workspace').then(d=>{
   items=[...d.jobs,...d.index];indexReady=true;p5LoadLabels();
   const previous=workflowRead('last');if(!selected&&activeView==='studio'&&previous){const hit=items.find(x=>x.clip_id===previous.id&&x.preview_sha256===previous.sha&&x.preview_available);if(hit)select(hit);} const next=selected&&items.find(x=>x.clip_id===selected.clip_id);
   if(next){selected=next;syncReviewButtons(next);}
   renderRoll(true);
   if(activeView==='gallery'){renderGallery();inspectMetadata();}
 }).catch(e=>{
   if(!indexReady){$('roll-count').textContent=e.message;$('gallery-count').textContent=e.message;}
 }).finally(()=>{indexLoad=null;});
 return indexLoad;
}
function loadLedger(){
 if(ledgerLoad)return ledgerLoad;
 ledgerLoad=get('/api/workspace/ledger').then(d=>{
   ledgerRows=d.records;ledgerReady=true;renderLedger();renderSystem();workFlow();
 }).catch(e=>{if(!ledgerReady)$('ledger-notice').textContent=e.message;}).finally(()=>{ledgerLoad=null;});
 return ledgerLoad;
}
get('/api/controller/provider-auth').then(d=>{providers=d.providers;renderProviders();refreshDraftEditor();}).catch(renderProviders);
let feed, feedEnabled=false;
const eventStages=new Map();
function retainStage(data){if(data?.clip_id===phase3Clip&&typeof data.stage==='string')loadNvenc();if(data?.stage==='intake_pending_review'&&activeView==='system')updatePhase4();if(data?.job_id&&typeof data.stage==='string'){eventStages.set(data.job_id,data.stage);if(selected?.id===data.job_id)$('stage-state').textContent=data.stage.replaceAll('_',' ').toUpperCase();}}
function stopFeed(){if(feed){feed.close();feed=null;}}
function syncFeed(){
 if(!feedEnabled || document.hidden || !document.hasFocus()){stopFeed();$('connection').textContent='EVENT FEED STANDBY';return;}
 if(feed)return;
 feed=new EventSource('/api/events');
 feed.onopen=()=>$('connection').textContent='LOCAL EVENT FEED';
 feed.onerror=()=>$('connection').textContent='EVENT FEED RECONNECTING';
 feed.addEventListener('snapshot',e=>{try{const d=JSON.parse(e.data);eventStages.clear();d.jobs.forEach(retainStage);reload();if(activeView==='ledger')loadLedger();}catch{}});
 feed.addEventListener('clip_ingested',e=>{
  try{
   const d=JSON.parse(e.data);retainStage(d);
   reload().then(()=>{
    const r=items.find(x=>x.clip_id===d.clip_id);if(!r)return;
    const inGallery=activeView==='gallery',needle=$('gallery-search').value.trim().toLocaleLowerCase();
    const filtered=!activeProfile()||!shown(r,inGallery?galleryFilter:filter)||(inGallery&&needle&&!r.name.toLocaleLowerCase().includes(needle));
    $('ingest-toast').textContent=filtered?'Clip added to Pending Review; hidden by the current filter.':'Clip added to Pending Review: '+r.name;
   }).catch(()=>{});
  }catch{}
 });
 feed.addEventListener('controller_state',e=>{try{retainStage(JSON.parse(e.data));}catch{}reload();if(activeView==='ledger')loadLedger();});
}
window.addEventListener('focus',syncFeed);
window.addEventListener('blur',()=>{stopFeed();$('connection').textContent='EVENT FEED STANDBY';});
document.addEventListener('visibilitychange',syncFeed);
setInterval(()=>{if(!document.hidden){updatePhase4();refreshLocalStatus();}},15000);
$('sm-volume').oninput=()=>{video.volume=Number($('sm-volume').value)/100;};
video.volume=Number($('sm-volume').value)/100;
document.querySelectorAll('[data-shell-link]').forEach(a=>a.onclick=e=>{
 if(e.ctrlKey||e.metaKey||e.shiftKey||e.altKey||e.button!==0)return;
 e.preventDefault();const target=a.dataset.shellLink;showView(target,true);
 const heading=document.querySelector('#'+target+' h1');if(heading){heading.tabIndex=-1;heading.focus();}
});
window.addEventListener('message',e=>{
 if(e.origin!==location.origin||e.data?.type!=='snapmatic:open-studio'||!e.data.clipId)return;
 const targetId=String(e.data.clipId);
 showView('studio',true);
 reload().then(()=>{
  const hit=items.find(x=>x.clip_id===targetId);
  if(hit){select(hit);$('viewfinder').focus();}
 }).catch(()=>{});
});
updatePhase4();
refreshLocalStatus();
get('/api/controller').then(s=>{if(s.isolated_ui_lifetime){const ping=()=>get('/api/controller/ui-lifetime',{method:'POST',headers:{'Content-Type':'application/json'},body:'{"action":"ready"}'}).catch(()=>{});ping();setInterval(ping,5000);}}).catch(()=>{});
window.addEventListener('pagehide',()=>{workflowFlush();stopFeed();cancelFrameWatch();stopVideo();});
const verifyRequested=new URLSearchParams(location.search).get('verify-film-roll')==='1';
async function timed(check,ms){
  const deadline=performance.now()+ms;
  while(!check()&&performance.now()<deadline)await new Promise(r=>setTimeout(r,50));
  return check();
}
async function verifyVisibleFilmRoll(){
  const pending=items.filter(x=>x.state==='pending_review'&&x.kind==='original');
  const hold=items.filter(x=>x.state==='test_hold');
  const proof=$('playback-proof');proof.hidden=false;
  if(pending.length!==5||hold.length!==1||hold[0].preview_available||hold[0].poster_available||
     pending.some(x=>!x.preview_available||!x.poster_available)){
    proof.append(node('li','Film Roll verification stopped: expected five playable rows and one private test hold.'));return;
  }
  const oldMute=video.muted;
  video.muted=true;
  try{
    for(let i=0;i<pending.length;i++){
      const row=pending[i];select(row);
      const turn=generation;
      const img=Array.from(roll.querySelectorAll('.clip')).find(e=>e.dataset.clip===row.clip_id)?.querySelector('img');
      if(!img)throw Error('Clip '+(i+1)+' poster missing');
      img.loading='eager';
      await img.decode();
      if(img.naturalWidth!==540||img.naturalHeight!==960)throw Error('Clip '+(i+1)+' poster dimensions did not match');
      if(!await timed(()=>video.readyState>=2||video.error!==null||turn!==generation,7000)||video.error||turn!==generation)
        throw Error('Clip '+(i+1)+' did not become ready');
      const start=video.currentTime;
      await video.play();
      const advanced=await timed(()=>video.currentTime>start+0.17||video.error!==null||turn!==generation,5000);
      const t=video.currentTime,w=video.videoWidth,h=video.videoHeight,r=video.readyState,paused=video.paused;
      if(!advanced||t<=start||paused||r<2||video.error||turn!==generation||h<=w)
        throw Error('Clip '+(i+1)+' failed actual vertical playback');
      video.pause();
      proof.append(node('li','Clip '+(i+1)+' · poster 540×960 · video '+w+'×'+h+' · readyState '+r+' · '+start.toFixed(3)+'→'+t.toFixed(3)+' s · paused=false · error=null'));
    }
    proof.append(node('li','Verified five existing previews. Private test hold was not played.'));
  }catch(e){proof.append(node('li','Stopped: '+e.message,'hold'));}
  finally{video.pause();video.muted=oldMute;}
}
document.querySelectorAll('.view-nav [data-view]').forEach(a=>a.onclick=e=>{if(!e.ctrlKey&&!e.metaKey&&!e.shiftKey){e.preventDefault();showView(a.dataset.view,true);}});
window.addEventListener('popstate',()=>showView(new URLSearchParams(location.search).get('view')));
$('gallery-search').oninput=renderGallery;
if($('gallery-source-filter'))$('gallery-source-filter').onchange=e=>{gallerySource=e.currentTarget.value;renderGallery();};
if($('gallery-sort'))$('gallery-sort').onchange=e=>{gallerySort=e.currentTarget.value;renderGallery();};
if($('gallery-clear-approved'))$('gallery-clear-approved').onclick=async()=>{
 const btn=$('gallery-clear-approved');if(btn.disabled)return;
 btn.disabled=true;
 try{
  const r=await get('/api/workspace/film-roll/clear',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({state:'approved'})});
  if(selected?.state==='approved'){selected=null;stopVideo();syncReviewButtons(null);}
  await reload();refreshLocalStatus();
  if(r.removed>0)note('Cleared '+r.removed+' approved clip'+(r.removed===1?'':'s')+' from Film Roll (original files kept on disk).');
 }catch(e){note(e.message);}
 finally{renderGallery();}
};
document.querySelectorAll('#gallery-filters button').forEach(b=>b.onclick=()=>{galleryFilter=b.dataset.filter;document.querySelectorAll('#gallery-filters button').forEach(x=>x.setAttribute('aria-pressed',String(x===b)));renderGallery();});
$('ledger-search').oninput=renderLedger;

get('/api/workspace/system').then(d=>{systemData=d;renderSystem();}).catch(renderSystem);
async function verifyGalleryThenStudio(){
  showView('gallery');const out=$('gallery-proof');out.hidden=false;
  const cards=Array.from($('gallery-items').querySelectorAll('.gallery-card'));
  try {
    if(cards.length!==6)throw Error('Expected five pending cards and one private hold');
    for(const r of items.filter(x=>x.state==='pending_review')){
      const im=cards.find(x=>x.dataset.clip===r.clip_id)?.querySelector('img');
      if(!im)throw Error('Gallery poster missing');
      im.loading='eager';await im.decode();if(im.naturalWidth!==540||im.naturalHeight!==960)throw Error('Gallery poster dimensions invalid');
    }
    out.append(node('li','Five Gallery posters decoded at 540×960; one private test hold has no poster.'));
    const first=items.find(x=>x.state==='pending_review');
    cards.find(x=>x.dataset.clip===first?.clip_id)?.click();
    if(selected?.clip_id!==first?.clip_id||activeView!=='studio')throw Error('Gallery did not open the exact clip in Studio');
    await verifyVisibleFilmRoll();
    $('playback-proof').prepend(node('li','Gallery decoded all five posters and opened the exact selected Studio clip.'));
  }catch(e){out.append(node('li','Stopped: '+e.message,'hold'));}
}
if($('roll-search'))$('roll-search').oninput=()=>renderRoll();
if($('gallery-export-approved'))$('gallery-export-approved').onclick=async()=>{
 const btn=$('gallery-export-approved');if(btn.disabled)return;
 btn.disabled=true;
 try{
  const body={
   state:'approved',
   framing:$('edit-framing')?.value||'original',
   quality:$('edit-quality')?.value||'balanced',
   mute:Boolean($('edit-mute')?.checked),
   normalize_audio:Boolean($('edit-norm')?.checked)&&!Boolean($('edit-mute')?.checked),
   use_hw_encode:Boolean($('edit-hw')?.checked),
   save_caption_txt:Boolean($('edit-pack')?.checked),
   hook_text:($('edit-hook')?.value||'').trim()||null,
   watermark_text:($('edit-watermark')?.value||'').trim()||null,
   watermark_pos:$('edit-wm-pos')?.value||'bottom_right',
   fade_io:Boolean($('edit-fade-io')?.checked),
   vivid_filter:Boolean($('edit-vivid')?.checked),
   speed:Number($('edit-export-speed')?.value)||1,
  };
  const r=await get('/api/workspace/film-roll/export-batch',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
  note('Started batch export of '+r.total+' approved clip'+(r.total===1?'':'s')+'.');
  startExportPoll();
 }catch(e){note('Batch export: '+e.message);renderGallery();}
};
const params=new URLSearchParams(location.search);
renderProviders();updateGuides();showView(params.get('view'));
$('ledger-notice').textContent='Loading verified local receipts...';
Promise.all([reload(),loadLedger(),loadExportHistory(),loadPresets(),loadNvenc(),refreshLocalStatus(),refreshOAuthStatus()]).then(()=>{
 feedEnabled=true;syncFeed();
 get('/api/workspace/film-roll/export-progress').then(p=>{if(p?.active){updateExportProgressUI(p);startExportPoll();}}).catch(()=>{});
 if(params.get('verify-workspace')==='1')verifyGalleryThenStudio();else if(verifyRequested)verifyVisibleFilmRoll();
});
window.addEventListener('message',e=>{if(e.origin===location.origin&&e.data?.type==='snapmatic:oauth-updated'){insightsAutoSynced=false;refreshOAuthStatus().then(()=>syncInsights());loadChannelProfiles();}});
window.addEventListener('focus',()=>{if(activeView==='system')refreshOAuthStatus();});

const p5Labels=new Map();
const p5Key=row=>scope+':'+row.clip_id+':'+(row.media_sha256||row.preview_sha256);
const p5Actions=new Set(['Approve','Transcode','Hold','Archive']);
const batchFilmActions=new Set(['approve','hold','pending_review','remove']);
let p5Read=null,p5ReadScope=null,p5Write=false,p5Generation=0;
function p5Apply(response,which){
 if(scope!==which||response.profile_id!==which||response.schema!=='snapmatic-triage-annotations-v1'||
  response.changes_review_eligibility!==false||response.dispatch_enabled!==false||response.publishing_enabled!==false||!Array.isArray(response.annotations))return false;
 const next=response.annotations.filter(r=>p5Actions.has(r.label)&&/^[0-9a-f]{64}$/.test(r.clip_id)&&/^[0-9a-f]{64}$/.test(r.media_sha256));
 for(const k of p5Labels.keys())if(k.startsWith(which+':'))p5Labels.delete(k);
 for(const row of next)p5Labels.set(p5Key(row),row.label);
 return true;
}
function p5LoadLabels(){
 if(scope!=='demo-channel'||!indexReady||p5Write||(p5Read&&p5ReadScope===scope))return;
 const which=scope,seq=++p5Generation;
 p5ReadScope=which;
 p5Read=get('/api/workspace/triage?profile_id='+encodeURIComponent(which)).then(data=>{
   if(seq===p5Generation&&p5Apply(data,which))renderRoll(true);
 }).catch(()=>{}).finally(()=>{if(p5ReadScope===which){p5Read=null;p5ReadScope=null;}});
}
function phase5Visible(){
 if(!activeProfile())return [];
 const list=items.filter(r=>shown(r)&&r.preview_available);
 return list.filter(r=>r.from_sources||(scope==='demo-channel'&&['original','intake_original'].includes(r.kind)&&r.state==='pending_review'));
}
function phase5Facts(){
 if(!selected)return null;
 if(derivedMode&&nvenc?.receipt?.source_sha256===selected.preview_sha256&&nvenc?.receipt?.sha256==='3c45bfc8aadcaaae1bef5c5beda8525899e8113254efde014019b5cde3ea7e89')
   return {streams:[{codec_type:'video',avg_frame_rate:'30/1',r_frame_rate:'30/1'}]};
 const facts=mediaFacts[selected.preview_sha256];
 if(!facts)return null;
 if(selected.from_sources)return facts;
 return Number(facts?.format?.size)===selected.preview_bytes?facts:null;
}
function phase5SourcePair(){
 if(selected?.from_sources&&selected.preview_available&&selected.exported&&selected.export_url)return true;
 return !!(selected?.kind==='original'&&selected.preview_available&&selected.clip_id===phase3Clip&&
  nvenc?.stage==='ready'&&nvenc.job_id&&nvenc.receipt?.source_sha256===selected.preview_sha256);
}

const p5Wave={key:null,peaks:null,error:null,controller:null,loading:false};
function p5WaveScope(){
 if(!selected?.preview_available)return null;
 if(derivedMode&&phase5SourcePair()){
  if(selected.from_sources)return null;
  return {key:nvenc.receipt.sha256,bytes:nvenc.receipt.bytes,url:'/api/workspace/local-preview/'+encodeURIComponent(nvenc.job_id)+'/video'};
 }
 return {key:selected.preview_sha256,bytes:selected.preview_bytes,url:endpoint(selected,'stream')};
}
function p5WaveReset(key){
 if(p5Wave.key===key)return;
 p5Wave.controller?.abort();Object.assign(p5Wave,{key,peaks:null,error:null,controller:null,loading:false});
}
async function p5LoadWave(){
 const scope=p5WaveScope(),d=video.duration;
 if(!scope||$('p5-waveform-load').disabled||!Number.isFinite(d)||d>45)return;
 const c=new AbortController();let reader;
 Object.assign(p5Wave,{controller:c,loading:true,error:null});phase5Panel();
 let timer;
 try{
  const timeout=new Promise((_,reject)=>{timer=setTimeout(()=>{c.abort();reject(new Error('Waveform decode timed out'));},10000);});
  const decode=async()=>{
   const r=await fetch(scope.url,{signal:c.signal,redirect:'error',credentials:'same-origin'});
   if(!r.ok||Number(r.headers.get('content-length'))!==scope.bytes||!r.body)throw Error('Exact media length unavailable');
   reader=r.body.getReader();const bytes=new Uint8Array(scope.bytes);let cursor=0;
   for(;;){const {value,done}=await reader.read();if(done)break;if(cursor+value.byteLength>bytes.length)throw Error('Media exceeds waveform limit');bytes.set(value,cursor);cursor+=value.byteLength;}
   if(cursor!==bytes.length)throw Error('Media shortened during waveform read');
   if(c.signal.aborted)throw Error('Waveform request stopped');
   const audio=await new OfflineAudioContext(1,1,16000).decodeAudioData(bytes.buffer);
   if(audio.duration>45.1||audio.length>721600||audio.numberOfChannels<1||audio.numberOfChannels>8||audio.length<128)throw Error('Decoded audio outside waveform limits');
   const channels=Array.from({length:audio.numberOfChannels},(_,i)=>audio.getChannelData(i));
   return Array.from({length:128},(_,i)=>{let peak=0;const start=Math.floor(i*audio.length/128),end=Math.floor((i+1)*audio.length/128);
     for(let j=start;j<end;j++)for(const channel of channels)peak=Math.max(peak,Math.abs(channel[j]));return Math.min(1,peak);
   });
  };
  const peaks=await Promise.race([timeout,decode()]);
  if(p5Wave.controller===c)p5Wave.peaks=peaks;
 }catch{if(p5Wave.controller===c)p5Wave.error='Local waveform unavailable (unsupported audio, incomplete media, or time limit)';}
 finally{clearTimeout(timer);if(reader)reader.cancel().catch(()=>{});if(p5Wave.controller===c){p5Wave.loading=false;p5Wave.controller=null;phase5Panel();}}
}
$('p5-waveform-load').onclick=p5LoadWave;

function phase5Panel(){
 const pair=phase5SourcePair(), facts=phase5Facts(), d=video.duration;
 $('p5-source').disabled=!pair||!derivedMode;
 $('p5-render').disabled=!pair||derivedMode;
 $('p5-split').disabled=!pair;if(!pair)phase5Compare(false);
 else if(!$('p5-compare').hidden)phase5Compare(true);
 $('p5-source').setAttribute('aria-pressed',String(!derivedMode&&!!selected?.preview_available));
 $('p5-render').setAttribute('aria-pressed',String(derivedMode));
 $('p5-step-back').disabled=$('p5-step-next').disabled=phase5.step(video.currentTime,d,1,facts)===null;
 const ready=!!editState&&!derivedMode&&Number.isFinite(d)&&editState.out_ms/1000<=d+.001;
 $('p5-loop').disabled=!ready;if(!ready)$('p5-loop').checked=false;
 const boundSpeech=!derivedMode&&selected?.clip_id===phase3Clip&&nvenc?.request?.transcript_sha256&&nvenc.request.source_sha256===selected.preview_sha256?ed.grounded(nvenc.segments,editState?.duration_ms):null;
 const ts=selected?.clip_id===phase3Clip&&nvenc?.timeline?.source_sha256===selected.preview_sha256?nvenc.timeline:null;
 const rsp=ts&&Array.isArray(ts.speech_segments)?(derivedMode?ts.speech_segments.filter(x=>x.end_ms>ts.in_ms&&x.start_ms<ts.out_ms).map(x=>({start_ms:Math.max(x.start_ms,ts.in_ms)-ts.in_ms,end_ms:Math.min(x.end_ms,ts.out_ms)-ts.in_ms})):ts.speech_segments):null;
 const m=phase5.metadata({...facts,speech_segments:boundSpeech??rsp??facts?.speech_segments},d);
 const scope=p5WaveScope();p5WaveReset(scope?.key??null);
 const bounded=!!scope?.key&&Number.isInteger(scope?.bytes)&&scope.bytes>0&&scope.bytes<=40*1024*1024&&Number.isFinite(d)&&d>0&&d<=45;
 $('p5-waveform-load').disabled=!bounded||p5Wave.loading||!!p5Wave.peaks||!!m.peaks;
 const peaks=m.peaks??p5Wave.peaks;
 const out=$('p5-scrub');
 const bars=peaks?.map((v,i)=>{const e=node('i');e.style.left=(100*i/peaks.length)+'%';e.style.width=(100/peaks.length)+'%';e.style.height=(100*v)+'%';return e;})??[];
 const speech=m.speech?.map(v=>{const e=node('b');e.style.left=(v.start_ms/10/d)+'%';e.style.width=((v.end_ms-v.start_ms)/10/d)+'%';return e;})??[];
 out.replaceChildren(...bars,...speech);
 $('p5-scrub-note').textContent=(m.peaks?'Verified producer waveform':p5Wave.peaks?'Waveform decoded from this exact local clip':p5Wave.loading?'Decoding local audio…':p5Wave.error??(bounded?'Load local waveform on demand':'Waveform unavailable (clip exceeds local limits or has no verified duration)'))+' · '+(m.speech?m.speech.length+' verified speech highlights':'Speech highlights unavailable');
}
function phase5Swap(mode){
 if(!phase5SourcePair()||mode===(derivedMode?'render':'source'))return;
 const t=video.currentTime,split=!$('p5-compare').hidden;
 if(mode==='source')select(selected);else $('edit-open-derived').click();
 if(split)phase5Compare(true);
 if(t>0)video.addEventListener('loadedmetadata',()=>{if(Number.isFinite(video.duration))video.currentTime=Math.min(t,Math.max(0,video.duration-.001));},{once:true});
 phase5Panel();workFlow();
 window.dispatchEvent(new CustomEvent('snapmatic:clip-selected',{detail:{row}}));
}
function phase5Step(direction){
 const to=phase5.step(video.currentTime,video.duration,direction,phase5Facts());
 if(to===null)return false;
 video.pause();video.currentTime=to;return true;
}
function phase5Tray(){
 const avail=phase5Visible().filter(r=>phase5.batch.has(r.clip_id)).length;
 $('p5-batch-size').textContent=avail?avail+' clip'+(avail===1?'':'s')+' selected for batch action':'Shift+Click or Shift+Enter to select a range';
 $('p5-batch').querySelectorAll('button').forEach(b=>b.disabled=p5Write||avail===0||avail>200);
}
function phase5Compare(show){
 const pane=$('p5-compare'), alt=$('p5-compare-video');
 if(!show||!phase5SourcePair()){
   pane.hidden=true;$('p5-split').setAttribute('aria-pressed','false');
   if(alt.hasAttribute('src')){alt.pause();alt.removeAttribute('src');alt.load();}return;
 }
 const exportSrc=selected.from_sources?selected.export_url:'/api/workspace/local-preview/'+encodeURIComponent(nvenc.job_id)+'/video';
 const src=derivedMode?endpoint(selected,'stream'):exportSrc;
 pane.hidden=false;$('p5-split').setAttribute('aria-pressed','true');
 $('p5-compare-label').textContent=derivedMode?'Source (muted)':'Exported (muted)';
 if(alt.getAttribute('src')!==src){$('p5-compare-state').textContent='Loading comparison';alt.src=src;alt.load();}
 phase5CompareSync();
}
function phase5CompareSync(){
 const alt=$('p5-compare-video');
 if($('p5-compare').hidden||!Number.isFinite(alt.duration))return;
 const a=selected?.from_sources?(editState?.in_ms||0):(nvenc?.timeline?.in_ms||0);
 const offset=(derivedMode?1:-1)*a/1000,target=video.currentTime+offset;
 const inRange=target>=0&&target<alt.duration;
 $('p5-compare-state').textContent=inRange?'Primary view supplies audio':'No corresponding frame in this trim';
 alt.style.visibility=inRange?'visible':'hidden';
 if(!inRange){alt.pause();return;}
 if(!alt.seeking&&Math.abs(alt.currentTime-target)>(video.paused ? .015 : .12))alt.currentTime=target;
 alt.playbackRate=video.playbackRate;
 if(video.paused)alt.pause();else if(alt.paused)alt.play().catch(()=>{$('p5-compare-state').textContent='Comparison playback could not start';});
}
$('p5-split').onclick=()=>phase5Compare($('p5-compare').hidden);
$('p5-compare-video').addEventListener('loadedmetadata',phase5CompareSync);
$('p5-compare-video').addEventListener('error',()=>{if(!$('p5-compare').hidden)$('p5-compare-state').textContent='Comparison video could not be decoded';});
for(const x of ['timeupdate','seeked','play','pause','ratechange'])video.addEventListener(x,phase5CompareSync);
$('p5-source').onclick=()=>phase5Swap('source');$('p5-render').onclick=()=>phase5Swap('render');
$('p5-step-back').onclick=()=>phase5Step(-1);$('p5-step-next').onclick=()=>phase5Step(1);
function phase5Loop(){
 if(!$('p5-loop').checked||!editState||derivedMode||video.paused)return;
 const to=phase5.loop(editState.in_ms/1000,editState.out_ms/1000,video.duration,video.currentTime);
 if(to!==null&&!video.seeking)video.currentTime=to;
}
let phase5Frame=null;
function phase5Tick(){phase5Frame=null;if(video.paused||video.ended)return;phase5Loop();phase5Frame=video.requestVideoFrameCallback(phase5Tick);}
function phase5Cancel(){if(phase5Frame!==null){video.cancelVideoFrameCallback(phase5Frame);phase5Frame=null;}}
video.addEventListener('play',()=>{phase5Loop();if(phase5Frame===null&&typeof video.requestVideoFrameCallback==='function')phase5Frame=video.requestVideoFrameCallback(phase5Tick);});
video.addEventListener('pause',phase5Cancel);video.addEventListener('emptied',phase5Cancel);
video.addEventListener('ended',()=>{if($('p5-loop').checked&&!$('p5-loop').disabled&&editState&&!derivedMode){video.currentTime=editState.in_ms/1000;video.play().catch(()=>{});}});
video.addEventListener('timeupdate',phase5Loop);
video.addEventListener('loadedmetadata',phase5Panel);
$('p5-batch').querySelectorAll('[data-triage]').forEach(b=>b.onclick=async()=>{
 if(b.disabled||p5Write)return;
 const action=b.dataset.triage;
 const picked=phase5Visible().filter(r=>phase5.batch.has(r.clip_id));
 if(!picked.length)return;
 if(batchFilmActions.has(action)&&picked.every(r=>r.from_sources)){
  p5Write=true;phase5Tray();const result=$('p5-batch-result');
  result.textContent='Applying batch action…';
  try{
   const clip_ids=picked.map(r=>r.clip_id);
   const d=await get('/api/workspace/film-roll/batch',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({clip_ids,action})});
   if(action==='remove'&&selected&&clip_ids.includes(selected.clip_id)){selected=null;stopVideo();syncReviewButtons(null);}
   phase5.clear();
   const msg=(action==='remove'?'Removed ':'Updated ')+d.affected+' clip'+(d.affected===1?'':'s')+(action==='remove'?' from Film Roll (files kept on disk).':' ('+action.replace('_',' ')+').');
   result.textContent=msg;note(msg);
   await reload();refreshLocalStatus();
  }catch(e){result.textContent='Batch action failed: '+e.message;note(e.message);}
  finally{p5Write=false;renderRoll(true);}
  return;
 }
 const which=scope;
 const annotations=picked.map(r=>({clip_id:r.clip_id,media_sha256:r.preview_sha256,label:action}));
 if(!p5Actions.has(action)||!annotations.length||annotations.length>32)return;
 p5Write=true;++p5Generation;phase5Tray();const result=$('p5-batch-result');
 result.textContent='Saving local annotations…';
 try{
  const d=await get('/api/workspace/triage',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({profile_id:which,annotations})});
  if(scope===which){
   if(!p5Apply(d,which))throw Error('Local annotation response was not verified');
   phase5.clear();
   result.textContent='Saved '+annotations.length+' local labels. Review eligibility, media and publishing are unchanged.';
  }
 }catch(e){if(scope===which)result.textContent='Local annotation was not verified: '+e.message;}
 finally{p5Write=false;renderRoll(true);if(scope!==which)p5LoadLabels();}
});
const p5Dialogs=[$('p5-command'),$('p5-help')];
const p5Commands=[
 ['Viewfinder Studio',()=>showView('studio',true)],['Film Roll Gallery',()=>showView('gallery',true)],
 ['Channel Insights',()=>showView('insights',true)],['Content Calendar',()=>showView('calendar',true)],
 ['Sync Live Analytics',()=>{showView('insights',true);syncInsights();}],
 ['Exports & Receipts',()=>showView('ledger',true)],['Connections & System',()=>showView('system',true)],
 ['Profiles',()=>showView('profiles',true)],['Sources',()=>showView('sources',true)],
 ['Inspector: Captions',()=>{showView('studio',true);setInspectorTab('captions');}],
 ['Inspector: Edit & Export',()=>{showView('studio',true);setInspectorTab('export');}],
 ['Inspector: Upload',()=>{showView('studio',true);setInspectorTab('upload');}],
 ['Inspector: Checks',()=>{showView('studio',true);setInspectorTab('checks');}],
 ['Toggle Platform UI Preview',()=>{if($('mock-ui-toggle')){$('mock-ui-toggle').checked=!$('mock-ui-toggle').checked;updateGuides();}},()=>activeView==='studio'],
 ['Auto-Trim Dead Air (Silence Detect)',()=>$('trim-detect-silence')?.click(),()=>Boolean(selected?.from_sources&&!$('trim-detect-silence')?.disabled)],
 ['Copy All Platform Captions',()=>$('copy-all-captions')?.click(),()=>Boolean(selected)],
 ['Upload Draft to YouTube',()=>$('yt-upload-draft')?.click(),()=>Boolean(selected?.from_sources&&oauthStatus?.youtube?.connected)],
 ['Upload Instagram Reel (Test Container)',()=>$('ig-upload-container')?.click(),()=>Boolean(selected?.from_sources&&oauthStatus?.instagram?.connected)],
 ['Send Draft to TikTok Inbox',()=>$('tt-upload-draft')?.click(),()=>Boolean(selected?.from_sources&&oauthStatus?.tiktok?.connected)],
 ['Toggle Light / Dark Theme',()=>applyShellTheme(document.documentElement.dataset.theme==='light'?'dark':'light')],
 ['Open Exports Folder',()=>$('edit-open-exports')?.click()],
 ['Toggle Source / Exported',()=>phase5Swap(derivedMode?'source':'render'),phase5SourcePair],
 ['Toggle Split Comparison',()=>phase5Compare($('p5-compare').hidden),phase5SourcePair],
 ['Toggle Loop In / Out',()=>{$('p5-loop').checked=!$('p5-loop').checked;phase5Loop();},()=>!$('p5-loop').disabled&&activeView==='studio'],
 ['Keyboard shortcuts',()=>openPhase5($('p5-help'))]
];
let p5Focus=null;
function openPhase5(dialog){
 const prior=p5Dialogs.find(d=>d.open);if(prior){if(prior===dialog){prior.close();return;}prior.close();}
 p5Focus=document.activeElement;
 dialog.showModal();if(dialog===$('p5-command')){$('p5-search').value='';p5Results();$('p5-search').focus();}
}
function p5Results(){
 const query=$('p5-search').value.trim().toLowerCase();
 const rows=p5Commands.filter(([name,,on])=>name.toLowerCase().includes(query)&&(!on||on()));
 $('p5-results').replaceChildren(...rows.map(([name,go])=>{const b=node('button',name);b.type='button';b.onclick=()=>{$('p5-command').close();go();};return b;}));
 if(!rows.length)$('p5-results').append(node('p','No matching commands.'));
}
$('p5-search').oninput=p5Results;
$('p5-search').onkeydown=e=>{if(e.key==='ArrowDown'){e.preventDefault();$('p5-results').querySelector('button')?.focus();}};
$('p5-open-command').onclick=()=>openPhase5($('p5-command'));
$('p5-open-help').onclick=()=>openPhase5($('p5-help'));
for(const d of p5Dialogs){d.querySelector('[data-close]').onclick=()=>d.close();d.addEventListener('close',()=>{if(!p5Dialogs.some(x=>x.open))p5Focus?.focus();});}
document.addEventListener('keydown',e=>{
 if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='k'){e.preventDefault();openPhase5($('p5-command'));return;}
 if(e.target.closest('input,textarea,select,[contenteditable]')||p5Dialogs.some(d=>d.open)||e.altKey||e.ctrlKey||e.metaKey)return;
 if(e.key==='?'){e.preventDefault();openPhase5($('p5-help'));return;}
 if(activeView!=='studio')return;
 const kl=e.key.toLowerCase();
 if(kl==='k'&&!play.disabled){e.preventDefault();togglePlay();return;}
 if(kl==='j'&&Number.isFinite(video.duration)){e.preventDefault();video.currentTime=Math.max(0,video.currentTime-2);return;}
 if(kl==='l'&&!play.disabled){
  e.preventDefault();
  if(video.paused){video.playbackRate=1;if($('sm-speed'))$('sm-speed').value='1';video.play().catch(()=>{});}
  else{const next=video.playbackRate<1.5?1.5:video.playbackRate<2?2:1;video.playbackRate=next;if($('sm-speed'))$('sm-speed').value=String(next);note('Playback speed: '+next+'×');}
  return;
 }
 if(kl==='m'){e.preventDefault();video.muted=!video.muted;note(video.muted?'Preview muted.':'Preview unmuted.');return;}
 if(e.key==='\\'&&phase5SourcePair()){e.preventDefault();phase5Swap(derivedMode?'source':'render');}
 if((e.key===','||e.key==='.')&&phase5Step(e.key===','?-1:1))e.preventDefault();
});
window.snapmaticApp={
 getSelected:()=>selected,
 getItems:()=>items,
 getVisibleItems:()=>items.filter(x=>x.preview_available&&shown(x)),
 getOAuth:()=>oauthStatus,
 getEditState:()=>editState,
 getMediaFacts:()=>mediaFacts,
 getPDState:()=>pdState,
 getCoverUrl:()=>coverUrl,
 setCoverBlob:(blob,frame_ms,thumb_offset_ms)=>{
  if(coverUrl)URL.revokeObjectURL(coverUrl);
  coverUrl=URL.createObjectURL(blob);
  if(pdState){pdVersion++;pdState.cover={frame_ms:frame_ms||0,thumb_offset_ms:thumb_offset_ms??frame_ms??0};}
  const img=$('cover-image'),link=$('cover-download');
  if(img){img.src=coverUrl;img.hidden=false;}
  if(link){link.href=coverUrl;link.download='snapmatic-cover-'+(selected?.clip_id?.slice(0,12)||'clip')+'.png';link.hidden=false;}
  updateCoverState();workflowDirty();
 },
 getVideo:()=>video,
 select,setTrim,reload,loadExportHistory,loadPresets,refreshLocalStatus,refreshOAuthStatus,
 renderRoll,renderGallery,renderNvenc,renderPreflight,refreshDraftEditor,syncYoutubeDraftButton,
 setInspectorTab,showView,updateGuides,updateOverlayPreview,buildCaptionPackText,coverBase64,
 xPostText,note,get,node,brandIcon,time,pd,workflowDirty,
 getCalendarItems:()=>calendarItems,loadCalendar,
 getAnalyticsData:()=>analyticsData,loadInsights,syncInsights,
};
})();
