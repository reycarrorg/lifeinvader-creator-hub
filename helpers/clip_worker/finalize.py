import hashlib,json,os,re
from pathlib import Path
STAGES=('transcribing','transcribed','rendering','encoded','poster_verified','verifying_sha256')

def phase_event(root,seq,worker_job_id,cut_key):
 if type(seq) is not int or not 1<=seq<=len(STAGES):raise ValueError('unknown progress sequence')
 if not re.fullmatch(r'[A-Za-z0-9_.-]{1,256}',worker_job_id) or not re.fullmatch(r'[0-9a-f]{64}',cut_key):raise ValueError('progress identity')
 def record(n):
  return dict(schema='worker-progress-v2',seq=n,stage=STAGES[n-1],worker_job_id=worker_job_id,cut_key=cut_key)
 if seq>1:
  with (root/f'progress-{seq-1:02d}.json').open('rb') as previous:
   if json.loads(previous.read(1025))!=record(seq-1):raise ValueError('progress predecessor mismatch')
 dest=root/f'progress-{seq:02d}.json'
 pending=dest.with_suffix('.pending')
 with pending.open('xb') as f:
  f.write(json.dumps(record(seq),separators=(',',':')).encode());f.flush();os.fsync(f.fileno())
 # Windows same-directory rename is atomic and refuses an existing destination.
 if os.name!='nt':raise RuntimeError('Windows progress publication required')
 os.rename(pending,dest)

def finish_artifact(queue,owner,pid,claim,artifact,proof,duration,recipe,caption_status,editorial=None):
 try:
  # Sync completed encoding before issuing proof. No existing proof overwritten.
  size=0
  digest=hashlib.sha256()
  with Path(artifact).open('r+b') as f:
   os.fsync(f.fileno())
   for chunk in iter(lambda:f.read(1048576),b''):
    if not size:phase_event(Path(artifact).parent,6,claim['job_id'],claim['video_hash'])
    size+=len(chunk); digest.update(chunk)
  if not size: raise ValueError('empty artifact')
  receipt=dict(job_id=claim['job_id'],clip_id=claim['clip_id'],revision=claim['video_hash'],kind=claim['kind'],sha256=digest.hexdigest(),bytes=size,duration=duration,recipe=recipe,caption_status=caption_status,publication_enabled=False)
  poster=Path(artifact).with_suffix('.webp')
  with poster.open('rb') as stream:
   length=os.fstat(stream.fileno()).st_size
   if not 0<length<=2097152:raise ValueError('poster size')
   receipt['poster']={'sha256':hashlib.file_digest(stream,'sha256').hexdigest(),'bytes':length,'width':540,'height':960,'format':'webp'}
  if editorial is not None:receipt['editorial_review']=editorial
  with Path(proof).open('x',encoding='utf-8') as f:
   json.dump(receipt,f); f.flush(); os.fsync(f.fileno())
 except Exception:
  queue.finish(owner,claim['job_id'],pid,success=False)
  raise
 # Do not mark failed after ambiguous completion; queue owns its transition.
 queue.finish(owner,claim['job_id'],pid,success=True,receipt_path=proof,artifact_path=artifact)
 return receipt
