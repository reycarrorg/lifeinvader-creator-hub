"""Test profile ID recovery and archive reuse using a fake downloader."""
import json,os,subprocess,tempfile,time
from pathlib import Path
from smoke_helpers import ROOT,request,json_post,start

def get(p):
 code,b=request(p);assert code==200,(code,b);return json.loads(b)
def wait(pid):
 for _ in range(150):
  d=get(f'/api/profiles/{pid}/downloads')
  if d['last_run'] and d['last_run']['state']!='running':return d
  time.sleep(.05)
 raise AssertionError('timeout')
with tempfile.TemporaryDirectory(prefix='lifeinvader-recovery-') as temp:
 root=Path(temp);drive=root/'drive';drive.mkdir();media=root/'media';media.mkdir();fake=root/'fake.exe'
 subprocess.run(['rustc','+stable-x86_64-pc-windows-msvc',str(ROOT/'tests/fake_recovery_ytdlp.rs'),'-o',str(fake)],check=True)
 env=os.environ.copy();env.update(LIFEINVADER_DB=str(root/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859')
 p=start(env)
 try:
  assert json_post('/api/source-settings',{'root':str(drive),'downloader':str(fake),'photo_helper':''})[0]==204
  pid=next(x['id'] for x in get('/api/items')['profiles'] if x['role']=='destination' and x['platform']=='tiktok')
  api=f'/api/profiles/{pid}/sync-recovery'
  assert json_post(api,{'post_url':'https://www.tiktok.com/@other/video/12345'})[0]==400
  assert json_post(api,{'post_url':'https://tiktok.com.evil.test/@demo-account/video/12345'})[0]==400
  seed='https://www.tiktok.com/@demo-account/video/12345';assert json_post(api,{'post_url':seed})[0]==204 and get(api)['post_url']==seed
  for _ in range(2):
   assert json_post(f'/api/profiles/{pid}/download-sync',{'include_photos':False})[0]==202
   result=wait(pid);assert result['last_run']['state']=='finished',result['last_run']
   assert len(result['files'])==1 and 'Account verified' in result['last_run']['log']
  archive=drive/f'_My Profiles/tiktok/profile-{pid}/download-archive.txt';assert archive.read_text().count('recovered-id')==1
  assert json_post(api,{'post_url':''})[0]==204 and get(api)['post_url']==''
  assert json_post(f'/api/profiles/{pid}/download-sync',{'include_photos':False})[0]==202
  assert wait(pid)['last_run']['state']=='error'
  assert json_post(api,{'post_url':seed})[0]==204
  p.terminate();p.wait(timeout=5);env['LV_FAKE_OWNER']='someone-else';p=start(env,seed=False)
  assert json_post(f'/api/profiles/{pid}/download-sync',{'include_photos':False})[0]==202
  rejected=wait(pid);assert rejected['last_run']['state']=='error' and 'Could not verify' in rejected['last_run']['log']
  print('PASS: account matching, recovery retry, archive reuse and owner mismatch rejection')
 finally:p.terminate();p.wait(timeout=5)
