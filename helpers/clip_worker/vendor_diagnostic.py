"""Explicit vendor-only bootstrap diagnostic. No render/queue/model capability."""
import datetime,hashlib,json,os,re,subprocess,threading,time
from pathlib import Path
TOOL=Path(r'C:\Windows\System32\nvidia-smi.exe')
PIN='19dfd1b1cd1e0bb24c720f18252baff639a0f967f305dc409470218ee24d039c'
UUID='GPU-c63f17e9-c9dd-6a3f-42b4-e37830a32799'
CASES={'minimal_control':(),'windir_only':('WINDIR',),'system32_path_only':('PATH',),'windir_and_system32_path':('WINDIR','PATH')}

OS_CASES=['os_dirs_combined','omit_programdata','omit_programfiles','omit_systemdrive']
CASES.update(os_dirs_combined=('ProgramData','ProgramFiles','ProgramW6432','SystemDrive'),omit_programdata=('ProgramFiles','ProgramW6432','SystemDrive'),omit_programfiles=('ProgramData','SystemDrive'),omit_systemdrive=('ProgramData','ProgramFiles','ProgramW6432'))

SINGLE_CASES=['programfiles_only','programw6432_only','programfiles_pair']
CASES.update(programfiles_only=('ProgramFiles',),programw6432_only=('ProgramW6432',),programfiles_pair=('ProgramFiles','ProgramW6432'))

def environment(label,base,root):
    if label not in CASES:raise ValueError('vendor case')
    sr=Path(base['SystemRoot'])
    if not sr.is_absolute() or sr!=TOOL.parent.parent:raise ValueError('system root identity')
    result=dict(SystemRoot=str(sr),TEMP=str(root),TMP=str(root),HF_HUB_OFFLINE='1',TRANSFORMERS_OFFLINE='1',HF_HUB_DISABLE_TELEMETRY='1',PYTHONDONTWRITEBYTECODE='1')
    values={'WINDIR':str(sr),'PATH':str(sr/'System32'),'ProgramData':str(Path(sr.anchor)/'ProgramData'),'ProgramFiles':str(Path(sr.anchor)/'Program Files'),'ProgramW6432':str(Path(sr.anchor)/'Program Files'),'SystemDrive':sr.drive}
    for key in CASES[label]:result[key]=values[key]
    return result

def classify(raw,code):
    if raw.strip()==b'Failed to initialize NVML: Unknown Error':return 'NVML_UNKNOWN_ERROR'
    if code==0 and re.fullmatch(r'0,\s*'+UUID+r',\s*WDDM,\s*(?:[0-9]|[1-9][0-9]|100)',raw.decode('ascii',errors='replace').strip()):return 'VALID_PINNED_INVENTORY'
    return 'UNCLASSIFIED_HOLD'

def capture(stream,data,total):
    while block:=stream.read(1024):
        total[0]+=len(block);data.extend(block[:max(0,4096-len(data))])

def run(config,files,phase,checkpoint):
    labels=config['vendor_cases'];root=Path.cwd();output=Path(config['output'])
    if not isinstance(labels,list) or not 1<=len(labels)<=4 or any(type(x)!=str or x not in CASES for x in labels) or len(set(labels))!=len(labels):raise ValueError('vendor cases')
    if any(x in OS_CASES for x in labels) and labels!=OS_CASES:raise ValueError('OS diagnostic order')
    if any(x in SINGLE_CASES for x in labels) and labels!=SINGLE_CASES:raise ValueError('single diagnostic order')
    if output!=root/'vendor-result.json' or output.exists():raise ValueError('vendor output')
    if files.get(os.path.normcase(str(TOOL.resolve())))!=PIN:raise ValueError('vendor manifest pin')
    with TOOL.open('rb') as f:
        if hashlib.file_digest(f,'sha256').hexdigest()!=PIN:raise ValueError('vendor bytes')
    # Exclusive result opened before any subprocess; fixed codes only, never raw output.
    with output.open('x',encoding='ascii') as log:
        single_passed=False
        for label in labels:
            if label=='programfiles_pair' and single_passed:break
            checkpoint();env=environment(label,os.environ,root)
            phase('VENDOR_START_'+label.upper());start=time.monotonic();p=None
            record=dict(case=label,added_keys=list(CASES[label]),stage='SPAWN',render_authorized=False)
            try:
                p=subprocess.Popen([str(TOOL),'--query-gpu=index,uuid,driver_model.current,utilization.encoder','--format=csv,noheader,nounits'],cwd=root,env=env,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=0x08000000)
                record.update(pid=p.pid,stage='WAIT');data=[bytearray(),bytearray()];counts=[[0],[0]]
                threads=[threading.Thread(target=capture,args=(s,d,n),daemon=True) for s,d,n in zip((p.stdout,p.stderr),data,counts)]
                for t in threads:t.start()
                deadline=start+5
                while p.poll() is None:
                    checkpoint()
                    if time.monotonic()>=deadline:raise TimeoutError('vendor bound')
                    time.sleep(.02)
                for t in threads:t.join(.5)
                if any(t.is_alive() for t in threads):raise RuntimeError('vendor capture')
                raw=bytes(data[0]);err=bytes(data[1]);overflow=any(n[0]>4096 for n in counts)
                category=classify(raw,p.returncode) if not overflow and not err else 'OUTPUT_HOLD'
                record.update(stage='COMPLETE',exit=p.returncode,category=category,stdout_bytes=counts[0][0],stderr_bytes=counts[1][0],stdout_sha256=hashlib.sha256(raw).hexdigest(),stderr_sha256=hashlib.sha256(err).hexdigest(),stderr_category=classify(err,p.returncode) if err else 'EMPTY')
                if category not in ('VALID_PINNED_INVENTORY','NVML_UNKNOWN_ERROR'):raise RuntimeError('vendor result hold')
                if label in SINGLE_CASES[:2] and category=='VALID_PINNED_INVENTORY':single_passed=True
                if label=='os_dirs_combined' and category!='VALID_PINNED_INVENTORY':return # no removal probes after combined failure; finally preserves evidence
            except Exception as exc:
                record.update(error_code={TimeoutError:'TIMEOUT',OSError:'OS_ERROR',ValueError:'CONTRACT'}.get(type(exc),'HOLD'))
                if isinstance(exc,OSError):record['winerror']=getattr(exc,'winerror',None)
                # Do not start another condition after unknown exit/cancellation.
                raise
            finally:
                record.update(seconds=round(time.monotonic()-start,3),utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
                log.write(json.dumps(record)+'\n');log.flush();os.fsync(log.fileno())
                if p is not None:
                    if p.poll() is None:p.terminate();p.wait(timeout=2)
                    for stream in (p.stdout,p.stderr):stream.close()
