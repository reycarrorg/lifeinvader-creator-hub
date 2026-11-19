"""Removing accounts is reversible and preserves files and queue references."""
import json, os, sqlite3, tempfile
from contextlib import closing
from pathlib import Path
from smoke_helpers import request, json_post, start

def get(path):
    code,body=request(path); assert code==200,(code,body); return json.loads(body)
with tempfile.TemporaryDirectory(prefix='lifeinvader-remove-') as tmp:
    root=Path(tmp); media=root/'media';media.mkdir(); drive=root/'drive';drive.mkdir()
    folder=drive/'sample-source';folder.mkdir();video=folder/'example.mp4';video.write_bytes(b'fake-video')
    dbpath=root/'db.sqlite3';env=os.environ.copy();env.update(LIFEINVADER_DB=str(dbpath),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859')
    proc=start(env)
    try:
        accounts=get('/api/items')['profiles']; source=next(p for p in accounts if p['name']=='sample-source'); dest=next(p for p in accounts if p['role']=='destination'); sid=source['id']; pid=dest['id']
        assert json_post('/api/source-settings',{'root':str(drive),'downloader':'','photo_helper':''})[0]==204
        assert json_post(f'/api/profiles/{pid}/queue',{'source_id':sid,'path':'sample-source/example.mp4'})[0]==201
        assert json_post(f'/api/sources/{sid}/schedule',{'enabled':True,'interval_seconds':3600})[0]==204
        assert json_post(f'/api/profiles/{sid}/analytics-bridge',{'enabled':True,'interval_seconds':3600,'after_post':False})[0]==204
        assert json_post(f'/api/profiles/{sid}/archive',{'archived':True})[0]==204
        assert sid not in [p['id'] for p in get('/api/items')['profiles']]
        assert sid in [p['id'] for p in get('/api/profiles/hidden')]
        assert get(f'/api/profiles/{pid}/queue')[0]['source_id']==sid
        assert video.read_bytes()==b'fake-video'
        with closing(sqlite3.connect(dbpath)) as db:
            assert db.execute('select enabled from source_schedules where profile_id=?',(sid,)).fetchone()[0]==0
            assert db.execute('select enabled from analytics_bridge where profile_id=?',(sid,)).fetchone()[0]==0
        assert json_post(f'/api/sources/{sid}/schedule',{'enabled':True,'interval_seconds':3600})[0]==400
        assert json_post(f'/api/sources/{sid}/sync',{'include_photos':False})[0]==400
        assert json_post('/api/categories/members',{'role':'source','name':'Sources','profile_ids':[]})[0]==204
        assert json_post(f'/api/profiles/{sid}/archive',{'archived':False})[0]==204
        restored=next(p for p in get('/api/items')['profiles'] if p['id']==sid)
        assert 'Sources' in restored['tags']
        assert get(f'/api/profiles/{pid}/queue')[0]['source_id']==sid
        assert not get('/api/profiles/hidden')
        with closing(sqlite3.connect(dbpath)) as db:
            db.execute("insert into source_sync_runs(profile_id,state,log_file) values(?,'running','test')",(sid,));db.commit()
        assert json_post(f'/api/profiles/{sid}/archive',{'archived':True})[0]==409
        assert sid in [p['id'] for p in get('/api/items')['profiles']]
        assert json_post('/api/profiles/999999/archive',{'archived':True})[0]==404
        print('PASS: removable and restorable accounts, schedules disabled, category, queue and media preserved, running sync guarded')
    finally:
        proc.terminate();proc.wait(timeout=5)
