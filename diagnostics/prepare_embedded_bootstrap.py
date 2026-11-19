"""Create-only embedded production-bootstrap diagnostic bindings; no dispatch."""
from pathlib import Path
import os,json,hashlib,shutil
repo=Path.cwd();package=repo/'tools/embedded-review-folder-01';out=repo/'tools/embedded-bootstrap-07'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
out.mkdir(exist_ok=False)
batch=json.loads((repo/'tools/embedded-runtime-inventory-06/host-batch-review.json').read_text(encoding='utf-8-sig'))
for r in batch:
 assert r['observed_hash_matches'] and r['signature_status']=='Valid' and r['publisher'] in ('CN=Microsoft Windows, O=Microsoft Corporation, L=Redmond, S=Washington, C=US','CN=Microsoft Windows Software Compatibility Publisher, O=Microsoft Corporation, L=Redmond, S=Washington, C=US')
 assert Path(r['path']).parent==Path(os.environ['SystemRoot'])/'System32' and sha(Path(r['path']))==r['sha256'] and not r['copy_allowed']
with (package/'host-requirements-batch.json').open('x') as f:json.dump(batch,f,indent=2)
for source in sorted((repo/'helpers/clip_worker').glob('*.py')):
 with source.open('rb') as a,(package/'worker'/source.name).open('xb') as b:shutil.copyfileobj(a,b)
exe=repo/'tools/headless-programfiles-build-target/debug/lifeinvader_hub.exe'
assert sha(exe)=='49cd51396239f728e171e57ddff856b0414a8b781198a89f2b4de27f3dcdc8ea'
with exe.open('rb') as a,(package/'app/lifeinvader_hub.exe').open('xb') as b:shutil.copyfileobj(a,b)
files={str(p.resolve()):sha(p) for p in package.rglob('*') if p.is_file()}
old=json.loads((repo/'tools/native-fresh-hardware-09/config/runtime.json').read_text())['files']
files.update({str(Path(k).resolve()):v for k,v in old.items() if Path(k).is_relative_to(Path(os.environ['SystemRoot']))})
u=json.loads((package/'host-requirements.json').read_text(encoding='utf-8-sig'));files[u['path']]=u['sha256']
files.update({r['path']:r['sha256'] for r in batch})
manifest=out/'manifest.json';manifest.write_text(json.dumps(dict(schema='native-runtime-files-v1',files=files),sort_keys=True))
python=package/'runtime/python/python.exe';helper=package/'worker/native_bootstrap.py'
c=dict(schema='native-bootstrap-diagnostic-v1',mode='diagnostic-asr',manifest=str(manifest),manifest_sha256=sha(manifest),worker_root=str(package/'worker'),site_packages=str(package/'runtime/site-packages'),output=str(out/'bootstrap-result.json'),wait_cancel=False,asr_python=str(python),asr_model=str(package/'runtime/model'))
config=out/'config.json';config.write_text(json.dumps(c,sort_keys=True))
env=dict(LIFEINVADER_TEST_DIAGNOSTIC_ROOT=str(out),LIFEINVADER_TEST_PYTHON=str(python),LIFEINVADER_TEST_PYTHON_SHA256=sha(python),LIFEINVADER_TEST_HELPER=str(helper),LIFEINVADER_TEST_HELPER_SHA256=sha(helper),LIFEINVADER_TEST_MANIFEST_SHA256=sha(manifest),LIFEINVADER_TEST_CONFIG_SHA256=sha(config),LIFEINVADER_TEST_IMPORT_BOUND='60')
(out/'test-env.json').write_text(json.dumps(env,indent=2))
lines=[sha(p)+'  '+p.relative_to(package).as_posix() for p in sorted(package.rglob('*')) if p.is_file()]
with (package/'manifest.bootstrap-check.sha256').open('x') as f:f.write('\n'.join(lines)+'\n')
print(json.dumps(dict(files=len(files),package_manifest=sha(package/'manifest.bootstrap-check.sha256'),manifest_sha256=sha(manifest),config_sha256=sha(config),model_inference=False,gpu=False,queue=False)))
