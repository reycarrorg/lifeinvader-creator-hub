'use strict';
if(window.parent!==window&&location.pathname==='/controller'&&new URLSearchParams(location.search).get('embedded')==='1')
 document.body.classList.add('sm-embedded');
(() => { const el=document.getElementById('sm-health');if(!el)return;
fetch('/api/controller/provider-auth',{cache:'no-store',credentials:'same-origin',redirect:'error'})
 .then(r=>{if(!r.ok)throw Error('health');return r.json();})
 .then(v=>{const names={youtube:'YouTube @Demo_Channel',instagram:'Instagram @demo_channel',tiktok:'TikTok'};el.textContent=v.providers.map(p=>(names[p.provider]||p.provider)+' · '+p.health).join('  |  ');})
 .catch(()=>{el.textContent='Provider health unavailable';});
})();
