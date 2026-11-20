"""Phase 3 preview worker. Caller supplies leased source, independently pinned request and local tools."""
import hashlib
import json
import math
import os
from pathlib import Path
from fractions import Fraction
from captions import make_ass, faststart
from export_profile import validate_stream_layout
from local_producer import bounded, pinned_tool, gpu_idle, exclusive_json, read_lease, measure_audio, audio_output, linear_prepare, encode_checked
from prepare import digest
from worker_cancel import checkpoint

SCHEMA='snapmatic-preview-transcode-v1'
CLIP='d471e12c49d298ef7276d8b147160636f16da76ad3b71fe43727516f3f292707e'
SOURCE='c157e6bf0f844b99932be9d12525e0f8137b88a9072eac01cc08bfe24c4dcdc6'
def canonical(v):
    return json.dumps(v,sort_keys=True,separators=(',',':'),allow_nan=False).encode()

def editor(r, width, height, duration, transcript):
    fields={'schema','clip_id','source_sha256','source_revision','in_ms','out_ms','framing','transcript_sha256','edits'}
    if set(r)!=fields or r['schema']!=SCHEMA or r['clip_id']!=CLIP or r['source_sha256']!=SOURCE or not isinstance(r['source_revision'],str) or not 0<len(r['source_revision'])<=256:raise ValueError('source binding')
    a,b=r['in_ms'],r['out_ms']
    if type(a)!=int or type(b)!=int or not 0<=a<b<=duration or b-a>60000:raise ValueError('trim')
    fr=r['framing'];mode=fr['mode']
    if mode=='dual_zone':
        if set(fr)!={'mode','face','game'}:raise ValueError('zones')
        parts=[]
        for key,hh in [('face',640),('game',1280)]:
            rect=fr[key]
            if type(rect)!=list or len(rect)!=4 or any(type(n)!=int or n<0 or n%2 for n in rect):raise ValueError('crop')
            x,y,w,h=rect
            if min(w,h)<2 or x+w>width or y+h>height:raise ValueError('crop bounds')
            parts.append(f'[{key}]crop={w}:{h}:{x}:{y},scale=1080:{hh}:force_original_aspect_ratio=increase,crop=1080:{hh}[{key}out]')
        graph='[0:v]split[face][game];'+';'.join(parts)+';[faceout][gameout]vstack'
    elif fr=={'mode':'center'}:graph='[0:v]scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920'
    elif fr=={'mode':'blur'}:graph='[0:v]split[bg][fg];[bg]scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920,boxblur=20[back];[fg]scale=1080:1920:force_original_aspect_ratio=decrease[front];[back][front]overlay=(W-w)/2:(H-h)/2'
    else:raise ValueError('framing')
    if not isinstance(r['edits'],dict):raise ValueError('edits')
    words=[];seen=set();last=0
    if transcript is None:
        if r['transcript_sha256'] is not None or r['edits']:raise ValueError('missing transcript')
    else:
        if hashlib.sha256(canonical(transcript)).hexdigest()!=r['transcript_sha256'] or transcript['source_sha256']!=SOURCE or transcript['source_revision']!=r['source_revision']:raise ValueError('transcript binding')
        for seg in transcript['segments']:
            i=seg['id'];start,end=seg['start_ms'],seg['end_ms']
            if not isinstance(i,str) or i in seen or type(start)!=int or type(end)!=int or not last<=start<end<=duration:raise ValueError('segment ordering')
            last=end;seen.add(i)
            text=r['edits'].get(i,seg['text'])
            if not isinstance(text,str) or not text.strip() or len(text)>150 or not seg['text'].strip():raise ValueError('segment text')
            if start<b and end>a:words.append(((max(start,a)-a)/1000,(min(end,b)-a)/1000,text))
        if set(r['edits'])-seen:raise ValueError('unknown segment')
    ass=make_ass(words,duration=(b-a)/1000)[0] if words else None
    return graph+',setsar=1'+(',ass=selected.ass' if ass else '')+'[v]',ass

def validate_preview_audio(data,command):
    # Isolated preview only. FFprobe reports measured elementary-stream mean.
    for flag,value in (('-c:a','aac'),('-b:a','128k'),('-ar','48000'),('-ac','2')):
        if command.count(flag)!=1 or command[command.index(flag)+1:command.index(flag)+2]!=[value]:
            raise ValueError('preview AAC encoder contract')
    audio=validate_stream_layout(data)
    measured=int(audio.get('bit_rate',0))
    if audio.get('profile')!='LC' or not 0<measured<=130560:
        raise ValueError('preview AAC profile or measured bitrate')
    return measured

def copy_eligible(v,request,ass,audio_duration_ms):
    fps=Fraction(v['avg_frame_rate'])
    return (v.get('codec_name')=='h264' and v.get('profile')=='High' and v.get('pix_fmt')=='yuv420p'
            and (v.get('width'),v.get('height'))==(1080,1920) and v.get('sample_aspect_ratio')=='1:1'
            and not v.get('side_data_list') and not v.get('tags',{}).get('rotate')
            and Fraction(v['r_frame_rate'])==fps and 23<=fps<=60 and 0<int(v['bit_rate'])<=25000000
            and request['framing']=={'mode':'center'} and ass is None and request['in_ms']==0
            and Fraction(v['duration'])*1000==request['out_ms']==audio_duration_ms
            and Fraction(v.get('start_time','1'))==0)

def video_digest(ff,path,root):
    raw=bounded([ff,'-v','error','-nostdin','-protocol_whitelist','file,pipe','-i',str(path),'-map','0:v:0',
                 '-c:v','copy','-f','streamhash','-hash','sha256','-'],root,45).decode().strip()
    import re
    m=re.fullmatch(r'0,v,SHA256=([0-9a-f]{64})',raw)
    if not m:raise ValueError('video payload digest unavailable')
    return m.group(1)

def run(request, expected_request_sha256, source, root, config, transcript=None):
    # No CLI, discovery, authority issuance, or fallback to the publishing queue.
    key=hashlib.sha256(canonical(request)).hexdigest()
    if key!=expected_request_sha256 or not root.is_absolute() or not root.is_dir() or root.resolve()!=root:raise ValueError('unreviewed preview')
    source=Path(source)
    if source.name!=CLIP+'.mp4' or source.resolve()!=source or source.parent.name!='snapmatic-preview-staging-drive-v1':raise ValueError('staged source')
    work=root/key;proof=work/'preview-receipt.json';out=work/'preview.mp4'
    with read_lease(source):
        if digest(source)!=SOURCE:raise ValueError('source changed')
        if work.exists():
            if work.resolve()!=work or not proof.is_file() or proof.resolve()!=proof or out.resolve()!=out:raise ValueError('incomplete preview HOLD')
            saved=json.loads(proof.read_bytes())
            if saved['request_sha256']!=key or saved['source_sha256']!=SOURCE or saved['sha256']!=digest(out) or saved['bytes']!=out.stat().st_size:raise ValueError('preview mismatch HOLD')
            return saved
        ff=pinned_tool(config,'ffmpeg');fp=pinned_tool(config,'ffprobe')
        def probe(path):return json.loads(bounded([fp,'-v','error','-protocol_whitelist','file,pipe','-show_streams','-show_format','-of','json',str(path)],root,15))
        before=probe(source);v=next(s for s in before['streams'] if s['codec_type']=='video')
        if v['codec_name']!='av1':raise ValueError('expected AV1')
        video_mode=config.get('video_mode','nvenc')
        if video_mode not in ('nvenc','copy_if_compliant'):raise ValueError('video mode')
        graph,ass=editor(request,v['width'],v['height'],int(float(v['duration'])*1000),transcript)
        work.mkdir();checkpoint()
        if ass:
            with (work/'selected.ass').open('x',encoding='utf8') as f:f.write(ass)
        a,b=request['in_ms'],request['out_ms']
        measurement=measure_audio(ff,source,work,a,b)
        linear_prepare(ff,source,work,a,b,measurement,config.get('audio_mode','dynamic'))
        aud=next(s for s in before['streams'] if s['codec_type']=='audio')
        copy=video_mode=='copy_if_compliant' and copy_eligible(v,request,ass,Fraction(aud['duration'])*1000)
        if not copy:gpu_idle(config,work,'preencode')
        fps=max(Fraction(23),min(Fraction(60),Fraction(v['avg_frame_rate'])))
        cmd=[ff,'-nostdin','-v','error','-protocol_whitelist','file,pipe','-ss',f'{a/1000:.3f}','-to',f'{b/1000:.3f}','-i',str(source),'-filter_complex',graph,'-map','[v]','-map','0:a:0','-r',str(fps),'-c:v','h264_nvenc','-gpu','0','-profile:v','high','-rc','vbr','-b:v','12M','-maxrate','25M','-bufsize','25M','-pix_fmt','yuv420p',*audio_output(measurement),'-movflags','+faststart','-n',str(out)]
        copied_hash=None
        if copy:
            copied_hash=video_digest(ff,source,work)
            cmd=[ff,'-nostdin','-v','error','-protocol_whitelist','file,pipe','-i',str(source),'-map','0:v:0','-map','0:a:0','-c:v','copy',*audio_output(measurement),'-movflags','+faststart','-n',str(out)]
        encode_checked(cmd,work,180,measurement)
        if copy and video_digest(ff,out,work)!=copied_hash:raise ValueError('copied video payload changed')
        data=probe(out);audio_bps=validate_preview_audio(data,cmd);v=next(s for s in data['streams'] if s['codec_type']=='video')
        if v.get('profile')!='High' or not 23<=Fraction(v['avg_frame_rate'])<=60 or not 0<int(v['bit_rate'])<=25000000 or not faststart(out) or abs(float(data['format']['duration'])-(b-a)/1000)>.15:raise ValueError('preview verification')
        # Measure the actual decoded derivative; FFmpeg metadata goes to a private local file.
        bounded([ff,'-nostdin','-v','error','-protocol_whitelist','file,pipe','-i',str(out),'-vn','-af','ebur128=metadata=1:peak=true,ametadata=print:file=levels.txt','-f','null','-'],work,45)
        readings={}
        for line in (work/'levels.txt').read_text().splitlines():
            k,sep,val=line.partition('=')
            if k in ('lavfi.r128.I','lavfi.r128.true_peak'):
                value=float(val)
                if not math.isfinite(value):raise ValueError('invalid audio measurement')
                readings[k]=value
        peak=readings.get('lavfi.r128.true_peak')
        telemetry=dict(integrated_lufs=readings.get('lavfi.r128.I') if peak and peak>0 else None,true_peak_dbtp=20*math.log10(peak) if peak and peak>0 else None,status='unavailable' if peak is None else 'silence' if peak==0 else 'measured')
        result=dict(schema='snapmatic-preview-receipt-v1',request_sha256=key,source_sha256=SOURCE,source_revision=request['source_revision'],sha256=digest(out),bytes=out.stat().st_size,video_mode='copy' if copy else 'nvenc',copied_video_sha256=copied_hash,audio_input=measurement,audio_target_bps=128000,audio_measured_bps=audio_bps,audio=telemetry,publication_enabled=False)
        exclusive_json(proof,result)
        return result
