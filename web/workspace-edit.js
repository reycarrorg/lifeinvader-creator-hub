'use strict';
(() => {
const nativeFrames=['center_crop','blurred_fit','dual_zone'];
const stageLabels={queued:'Queued',processing_evidence_pending:'Processing · evidence pending',transcribing:'Transcribing',rendering:'Rendering',verifying_sha256:'Verifying SHA-256',ready:'Ready for review',hold:'Held',unverified:'Not verified'};
function trim(start,end,duration){
 return [start,end,duration].every(Number.isSafeInteger)&&start>=0&&start<end&&end<=duration?
   {in_ms:start,out_ms:end}:null;
}
function rectangle(p){
 if(!Array.isArray(p)||p.length!==4||!p.every(Number.isSafeInteger))return null;
 const [x,y,w,h]=p;
 return x>=0&&y>=0&&w>=1&&h>=1&&x+w<=100&&y+h<=100?p.slice():null;
}
function framing(mode,facecam,gameplay){
 if(!nativeFrames.includes(mode))return null;
 if(mode!=='dual_zone')return {mode};
 const f=rectangle(facecam),g=rectangle(gameplay);
 if(!f||!g)return null;
 if(f[0]<g[0]+g[2]&&g[0]<f[0]+f[2]&&f[1]<g[1]+g[3]&&g[1]<f[1]+f[3])return null;
 return {mode,facecam:f,gameplay:g};
}
function grounded(segments,duration){
 if(!Array.isArray(segments)||segments.length>2000||!Number.isSafeInteger(duration))return null;
 let last=0;const ids=new Set();
 for(const s of segments){
  if(typeof s?.id!=='string'||!/^[A-Za-z0-9_-]{1,64}$/.test(s.id)||ids.has(s.id)||
     !trim(s.start_ms,s.end_ms,duration)||s.start_ms<last||typeof s.text!=='string'||s.text.length>1000||
     s.text.includes(String.fromCharCode(0)))return null;
  ids.add(s.id);last=s.end_ms;
 }
 return segments.map(s=>({id:s.id,start_ms:s.start_ms,end_ms:s.end_ms,text:s.text}));
}
const api={nativeFrames,stageLabels,trim,rectangle,framing,grounded};
if(typeof module!=='undefined'&&module.exports){module.exports=api;}else{window.snapmaticEdit=api;}
})();