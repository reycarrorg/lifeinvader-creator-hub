"""Create-only private payload staging; never installs, launches or authorizes."""
import argparse, hashlib, json, shutil
from pathlib import Path, PurePosixPath

def digest(path):
    with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()

def copy_pinned(source,target,expected):
    for p in (source,*source.parents):
        if p.is_symlink() or (p.exists() and getattr(p.stat(),'st_file_attributes',0)&0x400):
            raise ValueError('reparse source refused')
    target.parent.mkdir(parents=True,exist_ok=True)
    h=hashlib.sha256();size=0
    with source.open('rb') as src,target.open('xb') as dst:
        while block:=src.read(1024*1024):h.update(block);dst.write(block);size+=len(block)
    if h.hexdigest()!=expected:raise ValueError('pin mismatch; retain partial staging, no retry')
    return dict(sha256=expected,bytes=size)

def stage(plan_path,plan_sha,exe,exe_sha,destination):
    if digest(plan_path)!=plan_sha:raise ValueError('layout pin mismatch')
    plan=json.loads(plan_path.read_text())
    if plan['schema']!='runtime-relocation-plan-v1' or plan['launch_enabled'] or plan['qualified']:
        raise ValueError('unqualified source plan required')
    entries=list(plan['files'])+[dict(source=str(exe),destination='app/lifeinvader_hub.exe',sha256=exe_sha)]
    seen=set()
    for e in entries:
        rel=PurePosixPath(e['destination'])
        if rel.is_absolute() or '..' in rel.parts or ':' in str(rel) or '\\' in str(rel):raise ValueError('payload path')
        if str(rel).casefold() in seen:raise ValueError('payload collision')
        seen.add(str(rel).casefold())
    destination.mkdir(parents=False,exist_ok=False)
    result={}
    for e in entries:
        result[e['destination']]=copy_pinned(Path(e['source']),destination/e['destination'],e['sha256'])
    manifest=dict(schema='private-relocated-review-payload-v1',launch_enabled=False,
                  runtime_qualified=False,redistribution_reviewed=False,installer=False,
                  source_layout_sha256=plan_sha,files=result)
    with (destination/'payload-manifest.json').open('x') as f:json.dump(manifest,f,sort_keys=True,indent=2)
    return dict(files=len(result),bytes=sum(v['bytes'] for v in result.values()),manifest_sha256=digest(destination/'payload-manifest.json'))

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--plan',type=Path,required=True);p.add_argument('--plan-sha',required=True);p.add_argument('--exe',type=Path,required=True);p.add_argument('--exe-sha',required=True);p.add_argument('--destination',type=Path,required=True)
    a=p.parse_args();print(json.dumps(stage(a.plan,a.plan_sha,a.exe,a.exe_sha,a.destination)))
