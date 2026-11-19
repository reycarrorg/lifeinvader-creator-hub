"""Category renaming, new-media expiration, and safe folder discovery."""
import json, os, sqlite3, tempfile
from contextlib import closing
from pathlib import Path
from smoke_helpers import request,json_post,start

def get(path):
 code,body=request(path);assert code==200,(code,body);return json.loads(body)
with tempfile.TemporaryDirectory(prefix='lifeinvader-categories-expiry-') as tmp:
 base=Path(tmp);drive=base/'drive';drive.mkdir();media=base/'media';media.mkdir();folder=drive/'sample-source';folder.mkdir()
 for name in ['fresh.mp4','older.mp4','seen.mp4']:(folder/name).write_bytes(b'fixture')
 env=os.environ.copy();env.update(LIFEINVADER_DB=str(base/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859');proc=start(env)
 try:
  allp=get('/api/items')['profiles'];src=next(x for x in allp if x['name']=='sample-source');hidden=next(x for x in allp if x['name']=='sample-two');dest=next(x for x in allp if x['role']=='destination');sid=src['id'];hid=hidden['id'];did=dest['id']
  assert json_post('/api/source-settings',{'root':str(drive),'downloader':'','photo_helper':''})[0]==204
  paths=get(f'/api/sources/{sid}/folder')['paths'];assert len(paths)==1 and Path(paths[0]).resolve()==folder.resolve(),paths
  assert request(f'/api/sources/{hid}/folder','POST')[0]==404
  assert json_post('/api/categories/appearance',{'role':'source','name':'Sources','icon':'★','color':'blue','clear_image':False})[0]==204
  assert json_post('/api/categories',{'role':'source','name':'Existing'})[0]==204
  payload={'role':'source','name':'Sources','new_name':'Existing','profile_ids':[sid]}
  assert json_post('/api/categories/update',payload)[0]==409
  assert 'Sources' in next(x for x in get('/api/items')['profiles'] if x['id']==sid)['tags']
  assert json_post(f'/api/profiles/{hid}/archive',{'archived':True})[0]==204
  payload['new_name']='Clippers';assert json_post('/api/categories/update',payload)[0]==204
  snapshot=get('/api/items');renamed=next(c for c in snapshot['categories'] if c['role']=='source' and c['name']=='Clippers');assert renamed['icon']=='★' and renamed['color']=='blue'
  assert 'Clippers' in next(x for x in snapshot['profiles'] if x['id']==sid)['tags']
  assert 'Lifeinvader' in next(x for x in snapshot['profiles'] if x['id']==did)['tags']
  assert json_post(f'/api/profiles/{hid}/archive',{'archived':False})[0]==204
  assert 'Clippers' in next(x for x in get('/api/items')['profiles'] if x['id']==hid)['tags']
  assert json_post('/api/categories/update',{'role':'source','name':'Clippers','new_name':'CLIPPERS','profile_ids':[sid,hid]})[0]==204
  root=('\\\\?\\'+str(drive)) if os.name=='nt' else str(drive)
  with closing(sqlite3.connect(base/'db.sqlite3')) as db:
   for name,seconds,seen in [('fresh.mp4',100,0),('older.mp4',3601,0),('seen.mp4',100,1)]:db.execute('insert into source_new_files(profile_id,root,path,downloaded_at,seen) values(?,?,?,unixepoch()-?,?)',(sid,root,'sample-source/'+name,seconds,seen))
   db.commit()
  files={f['name']:f for f in get(f'/api/sources/{sid}')['files']};assert files['fresh.mp4']['is_new'] and not files['older.mp4']['is_new'] and not files['seen.mp4']['is_new'],files
  stat=next(s for s in get('/api/source-schedules')['schedules'] if s['profile_id']==sid);assert stat['new_count']==1 and stat['added_24h']==3,stat
  with closing(sqlite3.connect(base/'db.sqlite3')) as db:db.execute("update source_new_files set downloaded_at=unixepoch()-3601 where path='sample-source/fresh.mp4'");db.commit()
  assert not next(f for f in get(f'/api/sources/{sid}')['files'] if f['name']=='fresh.mp4')['is_new']
  print('PASS: atomic category renaming, archived memberships, appearance, expiration, 24h count, and folder paths')
 finally:proc.terminate();proc.wait(timeout=5)
