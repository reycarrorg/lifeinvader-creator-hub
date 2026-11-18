"""Exercise browser pairing, origin rules, leases, and real-data provenance with synthetic data."""
import json,os,sqlite3,tempfile
from pathlib import Path
from smoke_helpers import request,json_post,start

def get(path,headers=None):
 code,body=request(path,headers=headers);assert code==200,(code,body);return json.loads(body)
with tempfile.TemporaryDirectory(prefix='lifeinvader-bridge-test-') as t:
 root=Path(t);media=root/'media';media.mkdir();env=os.environ.copy();env.update(LIFEINVADER_DB=str(root/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859');proc=start(env)
 try:
  profiles=get('/api/items')['profiles'];pid=next(p['id'] for p in profiles if p['role']=='destination' and p['platform']=='tiktok');path=f'/api/profiles/{pid}/analytics-bridge'
  assert not get(path)['enabled'];assert request('/api/bridge/poll')[0]==401
  assert request('/api/bridge/pair',headers={'Origin':'https://evil.example'})[0]==403
  key=get('/api/bridge/pair')['token'];assert len(key)==64
  ext={'Origin':'chrome-extension://'+'a'*32,'x-lifeinvader-token':key}
  assert request('/api/bridge/check',headers=ext)[0]==204
  assert request('/api/bridge/pair',headers=ext)[0]==403
  assert request('/api/items',headers=ext)[0]==403
  assert json_post(path,{'enabled':True,'interval_seconds':3600,'after_post':True})[0]==204
  assert request(path+'/refresh','POST')[0]==202
  job=get('/api/bridge/poll',ext)['job'];assert job['profile_id']==pid
  assert get('/api/bridge/poll',ext)['job'] is None
  payload={'profile_id':pid,'job_id':job['id'],'followers':460,'following':4,'likes':44600,'likes_approx':True}
  h={**ext,'Content-Type':'application/json'}
  assert request('/api/bridge/result','POST',json.dumps(payload).encode(),h)[0]==204
  assert request('/api/bridge/result','POST',json.dumps(payload).encode(),h)[0]==409
  records=get(f'/api/profiles/{pid}/analytics');assert records[0]['followers']==460 and records[0]['likes']==44600 and records[0]['approx']==['likes'] and records[0]['source']=='Brave profile'
  state=get(path);assert state['last_success'] and not state['last_error']
  source=next(p['id'] for p in profiles if p['role']=='source')
  con=sqlite3.connect(env['LIFEINVADER_DB']);con.execute("INSERT INTO posting_queue(profile_id,source_id,platform,identity_key,root,path) VALUES(?,?,?,?,?,?)",(pid,source,'tiktok','test','root','file'));qid=con.execute('SELECT last_insert_rowid()').fetchone()[0];con.commit();con.close()
  assert json_post(f'/api/profiles/{pid}/queue/{qid}',{'state':'posted'})[0]==204
  import time
  assert get(path)['next_run'] <= time.time()
  instagram=next(p['id'] for p in profiles if p['role']=='destination' and p['platform']=='instagram')
  assert json_post(f'/api/profiles/{instagram}/analytics-bridge',{'enabled':True,'interval_seconds':3600,'after_post':False})[0]==204
  # Complete the due TikTok job, then claim the Instagram job.
  tiktok_job=get('/api/bridge/poll',ext)['job'];assert tiktok_job['profile_id']==pid
  assert request('/api/bridge/result','POST',json.dumps({'profile_id':pid,'job_id':tiktok_job['id'],'error':'login'}).encode(),h)[0]==204
  assert 'not visible' in get(path)['last_error']
  ij=get('/api/bridge/poll',ext)['job'];assert ij['profile_id']==instagram
  inp={'profile_id':instagram,'job_id':ij['id'],'followers':64,'post_links':['https://www.instagram.com/demo-account/reel/abcde123/','https://www.instagram.com/other/reel/bad99999/','https://evil.example/demo-account/reel/bad99999/']}
  assert request('/api/bridge/result','POST',json.dumps(inp).encode(),h)[0]==204
  con=sqlite3.connect(env['LIFEINVADER_DB']);links=con.execute('SELECT url FROM analytics_bridge_links WHERE profile_id=?',(instagram,)).fetchall();con.close();assert links==[('https://www.instagram.com/reel/abcde123/',)]
  assert request('/api/bridge/pair','DELETE')[0]==204
  assert request('/api/bridge/check',headers=ext)[0]==401
  print('PASS: pairing, extension origin restrictions, leases, provenance, rounded values and disconnect')
 finally:proc.terminate();proc.wait(timeout=5)
