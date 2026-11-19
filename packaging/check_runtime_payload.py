"""Static staged-byte/layout check only. Does not import or execute payload code."""
import hashlib,json
from pathlib import Path,PurePosixPath

def check(root,expected_manifest):
    manifest=root/'payload-manifest.json'
    if hashlib.sha256(manifest.read_bytes()).hexdigest()!=expected_manifest:raise ValueError('manifest mismatch')
    m=json.loads(manifest.read_text())
    if m['schema']!='private-relocated-review-payload-v1' or any(m[k] for k in ('launch_enabled','runtime_qualified','redistribution_reviewed','installer')):raise ValueError('unexpected qualification claim')
    expected=set(m['files'])|{'payload-manifest.json'};actual=set();total=0
    for p in root.rglob('*'):
        if p.is_symlink() or p.stat().st_file_attributes&0x400:raise ValueError('reparse payload')
        if p.is_file():actual.add(p.relative_to(root).as_posix())
    if actual!=expected:raise ValueError('missing or extra payload bytes')
    for name,pin in m['files'].items():
        rel=PurePosixPath(name)
        if rel.is_absolute() or '..' in rel.parts or ':' in name or '\\' in name:raise ValueError('unsafe manifest path')
        p=root/name
        if p.stat().st_size!=pin['bytes']:raise ValueError('size mismatch')
        with p.open('rb') as f:h=hashlib.file_digest(f,'sha256').hexdigest()
        if h!=pin['sha256']:raise ValueError('byte mismatch')
        total+=pin['bytes']
    required={'app/lifeinvader_hub.exe','runtime/python/python.exe','runtime/python/python312.dll',
              'runtime/python/Lib/encodings/__init__.py','runtime/model/model.bin','runtime/model/config.json',
              'runtime/model/tokenizer.json','runtime/model/vocabulary.txt','runtime/ffmpeg/ffmpeg.exe',
              'runtime/ffmpeg/ffprobe.exe','worker/native_bootstrap.py','worker/local_producer.py'}
    if not required<=expected:raise ValueError('required layout missing')
    if any(name.endswith('pyvenv.cfg') or name.startswith('runtime/Scripts/') for name in actual):raise ValueError('nonrelocatable venv copied')
    return dict(schema='static-runtime-layout-check-v1',static_bytes_and_layout='PASS',files=len(m['files']),bytes=total,
                executed_payload=False,relocated_imports_qualified=False,licenses_qualified=False,launch_enabled=False)

if __name__=='__main__':
    import argparse
    p=argparse.ArgumentParser();p.add_argument('root',type=Path);p.add_argument('manifest_sha');a=p.parse_args()
    print(json.dumps(check(a.root,a.manifest_sha)))
