"""Non-render evidence routing regression; retains its disposable evidence."""
import pathlib
import tempfile
import unittest
from unittest.mock import patch
from headless_review_run import create_attempt_evidence


class EvidenceIsolation(unittest.TestCase):
    def test_create_only_and_reparse_rejection(self):
        root = pathlib.Path(tempfile.mkdtemp(prefix='lifeinvader-evidence-test-'))
        (root / 'preflight').mkdir()
        old = root / 'preflight' / 'headless-run.jsonl'
        old.write_bytes(b'preserved')
        target = create_attempt_evidence(root, 'attempt-01')
        self.assertEqual(list(target.iterdir()), [])
        with self.assertRaises(FileExistsError):
            create_attempt_evidence(root, 'attempt-01')
        for value in ('../escape', 'attempt-01/child'):
            with self.assertRaises(ValueError):
                create_attempt_evidence(root, value)
        original = pathlib.Path.lstat
        def reparse(path):
            if path == root / 'preflight':
                class Reparse:
                    st_mode = 0o40700
                    st_file_attributes = 0x400
                return Reparse()
            return original(path)
        with patch.object(pathlib.Path, 'lstat', reparse):
            with self.assertRaisesRegex(RuntimeError, 'reparse'):
                create_attempt_evidence(root, 'attempt-02')
        self.assertFalse((root / 'preflight' / 'attempt-02').exists())
        self.assertEqual(old.read_bytes(), b'preserved')


if __name__ == '__main__':
    unittest.main()
