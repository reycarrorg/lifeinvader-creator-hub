"""Synthetic query replies and real disposable queue transitions; no subprocess/GPU."""
import contextlib,json,pathlib,sqlite3,sys,unittest,time
from unittest.mock import patch
import clip_prepare_test
import local_producer as p
import gpu_guard as g

GPU='GPU-11111111-1111-1111-1111-111111111111'
def reply(text='',**kw):
    raw=text.encode();return dict(stdout=raw,bytes=len(raw),exit=kw.get('exit',0),
        unavailable=kw.get('unavailable',False),timeout=kw.get('timeout',False),overflow=kw.get('overflow',False))
def inventory(encoder='0',driver='WDDM',uuid=GPU):return reply(f'0, {uuid}, {driver}, {encoder}\n')

def sessions(count=0,encoder=0,**kw):
    now=int(time.time()*1000)
    value=dict(uuid=GPU,start_ms=now,end_ms=now,session_count=count,enumerated_count=count,encoder_percent=encoder,sampling_period_us=1000000)
    value.update(kw);return reply(json.dumps(value))

class Preclaim(unittest.TestCase):
    def fixture(self):
        f=clip_prepare_test.Preparation();f.setUp();c=dict(f.c,enabled=True,gpu_uuid=GPU,asr_timeout=1,render_timeout=1)
        c.update(nvidia_smi=c['ffmpeg'],nvidia_smi_sha256=c['ffmpeg_sha256'],model_files={})
        return f,c
    def invoke(self,f,c,replies):
        with patch.object(p,'_runtime_identity',{'synthetic':True}),patch.object(p,'check_config'),patch.object(p,'read_lease',side_effect=lambda _:contextlib.nullcontext()),\
             patch.object(g,'query',side_effect=replies),patch.object(p,'bounded',side_effect=AssertionError('work started')):
            return p.run_local(f.c,f.root,c)
    def test_preclaim_rejections_create_no_queue(self):
        cases=[('UNAVAILABLE',[reply(exit=None,unavailable=True)]),
               ('UNAVAILABLE',[reply(exit=9)]),('UNAVAILABLE',[reply(timeout=True)]),
               ('ENCODER_BUSY',[inventory('11')]),('WDDM_UNKNOWN',[inventory('N/A')]),
               ('UNKNOWN',[reply('private arbitrary diagnostic path')]),
               ('UNKNOWN',[reply('x'*4096,overflow=True)]),
               ('TARGET_MISMATCH',[inventory(uuid='GPU-22222222-2222-2222-2222-222222222222')]),
               ('ENCODER_BUSY',[inventory(),sessions(1)]),
               ('SESSION_DISAGREEMENT',[inventory(),sessions(enumerated_count=1)]),
               ('STALE',[inventory(),sessions(start_ms=0,end_ms=1)]),
               ('UNKNOWN',[inventory(),sessions(sampling_period_us=0)]),
               ('UTILIZATION_DISAGREEMENT',[inventory('1'),sessions(encoder=0)])]
        for reason,replies in cases:
            with self.subTest(reason=reason):
                f,c=self.fixture()
                with self.assertRaisesRegex(RuntimeError,reason):self.invoke(f,c,replies)
                self.assertFalse((f.root/'queue.sqlite').exists())
                rows=[json.loads(x) for x in (f.root/'gpu-preclaim.jsonl').read_text().splitlines()]
                self.assertEqual(rows[-1]['decision'],reason)
                self.assertLess((f.root/'gpu-preclaim.jsonl').stat().st_size,12000)
                self.assertNotIn('private arbitrary',json.dumps(rows))
    def test_postclaim_race_retains_claim_and_refuses_reentry(self):
        f,c=self.fixture()
        with self.assertRaisesRegex(RuntimeError,'ENCODER_BUSY'):
            self.invoke(f,c,[inventory(),sessions(),inventory('11')])
        with sqlite3.connect((f.root/'queue.sqlite').as_uri()+'?mode=ro',uri=True) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM events WHERE event='claimed'").fetchone()[0],1)
            self.assertEqual(db.execute("SELECT state FROM jobs").fetchone()[0],'claimed')
            self.assertEqual(db.execute('SELECT count(*) FROM completion_proofs').fetchone()[0],0)
            self.assertEqual(db.execute('SELECT count(*) FROM manager').fetchone()[0],1)
        with self.assertRaisesRegex(RuntimeError,'unresolved owner/claim HOLD'):
            self.invoke(f,c,[]) # Must refuse before another GPU query/claim.
    def test_single_target_idle_and_exclusive_diagnostic(self):
        f,c=self.fixture()
        with patch.object(g,'query',side_effect=[inventory('10'),sessions(encoder=10)]):
            evidence=g.guard('inert',GPU,f.root,'preencode')
        self.assertEqual(evidence['gpu_uuid'],GPU);self.assertEqual(evidence['gpu_index'],0)
        with patch.object(g,'query',side_effect=AssertionError('retry')):
            with self.assertRaises(FileExistsError):g.guard('inert',GPU,f.root,'preencode')

    def test_preencode_race_never_starts_encoder(self):
        f,c=self.fixture()
        with patch.object(p,'_runtime_identity',{'synthetic':True}),patch.object(p,'check_config'),patch.object(p,'read_lease',side_effect=lambda _:contextlib.nullcontext()),\
             patch.object(g,'query',side_effect=[inventory(),sessions(),inventory(),sessions(),inventory('11')]),\
             patch.object(p,'bounded',return_value=None) as work:
            with self.assertRaisesRegex(RuntimeError,'ENCODER_BUSY'):p.run_local(f.c,f.root,c)
        self.assertEqual(work.call_count,2) # mocked PCM decode and mocked ASR only
        self.assertIn('--asr-child',work.call_args_list[1].args[0])
        self.assertTrue(all('h264_nvenc' not in call.args[0] for call in work.call_args_list))
        with sqlite3.connect((f.root/'queue.sqlite').as_uri()+'?mode=ro',uri=True) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM events WHERE event='claimed'").fetchone()[0],1)
            self.assertEqual(db.execute('SELECT count(*) FROM completion_proofs').fetchone()[0],0)

if __name__=='__main__':unittest.main()
