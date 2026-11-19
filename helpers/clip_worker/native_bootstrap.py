"""Pinned native worker entry. Diagnostic qualified first; render stays disabled."""
import sys,time
_started=time.monotonic()
_trace=open('bootstrap-phases.txt','x',encoding='ascii')
def phase(label):
    _trace.write(f'{time.monotonic()-_started:.6f} {label}\n');_trace.flush()
phase('WAIT_GO')
if sys.stdin.buffer.readline()!=b'GO\n':raise SystemExit(20)
phase('GO_RECEIVED')
import pathlib,json,hashlib,os,threading,importlib.machinery,importlib.metadata
phase('STDLIB_IMPORTED')

def digest(path):
    with open(path,'rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()


def pinned_module_origin(spec, files):
    """Check a file or an exact retained ZIP container, never a path prefix alone."""
    import zipimport
    if isinstance(spec.loader, zipimport.zipimporter):
        archive = pathlib.Path(spec.loader.archive).resolve(strict=True)
        key = os.path.normcase(str(archive))
        if key not in files or digest(archive) != files[key]:
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
    if origin not in files or digest(origin) != files[origin]:
        raise ImportError('unlisted or changed module')
    if spec.cached and pathlib.Path(spec.cached).exists():
        cached = os.path.normcase(str(pathlib.Path(spec.cached).resolve()))
        if cached not in files or digest(cached) != files[cached]:
            raise ImportError('unlisted or changed bytecode')

def run():
    if len(sys.argv)!=2:raise ValueError('config argv')
    config_path=pathlib.Path(sys.argv[1]);raw=config_path.read_bytes()
    if len(raw)>16384:raise ValueError('config size')
    def unique_fields(pairs):
        result={}
        for key,value in pairs:
            if key in result:raise ValueError('duplicate bootstrap field')
            result[key]=value
        return result
    c=json.loads(raw,object_pairs_hook=unique_fields)
    phase('CONFIG_PARSED')
    expected={'schema','mode','manifest','manifest_sha256','worker_root','site_packages','output','wait_cancel'}
    if c.get('mode')=='diagnostic-vendor':expected|={'vendor_cases'}
    if c.get('mode')=='reviewed-job':expected|={'request','publishing_enabled','preparation','preparation_sha256','producer_config','producer_config_sha256','asr_python'}
    if c.get('mode') in ('diagnostic-asr','diagnostic-asr-tone'):expected|={'asr_model','asr_python'}
    if c.get('mode')=='diagnostic-asr-tone':expected|={'diagnostic_wav','diagnostic_wav_sha256'}
    if set(c)!=expected or c['schema']!=('native-bootstrap-reviewed-job-v1' if c['mode']=='reviewed-job' else 'native-bootstrap-diagnostic-v1') or c['mode'] not in ('diagnostic','diagnostic-asr','diagnostic-asr-tone','diagnostic-vendor','reviewed-job') or type(c['wait_cancel']) is not bool:
        raise ValueError('diagnostic config only; render disabled')
    manifest=pathlib.Path(c['manifest']);mraw=manifest.read_bytes()
    phase('MANIFEST_BYTES_READ')
    if len(mraw)>8*1024*1024 or hashlib.sha256(mraw).hexdigest()!=c['manifest_sha256']:raise ValueError('manifest pin')
    m=json.loads(mraw)
    phase('MANIFEST_HASH_VALID')
    if set(m)!={'schema','files'} or m['schema']!='native-runtime-files-v1':raise ValueError('manifest schema')
    files={os.path.normcase(str(pathlib.Path(p).resolve())):h for p,h in m['files'].items()}
    phase('MANIFEST_PATHS_NORMALIZED')
    if len(files)!=len(m['files']):raise ValueError('duplicate normalized manifest path')
    phase('FILE_HASH_BEGIN')
    for index,(path,pin) in enumerate(files.items()):
        if not pathlib.Path(path).is_absolute() or digest(path)!=pin:raise ValueError('manifest bytes')
        if index%1000==0:phase('FILE_HASH_COUNT_'+str(index))
    phase('FILE_HASH_COMPLETE')
    class PinnedImports:
        def find_spec(self,fullname,path=None,target=None):
            spec=importlib.machinery.PathFinder.find_spec(fullname,path,target)
            if spec is None:return None
            if spec.origin is None: # namespace package: only listed file parents
                if any(not any(p.startswith(os.path.normcase(str(pathlib.Path(d).resolve()))+os.sep) for p in files) for d in spec.submodule_search_locations or []):raise ImportError('unlisted namespace')
            else:
                pinned_module_origin(spec, files)
            return spec
    # Replace PathFinder, preserving built-in/frozen importers; never run .pth/site hooks.
    sys.meta_path=[PinnedImports() if x is importlib.machinery.PathFinder else x for x in sys.meta_path]
    sys.path[:0]=[c['worker_root'],c['site_packages']]
    os.environ.update(HF_HUB_OFFLINE='1',TRANSFORMERS_OFFLINE='1',HF_HUB_DISABLE_TELEMETRY='1',PYTHONDONTWRITEBYTECODE='1')
    import dll_inventory
    dll_inventory.install(files)
    import worker_cancel
    phase('CANCELLATION_IMPORTED')
    # Control reads own descriptor/lock; dependencies see EOF, never controller input.
    # Parent cancellation closes its writer, so EOF is sticky cancellation even
    # if the initial buffered GO read had already prefetched a STOP byte.
    _original_stdin=sys.stdin
    control=os.fdopen(os.dup(_original_stdin.fileno()),'rb',buffering=0)
    sys.stdin=open(os.devnull,'r',encoding='utf-8');sys.__stdin__=sys.stdin
    threading.Thread(target=worker_cancel.listen,args=(control,),daemon=True).start()
    phase('CONTROL_DESCRIPTOR_ISOLATED')
    worker_cancel.checkpoint()
    if c['mode']=='diagnostic-vendor':
        # Retained manifest leases, import/DLL policy and cancellation are active.
        # Branch before producer/model imports: no request or dispatch capability.
        if c['wait_cancel']:raise ValueError('vendor wait mode')
        import vendor_diagnostic
        phase('VENDOR_DIAGNOSTIC_BEGIN')
        vendor_diagnostic.run(c,files,phase,worker_cancel.checkpoint)
        phase('VENDOR_DIAGNOSTIC_COMPLETE');return 0
    phase('WORKER_IMPORT_BEGIN')
    import local_producer
    local_producer._runtime_identity=dict(manifest=c['manifest'],manifest_sha256=c['manifest_sha256'],bootstrap_config=str(config_path.resolve()),bootstrap_config_sha256=hashlib.sha256(raw).hexdigest())
    phase('WORKER_IMPORT_COMPLETE')
    if c['mode']=='reviewed-job':
        import reviewed_job
        from prepare import prepare
        reviewed_job.dispatch(c,local_producer._runtime_identity,files,worker=local_producer.run_local,prepare_check=prepare)
        phase('REVIEWED_JOB_COMPLETE');return 0
    phase('FASTER_WHISPER_IMPORT_BEGIN')
    import faulthandler
    stack=open('import-stack.txt','x',encoding='ascii')
    faulthandler.dump_traceback_later(3, file=stack)
    try:
        import faster_whisper
    finally:
        faulthandler.cancel_dump_traceback_later();stack.close()
    phase('FASTER_WHISPER_IMPORT_COMPLETE')
    import ctranslate2,numpy
    phase('NATIVE_IMPORT_COMPLETE')
    # Import only: never instantiate model, query GPU, open media, or dispatch Queue.
    worker_cancel.checkpoint()
    result={'schema':'native-bootstrap-diagnostic-result-v1','pid':os.getpid(),'render_enabled':False,'model_loaded':False,'gpu_queried':False,'queue_started':False,'manifest_sha256':c['manifest_sha256'],'config_sha256':hashlib.sha256(raw).hexdigest(),'versions':{'faster-whisper':faster_whisper.__version__,'ctranslate2':ctranslate2.__version__,'numpy':numpy.__version__}}
    with open(c['output'],'x',encoding='utf-8') as f:json.dump(result,f);f.flush();os.fsync(f.fileno())
    phase('RESULT_WRITTEN')
    if c['mode'] in ('diagnostic-asr','diagnostic-asr-tone'):
        python=pathlib.Path(c['asr_python']);key=os.path.normcase(str(python.resolve()))
        if key not in files or digest(python)!=files[key]:raise ValueError('ASR interpreter pin')
        payload=dict(runtime_identity=local_producer._runtime_identity,model=c['asr_model'],output=str(pathlib.Path('asr-diagnostic.json').resolve()))
        if c['mode']=='diagnostic-asr-tone':payload.update(diagnostic_wav=c['diagnostic_wav'],diagnostic_wav_sha256=c['diagnostic_wav_sha256'])
        phase('ASR_DIAGNOSTIC_BEGIN')
        local_producer.bounded([str(python),'-I','-S','-B','-u',str(pathlib.Path(c['worker_root'])/'asr_bootstrap.py'),'--diagnostic'],pathlib.Path.cwd(),20,input_data=json.dumps(payload).encode())
        phase('ASR_DIAGNOSTIC_COMPLETE')
    if c['wait_cancel']:
        worker_cancel._cancelled.wait(10);worker_cancel.checkpoint()
    phase('EXIT_ZERO');return 0

try:raise SystemExit(run())
except Exception as exc:
    # Fixed class only; no paths/configs/media/credentials in stderr.
    phase('EXIT_HOLD_'+type(exc).__name__)
    print('BOOTSTRAP_HOLD:'+type(exc).__name__,file=sys.stderr);raise SystemExit(31)
