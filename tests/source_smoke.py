"""Exercise source hub and fake sync using only disposable local files."""
import json, os, subprocess, tempfile, time, urllib.parse
from pathlib import Path
from smoke_helpers import request, json_post, start, BASE, PNG, EXE, ROOT

with tempfile.TemporaryDirectory(prefix='lifeinvader-source-test-') as tmp:
    base=Path(tmp); drive=base/'Drive'; drive.mkdir(); account=drive/'sample-source'; account.mkdir()
    (account/'existing [12345678].mp4').write_bytes(b'0123456789')
    (account/'existing [12345678].info.json').write_text('{"upload_date":"20240102"}',encoding='utf8')
    (account/'slide [12345678] - 01.png').write_bytes(PNG)
    (drive/'download-archive.txt').write_text('tiktok previously-downloaded\n',encoding='utf8')
    (drive/'TikTok Profile Links.txt').write_text('https://www.tiktok.com/@sample-source - already added\nhttps://www.tiktok.com/@newsource - still uploads\n',encoding='utf8')
    other=drive/'other'; other.mkdir(); (other/'private.mp4').write_bytes(b'outside')
    fake=base/'fake-ytdlp.exe'
    command=['rustc']+(['+stable-x86_64-pc-windows-msvc'] if os.name=='nt' else [])+[str(ROOT/'tests'/'fake_ytdlp.rs'),'-o',str(fake)]
    subprocess.run(command,check=True,stdout=subprocess.DEVNULL)
    env=os.environ.copy(); env['LIFEINVADER_DB']=str(base/'test.sqlite3'); env['LIFEINVADER_MEDIA']=str(base/'media'); env['LIFEINVADER_PORT']='3859'; Path(env['LIFEINVADER_MEDIA']).mkdir()
    proc=start(env)
    try:
        status,_=json_post('/api/source-settings',{'root':str(drive),'downloader':str(fake),'photo_helper':str(ROOT/'tests'/'fake_photos.py')});assert status==204
        data=json.loads(request('/api/items')[1]); source=next(p for p in data['profiles'] if p['name']=='sample-source'); sid=source['id']
        listing=json.loads(request(f'/api/sources/{sid}')[1]); assert listing['configured'] and len(listing['files'])==2
        assert {f['kind'] for f in listing['files']}=={'video','image'}
        assert next(f for f in listing['files'] if f['kind']=='video')['posted_date']=='2024-01-02'
        path=next(f['path'] for f in listing['files'] if f['kind']=='video')
        url=f'/api/sources/{sid}/file?'+urllib.parse.urlencode({'path':path})
        code,body=request(url,headers={'Range':'bytes=0-3'});assert code==206 and body==b'0123'
        assert request(f'/api/sources/{sid}/file?'+urllib.parse.urlencode({'path':'other/private.mp4'}))[0]==403
        assert request(f'/api/sources/{sid}/file?'+urllib.parse.urlencode({'path':'../outside.mp4'}))[0]==400
        suggestions=json.loads(request('/api/sources/discover')[1])['sources']; assert [s['name'] for s in suggestions]==['newsource']
        assert json_post(f'/api/sources/{sid}/sync',{'include_photos':False})[0]==202
        assert json_post(f'/api/sources/{sid}/sync',{'include_photos':False})[0]==409
        time.sleep(.3)
        running=json.loads(request(f'/api/sources/{sid}')[1])['last_run']
        assert running['state']=='running' and '__LV_PROGRESS__ 43.2%' in running['log'],running
        for _ in range(100):
            listing=json.loads(request(f'/api/sources/{sid}')[1]); run=listing['last_run']
            if run and run['state']!='running': break
            time.sleep(.05)
        assert run['state']=='finished' and 'fake test file' in run['log'],run
        assert len(listing['files'])==3
        new_file=next(f for f in listing['files'] if 'Fake sample' in f['name'])
        assert new_file['is_new'] and new_file['downloaded_at']
        assert json_post(f'/api/sources/{sid}/seen',{})[0]==204
        assert not next(f for f in json.loads(request(f'/api/sources/{sid}')[1])['files'] if 'Fake sample' in f['name'])['is_new']
        assert (drive/'download-archive.txt').read_text().splitlines()==['tiktok previously-downloaded','tiktok fake-new-id']
        assert json_post(f'/api/sources/{sid}/sync',{'include_photos':False})[0]==202
        for _ in range(100):
            listing=json.loads(request(f'/api/sources/{sid}')[1]);run=listing['last_run']
            if run and run['state']!='running': break
            time.sleep(.05)
        assert run['state']=='finished' and len(listing['files'])==3
        assert (drive/'download-archive.txt').read_text().count('tiktok fake-new-id')==1
        assert json_post(f'/api/sources/{sid}/sync',{'include_photos':True})[0]==202
        for _ in range(100):
            listing=json.loads(request(f'/api/sources/{sid}')[1]);run=listing['last_run']
            if run and run['state']!='running': break
            time.sleep(.05)
        assert run['state']=='finished' and 'fake photo' in run['log'] and len(listing['files'])==4
        assert json_post('/api/source-settings',{'root':str(base/'missing'),'downloader':str(fake),'photo_helper':''})[0]==400
        print('PASS: source folder discovery, video/photo listing, range preview, source boundary, fake sync, archive reuse, concurrency and repeat sync')
    finally:
        proc.terminate(); proc.wait(timeout=5)
