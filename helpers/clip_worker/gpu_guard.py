"""Read-only, fail-closed GPU qualification. No process names or commands logged."""
import datetime, hashlib, json, os, re, subprocess, threading, sys, time
from pathlib import Path

UUID = r'GPU-[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}'
QUERIES = {'inventory': '--query-gpu=index,uuid,driver_model.current,utilization.encoder',
           'processes': '--query-compute-apps=gpu_uuid,pid,used_gpu_memory'}

def query(tool, kind, root, expected_uuid=None):
    captured=bytearray(); total=[0]
    try:
        args=([sys.executable,'-I','-S','-B',str(Path(__file__).with_name('nvml_encoder_probe.py')),expected_uuid]
              if kind=='encoder_sessions' else [tool,QUERIES[kind],'--format=csv,noheader,nounits'])
        with subprocess.Popen(args,
                cwd=root,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0) as child:
            def drain():
                while True:
                    block=child.stdout.read(1024)
                    if not block:break
                    total[0]+=len(block)
                    captured.extend(block[:max(0,4096-len(captured))])
            thread=threading.Thread(target=drain,daemon=True);thread.start()
            timed_out=False
            try:code=child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                timed_out=True;child.terminate()
                try:code=child.wait(timeout=3)
                except subprocess.TimeoutExpired:child.kill();code=child.wait(timeout=3)
            thread.join(timeout=2)
            if thread.is_alive():raise RuntimeError('GPU reader did not close')
            return dict(stdout=bytes(captured),bytes=total[0],exit=code,
                        unavailable=False,timeout=timed_out,overflow=total[0]>4096)
    except OSError:
        return dict(stdout=b'',bytes=0,exit=None,unavailable=True,timeout=False,overflow=False)

def guard(tool, expected_uuid, root, stage):
    if not re.fullmatch(UUID,expected_uuid):raise ValueError('explicit GPU UUID required')
    if stage not in ('preclaim','postclaim','preencode'):raise ValueError('GPU diagnostic stage')
    root=Path(root)
    # Exclusive diagnostics also prohibit an automatic retry of this stage.
    with (root/('gpu-'+stage+'.jsonl')).open('x',encoding='utf-8') as log:
        def record(value):
            log.write(json.dumps(dict(time=datetime.datetime.now().astimezone().isoformat(),**value))+'\n')
            log.flush();os.fsync(log.fileno())
        def refuse(code):
            record(dict(decision=code));raise RuntimeError('GPU '+code+'; no retry')
        def read(kind):
            r=query(tool,kind,root);raw=r['stdout']
            try:text=raw.decode('ascii').strip()
            except UnicodeDecodeError:text=''
            rows=[x.strip() for x in text.splitlines()]
            pattern=(r'0,\s*'+UUID+r',\s*(?:WDDM|TCC|N/A|\[N/A\]),\s*(?:[0-9]+|N/A|\[N/A\])'
                     if kind=='inventory' else UUID+r',\s*[1-9][0-9]*,\s*(?:[0-9]+|N/A|\[N/A\])')
            safe=all(re.fullmatch(pattern,x) for x in rows)
            record(dict(query=kind,exit=r['exit'],stdout=text if safe else '<unrecognized response redacted>',
                        stdout_sha256=hashlib.sha256(raw).hexdigest(),bytes=r['bytes'],
                        overflow=r['overflow'],timeout=r['timeout'],unavailable=r['unavailable']))
            if r['unavailable'] or r['exit'] not in (0,) or r['timeout']:refuse('UNAVAILABLE')
            if r['overflow'] or not safe or (raw.strip() and not text):refuse('UNKNOWN')
            return rows
        begun=time.monotonic()
        inventory=read('inventory')
        if len(inventory)!=1:refuse('TARGET_AMBIGUOUS')
        index,uuid,driver,encoder=[x.strip() for x in inventory[0].split(',')]
        if index!='0' or uuid!=expected_uuid:refuse('TARGET_MISMATCH')
        if driver not in ('WDDM','TCC') or not encoder.isdecimal():refuse('WDDM_UNKNOWN' if driver=='WDDM' else 'UNKNOWN')
        if int(encoder)>100:refuse('UNKNOWN')
        if int(encoder)>10:refuse('ENCODER_BUSY')
        # WDDM/desktop residency is informational under the explicit 2026-11-19
        # operator policy. Never enumerate/whitelist foreign PIDs for admission.
        r=query(tool,'encoder_sessions',root,expected_uuid)
        raw=r['stdout']
        record(dict(query='encoder_sessions',exit=r['exit'],bytes=r['bytes'],
                    stdout_sha256=hashlib.sha256(raw).hexdigest(),overflow=r['overflow'],
                    timeout=r['timeout'],unavailable=r['unavailable']))
        if r['unavailable'] or r['timeout'] or r['exit']!=0:refuse('UNAVAILABLE')
        if r['overflow']:refuse('UNKNOWN')
        try:
            def unique(pairs):
                result={}
                for k,v in pairs:
                    if k in result:raise ValueError('duplicate')
                    result[k]=v
                return result
            sample=json.loads(raw,object_pairs_hook=unique)
            reason=encoder_admission(sample,expected_uuid,int(time.time()*1000))
        except (ValueError,TypeError,UnicodeDecodeError):refuse('UNKNOWN')
        if time.monotonic()-begun>5:refuse('STALE')
        if reason!='QUALIFIED':refuse(reason)
        if int(encoder)!=sample['encoder_percent']:refuse('UTILIZATION_DISAGREEMENT')
        result=dict(gpu_uuid=uuid,gpu_index=0,driver_model=driver,
                    encoder_percent=[int(encoder),sample['encoder_percent']],encoder_sessions=0,
                    desktop_rows='INFORMATIONAL_NOT_QUERIED',sample=sample)
        record(dict(decision='QUALIFIED',**result));return result


def encoder_admission(sample,expected_uuid,now_ms):
    fields={'uuid','start_ms','end_ms','session_count','enumerated_count','encoder_percent','sampling_period_us'}
    if type(sample) is not dict or set(sample)!=fields or sample['uuid']!=expected_uuid:return 'TARGET_MISMATCH'
    if any(type(sample[k]) is not int or sample[k]<0 for k in fields-{'uuid'}):return 'UNKNOWN'
    a,b=sample['start_ms'],sample['end_ms']
    # Fresh complete sample, bounded span; all three race guards recollect.
    if not 0<=a<=b<=now_ms or now_ms-a>5000:return 'STALE'
    if not 0<sample['sampling_period_us']<=5000000 or sample['encoder_percent']>100:return 'UNKNOWN'
    if sample['session_count']!=sample['enumerated_count']:return 'SESSION_DISAGREEMENT'
    if sample['session_count'] or sample['encoder_percent']>10:return 'ENCODER_BUSY'
    return 'QUALIFIED'
