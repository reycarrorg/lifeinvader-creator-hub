import hashlib,re,json
from pathlib import Path
from cut_gate import parse
SCORE=frozenset('hook stakes escalation payoff vocal_energy visual_clarity loop_potential'.split())
REVIEW=frozenset('visual_clarity_verified copyright_music_clear gameplay_visual_payoff_verified sentence_boundary_verified rubric_score_verified'.split())
def digest(b):return hashlib.sha256(b).hexdigest()
def binding(intake_pin,proposal,start,end):
 return dict(intake_sha256=intake_pin,proposal_sha256=digest(proposal.encode('utf-8')),start_ms=start,end_ms=end)
def verify(path,pin,intake_pin,proposal,start,end):
 if not isinstance(pin,str) or not re.fullmatch('[0-9a-f]{64}',pin):raise ValueError('Missing independent review pin')
 raw=Path(path).read_bytes()
 if digest(raw)!=pin:raise ValueError('Review digest mismatch')
 r=parse(raw.decode('utf-8'))
 if not isinstance(r,dict) or set(r)!={'schema','reviews'} or r['schema']!='trusted-rubric-v1' or not isinstance(r['reviews'],dict):raise ValueError('Review schema')
 b=binding(intake_pin,proposal,start,end)
 clip_id=digest(json.dumps(b,sort_keys=True,separators=(',',':')).encode())
 v=r['reviews'].get(clip_id)
 if not isinstance(v,dict) or set(v)!=(set(b)|{'scores','total','attestations'}) or any(type(v[k]) is not type(x) or v[k]!=x for k,x in b.items()):raise ValueError('Review does not bind exact cut')
 scores=v['scores']; a=v['attestations']
 if not isinstance(scores,dict) or set(scores)!=SCORE or any(type(n) is not int or n not in (0,1) for n in scores.values()):raise ValueError('Invalid rubric scores')
 if type(v['total']) is not int or v['total']!=sum(scores.values()) or v['total']<5:raise ValueError('Rubric total')
 if not isinstance(a,dict) or set(a)!=REVIEW or any(x is not True for x in a.values()):raise ValueError('Missing explicit review attestation')
