"""Authorized encoder-only policy, synthetic values; no DLL or hardware calls."""
import pathlib,sys,unittest
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]/'helpers/clip_worker'))
from gpu_guard import encoder_admission
GPU='GPU-11111111-1111-1111-1111-111111111111'
def sample(**kw):
    s=dict(uuid=GPU,start_ms=1000,end_ms=1001,session_count=0,enumerated_count=0,encoder_percent=10,sampling_period_us=1000000);s.update(kw);return s
class Policy(unittest.TestCase):
    def test_threshold_inclusive(self):
        for n in (0,1,10):self.assertEqual(encoder_admission(sample(encoder_percent=n),GPU,1002),'QUALIFIED')
        self.assertEqual(encoder_admission(sample(encoder_percent=11),GPU,1002),'ENCODER_BUSY')
    def test_any_encoder_session_blocks_even_zero_util(self):
        self.assertEqual(encoder_admission(sample(session_count=1,enumerated_count=1,encoder_percent=0),GPU,1002),'ENCODER_BUSY')
    def test_count_disagreement_is_not_idle(self):
        self.assertEqual(encoder_admission(sample(enumerated_count=1),GPU,1002),'SESSION_DISAGREEMENT')
    def test_stale_future_invalid_fail_closed(self):
        for s in (sample(start_ms=0),sample(end_ms=2000),sample(start_ms=1002)):
            self.assertEqual(encoder_admission(s,GPU,6001),'STALE')
        for k,v in [('encoder_percent',True),('encoder_percent',101),('sampling_period_us',0),('sampling_period_us',5000001),('session_count',-1)]:
            self.assertEqual(encoder_admission(sample(**{k:v}),GPU,1002),'UNKNOWN')
    def test_wrong_target_unknown_extra_fields(self):
        for s in (sample(uuid='wrong'),dict(sample(),resident_rows=[]),{},None):
            self.assertNotEqual(encoder_admission(s,GPU,1002),'QUALIFIED')
if __name__=='__main__':unittest.main()
