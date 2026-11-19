"""Pure rejection/cancellation fixtures: no imports of untrusted code or subprocess."""
import ast,hashlib,importlib.machinery,os,pathlib,sqlite3,sys,tempfile,unittest
from unittest.mock import patch,MagicMock
import clip_prepare_test
import local_producer as producer
import worker_cancel
from stage_queue import run_reviewed

class Boundaries(unittest.TestCase):
    def tearDown(self):worker_cancel._cancelled.clear() # fixture-only reset, never production
    def test_import_guard_rejects_unlisted_and_changed_module(self):
        source=pathlib.Path(__file__).resolve().parents[1]/'helpers/clip_worker/native_bootstrap.py'
        cls=next(n for n in ast.walk(ast.parse(source.read_text())) if isinstance(n,ast.ClassDef) and n.name=='PinnedImports')
        env=dict(importlib=__import__('importlib'),os=os,pathlib=pathlib,files={},digest=lambda p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest())
        exec(compile(ast.Module(body=[cls],type_ignores=[]),str(source),'exec'),env)
        root=pathlib.Path(tempfile.mkdtemp(prefix='bootstrap-import-'));file=root/'untrusted.py';file.write_text('raise AssertionError("must never execute")')
        spec=importlib.machinery.ModuleSpec('untrusted',None,origin=str(file))
        with patch.object(importlib.machinery.PathFinder,'find_spec',return_value=spec):
            with self.assertRaises(ImportError):env['PinnedImports']().find_spec('untrusted')
            env['files'][os.path.normcase(str(file.resolve()))]='0'*64
            with self.assertRaises(ImportError):env['PinnedImports']().find_spec('untrusted')
    def test_cancel_before_claim_and_after_output_hold(self):
        f=clip_prepare_test.Preparation();f.setUp();worker_cancel.request()
        with self.assertRaisesRegex(RuntimeError,'cancelled'):run_reviewed(f.c,f.root,producer=lambda *_:self.fail('producer ran'),cancel=worker_cancel.checkpoint)
        self.assertFalse((f.root/'queue.sqlite').exists());worker_cancel._cancelled.clear()
        def fake(plan,out):out.write_bytes(b'isolated fake incomplete result');worker_cancel.request();return '32.430'
        with self.assertRaisesRegex(RuntimeError,'cancelled'):run_reviewed(f.c,f.root,producer=fake,cancel=worker_cancel.checkpoint)
        with sqlite3.connect((f.root/'queue.sqlite').as_uri()+'?mode=ro',uri=True) as db:
            self.assertEqual(db.execute('SELECT state FROM jobs').fetchone()[0],'claimed')
            self.assertEqual(db.execute('SELECT count(*) FROM completion_proofs').fetchone()[0],0)
        self.assertFalse((f.root/(f.c['cut_key']+'.receipt.json')).exists())
    def test_active_owned_child_cancel_is_bounded(self):
        child=MagicMock()
        def pending(**kwargs):worker_cancel.request();raise producer.subprocess.TimeoutExpired('fake',.1)
        child.communicate.side_effect=pending
        with patch.object(producer.subprocess,'Popen') as popen:
            popen.return_value.__enter__.return_value=child
            with self.assertRaisesRegex(RuntimeError,'cancelled'):producer.bounded(['inert'],pathlib.Path.cwd(),1)
        child.terminate.assert_called_once();child.wait.assert_called_once_with(timeout=3)

if __name__=='__main__':unittest.main()
