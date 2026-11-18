"""Exercise local and online date audits with isolated data and a fake metadata-only downloader."""
import hashlib,json,os,subprocess,tempfile,time
from pathlib import Path
from smoke_helpers import ROOT,request,json_post,start

def get(p):
 code,body=request(p);assert code==200,(p,code,body);return json.loads(body)
def wait_run():
 for _ in range(300):
  run=get('/api/date-audit')['run']
  if run and run['state']!='running':return run
  time.sleep(.05)
 raise AssertionError('audit timed out')
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
with tempfile.TemporaryDirectory(prefix='lifeinvader-date-audit-') as tmp:
 root=Path(tmp);drive=root/'drive';drive.mkdir();media=root/'media';media.mkdir();folder=drive/'sample-source';folder.mkdir();fake=root/'metadata-only.exe'
 subprocess.run(['rustc']+(['+stable-x86_64-pc-windows-msvc'] if os.name=='nt' else [])+[str(ROOT/'tests'/'fake_date_ytdlp.rs'),'-o',str(fake)],check=True)
 full='7262913527093005611';short='7300000001234567890';amb1='7262913527000012345';amb2='7262913538000012345'
 archive=drive/'download-archive.txt';archive.write_text(f'tiktok {full}\ntiktok {short}\ntiktok {amb1}\ntiktok {amb2}\n',encoding='utf8')
 names=[f'full [{full}].mp4',f'short [{short[-8:]}].mp4',f'ambiguous [{amb1[-8:]}].mp4','no-id.mp4','original.mp4','timestamp.mp4']
 for n in names:(folder/n).write_bytes(b'original-video')
 (folder/'original.info.json').write_text(json.dumps({'id':'12345','upload_date':'20230710'}),encoding='utf8')
 (folder/'timestamp.info.json').write_text(json.dumps({'id':'24680','timestamp':1704153600}),encoding='utf8')
 before={p.relative_to(drive).as_posix():digest(p) for p in drive.rglob('*') if p.is_file()}
 env=os.environ.copy();env.update(LIFEINVADER_DB=str(root/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859',LV_DATE_SLOW_FLAG=str(root/'slow.flag'));proc=start(env)
 try:
  profiles=get('/api/items')['profiles'];src=next(p['id'] for p in profiles if p['name']=='sample-source');dest=next(p['id'] for p in profiles if p['role']=='destination')
  assert json_post('/api/source-settings',{'root':str(drive),'downloader':str(fake),'photo_helper':''})[0]==204
  def files():return {f['name']:f for f in get(f'/api/sources/{src}')['files']}
  f=files();assert f[names[0]]['posted_date']=='2023-08-03' and f[names[0]]['posted_date_kind']=='estimated_id';assert f[names[1]]['post_id']==short and f[names[1]]['posted_date_kind']=='estimated_id';assert not f[names[2]]['posted_date'];assert f['original.mp4']['posted_date']=='2023-07-10' and f['original.mp4']['posted_date_kind']=='metadata';assert f['timestamp.mp4']['posted_date']=='2024-01-02' and f['timestamp.mp4']['posted_date_kind']=='metadata'
  code,body=json_post(f'/api/profiles/{dest}/queue',{'source_id':src,'path':'sample-source/'+names[0]});assert code==201,(code,body)
  assert get(f'/api/profiles/{dest}/queue')[0]['posted_date_kind']=='estimated_id'
  assert json_post('/api/date-audit',{'profile_ids':[src],'online':False})[0]==202
  r=wait_run();assert r['state']=='finished' and r['total']==6 and r['verified']==2 and r['estimated']==2 and r['unknown']==2,r
  assert json_post('/api/date-audit',{'profile_ids':[src],'online':True})[0]==202
  r=wait_run();assert r['state']=='finished' and r['verified']==4 and r['unknown']==2,r
  f=files();assert f[names[0]]['posted_date']=='2024-01-02' and f[names[0]]['posted_date_kind']=='verified';assert f['original.mp4']['posted_date']=='2023-07-10'
  queued=get(f'/api/profiles/{dest}/queue')[0];assert queued['posted_date']=='2024-01-02' and queued['posted_date_kind']=='verified'
  after={p.relative_to(drive).as_posix():digest(p) for p in drive.rglob('*') if p.is_file()};assert before==after,(before,after)
  # A replacement file at the same path must not inherit stale verified metadata.
  (folder/names[0]).write_bytes(b'replacement-file-with-different-size')
  assert files()[names[0]]['posted_date_kind']=='estimated_id'
  # Cancellation stops an in-progress metadata process and keeps the archive unchanged.
  (folder/f'cancel [{full}].mp4').write_bytes(b'cancel-test');(root/'slow.flag').write_text('slow')
  assert json_post('/api/date-audit',{'profile_ids':[src],'online':True})[0]==202
  time.sleep(.4);assert json_post(f'/api/sources/{src}/sync',{'include_photos':False})[0]==409
  code,_=request('/api/date-audit/cancel','POST');assert code==204
  r=wait_run();assert r['state']=='cancelled',r
  assert digest(archive)==before['download-archive.txt']
  print('PASS: local ID estimates, ambiguous suffixes, metadata-only online audit, queue updates, cancellation, and untouched media/archive')
 finally:proc.terminate();proc.wait(timeout=5)
