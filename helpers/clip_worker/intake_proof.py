import hashlib,re
from pathlib import Path
from cut_gate import parse,gate
def dispatch(*,sealed_record,expected_record_sha256,artifact,proposal,producer):
 # Record path and digest come from trusted dispatcher, never from proposal.
 if not isinstance(expected_record_sha256,str) or not re.fullmatch('[a-f0-9]{64}',expected_record_sha256):raise ValueError('missing independent intake pin')
 raw=Path(sealed_record).read_bytes()
 if hashlib.sha256(raw).hexdigest()!=expected_record_sha256:raise ValueError('intake record digest mismatch')
 r=parse(raw.decode('utf-8'))
 if not isinstance(r,dict) or set(r)!={'schema','status','source_sha256','bytes','duration_ms','segments'} or r['schema']!='verified-intake-v1' or r['status']!='verified':raise ValueError('no verified intake')
 if type(r['bytes']) is not int or r['bytes']<=0:raise ValueError('invalid intake byte count')
 h=hashlib.sha256();size=0
 with Path(artifact).open('rb') as f:
  for c in iter(lambda:f.read(1048576),b''):
   h.update(c);size+=len(c)
 if size!=r['bytes']:raise ValueError('source size differs from verified intake')
 return gate(r,parse(proposal),h.hexdigest(),producer)
