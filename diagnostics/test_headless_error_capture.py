import unittest
from headless_review_run import sanitized_http_error
class Capture(unittest.TestCase):
    def test_known_reason_only(self):
        r=sanitized_http_error(409,b'{"reason":"source_unavailable","secret":"never print this"}')
        self.assertEqual(r['reason'],'source_unavailable');self.assertNotIn('never print',str(r))
    def test_unknown_malformed_or_oversize_redacted(self):
        for raw in (b'{"reason":"private-value"}',b'private stderr',b'x'*4097):
            r=sanitized_http_error(409,raw);self.assertEqual(r['reason'],'UNCLASSIFIED')
            self.assertNotIn('private',str(r));self.assertNotIn('stdout',r)
if __name__=='__main__':unittest.main()
