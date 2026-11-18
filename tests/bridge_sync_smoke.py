"""Regression test for Instagram fallback when a profile scan fails."""
import json,os,subprocess,tempfile,time
from pathlib import Path
from smoke_helpers import request,json_post,start,ROOT

def get(p,h=None):
 code,body=request(p,headers=h);assert code==200,(code,body);return json.loads(body)
with tempfile.TemporaryDirectory(prefix='lifeinvader-bridge-sync-') as t:
 root=Path(t);drive=root/'drive';drive.mkdir();media=root/'media';media.mkdir();fake=root/'fake.exe'
 subprocess.run(['rustc']+(['+stable-x86_64-pc-windows-msvc'] if os.name=='nt' else [])+[str(ROOT/'tests/fake_bridge_ytdlp.rs'),'-o',str(fake)],check=True)
 env=os.environ.copy();env.update(LIFEINVADER_DB=str(root/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859');p=start(env)
 try:
  profile=next(x for x in get('/api/items')['profiles'] if x['role']=='destination' and x['platform']=='instagram');pid=profile['id']
  assert json_post('/api/source-settings',{'root':str(drive),'downloader':str(fake),'photo_helper':''})[0]==204
  key=get('/api/bridge/pair')['token'];h={'x-lifeinvader-token':key,'Content-Type':'application/json'}
  assert json_post(f'/api/profiles/{pid}/analytics-bridge',{'enabled':True,'interval_seconds':3600,'after_post':False})[0]==204
  job=get('/api/bridge/poll',h)['job'];payload={'profile_id':pid,'job_id':job['id'],'followers':5,'post_links':['https://www.instagram.com/demo-account/reel/abcde123/']}
  assert request('/api/bridge/result','POST',json.dumps(payload).encode(),h)[0]==204
  assert json_post(f'/api/profiles/{pid}/download-sync',{'include_photos':False})[0]==202
  for _ in range(100):
   out=get(f'/api/profiles/{pid}/downloads')
   if out['last_run'] and out['last_run']['state']!='running':break
   time.sleep(.05)
  assert out['last_run']['state']=='finished',out['last_run'];assert any(f['post_id']=='abcde123' for f in out['files'])
  assert 'Profile scan failed' in out['last_run']['log']
  print('PASS: failed Instagram profile scan falls back to captured post links')
 finally:p.terminate();p.wait(timeout=5)
