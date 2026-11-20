"""Final path negatives and one ffprobe metadata call; no media/model/GPU."""
from pathlib import Path
import sys,os,json,hashlib,subprocess
repo=Path.cwd();sys.path.insert(0,str(repo/'packaging'))
from final_paths import pinned_final_file
root=(repo/'tools/embedded-review-folder-01').resolve();out=repo/'tools/embedded-final-paths-08';out.mkdir(exist_ok=False)
pins={s.split('  ',1)[1]:s.split('  ',1)[0] for s in (root/'manifest.bootstrap-check.sha256').read_text().splitlines()}
roles=['runtime/python/python.exe','worker/native_bootstrap.py','worker/asr_bootstrap.py','runtime/model/model.bin','runtime/model/config.json','runtime/model/tokenizer.json','runtime/model/vocabulary.txt','runtime/ffmpeg/ffmpeg.exe','runtime/ffmpeg/ffprobe.exe']
for role in roles:assert pinned_final_file(root,root/role,pins)==root/role
negative=0
for path,inventory in [(repo/'tools/speech-runtime/Scripts/python.exe',pins),(root/'runtime/python/python.exe',{}),(root/'runtime/model/missing.bin',pins),(Path('ffprobe.exe'),pins)]:
 try:pinned_final_file(root,path,inventory)
 except (ValueError,FileNotFoundError):negative+=1
 else:raise AssertionError('fallback admitted')
assert negative==4
exe=root/'runtime/ffmpeg/ffprobe.exe';assert pins['runtime/ffmpeg/ffprobe.exe']=='02264dfa4f56baaa31e443ece8aa16d397f6b008200e7fd184d22b9009e75fd3'
env=dict(SystemRoot=os.environ['SystemRoot'],PATH='',TEMP=str(out),TMP=str(out))
p=subprocess.Popen([str(exe),'-version'],cwd=out,env=env,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,creationflags=0x08000000)
raw,_=p.communicate(timeout=5);assert p.returncode==0 and len(raw)<65536
(out/'ffprobe-version.txt').write_bytes(raw)
result=dict(final_roles=len(roles),no_fallback_negatives=negative,ffprobe_pid=p.pid,ffprobe_exit=p.returncode,ffprobe_stdout_sha256=hashlib.sha256(raw).hexdigest(),media_opened=False,gpu=False,model_inference=False,ffmpeg_metadata='retained embedded-ffmpeg-license-evidence-01; not repeated')
(out/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
