import hashlib,re
def bounds(trusted,candidates):
 # trusted must come from probe/transcript pipeline, never proposed candidate.
 duration=trusted['duration_ms']
 segments=trusted['segments']
 out=[]
 if not isinstance(candidates,list) or not candidates:raise ValueError('empty candidate list')
 for c in candidates:
  silent=trusted['caption_status']=='no_speech'
  fields={'source_sha256','start_ms','end_ms'} if silent else {'source_sha256','speech_id'}
  if not isinstance(c,dict) or set(c)!=fields:raise ValueError('candidate schema')
  if not isinstance(c['source_sha256'],str) or not re.fullmatch('[a-f0-9]{64}',c['source_sha256']) or c['source_sha256']!=trusted['source_sha256']:raise ValueError('wrong source')
  if silent:
   if segments!={}:raise ValueError('no_speech intake has segments')
   start,end=c['start_ms'],c['end_ms']
  else:
   if not isinstance(c['speech_id'],str) or c['speech_id'] not in segments:raise ValueError('ungrounded candidate')
   start,end=segments[c['speech_id']]
  if not all(type(t) is int for t in (start,end,duration)) or not 0<=start<end<=duration:raise ValueError('invalid grounded time')
  if not silent:start-=350;end+=500
  if not 0<=start<end<=duration or not 0<end-start<=60000:raise ValueError('boundaries or duration')
  out.append((start,end))
 out.sort()
 if any(a[1]>b[0] for a,b in zip(out,out[1:])):raise ValueError('overlap')
 return out
def gate(trusted,candidates,actual_hash,producer):
 if trusted['source_sha256']!=actual_hash:raise ValueError('input checksum mismatch')
 times=bounds(trusted,candidates)
 if len(times)!=1:raise ValueError('isolated worker expects exactly one cut')
 return producer(*times[0])
def parse(text):
 import json
 def pairs(items):
  out={}
  for key,value in items:
   if key in out:raise ValueError('duplicate JSON key')
   out[key]=value
  return out
 return json.loads(text,object_pairs_hook=pairs)
