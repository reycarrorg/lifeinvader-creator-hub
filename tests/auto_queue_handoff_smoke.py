"""Disposable auto-queue rules and AI result contract; no network downloads or posts."""
import json, os, sqlite3, tempfile, time
from pathlib import Path
from contextlib import closing
from smoke_helpers import request, json_post, start

def get(path):
 code,body=request(path); assert code==200,(code,body); return json.loads(body)
def wait_for(fn, seconds=15):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  value=fn()
  if value:return value
  time.sleep(.25)
 raise AssertionError('Timed out waiting for expected state')
with tempfile.TemporaryDirectory(prefix='lifeinvader-auto-queue-') as tmp:
 root=Path(tmp);media=root/'media';media.mkdir();drive=root/'drive';drive.mkdir();folder=drive/'sample-source';folder.mkdir()
 for name,date,length in [('one','20240102',12),('two','20240502',15),('too-short','20240202',2),('old','20200102',12)]:
  (folder/(name+'.mp4')).write_bytes(b'fake video')
  info={'id':name,'title':'clip '+name,'upload_date':date,'webpage_url':'https://www.tiktok.com/@sample-source/video/12345'}
  if name!='two':info['duration']=length
  (folder/(name+'.info.json')).write_text(json.dumps(info))
 env=os.environ.copy();env.update(LIFEINVADER_DB=str(root/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859');proc=start(env)
 try:
  profiles=get('/api/items')['profiles'];target=next(p['id'] for p in profiles if p['role']=='destination');source=next(p['id'] for p in profiles if p['name']=='sample-source')
  assert json_post('/api/source-settings',{'root':str(drive),'downloader':'','photo_helper':''})[0]==204
  code,prompt=request('/api/posting-handoff/prompt');assert code==200 and b'ready_for_scheduled_run' in prompt
  overview=get(f'/api/sources/{source}');two=next(f for f in overview['files'] if f['name']=='two.mp4');assert two['duration_seconds'] is None
  scan={'root':str(drive),'profile_id':source,'path':two['path'],'size':two['size'],'modified':two['modified'],'seconds':15}
  assert json.loads(json_post('/api/source-durations',{'entries':[dict(scan,root='wrong') ]})[1])['stored']==0
  assert json.loads(json_post('/api/source-durations',{'entries':[scan]})[1])['stored']==1
  assert next(f for f in get(f'/api/sources/{source}')['files'] if f['name']=='two.mp4')['duration_seconds']==15
  (folder/'two.mp4').write_bytes(b'changed size video')
  two=next(f for f in get(f'/api/sources/{source}')['files'] if f['name']=='two.mp4');assert two['duration_seconds'] is None
  scan.update(size=two['size'],modified=two['modified']);assert json.loads(json_post('/api/source-durations',{'entries':[scan]})[1])['stored']==1
  filters={'category':'Sources','source_id':source,'kind':'video','min':5,'max':20,'from':'2024-01-01','to':'2024-12-31','search':'clip','unknown':False,'estimated':True}
  config={'enabled':True,'interval_seconds':60,'max_pending':1,'filters':filters,'instructions':'Use a concise caption.'}
  assert json_post(f'/api/profiles/{target}/auto-queue',dict(config,interval_seconds=1))[0]==400
  assert json_post(f'/api/profiles/{target}/auto-queue',config)[0]==204
  rule=next(x for x in get('/api/auto-queue') if x['profile_id']==target);assert rule['enabled'] and rule['filters']==filters and rule['next_run']
  def due():
   with closing(sqlite3.connect(root/'db.sqlite3')) as db:
    db.execute('update auto_queue_rules set next_run=0 where profile_id=?',(target,));db.commit()
  def queued():return get(f'/api/profiles/{target}/queue')
  due();first=wait_for(lambda:queued() or None);assert len(first)==1 and first[0]['post_id'] in ('one','two')
  due();wait_for(lambda:'limit reached' in next(x['last_result'] for x in get('/api/auto-queue') if x['profile_id']==target));assert len(queued())==1
  a=next(x for x in get('/api/posting-handoff')['work_items'] if x['id']==first[0]['id']);assert a['ready_for_scheduled_run'] and a['timing']=='due' and not a['needs_review'] and a['target']['posting_instructions']=='Use a concise caption.'
  claim_request={'profile_id':target,'queue_id':a['id'],'expected_revision':a['revision']}
  code,body=json_post('/api/posting-handoff/claim',claim_request);assert code==201,(code,body);token=json.loads(body)['claim_token']
  assert json_post('/api/posting-handoff/claim',claim_request)[0]==409
  claimed=next(x for x in get('/api/posting-handoff')['work_items'] if x['id']==a['id']);assert claimed['claimed'] and not claimed['ready_for_scheduled_run']
  result=dict(a['record_template'],profile_id=target,queue_id=a['id'],note_kind='needs_review',note='Upload result unclear')
  assert json.loads(json_post('/api/posting-handoff/results',{'results':[result]})[1])['results'][0]['status']==409
  result['claim_token']=token
  code,body=json_post('/api/posting-handoff/results',{'results':[result]});assert code==200 and json.loads(body)['results'][0]['ok']
  code,body=json_post('/api/posting-handoff/results',{'results':[result]});assert code==200 and json.loads(body)['results'][0]['status']==409
  a=next(x for x in get('/api/posting-handoff')['work_items'] if x['id']==first[0]['id']);assert a['needs_review'] and not a['ready_for_scheduled_run']
  missing=dict(a['record_template'],profile_id=target,queue_id=a['id']);del missing['caption']
  assert not json.loads(json_post('/api/posting-handoff/results',{'results':[missing]})[1])['results'][0]['ok']
  result=dict(a['record_template'],profile_id=target,queue_id=a['id'],state='posted',caption='confirmed caption',post_url='https://example.com/post/1',note='Confirmed after review')
  assert json.loads(json_post('/api/posting-handoff/results',{'results':[result]})[1])['results'][0]['ok']
  due();second=wait_for(lambda: (q if len(q:=queued())==2 else None));assert {x['post_id'] for x in second}=={'one','two'}
  b=next(x for x in get('/api/posting-handoff')['work_items'] if x['id']!=first[0]['id'])
  assert json_post('/api/posting-handoff/claim',{'profile_id':target,'queue_id':b['id'],'expected_revision':b['revision']})[0]==201
  assert json_post(f'/api/profiles/{target}/archive',{'archived':True})[0]==409
  assert json_post(f'/api/profiles/{target}/auto-queue',dict(config,enabled=False))[0]==204
  proc.terminate();proc.wait(timeout=5);proc=start(env,seed=False)
  stuck=next(x for x in get('/api/posting-handoff')['work_items'] if x['id']==b['id']);assert stuck['claimed'] and not stuck['ready_for_scheduled_run']
  assert json_post(f"/api/profiles/{target}/queue/{b['id']}/release",{'expected_revision':b['revision'],'note':'Checked account; upload outcome is unclear.'})[0]==204
  stuck=next(x for x in get('/api/posting-handoff')['work_items'] if x['id']==b['id']);assert not stuck['claimed'] and stuck['needs_review'] and not stuck['ready_for_scheduled_run']
  rule=next(x for x in get('/api/auto-queue') if x['profile_id']==target);assert not rule['enabled'] and rule['filters']==filters and rule['next_run'] is None
  assert len(queued())==2
  print('PASS: scanned lengths, saved filters, due picks, queue limit, no duplicates, claim exclusion, interrupted-claim recovery, handoff results, review state and restart')
 finally:proc.terminate();proc.wait(timeout=5)
