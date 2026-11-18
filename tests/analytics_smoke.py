"""Test profile analytics with isolated database, no live platform connections."""
import json, os, tempfile
from pathlib import Path
from smoke_helpers import request, json_post, start

def get(path):
    code, body = request(path)
    assert code == 200, (code, body)
    return json.loads(body)

def csv_import(pid, content):
    return request(f'/api/profiles/{pid}/analytics/import','POST',content.encode(),{'Content-Type':'text/csv'})

with tempfile.TemporaryDirectory(prefix='lifeinvader-analytics-') as tmp:
    root=Path(tmp); media=root/'media'; media.mkdir()
    env=os.environ.copy(); env.update(LIFEINVADER_DB=str(root/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859')
    proc=start(env)
    try:
        profiles=get('/api/items')['profiles']; first,second=[p['id'] for p in profiles if p['role']=='destination']; source=next(p['id'] for p in profiles if p['role']=='source')
        path=f'/api/profiles/{first}/analytics'
        assert get(path)==[]
        assert json_post(path,{'date':'2026-11-17','followers':100,'views':1000})[0]==204
        assert json_post(path,{'date':'2026-11-18','followers':110})[0]==204
        assert json_post(path,{'date':'2026-11-18','post_id':'abc','views':50,'likes':3})[0]==204
        assert json_post(path,{'date':'2026-11-18','post_id':'abc','followers':3})[0]==400
        assert json_post(path,{'date':'2026-11-18','views':-1})[0]==400
        assert json_post(f'/api/profiles/{source}/analytics',{'date':'2026-11-18','views':2})[0]==404
        assert get(f'/api/profiles/{second}/analytics')==[]
        good='date,post_id,followers,following,views,likes,comments,shares\n2026-11-18,,111,,,,,\n2026-11-18,abc,,,,4,1,\n'
        code,body=csv_import(first,good);assert code==200 and json.loads(body)['imported']==2,(code,body)
        assert csv_import(first,good)[0]==200
        rows=get(path);assert len(rows)==3
        account=next(r for r in rows if r['date']=='2026-11-18' and not r['post_id']);assert account['followers']==111
        post=next(r for r in rows if r['post_id']=='abc');assert post['views']==50 and post['likes']==4 and post['comments']==1
        invalid='date,followers\n2026-11-19,120\n2026-11-20,-1\n'
        assert csv_import(first,invalid)[0]==400
        assert len(get(path))==3
        assert csv_import(first,'garbage,value\na,1\n')[0]==400
        print('PASS: isolated profiles, snapshot validation, atomic CSV import, repeat import and missing-value preservation')
    finally:
        proc.terminate();proc.wait(timeout=5)
