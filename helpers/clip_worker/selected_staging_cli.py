"""Entrypoint for Owned::gated_manifest. No file access by this module before GO."""
import sys

def execute():
    if sys.stdin.buffer.readline(4)!=b'GO\n':return 20
    import ctypes, os
    from ctypes import wintypes
    kernel=ctypes.WinDLL('kernel32',use_last_error=True)
    enrolled=wintypes.BOOL()
    query=kernel.IsProcessInJob
    query.argtypes=(wintypes.HANDLE,wintypes.HANDLE,ctypes.POINTER(wintypes.BOOL))
    if not query(wintypes.HANDLE(-1),None,ctypes.byref(enrolled)) or not enrolled.value:return 21
    import hashlib,json,re,uuid,shutil,threading
    from pathlib import Path
    from contextlib import ExitStack
    import importlib.abc,importlib.util
    if len(sys.argv)!=2 or not sys.flags.isolated or not sys.flags.dont_write_bytecode:raise ValueError()
    def one(pairs):
        d=dict(pairs)
        if len(d)!=len(pairs):raise ValueError()
        return d
    def load(p,cap):
        with p.open('rb') as f:b=f.read(cap+1)
        if len(b)>cap:raise ValueError()
        return json.loads(b,object_pairs_hook=one)
    def safe(p):
        if not p.is_absolute() or p.drive.lower()!='c:' or p.resolve()!=p:raise ValueError()
        for part in (*p.parents,p):
            if part.exists() and part.lstat().st_file_attributes&0x400:raise ValueError()
        return p
    def sha(p):
        with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
    cfg=safe(Path(sys.argv[1]));cwd=safe(Path.cwd())
    if cfg!=cwd/'config.json':raise ValueError()
    c=load(cfg,65536)
    if set(c)!={'schema','mode','job_id','profile_id','account','video_id','allowed','excluded','tools','manifest','manifest_sha256','fixture'} or c['schema']!='selected-stage-launch-v1':raise ValueError()
    job_id=c['job_id']; mode=c['mode']
    if mode not in ('fixture','tiktok') or str(uuid.UUID(job_id))!=job_id:raise ValueError()
    worker=safe(Path(__file__)).parent
    root=safe(worker.parents[2]/('snapmatic-selected-fixtures-v1' if mode=='fixture' else 'snapmatic-selected-local-cache-v1'))
    if cwd!=root/('control-'+job_id):raise ValueError()
    m=safe(Path(c['manifest']))
    if m!=cwd/'manifest.json' or sha(m)!=c['manifest_sha256']:raise ValueError()
    manifest=load(m,8388608)
    if set(manifest)!={'schema','files'} or manifest['schema']!='native-runtime-files-v1':raise ValueError()
    files=manifest['files']
    def pinned(p):
        p=safe(p)
        if sha(p)!=files.get(str(p)):raise ValueError()
        return p
    pinned(Path(__file__));pinned(Path(sys.executable))
    modules=('selected_staging','local_producer','prepare','cut_gate','intake_proof','rubric_gate','export_profile','captions','gpu_guard','nvml_encoder_probe','worker_cancel','finalize')
    class WorkerImports(importlib.abc.MetaPathFinder):
        def find_spec(self,fullname,path,target=None):
            if fullname in modules:return importlib.util.spec_from_file_location(fullname,pinned(worker/(fullname+'.py')))
    sys.meta_path.insert(0,WorkerImports())
    import selected_staging as s,worker_cancel
    # Parent owns the process tree. Closing control also cancels bounded children.
    threading.Thread(target=worker_cancel.listen,args=(sys.stdin.buffer,),daemon=True).start()
    worker_cancel.checkpoint()
    if type(c['profile_id'])!=int or c['profile_id']<=0:raise ValueError()
    for ids in (c['allowed'],c['excluded']):
        if type(ids)!=list or len(ids)>1500 or any(type(i)!=str or not re.fullmatch(r'[A-Za-z0-9_-]{1,80}',i) for i in ids) or len(set(ids))!=len(ids):raise ValueError()
    if c['video_id'] not in c['allowed'] or set(c['allowed'])&set(c['excluded']):raise ValueError()
    tools=c['tools'];names=('ffmpeg','ffprobe') if mode=='fixture' else ('ffmpeg','ffprobe','downloader')
    if set(tools)!={k for n in names for k in (n,n+'_sha256')}:raise ValueError()
    for n in names:
        if files.get(str(pinned(Path(tools[n]))))!=tools[n+'_sha256']:raise ValueError()
    job=safe(root/job_id)
    s.command('unused',job,c['account'],c['video_id'])
    if mode=='tiktok':
        if c['fixture'] is not None:raise ValueError()
        s.run(job_id,c['profile_id'],c['account'],c['video_id'],c['allowed'],c['excluded'],tools)
    else:
        fixture=c['fixture']
        if type(fixture)!=dict or set(fixture)!={'path','sha256'}:raise ValueError()
        src=pinned(Path(fixture['path']))
        if src!=cwd/'fixture.mp4' or files[str(src)]!=fixture['sha256'] or not 0<src.stat().st_size<=s.LIMIT:raise ValueError()
        job.mkdir()
        try:
            s.state(job,1,'staging_fixture')
            with s.read_lease(src),src.open('rb') as inp,(job/'selected.mp4').open('xb') as out:
                shutil.copyfileobj(inp,out);out.flush();os.fsync(out.fileno())
            s.accept(root,job,dict(profile_id=c['profile_id'],platform='fixture',account=c['account'],video_id=c['video_id']),Path(tools['ffmpeg']),Path(tools['ffprobe']))
        except Exception:
            s.exclusive_json(job/'failure.json',dict(state='HOLD',reason='validation_failed'))
            raise
    return 0

if __name__=='__main__':
    try:code=execute()
    except Exception:code=22
    raise SystemExit(code)
