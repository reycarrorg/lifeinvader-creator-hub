/* All functions are local-only. Playback reads the selected verified clip routes. */
const phase5=(()=>{
 const batch=new Set(), decisions=new Map();
 let anchor=null;
 const approvedActions=new Set(['Approve','Transcode','Hold','Archive']);
 function range(ids,id){
   if(!ids.includes(id))return;
   const a=ids.indexOf(anchor),b=ids.indexOf(id);
   if(a<0)batch.has(id)?batch.delete(id):batch.add(id);
   else for(const item of ids.slice(Math.min(a,b),Math.max(a,b)+1))batch.add(item);
   anchor=id;
 }
 function review(action,ids){
   if(!approvedActions.has(action))return 0;
   let n=0;for(const id of ids)if(batch.has(id)){decisions.set(id,action);n++;}
   batch.clear();anchor=null;return n;
 }
 function step(current,end,direction,facts){
   if(!Number.isFinite(current)||!Number.isFinite(end)||end<=0||![-1,1].includes(direction))return null;
   const s=facts?.streams?.find(x=>x.codec_type==='video');
   if(!s?.avg_frame_rate||s.avg_frame_rate!==s.r_frame_rate)return null;
   const parts=/^(\d+)\/([1-9]\d*)$/.exec(s.avg_frame_rate);
   if(!parts)return null;
   const fps=Number(parts[1])/Number(parts[2]);if(!(fps>=1&&fps<=240))return null;
   const index=direction>0?Math.floor(current*fps+.00001)+1:Math.ceil(current*fps-.00001)-1;
   return Math.max(0,Math.min(index,Math.ceil(end*fps)-1)/fps);
 }
 function loop(a,b,d,t){
   if(![a,b,d,t].every(Number.isFinite)||a<0||b<=a||b>d+.001)return null;
   return t<a||t>=b?a:null;
 }
 function metadata(f,d){
   if(!Number.isFinite(d)||d<=0)return {peaks:null,speech:null};
   const a=f?.waveform_peaks,p=f?.speech_segments;
   const peaks=Array.isArray(a)&&a.length>=8&&a.length<=256&&a.every(x=>Number.isFinite(x)&&x>=0&&x<=1)?a:null;
   const speech=Array.isArray(p)&&p.length<=256&&p.every((x,i)=>Number.isInteger(x.start_ms)&&Number.isInteger(x.end_ms)&&x.start_ms>=0&&x.end_ms>x.start_ms&&x.end_ms<=d*1000&&(!i||x.start_ms>=p[i-1].end_ms))?p:null;
   return {peaks,speech};
 }
 return {batch,decisions,range,review,step,loop,metadata,setAnchor:id=>{anchor=id;},clear:()=>{batch.clear();anchor=null;}};
})();
if(typeof module!=='undefined')module.exports=phase5;