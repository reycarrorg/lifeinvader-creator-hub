"""Offline staged producer. Import is inert; no default config or auto-resume.

Only an explicitly admitted caller may call run_local(preparation, workspace,
producer_config). The dry-run HTTP route never calls it.
"""
import hashlib, importlib.metadata, importlib.util, json, math, os, re
from pathlib import Path
import subprocess, sys, time
from contextlib import contextmanager, ExitStack
from finalize import phase_event
from prepare import digest
from captions import make_ass, faststart
from export_profile import validate
import gpu_guard
import nvml_encoder_probe
import worker_cancel
_runtime_identity=None
_runtime_files=None

FIELDS={'schema','enabled','source_sha256','cut_key','model','model_files',
        'device','compute_type','ffmpeg','ffmpeg_sha256','ffprobe','ffprobe_sha256',
        'nvidia_smi','nvidia_smi_sha256','gpu_uuid','asr_timeout','render_timeout'}

@contextmanager
def read_lease(path):
    """Windows read-only handle excluding writes/rename for the whole attempt."""
    if os.name!='nt':raise RuntimeError('Windows file lease required')
    import ctypes, msvcrt, stat
    from ctypes import wintypes
    kernel=ctypes.WinDLL('kernel32',use_last_error=True)
    create=kernel.CreateFileW
    create.argtypes=[wintypes.LPCWSTR,wintypes.DWORD,wintypes.DWORD,ctypes.c_void_p,wintypes.DWORD,wintypes.DWORD,wintypes.HANDLE]
    create.restype=wintypes.HANDLE
    handle=create(str(Path(path)),0x80000000,1,None,3,0x00200000,None)
    if handle==ctypes.c_void_p(-1).value:raise OSError('read lease unavailable')
    try:fd=msvcrt.open_osfhandle(handle,os.O_RDONLY|os.O_BINARY)
    except BaseException:
        close=kernel.CloseHandle;close.argtypes=[wintypes.HANDLE];close.restype=wintypes.BOOL;close(handle);raise
    try:
        metadata=os.fstat(fd)
        if not stat.S_ISREG(metadata.st_mode) or getattr(metadata,'st_file_attributes',0)&0x400:raise ValueError('nonregular/reparse input')
        yield
    finally:os.close(fd)

def pinned_tool(c,name):
    p=Path(c[name])
    if not p.is_absolute() or p.is_symlink() or not p.is_file() or digest(p)!=c[name+'_sha256']:
        raise ValueError('tool identity mismatch')
    return str(p)

def check_config(c, *, require_enabled=True):
    if type(c) is not dict or set(c)-{'audio_mode'}!=FIELDS or c['schema']!='local-clip-producer-v1' or type(c['enabled']) is not bool:
        raise ValueError('producer config schema')
    if c.get('audio_mode','dynamic') not in ('dynamic','linear'):raise ValueError('audio mode')
    if require_enabled and c['enabled'] is not True:raise ValueError('producer disabled')
    if type(c['gpu_uuid']) is not str or not re.fullmatch(gpu_guard.UUID,c['gpu_uuid']):raise ValueError('explicit GPU UUID required')
    if any(type(c[k]) is not str or not re.fullmatch('[0-9a-f]{64}',c[k]) for k in ('source_sha256','cut_key')):raise ValueError('source/cut pin')
    if (c['device'],c['compute_type']) not in [('cpu','int8'),('cuda','float16')]:raise ValueError('explicit device required')
    for k in ('asr_timeout','render_timeout'):
        if type(c[k]) is not int or not 1<=c[k]<=180:raise ValueError('bounded timeout required')
    model=Path(c['model']);files=c['model_files']
    if not model.is_absolute() or not model.is_dir() or model.is_symlink() or type(files) is not dict or not {'model.bin','config.json','tokenizer.json'}<=set(files):raise ValueError('local model manifest required')
    actual={p.name for p in model.iterdir()}
    if actual!=set(files):raise ValueError('model inventory mismatch')
    for name,pin in files.items():
        if not re.fullmatch(r'[A-Za-z0-9_.-]+',name) or name in ('.','..') or type(pin) is not str or not re.fullmatch('[0-9a-f]{64}',pin):raise ValueError('model manifest invalid')
        p=model/name
        if p.is_symlink() or not p.is_file() or digest(p)!=pin:raise ValueError('model bytes mismatch')
    for name in ('ffmpeg','ffprobe','nvidia_smi'):pinned_tool(c,name)
    missing=[n for n in ('faster_whisper','ctranslate2','numpy') if importlib.util.find_spec(n) is None]
    if missing:raise ValueError('missing local ASR dependencies: '+','.join(missing))

def exclusive_json(path,value):
    raw=json.dumps(value,sort_keys=True,separators=(',',':'),allow_nan=False).encode()
    if len(raw)>1024*1024:raise ValueError('evidence size')
    with Path(path).open('xb') as f:f.write(raw);f.flush();os.fsync(f.fileno())

def bounded(args,root,seconds,*,stdout=subprocess.PIPE,input_data=None,stderr=subprocess.DEVNULL):
    worker_cancel.checkpoint()
    env=dict(os.environ,HF_HUB_OFFLINE='1',TRANSFORMERS_OFFLINE='1',HF_HUB_DISABLE_TELEMETRY='1',PYTHONDONTWRITEBYTECODE='1')
    env.pop('PYTHONPATH',None)
    flags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0
    with subprocess.Popen(args,cwd=root,stdin=subprocess.PIPE if input_data is not None else subprocess.DEVNULL,
                          stdout=stdout,stderr=stderr,env=env,creationflags=flags) as child:
        try:
            deadline=time.monotonic()+seconds
            while True:
                worker_cancel.checkpoint()
                remaining=deadline-time.monotonic()
                if remaining<=0:raise subprocess.TimeoutExpired(args,seconds)
                try:
                    out,_=child.communicate(input=input_data,timeout=min(.1,remaining));break
                except subprocess.TimeoutExpired:input_data=None
        except BaseException:
            # Exact child only; no process-tree/foreign-process termination.
            child.terminate()
            try:child.wait(timeout=3)
            except subprocess.TimeoutExpired:child.kill();child.wait(timeout=3)
            raise
        if child.returncode:raise RuntimeError('owned child failed')
        if out is not None and len(out)>1024*1024:raise ValueError('child output too large')
        return out

def gpu_idle(c,root,stage):
    return gpu_guard.guard(pinned_tool(c,'nvidia_smi'),c['gpu_uuid'],root,stage)

def measure_audio(ffmpeg,source,root,start_ms,end_ms):
    # Put input trim before the filter: output -t alone can measure past the cut.
    if not 0<=start_ms<end_ms or end_ms-start_ms>60000:raise ValueError('meter range')
    graph=('aformat=channel_layouts=stereo,astats=metadata=1:reset=0,'
           'ametadata=print:key=lavfi.astats.Overall.Peak_level:file=-,'
           'ebur128=metadata=1,ametadata=print:key=lavfi.r128.I:file=-')
    raw=bounded([ffmpeg,'-nostdin','-v','error','-xerror','-protocol_whitelist','file,pipe',
                 '-ss',f'{start_ms/1000:.3f}','-to',f'{end_ms/1000:.3f}','-i',str(source),
                 '-map','0:a:0','-vn','-af',graph,'-f','null','-'],root,45)
    vals={}
    for line in raw.decode('ascii').splitlines():
        k,sep,v=line.partition('=')
        if k in ('lavfi.r128.I','lavfi.astats.Overall.Peak_level'):
            vals[k]=float(v)
    peak=vals.get('lavfi.astats.Overall.Peak_level');level=vals.get('lavfi.r128.I')
    if peak is None or (peak!=-math.inf and not math.isfinite(peak)):raise ValueError('audio peak unavailable')
    silent=peak==-math.inf
    if not silent and (level is None or not math.isfinite(level) or (end_ms-start_ms<400 and peak>-50)):
        raise ValueError('integrated loudness unavailable')
    quiet=silent or level<=-50
    return dict(input_integrated_lufs=None if silent else level,input_sample_peak_dbfs=None if silent else peak,
                near_silent=quiet,digital_silence=silent,mode='bypass' if quiet else 'dynamic')

def loudnorm_report(path):
    with path.open('rb') as stream:
        stream.seek(max(0,path.stat().st_size-8192))
        tail=stream.read().decode('utf8',errors='replace')
    reports=re.findall(r'\{[^{}]*"normalization_type"[^{}]*\}',tail)
    if len(reports)!=1:raise ValueError('missing or ambiguous loudnorm report')
    return json.loads(reports[0])

def linear_prepare(ff,source,root,a,b,measurement,mode):
    if mode not in ('dynamic','linear'):raise ValueError('audio mode')
    if mode!='linear' or measurement['near_silent']:return
    log=root/'linear-analysis.log'
    with log.open('xb') as err:
        bounded([ff,'-nostdin','-hide_banner','-nostats','-v','info','-ss',f'{a/1000:.3f}','-to',f'{b/1000:.3f}',
                 '-protocol_whitelist','file,pipe','-i',str(source),'-map','0:a:0','-af',
                 'loudnorm=I=-14:TP=-1.5:LRA=11:print_format=json','-f','null','-'],root,60,stderr=err)
    report=loudnorm_report(log)
    fields={'measured_I':'input_i','measured_TP':'input_tp','measured_LRA':'input_lra','measured_thresh':'input_thresh','offset':'target_offset'}
    measured={k:float(report[v]) for k,v in fields.items()}
    if not all(math.isfinite(v) for v in measured.values()):raise ValueError('nonfinite linear measurement')
    measurement.update(mode='linear_requested',linear_measurements=measured)

def encode_checked(command,root,seconds,measurement):
    if measurement['mode']!='linear_requested':
        return bounded(command,root,seconds,stdout=subprocess.DEVNULL)
    command=list(command)
    command[command.index('-v')+1]='info'
    command[1:1]=['-hide_banner','-nostats']
    log=root/'linear-encode.log'
    with log.open('xb') as err:bounded(command,root,seconds,stdout=subprocess.DEVNULL,stderr=err)
    report=loudnorm_report(log)
    if report.get('normalization_type')!='linear':raise ValueError('requested linear normalization fell back: HOLD')
    output={key:float(report[key]) for key in ('output_i','output_tp','output_lra')}
    if not all(math.isfinite(v) for v in output.values()):raise ValueError('nonfinite linear output')
    measurement.update(mode='linear_verified',linear_filter_output=output)

def audio_output(measurement):
    graph='anull' if measurement['near_silent'] else 'loudnorm=I=-14:TP=-1.5:LRA=11'
    if measurement['mode']=='linear_requested':
        graph+=':linear=true:print_format=json:'+':'.join(f'{k}={v}' for k,v in measurement['linear_measurements'].items())
    return ['-af',graph,'-c:a','aac','-ar','48000','-ac','2','-b:a','128k']

def render_command(c,source,plan,partial,measurement):
    a,b=plan['start_ms'],plan['end_ms']
    if type(a) is not int or type(b) is not int or not 0<b-a<=60000:raise ValueError('cut bounds')
    return [pinned_tool(c,'ffmpeg'),'-nostdin','-v','error','-protocol_whitelist','file,pipe','-ss',f'{a/1000:.3f}',
            '-i',str(source),'-map','0:v:0','-map','0:a:0','-vf','scale=1080:608,pad=1080:1920:0:656,setsar=1'+{'captioned':',ass=selected.ass','no_speech':''}[plan['caption_status']],
            '-c:v','h264_nvenc','-gpu','0','-pix_fmt','yuv420p',*audio_output(measurement),
            '-t',f'{(b-a)/1000:.3f}','-movflags','+faststart','-f','mp4','-n',str(partial)]

def vad_segments(model,audio):
    for count,options in enumerate((None,{'threshold':0.3,'min_speech_duration_ms':150}),1):
        try:
            stream,_=model.transcribe(audio,language='en',beam_size=3,word_timestamps=True,
                                      vad_filter=True,vad_parameters=options)
        except ValueError as exc:
            # Only the pinned vendor's empty VAD chunk concatenation is benign.
            tb=exc.__traceback__
            while tb and tb.tb_frame.f_code is not model.transcribe.__func__.__code__:tb=tb.tb_next
            if (str(exc)!='need at least one array to concatenate' or not tb
                    or tb.tb_frame.f_locals.get('speech_chunks')!=[]
                    or tb.tb_frame.f_locals.get('audio_chunks')!=[]):raise
            stream=()
        segments=list(stream)
        if segments:return segments,count
    return [],2

def backend_evidence(requested_device, requested_compute, actual_device, actual_compute):
    allowed = {('cpu', 'int8'): {('cpu', 'int8'), ('cpu', 'int8_float32')},
               ('cuda', 'float16'): {('cuda', 'float16')}}
    if (actual_device, actual_compute) not in allowed.get((requested_device, requested_compute), set()):
        raise ValueError('ASR backend mismatch')
    return dict(requested_device=requested_device, requested_compute_type=requested_compute,
                device=actual_device, compute_type=actual_compute)

def asr_child(payload):
    if _runtime_identity is None or payload.get('runtime_identity')!=_runtime_identity:raise ValueError('ASR runtime identity missing')
    if _runtime_files is None:raise ValueError('ASR DLL policy missing')
    import dll_inventory
    c=payload['config'];check_config(c)
    import numpy as np
    import faster_whisper,ctranslate2
    from faster_whisper import WhisperModel
    dll_inventory.enforce_snapshot(_runtime_files)
    pcm=Path(payload['pcm'])
    if pcm.stat().st_size>16000*4*60 or digest(pcm)!=payload['pcm_sha256']:raise ValueError('PCM pin')
    audio=np.fromfile(pcm,dtype='<f4')
    if not len(audio) or not np.isfinite(audio).all():raise ValueError('PCM invalid')
    model=WhisperModel(c['model'],device=c['device'],compute_type=c['compute_type'],cpu_threads=8,local_files_only=True)
    dll_inventory.enforce_snapshot(_runtime_files)
    actual_device=model.model.device;actual_compute=model.model.compute_type
    backend=backend_evidence(c['device'],c['compute_type'],actual_device,actual_compute)
    segments,passes=vad_segments(model,audio)
    status='captioned' if segments else 'no_speech'
    if status!=payload['caption_status']:raise ValueError('reviewed caption status mismatch')
    words=[]
    for segment in segments:
        dll_inventory.enforce_snapshot(_runtime_files)
        for w in segment.words or []:
            a,b=float(w.start),float(w.end)
            if a==b:continue
            if not all(math.isfinite(t) for t in (a,b)) or not 0<=a<b<=payload['duration']:raise ValueError('word outside exact cut')
            words.append((a,b,w.word))
    dll_inventory.enforce_snapshot(_runtime_files)
    if segments:ass,cues=make_ass(words,duration=payload['duration'])
    evidence={'schema':'local-asr-evidence-v1','source_sha256':c['source_sha256'],'cut_key':c['cut_key'],
        'model_files':c['model_files'],**backend,'language':'en','caption_status':status,'vad_passes':passes,
        'versions':{'faster-whisper':faster_whisper.__version__,'ctranslate2':ctranslate2.__version__,'numpy':np.__version__},'words':words}
    exclusive_json('asr-evidence.json',evidence)
    if not segments:return
    with Path('selected.ass').open('x',encoding='utf-8',newline='\n') as f:f.write(ass);f.flush();os.fsync(f.fileno())

def create_poster(ffmpeg, source, key, root):
    # One scaled frame from the actual encoded artifact, including burned-in captions.
    dest=root/(key+'.webp')
    with dest.open('xb') as f:
        bounded([ffmpeg,'-nostdin','-v','error','-protocol_whitelist','file,pipe','-i',str(source),
                 '-map','0:v:0','-an','-frames:v','1','-vf','scale=540:960','-c:v','libwebp',
                 '-q:v','75','-threads','1','-f','webp','pipe:1'], root,15,stdout=f)
        f.flush();os.fsync(f.fileno())
    return dest

def producer(preparation,c):
    check_config(c)
    if c['source_sha256']!=preparation['source_sha256'] or c['cut_key']!=preparation['cut_key'] or c['model']!=preparation['model']:
        raise ValueError('producer/preparation identity')
    for name in ('ffmpeg','ffprobe'):
        if c[name]!=preparation[name] or c[name+'_sha256']!=preparation[name+'_sha256']:raise ValueError('producer tool mismatch')
    source=Path(preparation['source']).resolve(strict=True)
    def produce(plan,out):
        worker_cancel.checkpoint()
        if os.name!='nt':raise RuntimeError('native Windows staging only')
        if plan['cut_key']!=c['cut_key'] or digest(source)!=c['source_sha256']:raise ValueError('source/cut changed')
        root=out.parent;partial=root/(out.name+'.partial');pcm=root/'selected.f32'
        if any(p.exists() for p in (out,partial,pcm,root/'selected.ass',root/'asr-evidence.json',root/'producer-evidence.json')):
            raise RuntimeError('existing output or ambiguous attempt; no retry')
        gpu_before=gpu_idle(c,root,'postclaim')
        worker_cancel.checkpoint()
        a,b=plan['start_ms'],plan['end_ms'];duration=(b-a)/1000
        ffmpeg=pinned_tool(c,'ffmpeg')
        with pcm.open('xb') as f:
            bounded([ffmpeg,'-nostdin','-v','error','-protocol_whitelist','file,pipe','-ss',f'{a/1000:.3f}','-i',str(source),
                     '-t',f'{duration:.3f}','-ac','1','-ar','16000','-f','f32le','-'],root,30,stdout=f)
            f.flush();os.fsync(f.fileno())
        if pcm.stat().st_size>16000*4*60:raise ValueError('PCM limit')
        payload=json.dumps(dict(config=c,pcm=str(pcm),pcm_sha256=digest(pcm),duration=duration,caption_status=plan['caption_status'],runtime_identity=_runtime_identity)).encode()
        phase_event(root,1,plan['worker_job_id'],plan['cut_key'])
        bounded([sys.executable,'-I','-S','-B','-u',str(Path(__file__).with_name('asr_bootstrap.py').resolve()),'--asr-child'],root,c['asr_timeout'],input_data=payload)
        phase_event(root,2,plan['worker_job_id'],plan['cut_key'])
        # ASR child has exited, including any CUDA context; don't overlap GPU work.
        measurement=measure_audio(ffmpeg,source,root,a,b)
        linear_prepare(ffmpeg,source,root,a,b,measurement,c.get('audio_mode','dynamic'))
        gpu_before_render=gpu_idle(c,root,'preencode')
        worker_cancel.checkpoint()
        phase_event(root,3,plan['worker_job_id'],plan['cut_key'])
        command=render_command(c,source,plan,partial,measurement)
        if measurement['mode']=='linear_requested':
            command[command.index('-i'):command.index('-i')]=['-to',f'{b/1000:.3f}']
        encode_checked(command,root,c['render_timeout'],measurement)
        phase_event(root,4,plan['worker_job_id'],plan['cut_key'])
        probe=json.loads(bounded([pinned_tool(c,'ffprobe'),'-v','error','-protocol_whitelist','file,pipe','-show_streams','-show_format','-of','json',str(partial)],root,15))
        validate(probe)
        if abs(float(probe['format']['duration'])-duration)>.15 or not faststart(partial):raise ValueError('duration/faststart')
        bounded([ffmpeg,'-nostdin','-v','error','-protocol_whitelist','file,pipe','-i',str(partial),'-f','null','-'],root,30,stdout=subprocess.DEVNULL)
        poster=create_poster(ffmpeg,partial,plan['cut_key'],root)
        if not 0<poster.stat().st_size<=2097152:raise ValueError('poster size')
        pp=json.loads(bounded([pinned_tool(c,'ffprobe'),'-v','error','-protocol_whitelist','file,pipe','-show_streams','-of','json',str(poster)],root,10))
        streams=pp['streams']
        if len(streams)!=1 or (streams[0]['codec_name'],streams[0]['width'],streams[0]['height'])!=('webp',540,960):raise ValueError('poster format')
        phase_event(root,5,plan['worker_job_id'],plan['cut_key'])
        if digest(source)!=c['source_sha256']:raise ValueError('source changed during producer')
        with partial.open('r+b') as f:f.flush();os.fsync(f.fileno())
        exclusive_json(root/'producer-evidence.json',dict(schema='local-producer-evidence-v1',source_sha256=c['source_sha256'],cut_key=c['cut_key'],
            ffmpeg_sha256=c['ffmpeg_sha256'],ffprobe_sha256=c['ffprobe_sha256'],gpu_before=gpu_before,gpu_before_render=gpu_before_render,
            audio_input=measurement,audio_target_bps=128000,audio_measured_bps=int(next(s for s in probe['streams'] if s['codec_type']=='audio')['bit_rate']),codec='h264_nvenc',probe=probe,asr_evidence_sha256=digest(root/'asr-evidence.json'),artifact_sha256=digest(partial),artifact_bytes=partial.stat().st_size))
        # Windows same-directory rename refuses an existing destination. No cleanup
        # or fallback: any partial/collision is durable HOLD, not retry authority.
        worker_cancel.checkpoint()
        os.rename(partial,out)
        return probe['format']['duration']
    return produce

def run_local(preparation,workspace,config,*,worker_job_id=None):
    worker_cancel.checkpoint()
    if _runtime_identity is None:raise ValueError('verified bootstrap runtime identity required')
    from stage_queue import run_reviewed
    check_config(config)
    with ExitStack() as leases:
        paths=[preparation['source'],preparation['intake'],preparation['review']]
        paths += [config[n] for n in ('ffmpeg','ffprobe','nvidia_smi')]
        paths += [nvml_encoder_probe.DLL]
        paths += [Path(config['model'])/name for name in config['model_files']]
        for path in paths:leases.enter_context(read_lease(path))
        # Recheck under leases; no writable source/model/tool window during use.
        return run_reviewed(preparation,workspace,producer=producer(preparation,config),
                            preclaim=lambda plan,root:gpu_idle(config,root,'preclaim'),worker_job_id=worker_job_id,cancel=worker_cancel.checkpoint)

if __name__=='__main__':
    if sys.argv[1:]!=['--asr-child']:raise SystemExit('No standalone render CLI; admitted local app caller required')
    try:
        raw=sys.stdin.buffer.read(65537)
        if len(raw)>65536:raise ValueError('ASR input cap')
        asr_child(json.loads(raw))
    except Exception:
        raise SystemExit('ASR refused; retained attempt requires investigation')
