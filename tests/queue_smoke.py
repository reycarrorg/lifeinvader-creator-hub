"""Exercise queue reference safety and metadata with disposable synthetic files."""
import json, os, tempfile
from pathlib import Path
from smoke_helpers import request, json_post, start

def get(path):
    code,body=request(path);assert code==200,(code,body)
    return json.loads(body)
with tempfile.TemporaryDirectory(prefix='lifeinvader-queue-test-') as tmp:
    root=Path(tmp); media=root/'media';media.mkdir(); drive=root/'drive';drive.mkdir()
    folder=drive/'sample-source';folder.mkdir();video=folder/'example.mp4';video.write_bytes(b'fake-video')
    (folder/'example.info.json').write_text(json.dumps({'id':'12345','upload_date':'20240102','title':'A title','description':'Original description','webpage_url':'https://www.tiktok.com/@sample-source/video/12345'}))
    env=os.environ.copy();env.update(LIFEINVADER_DB=str(root/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859')
    proc=start(env)
    try:
        profiles=get('/api/items')['profiles'];dest=next(p['id'] for p in profiles if p['role']=='destination');other=next(p['id'] for p in profiles if p['role']=='destination' and p['id']!=dest);src=next(p['id'] for p in profiles if p['name']=='sample-source')
        assert json_post('/api/source-settings',{'root':str(drive),'downloader':'','photo_helper':''})[0]==204
        files=get(f'/api/sources/{src}')['files']; assert files[0]['posted_date']=='2024-01-02' and files[0]['title']=='A title' and files[0]['description']=='Original description'
        path=f'/api/profiles/{dest}/queue';payload={'source_id':src,'path':'sample-source/example.mp4','scheduled_for':'2026-11-19T09:30'}
        code,body=json_post(path,payload);assert code==201,(code,body);qid=json.loads(body)['id']
        assert json_post(path,payload)[0]==409
        assert json_post(f'/api/profiles/{other}/queue',payload)[0]==201
        assert json_post(path,{'source_id':src,'path':'../db.sqlite3'})[0] in (400,403,404)
        row=get(path)[0];assert row['available'] and row['post_id']=='12345' and row['title']=='A title' and row['scheduled_for']=='2026-11-19T09:30'
        assert json_post(f'{path}/{qid}',{'state':'posted','caption':'my caption','post_url':'https://example.com/post','scheduled_for':'2026-11-19T09:30'})[0]==204
        row=get(path)[0];assert row['state']=='posted' and row['caption']=='my caption'
        assert json_post(path,payload)[0]==409
        alt=root/'other-drive';alt.mkdir();assert json_post('/api/source-settings',{'root':str(alt),'downloader':'','photo_helper':''})[0]==204
        assert not get(path)[0]['available']
        code,_=json_post('/api/profiles',{'name':'demo','platform':'youtube','url':'https://www.youtube.com/@demo','role':'source','tags':[]});assert code==201
        yt=next(p['id'] for p in get('/api/items')['profiles'] if p['platform']=='youtube');assert get(f'/api/sources/{yt}')['configured']
        print('PASS: metadata, original file references, duplicate prevention, recorded status, folder change, and YouTube channel acceptance')
    finally:
        proc.terminate();proc.wait(timeout=5)
