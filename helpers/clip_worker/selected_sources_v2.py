"""Pure provider discovery policy and offline fixture CLI. No acquisition or provider requests."""
import hashlib,json,re,sys
from pathlib import Path
from urllib.parse import urlsplit

SCHEMA='selected-discovery-v2'
HANDLE=r'[A-Za-z0-9_.]{1,24}'
YHANDLE=r'[A-Za-z0-9_.-]{3,30}'

def source_identity(url):
    if type(url)!=str or len(url)>256 or not url.isascii() or any(ord(c)<=32 for c in url):raise ValueError('source_url')
    u=urlsplit(url)
    if u.scheme!='https' or u.query or u.fragment or any(c in url for c in ('%','\\','?','#')):raise ValueError('source_url')
    host=u.netloc.lower()
    routes=[
        ('tiktok','www.tiktok.com',r'/@('+HANDLE+r')/?','/@{}'),
        ('youtube','www.youtube.com',r'/(@'+YHANDLE+r'|channel/UC[A-Za-z0-9_-]{22})(?:/shorts)?/?','/{}/shorts'),
        ('instagram','www.instagram.com',r'/([a-zA-Z0-9_.]{1,30})(?:/reels)?/?','/{}/reels/')]
    for provider,official,pattern,template in routes:
        if host not in (official,official[4:]):continue
        match=re.fullmatch(pattern,u.path)
        if not match:break
        account=match[1]
        if account in ('.','..') or (provider=='instagram' and account.lower() in ('p','reel','reels','accounts','explore','direct','stories','api')):break
        if not account.startswith('channel/'):account=account.lower()
        return provider,'https://'+official+template.format(account)
    raise ValueError('unsupported_profile')

def candidate(scope,row):
    provider,source=scope
    patterns={'youtube':r'[A-Za-z0-9_-]{11}','instagram':r'[A-Za-z0-9_-]{1,40}','tiktok':r'[0-9]{15,22}'}
    if type(row)!=dict or not re.fullmatch(patterns[provider],str(row.get('id',''))) or type(row.get('id'))!=str:raise ValueError('candidate_id')
    identity={'provider':provider,'source':source,'id':row['id']}
    status=row.get('availability')
    if status in ('private','premium_only','subscriber_only','needs_auth') or row.get('has_drm') is True:return dict(identity,state='blocked',reason='restricted')
    if row.get('source')!=source or status!='public' or row.get('has_drm') is not False:return dict(identity,state='recheck',reason='missing_public_metadata')
    if row.get('kind') not in ('video','short','reel'):return dict(identity,state='recheck',reason='media_type')
    required={'youtube':'short','instagram':'reel','tiktok':'video'}[provider]
    if row['kind']!=required:return dict(identity,state='excluded',reason='wrong_media_type')
    return dict(identity,state='eligible_for_manual_review',reason='public_metadata')

def classify(profile,rows):
    scope=source_identity(profile)
    if type(rows)!=list or len(rows)>50:raise ValueError('discovery_limit')
    seen=set();result=[]
    for row in rows:
        item=candidate(scope,row)
        if item['id'] in seen:raise ValueError('duplicate_candidate')
        seen.add(item['id']);result.append(item)
    return result

def discovery_args(profile):
    _,url=source_identity(profile)
    return ['--ignore-config','--no-plugin-dirs','--no-cache-dir','--flat-playlist','--skip-download','--dump-json','--playlist-end','50','--sleep-requests','2','--socket-timeout','10','--retries','0','--extractor-retries','0','--',url]

def verified_duplicate(item,receipt_pin,receipt,manifest_raw,media,poster):
    # Caller supplies trusted archive receipt pin, leased object streams and raw manifest bytes.
    sha=lambda b:hashlib.sha256(b).hexdigest()
    if sha(receipt)!=receipt_pin:raise ValueError('archive_receipt_pin')
    r=json.loads(receipt);identity={k:item[k] for k in ('provider','source','id')}
    if r.get('schema')!='selected-source-archive-v2' or r.get('publication_enabled') is not False:raise ValueError('archive_schema')
    if r.get('identity')!=identity:return False
    if sha(manifest_raw)!=r.get('manifest_sha256'):raise ValueError('archive_manifest_pin')
    m=json.loads(manifest_raw)
    if m.get('schema')!='selected-local-object-v1' or m.get('sha256')!=r.get('object_sha256'):raise ValueError('archive_object')
    for stream,h,n in ((media,m['sha256'],m['bytes']),(poster,m['poster_sha256'],m['poster_bytes'])):
        if type(n)!=int or n<=0 or n>256*1024*1024:raise ValueError('archive_size')
        stream.seek(0,2)
        if stream.tell()!=n:raise ValueError('archive_size')
        stream.seek(0)
        if hashlib.file_digest(stream,'sha256').hexdigest()!=h:raise ValueError('archive_changed')
    return True

if __name__=='__main__':
    try:
        if sys.argv[1:]!=['--classify-offline']:raise ValueError()
        raw=sys.stdin.buffer.read(65537)
        if len(raw)>65536:raise ValueError()
        req=json.loads(raw)
        if set(req)!={'schema','profile','rows'} or req['schema']!=SCHEMA:raise ValueError()
        print(json.dumps({'schema':SCHEMA,'items':classify(req['profile'],req['rows']),'dispatch':False},separators=(',',':')))
    except Exception:print('{"state":"HOLD","reason":"invalid_discovery_input"}');raise SystemExit(2)
