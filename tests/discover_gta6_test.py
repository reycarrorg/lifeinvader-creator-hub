import hashlib
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'helpers' / 'clip_worker'))
from discover_gta6 import discover


class DiscoveryTests(unittest.TestCase):
    def source(self, **changes):
        return dict(dict(source_id='synthetic-1', revision='1', topic='gta6',
                         origin='local-owned', source_sha256='a' * 64,
                         rights_basis='owned', authorization_ref='review-1'), **changes)

    def call(self, sources):
        raw = json.dumps(dict(schema='authorized-source-catalog-v1', sources=sources)).encode()
        return discover(raw, approved_catalog_sha256=hashlib.sha256(raw).hexdigest())

    def test_selects_exact_topic_without_mutating_or_granting_authority(self):
        source = self.source()
        self.assertEqual(self.call([source, self.source(source_id='other', topic='other')]), [source])
        self.assertNotIn('render_authorized', source)

    def test_no_unpinned_catalog(self):
        with self.assertRaises(ValueError):
            discover(b'{}', approved_catalog_sha256='0' * 64)

    def test_public_availability_is_not_permission(self):
        for changes in ({'rights_basis': 'public'}, {'origin': 'scraped'},
                        {'authorization_ref': ''}, {'revision': ''}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                self.call([self.source(**changes)])

    def test_duplicate_identity_and_duplicate_json_rejected(self):
        with self.assertRaises(ValueError):
            self.call([self.source(), self.source(revision='2')])
        raw = b'{"schema":"a","schema":"b","sources":[]}'
        with self.assertRaises(ValueError):
            discover(raw, approved_catalog_sha256=hashlib.sha256(raw).hexdigest())


if __name__ == '__main__':
    unittest.main()
