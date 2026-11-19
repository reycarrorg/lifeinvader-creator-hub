"""Identity rejection only: never import an ASR dependency or process media."""
import pathlib,sys,tempfile,json,hashlib,unittest
from unittest.mock import patch
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]/'helpers/clip_worker'))
import asr_bootstrap,local_producer

class Identity(unittest.TestCase):
    def test_missing_identity_rejects_before_payload_or_queue(self):
        with patch.object(local_producer,'_runtime_identity',None):
            with self.assertRaisesRegex(ValueError,'identity'):local_producer.asr_child({})
            with self.assertRaisesRegex(ValueError,'identity'):local_producer.run_local({},'unopened',{})
        with self.assertRaises(ValueError):asr_bootstrap.verify(None)
    def test_config_tamper_refused_before_import(self):
        root=pathlib.Path(tempfile.mkdtemp(prefix='asr-identity-'));m=root/'manifest.json';c=root/'config.json'
        m.write_text('{"schema":"native-runtime-files-v1","files":{}}');c.write_text('{}')
        h=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
        identity=dict(manifest=str(m),manifest_sha256=h(m),bootstrap_config=str(c),bootstrap_config_sha256=h(c))
        c.write_text('{"changed":true}')
        with self.assertRaisesRegex(ValueError,'identity bytes'):asr_bootstrap.verify(identity)

if __name__=='__main__':unittest.main()
