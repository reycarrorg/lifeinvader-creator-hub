"""Keep unrelated Hub endpoints responsive while synthetic source folders stall."""
import concurrent.futures
import json
import os
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

from smoke_helpers import ROOT, json_post, request, start

if sys.platform != 'darwin':
    print('SKIP: macOS filesystem interposition test')
    raise SystemExit(0)

def get(path, timeout=2):
    try:
        with urllib.request.urlopen('http://127.0.0.1:3859' + path, timeout=timeout) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()

with tempfile.TemporaryDirectory(prefix='lifeinvader-slow-source-') as tmp:
    base = Path(tmp).resolve()
    drive, media = base / 'drive', base / 'media'
    drive.mkdir(); media.mkdir()
    shim, marker = base / 'slow.dylib', base / 'marker'
    subprocess.run(['clang', '-dynamiclib', str(ROOT / 'tests/fixtures/macos_slow_opendir.c'),
                    '-o', str(shim)], check=True)
    env = os.environ.copy()
    env.update(LIFEINVADER_DB=str(base / 'db.sqlite3'), LIFEINVADER_MEDIA=str(media),
               LIFEINVADER_PORT='3859', DYLD_INSERT_LIBRARIES=str(shim),
               LIFEINVADER_TEST_SLOW_ROOT=str(drive / 'slow'),
               LIFEINVADER_TEST_SLOW_MARKER=str(marker))
    proc = start(env, seed=False)
    pool = concurrent.futures.ThreadPoolExecutor(max_workers=8)
    try:
        assert json_post('/api/source-settings', {'root':str(drive), 'downloader':'', 'photo_helper':''})[0] == 204
        ids = []
        for i in range(8):
            name = f'slow{i}'
            folder = drive / name
            folder.mkdir()
            (folder / 'synthetic.mp4').write_bytes(b'fake')
            status, raw = json_post('/api/profiles', {'name':name, 'platform':'tiktok',
                'url':f'https://www.tiktok.com/@{name}', 'role':'source', 'tags':[]})
            assert status == 201, raw
            ids.append(json.loads(raw)['id'])
        solo_dir = drive / 'Solo Posts' / 'Downloads' / 'creator'
        solo_dir.mkdir(parents=True)
        (solo_dir / 'clip.mp4').write_bytes(b'fake-solo-video')
        with sqlite3.connect(base / 'db.sqlite3') as db:
            db.execute("INSERT INTO profiles(name,platform,url,role,tags,source_folder) VALUES('Solo Posts','other','https://lifeinvader.local/solo-posts','source','[]','')")
            solo = db.execute('SELECT last_insert_rowid()').fetchone()[0]
        status, raw = json_post('/api/profiles', {'name':'target','platform':'tiktok',
            'url':'https://www.tiktok.com/@target','role':'destination','tags':[]})
        assert status == 201, raw
        target = json.loads(raw)['id']

        background = pool.submit(get, f'/api/sources/{ids[0]}/summary', 16)
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline and (not marker.exists() or marker.stat().st_size < 1):
            time.sleep(.05)
        assert marker.exists() and marker.stat().st_size == 1, 'Synthetic background scan did not start'
        assert get(f'/api/sources/{ids[1]}/summary')[0] == 503

        # A stuck dashboard scan must not stop the user opening and queueing Solo media.
        code, raw = get(f'/api/sources/{solo}/browse')
        result = json.loads(raw)
        assert code == 200 and len(result['files']) == 1, result
        path = result['files'][0]['path']
        status, raw = json_post(f'/api/profiles/{target}/queue', {'source_id':solo, 'path':path})
        assert status == 201, raw
        code, raw = get(f'/api/profiles/{target}/queue')
        assert code == 200 and len(json.loads(raw)) == 1 and json.loads(raw)[0]['available']

        foreground = pool.submit(get, f'/api/sources/{ids[1]}/browse', 16)
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline and marker.stat().st_size < 2:
            time.sleep(.05)
        assert marker.stat().st_size == 2, 'Synthetic interactive scan did not start'
        assert get(f'/api/sources/{solo}/browse')[0] == 503
        for route in ('/api/items', '/api/source-settings'):
            started = time.monotonic()
            code, _ = get(route, timeout=2)
            assert code == 200 and time.monotonic() - started < 2, route
        time.sleep(9)
        assert get(f'/api/sources/{ids[0]}/summary', timeout=2)[0] == 503, 'Timed-out scans released their slots before the filesystem returned'
        assert get(f'/api/sources/{solo}/browse', timeout=2)[0] == 503
        time.sleep(4)
        assert background.result(timeout=1)[0] == 503 and foreground.result(timeout=1)[0] == 503
        code, raw = get(f'/api/sources/{ids[0]}/summary', timeout=3)
        result = json.loads(raw)
        assert code == 200 and result['videos'] == 1 and result['photos'] == 0, result
        code, raw = get(f'/api/sources/{solo}/browse', timeout=3)
        assert code == 200 and len(json.loads(raw)['files']) == 1
        print('PASS: a stuck dashboard scan does not block opening or queueing Solo media; scan lanes remain bounded and recover after actual filesystem completion')
    finally:
        proc.terminate(); proc.wait(timeout=5)
        pool.shutdown(wait=True, cancel_futures=True)
