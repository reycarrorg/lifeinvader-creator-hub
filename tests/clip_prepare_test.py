"""Synthetic gate wiring only; no model, GPU, subprocess or real evidence."""
import hashlib, json, pathlib, sys, tempfile, unittest
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]/'helpers'/'clip_worker'))
import prepare
import rubric_gate
from export_profile import RECIPE

class Preparation(unittest.TestCase):
    def setUp(self):
        self.root=pathlib.Path(tempfile.mkdtemp(prefix='lifeinvader-prepare-'))
        def save(name,data):
            p=self.root/name;p.write_bytes(data);return str(p),hashlib.sha256(data).hexdigest()
        source,source_hash=save('source.mp4',b'inert original source')
        intake,intake_hash=save('intake.json',json.dumps(dict(schema='verified-intake-v1',status='verified',source_sha256=source_hash,bytes=21,duration_ms=40000,segments={'speech':[1640,33220]})).encode())
        proposal=json.dumps([dict(speech_id='speech',source_sha256=source_hash)])
        b=rubric_gate.binding(intake_hash,proposal,1290,33720)
        key=rubric_gate.digest(json.dumps(b,sort_keys=True,separators=(',',':')).encode())
        entry=dict(b,scores={k:1 for k in rubric_gate.SCORE},total=7,attestations={k:True for k in rubric_gate.REVIEW})
        review,review_hash=save('review.json',json.dumps(dict(schema='trusted-rubric-v1',reviews={key:entry})).encode())
        tool,tool_hash=save('fake-tool.exe',b'never execute')
        self.c=dict(source=source,source_sha256=source_hash,speech_id='speech',intake=intake,intake_sha256=intake_hash,review=review,review_sha256=review_hash,proposal=proposal,start_ms=1290,end_ms=33720,cut_key=hashlib.sha256((source_hash+RECIPE+':cut=1290-33720').encode()).hexdigest(),ffmpeg=tool,ffmpeg_sha256=tool_hash,ffprobe=tool,ffprobe_sha256=tool_hash,model=str(self.root))
    def test_exact_review_without_queue_or_producer(self):
        before={p.name:p.read_bytes() for p in self.root.iterdir()}
        result=prepare.prepare(self.c)
        self.assertEqual(result['state'],'DRY_RUN_READY')
        self.assertFalse(result['render_authorized']);self.assertFalse(result['worker_started'])
        self.assertEqual(before,{p.name:p.read_bytes() for p in self.root.iterdir()})
    def test_tampered_source_rejected(self):
        pathlib.Path(self.c['source']).write_bytes(b'changed')
        with self.assertRaises(ValueError):prepare.prepare(self.c)
    def test_cut_change_rejected(self):
        self.c['end_ms']-=1
        with self.assertRaises(ValueError):prepare.prepare(self.c)
    def test_review_and_tool_pins_rejected(self):
        for name in ('review_sha256','ffmpeg_sha256'):
            c=dict(self.c);c[name]='0'*64
            with self.assertRaises(ValueError):prepare.prepare(c)
if __name__=='__main__':unittest.main()
