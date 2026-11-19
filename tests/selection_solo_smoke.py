"""Selection and solo posts: isolated database, fake downloader and no platform access."""
import json,os,subprocess,tempfile,time,sqlite3
from pathlib import Path
from smoke_helpers import ROOT,request,json_post,start

def wait_run(sid):
 for _ in range(200):
  r=json.loads(request(f'/api/sources/{sid}/status')[1])['last_run']
  if r and r['state']!='running': return r
  time.sleep(.05)
 raise AssertionError('download timed out')
with tempfile.TemporaryDirectory(prefix='lifeinvader-selection-') as tmp:
 base=Path(tmp); drive=base/'drive';drive.mkdir();media=base/'media';media.mkdir();fake=base/'fake.exe'
 subprocess.run(['rustc']+(['+stable-x86_64-pc-windows-msvc'] if os.name=='nt' else [])+[str(ROOT/'tests/fake_selection_ytdlp.rs'),'-o',str(fake)],check=True)
 env=os.environ.copy();env.update(LIFEINVADER_DB=str(base/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859')
 p=start(env,seed=False)
 try:
  assert json_post('/api/source-settings',{'root':str(drive),'downloader':str(fake),'photo_helper':''})[0]==204
  code,b=json_post('/api/profiles',{'name':'example','platform':'tiktok','url':'https://www.tiktok.com/@example','role':'source','tags':[],'setup_selection':True});assert code==201,b
  sid=json.loads(b)['id']
  assert json_post(f'/api/sources/{sid}/sync',{'include_photos':False})[0]==400
  code,b=json_post(f'/api/sources/{sid}/preview',{});assert code==200,b
  preview=json.loads(b)['preview'];assert [x['id'] for x in preview['posts']]==['300','200','100']
  assert json_post(f'/api/sources/{sid}/selection',{'token':'wrong','mode':'after','cutoff':1})[0]==400
  code,b=json_post(f'/api/sources/{sid}/selection',{'token':preview['token'],'mode':'after','cutoff':1});assert code==200,b
  assert json.loads(b)=={'selected':2,'excluded':1}
  assert json_post(f'/api/sources/{sid}/sync',{'include_photos':False})[0]==202
  assert wait_run(sid)['state']=='finished'
  assert (drive/'download-archive.txt').read_text().splitlines()==['tiktok 300','tiktok 200']
  assert json_post(f'/api/sources/{sid}/sync',{'include_photos':True})[0]==400
  assert json_post(f'/api/sources/{sid}/sync',{'include_photos':False})[0]==202
  assert wait_run(sid)['state']=='finished'
  assert (drive/'download-archive.txt').read_text().splitlines()==['tiktok 300','tiktok 200','tiktok 400']
  assert not (drive/'example'/'post [100].mp4').exists()
  code,b=json_post('/api/profiles',{'name':'second-source','platform':'tiktok','url':'https://www.tiktok.com/@second-source','role':'source','tags':[],'setup_selection':True});assert code==201,b
  second=json.loads(b)['id']
  code,b=json_post(f'/api/sources/{second}/preview',{});assert code==200,b
  prev2=json.loads(b)['preview']
  assert json_post(f'/api/sources/{second}/selection',{'token':prev2['token'],'mode':'before','cutoff':1})[0]==200
  assert json_post(f'/api/sources/{second}/sync',{'include_photos':False})[0]==202
  assert wait_run(second)['state']=='finished'
  assert json_post(f'/api/sources/{second}/sync',{'include_photos':False})[0]==202
  assert wait_run(second)['state']=='finished'
  assert 'tiktok 600' not in (drive/'download-archive.txt').read_text()
  assert not (drive/'second-source'/'post [600].mp4').exists()
  assert json_post('/api/solo-posts',{'links':['https://evil.test/@x/video/12']})[0]==400
  code,b=json_post('/api/solo-posts',{'links':['https://www.tiktok.com/@x/video/123','https://www.tiktok.com/@x/video/123']});assert code==202,b
  solo=json.loads(b);assert solo['links']==1
  assert wait_run(solo['id'])['state']=='finished'
  assert json_post(f"/api/sources/{solo['id']}/sync",{'include_photos':False})[0]==400
  assert json_post(f"/api/sources/{solo['id']}/schedule",{'enabled':True,'interval_seconds':3600})[0]==400
  data=json.loads(request(f"/api/sources/{solo['id']}")[1]);assert data['files'],data
  assert data['files'][0]['original_account']=='creator' and data['files'][0]['original_platform']=='tiktok'
  print('PASS: selection guards, archive exclusions across syncs, separate solo posts and metadata')
 finally:p.terminate();p.wait(timeout=5)
