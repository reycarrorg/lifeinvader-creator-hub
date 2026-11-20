"""Isolated ASR child identity gate. No execution on import; no downloads."""
import sys,json,hashlib,pathlib,os,importlib.machinery

def diagnostic_phase(label):
    # Fixed labels only; never record payload/audio/transcript/credentials.
    with open('asr-phases.jsonl','a',encoding='ascii') as f:
        f.write(json.dumps({'pid':os.getpid(),'phase':label})+'\n');f.flush()


def sha(path):
    with open(path,'rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()

def pinned_module_origin(spec, files):
    """Check a file or an exact retained ZIP container, never a path prefix alone."""
    import zipimport
    if isinstance(spec.loader, zipimport.zipimporter):
        archive = pathlib.Path(spec.loader.archive).resolve(strict=True)
        key = os.path.normcase(str(archive))
        if key not in files or sha(archive) != files[key]:
            raise ImportError('unlisted or changed module archive')
        origin = pathlib.Path(spec.origin)
        try: member = origin.relative_to(archive)
        except ValueError: raise ImportError('archive origin mismatch') from None
        if not member.parts or any(p in ('.', '..') for p in member.parts):
            raise ImportError('archive member escape')
        if spec.loader.get_filename(spec.name) != spec.origin:
            raise ImportError('archive loader mismatch')
        return
    origin = os.path.normcase(str(pathlib.Path(spec.origin).resolve()))
    if origin not in files or sha(origin) != files[origin]:
        raise ImportError('unlisted or changed module')
    if spec.cached and pathlib.Path(spec.cached).exists():
        cached = os.path.normcase(str(pathlib.Path(spec.cached).resolve()))
        if cached not in files or sha(cached) != files[cached]:
            raise ImportError('unlisted or changed bytecode')

def verify(identity):
    keys={'manifest','manifest_sha256','bootstrap_config','bootstrap_config_sha256'}
    if type(identity) is not dict or set(identity)!=keys:raise ValueError('ASR runtime identity missing')
    for name in ('manifest','bootstrap_config'):
        if not pathlib.Path(identity[name]).is_absolute() or sha(identity[name])!=identity[name+'_sha256']:raise ValueError('ASR identity bytes')
    m=json.loads(pathlib.Path(identity['manifest']).read_bytes());c=json.loads(pathlib.Path(identity['bootstrap_config']).read_bytes())
    if m.get('schema')!='native-runtime-files-v1' or c.get('manifest')!=identity['manifest'] or c.get('manifest_sha256')!=identity['manifest_sha256']:raise ValueError('ASR identity binding')
    files={os.path.normcase(str(pathlib.Path(p).resolve())):h for p,h in m['files'].items()}
    for p,h in files.items():
        if sha(p)!=h:raise ValueError('ASR dependency changed')
    for p in (sys.executable,__file__,str(pathlib.Path(c['worker_root'])/'local_producer.py')):
        key=os.path.normcase(str(pathlib.Path(p).resolve()))
        if key not in files or sha(key)!=files[key]:raise ValueError('ASR executable/module unlisted')
    return c,files

def main():
    if sys.argv[1:] not in (['--asr-child'],['--diagnostic']):raise ValueError('ASR invocation')
    raw=sys.stdin.buffer.read(65537)
    if len(raw)>65536:raise ValueError('ASR payload cap')
    payload=json.loads(raw);identity=payload.get('runtime_identity');c,files=verify(identity)
    class PinnedImports:
        def find_spec(self,fullname,path=None,target=None):
            spec=importlib.machinery.PathFinder.find_spec(fullname,path,target)
            if spec is None:return None
            if spec.origin is None:
                if any(not any(p.startswith(os.path.normcase(str(pathlib.Path(d).resolve()))+os.sep) for p in files) for d in spec.submodule_search_locations or []):raise ImportError('ASR namespace unlisted')
            else:
                pinned_module_origin(spec, files)
            return spec
    sys.meta_path=[PinnedImports() if x is importlib.machinery.PathFinder else x for x in sys.meta_path]
    sys.path[:0]=[c['worker_root'],c['site_packages']]
    os.environ.update(HF_HUB_OFFLINE='1',TRANSFORMERS_OFFLINE='1',HF_HUB_DISABLE_TELEMETRY='1')
    import dll_inventory
    dll_inventory.install(files)
    import local_producer
    local_producer._runtime_identity=identity
    local_producer._runtime_files=files
    if sys.argv[1:]==['--diagnostic']:
        diagnostic(payload,c,files,local_producer)
    else:local_producer.asr_child(payload)

def diagnostic(payload,c,files,producer):
    # Tone inference is an explicit diagnostic only; no queue/render admission.
    expected={'runtime_identity','model','output'}
    tone=c.get('mode')=='diagnostic-asr-tone'
    if tone:expected|={'diagnostic_wav','diagnostic_wav_sha256'}
    if set(payload)!=expected or c.get('mode') not in ('diagnostic-asr','diagnostic-asr-tone') or payload['model']!=c['asr_model']:
        raise ValueError('diagnostic binding')
    import dll_inventory
    import faster_whisper,ctranslate2,numpy,time
    started=time.monotonic()
    before=dll_inventory.enforce_snapshot(files)
    producer.exclusive_json('asr-import-dlls.json',before)
    if before['unlisted']:raise ValueError('unlisted DLL before model')
    model=pathlib.Path(payload['model'])
    if not model.is_absolute() or model.is_symlink() or {p.name for p in model.iterdir()}!={'config.json','model.bin','tokenizer.json','vocabulary.txt'}:raise ValueError('model inventory')
    for p in model.iterdir():
        key=os.path.normcase(str(p.resolve()))
        if p.is_symlink() or key not in files or sha(p)!=files[key]:raise ValueError('model pin')
    instance=faster_whisper.WhisperModel(str(model),device='cpu',compute_type='int8',cpu_threads=8,local_files_only=True)
    backend=producer.backend_evidence('cpu','int8',instance.model.device,instance.model.compute_type)
    after=dll_inventory.enforce_snapshot(files)
    producer.exclusive_json('asr-model-dlls.json',after)
    if after['unlisted']:raise ValueError('unlisted DLL after model')
    diagnostic_phase('MODEL_VALIDATED')
    calls={'encode':0,'generate':0};segment_count=0
    if tone:
        import wave
        wav=pathlib.Path(payload['diagnostic_wav']);key=os.path.normcase(str(wav.resolve()))
        if str(wav)!=c['diagnostic_wav'] or payload['diagnostic_wav_sha256']!=c['diagnostic_wav_sha256'] or key not in files or sha(wav)!=files[key] or sha(wav)!=c['diagnostic_wav_sha256']:raise ValueError('tone identity')
        with wave.open(str(wav),'rb') as audio:
            if (audio.getnchannels(),audio.getsampwidth(),audio.getframerate(),audio.getnframes())!=(1,2,16000,48000):raise ValueError('tone format')
            pcm=numpy.frombuffer(audio.readframes(48000),dtype='<i2').astype(numpy.float32)/32768.0
        class CountNativeCalls:
            def __init__(self,inner):self.inner=inner
            def __getattr__(self,name):return getattr(self.inner,name)
            def encode(self,*args,**kwargs):
                calls['encode']+=1;diagnostic_phase('ENCODE_ENTER');result=self.inner.encode(*args,**kwargs);diagnostic_phase('ENCODE_RETURN');return result
            def generate(self,*args,**kwargs):
                calls['generate']+=1;diagnostic_phase('GENERATE_ENTER');result=self.inner.generate(*args,**kwargs);diagnostic_phase('GENERATE_RETURN');return result
        diagnostic_phase('TONE_VALIDATED')
        instance.model=CountNativeCalls(instance.model)
        segments,_=instance.transcribe(pcm,language='en',beam_size=1,temperature=0.0,vad_filter=False,no_speech_threshold=None,condition_on_previous_text=False,word_timestamps=False)
        diagnostic_phase('GENERATOR_CREATED')
        for segment in segments:
            diagnostic_phase('SEGMENT_YIELDED')
            segment_count+=1;dll_inventory.enforce_snapshot(files)
        loaded=dll_inventory.snapshot(files)
        producer.exclusive_json('asr-inference-dlls.json',loaded)
        dll_inventory.enforce_snapshot(files)
        if calls['encode']<1 or calls['generate']<1:raise ValueError('decoder not exercised')
    producer.exclusive_json(payload['output'],dict(schema='asr-no-media-diagnostic-v1',pid=os.getpid(),executable=sys.executable,elapsed=time.monotonic()-started,backend=backend,transcription=tone,native_calls=calls,segments=segment_count,synthetic_tone_only=tone,caption_validation=False,gpu_query=False,queue=False,system_trust_pending=False,manifest_sha256=payload['runtime_identity']['manifest_sha256']))

if __name__=='__main__':
    try:main()
    except Exception as exc:
        # Capture fixed class and code locations only, no exception message or locals.
        import traceback
        record={'state':'HOLD','exception_class':type(exc).__name__,'frames':[{'file':pathlib.Path(t.filename).name,'function':t.name,'line':t.lineno} for t in traceback.extract_tb(exc.__traceback__)[-8:]]}
        try:
            with open('asr-error.json','x',encoding='ascii') as f:json.dump(record,f);f.flush()
        except Exception:pass
        raise SystemExit('ASR identity/admission HOLD; no retry')
