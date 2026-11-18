/* Small, local-only thumbnail cache. Decode one video at a time and release it. */
(() => {
  const memory=new Map(), queue=[];
  let working=false, dbPromise;
  function database(){
    if(!('indexedDB' in window))return Promise.resolve(null);
    if(!dbPromise)dbPromise=new Promise(resolve=>{
      const request=indexedDB.open('lifeinvader-preview-cache',1);
      request.onupgradeneeded=()=>{const store=request.result.createObjectStore('frames',{keyPath:'key'});store.createIndex('saved','saved');};
      request.onsuccess=()=>resolve(request.result);request.onerror=()=>resolve(null);request.onblocked=()=>resolve(null);
    });
    return dbPromise;
  }
  async function get(key){if(memory.has(key))return memory.get(key);const db=await database();if(!db)return null;return new Promise(resolve=>{const req=db.transaction('frames').objectStore('frames').get(key);req.onsuccess=()=>resolve(req.result||null);req.onerror=()=>resolve(null);});}
  async function put(record){memory.set(record.key,record);if(memory.size>100)memory.delete(memory.keys().next().value);const db=await database();if(!db)return;try{const tx=db.transaction('frames','readwrite'),store=tx.objectStore('frames');store.put({...record,saved:Date.now()});const count=store.count();count.onsuccess=()=>{let remove=count.result-600;if(remove>0){const cursor=store.index('saved').openCursor();cursor.onsuccess=()=>{const c=cursor.result;if(c&&remove-->0){c.delete();c.continue();}};}};}catch{}}
  function extract(url){return new Promise((resolve,reject)=>{
    const video=document.createElement('video');let done=false;
    const timeout=setTimeout(()=>finish(new Error('Preview timed out')),18000);
    function finish(error,result){if(done)return;done=true;clearTimeout(timeout);video.pause();video.removeAttribute('src');video.load();error?reject(error):resolve(result);}
    function capture(){if(done||!video.videoWidth||!video.videoHeight||video.readyState<2)return;try{const w=video.videoWidth,h=video.videoHeight,scale=Math.min(1,480/Math.max(w,h));const canvas=document.createElement('canvas');canvas.width=Math.max(1,Math.round(w*scale));canvas.height=Math.max(1,Math.round(h*scale));canvas.getContext('2d').drawImage(video,0,0,canvas.width,canvas.height);canvas.toBlob(blob=>blob?finish(null,{blob,width:w,height:h}):finish(new Error('Preview unavailable')),'image/jpeg',.78);}catch(e){finish(e);}}
    video.muted=true;video.playsInline=true;video.preload='auto';
    video.onloadedmetadata=()=>{try{if(Number.isFinite(video.duration)&&video.duration>.7){video.currentTime=Math.min(.5,video.duration/4);}else capture();}catch{capture();}};
    video.onseeked=capture;video.onloadeddata=()=>{if(!video.seeking)capture();};video.onerror=()=>finish(new Error('Preview unavailable'));video.src=url;video.load();
  });}
  async function work(){if(working)return;working=true;while(queue.length){const job=queue.shift();if(!job.isUseful()){job.resolve(null);continue;}try{let result=await get(job.key);if(!result){const frame=await extract(job.url);result={key:job.key,...frame};await put(result);}job.resolve(result);}catch{job.resolve(null);}}working=false;}
  window.LVPreviews={load(key,url,isUseful){return new Promise(resolve=>{queue.push({key,url,isUseful,resolve});work();});}};
})();
