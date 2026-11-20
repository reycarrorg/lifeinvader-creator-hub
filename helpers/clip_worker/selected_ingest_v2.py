"""Offline qualification of v2 discovery and bounded ingest. No HTTP opener or live mode."""
import hashlib,json,os,re,sys,time,uuid
from pathlib import Path
from urllib.parse import urlsplit
from selected_sources_v2 import source_identity,candidate
from selected_staging import safe,state,accept,LIMIT
from local_producer import exclusive_json
from prepare import digest

VERSION='selected-ingest-fixture-v2'
WHEEL='a05c5b5d8562f7e8a62f5334348b406af4930d56d3a65f8021d14af0e22339ae'
PYTHON='0b471133e110cfb53a061cad528ce8e517d7b9ac41a0a396c39ad795a487fc14'
ROOT=Path(__file__).resolve().parents[3]/'snapmatic-selected-fixtures-v2'

def map_listing(profile,entry):
    provider,source=source_identity(profile)
    vid=entry.get('id')
    if type(vid)!=str:raise ValueError('candidate_id')
    result={'id':vid,'availability':entry.get('availability'),'has_drm':entry.get('has_drm')}
    # Owner URL and item URL must independently match the correct extractor and canonical route.
    ex=entry.get('extractor_key') or entry.get('ie_key')
    owner=entry.get('channel_url') or entry.get('uploader_url')
    if ex=={'youtube':'Youtube','tiktok':'TikTok','instagram':'Instagram'}[provider] and isinstance(owner,str):
        try:match=source_identity(owner)==(provider,source)
        except ValueError:match=False
        if match:result['source']=source
    url=entry.get('webpage_url')
    routes={'tiktok':source+'/video/'+vid,'instagram':'https://www.instagram.com/reel/'+vid,'youtube':'https://www.youtube.com/shorts/'+vid}
    if isinstance(url,str) and url.rstrip('/')==routes[provider]:
        result['kind']={'tiktok':'video','youtube':'short','instagram':'reel'}[provider]
    if entry.get('live_status') not in ('not_live','was_live'):result.pop('kind',None)
    if entry.get('is_live') is True:result.pop('kind',None)
    return candidate((provider,source),result)

def capture(reader,headers,status,redirected,path,limit,deadline):
    # reader is parent-owned. A blocking read also requires a hard outer Job deadline.
    n=headers.get('content-length','')
    if status!=200 or redirected or headers.get('content-encoding','identity')!='identity' or not re.fullmatch(r'[1-9][0-9]{0,9}',n):raise ValueError('transport_unbounded')
    length=int(n)
    if not 0<length<=limit<=LIMIT:raise ValueError('byte_limit')
    with path.open('xb') as f:
        remaining=length
        while remaining:
            if time.monotonic()>=deadline:raise ValueError('deadline')
            chunk=reader.read(min(65536,remaining))
            if not chunk or len(chunk)>remaining:raise ValueError('length_mismatch')
            f.write(chunk);remaining-=len(chunk)
        if reader.read(1) or time.monotonic()>=deadline:raise ValueError('length_or_deadline')
        f.flush();os.fsync(f.fileno())

def fixture(config,control):
    if set(config)!={'schema','batch_id','profile','entries','approved_ids','max_bytes','tools'} or config['schema']!=VERSION:raise ValueError('config')
    bid=config['batch_id']
    if str(uuid.UUID(bid))!=bid or safe(control)!=safe(ROOT/('control-'+bid)):raise ValueError('scope')
    rows=config['entries'];approved=config['approved_ids'];cap=config['max_bytes']
    if type(rows)!=list or not 1<=len(rows)<=4 or type(approved)!=list or len(approved)!=len(rows) or len(set(approved))!=len(rows) or type(cap)!=int or not 0<cap<=LIMIT:raise ValueError('batch_scope')
    tools=config['tools']; names=('ffmpeg','ffprobe')
    if set(tools)!={k for n in names for k in (n,n+'_sha256')}:raise ValueError('tools')
    for n in names:
        if digest(safe(Path(tools[n])))!=tools[n+'_sha256']:raise ValueError('tool_pin')
    prepared=[];total=0
    for i,row in enumerate(rows):
        if set(row)!={'entry','fixture_sha256'}:raise ValueError('entry')
        item=map_listing(config['profile'],row['entry'])
        if item['state']!='eligible_for_manual_review' or item['id']!=approved[i]:raise ValueError('not_admitted')
        src=safe(control/f'fixture-{i}.mp4');total+=src.stat().st_size
        if total>cap or digest(src)!=row['fixture_sha256']:raise ValueError('fixture_identity')
        prepared.append((i,item,src,row['fixture_sha256']))
    batch=safe(ROOT/bid);batch.mkdir();deadline=time.monotonic()+120
    for i,item,src,h in prepared:
        job=batch/str(i);job.mkdir()
        try:
            state(job,1,'staging_fixture')
            with src.open('rb') as stream:capture(stream,{'content-length':str(src.stat().st_size)},200,False,job/'selected.mp4',cap,deadline)
            if digest(job/'selected.mp4')!=h:raise ValueError('changed_source')
            identity={k:item[k] for k in ('provider','source','id')}
            accept(ROOT,job,identity,Path(tools['ffmpeg']),Path(tools['ffprobe']))
            rp=job/'receipt.json';rec=json.loads(rp.read_bytes())
            if rec['selection']!=identity or rec['object_sha256']!=h:raise ValueError('receipt_mismatch')
            exclusive_json(job/'archive.json',dict(schema='selected-source-archive-v2',identity=identity,object_sha256=h,manifest_sha256=rec['manifest_sha256'],stage_receipt_sha256=digest(rp),publication_enabled=False))
        except Exception:
            exclusive_json(job/'failure.json',dict(state='HOLD',reason='fixture_ingest_failed'))
            raise

if __name__=='__main__':
    try:
        if len(sys.argv)!=4 or sys.argv[1]!='--fixture' or not re.fullmatch('[0-9a-f]{64}',sys.argv[3]):raise ValueError()
        p=safe(Path(sys.argv[2]))
        if p.parent.parent!=ROOT or p.name!='config.json' or p.stat().st_size>65536:raise ValueError()
        raw=p.read_bytes()
        if hashlib.sha256(raw).hexdigest()!=sys.argv[3]:raise ValueError()
        fixture(json.loads(raw),p.parent)
    except Exception:print('HOLD fixture_ingest_failed');raise SystemExit(2)
