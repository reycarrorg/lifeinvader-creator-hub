"""Create only a new synthetic completion for the Rust integration test."""
import hashlib,json,sqlite3
from clip_prepare_test import Preparation
from stage_queue import run_reviewed
from export_profile import RECIPE

def main():
    fixture=Preparation();fixture.setUp();c=fixture.c;root=fixture.root
    starts=[]
    def inert(plan,out):
        starts.append(1)
        with out.open('xb') as f:f.write(b'external synthetic completed bytes; not playable video')
        return '32.430'
    receipt=run_reviewed(c,root,producer=inert)
    before=(root/'queue.sqlite').read_bytes()
    def forbidden(*args):raise AssertionError('duplicate started producer')
    assert run_reviewed(c,root,producer=forbidden)==receipt
    assert before==(root/'queue.sqlite').read_bytes()
    with sqlite3.connect(root/'queue.sqlite') as db:
        assert db.execute("SELECT count(*) FROM events WHERE event='claimed'").fetchone()[0]==1
        assert db.execute('SELECT count(*) FROM manager').fetchone()[0]==0
        assert db.execute('SELECT count(*) FROM completion_proofs').fetchone()[0]==1
    assert len(starts)==1
    sidecar=root/(c['cut_key']+'.receipt.json')
    print(json.dumps(dict(queue=str(root/'queue.sqlite'),receipt=str(sidecar),artifact=str(root/(c['cut_key']+'.mp4')),
        receipt_sha256=hashlib.sha256(sidecar.read_bytes()).hexdigest(),artifact_sha256=receipt['sha256'],artifact_bytes=receipt['bytes'],
        selection=dict(generation=1,provider='synthetic',provider_id='1',source_sha256=c['source_sha256'],source_revision='synthetic1',start_ms=1290,end_ms=33720,recipe_version=RECIPE,durable_cut_key=c['cut_key']))))

if __name__=='__main__':main()
