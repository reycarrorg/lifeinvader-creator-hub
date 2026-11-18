"""Test personal-profile sync with a fake downloader and isolated folders."""
import json,os,sqlite3,subprocess,tempfile,time,urllib.parse
from pathlib import Path
from smoke_helpers import request,json_post,start,ROOT

def get(path):
    code,body=request(path);assert code==200,(path,code,body)
    return json.loads(body)
def wait_done(pid):
    for _ in range(150):
        result=get(f'/api/profiles/{pid}/downloads')
        if result['last_run'] and result['last_run']['state']!='running':return result
        time.sleep(.05)
    raise AssertionError('Profile sync timed out')

with tempfile.TemporaryDirectory(prefix='lifeinvader-own-sync-') as tmp:
    base=Path(tmp);drive=base/'drive';drive.mkdir();media=base/'media';media.mkdir()
    fake=base/'fake-ytdlp.exe'
    subprocess.run(['rustc']+(['+stable-x86_64-pc-windows-msvc'] if os.name=='nt' else [])+[str(ROOT/'tests'/'fake_profile_ytdlp.rs'),'-o',str(fake)],check=True)
    env=os.environ.copy();env.update(LIFEINVADER_DB=str(base/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859',LIFEINVADER_GALLERY_DL=str(ROOT/'tests'/'fake_gallery'))
    proc=start(env)
    try:
        assert json_post('/api/source-settings',{'root':str(drive),'downloader':str(fake),'photo_helper':''})[0]==204
        profiles=get('/api/items')['profiles'];own=next(p for p in profiles if p['role']=='destination' and p['platform']=='tiktok');src=next(p for p in profiles if p['role']=='source');pid=own['id']
        empty=get(f'/api/profiles/{pid}/downloads');assert empty['configured'] and empty['files']==[]
        assert request(f'/api/sources/{pid}')[0]==400 and request(f'/api/profiles/{src["id"]}/downloads')[0]==400
        assert json_post(f'/api/profiles/{pid}/download-sync',{'include_photos':False})[0]==202
        assert json_post(f'/api/profiles/{pid}/download-sync',{'include_photos':False})[0]==409
        first=wait_done(pid);assert first['last_run']['state']=='finished',first['last_run']
        f=first['files'][0];assert f['post_id']=='1234567890123456789' and f['posted_date']=='2024-01-02' and f['is_new']
        assert f['path'].startswith('_My Profiles/tiktok/profile-')
        address=f'/api/profiles/{pid}/download-file?'+urllib.parse.urlencode({'path':f['path']})
        assert request(address)[1]==b'video-bytes'
        assert request(f'/api/sources/{src["id"]}/file?'+urllib.parse.urlencode({'path':f['path']}))[0]==403
        assert json_post(f'/api/profiles/{pid}/download-sync',{'include_photos':False})[0]==202
        second=wait_done(pid);assert second['last_run']['state']=='finished' and len(second['files'])==1
        archive=drive/('_My Profiles/tiktok/profile-'+str(pid))/'download-archive.txt';assert archive.read_text().count('1234567890123456789')==1
        db=sqlite3.connect(env['LIFEINVADER_DB']);rows=db.execute('SELECT platform,post_id,path FROM profile_downloads').fetchall();db.close()
        assert rows==[('tiktok','1234567890123456789',f['path'])],rows
        assert not (drive/'download-archive.txt').exists()
        instagram=next(p for p in profiles if p['role']=='destination' and p['platform']=='instagram');iid=instagram['id']
        assert get(f'/api/profiles/{iid}/downloads')['photos_supported']
        assert json_post(f'/api/profiles/{iid}/download-sync',{'include_photos':True})[0]==202
        result=wait_done(iid);assert result['last_run']['state']=='finished',result['last_run']
        image=next(f for f in result['files'] if f['kind']=='image');assert image['post_id']=='post-code' and image['asset_id']=='image-1' and image['posted_date']=='2024-05-06'
        assert json_post(f'/api/profiles/{iid}/download-sync',{'include_photos':True})[0]==202
        assert len(wait_done(iid)['files'])==2
        db=sqlite3.connect(env['LIFEINVADER_DB']);rows=db.execute('SELECT platform,post_id,asset_id FROM profile_download_files WHERE profile_id=?',(iid,)).fetchall();db.close()
        assert ('instagram','post-code','image-1') in rows
        print('PASS: profile-only folder, sync, media identity, archive reuse, and account boundaries')
    finally:
        proc.terminate();proc.wait(timeout=5)
