import hashlib,tempfile,unittest
from pathlib import Path
from final_paths import pinned_final_file
class Paths(unittest.TestCase):
    def setUp(self):
        self.base=Path(tempfile.mkdtemp(prefix='lifeinvader-final-paths-')).resolve()
        self.root=self.base/'payload';self.root.mkdir();self.file=self.root/'model.bin';self.file.write_bytes(b'inert')
        self.pins={'model.bin':hashlib.sha256(b'inert').hexdigest()}
    def test_final_pinned_file(self):self.assertEqual(pinned_final_file(self.root,self.file,self.pins),self.file)
    def test_original_same_bytes_never_substituted(self):
        old=self.base/'original-model.bin';old.write_bytes(b'inert')
        with self.assertRaises(ValueError):pinned_final_file(self.root,old,self.pins)
        with self.assertRaises(ValueError):pinned_final_file(self.root,'model.bin',self.pins)
    def test_missing_or_changed_is_hold(self):
        with self.assertRaises(FileNotFoundError):pinned_final_file(self.root,self.root/'missing.bin',self.pins)
        with self.assertRaises(ValueError):pinned_final_file(self.root,self.file,{})
        self.file.write_bytes(b'changed')
        with self.assertRaises(ValueError):pinned_final_file(self.root,self.file,self.pins)
if __name__=='__main__':unittest.main()
