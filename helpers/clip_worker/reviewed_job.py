"""Strict Controller-to-native producer adapter. No CLI or default authority."""
import json,hashlib,os,time,re
from pathlib import Path
from export_profile import RECIPE
REQUEST={'job_id','revision','source_binding','selection','approval_sha256','worker_job_id','workspace','executable_sha256','helper_sha256','expires_at'}
SELECTION={'generation','provider','provider_id','source_sha256','source_revision','start_ms','end_ms','recipe_version','durable_cut_key'}
FIELDS={'schema','mode','manifest','manifest_sha256','worker_root','site_packages','output','wait_cancel','request','publishing_enabled','preparation','preparation_sha256','producer_config','producer_config_sha256','asr_python'}
def strict_object(pairs):
    result={}
    for k,v in pairs:
        if k in result:raise ValueError('duplicate reviewed field')
        result[k]=v
    return result

def bound_json(path,pin,files):
    p=Path(path);key=os.path.normcase(str(p.resolve()))
    if not p.is_absolute() or p.is_symlink() or files.get(key)!=pin:raise ValueError('reviewed file not leased')
    with p.open('rb') as f:raw=f.read(16385)
    if len(raw)>16384 or hashlib.sha256(raw).hexdigest()!=pin:raise ValueError('reviewed file bytes')
    return json.loads(raw,object_pairs_hook=strict_object)

def validate(c,identity,files,*,prepare_check,now=None):
    if set(c)!=FIELDS or c['schema']!='native-bootstrap-reviewed-job-v1' or c['mode']!='reviewed-job' or c['publishing_enabled'] is not False or c['wait_cancel'] is not False:
        raise ValueError('reviewed job schema')
    r=c['request'];s=r.get('selection',{})
    if set(r)!=REQUEST or set(s)!=SELECTION:raise ValueError('reviewed request schema')
    now=int(time.time()) if now is None else now
    if type(r['expires_at']) is not int or not 0<r['expires_at']-now<=900:raise ValueError('review expired')
    if type(r['revision']) is not int or r['revision']<0 or type(s['generation']) is not int or s['generation']<1:raise ValueError('reviewed revision')
    for field in ('job_id','worker_job_id','source_binding'):
        if type(r[field]) is not str or not 0<len(r[field])<=512:raise ValueError('reviewed identity')
    if not re.fullmatch(r'[A-Za-z0-9_.-]{1,256}',r['worker_job_id']):raise ValueError('worker identity')
    root=Path(r['workspace'])
    if not root.is_absolute() or root.resolve()!=Path.cwd().resolve() or root.is_symlink():raise ValueError('reviewed workspace')
    if identity.get('manifest')!=c['manifest'] or identity.get('manifest_sha256')!=c['manifest_sha256']:raise ValueError('runtime binding')
    if files.get(os.path.normcase(str(Path(c['asr_python']).resolve())))!=r['executable_sha256'] or files.get(os.path.normcase(str((Path(c['worker_root'])/'native_bootstrap.py').resolve())))!=r['helper_sha256']:raise ValueError('runtime request pins')
    p=bound_json(c['preparation'],c['preparation_sha256'],files);producer=bound_json(c['producer_config'],c['producer_config_sha256'],files)
    if p['review_sha256']!=r['approval_sha256'] or p['source_sha256']!=s['source_sha256'] or p['cut_key']!=s['durable_cut_key'] or (p['start_ms'],p['end_ms'])!=(s['start_ms'],s['end_ms']) or s['recipe_version']!=RECIPE:raise ValueError('reviewed cut mismatch')
    if not s['source_revision'] or not s['provider'] or not s['provider_id']:raise ValueError('source revision missing')
    if producer['source_sha256']!=s['source_sha256'] or producer['cut_key']!=s['durable_cut_key'] or producer['enabled'] is not True:raise ValueError('producer binding')
    plan=prepare_check(p) # existing exact intake/rubric/cut gates, never replaced
    if plan['cut_key']!=s['durable_cut_key'] or plan['recipe']!=f"{RECIPE}:cut={s['start_ms']}-{s['end_ms']}":raise ValueError('prepared plan mismatch')
    return p,producer,plan

def dispatch(c,identity,files,*,worker,prepare_check,now=None):
    p,producer,plan=validate(c,identity,files,prepare_check=prepare_check,now=now)
    r=c['request'];s=r['selection'];root=Path(r['workspace'])
    receipt=worker(p,root,producer,worker_job_id=r['worker_job_id'])
    if receipt.get('job_id')!=r['worker_job_id'] or receipt.get('clip_id')!=s['source_sha256'] or receipt.get('revision')!=s['durable_cut_key'] or receipt.get('recipe')!=plan['recipe'] or receipt.get('publication_enabled') is not False:raise ValueError('returned receipt mismatch HOLD')
    # Rust independently reopens queue/artifact and verifies the committed proof.
    return receipt
