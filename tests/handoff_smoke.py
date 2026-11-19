"""Computer-use handoff and activity log with isolated data and no publishing."""
import json, os, sqlite3, tempfile
from pathlib import Path
from smoke_helpers import request, json_post, start

def get(path, headers=None):
 code,body=request(path,headers=headers); assert code==200,(code,body); return json.loads(body)
with tempfile.TemporaryDirectory(prefix='lifeinvader-handoff-') as tmp:
 root=Path(tmp); media=root/'media';media.mkdir();drive=root/'drive';drive.mkdir();folder=drive/'sample-source';folder.mkdir()
 for item in ('one','two','three'):
  (folder/(item+'.mp4')).write_bytes(b'fake-media')
  (folder/(item+'.info.json')).write_text(json.dumps({'id':item,'upload_date':'20240102','title':'Title '+item,'description':'Original context','webpage_url':'https://www.tiktok.com/@sample-source/video/12345'}))
 env=os.environ.copy();env.update(LIFEINVADER_DB=str(root/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859');proc=start(env)
 try:
  profiles=get('/api/items')['profiles'];target=next(p['id'] for p in profiles if p['role']=='destination');other=next(p['id'] for p in profiles if p['role']=='destination' and p['id']!=target);source=next(p['id'] for p in profiles if p['name']=='sample-source')
  assert json_post('/api/source-settings',{'root':str(drive),'downloader':'','photo_helper':''})[0]==204
  def add(pid,name,when):
   code,body=json_post(f'/api/profiles/{pid}/queue',{'source_id':source,'path':f'sample-source/{name}.mp4','scheduled_for':when});assert code==201,(code,body);return json.loads(body)['id']
  due=add(target,'one','2000-01-01T12:00'); future=add(target,'two','2999-01-01T12:00');unscheduled=add(other,'three','')
  handoff=get('/api/posting-handoff');assert handoff['schema_version']==1
  assert [x['id'] for x in handoff['work_items']]==[due,unscheduled,future]
  byid={x['id']:x for x in handoff['work_items']};a=byid[due]
  assert a['timing']=='due' and byid[future]['timing']=='upcoming' and byid[unscheduled]['timing']=='unscheduled'
  assert a['target']['profile_id']==target and a['title']=='Title one' and a['description']=='Original context'
  assert Path(a['local_file']).resolve()==(folder/'one.mp4').resolve()
  assert a['record_template']['expected_revision']==0 and a['record_status_path']==f'/api/profiles/{target}/queue/{due}'
  assert len(a['events'])==1 and a['events'][0]['event_type']=='queued'
  assert request('/api/posting-handoff',headers={'Origin':'https://evil.example'})[0]==403
  path=a['record_status_path'];payload=dict(a['record_template']);payload.update(note_kind='needs_review',note='Upload result unclear; check account first.')
  assert json_post(path,payload)[0]==204
  assert json_post(path,payload)[0]==409
  a=next(x for x in get('/api/posting-handoff')['work_items'] if x['id']==due)
  assert a['state']=='queued' and a['revision']==1 and a['events'][-1]['event_type']=='needs_review'
  assert a['events'][-1]['note']=='Upload result unclear; check account first.'
  payload=dict(a['record_template']);payload.update(state='posted',caption='Final caption',post_url='https://example.com/post/123')
  assert json_post(path,payload)[0]==204
  h=get('/api/posting-handoff');assert all(x['id']!=due for x in h['work_items'])
  posted=next(x for x in h['recorded_items'] if x['id']==due);assert posted['state']=='posted' and posted['revision']==2 and len(posted['events'])==3
  assert get(f'/api/profiles/{target}/queue/{due}/events')[-1]['post_url']=='https://example.com/post/123'
  assert request(f'/api/profiles/{other}/queue/{due}/events')[0]==404
  (folder/'two.mp4').unlink();missing=next(x for x in get('/api/posting-handoff')['work_items'] if x['id']==future);assert not missing['available'] and missing['local_file'] is None
  proc.terminate();proc.wait(timeout=5);proc=start(env,seed=False)
  persisted=next(x for x in get('/api/posting-handoff')['recorded_items'] if x['id']==due);assert persisted['events'][-1]['caption']=='Final caption'
  print('PASS: multi-profile handoff, timestamps, missing files, durable notes, stale revision rejection and manual status history')
 finally:proc.terminate();proc.wait(timeout=5)
