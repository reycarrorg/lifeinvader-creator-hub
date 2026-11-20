"""New loader-policy rejection tests. No native dependency/import or ASR execution."""
import sys,os,tempfile,hashlib,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'helpers/clip_worker'))
import dll_inventory as d
class Policy(unittest.TestCase):
    def test_unlisted_or_changed_runtime_is_hold(self):
        for kind in ('UNLISTED','windows-system-pending-signature'):
            with patch.object(d,'snapshot',return_value={'unlisted':[],'modules':[{'kind':kind}]}):
                with self.assertRaisesRegex(ValueError,'HOLD'):d.enforce_snapshot({})
    def test_ambiguous_dll_and_unlisted_sibling_rejected(self):
        root=Path(tempfile.mkdtemp(prefix='dll-policy-'));a=root/'a';b=root/'b';a.mkdir();b.mkdir()
        x=a/'same.dll';y=b/'same.dll';x.write_bytes(b'a');y.write_bytes(b'b')
        norm=lambda p:os.path.normcase(str(p.resolve()))
        pins={norm(x):hashlib.sha256(b'a').hexdigest(),norm(y):hashlib.sha256(b'b').hexdigest()}
        with self.assertRaisesRegex(ValueError,'ambiguous'):d.search_plan(pins)
        z=a/'unexpected.pyd';z.write_bytes(b'not executable')
        with self.assertRaisesRegex(ValueError,'unlisted native sibling'):d.search_plan({norm(x):pins[norm(x)]})
    def test_real_asr_has_checks_around_lazy_generator(self):
        import ast
        source=(Path(__file__).resolve().parents[1]/'helpers/clip_worker/local_producer.py').read_text()
        fn=next(n for n in ast.parse(source).body if isinstance(n,ast.FunctionDef) and n.name=='asr_child')
        calls=[n for n in ast.walk(fn) if isinstance(n,ast.Call) and isinstance(n.func,ast.Attribute) and n.func.attr=='enforce_snapshot']
        self.assertEqual(len(calls),4)
        loop=next(n for n in fn.body if isinstance(n,ast.For))
        self.assertEqual(loop.body[0].value.func.attr,'enforce_snapshot')
if __name__=='__main__':unittest.main()
