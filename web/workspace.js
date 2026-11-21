'use strict';
(() => {
const $=id=>document.getElementById(id);
const video=$('video'), seek=$('seek'), play=$('play'), roll=$('roll');
const labels={pending_review:'Pending review',test_hold:'Private test hold',duplicate_hold:'Duplicate hold',QUEUED:'Queued',PROCESSING:'Processing',READY_FOR_REVIEW:'Ready for review',HELD:'Held',FAILED:'Failed',POSTED:'Posted',BACKED_UP:'Posted · backup verified'};
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
let channelProfiles=[], scope='demo-channel';
function activeProfile(){return scope==='demo-channel'||channelProfiles.some(p=>p.profile_id===scope&&p.configured);}
function refreshScope(){
 phase5.clear();p5LoadLabels();
 renderProviders();if(indexReady)renderRoll();if(activeView==='gallery')renderGallery();if(activeView==='ledger')renderLedger();
 if(!activeProfile()){saveDraft();selected=null;pdState=null;editState=null;generation++;cancelFrameWatch();stopVideo();refreshDraftEditor();updateTrim();$('approve').disabled=true;mediaEmpty('CHANNEL NOT CONFIGURED','NO CREDENTIAL OR SOURCE ALIAS VERIFIED');}
}
$('channel-scope').onchange=()=>{scope=$('channel-scope').value;sessionStorage.setItem('snapmatic-profile',scope);refreshScope();};
get('/api/workspace/channel-profiles').then(d=>{
 channelProfiles=d.profiles;
 const saved=sessionStorage.getItem('snapmatic-profile');if(channelProfiles.some(p=>p.profile_id===saved))scope=saved;
 $('channel-scope').replaceChildren(...channelProfiles.map(p=>{const o=node('option',p.channel_alias+(p.configured?'':' (not configured)'));o.value=p.profile_id;return o;}));
 $('channel-scope').value=scope;refreshScope();
}).catch(()=>{});
$('export-diagnostics').onclick=async()=>{
 const b=$('export-diagnostics'),out=$('diagnostics-result');b.disabled=true;
 try{const r=await get('/api/workspace/diagnostics',{method:'POST'});out.textContent='Saved '+r.file+' to '+r.location+'. Nothing shared.';}catch(e){out.textContent=e.message;}finally{b.disabled=false;}
};
const providerNames={youtube:'YouTube',instagram:'Instagram',tiktok:'TikTok'};
const guides={tiktok:['10% 18% 23% 7%','TikTok guide','18%','31%'],reels:['11% 16% 24% 7%','Reels guide','16%','32%'],shorts:['9% 18% 22% 7%','Shorts guide','18%','29%']};
const drafts=new Map(), durations=new Map(), dimensions=new Map(), inspected=new Map([['tiktok',true],['instagram',true],['youtube',true]]);
let items=[],providers=[],selected=null,filter='all',galleryFilter='all',generation=0,refreshing=false,ledgerRows=[],systemData=null,activeView='studio',indexReady=false,ledgerReady=false,indexLoad=null,ledgerLoad=null;
function node(tag,text,cls){const e=document.createElement(tag);if(text!==undefined)e.textContent=text;if(cls)e.className=cls;return e;}
async function get(url,opts){const r=await fetch(url,{cache:'no-store',credentials:'same-origin',redirect:'error',...opts});if(!r.ok){let reason='The local Hub did not accept this request.';try{reason=(await r.json()).reason||reason;}catch{}throw Error(reason);}return r.json();}
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
function renderNvenc(){
 const target=selected?.clip_id===phase3Clip,b=$('edit-transcode'),open=$('edit-open-derived');
 const r=target?requestFromEditor():null;
 const matches=r&&requestStable(r)===requestStable(nvenc.request);
 b.disabled=!target||!nvenc?.can_dispatch||!matches;
 const audio=target?nvenc?.receipt?.audio:null;
 $('edit-lufs').textContent=audio?.status==='measured'?Number(audio.integrated_lufs).toFixed(2)+' LUFS':audio?.status==='silence'?'Measured silence':'Not measured';
 $('edit-peak').textContent=audio?.status==='measured'?Number(audio.true_peak_dbtp).toFixed(2)+' dBTP':audio?.status==='silence'?'Measured silence':'Not measured';
 open.hidden=!(target&&nvenc?.stage==='ready'&&/^[a-f0-9]{64}$/.test(nvenc?.job_id||''));
 $('edit-state').textContent=!target?'Select the reviewed AV1 clip. Publishing is off.':
   nvenc?.can_dispatch?(matches?'Exact reviewed source, trim and framing. GPU readiness is rechecked before encoding.':'Edits differ from the independently reviewed request. Export is held.'):
   (ed.stageLabels[nvenc?.stage]||(nvenc?.reason==='preview_plan_unconfigured'?'No independently verified local worker plan.':'Local preview not available'))+
   (nvenc?.failure?' · '+nvenc.failure:'');
}
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
   nvenc=v;$('sm-live-nvenc').textContent=v.stage==='ready'?'NVENC · verified preview ready':v.stage==='reviewed'?'NVENC · reviewed plan, runtime unchecked':'NVENC · '+(v.stage||'not verified');
   if(selected?.clip_id===phase3Clip){approvedSeed();updateTrim();showTranscript();}
   renderNvenc();if(activeView==='gallery')renderGallery();phase5Panel();
 }).catch(()=>{}).finally(()=>{nvencLoading=null;});return nvencLoading;
}
$('edit-transcode').onclick=async()=>{
 const button=$('edit-transcode'),r=requestFromEditor();
 if(button.disabled||!r)return;
 button.disabled=true;
 try{const result=await get('/api/workspace/local-preview',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(r)});
   if(result.publishing_enabled!==false)throw Error('Unexpected publishing status');
   nvenc={...nvenc,...result,can_dispatch:false};renderNvenc();
 }catch(e){$('edit-state').textContent='Held: '+e.message;}
};
$('edit-open-derived').onclick=()=>{
 if($('edit-open-derived').hidden||!nvenc||selected?.clip_id!==phase3Clip)return;
 generation++;cancelFrameWatch();stopVideo();derivedMode=true;editState=null;updateTrim();pdState=null;refreshDraftEditor();
 video.src='/api/workspace/local-preview/'+encodeURIComponent(nvenc.job_id)+'/video';video.load();
 $('inspect-state').textContent='Verified H.264 local derivative only · original unchanged';
 renderNvenc();
};

function knownDuration(row){
 const f=mediaFacts[row?.preview_sha256]?.format,d=Number(f?.duration);
 return row&&Number(f?.size)===row.preview_bytes&&d>0&&Number.isFinite(d)?Math.ceil(d*1000):null;
}
function resetEdit(row){
 const d=row?.clip_id===phase3Clip?20800:knownDuration(row),key=row?.clip_id+':'+row?.preview_sha256;
 editState=d?(editSessions.get(key)||{clip_id:row.clip_id,media_sha256:row.preview_sha256,duration_ms:d,in_ms:0,out_ms:d,framing:'center_crop',facecam:[],gameplay:[]}):null;
 if(editState)editSessions.set(key,editState);approvedSeed();
 $('edit-framing').value=editState?.framing||'center_crop';$('edit-zones').hidden=$('edit-framing').value!=='dual_zone';
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
}
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
$('edit-framing').onchange=()=>{if(editState){editState.framing=$('edit-framing').value;editState.dirty=true;}$('edit-zones').hidden=$('edit-framing').value!=='dual_zone';renderNvenc();};
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
$('draft-preset').onclick=()=>{if(pdState){pdVersion++;$('tags').value=pd.mergeTags($('tags').value,pd.preset);refreshDraftEditor();renderPreflight();}};
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
 const real=selected&&(ttSHA===selected.preview_sha256?video.duration:Number(exact?.size)===selected.preview_bytes?Number(exact.duration):null);
 $('cover-shutter').disabled=!pdState||!frameToken||frameToken.turn!==generation||video.seeking||video.readyState<2||!real;
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
 const row=selected,token=frameToken;
 if($('cover-shutter').disabled||!row||!pdState||!token)return;
 const f=mediaFacts[row.preview_sha256]?.format||{duration:ttSHA===row.preview_sha256?video.duration:null};
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
 $('draft-tt-capacity').textContent=!row?'Select a clip.':row.kind!=='rendered'?'Preview only — not export eligible.':'Held pending exact export verification.';
 if(!activeProfile()||row?.kind!=='rendered'||row.state!=='READY_FOR_REVIEW')return;
 ttCheck=get('/api/workspace/tiktok/export/'+encodeURIComponent(id)).then(x=>{
  if(ttKey!==k||!x.ready||x.sha256!==row.preview_sha256)return;
  ttSHA=x.sha256;$('draft-tt-capacity').textContent='EXPORT READY · verified local MP4';$('tt-export').disabled=false;updateCoverState();workFlow();
 }).catch(()=>{});
}
$('tt-copy').onclick=async()=>{try{await navigator.clipboard.writeText(ttText());$('tt-result').textContent='Caption and hashtags copied.';}catch{$('tt-result').textContent='Clipboard unavailable. Copy the editor text manually.';}};
$('tt-folder').onclick=async()=>{try{const r=await fetch('/api/workspace/tiktok/export-folder',{method:'POST'});if(!r.ok)throw Error();$('tt-result').textContent='Opened the local export folder.';}catch{$('tt-result').textContent='Could not open the local export folder.';}};
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
 const ready=Boolean(r&&ttSHA&&r.preview_sha256===ttSHA),review=r?.kind==='rendered'&&r.state==='READY_FOR_REVIEW'&&r.review==='PENDING';
 const renderer=r?.clip_id===phase3Clip&&!$('edit-transcode').disabled;
 const hold=/hold|HELD|FAILED/i.test(r?.state||'');
 const protectedHold=r?.state==='test_hold'&&String(r.name).includes('1000000000000000001')&&ledgerRows.some(x=>x.job_id==='00000000-0000-4000-8000-000000000001'&&x.verified_export_format?.measured_aac_bps===127605);
 const stages=items.filter(x=>x.state==='pending_review');
 $('wf-count').textContent=stages.length+' in review · '+items.filter(x=>x.state==='test_hold').length+' test hold';
 h.textContent=ready?'Export':hold?'Hold':review?'Review':r?'Render':'Review';
 b.textContent=ready?'Create manual export':review?'Approve local review':renderer?'Render verified local preview':hold?'On hold':r?'Render not authorized':'Choose a clip';
 b.disabled=!(ready||review||renderer);
 b.onclick=()=>{if(ready)$('tt-export').click();else if(review)$('approve').click();else if(renderer)$('edit-transcode').click();};
 d.textContent=protectedHold?'Ready to export (format only): 8 s, H.264 1080×1920, AAC 48 kHz (127,605 bps measured; 128 kbps target). Private test hold: export remains locked.':hold?'Protected or unresolved media; no export or render.':ready?'Verified MP4. A local package never posts.':review?'Approve the exact existing render for local review.':renderer?'The existing controller will recheck the sealed render plan.':r?'Review the local source. No authorized new render or export is available for this clip.':'Select a clip.';
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
function note(t){$('message').textContent=t;}
function tone(row){return /hold|FAILED|HELD/.test(row.state)?'hold':row.state==='pending_review'||row.review==='PENDING'?'warn':'ready';}
function shown(row,mode=filter){
 if(mode==='pending')return row.state==='pending_review'||(row.state==='READY_FOR_REVIEW'&&row.review==='PENDING');
 if(mode==='ready')return row.state==='READY_FOR_REVIEW';
 if(mode==='workflow-render')return row.state==='pending_review'&&!row.preview_only;
 if(mode==='workflow-export')return row.manual_export_ready===true;
 if(mode==='no-speech')return row.caption_status==='no_speech';
 if(mode==='hold')return ['test_hold','duplicate_hold','HELD','FAILED'].includes(row.state);
 return true;
}
function time(s){if(!Number.isFinite(s))return '--:--.---';const ms=Math.max(0,Math.floor(s*1000));return String(Math.floor(ms/60000)).padStart(2,'0')+':'+String(Math.floor(ms/1000)%60).padStart(2,'0')+'.'+String(ms%1000).padStart(3,'0');}
function updateTime(){seek.value=String(video.currentTime||0);$('time').textContent=time(video.currentTime)+' / '+time(video.duration);}
function updateGuides(){
 $('grid').hidden=!$('grid-toggle').checked;$('zones').hidden=!$('safe-toggle').checked;
 const [ratios,name,right,bottom]=guides[$('platform-zone').value];$('zones').style.setProperty('--zone',ratios);$('zones').style.setProperty('--right',right);$('zones').style.setProperty('--bottom',bottom);$('zone-label').textContent=name;
}
function showView(mode,push=false){
 if(!['studio','gallery','ledger','system','profiles','sources'].includes(mode))mode='studio';
 activeView=mode;
 for(const id of ['studio','gallery','ledger','system','profiles','sources'])$(id).hidden=id!==mode;
 document.querySelectorAll('.view-nav [data-view]').forEach(a=>{if(a.dataset.view===mode)a.setAttribute('aria-current','page');else a.removeAttribute('aria-current');});
 document.querySelectorAll('.utility-strip [data-shell-link]').forEach(a=>{if(a.dataset.shellLink===mode)a.setAttribute('aria-current','page');else a.removeAttribute('aria-current');});
 if(push&&new URLSearchParams(location.search).get('view')!==mode)history.pushState(null,'','/workspace?view='+mode);
 if(mode!=='studio')video.pause();
 if(mode==='profiles')renderShellProfile();
 if(mode==='sources'){const frame=$('sources').querySelector('iframe');if(!frame.hasAttribute('src'))frame.src=frame.dataset.embed;}
 if(mode==='gallery'){renderGallery();inspectMetadata();if(indexReady)reload();}
 if(mode==='ledger'){renderLedger();if(ledgerReady)loadLedger();}
 if(mode==='system'){renderSystem();updatePhase4();cacheUsage();}
}
function renderGallery(){
 if(!indexReady)return; const needle=$('gallery-search').value.trim().toLocaleLowerCase();
 const visible=(activeProfile()?items:[]).filter(r=>shown(r,galleryFilter)&&(!needle||r.name.toLocaleLowerCase().includes(needle)));
 $('gallery-count').textContent=visible.length+' shown · '+items.filter(r=>r.state==='pending_review').length+' pending review · '+items.filter(r=>r.state==='test_hold').length+' private test hold';
 $('gallery-items').replaceChildren(...visible.map(r=>{
  const b=node('button',undefined,'gallery-card');b.type='button';b.dataset.clip=r.clip_id;
  if(r.poster_available){const i=node('img');i.alt='';i.src=endpoint(r,'poster');i.loading='lazy';i.onerror=()=>i.replaceWith(node('span','POSTER HELD','gallery-placeholder'));b.append(i);}
  else b.append(node('span',r.state==='test_hold'?'PRIVATE TEST HOLD':'NO POSTER','gallery-placeholder'));
  b.append(node('strong',r.name),node('small',labels[r.state]||r.state,tone(r)));
  const m=node('small',(dimensions.get(r.clip_id)||'Resolution unmeasured')+' · '+(durations.has(r.clip_id)?time(durations.get(r.clip_id)):'--:--'),'dimensions');
  if(r.import_metadata?.description){const summary=node('small',r.import_metadata.description,'import-description');summary.title=r.import_metadata.description;b.append(summary);} b.append(m);b.onclick=()=>{showView('studio',true);select(r);$('viewfinder').focus();};
  if(r.from_sources){
   const wrap=node('div',undefined,'gallery-card-wrap');wrap.append(b);
   const rm=node('button','Remove from Film Roll','gallery-remove-btn');rm.type='button';rm.title='Remove from Film Roll Gallery (keeps file on disk)';
   rm.onclick=async()=>{rm.disabled=true;try{await fetch('/api/workspace/film-roll/'+encodeURIComponent(r.clip_id),{method:'DELETE',credentials:'same-origin',redirect:'error'});if(selected?.clip_id===r.clip_id){selected=null;stopVideo();}await reload();}catch{rm.disabled=false;}};
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
function renderLedger(){
 if(!ledgerReady)return;
 const word=$('ledger-search').value.trim().toLocaleLowerCase();
 const records=(activeProfile()?ledgerRows:[]).filter(r=>!word||Object.values(r).join(' ').toLocaleLowerCase().includes(word));
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
  $('sm-live-source').textContent='Source · '+(d.admission_enabled?'Local Media mounted, pending review only':(d.source_state||'unverified'));
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
  if(id==='tiktok'){el.append(node('strong','TikTok'),node('span','MANUAL EXPORT'),node('div','No TikTok account connection, Inbox, or automatic posting. Export is available only for individually verified and approved media.','muted'));return el;}
  const named=id==='youtube'?'YouTube @Demo_Channel':id==='instagram'?'Instagram @demo_channel':'TikTok';
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
 $('system-paths').replaceChildren(...[['Source · read only',activeProfile()?systemData?.source_path:null],['Backup · separate trust',activeProfile()?systemData?.backup_path:null]].map(([k,v])=>{
  const el=node('div',undefined,'kv');el.append(node('strong',k),node('span',v||'Unavailable'));return el;
 }));
}
function renderShellProfile(){
 const profile=channelProfiles.find(p=>p.configured&&p.profile_id===scope);
 $('sm-active-profile').textContent=profile?profile.channel_alias+' · active local profile':'No verified active channel';
 $('sm-active-health').textContent=profile?['youtube','instagram','tiktok'].map(id=>providerNames[id]+' '+(id==='tiktok'?'Manual Export':providers.find(p=>p.provider===id)?.health||'not verified')).join(' · '):'Profile setup not verified.';
}
function renderProviders(){
 renderShellProfile();
 $('credentials').replaceChildren(...['youtube','instagram','tiktok'].map(id=>{
  const p=(activeProfile()?providers:[]).find(x=>x.provider===id), health=p?.health||'unknown';
  const badge=node('a',providerNames[id].toUpperCase()+(id==='youtube'?' @DEMO_CHANNEL':id==='instagram'?' @DEMO_CHANNEL':'')+' · '+(id==='tiktok'?'MANUAL EXPORT':health.toUpperCase()),health==='ready'?'ready':health==='held'||health==='expired'?'hold':'');
  const seconds=Number.isSafeInteger(p?.access_expires_at_unix)&&Number.isSafeInteger(p?.checked_at_unix)?p.access_expires_at_unix-p.checked_at_unix:null;
  badge.title='Last verified refresh attempt: '+healthTime(p?.last_refresh_at_unix)+' | Access expiry: '+healthTime(p?.access_expires_at_unix)+(seconds>0?' ('+Math.ceil(seconds/60)+' min left)':'')+' | Data access expiry: '+healthTime(p?.data_access_expires_at_unix);
  badge.href='/workspace?view=system';badge.onclick=e=>{e.preventDefault();showView('system',true);};if(seconds>0&&seconds<=14*86400)badge.classList.add('warn');return badge;
 }));
 $('platforms').replaceChildren(...['tiktok','instagram','youtube'].map(id=>{
  const l=node('label'),i=node('input');i.type='checkbox';i.checked=inspected.get(id);i.onchange=()=>{inspected.set(id,i.checked);if(i.checked){$('platform-zone').value={instagram:'reels',youtube:'shorts',tiktok:'tiktok'}[id];updateGuides();}};
  const p=(activeProfile()?providers:[]).find(x=>x.provider===id),health=p?.health||'unknown';
  l.append(i,node('span',providerNames[id]),node('small',id==='tiktok'?'MANUAL EXPORT':health==='ready'?'AUTH READY · POST HELD':health.toUpperCase()+' · POST HELD'));return l;
 }));
 renderSystem();
}
function renderRoll(restore=false){
 const visible=(activeProfile()?items:[]).filter(r=>shown(r));
 const pending=activeProfile()?items.filter(j=>j.state==='pending_review').length:0,hold=activeProfile()?items.filter(j=>j.state==='test_hold').length:0;
 $('roll-count').textContent=visible.length+' shown · '+pending+' indexed for review · '+hold+' private test hold';
 const active=restore?document.activeElement?.dataset.clip:null;
 roll.replaceChildren(...visible.map((row,i)=>{
  const b=node('button',undefined,'clip');b.type='button';b.dataset.clip=row.clip_id;b.setAttribute('aria-pressed',String(selected?.clip_id===row.clip_id));
  let thumb;
  if(row.poster_available){thumb=node('img',undefined,'thumb');thumb.alt='';thumb.loading='lazy';thumb.src=endpoint(row,'poster');thumb.onerror=()=>thumb.replaceWith(node('span',String(i+1).padStart(2,'0'),'thumb'));}
  else thumb=node('span',String(i+1).padStart(2,'0'),'thumb');
  const words=node('span');words.append(node('strong',row.name),node('small',(labels[row.state]||row.state)+(row.caption_status==='no_speech'?' · No speech':'')+(durations.has(row.clip_id)?' · '+time(durations.get(row.clip_id)):' · --:--'),tone(row)));b.append(thumb,words);
  const eligible=phase5Visible().some(r=>r.clip_id===row.clip_id);const decision=p5Labels.get(p5Key(row));if(decision)words.append(node('small','LOCAL LABEL · '+decision));b.classList.toggle('p5-picked',phase5.batch.has(row.clip_id));b.onclick=e=>{if(e.shiftKey&&eligible){phase5.range(phase5Visible().map(r=>r.clip_id),row.clip_id);renderRoll(true);}else {if(eligible)phase5.setAnchor(row.clip_id);select(row);}};return b;
 }));
 phase5Tray();if(!visible.length)roll.append(node('p','No clips match this filter.','muted'));
 workFlow();if(active)Array.from(roll.querySelectorAll('.clip')).find(b=>b.dataset.clip===active)?.focus();
}
function endpoint(row,action){return '/api/clips/'+encodeURIComponent(row.clip_id)+'/'+(action==='stream'?'video':action);}
function mediaEmpty(label,detail){$('empty').hidden=false;$('empty').querySelector('span').textContent=label;$('empty').querySelector('small').textContent=detail;}
function stopVideo(){video.pause();video.removeAttribute('src');video.removeAttribute('poster');video.load();seek.disabled=true;play.disabled=true;$('frame').textContent='—';$('bitrate').textContent='—';updateTime();}
function saveDraft(){if(selected){drafts.set(selected.clip_id,{title:$('title').value,caption:$('caption').value,tags:$('tags').value});if(pdState)pdCache.set(pdKey(selected),pdState);}}
function select(row){workflowFlush();saveDraft();selected=row;derivedMode=false;$('p5-loop').checked=false;generation++;cancelFrameWatch();stopVideo();note('');renderRoll(true);
 $('stage-state').textContent=(labels[row.state]||row.state).toUpperCase();$('stage-state').className='flag '+tone(row);
 $('inspect-description').textContent=row.import_metadata?.description||'';$('inspect-description').hidden=!row.import_metadata?.description;
 $('inspect-state').textContent=row.name+(row.preview_only?' · Preview only':'');
 const d=drafts.get(row.clip_id)||{};$('title').value=d.title??row.name;$('caption').value=d.caption??'';$('tags').value=d.tags??'';
 resetEdit(row);loadPlatformDraft(row,generation);workflowApply(row);renderPreflight();$('digest').textContent=row.preview_sha256||'Not verified';$('digest').title=row.preview_sha256||'';
 $('codec').textContent=row.codec||'Not recorded';$('encoder').textContent=row.encoder||'Not recorded';$('receipt').textContent=row.kind==='original'?(row.preview_available?'Pinned preview':'No render receipt'):(row.preview_available?'Verified output':'Not verified');
 $('approve').disabled=!(row.kind==='rendered'&&row.state==='READY_FOR_REVIEW'&&row.review==='PENDING');
 $('drive').textContent=row.state==='BACKED_UP'?'BACKUP VERIFIED':row.kind==='original'?'SOURCE INDEXED':'BACKUP NOT VERIFIED';$('drive').className='flag '+(row.state==='BACKED_UP'?'ready':'warn');
 $('drive-detail').textContent=row.state==='BACKED_UP'?'Controller recorded a verified restored-media receipt.':'This is not a backup receipt.';
 if(row.preview_available){mediaEmpty('LOADING PREVIEW','READ-ONLY PLAYBACK');if(row.poster_available)video.poster=endpoint(row,'poster');video.src=endpoint(row,'stream');video.load();}
 else mediaEmpty(row.state==='test_hold'?'PRIVATE TEST HOLD':'PREVIEW UNAVAILABLE',row.preview_only?'PREVIEW ONLY · NOT FOR POSTING':'NO VERIFIED PLAYABLE FILE');
 phase5Panel();workFlow();
}
video.addEventListener('loadedmetadata',()=>{if(!selected)return;const d=video.duration,w=video.videoWidth,h=video.videoHeight;if(Number.isFinite(d)&&d>0&&durations.get(selected.clip_id)!==d){durations.set(selected.clip_id,d);dimensions.set(selected.clip_id,w+'×'+h);renderRoll(true);}$('frame').textContent=w&&h?w+' × '+h:'—';if(Number.isFinite(d)&&d>0){seek.max=String(d);seek.disabled=false;play.disabled=false;}$('bitrate').textContent=Number.isFinite(d)&&d>0&&selected.preview_bytes?(selected.preview_bytes*8/d/1000).toFixed(0)+' kb/s (avg)':'—';$('empty').hidden=true;updateTime();refreshDraftEditor();updateTrim();});
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
['grid-toggle','safe-toggle','platform-zone'].forEach(id=>$(id).onchange=updateGuides);
document.addEventListener('keydown',e=>{if(e.ctrlKey&&e.key==='Enter'&&!e.target.closest('input, textarea, select')&&!$('approve').disabled){e.preventDefault();$('approve').click();}});
$('approve').onclick=async()=>{const j=selected;if(!j||$('approve').disabled)return;$('approve').disabled=true;try{await get('/api/controller/jobs/'+encodeURIComponent(j.id)+'/review',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({revision:j.revision,action:'approve'})});note('Approved for local review. Nothing was posted.');await reload();}catch(e){note(e.message);await reload();}};
function reload(){
 if(indexLoad)return indexLoad;
 indexLoad=get('/api/workspace').then(d=>{
   items=[...d.jobs,...d.index];indexReady=true;$('sm-live-queue').textContent='Local review · '+d.index.filter(x=>x.state==='pending_review').length+' pending · '+d.index.filter(x=>x.state==='test_hold').length+' test holds';p5LoadLabels();
   const previous=workflowRead('last');if(!selected&&activeView==='studio'&&previous){const hit=items.find(x=>x.clip_id===previous.id&&x.preview_sha256===previous.sha&&x.preview_available);if(hit)select(hit);} const next=selected&&items.find(x=>x.clip_id===selected.clip_id);
   if(next){selected=next;$('approve').disabled=!(next.kind==='rendered'&&next.state==='READY_FOR_REVIEW'&&next.review==='PENDING');}
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
setInterval(()=>{if(!document.hidden)updatePhase4();},15000);
$('sm-volume').oninput=()=>{video.volume=Number($('sm-volume').value)/100;};
video.volume=Number($('sm-volume').value)/100;
document.querySelectorAll('[data-shell-link]').forEach(a=>a.onclick=e=>{
 if(e.ctrlKey||e.metaKey||e.shiftKey||e.altKey||e.button!==0)return;
 e.preventDefault();const target=a.dataset.shellLink;showView(target,true);
 const heading=document.querySelector('#'+target+' h1');if(heading){heading.tabIndex=-1;heading.focus();}
});
$('profiles').querySelector('details').addEventListener('toggle',e=>{if(e.target.open){const f=e.target.querySelector('iframe');if(!f.hasAttribute('src'))f.src=f.dataset.embed;}});
updatePhase4();
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
const params=new URLSearchParams(location.search);
renderProviders();updateGuides();showView(params.get('view'));
$('ledger-notice').textContent='Loading verified local receipts...';
Promise.all([reload(),loadLedger(),loadNvenc()]).then(()=>{feedEnabled=true;syncFeed();if(params.get('verify-workspace')==='1')verifyGalleryThenStudio();else if(verifyRequested)verifyVisibleFilmRoll();});

const p5Labels=new Map();
const p5Key=row=>scope+':'+row.clip_id+':'+(row.media_sha256||row.preview_sha256);
const p5Actions=new Set(['Approve','Transcode','Hold','Archive']);
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
function phase5Visible(){return (scope==='demo-channel'?items:[]).filter(r=>shown(r)).filter(r=>['original','intake_original'].includes(r.kind)&&r.state==='pending_review'&&r.preview_available);}
function phase5Facts(){
 if(!selected)return null;
 if(derivedMode&&nvenc?.receipt?.source_sha256===selected.preview_sha256&&nvenc?.receipt?.sha256==='3c45bfc8aadcaaae1bef5c5beda8525899e8113254efde014019b5cde3ea7e89')
   return {streams:[{codec_type:'video',avg_frame_rate:'30/1',r_frame_rate:'30/1'}]};
 const facts=mediaFacts[selected.preview_sha256];return Number(facts?.format?.size)===selected.preview_bytes?facts:null;
}
function phase5SourcePair(){
 return !!(selected?.kind==='original'&&selected.preview_available&&selected.clip_id===phase3Clip&&
  nvenc?.stage==='ready'&&nvenc.job_id&&nvenc.receipt?.source_sha256===selected.preview_sha256);
}

const p5Wave={key:null,peaks:null,error:null,controller:null,loading:false};
function p5WaveScope(){
 if(!selected?.preview_available)return null;
 if(derivedMode&&phase5SourcePair())return {key:nvenc.receipt.sha256,bytes:nvenc.receipt.bytes,url:'/api/workspace/local-preview/'+encodeURIComponent(nvenc.job_id)+'/video'};
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
}
function phase5Step(direction){
 const to=phase5.step(video.currentTime,video.duration,direction,phase5Facts());
 if(to===null)return false;
 video.pause();video.currentTime=to;return true;
}
function phase5Tray(){
 const avail=phase5Visible().filter(r=>phase5.batch.has(r.clip_id)).length;
 $('p5-batch-size').textContent=avail?avail+' selected · local annotations only':'Shift+Click or Shift+Enter to select a range';
 $('p5-batch').querySelectorAll('button').forEach(b=>b.disabled=p5Write||avail===0||avail>32);
}
function phase5Compare(show){
 const pane=$('p5-compare'), alt=$('p5-compare-video');
 if(!show||!phase5SourcePair()){
   pane.hidden=true;$('p5-split').setAttribute('aria-pressed','false');
   if(alt.hasAttribute('src')){alt.pause();alt.removeAttribute('src');alt.load();}return;
 }
 const src=derivedMode?endpoint(selected,'stream'):'/api/workspace/local-preview/'+encodeURIComponent(nvenc.job_id)+'/video';
 pane.hidden=false;$('p5-split').setAttribute('aria-pressed','true');
 $('p5-compare-label').textContent=derivedMode?'Source (muted)':'Render (muted)';
 if(alt.getAttribute('src')!==src){$('p5-compare-state').textContent='Loading verified comparison';alt.src=src;alt.load();}
 phase5CompareSync();
}
function phase5CompareSync(){
 const alt=$('p5-compare-video');
 if($('p5-compare').hidden||!Number.isFinite(alt.duration))return;
 const a=nvenc?.timeline?.in_ms||0,offset=(derivedMode?1:-1)*a/1000,target=video.currentTime+offset;
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
$('p5-compare-video').addEventListener('error',()=>{if(!$('p5-compare').hidden)$('p5-compare-state').textContent='Verified comparison could not be decoded';});
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
 const action=b.dataset.triage,which=scope;
 const annotations=phase5Visible().filter(r=>phase5.batch.has(r.clip_id)).map(r=>({clip_id:r.clip_id,media_sha256:r.preview_sha256,label:action}));
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
 ['Receipts & Backups',()=>showView('ledger',true)],['Connections & System',()=>showView('system',true)],
 ['Toggle Source / Render',()=>phase5Swap(derivedMode?'source':'render'),phase5SourcePair],
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
 if(e.key==='\\'&&phase5SourcePair()){e.preventDefault();phase5Swap(derivedMode?'source':'render');}
 if((e.key===','||e.key==='.')&&phase5Step(e.key===','?-1:1))e.preventDefault();
});
})();
