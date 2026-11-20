"""No-process invocation/diagnostic seam regressions."""
import ast,hashlib,importlib.util,io,json,os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[1]
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
class Contract(unittest.TestCase):
    def test_old_child_fails_windows_environment_case_before_spawn(self):
        m=load('held_matrix',ROOT/'diagnostics/vendor_environment_matrix.py')
        root=Path(tempfile.mkdtemp(prefix='vendor-inert-contract-'));tool=root/'inert-vendor.txt';tool.write_bytes(b'inert')
        m.TOOL=tool;m.PIN=hashlib.sha256(b'inert').hexdigest()
        calls=[]
        class Child:
            pid=123
            def __init__(self,args,**kwargs):calls.append((args,kwargs));self.stdout=io.BytesIO(b'Failed to initialize NVML: Unknown Error\r\n')
            def wait(self,timeout):return 255
        with patch.object(m.sys,'stdin',type('Input',(),{'buffer':io.BytesIO(b'GO\n')})()),patch.object(m.subprocess,'Popen',Child),patch.dict(os.environ,{'SystemRoot':r'C:\Windows'},clear=True):
            self.assertIn('SYSTEMROOT',dict(os.environ))
            with self.assertRaisesRegex(KeyError,'SystemRoot'):m.child(root)
        self.assertEqual(calls,[]) # Proven before vendor spawn, not an NVML failure.
        tree=ast.parse((ROOT/'diagnostics/vendor_environment_matrix.py').read_text(encoding='utf-8-sig'))
        dispatch=tree.body[-1].body[0]
        self.assertEqual(ast.unparse(dispatch.test),"len(sys.argv) == 3 and sys.argv[1] == '--child'")
    def test_vendor_environment_never_inherits_extra_keys(self):
        m=load('vendor_diagnostic',ROOT/'helpers/clip_worker/vendor_diagnostic.py')
        env=m.environment('windir_only',{'SystemRoot':r'C:\Windows','SECRET':'never','PATH':'untrusted'},Path.cwd())
        self.assertNotIn('SECRET',env);self.assertNotIn('PATH',env);self.assertEqual(env['WINDIR'],r'C:\Windows')
        with self.assertRaises(ValueError):m.environment('inherit_everything',{},Path.cwd())
        self.assertEqual(m.classify(b'sensitive arbitrary output',255),'UNCLASSIFIED_HOLD')
        with patch.dict(os.environ,{'SystemRoot':r'C:\Windows'},clear=True):
            self.assertEqual(m.environment('windir_only',os.environ,Path.cwd())['WINDIR'],r'C:\Windows')
    def test_spawn_failure_is_recorded_without_raw_error(self):
        m=load('vendor_diag_error',ROOT/'helpers/clip_worker/vendor_diagnostic.py')
        root=Path(tempfile.mkdtemp(prefix='vendor-error-contract-'));tool=root/'inert.txt';tool.write_bytes(b'inert')
        m.TOOL=tool;m.PIN=hashlib.sha256(b'inert').hexdigest();marks=[]
        config={'vendor_cases':['windir_only'],'output':str(root/'vendor-result.json')}
        with patch.object(m.Path,'cwd',return_value=root),patch.object(m,'environment',return_value={}),patch.object(m.subprocess,'Popen',side_effect=OSError(5,'private must not appear')):
            with self.assertRaises(OSError):m.run(config,{os.path.normcase(str(tool.resolve())):m.PIN},marks.append,lambda:None)
        raw=(root/'vendor-result.json').read_text();row=json.loads(raw)
        self.assertEqual(row['error_code'],'OS_ERROR');self.assertEqual(row['stage'],'SPAWN')
        self.assertNotIn('private',raw);self.assertEqual(marks,['VENDOR_START_WINDIR_ONLY'])
    def test_combined_failure_stops_before_any_removal(self):
        m=load('vendor_os_diag',ROOT/'helpers/clip_worker/vendor_diagnostic.py')
        root=Path(tempfile.mkdtemp(prefix='vendor-os-contract-'));tool=root/'inert.txt';tool.write_bytes(b'inert')
        m.TOOL=tool;m.PIN=hashlib.sha256(b'inert').hexdigest();calls=[]
        class Child:
            pid=1;returncode=255
            def __init__(self,*args,**kwargs):calls.append(1);self.stdout=io.BytesIO(b'Failed to initialize NVML: Unknown Error');self.stderr=io.BytesIO()
            def poll(self):return 255
        with patch.object(m.Path,'cwd',return_value=root),patch.object(m,'environment',return_value={}),patch.object(m.subprocess,'Popen',Child):
            m.run({'vendor_cases':m.OS_CASES,'output':str(root/'vendor-result.json')},{os.path.normcase(str(tool.resolve())):m.PIN},lambda _:None,lambda:None)
        self.assertEqual(calls,[1]);self.assertEqual(len((root/'vendor-result.json').read_text().splitlines()),1)
    def test_real_bootstrap_vendor_branch_precedes_producer(self):
        text=(ROOT/'helpers/clip_worker/native_bootstrap.py').read_text()
        self.assertLess(text.index("if c['mode']=='diagnostic-vendor':\n        #"),text.index('    import local_producer'))
        self.assertLess(text.index('dll_inventory.install(files)'),text.index('        vendor_diagnostic.run('))
        compile(text,'native_bootstrap.py','exec')
if __name__=='__main__':unittest.main()
