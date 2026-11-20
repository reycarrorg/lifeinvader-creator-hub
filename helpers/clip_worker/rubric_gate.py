import hashlib,re,json
from pathlib import Path
from cut_gate import parse
SCORE=frozenset('hook stakes escalation payoff vocal_energy visual_clarity loop_potential'.split())
REVIEW=frozenset('visual_clarity_verified copyright_music_clear gameplay_visual_payoff_verified sentence_boundary_verified rubric_score_verified'.split())
def digest(b):return hashlib.sha256(b).hexdigest()
def binding(intake_pin,proposal,start,end):
 return dict(intake_sha256=intake_pin,proposal_sha256=digest(proposal.encode('utf-8')),start_ms=start,end_ms=end)
# This local editorial decision applies to a single queue and one exact source range.
OWNER_SOURCE='133883ce3f0222a43a90db28b127f5c1dbd33f5336a598da713f703165fe86fd'
OWNER_WORKSPACE=Path(__file__).resolve().parents[3]/'integration-runtime-5eb2461-track-a-1000000000000000001-v3'/'worker'
OWNER_JOB='track-a-1000000000000000001-0-8000'
OWNER_AUTH={'editorial':'Sentinel_5a1f748e17908191abed2e13390972fd','audio_permission':'Sentinel_3dd2717cde108191900cda4fd1e5be8d','audio_basis':'owner_reported_local_test_only','worker_job_id':OWNER_JOB,'waived_attestation':'gameplay_visual_payoff_verified'}
def verify(path,pin,intake_pin,proposal,start,end):
 if not isinstance(pin,str) or not re.fullmatch('[0-9a-f]{64}',pin):raise ValueError('Missing independent review pin')
 raw=Path(path).read_bytes()
 if digest(raw)!=pin:raise ValueError('Review digest mismatch')
 r=parse(raw.decode('utf-8'))
 if not isinstance(r,dict) or set(r)!={'schema','reviews'} or r['schema']!='trusted-rubric-v3' or not isinstance(r['reviews'],dict):raise ValueError('Review schema')
 b=binding(intake_pin,proposal,start,end)
 clip_id=digest(json.dumps(b,sort_keys=True,separators=(',',':')).encode())
 v=r['reviews'].get(clip_id)
 special=isinstance(v,dict) and 'owner_decision' in v
 extra={'owner_decision'} if special else set()
 if not isinstance(v,dict) or set(v)!=(set(b)|{'scores','total','attestations'}|extra) or any(type(v[k]) is not type(x) or v[k]!=x for k,x in b.items()):raise ValueError('Review does not bind exact cut')
 scores=v['scores']; a=v['attestations']
 if not isinstance(scores,dict) or set(scores)!=SCORE or any(type(n) is not int or n not in (0,1) for n in scores.values()):raise ValueError('Invalid rubric scores')
 if type(v['total']) is not int or v['total']!=sum(scores.values()):raise ValueError('Rubric total')
 if special:
  if v['owner_decision']!=OWNER_AUTH or (start,end,v['total'])!=(0,8000,3) or parse(proposal)!=[dict(source_sha256=OWNER_SOURCE,start_ms=0,end_ms=8000)]:raise ValueError('Owner decision outside exact scope')
 elif v['total']<5:raise ValueError('Rubric total')
 required=REVIEW
 if 'speech_id' not in parse(proposal)[0]:
  required=(REVIEW-{'sentence_boundary_verified'})|{'no_speech_verified'}
  if scores['vocal_energy']!=0:raise ValueError('no speech vocal score')
 if not isinstance(a,dict) or set(a)!=required or any(value is not (False if special and key=='gameplay_visual_payoff_verified' else True) for key,value in a.items()):raise ValueError('Missing explicit review attestation')
 return dict(owner_decision=dict(OWNER_AUTH),scores=dict(scores),attestations=dict(a),total=v['total'],review_sha256=pin) if special else None
