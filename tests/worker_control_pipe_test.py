"""Windows control-only regression: no dependencies, model, media or child process."""
import os,sys,threading,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'helpers/clip_worker'))
import worker_cancel as c

@unittest.skipUnless(os.name=='nt','Windows anonymous pipe')
class ControlPipe(unittest.TestCase):
    def test_quiet_stop_eof_and_malformed_all_bounded(self):
        for payload in (b'STOP\n',None,b'x'):
            with self.subTest(payload=payload):
                c._cancelled.clear() # test isolation only
                read,write=os.pipe()
                stream=os.fdopen(read,'rb',buffering=0)
                thread=threading.Thread(target=c.listen,args=(stream,),daemon=True)
                thread.start()
                try:
                    self.assertFalse(c._cancelled.wait(.06))
                    if payload is None:os.close(write);write=None
                    else:os.write(write,payload)
                    self.assertTrue(c._cancelled.wait(.5));thread.join(.5)
                    self.assertFalse(thread.is_alive())
                    with self.assertRaisesRegex(RuntimeError,'HOLD'):c.checkpoint()
                finally:
                    c.request();thread.join(.5);stream.close()
                    if write is not None:os.close(write)
        c._cancelled.clear()
    def test_invalid_control_fails_closed(self):
        c._cancelled.clear();c.listen(object())
        self.assertTrue(c._cancelled.is_set());c._cancelled.clear()
if __name__=='__main__':unittest.main()
