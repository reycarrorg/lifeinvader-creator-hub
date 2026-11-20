"""Pending exact package app startup/close test. No render route or worker authority."""
import sys,os,json,hashlib,subprocess,time,http.client
from pathlib import Path
_helper=Path(__file__).resolve().with_name('headless_review_run.py')
if hashlib.sha256(_helper.read_bytes()).hexdigest()!='95077deb571534bc66b18b12e6431e7771c3e8c20340cd55f139f3f602a595dc':raise ValueError('read-only helper pin')
sys.path.insert(0,str(_helper.parent))
from headless_review_run import listener_pid,descendants

def run(plan_path,pin):
 p=Path(plan_path).resolve(strict=True);raw=p.read_bytes()
 if hashlib.sha256(raw).hexdigest()!=pin:raise ValueError('plan pin')
 plan=json.loads(raw);exe=Path(plan['exe']);root=Path(plan['state_root'])
 if plan['scope']!='package-startup-close-no-authority' or plan['render'] or plan['publishing'] or plan['deletion']:raise ValueError('scope')
 if hashlib.sha256(exe.read_bytes()).hexdigest()!=plan['exe_sha256']:raise ValueError('app pin')
 if root.exists():raise ValueError('fresh state required')
 port=plan['port']
 if listener_pid(port):raise ValueError('occupied port')
 root.mkdir();(root/'media').mkdir();(root/'temp').mkdir()
 env=dict(SystemRoot=os.environ['SystemRoot'],PATH='',TEMP=str(root/'temp'),TMP=str(root/'temp'),LIFEINVADER_PORT=str(port))
 records=[]
 def req(path,body=None,expected=200):
  if listener_pid(port)!=[child.pid]:raise ValueError('listener peer mismatch')
  c=http.client.HTTPConnection('127.0.0.1',port,timeout=3)
  try:
   c.request('GET' if body is None else 'POST',path,body=None if body is None else json.dumps(body),headers={'Origin':f'http://127.0.0.1:{port}','Content-Type':'application/json'})
   r=c.getresponse();data=r.read(65537)
   if r.status!=expected or len(data)>65536:raise ValueError('response scope')
   return json.loads(data) if expected==200 else None
  finally:c.close()
 with (root/'stdout.txt').open('xb') as stdout,(root/'stderr.txt').open('xb') as stderr:
  child=subprocess.Popen([str(exe),'--server-only','--package-review-root',str(root)],cwd=exe.parent,env=env,stdin=subprocess.DEVNULL,stdout=stdout,stderr=stderr,creationflags=0x08000000)
  try:
   deadline=time.monotonic()+35
   while not listener_pid(port):
    if child.poll() is not None or time.monotonic()>deadline:raise ValueError('readiness HOLD')
    time.sleep(.1)
   view=req('/api/controller')
   assert view['package_read_only'] and not view['manual_native_enabled'] and not view['publishing_connected'] and view['total_jobs']==0
   assert 'metricool' not in json.dumps(view).lower()
   req('/api/controller/ui-lifetime',{'action':'ready'})
   for endpoint in ('/api/controller/jobs','/api/controller/jobs/x/render-reviewed','/api/controller/jobs/x/import-completed','/api/controller/settings'):
    req(endpoint,{},404)
   req('/api/controller/ui-lifetime',{'action':'close'})
   code=child.wait(timeout=10);owned=descendants(child.pid)
   assert code==0 and not owned
   result=dict(state='PASS',pid=child.pid,exit=code,owned_descendants=owned,render=False,publishing=False,deletion=False,metricool_consulted=False,zero_jobs=True)
   (root/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
  finally:
   if child.poll() is None:
    # Only documented exact-instance close; no force/foreign process operations.
    try:req('/api/controller/ui-lifetime',{'action':'close'});child.wait(timeout=10)
    except Exception:print('HOLD: exact package process requires inspection; no force or retry')
if __name__=='__main__':run(sys.argv[1],sys.argv[2])
