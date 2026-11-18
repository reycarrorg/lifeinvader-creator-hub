"""Verify categories remain separate and preserve other account tags."""
import json, os, tempfile
from pathlib import Path
from smoke_helpers import request, json_post, start

def data():
    code,body=request('/api/items');assert code==200
    return json.loads(body)

with tempfile.TemporaryDirectory(prefix='lifeinvader-categories-test-') as tmp:
    base=Path(tmp); media=base/'media';media.mkdir()
    env=os.environ.copy();env.update(LIFEINVADER_DB=str(base/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859')
    proc=start(env)
    try:
        profiles=data()['profiles'];dest=next(p for p in profiles if p['role']=='destination');source=next(p for p in profiles if p['role']=='source')
        for role in ['destination','source']:
            assert json_post('/api/categories',{'role':role,'name':'Lifeinvader'})[0]==204
        assert {(c['role'],c['name']) for c in data()['categories']}=={('destination','Lifeinvader'),('source','Lifeinvader')}
        assert json_post('/api/categories',{'role':'source','name':'Other Channel Clipper'})[0]==204
        assert json_post('/api/categories/members',{'role':'source','name':'Other Channel Clipper','profile_ids':[source['id']]})[0]==204
        before={p['id']:p for p in data()['profiles']}
        assert 'Sources' in before[source['id']]['tags'] and 'Other Channel Clipper' in before[source['id']]['tags']
        assert before[dest['id']]['tags']==dest['tags']
        assert json_post('/api/categories/members',{'role':'source','name':'Other Channel Clipper','profile_ids':[source['id'],dest['id']]})[0]==400
        assert next(p for p in data()['profiles'] if p['id']==source['id'])['tags']==before[source['id']]['tags']
        assert json_post('/api/categories/members',{'role':'source','name':'Other Channel Clipper','profile_ids':[]})[0]==204
        assert next(p for p in data()['profiles'] if p['id']==source['id'])['tags']==source['tags']
        assert any(c['name']=='Other Channel Clipper' for c in data()['categories'])
        assert json_post('/api/categories',{'role':'source','name':'Bad,Name'})[0]==400
        print('PASS: separate categories, membership updates, preserved tags, and empty categories')
    finally:
        proc.terminate();proc.wait(timeout=5)
