"""Bounded acceptance driver for an externally approved exact packet.
Normal app endpoints only. One render POST, then at most one read-only restart.
No browser, producer replacement, credential, GPU polling loop or retries.
"""
import ctypes,datetime,hashlib,http.client,json,os,re,sqlite3,stat,subprocess,sys,time
from pathlib import Path

def sha(p):
    with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()

def sanitized_http_error(status,data):
    allowed={'source_unavailable','source_changed','worker_pin_unavailable','worker_pin_mismatch',
             'fresh_worker_workspace_required','isolated_controller_db_required','runtime_config_binding',
             'runtime_config_schema','isolated_runtime_admission_denied','manual_admission_denied',
             'stale_revision','supervisor_start_failed','review_ui_not_ready','native_manual_unavailable',
             'native_attempt_already_consumed','native_session_closing','reviewed_job_mismatch',
             'storage_unavailable','admission_reconciliation_required','same_origin_required'}
    out=dict(status=status,reason='UNCLASSIFIED',captured_bytes=len(data),
             captured_sha256=hashlib.sha256(data).hexdigest(),truncated=len(data)>4096)
    try:
        if len(data)>4096:return out
        body=json.loads(data)
        if type(body) is dict and body.get('reason') in allowed:out['reason']=body['reason']
    except (ValueError,TypeError,UnicodeDecodeError):pass
    return out

def listener_pid(port):
    # Windows documented TCP_TABLE_OWNER_PID_LISTENER; no process arguments read.
    dll=ctypes.WinDLL('iphlpapi');f=dll.GetExtendedTcpTable
    f.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_ulong),ctypes.c_bool,ctypes.c_ulong,ctypes.c_int,ctypes.c_ulong];f.restype=ctypes.c_ulong
    size=ctypes.c_ulong(65536);buf=ctypes.create_string_buffer(size.value)
    if f(buf,ctypes.byref(size),False,2,3,0)!=0:raise RuntimeError('listener identity unavailable')
    words=ctypes.cast(buf,ctypes.POINTER(ctypes.c_ulong));count=words[0]
    if count>(size.value-4)//24:raise RuntimeError('listener table invalid')
    found=[]
    for i in range(count):
        row=[words[1+i*6+k] for k in range(6)]
        actual=((row[2]&255)<<8)|((row[2]>>8)&255)
        if actual==port:
            if row[1]!=0x0100007f:raise RuntimeError('non-loopback listener')
            found.append(row[5])
    return found

def descendants(root_pid):
    from ctypes import wintypes as W
    class Entry(ctypes.Structure):
        _fields_=[('size',W.DWORD),('usage',W.DWORD),('pid',W.DWORD),('heap',ctypes.c_size_t),('module',W.DWORD),('threads',W.DWORD),('parent',W.DWORD),('priority',W.LONG),('flags',W.DWORD),('exe',W.WCHAR*260)]
    k=ctypes.WinDLL('kernel32',use_last_error=True)
    snap=k.CreateToolhelp32Snapshot;snap.argtypes=[W.DWORD,W.DWORD];snap.restype=W.HANDLE
    close=k.CloseHandle;close.argtypes=[W.HANDLE];close.restype=W.BOOL
    first=k.Process32FirstW;next_=k.Process32NextW
    for f in (first,next_):f.argtypes=[W.HANDLE,ctypes.POINTER(Entry)];f.restype=W.BOOL
    h=snap(2,0)
    if h==ctypes.c_void_p(-1).value:raise RuntimeError('process snapshot unavailable')
    try:
        pairs=[];e=Entry();e.size=ctypes.sizeof(e);ok=first(h,ctypes.byref(e))
        if not ok:raise RuntimeError('process snapshot empty')
        while ok:pairs.append((e.pid,e.parent));ok=next_(h,ctypes.byref(e))
        if ctypes.get_last_error()!=18:raise RuntimeError('process snapshot incomplete')
        owned={root_pid}
        for _ in range(len(pairs)):
            found={pid for pid,parent in pairs if parent in owned};new=owned|found
            if new==owned:break
            owned=new
        return sorted(pid for pid,_ in pairs if pid in owned and pid!=root_pid)
    finally:close(h)

def create_attempt_evidence(root, attempt):
    if not re.fullmatch(r'attempt-[0-9]{2}',attempt):raise ValueError('exact attempt name required')
    parent=Path(root)/'preflight'
    for p in (parent,*parent.parents):
        s=p.lstat()
        if stat.S_ISLNK(s.st_mode) or getattr(s,'st_file_attributes',0)&0x400:
            raise RuntimeError('reparse evidence path')
        if not stat.S_ISDIR(s.st_mode):raise RuntimeError('evidence parent not directory')
    target=parent/attempt
    target.mkdir(exist_ok=False)
    s=target.lstat()
    if stat.S_ISLNK(s.st_mode) or getattr(s,'st_file_attributes',0)&0x400:
        raise RuntimeError('reparse evidence path')
    return target

def run(packet_path,pin,attempt=None):
    packet_path=Path(packet_path).resolve(strict=True)
    if sha(packet_path)!=pin:raise RuntimeError('packet pin')
    packet=json.loads(Path(packet_path).read_bytes());root=Path(packet_path).parent.parent
    evidence=create_attempt_evidence(root,attempt) if attempt is not None else root/'preflight'
    log=(evidence/'headless-run.jsonl').open('x',encoding='utf-8')
    def record(event,**values):
        row=dict(event=event,utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),**values)
        log.write(json.dumps(row)+'\n');log.flush();print(json.dumps(row),flush=True)
    exe=Path(packet['exe']);plan_path=Path(packet['plan']);port=packet['port'];plan=json.loads(plan_path.read_bytes())
    if sha(exe)!=packet['exe_sha256'] or sha(plan_path)!=packet['plan_sha256']:raise RuntimeError('launch pin')
    if time.time()>=packet['expires_at'] or packet['launch_mode']!='explicit-headless-api':raise RuntimeError('expired or wrong mode')
    for leaf,pinned in packet['config_sha256'].items():
        if sha(root/'config'/leaf)!=pinned:raise RuntimeError('config pin')
    worker=root/'worker';dbpath=root/'controller.sqlite';job=packet['request']['job_id'];key=packet['request']['selection']['durable_cut_key']
    if any(worker.iterdir()) or listener_pid(port):raise RuntimeError('nonempty workspace or occupied port')
    with sqlite3.connect(dbpath.as_uri()+'?mode=ro',uri=True) as db:
        if db.execute('SELECT count(*) FROM controller_admissions').fetchone()[0]!=0:raise RuntimeError('existing admission')
    # Exact module pins are verified before importing this otherwise inert guard.
    manifest=json.loads((root/'config/runtime.json').read_bytes())['files']
    module_root=Path(packet['worker_module_root']) if 'worker_module_root' in packet else Path(__file__).resolve().parents[1]/'helpers/clip_worker'
    for leaf in ('gpu_guard.py','nvml_encoder_probe.py'):
        p=module_root/leaf
        if manifest.get(str(p))!=sha(p):raise RuntimeError('GPU helper pin')
    sys.path.insert(0,str(module_root));import gpu_guard
    producer=json.loads((root/'config/producer.json').read_bytes())
    if sha(producer['nvidia_smi'])!=producer['nvidia_smi_sha256']:raise RuntimeError('GPU tool pin')
    gpu_guard.guard(producer['nvidia_smi'],packet['gpu_uuid'],evidence,'preclaim')
    record('GPU_PREFLIGHT_QUALIFIED')
    env=dict(os.environ,LIFEINVADER_DB=str(dbpath),LIFEINVADER_MEDIA=str(root/'media'),LIFEINVADER_PORT=str(port))
    env.pop('PYTHONPATH',None)
    def launch(number):
        if sha(exe)!=packet['exe_sha256']:raise RuntimeError('app pin changed')
        out=(evidence/f'app-{number}.stdout').open('xb');err=(evidence/f'app-{number}.stderr').open('xb')
        args=[str(exe),'--server-only','--headless-review','--isolated-review-plan',str(plan_path),'--isolated-review-sha256',packet['plan_sha256']]
        if 'authorized_catalog' in packet:
            if sha(packet['authorized_catalog'])!=packet['authorized_catalog_sha256']:raise RuntimeError('catalog pin changed')
            args+=['--authorized-source-catalog',packet['authorized_catalog'],'--authorized-source-catalog-sha256',packet['authorized_catalog_sha256']]
        child=subprocess.Popen(args,cwd=root,env=env,stdin=subprocess.DEVNULL,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
        out.close();err.close();record('APP_STARTED',number=number,pid=child.pid)
        return child
    def request(child,path,body=None,binary=False):
        if child.poll() is not None or listener_pid(port)!=[child.pid]:raise RuntimeError('exact listener/child mismatch')
        client=http.client.HTTPConnection('127.0.0.1',port,timeout=5)
        try:
            raw=None if body is None else json.dumps(body).encode()
            client.request('GET' if body is None else 'POST',path,body=raw,headers={'Origin':f'http://127.0.0.1:{port}','Content-Type':'application/json'})
            res=client.getresponse();data=res.read(128*1024*1024+1 if res.status in (200,202) else 4097)
            if res.status not in (200,202):
                stage='RENDER_ADMISSION' if path.endswith('/render-reviewed') else 'LIFETIME' if path.endswith('/ui-lifetime') else 'CONTROLLER_API'
                record('HTTP_REJECTED',stage=stage,**sanitized_http_error(res.status,data))
                raise RuntimeError('HTTP status '+str(res.status))
            if len(data)>128*1024*1024:raise RuntimeError('HTTP size limit')
            return data if binary else json.loads(data)
        finally:client.close()
    def ready(child):
        end=time.monotonic()+45
        while time.monotonic()<end:
            if child.poll() is not None:raise RuntimeError('app exited before ready')
            ids=listener_pid(port)
            if ids:
                if ids!=[child.pid]:raise RuntimeError('foreign listener')
                view=request(child,'/api/controller')
                if 'authorized_catalog' in packet:
                    catalog=request(child,'/api/controller/authorized-sources')
                    if not catalog['configured'] or len(catalog['sources'])!=1 or catalog['sources'][0]['source_sha256']!=packet['request']['selection']['source_sha256']:raise RuntimeError('catalog readiness mismatch')
                if view['publishing_connected'] or not view['manual_native_enabled'] or view['total_jobs']!=1:raise RuntimeError('headless scope')
                request(child,'/api/controller/ui-lifetime',{'action':'ready'})
                return view
            time.sleep(.2)
        raise RuntimeError('readiness timeout')
    def close(child):
        if child.poll() is None:request(child,'/api/controller/ui-lifetime',{'action':'close'})
        code=child.wait(timeout=25);owned=descendants(child.pid)
        record('APP_EXIT',pid=child.pid,exit=code,owned_descendants=owned)
        if code or owned:raise RuntimeError('unclean app exit')
    child=None;completed=None
    try:
        child=launch(1);view=ready(child)
        if time.time()>=packet['expires_at']:raise RuntimeError('expired before admission')
        request(child,'/api/controller/settings',{'action':'resume','revision':view['settings']['revision']})
        response=request(child,f'/api/controller/jobs/{job}/render-reviewed',{'revision':0})
        record('ONE_RENDER_SUBMITTED',pid=child.pid,response=response)
        end=time.monotonic()+210
        while time.monotonic()<end:
            request(child,'/api/controller/ui-lifetime',{'action':'ready'})
            state=request(child,f'/api/controller/jobs/{job}')
            if state['state']=='READY_FOR_REVIEW':completed=state;break
            if state['state'] not in ('PROCESSING',):raise RuntimeError('job '+state['state'])
            time.sleep(2)
        if completed is None:raise RuntimeError('completion timeout')
        receipt=worker/f'{key}.receipt.json';media=worker/f'{key}.mp4'
        receipt_sha=sha(receipt);media_sha=sha(media)
        data=request(child,f'/api/controller/jobs/{job}/preview',binary=True)
        if hashlib.sha256(data).hexdigest()!=media_sha:raise RuntimeError('preview hash')
        with sqlite3.connect((worker/'queue.sqlite').as_uri()+'?mode=ro',uri=True) as db:
            claims=db.execute("SELECT count(*) FROM events WHERE event='claimed'").fetchone()[0]
            if claims!=1 or db.execute('SELECT count(*) FROM completion_proofs').fetchone()[0]!=1:raise RuntimeError('worker proof count')
        record('VERIFIED_PREVIEW_GET',receipt_sha256=receipt_sha,media_sha256=media_sha,bytes=len(data),claim_count=claims,visual_playback_observed=False)
        close(child);child=None
        child=launch(2);ready(child)
        state=request(child,f'/api/controller/jobs/{job}')
        data=request(child,f'/api/controller/jobs/{job}/preview',binary=True)
        if state['state']!='READY_FOR_REVIEW' or hashlib.sha256(data).hexdigest()!=media_sha or sha(receipt)!=receipt_sha or sha(media)!=media_sha:raise RuntimeError('reopen identity')
        with sqlite3.connect((worker/'queue.sqlite').as_uri()+'?mode=ro',uri=True) as db:
            if db.execute("SELECT count(*) FROM events WHERE event='claimed'").fetchone()[0]!=1:raise RuntimeError('new claim on reopen')
        record('READONLY_REOPEN_VERIFIED',receipt_sha256=receipt_sha,media_sha256=media_sha)
        close(child);child=None;record('PASS')
    finally:
        if child is not None:
            try:close(child)
            except Exception:record('CLEANUP_UNPROVEN',pid=child.pid,exit=child.poll())
        log.close()

if __name__=='__main__':
    if len(sys.argv)==4:run(sys.argv[1],sys.argv[2],sys.argv[3]);raise SystemExit(0)
    if len(sys.argv)!=3:raise SystemExit('exact packet path and independent digest required')
    run(sys.argv[1],sys.argv[2])
