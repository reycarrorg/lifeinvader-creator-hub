import sys,tempfile,os,unittest
from pathlib import Path
from unittest.mock import Mock,patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'helpers/clip_worker'))
import reviewed_job as j
class Linkage(unittest.TestCase):
 def setUp(self):
  self.old=Path.cwd();self.root=Path(tempfile.mkdtemp(prefix='reviewed-job-mock-'));os.chdir(self.root)
  self.h='a'*64;self.key='b'*64
  self.s=dict(generation=1,provider='synthetic',provider_id='source1',source_sha256=self.h,source_revision='revision1',start_ms=1000,end_ms=31000,recipe_version=j.RECIPE,durable_cut_key=self.key)
  self.r=dict(job_id='controller1',revision=1,source_binding='binding1',selection=self.s,approval_sha256='c'*64,worker_job_id='worker1',workspace=str(self.root),executable_sha256=self.h,helper_sha256=self.h,expires_at=200)
  self.c=dict(schema='native-bootstrap-reviewed-job-v1',mode='reviewed-job',manifest='manifest',manifest_sha256=self.h,worker_root=str(self.root),site_packages='unused',output='unused',wait_cancel=False,request=self.r,publishing_enabled=False,preparation='prep',preparation_sha256=self.h,producer_config='producer',producer_config_sha256=self.h,asr_python=str(self.root/'python.exe'))
  self.identity=dict(manifest='manifest',manifest_sha256=self.h)
  self.files={os.path.normcase(str(self.root/'python.exe')):self.h,os.path.normcase(str(self.root/'native_bootstrap.py')):self.h}
  self.prep=dict(review_sha256='c'*64,source_sha256=self.h,cut_key=self.key,start_ms=1000,end_ms=31000)
  self.producer=dict(source_sha256=self.h,cut_key=self.key,enabled=True)
  self.plan=dict(cut_key=self.key,recipe=j.RECIPE+':cut=1000-31000')
  self.receipt=dict(job_id='worker1',clip_id=self.h,revision=self.key,recipe=self.plan['recipe'],publication_enabled=False)
 def tearDown(self):os.chdir(self.old)
 def call(self,worker):
  with patch.object(j,'bound_json',side_effect=lambda p,*_:self.prep if p=='prep' else self.producer):
   return j.dispatch(self.c,self.identity,self.files,worker=worker,prepare_check=Mock(return_value=self.plan),now=100)
 def test_exact_worker_and_receipt_link(self):
  worker=Mock(return_value=self.receipt);self.assertEqual(self.call(worker),self.receipt)
  self.assertEqual(worker.call_args.kwargs,{'worker_job_id':'worker1'})
  self.assertFalse((self.root/'queue.sqlite').exists())
 def test_wrong_review_no_dispatch(self):
  self.prep['review_sha256']='d'*64;worker=Mock()
  with self.assertRaisesRegex(ValueError,'cut mismatch'):self.call(worker)
  worker.assert_not_called()
 def test_wrong_receipt_hold_no_second_call(self):
  self.receipt['job_id']='foreign';worker=Mock(return_value=self.receipt)
  with self.assertRaisesRegex(ValueError,'receipt mismatch'):self.call(worker)
  self.assertEqual(worker.call_count,1)
 def test_expired_or_publishing_no_dispatch(self):
  worker=Mock();self.r['expires_at']=99
  with self.assertRaisesRegex(ValueError,'expired'):self.call(worker)
  self.r['expires_at']=200;self.c['publishing_enabled']=True
  with self.assertRaisesRegex(ValueError,'schema'):self.call(worker)
  worker.assert_not_called()
if __name__=='__main__':unittest.main()
