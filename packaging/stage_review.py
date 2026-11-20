"""Build-only local payload staging. Does not install, launch, download or arm."""
import argparse,hashlib,json,shutil
from pathlib import Path

def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def stage(repo,exe,expected,destination):
 repo=repo.resolve();exe=exe.resolve();destination=destination.absolute()
 if destination.exists():raise ValueError('create-only destination required')
 if sha(exe)!=expected:raise ValueError('candidate executable mismatch')
 sources=[(exe,Path('app/lifeinvader_hub.exe'))]
 sources += [(p,Path('worker')/p.name) for p in sorted((repo/'helpers/clip_worker').glob('*.py'))]
 destination.mkdir(parents=False)
 inventory={}
 for source,relative in sources:
  if source.is_symlink() or not source.is_file():raise ValueError('invalid payload source')
  target=destination/relative;target.parent.mkdir(parents=True,exist_ok=True)
  with source.open('rb') as src,target.open('xb') as dst:shutil.copyfileobj(src,dst)
  actual=sha(target)
  if actual!=sha(source):raise ValueError('staged bytes mismatch; retain evidence')
  inventory[str(relative).replace('\\','/')]=dict(sha256=actual,bytes=target.stat().st_size)
 (destination/'payload-manifest.json').write_text(json.dumps(dict(schema='review-payload-v1',installer=False,launch_enabled=False,publishing_enabled=False,runtime_bundled=False,files=inventory),sort_keys=True,indent=2))
 return inventory
if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('--repo',type=Path,required=True);parser.add_argument('--exe',type=Path,required=True);parser.add_argument('--sha256',required=True);parser.add_argument('--destination',type=Path,required=True)
 a=parser.parse_args();stage(a.repo,a.exe,a.sha256,a.destination)
