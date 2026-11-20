import unittest
from runtime_layout import plan

class LayoutTests(unittest.TestCase):
    def test_mapping_keeps_state_out_and_does_not_authorize(self):
        p=plan({'C:/base/python.exe':'a'*64,'C:/state/job.json':'b'*64},
               {'C:/base':'runtime/python'}, {'C:/state':'job input, never installed'})
        self.assertEqual(p['files'][0]['destination'],'runtime/python/python.exe')
        self.assertEqual(len(p['excluded']),1)
        self.assertFalse(p['qualified']);self.assertFalse(p['launch_enabled'])
    def test_rejects_unknown_collisions_and_escape(self):
        with self.assertRaises(ValueError):plan({'C:/unknown/x':'a'*64},{},{})
        with self.assertRaises(ValueError):plan({'C:/a/x':'a'*64,'C:/b/x':'b'*64},{'C:/a':'p','C:/b':'p'},{})
        with self.assertRaises(ValueError):plan({'C:/a/x':'a'*64},{'C:/a':'../p'},{})

if __name__=='__main__':unittest.main()
