"""A new install starts empty and does not use another user's folders."""
import json,os,tempfile
from pathlib import Path
from smoke_helpers import request,start

with tempfile.TemporaryDirectory(prefix='lifeinvader-fresh-') as tmp:
    root=Path(tmp);media=root/'media';media.mkdir()
    env=os.environ.copy();env.update(LIFEINVADER_DB=str(root/'data.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859')
    proc=start(env,seed=False)
    try:
        code,body=request('/api/items');data=json.loads(body)
        assert code==200 and not data['profiles'] and not data['items']
        code,body=request('/api/source-settings');assert code==200 and json.loads(body)['root']==''
        print('PASS: fresh install starts without personal accounts or folders')
    finally:
        proc.terminate();proc.wait(timeout=5)
