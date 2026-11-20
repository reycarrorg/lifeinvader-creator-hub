import hashlib,json,os,subprocess,threading,datetime
from pathlib import Path
repo=Path.cwd();python=repo/'tools/embedded-review-folder-01/runtime/python/python.exe';script=repo/'diagnostics/verify_embedded_runtime.py';root=repo/'tools/embedded-runtime-inventory-06'
assert hashlib.sha256(python.read_bytes()).hexdigest()=='4d6f5f81a4bca11191c4c7c6b43632694d0a4ce74e068619d8fdc161d469859a'
root.mkdir(exist_ok=False)
env=dict(SystemRoot=os.environ['SystemRoot'],TEMP=str(root),TMP=str(root),PATH='',HF_HUB_OFFLINE='1',TRANSFORMERS_OFFLINE='1',HF_HUB_DISABLE_TELEMETRY='1',PYTHONDONTWRITEBYTECODE='1')
p=subprocess.Popen([str(python),'-I','-B','-u',str(script),str(repo/'tools/embedded-review-folder-01'),str(root),str(repo/'tools/native-fresh-hardware-09/config/runtime.json')],cwd=root,env=env,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=0x08000000)
data=[bytearray(),bytearray()];sizes=[0,0]
def drain(stream,index):
    while b:=stream.read(1024):sizes[index]+=len(b);data[index].extend(b[:max(0,4096-len(data[index]))])
threads=[threading.Thread(target=drain,args=(stream,i),daemon=True) for i,stream in enumerate((p.stdout,p.stderr))]
for t in threads:t.start()
timed=False
try:code=p.wait(timeout=35)
except subprocess.TimeoutExpired:timed=True;p.terminate();code=p.wait(timeout=3)
for t in threads:t.join(1)
record=dict(pid=p.pid,exit=code,timeout=timed,stdout_bytes=sizes[0],stderr_bytes=sizes[1],stdout_sha256=hashlib.sha256(data[0]).hexdigest(),stderr_sha256=hashlib.sha256(data[1]).hexdigest(),utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),script_sha256=hashlib.sha256(script.read_bytes()).hexdigest())
(root/'exit.json').write_text(json.dumps(record,indent=2));print(json.dumps(record))
if (root/'result.json').exists():print((root/'result.json').read_text())
elif sizes[0]<=4096:
    try:
        safe=json.loads(data[0]);print(json.dumps({k:safe[k] for k in ('result','error_class','code') if k in safe}))
    except Exception:print('UNCLASSIFIED_OUTPUT_HOLD')
