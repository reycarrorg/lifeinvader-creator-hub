"""APP-owned GO child; direct creator-authorized MP4 only. Never resolves platform playback URLs."""
import sys

def execute():
    if sys.stdin.buffer.readline(3)!=b'GO\n':return 20
    import ctypes
    from ctypes import wintypes
    member=wintypes.BOOL()
    q=ctypes.WinDLL('kernel32',use_last_error=True).IsProcessInJob
    q.argtypes=(wintypes.HANDLE,wintypes.HANDLE,ctypes.POINTER(wintypes.BOOL))
    if not q(wintypes.HANDLE(-1),None,ctypes.byref(member)) or not member.value:return 21
    import os,hashlib,json,uuid,time,re,threading,importlib.abc,importlib.util
    from pathlib import Path
    from urllib.parse import urlsplit
    def safe(p):
        if not p.is_absolute() or p.drive.lower()!='c:' or p.resolve()!=p:raise ValueError()
        for a in (*p.parents,p):
            if a.exists() and a.lstat().st_file_attributes&0x400:raise ValueError()
        return p
    def sha(p):
        with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
    def load(p,cap):
        def unique(items):
            d=dict(items)
            if len(d)!=len(items):raise ValueError()
            return d
        with p.open('rb') as f:b=f.read(cap+1)
        if len(b)>cap:raise ValueError()
        return json.loads(b,object_pairs_hook=unique)
    if len(sys.argv)!=2 or not sys.flags.isolated or not sys.flags.no_site or not sys.flags.dont_write_bytecode:raise ValueError()
    cwd=safe(Path.cwd());cfg=safe(Path(sys.argv[1]))
    if cfg!=cwd/'config.json':raise ValueError()
    c=load(cfg,16384)
    two_stage=c.get('schema') in ('selected-tiktok-flat-creator-v1','selected-tiktok-flat-fixture-v1')
    flat_fixture=c.get('schema')=='selected-tiktok-flat-fixture-v1'
    tls_fixture=c.get('schema')=='selected-direct-tls-fixture-v1'
    if set(c)-({'listing','listing_sha256'} if two_stage else set())-({'fixture_port','fixture_ca_sha256'} if tls_fixture else set())!={'schema','job_id','profile','video_id','permission','permission_sha256','manifest','manifest_sha256','tools','max_bytes'}:raise ValueError()
    fixture=c['schema']=='selected-direct-fixture-v1' or flat_fixture
    if not (two_stage or fixture or tls_fixture) and c['schema']!='selected-direct-creator-v1':raise ValueError()
    worker=safe(Path(__file__)).parent
    root=safe(worker.parents[2]/('snapmatic-direct-tls-fixtures-v1' if tls_fixture else 'snapmatic-tiktok-flat-fixtures-v1' if flat_fixture else 'snapmatic-tiktok-flat-local-v1' if two_stage else 'snapmatic-direct-fixtures-v1' if fixture else 'snapmatic-direct-local-v1'))
    jid=c['job_id']
    if str(uuid.UUID(jid))!=jid or cwd!=root/('control-'+jid):raise ValueError()
    mp=safe(Path(c['manifest']))
    if mp!=cwd/'manifest.json' or sha(mp)!=c['manifest_sha256']:raise ValueError()
    m=load(mp,8388608)
    if set(m)!={'schema','files'} or m['schema']!='native-runtime-files-v1':raise ValueError()
    def pin(p):
        if m['files'].get(str(safe(p)))!=sha(p):raise ValueError()
        return p
    pin(Path(__file__));pin(Path(sys.executable))
    class Finder(importlib.abc.MetaPathFinder):
        def find_spec(self,name,path,target=None):
            if name in ('tiktok_flat_stage','tiktok_window_policy','selected_staging','selected_sources_v2','local_producer','prepare','cut_gate','intake_proof','rubric_gate','export_profile','captions','gpu_guard','nvml_encoder_probe','worker_cancel','finalize'):
                return importlib.util.spec_from_file_location(name,pin(worker/(name+'.py')))
    sys.meta_path.insert(0,Finder())
    from selected_sources_v2 import source_identity
    import selected_staging as s,worker_cancel
    threading.Thread(target=worker_cancel.listen,args=(sys.stdin.buffer,),daemon=True).start()
    worker_cancel.checkpoint()
    pp=safe(Path(c['permission']))
    if pp!=cwd/'permission.json' or sha(pin(pp))!=c['permission_sha256']:raise ValueError()
    a=load(pp,4096)
    if set(a)!={'basis','job_id','profile','video_id','url','source_sha256','bytes','expires_at'}:raise ValueError()
    if a['basis']!=('snapmatic_owned_test' if tls_fixture or flat_fixture else 'creator_supplied_direct_mp4') or any(a[k]!=c[k] for k in ('job_id','profile','video_id')) or type(a['expires_at'])!=int or not 0<a['expires_at']-time.time()<=900:raise ValueError()
    provider,profile=source_identity(a['profile'])
    if profile!=a['profile'] or not re.fullmatch({'youtube':r'[A-Za-z0-9_-]{11}','tiktok':r'[0-9]{15,22}','instagram':r'[A-Za-z0-9_-]{1,40}'}[provider],a['video_id']):raise ValueError()
    if type(c['max_bytes'])!=int or type(a['bytes'])!=int or not 0<a['bytes']<=c['max_bytes']<=s.LIMIT or not re.fullmatch(r'[0-9a-f]{64}',a['source_sha256']):raise ValueError()
    url=a['url'];u=urlsplit(url)
    if not re.fullmatch(r'https://[a-z0-9.-]+/[A-Za-z0-9._~/-]+[.]mp4',url) or len(url)>2048 or not re.fullmatch(r'(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?[.])+[a-z]{2,63}',u.netloc) or any(v in ('.','..') for v in u.path.split('/')):raise ValueError()
    tools=c['tools']
    if set(tools)!={'ffmpeg','ffprobe','ffmpeg_sha256','ffprobe_sha256'}:raise ValueError()
    for n in ('ffmpeg','ffprobe'):
        if sha(pin(Path(tools[n])))!=tools[n+'_sha256']:raise ValueError()
    if two_stage:
        from tiktok_flat_stage import classify
        lp=safe(Path(c['listing']))
        if provider!='tiktok' or lp!=cwd/'flat.jsonl' or lp.stat().st_size>262144 or sha(pin(lp))!=c['listing_sha256']:raise ValueError()
        rows=classify(profile,lp.read_bytes())
        if not any(row['post_id']==a['video_id'] and row['state']=='REVIEWABLE' for row in rows):raise ValueError()
        import msvcrt
        # Kernel releases the process-lifetime lock even after a crash. The per-post
        # claim persists on failure: never infer permission to retry an unknown write.
        serial=open(safe(root/'sequential.lock'),'a+b')
        if not serial.tell():serial.write(b'0');serial.flush()
        serial.seek(0);msvcrt.locking(serial.fileno(),msvcrt.LK_NBLCK,1)
        postkey=hashlib.sha256((profile+'|'+a['video_id']).encode()).hexdigest()
        with safe(root/('claim-'+postkey)).open('xb') as f:
            f.write(jid.encode('ascii'));f.flush();os.fsync(f.fileno())
        time.sleep(2)
        worker_cancel.checkpoint()
        if time.time()>=a['expires_at']:raise ValueError()
    final=safe(root/jid);attempt=safe(root/('attempt-'+jid))
    if final.exists():raise ValueError()
    attempt.mkdir();job=attempt/jid;job.mkdir()
    try:
        s.state(job,1,'acquiring_authorized_file')
        deadline=time.monotonic()+min(90,a['expires_at']-time.time())
        if fixture:
            from contextlib import closing
            fp=pin(cwd/'fixture.mp4')
            if u.netloc!='fixture.invalid':raise ValueError()
            with fp.open('rb') as f:stream(f,{'content-length':[str(fp.stat().st_size)],'content-type':['video/mp4']},200,job/'selected.mp4',a,deadline,worker_cancel.checkpoint)
        elif tls_fixture:
            import ssl,socket
            ca=pin(cwd/'fixture-ca.pem')
            port=c['fixture_port']
            if sha(ca)!=c['fixture_ca_sha256'] or type(port)!=int or not 1024<=port<=65535 or u.netloc!='fixture.invalid' or u.path!='/clip.mp4':raise ValueError()
            ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT);ctx.load_verify_locations(cafile=str(ca))
            transfer(u,job/'selected.mp4',a,deadline,worker_cancel.checkpoint,ctx,(socket.AF_INET,socket.SOCK_STREAM,0,'',('127.0.0.1',port)))
        else:fetch(u,job/'selected.mp4',a,deadline,worker_cancel.checkpoint)
        identity=dict(provider=provider,source=profile,id=a['video_id'],policy=a['basis'],permission_sha256=c['permission_sha256'])
        rec=s.accept(root,job,identity,Path(tools['ffmpeg']),Path(tools['ffprobe']))
        if rec['object_sha256']!=a['source_sha256']:raise ValueError()
        s.exclusive_json(job/'archive.json',dict(schema='selected-source-archive-v2',identity={k:identity[k] for k in ('provider','source','id')},object_sha256=rec['object_sha256'],manifest_sha256=rec['manifest_sha256'],stage_receipt_sha256=sha(job/'receipt.json'),publication_enabled=False))
        os.rename(job,final)
    except Exception:
        s.exclusive_json(job/'failure.json',dict(state='HOLD',reason='authorized_transfer_or_validation_failed'))
        raise
    return 0

def stream(reader,headers,status,path,auth,deadline,checkpoint):
    import time,hashlib,os
    n=auth['bytes']
    if status!=200 or headers.get('content-length')!=[str(n)] or headers.get('content-type')!=['video/mp4'] or any(k in headers for k in ('transfer-encoding','content-encoding','location','content-range')):raise ValueError()
    h=hashlib.sha256()
    with path.open('xb') as f:
        left=n
        while left:
            checkpoint()
            if time.monotonic()>=deadline:raise TimeoutError()
            b=reader.read(min(left,65536))
            if not b or len(b)>left:raise ValueError()
            left-=len(b);h.update(b);f.write(b)
        if time.monotonic()>=deadline or h.hexdigest()!=auth['source_sha256']:raise ValueError()
        f.flush();os.fsync(f.fileno())

def fetch(url,path,auth,deadline,checkpoint):
    import socket,ssl,ipaddress,http.client,time,threading
    def remaining():
        checkpoint()
        n=deadline-time.monotonic()
        if n<=0:raise TimeoutError()
        return min(n,10)
    host=url.hostname
    answers=socket.getaddrinfo(host,443,type=socket.SOCK_STREAM)
    if not answers:raise ValueError()
    for _,_,_,_,addr in answers:
        ip=ipaddress.ip_address(addr[0])
        if not ip.is_global or ip.is_multicast or ip.is_reserved or getattr(ip,'ipv4_mapped',None) or getattr(ip,'sixtofour',None) or getattr(ip,'teredo',None):raise ValueError()
    transfer(url,path,auth,deadline,checkpoint,ssl.create_default_context(),answers[0])

def transfer(url,path,auth,deadline,checkpoint,ctx,endpoint):
    import socket,http.client,time,threading
    host=url.hostname
    def remaining():
        checkpoint()
        n=deadline-time.monotonic()
        if n<=0:raise TimeoutError()
        return min(n,10)
    family,kind,proto,_,addr=endpoint
    with socket.socket(family,kind,proto) as sock:
        sock.settimeout(remaining());sock.connect(addr)
        with ctx.wrap_socket(sock,server_hostname=host) as tls:
            def expire():
                try:tls.shutdown(socket.SHUT_RDWR)
                except OSError:pass
            timer=threading.Timer(max(0,deadline-time.monotonic()),expire);timer.daemon=True;timer.start()
            try:
                tls.settimeout(remaining())
                tls.sendall(f'GET {url.path} HTTP/1.1\r\nHost: {host}\r\nAccept: video/mp4\r\nConnection: close\r\n\r\n'.encode('ascii'))
                with http.client.HTTPResponse(tls) as resp:
                    resp.begin()
                    headers={k.lower():resp.headers.get_all(k) for k in resp.headers}
                    stream(resp,headers,resp.status,path,auth,deadline,checkpoint)
            finally:timer.cancel()

if __name__=='__main__':
    try:code=execute()
    except Exception:code=22
    raise SystemExit(code)
