"""Build a disabled final-folder binding inventory; never execute payload code."""
import hashlib,json
from pathlib import Path
from final_paths import pinned_final_file
REQUIRED={'python':'runtime/python/python.exe','python_dll':'runtime/python/python312.dll','worker':'worker/native_bootstrap.py','ffmpeg':'runtime/ffmpeg/ffmpeg.exe','ffprobe':'runtime/ffmpeg/ffprobe.exe','model':'runtime/model/model.bin','model_config':'runtime/model/config.json','tokenizer':'runtime/model/tokenizer.json','vocabulary':'runtime/model/vocabulary.txt'}
def build(root,manifest_pin):
    root=Path(root).resolve(strict=True);raw=(root/'payload-manifest.json').read_bytes()
    if hashlib.sha256(raw).hexdigest()!=manifest_pin:raise ValueError('inventory identity')
    manifest=json.loads(raw)
    if manifest['schema']!='private-relocated-review-payload-v1':raise ValueError('inventory schema')
    pins={name:record['sha256'] for name,record in manifest['files'].items()}
    bindings={}
    for role,relative in REQUIRED.items():
        p=pinned_final_file(root,root/relative,pins)
        bindings[role]={'path':str(p),'sha256':pins[relative]}
    return dict(schema='disabled-final-folder-bindings-v1',payload_manifest_sha256=manifest_pin,bindings=bindings,dispatch_enabled=False,publishing_enabled=False,oauth_configured=False,runtime_qualified=False,path_fallback=False,static_final_paths_verified=True)
