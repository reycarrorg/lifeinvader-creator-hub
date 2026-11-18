importScripts('collector.js');
const base='http://127.0.0.1:3847/api/bridge';
let working=false;
async function wait(ms){return new Promise(resolve=>setTimeout(resolve,ms));}
async function run(){
 if(working)return;working=true;let tabId=null;
 try{
  const {token}=await chrome.storage.local.get('token');if(!token)return;
  const headers={'x-lifeinvader-token':token};const resp=await fetch(base+'/poll',{headers});if(!resp.ok)return;
  const {job}=await resp.json();if(!job)return;
  const parsed=new URL(job.url);if(!['https:'].includes(parsed.protocol)||!['www.instagram.com','instagram.com','www.tiktok.com','tiktok.com'].includes(parsed.hostname))return;
  const tab=await chrome.tabs.create({url:job.url,active:false});tabId=tab.id;
  let result={error:'timeout'};
  for(let i=0;i<20;i++){
   await wait(1500);
   try{const info=await chrome.tabs.get(tabId);if(info.status!=='complete')continue;const [out]=await chrome.scripting.executeScript({target:{tabId},func:readProfile,args:[job.url,job.platform]});result=out?.result||{error:'unavailable'};if(!result.error||result.error==='identity'||result.error==='login')break;}catch{}
  }
  await fetch(base+'/result',{method:'POST',headers:{...headers,'content-type':'application/json'},body:JSON.stringify({profile_id:job.profile_id,job_id:job.id,...result})});
 }catch{}finally{if(tabId!==null)try{await chrome.tabs.remove(tabId);}catch{}working=false;}
}
chrome.runtime.onInstalled.addListener(()=>{chrome.alarms.create('refresh',{periodInMinutes:1});run();});
chrome.runtime.onStartup.addListener(()=>{chrome.alarms.create('refresh',{periodInMinutes:1});run();});
chrome.alarms.onAlarm.addListener(a=>{if(a.name==='refresh')run();});
chrome.runtime.onMessage.addListener(msg=>{if(msg?.type==='poll')run();});
