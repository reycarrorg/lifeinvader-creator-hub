"""Create-only Fresh11 review packet; no launch, GPU query or authority delivery."""
import datetime,hashlib,json,sqlite3,uuid
from pathlib import Path
repo=Path(__file__).resolve().parents[1];old=repo/'tools/native-fresh-hardware-09';root=repo/'tools/native-fresh-hardware-13'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):
    with p.open('x',encoding='utf-8') as f:json.dump(v,f,sort_keys=True,separators=(',',':'))
def rebase(v):
    if isinstance(v,str):return v.replace(str(old),str(root))
    if isinstance(v,dict):return {k:rebase(x) for k,x in v.items()}
    if isinstance(v,list):return [rebase(x) for x in v]
    return v
root.mkdir(exist_ok=False)
for name in ('config','media','worker','preflight'):(root/name).mkdir()
(root/'media/synthetic-source').mkdir()
source=repo/'tools/fresh-synthetic-flite-01/synthetic-source.mp4';assert sha(source)=='b86b69e755cce1a6806a15aa3792aeb5bdca46d11f8b94ed6908662b657d6593'
(root/'media/synthetic-source/original.mp4').write_bytes(source.read_bytes())
packet=json.loads((old/'preflight/combined-launch-packet.json').read_text());plan=rebase(json.loads((old/'config/manual-review-plan.json').read_text()))
now=int(datetime.datetime.now(datetime.timezone.utc).timestamp());expiry=now+900
request=plan['request'];request.update(job_id='isolated-'+uuid.uuid4().hex,worker_job_id='native-'+uuid.uuid4().hex,expires_at=expiry,helper_sha256=sha(Path(plan['helper'])))
a=dict(id=1,url='lifeinvader-local://synthetic-review',platform='local',source_folder='synthetic-source',source_root='\\\\?\\'+str(root/'media'))
request['source_binding']=hashlib.sha256(json.dumps(a,separators=(',',':')).encode()).hexdigest()
# DB initialization/fixture seeding is performed by the real Rust startup test.
for name in ('intake.json','proposal.json','independent-app-review.json','producer.json'):
    data=(old/'config'/name).read_bytes();assert hashlib.sha256(data).hexdigest()==packet['config_sha256'][name];(root/'config'/name).write_bytes(data)
prep=rebase(json.loads((old/'config/preparation.json').read_text()));write(root/'config/preparation.json',prep)
m=json.loads((old/'config/runtime.json').read_text());changed=[]
# Every job-local leased input follows the new isolated root, never a held root.
local_inputs={'config/independent-app-review.json','config/intake.json','config/preparation.json','config/producer.json','config/proposal.json','media/synthetic-source/original.mp4'}
rebound={}
for key,pin in m['files'].items():
    path=Path(key)
    if path.is_relative_to(old):
        relative=path.relative_to(old).as_posix();assert relative in local_inputs
        target=root/relative;assert target.is_file();rebound[str(target)]=sha(target)
    else:rebound[key]=pin
m['files']=rebound
assert not any(Path(k).is_relative_to(old) for k in m['files'])
assert all(m['files'].get(str(root/r))==sha(root/r) for r in local_inputs)

for key in list(m['files']):
    p=Path(key)
    if p.parent==repo/'helpers/clip_worker':
        current=sha(p)
        if current!=m['files'][key]:changed.append(p.name);m['files'][key]=current
vendor=repo/'helpers/clip_worker/vendor_diagnostic.py';m['files'][str(vendor)]=sha(vendor)
write(root/'config/runtime.json',m)
b=rebase(json.loads((old/'config/bootstrap.json').read_text()));b.update(request=request,manifest_sha256=sha(root/'config/runtime.json'),preparation_sha256=sha(root/'config/preparation.json'));write(root/'config/bootstrap.json',b)
plan['runtime'].update(config_sha256=sha(root/'config/bootstrap.json'),manifest_sha256=sha(root/'config/runtime.json'));write(root/'config/manual-review-plan.json',plan)
exe=repo/'tools/headless-programfiles-build-target/debug/lifeinvader_hub.exe'
packet=rebase(packet);packet.update(exe=str(exe),exe_sha256=sha(exe),plan_sha256=sha(root/'config/manual-review-plan.json'),request=request,created_utc=datetime.datetime.fromtimestamp(now,datetime.timezone.utc).isoformat(),expires_at=expiry,expires_utc=datetime.datetime.fromtimestamp(expiry,datetime.timezone.utc).isoformat(),port=43868,url='http://127.0.0.1:43868/controller',baseline={'admissions':0,'jobs':[[request['job_id'],'QUEUED',0]],'paused':1,'worker_empty':True},runtime_rebound_entries=len(changed)+1,policy_changes=[],production_environment_addition='OS-known ProgramFiles only',owned_exit_drain_ms=500)
packet['authorization']='PENDING_SCOPED_SAME_BUILD_EXECUTION'
packet['config_sha256']={p.name:sha(p) for p in (root/'config').iterdir() if p.is_file()}
write(root/'preflight/combined-launch-packet.json',packet)
print(json.dumps({'packet':str(root/'preflight/combined-launch-packet.json'),'packet_sha256':sha(root/'preflight/combined-launch-packet.json'),'plan_sha256':packet['plan_sha256'],'exe_sha256':packet['exe_sha256'],'exe_bytes':exe.stat().st_size,'runtime_sha256':sha(root/'config/runtime.json'),'expiry_utc':packet['expires_utc'],'job':request['job_id'],'launched':False,'gpu_checked':False}))
