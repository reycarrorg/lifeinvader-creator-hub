"""Exercise scheduling and batch sync against disposable local files."""
import json, os, sqlite3, subprocess, tempfile, time
from pathlib import Path
from smoke_helpers import request, json_post, start, ROOT

def get(path):
    code, body = request(path)
    assert code == 200, (path, code, body)
    return json.loads(body)

def until(predicate, timeout=15):
    end=time.monotonic()+timeout
    while time.monotonic()<end:
        value=predicate()
        if value: return value
        time.sleep(.1)
    raise AssertionError('Timed out waiting for scheduler')

with tempfile.TemporaryDirectory(prefix='lifeinvader-schedule-test-') as tmp:
    base=Path(tmp); drive=base/'drive'; drive.mkdir(); fake=base/'fake-ytdlp.exe'
    subprocess.run(['rustc']+(['+stable-x86_64-pc-windows-msvc'] if os.name=='nt' else [])+[str(ROOT/'tests'/'fake_ytdlp.rs'),'-o',str(fake)],check=True)
    env=os.environ.copy(); env.update(LIFEINVADER_DB=str(base/'db.sqlite3'), LIFEINVADER_MEDIA=str(base/'media'), LIFEINVADER_PORT='3859', LV_FAKE_DELAY_MS='600'); Path(env['LIFEINVADER_MEDIA']).mkdir()
    proc=start(env)
    try:
        assert json_post('/api/source-settings',{'root':str(drive),'downloader':str(fake),'photo_helper':''})[0]==204
        profiles=get('/api/items')['profiles']; ids=[p['id'] for p in profiles if p['role']=='source'][:2]; assert len(ids)==2
        a,b=ids
        assert json_post(f'/api/sources/{a}/schedule',{'enabled':True,'interval_seconds':59})[0]==400
        assert json_post(f'/api/sources/{a}/schedule',{'enabled':True,'interval_seconds':60})[0]==204
        schedule=next(x for x in get('/api/source-schedules')['schedules'] if x['profile_id']==a)
        assert schedule['enabled'] and time.time()+50 < schedule['next_run'] <= time.time()+61
        db=sqlite3.connect(env['LIFEINVADER_DB']); db.execute('UPDATE source_schedules SET next_run=unixepoch()-1 WHERE profile_id=?',(a,));db.commit();db.close()
        until(lambda: get(f'/api/sources/{a}')['last_run'] and get(f'/api/sources/{a}')['last_run']['state']=='finished')
        schedule=next(x for x in get('/api/source-schedules')['schedules'] if x['profile_id']==a)
        assert schedule['next_run']>time.time() and schedule['new_count']==1
        files=get(f'/api/sources/{a}')['files']; assert len(files)==1 and files[0]['is_new']
        assert json_post(f'/api/sources/{a}/schedule',{'enabled':False,'interval_seconds':60})[0]==204
        until(lambda: not next(x for x in get('/api/source-schedules')['schedules'] if x['profile_id']==a)['queue_state'])
        assert json_post(f'/api/sources/{b}/batch-setting',{'include':False})[0]==204
        assert not next(x for x in get('/api/source-schedules')['schedules'] if x['profile_id']==b)['include_in_batch']
        code,body=json_post('/api/source-batches',{'profile_ids':[a,b],'include_photos':False}); assert code==202,body
        batch_response=json.loads(body);assert batch_response['added']==1 and batch_response['excluded']==1,batch_response
        until(lambda: (s:=get('/api/source-schedules')['batch']) and s['remaining']==0)
        assert json_post(f'/api/sources/{b}/batch-setting',{'include':True})[0]==204
        code,body=json_post('/api/source-batches',{'profile_ids':[a,b],'include_photos':False}); assert code==202,body
        assert json.loads(body)['added']==2
        until(lambda: (s:=get('/api/source-schedules')['batch']) and s['remaining']==0)
        batch=get('/api/source-schedules')['batch']; assert batch['finished']==2 and batch['errors']==0,batch
        assert (drive/'download-archive.txt').read_text().count('tiktok fake-new-id')==1
        db=sqlite3.connect(env['LIFEINVADER_DB'])
        assert db.execute("SELECT count(*) FROM source_queue WHERE state='finished'").fetchone()[0]==4
        assert db.execute("SELECT count(*) FROM source_sync_runs").fetchone()[0]==4
        db.close()
        print('PASS: automatic schedule, timer reset, source exclusion, serial batch sync, and archive reuse')
    finally:
        proc.terminate();proc.wait(timeout=5)
