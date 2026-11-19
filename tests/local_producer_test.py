"""New producer contracts only. Fake tools/model; never executed or loaded."""
import hashlib,pathlib,sys,tempfile,unittest,subprocess
from unittest.mock import patch,MagicMock
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]/'helpers'/'clip_worker'))
import local_producer as p

class ProducerContract(unittest.TestCase):
    def test_explicit_requested_and_effective_backend(self):
        for effective in ('int8','int8_float32'):
            evidence=p.backend_evidence('cpu','int8','cpu',effective)
            self.assertEqual(evidence,dict(requested_device='cpu',requested_compute_type='int8',device='cpu',compute_type=effective))
        self.assertEqual(p.backend_evidence('cuda','float16','cuda','float16')['compute_type'],'float16')
        for values in [('cpu','int8','cpu','float32'),('cpu','int8','cuda','int8_float32'),
                       ('cpu','int8','cpu','int8_float16'),('cuda','float16','cpu','float16'),
                       ('cuda','float16','cuda','int8_float32'),('cpu','auto','cpu','int8'),
                       ('unknown','int8','unknown','int8')]:
            with self.subTest(values=values),self.assertRaisesRegex(ValueError,'backend mismatch'):
                p.backend_evidence(*values)

    def setUp(self):
        self.root=pathlib.Path(tempfile.mkdtemp(prefix='lifeinvader-producer-contract-'))
        model=self.root/'model';model.mkdir();manifest={}
        for name in ('config.json','model.bin','tokenizer.json'):
            (model/name).write_bytes(b'inert');manifest[name]=hashlib.sha256(b'inert').hexdigest()
        tool=self.root/'fake.exe';tool.write_bytes(b'not executable')
        self.c=dict(schema='local-clip-producer-v1',enabled=False,source_sha256='a'*64,cut_key='b'*64,model=str(model),model_files=manifest,
            device='cpu',compute_type='int8',gpu_uuid='GPU-11111111-1111-1111-1111-111111111111',asr_timeout=60,render_timeout=60)
        for n in ('ffmpeg','ffprobe','nvidia_smi'):self.c[n]=str(tool);self.c[n+'_sha256']=p.digest(tool)
    def test_disabled_default_and_missing_dependencies(self):
        with self.assertRaises(ValueError):p.check_config(self.c)
        self.c['enabled']=True
        with patch.object(p.importlib.util,'find_spec',return_value=None):
            with self.assertRaisesRegex(ValueError,'missing local ASR'):p.check_config(self.c)
    def test_exact_local_model_and_tool_pins(self):
        self.c['enabled']=True
        with patch.object(p.importlib.util,'find_spec',return_value=object()):
            p.check_config(self.c)
            (pathlib.Path(self.c['model'])/'extra.json').write_text('{}')
            with self.assertRaisesRegex(ValueError,'inventory'):p.check_config(self.c)
        self.c['ffmpeg_sha256']='0'*64
        with self.assertRaises(ValueError):p.pinned_tool(self.c,'ffmpeg')
    def test_exact_nvenc_output_and_no_overwrite(self):
        args=p.render_command(self.c,self.root/'source.mp4',dict(start_ms=1290,end_ms=33720),self.root/'partial.mp4')
        for key,value in [('-c:v','h264_nvenc'),('-c:a','aac'),('-b:a','128k'),('-ar','48000'),('-ac','2'),('-t','32.430')]:
            self.assertEqual(args[args.index(key)+1],value)
        import export_profile
        self.assertEqual(export_profile.EXPORT[export_profile.EXPORT.index('-b:a')+1], '128k')
        self.assertIn('aac48-128k',export_profile.RECIPE)
        self.assertIn('1080:1920',args[args.index('-vf')+1]);self.assertIn('-n',args);self.assertNotIn('-y',args)
        with self.assertRaises(ValueError):p.render_command(self.c,'source',dict(start_ms=1,end_ms=2),'out')
    def test_timeout_only_stops_owned_child(self):
        child=MagicMock();child.communicate.side_effect=subprocess.TimeoutExpired('inert',1)
        with patch.object(p.subprocess,'Popen') as popen:
            popen.return_value.__enter__.return_value=child
            with self.assertRaises(subprocess.TimeoutExpired):p.bounded(['inert'],self.root,1)
        child.terminate.assert_called_once();child.wait.assert_called_once_with(timeout=3);child.kill.assert_not_called()

if __name__=='__main__':unittest.main()
