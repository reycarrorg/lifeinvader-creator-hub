"""Solo-post metadata and queue identity, with no live downloads."""
import json,os,sqlite3,tempfile,urllib.parse
from contextlib import closing
from pathlib import Path
from smoke_helpers import request,json_post,start

def get(path):
 code,body=request(path);assert code==200,(code,body);return json.loads(body)
with tempfile.TemporaryDirectory(prefix='lifeinvader-solo-picker-') as tmp:
 base=Path(tmp);drive=base/'drive';drive.mkdir();media=base/'media';media.mkdir();solo_dir=drive/'Solo Posts'/'Downloads'/'creator';solo_dir.mkdir(parents=True);source_dir=drive/'example';source_dir.mkdir()
 for path,body in [(solo_dir/'clip.mp4',b'video'),(solo_dir/'clip.jpg',b'thumbnail'),(solo_dir/'photo.jpg',b'photo'),(source_dir/'clip.mp4',b'video')]:path.write_bytes(body)
 info={'id':'IG123','title':'Original clip','description':'Original description','extractor_key':'Instagram','uploader_id':'creator','uploader_url':'https://www.instagram.com/creator/','webpage_url':'https://www.instagram.com/p/IG123/','upload_date':'20240102'}
 (solo_dir/'clip.info.json').write_text(json.dumps(info));(source_dir/'clip.info.json').write_text(json.dumps(info));(solo_dir/'photo.info.json').write_text(json.dumps(dict(info,id='IG456',title='Photo')))
 env=os.environ.copy();env.update(LIFEINVADER_DB=str(base/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859');proc=start(env)
 try:
  assert json_post('/api/source-settings',{'root':str(drive),'downloader':'','photo_helper':''})[0]==204
  with closing(sqlite3.connect(base/'db.sqlite3')) as db:
   db.execute("insert into profiles(name,platform,url,role,tags,source_folder) values('Solo Posts','other','https://lifeinvader.local/solo-posts','source','[]','')");db.commit()
  code,body=json_post('/api/profiles',{'name':'example','platform':'instagram','url':'https://www.instagram.com/example/','role':'source','tags':[]});assert code==201,(code,body)
  normal=json.loads(body)['id'];profiles=get('/api/items')['profiles'];solo=next(p['id'] for p in profiles if p['url']=='https://lifeinvader.local/solo-posts');target=next(p['id'] for p in profiles if p['role']=='destination')
  found=get(f'/api/sources/{solo}')['files'];assert len(found)==3
  clip=next(f for f in found if f['name']=='clip.mp4');assert clip['original_platform']=='instagram' and clip['original_account']=='creator' and clip['post_id']=='IG123'
  assert request(f'/api/sources/{solo}/file?'+urllib.parse.urlencode({'path':clip['path']}))[1]==b'video'
  code,body=json_post(f'/api/profiles/{target}/queue',{'source_id':solo,'path':clip['path']});assert code==201,(code,body)
  q=get(f'/api/profiles/{target}/queue')[0];assert q['platform']=='instagram' and q['source_id']==solo and q['title']=='Original clip' and q['original_url']==info['webpage_url'] and q['channel_url']==info['uploader_url']
  assert json_post(f'/api/profiles/{target}/queue',{'source_id':normal,'path':'example/clip.mp4'})[0]==409
  print('PASS: Solo Posts metadata, accessible media, source identity and cross-source queue deduplication')
 finally:proc.terminate();proc.wait(timeout=5)
