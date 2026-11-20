"""Only disposable synthetic queue fixtures; no subprocess or hardware."""
import json,os,pathlib,sqlite3,unittest
from unittest.mock import patch
import clip_prepare_test
from stage_queue import run_reviewed
from single_owner_queue import Queue

class AdmissionLineage(unittest.TestCase):
    def fixture(self):
        f=clip_prepare_test.Preparation();f.setUp();return f
    def forbidden(self,*args):raise AssertionError('producer must not run')
    def test_missing_wrong_or_changed_approval_never_enqueues(self):
        for change in ('missing','wrong_pin','wrong_bound_cut','false_attestation'):
            f=self.fixture();c=dict(f.c)
            if change=='missing':c['review']=str(f.root/'absent-review.json')
            elif change=='wrong_pin':c['review_sha256']='0'*64
            else:
                review=json.loads(pathlib.Path(c['review']).read_bytes())
                entry=next(iter(review['reviews'].values()))
                if change=='wrong_bound_cut':entry['end_ms']-=1
                else:entry['attestations']['sentence_boundary_verified']=False
                raw=json.dumps(review).encode();pathlib.Path(c['review']).write_bytes(raw)
                import hashlib;c['review_sha256']=hashlib.sha256(raw).hexdigest()
            with self.subTest(change=change),self.assertRaises((ValueError,FileNotFoundError)):
                run_reviewed(c,f.root,producer=self.forbidden)
            self.assertFalse((f.root/'queue.sqlite').exists())
    def test_foreign_pending_job_is_not_adopted_or_enqueued_behind(self):
        f=self.fixture();q=Queue(f.root/'queue.sqlite');owner=q.start(os.getpid())
        q.enqueue(owner,'foreign-job','foreign-source',f.c['cut_key'],'render');q.stop(owner)
        before=(f.root/'queue.sqlite').read_bytes()
        with self.assertRaisesRegex(RuntimeError,'foreign queue lineage HOLD'):
            run_reviewed(f.c,f.root,producer=self.forbidden)
        self.assertEqual(before,(f.root/'queue.sqlite').read_bytes())
        with sqlite3.connect((f.root/'queue.sqlite').as_uri()+'?mode=ro',uri=True) as db:
            self.assertEqual(db.execute('SELECT count(*) FROM jobs').fetchone()[0],1)
            self.assertEqual(db.execute("SELECT count(*) FROM events WHERE event='claimed'").fetchone()[0],0)
    def test_claim_identity_mismatch_holds_without_producer_or_retry(self):
        original=Queue.claim
        for field in ('job_id','clip_id','video_hash','kind'):
            f=self.fixture()
            def altered(q,owner,pid):
                claim=original(q,owner,pid);claim[field]='foreign';return claim
            with self.subTest(field=field),patch.object(Queue,'claim',altered):
                with self.assertRaisesRegex(RuntimeError,'claim lineage mismatch HOLD'):
                    run_reviewed(f.c,f.root,producer=self.forbidden)
            with self.assertRaisesRegex(RuntimeError,'unresolved owner/claim HOLD'):
                run_reviewed(f.c,f.root,producer=self.forbidden)
    def test_recording_producer_receipt_and_duplicate_share_claim_lineage(self):
        f=self.fixture();calls=[]
        def fake(plan,out):
            calls.append(plan['cut_key']);out.write_bytes(b'non-media fixture proof');return '32.430'
        receipt=run_reviewed(f.c,f.root,producer=fake,worker_job_id='admitted-worker-one')
        self.assertEqual(receipt['job_id'],'admitted-worker-one')
        self.assertEqual(calls,[f.c['cut_key']])
        with sqlite3.connect((f.root/'queue.sqlite').as_uri()+'?mode=ro',uri=True) as db:
            row=db.execute('SELECT job_id,clip_id,video_hash,kind,state FROM jobs').fetchone()
            self.assertEqual(row,(receipt['job_id'],f.c['source_sha256'],f.c['cut_key'],'render','finished'))
            events=db.execute("SELECT job_id FROM events WHERE event IN ('enqueued','claimed')").fetchall()
            self.assertEqual(events,[(receipt['job_id'],),(receipt['job_id'],)])
            self.assertEqual(db.execute('SELECT count(*) FROM completion_proofs').fetchone()[0],1)
        before=(f.root/'queue.sqlite').read_bytes()
        self.assertEqual(run_reviewed(f.c,f.root,producer=self.forbidden,worker_job_id='admitted-worker-one'),receipt)
        with self.assertRaisesRegex(RuntimeError,'completion worker lineage HOLD'):
            run_reviewed(f.c,f.root,producer=self.forbidden,worker_job_id='another-worker')
        self.assertEqual(before,(f.root/'queue.sqlite').read_bytes())

if __name__=='__main__':unittest.main()
