import hashlib,json,os
from pathlib import Path
def finish_artifact(queue,owner,pid,claim,artifact,proof,duration,recipe):
 try:
  # Sync completed encoding before issuing proof. No existing proof overwritten.
  size=0
  digest=hashlib.sha256()
  with Path(artifact).open('r+b') as f:
   os.fsync(f.fileno())
   for chunk in iter(lambda:f.read(1048576),b''):
    size+=len(chunk); digest.update(chunk)
  if not size: raise ValueError('empty artifact')
  receipt=dict(job_id=claim['job_id'],clip_id=claim['clip_id'],revision=claim['video_hash'],kind=claim['kind'],sha256=digest.hexdigest(),bytes=size,duration=duration,recipe=recipe,publication_enabled=False)
  with Path(proof).open('x',encoding='utf-8') as f:
   json.dump(receipt,f); f.flush(); os.fsync(f.fileno())
 except Exception:
  queue.finish(owner,claim['job_id'],pid,success=False)
  raise
 # Do not mark failed after ambiguous completion; queue owns its transition.
 queue.finish(owner,claim['job_id'],pid,success=True,receipt_path=proof,artifact_path=artifact)
 return receipt
