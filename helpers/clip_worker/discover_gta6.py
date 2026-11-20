"""Offline authorized-source discovery. No downloader, queue or render authority.

The caller supplies catalog bytes and a separately reviewed digest. Returned
references still require verified intake, cut/rubric review and manual admission.
No keyword search or public availability is treated as reuse permission.
"""
import hashlib
import re
from cut_gate import parse

FIELDS = {'source_id', 'revision', 'topic', 'origin', 'source_sha256',
          'rights_basis', 'authorization_ref'}


def discover(raw, *, approved_catalog_sha256):
    if (type(raw) is not bytes or len(raw) > 1048576 or
            type(approved_catalog_sha256) is not str or
            not re.fullmatch('[0-9a-f]{64}', approved_catalog_sha256) or
            hashlib.sha256(raw).hexdigest() != approved_catalog_sha256):
        raise ValueError('independent source catalog pin required')
    catalog = parse(raw.decode('utf-8'))
    if (type(catalog) is not dict or set(catalog) != {'schema', 'sources'} or
            catalog['schema'] != 'authorized-source-catalog-v1' or
            type(catalog['sources']) is not list or len(catalog['sources']) > 1000):
        raise ValueError('source catalog schema')
    result, seen = [], set()
    for source in catalog['sources']:
        if type(source) is not dict or set(source) != FIELDS:
            raise ValueError('authorized source schema')
        if any(type(v) is not str or not 0 < len(v) <= 512 or
               any(ord(c) < 32 for c in v) for v in source.values()):
            raise ValueError('source field invalid')
        if (source['rights_basis'] not in {'owned', 'licensed', 'permission'} or
                source['origin'] not in {'local-owned', 'authorized-export'} or
                not re.fullmatch('[0-9a-f]{64}', source['source_sha256'])):
            raise ValueError('source authorization required')
        identity = source['source_id']
        if identity in seen:
            raise ValueError('duplicate source identity')
        seen.add(identity)
        if source['topic'] == 'gta6':
            result.append(dict(source))
    return sorted(result, key=lambda s: s['source_id'])
