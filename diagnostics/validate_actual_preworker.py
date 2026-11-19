"""Actual production validation only, retained leases, no worker callable/processes."""
import contextlib,hashlib,json,os,sys,time,sqlite3,copy
from pathlib import Path
repo=Path(__file__).resolve().parents[1];root=Path(sys.argv[1]).resolve() if len(sys.argv)==2 else repo/'tools/native-preworker-chain-12'
assert root.is_relative_to(repo/'tools')
sys.path[:0]=[str(repo/'helpers/clip_worker'),str(repo/'tools/speech-runtime/Lib/site-packages')]
import reviewed_job,prepare,local_producer

def deny(event,args):
 if event in ('subprocess.Popen','os.system','socket.__new__','socket.connect','socket.bind'):raise RuntimeError('effect forbidden')
sys.addaudithook(deny)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
config=root/'config/bootstrap.json';c=json.loads(config.read_text());manifest=Path(c['manifest']);assert sha(manifest)==c['manifest_sha256']
m=json.loads(manifest.read_text());files={os.path.normcase(str(Path(k).resolve())):v for k,v in m['files'].items()}
p=json.loads(Path(c['preparation']).read_text());producer=json.loads(Path(c['producer_config']).read_text())
scoped=[config,manifest,Path(c['preparation']),Path(c['producer_config']),Path(p['source']),Path(p['intake']),Path(p['review']),root/'config/proposal.json']
for path in scoped:
 assert path.resolve().is_relative_to(root)
 if path not in (config,manifest):assert files.get(os.path.normcase(str(path.resolve())))==sha(path)
assert all('native-fresh-hardware-09' not in k for k in files)
identity={'manifest':c['manifest'],'manifest_sha256':c['manifest_sha256']}
report={'schema':'actual-reviewed-preworker-proof-v1','dispatch_allowed':False,'queue_started':False,'model_loaded':False,'gpu_queried':False,'config_sha256':sha(config),'manifest_sha256':sha(manifest)}
with contextlib.ExitStack() as leases:
 for path in (config,manifest):leases.enter_context(local_producer.read_lease(path))
 for path,pin in files.items():
  leases.enter_context(local_producer.read_lease(path));assert sha(Path(path))==pin
 old=Path.cwd();os.chdir(root/'worker')
 try:
  _,_,plan=reviewed_job.validate(c,identity,files,prepare_check=prepare.prepare)
  local_producer.check_config(producer)
  report.update(actual_validation='PASS',producer_readonly_config='PASS',cut_key=plan['cut_key'],leased_files=len(files)+2)
  stale=copy.deepcopy(c);stale['preparation']=str(repo/'tools/native-fresh-hardware-09/config/preparation.json')
  try:reviewed_job.validate(stale,identity,files,prepare_check=prepare.prepare)
  except ValueError as e:
   assert str(e)=='reviewed file not leased';report['stale_fresh09_rejected']='PASS'
  else:raise AssertionError('stale accepted')
 finally:os.chdir(old)
with sqlite3.connect((root/'controller.sqlite').as_uri()+'?mode=ro',uri=True) as db:
 assert db.execute('select schema_version from controller_meta where singleton=1').fetchall()==[(1,)]
 assert db.execute('select version from controller_admission_meta').fetchall()==[(1,)]
 assert db.execute('select count(*) from controller_admissions').fetchone()==(0,)
 assert db.execute('select count(*) from controller_admission_launches').fetchone()==(0,)
 report['database_versions_and_zero_admissions']='PASS'
assert not list((root/'worker').iterdir());report['worker_directory_unchanged_empty']=True
report['completed_unix']=int(time.time());report['review_expiry']=c['request']['expires_at']
with (root/'preflight/actual-preworker-proof.json').open('x') as f:json.dump(report,f,indent=2)
print(json.dumps(report))
