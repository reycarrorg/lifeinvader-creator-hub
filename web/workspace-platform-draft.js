'use strict';
(() => {
const keys=['youtube','instagram','tiktok'],preset=['#GTA5','#GTAVI','#LosSantos','#GTARP'];
const byteCount=s=>new TextEncoder().encode(s).length;
const charCount=s=>Array.from(s).length;
function tags(raw){
 const seen=new Set();
 return (raw.match(/#[\p{L}\p{N}_]+/gu)||[]).filter(v=>{const k=v.toLocaleLowerCase('en-US');if(seen.has(k))return false;seen.add(k);return true;});
}
function hashtagCount(s){return (s.match(/#[\p{L}\p{N}_]+/gu)||[]).length;}
function mergeTags(raw,next){const parts=tags(raw);return tags([...parts,...next].join(' ')).join(' ');}
function appendMissingTags(text, list) {
 const embedded = new Set(tags(text).map(tag => tag.toLowerCase()));
 const missing = list.filter(tag => !embedded.has(tag.toLowerCase())).join(' ');
 return [text, missing].filter(Boolean).join('\n\n');
}
function tiktokPostText(core, overrides = {}) {
 const title = (core.title ?? '').trim();
 const caption = (overrides.tiktok ?? core.caption ?? '').trim();
 const firstLine = caption.split(/\r\n?|\n/, 1)[0].trim();
 const hasTitle = caption === title || firstLine === title;
 const text = hasTitle ? caption : [title, caption].filter(Boolean).join('\n\n');
 return appendMissingTags(text, tags(core.tags ?? ''));
}
function materialize(core,overrides,options){
 const rows={};
 for(const key of keys){
  const cap=overrides[key]??core.caption;
  let list=tags(core.tags);
  if(key==='youtube'&&options.shorts&&!list.some(x=>x.toLowerCase()==='#shorts'))list.push('#Shorts');
  rows[key]=appendMissingTags(cap,list);
 }
 return rows;
}
function mapCover(frameSeconds, trimStart, trimEnd, duration){
 if(![frameSeconds,duration].every(Number.isFinite)||frameSeconds<0||frameSeconds>=duration||duration<=0)return null;
 const frame=Math.floor(frameSeconds*1000);
 const start=trimStart??0,end=trimEnd??Math.ceil(duration*1000);
 if(!Number.isSafeInteger(start)||!Number.isSafeInteger(end)||start<0||start>=end||end>Math.ceil(duration*1000)||frame<start||frame>=end)return null;
 return {frame_ms:frame, thumb_offset_ms:frame-start, trim_start_ms:start, trim_end_ms:end};
}
function youtubeTagCount(raw){
 const items=raw.split(',').map(x=>x.trim()).filter(Boolean);
 return items.reduce((sum,tag)=>sum+charCount(tag)+(tag.includes(' ')?2:0),Math.max(0,items.length-1));
}
function youtubeShorts(duration,width,height){
 if(![duration,width,height].every(Number.isFinite)||duration<=0||width<=0||height<=0)return 'Classification not verified';
 if(duration>180)return 'Over 180 s: not a Shorts candidate';
 if(height<width)return 'Horizontal: not a Shorts candidate';
 if(duration<1)return 'Under the local 1-second preflight floor';
 return 'Shorts candidate: square or vertical, up to 180 s';
}
const api={youtubeTagCount,youtubeShorts,keys,preset,byteCount,charCount,tags,hashtagCount,mergeTags,materialize,tiktokPostText,mapCover};
if(typeof module!=='undefined'&&module.exports){module.exports=api;}else{window.snapmaticDraft=api;}
})();
