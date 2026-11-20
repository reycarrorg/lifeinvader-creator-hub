"""Single selected TikTok video. APP supplies the current server-side selection and owns the child Job Object."""
import hashlib,json,os,re,subprocess,time,uuid
from pathlib import Path
from local_producer import bounded, read_lease
from prepare import digest

ROOT=Path(__file__).resolve().parents[3]/'snapmatic-selected-local-cache-v1'
LIMIT=256*1024*1024

def safe(path):
    if not path.is_absolute() or path.drive.lower()!='c:' or path.resolve()!=path:raise ValueError('staging_path')
    for parent in (*path.parents,path):
        if parent.exists() and parent.stat(follow_symlinks=False).st_file_attributes & 0x400:raise ValueError('staging_reparse')
    return path

def exclusive_json(path,value):
    # Same-directory Windows rename publishes complete bytes and refuses replacement.
    if os.name!='nt':raise RuntimeError('windows_required')
    path=safe(Path(path));pending=safe(path.with_name(path.name+'.pending'))
    if path.exists():raise FileExistsError(path.name)
    raw=json.dumps(value,sort_keys=True,separators=(',',':'),allow_nan=False).encode('utf8')
    if len(raw)>1048576:raise ValueError('evidence_size')
    with pending.open('xb') as f:
        f.write(raw);f.flush();os.fsync(f.fileno())
    os.rename(pending,path)

def commit_archive(job,identity,source_sha,expected):
    receipt=safe(job/'receipt.json')
    with read_lease(receipt):
        raw=receipt.read_bytes()
        if len(raw)>1048576 or json.loads(raw)!=expected or expected.get('schema')!='selected-local-stage-v1' or expected.get('job_id')!=job.name or expected.get('selection')!=identity or expected.get('object_sha256')!=source_sha or expected.get('publication_enabled') is not False:
            raise ValueError('receipt_binding')
        exclusive_json(job/'archive.json',dict(schema='selected-source-archive-v2',identity={k:identity[k] for k in ('provider','source','id')},object_sha256=source_sha,manifest_sha256=expected['manifest_sha256'],stage_receipt_sha256=hashlib.sha256(raw).hexdigest(),publication_enabled=False))

def state(job,seq,label):
    exclusive_json(job/f'{seq:02d}-{label}.json',dict(seq=seq,state=label))

def command(tool,job,account,video_id):
    if not re.fullmatch(r'[A-Za-z0-9_.]{1,24}',account) or not re.fullmatch(r'[0-9]{15,22}',video_id):raise ValueError('selection_identity')
    return [str(tool),'--ignore-config','--no-plugin-dirs','--no-cache-dir','--no-playlist','--no-overwrites','--no-progress',
            '--no-warnings','--quiet','--no-simulate','--retries','0','--fragment-retries','0','--extractor-retries','0',
            '--socket-timeout','12','--max-filesize',str(LIMIT),'--fixup','never','--downloader','native',
            '-f','best[ext=mp4][protocol=https][vcodec!=none][acodec!=none]','-o',str(job/'selected.mp4'),
            '--print','after_move:%(id)s','--','https://www.tiktok.com/@'+account+'/video/'+video_id]

def accept(root,job,identity,ff,fp):
    src=safe(job/'selected.mp4')
    if not 0<src.stat().st_size<=LIMIT:raise ValueError('media_size')
    def probe(p):
        return json.loads(bounded([str(fp),'-v','error','-protocol_whitelist','file,pipe','-show_streams','-show_format','-of','json',str(p)],job,15))
    with read_lease(src):
        h=digest(src);size=src.stat().st_size
        data=probe(src);vs=[s for s in data['streams'] if s['codec_type']=='video' and not s.get('disposition',{}).get('attached_pic')]
        if len(vs)!=1 or not any(s['codec_type']=='audio' for s in data['streams']) or not 0<float(data['format']['duration'])<=900 or not 0<vs[0]['width']*vs[0]['height']<=16777216 or 'mp4' not in data['format']['format_name'].split(','):raise ValueError('media_layout')
        bounded([str(ff),'-nostdin','-v','error','-xerror','-protocol_whitelist','file,pipe','-i',str(src),'-map','0:v:0','-map','0:a:0','-f','null','-'],job,120,stdout=subprocess.DEVNULL)
        state(job,2,'decoded')
        cache=safe(root/'objects');cache.mkdir(exist_ok=True);obj=safe(cache/h)
        if not obj.exists():
            candidate=job/'candidate';candidate.mkdir()
            with (candidate/'media.mp4').open('xb') as out,src.open('rb') as inp:
                import shutil
                shutil.copyfileobj(inp,out);out.flush();os.fsync(out.fileno())
            poster=candidate/'poster.webp'
            bounded([str(ff),'-nostdin','-v','error','-xerror','-protocol_whitelist','file,pipe','-i',str(src),'-map','0:v:0','-frames:v','1','-vf','scale=540:960:force_original_aspect_ratio=decrease,pad=540:960:(ow-iw)/2:(oh-ih)/2,setsar=1','-an','-c:v','libwebp','-n',str(poster)],job,30)
            with poster.open('r+b') as f:os.fsync(f.fileno())
            info=probe(poster)['streams']
            if len(info)!=1 or (info[0]['codec_name'],info[0]['width'],info[0]['height'])!=('webp',540,960) or not 0<poster.stat().st_size<=2097152:raise ValueError('poster_validation')
            if digest(candidate/'media.mp4')!=h:raise ValueError('copy_changed')
            exclusive_json(candidate/'manifest.json',dict(schema='selected-local-object-v1',sha256=h,bytes=size,poster_sha256=digest(poster),poster_bytes=poster.stat().st_size))
            try:os.rename(candidate,obj)
            except FileExistsError:pass
        manifest=safe(obj/'manifest.json');record=json.loads(manifest.read_bytes())
        media=safe(obj/'media.mp4');poster=safe(obj/'poster.webp')
        if record!=dict(schema='selected-local-object-v1',sha256=h,bytes=size,poster_sha256=digest(poster),poster_bytes=poster.stat().st_size) or media.stat().st_size!=size or digest(media)!=h:raise ValueError('object_conflict')
        result=dict(schema='selected-local-stage-v1',job_id=job.name,selection=identity,object_sha256=h,manifest_sha256=digest(manifest),publication_enabled=False)
        exclusive_json(job/'receipt.json',result);return result

def run(job_id,profile_id,account,video_id,allowed,excluded,tools):
    if str(uuid.UUID(job_id))!=job_id or type(profile_id)!=int or profile_id<=0 or video_id not in allowed or video_id in excluded:raise ValueError('selection_not_admitted')
    root=safe(ROOT);job=safe(root/job_id)
    paths={name:safe(Path(tools[name])) for name in ('downloader','ffmpeg','ffprobe')}
    from contextlib import ExitStack
    with ExitStack() as leases:
        for name,path in paths.items():
            leases.enter_context(read_lease(path))
            if digest(path)!=tools[name+'_sha256']:raise ValueError('tool_pin')
        args=command(paths['downloader'],job,account,video_id)
        root.mkdir(exist_ok=True);job.mkdir()
        state(job,1,'downloading')
        category='downloader_failed'
        try:
            raw=bounded(args,job,180)
            if raw.strip()!=video_id.encode():raise ValueError('selected_id_mismatch')
            category='validation_failed'
            return accept(root,job,dict(profile_id=profile_id,platform='tiktok',account=account,video_id=video_id),paths['ffmpeg'],paths['ffprobe'])
        except Exception:
            exclusive_json(job/'failure.json',dict(state='HOLD',reason=category))
            raise RuntimeError(category) from None
