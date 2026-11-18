import json, os, subprocess, time, urllib.error, urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
EXE=ROOT/'target'/'debug'/('lifeinvader_hub.exe' if os.name=='nt' else 'lifeinvader_hub')
BASE='http://127.0.0.1:3859'
PNG=bytes.fromhex('89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c6360f8cfc0000004010100c9fe92ef0000000049454e44ae426082')
def request(path,method='GET',data=None,headers=None):
    try:
        with urllib.request.urlopen(urllib.request.Request(BASE+path,method=method,data=data,headers=headers or {}),timeout=10) as r:return r.status,r.read()
    except urllib.error.HTTPError as e:return e.code,e.read()
def json_post(path,data):return request(path,'POST',json.dumps(data).encode(),{'Content-Type':'application/json'})
def start(env,seed=True):
    p=subprocess.Popen([str(EXE),'--server-only'],cwd=ROOT,env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
    for _ in range(100):
        if p.poll() is not None:raise RuntimeError(f'server exited: {p.stderr.read().decode()}')
        try:
            code,body=request('/api/items')
            if code==200:
                if seed and not json.loads(body)['profiles']:
                    for platform,name,role in [('tiktok','demo-account','destination'),('instagram','demo-account','destination'),('tiktok','sample-source','source'),('tiktok','sample-two','source'),('tiktok','sample-three','source')]:
                        url=f'https://www.tiktok.com/@{name}' if platform=='tiktok' else f'https://www.instagram.com/{name}'
                        result,_=json_post('/api/profiles',{'name':name,'platform':platform,'url':url,'role':role,'tags':['Sources'] if role=='source' else ['Lifeinvader']})
                        if result!=201:raise RuntimeError('Could not create test fixture')
                return p
        except OSError:pass
        time.sleep(.05)
    p.terminate();raise RuntimeError('server did not start')
