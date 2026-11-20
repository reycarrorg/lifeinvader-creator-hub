"""Owned::gated_manifest entry point. No config, workspace, or worker imports before GO/enrollment."""
import sys

def main():
    if sys.stdin.buffer.readline(3)!=b'GO\n':return 20
    import ctypes
    from ctypes import wintypes
    member=wintypes.BOOL()
    query=ctypes.WinDLL('kernel32',use_last_error=True).IsProcessInJob
    query.argtypes=(wintypes.HANDLE,wintypes.HANDLE,ctypes.POINTER(wintypes.BOOL))
    if not query(wintypes.HANDLE(-1),None,ctypes.byref(member)) or not member.value:return 21
    import os,json,hashlib,re,uuid,threading,importlib.util,importlib.abc
    from pathlib import Path
    from contextlib import ExitStack
    if len(sys.argv)!=2 or not sys.flags.isolated or not sys.flags.no_site or not sys.flags.dont_write_bytecode:raise ValueError()
    def safe(p):
        if not p.is_absolute() or p.drive.lower()!='c:' or p.resolve()!=p:raise ValueError()
        for part in (*p.parents,p):
            if part.exists() and part.lstat().st_file_attributes&0x400:raise ValueError()
        return p
    def sha(p):
        with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
    def pairs(values):
        d=dict(values)
        if len(d)!=len(values):raise ValueError()
        return d
    def load(p,size):
        with p.open('rb') as f:raw=f.read(size+1)
        if len(raw)>size:raise ValueError()
        return json.loads(raw,object_pairs_hook=pairs)
    cwd=safe(Path.cwd());cp=safe(Path(sys.argv[1]))
    if cp!=cwd/'config.json':raise ValueError()
    c=load(cp,16384)
    if set(c)!={'schema','job_id','profile','video_id','source_sha256','source_bytes','max_bytes','manifest','manifest_sha256','tools'}:raise ValueError()
    roots={'selected-ingest-fixture-go-v2':'snapmatic-selected-fixtures-v2','selected-ingest-local-authorized-v2':'snapmatic-selected-imports-v2'}
    schema=c['schema'];jid=c['job_id']
    if schema not in roots or str(uuid.UUID(jid))!=jid:raise ValueError()
    worker=safe(Path(__file__)).parent;root=safe(worker.parents[2]/roots[schema])
    if cwd!=root/('control-'+jid):raise ValueError()
    mp=safe(Path(c['manifest']))
    if mp!=cwd/'manifest.json' or sha(mp)!=c['manifest_sha256']:raise ValueError()
    m=load(mp,8388608)
    if set(m)!={'schema','files'} or m['schema']!='native-runtime-files-v1':raise ValueError()
    def pin(p):
        p=safe(p)
        if m['files'].get(str(p))!=sha(p):raise ValueError()
        return p
    pin(Path(__file__));pin(Path(sys.executable))
    modules={'selected_staging','selected_sources_v2','local_producer','prepare','cut_gate','intake_proof','rubric_gate','export_profile','captions','gpu_guard','nvml_encoder_probe','worker_cancel','finalize'}
    class LocalImports(importlib.abc.MetaPathFinder):
        def find_spec(self,name,path,target=None):
            if name in modules:return importlib.util.spec_from_file_location(name,pin(worker/(name+'.py')))
    sys.meta_path.insert(0,LocalImports())
    from selected_sources_v2 import source_identity
    import selected_staging as s,worker_cancel
    threading.Thread(target=worker_cancel.listen,args=(sys.stdin.buffer,),daemon=True).start()
    worker_cancel.checkpoint()
    standalone=schema=='selected-ingest-local-authorized-v2'
    if standalone:
        if c['profile'] is not None or c['video_id'] is not None:raise ValueError()
        identity=dict(kind='local',source_sha256=c['source_sha256'])
    else:
        provider,profile=source_identity(c['profile'])
        if not re.fullmatch({'youtube':r'[A-Za-z0-9_-]{11}','instagram':r'[A-Za-z0-9_-]{1,40}','tiktok':r'[0-9]{15,22}'}[provider],c['video_id']):raise ValueError()
        identity=dict(provider=provider,source=profile,id=c['video_id'])
    tools=c['tools']
    if set(tools)!={'ffmpeg','ffprobe','ffmpeg_sha256','ffprobe_sha256'}:raise ValueError()
    for n in ('ffmpeg','ffprobe'):
        if m['files'].get(str(pin(Path(tools[n]))))!=tools[n+'_sha256']:raise ValueError()
    src=safe(cwd/('fixture.mp4' if schema=='selected-ingest-fixture-go-v2' else 'accepted.mp4'))
    if type(c['source_bytes'])!=int or type(c['max_bytes'])!=int or not 0<c['source_bytes']<=c['max_bytes']<=s.LIMIT or src.stat().st_size!=c['source_bytes'] or src.stat().st_nlink!=1:raise ValueError()
    if m['files'].get(str(pin(src)))!=c['source_sha256']:raise ValueError()
    final=safe(root/jid)
    if final.exists():raise ValueError()
    pending=safe(root/('attempt-'+jid));pending.mkdir()
    job=safe(pending/jid);job.mkdir()
    try:
        s.state(job,1,'staging_local')
        with s.read_lease(src),src.open('rb') as inp,(job/'selected.mp4').open('xb') as out:
            remaining=c['source_bytes']
            while remaining:
                worker_cancel.checkpoint()
                chunk=inp.read(min(65536,remaining))
                if not chunk:raise ValueError()
                out.write(chunk);remaining-=len(chunk)
            if inp.read(1):raise ValueError()
            out.flush();os.fsync(out.fileno())
        if sha(job/'selected.mp4')!=c['source_sha256']:raise ValueError()
        receipt=s.accept(root,job,identity,Path(tools['ffmpeg']),Path(tools['ffprobe']))
        if receipt['object_sha256']!=c['source_sha256']:raise ValueError()
        if not standalone:s.exclusive_json(job/'archive.json',dict(schema='selected-source-archive-v2',identity=identity,object_sha256=receipt['object_sha256'],manifest_sha256=receipt['manifest_sha256'],stage_receipt_sha256=sha(job/'receipt.json'),publication_enabled=False))
    except Exception:
        s.exclusive_json(job/'failure.json',dict(state='HOLD',reason='local_validation_failed'))
        raise
    os.rename(job,final)  # Atomic same-volume publication, no replacement on Windows.
    return 0

if __name__=='__main__':
    try:code=main()
    except Exception:code=22
    raise SystemExit(code)
