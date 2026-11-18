"""Local end-to-end smoke check with synthetic media and an isolated temporary database."""
import concurrent.futures
import json
import os
import sqlite3
from pathlib import Path
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[1]
EXE = ROOT / 'target' / 'debug' / ('lifeinvader_hub.exe' if os.name == 'nt' else 'lifeinvader_hub')
BASE = 'http://127.0.0.1:3859'
PNG = bytes.fromhex('89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c6360f8cfc0000004010100c9fe92ef0000000049454e44ae426082')

def req(path, method='GET', data=None, headers=None):
    try:
        with urllib.request.urlopen(urllib.request.Request(BASE + path, method=method, data=data, headers=headers or {}), timeout=5) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()

def json_post(path, data):
    return req(path, 'POST', json.dumps(data).encode(), {'Content-Type':'application/json'})

def upload(post_id, date, count=1):
    boundary = 'test' + uuid.uuid4().hex
    parts = []
    for key, value in {'platform':'tiktok','account':'testcreator','post_id':post_id,'posted':date,'caption':'test'}.items():
        parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="{key}"\r\n\r\n{value}\r\n'.encode())
    for i in range(count):
        parts.extend([f'--{boundary}\r\nContent-Disposition: form-data; name="files"; filename="test{i}.png"\r\nContent-Type: image/png\r\n\r\n'.encode(),PNG,b'\r\n'])
    parts.append(f'--{boundary}--\r\n'.encode())
    return req('/api/items','POST',b''.join(parts),{'Content-Type':f'multipart/form-data; boundary={boundary}'})

def photo(profile_id, content):
    boundary='test'+uuid.uuid4().hex
    data=(f'--{boundary}\r\nContent-Disposition: form-data; name="photo"; filename="picture.png"\r\nContent-Type: image/png\r\n\r\n'.encode()+content+f'\r\n--{boundary}--\r\n'.encode())
    return req(f'/api/profiles/{profile_id}/photo','POST',data,{'Content-Type':f'multipart/form-data; boundary={boundary}'})

def start(env):
    p = subprocess.Popen([str(EXE), '--server-only'], cwd=ROOT, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    for _ in range(100):
        if p.poll() is not None: raise RuntimeError(f'server exited: {p.stderr.read().decode()}')
        try:
            if req('/api/items')[0] == 200: return p
        except OSError: pass
        time.sleep(.05)
    p.terminate(); raise RuntimeError('server did not start')

with tempfile.TemporaryDirectory(prefix='lifeinvader-test-') as tmp:
    env=os.environ.copy(); env['LIFEINVADER_DB']=str(Path(tmp)/'test.sqlite3'); env['LIFEINVADER_MEDIA']=str(Path(tmp)/'media'); env['LIFEINVADER_PORT']='3859'; Path(env['LIFEINVADER_MEDIA']).mkdir()
    # A database written by the previous version must retain its per-platform history.
    with sqlite3.connect(env['LIFEINVADER_DB']) as legacy:
        legacy.executescript("""CREATE TABLE settings (key TEXT PRIMARY KEY,value TEXT NOT NULL);
        CREATE TABLE items (id INTEGER PRIMARY KEY,platform TEXT NOT NULL,account TEXT NOT NULL,post_id TEXT NOT NULL,source_url TEXT NOT NULL,posted TEXT NOT NULL,caption TEXT NOT NULL,UNIQUE(platform,post_id));
        CREATE TABLE files (id INTEGER PRIMARY KEY,item_id INTEGER NOT NULL,name TEXT NOT NULL,path TEXT NOT NULL,kind TEXT NOT NULL);
        CREATE TABLE statuses (item_id INTEGER NOT NULL,destination TEXT NOT NULL,state TEXT NOT NULL DEFAULT 'available',claimed_by TEXT NOT NULL DEFAULT '',updated_by TEXT NOT NULL DEFAULT '',caption TEXT NOT NULL DEFAULT '',post_url TEXT NOT NULL DEFAULT '',PRIMARY KEY(item_id,destination));
        INSERT INTO items VALUES(1,'tiktok','legacycreator','oldpost','','','original');
        INSERT INTO statuses VALUES(1,'tiktok','reported_posted','Alice','Alice','old caption','');
        INSERT INTO statuses VALUES(1,'instagram','claimed','Bob','Bob','','');""")
    legacy.close()
    proc=start(env)
    try:
        initial=json.loads(req('/api/items')[1]); assert len(initial['profiles'])==2
        legacy=next(i for i in initial['items'] if i['post_id']=='oldpost')
        assert {s['destination']:s['state'] for s in legacy['statuses']}=={'tiktok':'reported_posted','instagram':'claimed'}
        payload={'name':'secondaccount','platform':'tiktok','url':'https://www.tiktok.com/@secondaccount','role':'destination','tags':['Gaming','Lifeinvader']}
        code,body=json_post('/api/profiles',payload); assert code==201
        second=json.loads(body)['id']; assert json_post('/api/profiles',payload)[0]==409
        payload['tags']=['Gaming','Personal']; assert json_post(f'/api/profiles/{second}',payload)[0]==204
        info=json.loads(req('/api/items')[1]); assert next(p for p in info['profiles'] if p['id']==second)['tags']==['Gaming','Personal']
        assert photo(second,b'not an image')[0]==400
        assert photo(second,PNG)[0]==204
        avatar=next(p for p in json.loads(req('/api/items')[1])['profiles'] if p['id']==second)['avatar_url']
        assert req(avatar)[1]==PNG
        legacy=next(i for i in info['items'] if i['post_id']=='oldpost')
        assert next(s for s in legacy['statuses'] if s['profile_id']==second)['state']=='available'
        assert upload('00001234567890123456789','2026-11-18',2)[0] == 201
        assert upload('00001234567890123456789','2026-11-18')[0] == 409
        assert upload('unknown','')[0] == 201
        assert upload('launch-day','2026-11-19')[0] == 201
        assert upload('exact','2026-11-19T06:00:00Z')[0] == 201
        d=json.loads(req('/api/items')[1]); by_id={x['post_id']:x for x in d['items']}
        assert by_id['00001234567890123456789']['category']=='before'
        assert by_id['unknown']['category']=='review' and by_id['launch-day']['category']=='review'
        assert by_id['exact']['category']=='after'
        item=by_id['00001234567890123456789']; assert len(item['files'])==2
        assert req(item['files'][0]['url'])[1] == PNG
        def claim(user): return json_post(f"/api/items/{item['id']}/status",{'destination':'tiktok','action':'claim','user':user})[0]
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool: outcomes=list(pool.map(claim,['Alice','Bob']))
        assert sorted(outcomes)==[204,409], outcomes
        item=next(x for x in json.loads(req('/api/items')[1])['items'] if x['id']==item['id']); winner=next(s for s in item['statuses'] if s['destination']=='tiktok')['claimed_by']
        assert json_post(f"/api/items/{item['id']}/status",{'destination':'instagram','action':'claim','user':'Other'})[0]==204
        assert json_post(f"/api/items/{item['id']}/status",{'destination':'tiktok','action':'posted','user':winner,'caption':'Test'})[0]==204
        assert req('/api/items','GET',headers={'Origin':'https://example.com'})[0]==403
        proc.terminate(); proc.wait(timeout=5); proc=start(env)
        item=next(x for x in json.loads(req('/api/items')[1])['items'] if x['id']==item['id'])
        assert next(p for p in json.loads(req('/api/items')[1])['profiles'] if p['id']==second)['avatar_url']==avatar
        assert {s['profile_id']:s['state'] for s in item['statuses']}[second]=='available'
        assert sorted(s['state'] for s in item['statuses']) == ['available','claimed','reported_posted']
        print('PASS: migration, profile photo and persistence, profile add/edit/tags, same-platform isolation, import, duplicate, preview, dates, competing claims, cross-origin guard and restart')
    finally:
        proc.terminate(); proc.wait(timeout=5)
