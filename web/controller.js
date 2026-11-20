/* Controller UI. Preview GET is intentionally independent from every mutation. */
window.LVController = Object.freeze({attach(article,sourceId,file){
  if(file.kind!=='video')return;
  const a=document.createElement('a');a.className='button outline';a.textContent='Make a clip';
  const query=new URLSearchParams({source_id:String(sourceId),path:file.path});
  a.href='/controller?'+query;a.title='Open controller with this original video';
  article.querySelector('.source-file-info').lastElementChild?.append(a);
}});
(() => {
  if(!document.getElementById('controller-page'))return;
  const $=id=>document.getElementById(id);let snapshot,loading=false,mutating=false,previewRequest,objectUrl,previewTurn=0;
  const reasons={stale_revision:'This item changed. Refresh and review its current state.',controller_paused:'The controller is paused.',interrupted_processing:'Processing was interrupted. The worker must reconcile it before retry.',review_rejected:'Review rejected. Resume creates a fresh attempt requiring new review.',reconciliation_required:'This job needs reconciliation before it can safely resume.',source_unavailable:'The original source is unavailable or its identity changed.',solo_provenance_unverified:'The exact original Solo post has not been independently verified.',original_source_required:'Choose the original video rather than an edited copy.',already_queued:'This video is already in the controller for this platform.',preview_unavailable:'No verified preview is available yet.',preview_integrity_failed:'Preview verification failed. This item stays on hold.',worker_failed:'The worker reported a failure.',same_origin_required:'Open this controller from its local app address.',native_manual_unavailable:'Rendering is not configured for this app session.',reviewed_job_mismatch:'This clip does not match the reviewed selection.',native_attempt_already_consumed:'This reviewed render has already been admitted. Check its current result; do not retry.'};
  function explain(code){return reasons[code]||'The action could not be completed. Refresh to check the current state.';}
  function notice(message){$('notice').textContent=message;}
  async function api(path,body){
    const response=await fetch(path,{method:body===undefined?'GET':'POST',headers:body===undefined?{}:{'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body),cache:'no-store',credentials:'same-origin',redirect:'error'});
    const value=await response.json();if(!response.ok)throw Error(explain(value.reason));return value;
  }
  function node(tag,text,cls){const el=document.createElement(tag);if(text!==undefined)el.textContent=text;if(cls)el.className=cls;return el;}
  async function mutate(path,body){if(mutating)return;mutating=true;try{await api(path,body);notice('Saved.');}catch(e){notice(e.message);}finally{mutating=false;await refresh();}}
  function action(label,run,enabled=true){const b=node('button',label);b.type='button';b.disabled=!enabled;b.onclick=run;return b;}
  function closePreview(){previewTurn++;previewRequest?.abort();previewRequest=null;$('preview-video').pause();$('preview-video').removeAttribute('src');$('preview-video').load();if(objectUrl)URL.revokeObjectURL(objectUrl);objectUrl=null;}
  async function openPreview(job){
    closePreview();const turn=previewTurn;previewRequest=new AbortController();$('preview-status').textContent='Reading verified preview…';if(!$('preview-dialog').open)$('preview-dialog').showModal();
    try{
      const r=await fetch('/api/controller/jobs/'+encodeURIComponent(job.id)+'/preview',{method:'GET',cache:'no-store',signal:previewRequest.signal,redirect:'error'});
      if(!r.ok){const result=await r.json();throw Error(explain(result.reason));}
      if(r.headers.get('Content-Type')!=='video/mp4')throw Error('Unexpected preview format.');
      const reader=r.body.getReader();const chunks=[];let count=0;
      try{for(;;){const {done,value}=await reader.read();if(done)break;count+=value.length;if(count>16*1024*1024)throw Error('Preview exceeds its size limit.');chunks.push(value);}}catch(e){await reader.cancel();throw e;}finally{reader.releaseLock();}
      if(turn!==previewTurn)return;if(!count)throw Error('Empty preview.');
      objectUrl=URL.createObjectURL(new Blob(chunks,{type:'video/mp4'}));$('preview-video').src=objectUrl;$('preview-status').textContent='Review the clip before approving. Nothing has been submitted by opening this preview.';
    }catch(e){if(turn===previewTurn&&e.name!=='AbortError')$('preview-status').textContent=e.message;}
  }
  function render(){
    const s=snapshot.settings;$('run-status').textContent=(s.paused?'Paused':'Resumed')+' · Autopilot '+(s.autopilot?'ON':'OFF')+' · review always required';$('pause').disabled=s.paused;$('resume').disabled=!s.paused;$('autopilot').checked=s.autopilot;
    const ops=snapshot.quota.ops;$('ops-status').textContent=!ops.ok?'OPS accounting unknown. Posting remains held.':ops.status.paused?'OPS is paused. Local Resume cannot clear this hold.':'OPS status available; posting still requires its reservation checks.';
    $('quotas').replaceChildren(...snapshot.quota.local_receipts.platforms.map(q=>{const el=node('div');el.append(node('strong',q.platform.toUpperCase()),node('p',q.confirmed_posts+' / '+q.limit+' locally confirmed posts'),node('small',ops.ok?('OPS: '+ops.status.platform_used[q.platform]+' used · '+ops.status.platform_remaining[q.platform]+' remaining'):'OPS remaining: unknown'));return el;}));
    $('job-count').textContent='Showing '+snapshot.jobs.length+' of '+snapshot.total_jobs+' jobs.';
    $('jobs').replaceChildren(...snapshot.jobs.map(j=>{
      const el=node('article');el.append(node('span',j.state.replaceAll('_',' '),'badge'),node('h2',j.source_ref,'job-path'),node('p',j.platform.toUpperCase()+' · '+j.review.toLowerCase()+' review'));
      if(j.hold_reason)el.append(node('p',explain(j.hold_reason),'reason'));
      if(j.state==='QUEUED')el.append(node('p',s.paused?'Waiting while the controller is paused.':'Waiting for the worker connection.','muted'));
      if(j.state==='READY_FOR_REVIEW'&&j.review==='APPROVED')el.append(node('p','Approved for review. Posting still requires the publishing checks.','muted'));
      const buttons=node('div',undefined,'actions');const base='/api/controller/jobs/'+encodeURIComponent(j.id);
      const evidence=node('p','Worker evidence not checked.','muted');el.append(evidence);
      if(j.state==='QUEUED'&&snapshot.manual_native_enabled)buttons.append(action('Render reviewed clip',()=>mutate(base+'/render-reviewed',{revision:j.revision}),!s.paused));
      if(j.state==='QUEUED')buttons.append(action('Check clip preparation',async()=>{try{const v=await api(base+'/prepare',{revision:j.revision});evidence.textContent=v.state+' · Dry run only; no render started. Whisper installed: '+Boolean(v.faster_whisper_available)+'. GPU/model loading not yet verified.';}catch(e){evidence.textContent=e.message;}}));
      if(j.state==='QUEUED')buttons.append(action('Import reviewed completion',()=>mutate(base+'/import-completed',{revision:j.revision})));
      buttons.append(action('Check worker evidence',async()=>{try{const v=await api(base+'/status-details');evidence.textContent='Job: '+v.durable_state+'; worker: '+v.worker.state+'; '+v.worker.reason+'; preview integrity checked on open.';}catch(e){evidence.textContent='Worker evidence unavailable';}}));
      if(['READY_FOR_REVIEW','POSTED','BACKED_UP'].includes(j.state))buttons.append(action('Reopen preview',()=>openPreview(j)));
      if(j.state==='READY_FOR_REVIEW'&&j.review==='PENDING')for(const name of ['approve','reject'])buttons.append(action(name==='approve'?'Approve review':'Reject review',()=>mutate(base+'/review',{revision:j.revision,action:name})));
      if(['HELD','FAILED'].includes(j.state))buttons.append(action('Resume job',()=>mutate(base+'/review',{revision:j.revision,action:'resume'}),j.retry_safe));
      el.append(buttons);return el;
    }));
  }
  let uiTimer,uiClosing=false;
  async function uiPulse(){
    if(uiClosing||!snapshot?.isolated_ui_lifetime)return;
    try{await api('/api/controller/ui-lifetime',{action:'ready'});}
    catch(e){notice('Review UI connection lost. Rendering is held; close this window.');}
  }
  function enableUiLifetime(){
    if(!snapshot.isolated_ui_lifetime||uiTimer)return;
    const close=node('button','Close review app');close.type='button';
    close.onclick=async()=>{
      if(uiClosing)return;uiClosing=true;clearInterval(uiTimer);closePreview();
      try{await api('/api/controller/ui-lifetime',{action:'close'});notice('Review app is shutting down. You can close this window.');document.querySelectorAll('button,input,select').forEach(e=>e.disabled=true);}
      catch(e){notice('Shutdown could not be confirmed. Leave this window open and check the app status.');}
    };
    document.querySelector('.controls').append(close);
    uiPulse();uiTimer=setInterval(uiPulse,5000);
    window.addEventListener('pagehide',()=>clearInterval(uiTimer));
  }
  async function refresh(){if(loading)return;loading=true;try{snapshot=await api('/api/controller');render();enableUiLifetime();}catch(e){notice(e.message);}finally{loading=false;}}
  $('pause').onclick=()=>snapshot&&mutate('/api/controller/settings',{revision:snapshot.settings.revision,action:'pause'});
  $('resume').onclick=()=>snapshot&&mutate('/api/controller/settings',{revision:snapshot.settings.revision,action:'resume'});
  $('autopilot').onchange=()=>snapshot&&mutate('/api/controller/settings',{revision:snapshot.settings.revision,action:$('autopilot').checked?'autopilot_on':'autopilot_off'});
  $('refresh').onclick=refresh;$('close-preview').onclick=()=>$('preview-dialog').close();$('preview-dialog').addEventListener('close',closePreview);window.addEventListener('pagehide',closePreview);
  $('enqueue').onsubmit=e=>{e.preventDefault();mutate('/api/controller/jobs',{source_id:Number($('source').value),path:$('source-path').value,platform:$('platform').value});};
  const prefill=new URLSearchParams(location.search);$('source-path').value=prefill.get('path')||'';
  Promise.all([api('/api/items'),api('/api/controller/authorized-sources')]).then(([data,catalog])=>{for(const p of data.profiles.filter(p=>p.role==='source'&&p.url!=='https://lifeinvader.local/solo-posts'&&(!catalog.configured||catalog.sources.some(s=>s.source_id===String(p.id))))){const o=node('option',p.name);o.value=p.id;$('source').append(o);}$('source').value=prefill.get('source_id')||'';}).catch(e=>notice(e.message));
  api('/api/controller/provider-auth').then(v=>{$('connection').textContent=v.providers.map(p=>p.provider+': '+p.health).join(' | ')+'. Connect unavailable; publishing off.';}).catch(()=>{$('connection').textContent='Provider auth unknown. Connect and publishing unavailable.';});
  refresh();
})();
