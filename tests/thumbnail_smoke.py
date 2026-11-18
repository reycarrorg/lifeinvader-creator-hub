"""Generate a disposable video and verify the local thumbnail endpoint."""
import json, os, subprocess, tempfile, urllib.parse
from pathlib import Path
from smoke_helpers import request, json_post, start, ROOT

ffmpeg=ROOT/'tools'/('ffmpeg.exe' if os.name=='nt' else 'ffmpeg')
if not ffmpeg.is_file():
    raise SystemExit('Thumbnail decoder is missing')
with tempfile.TemporaryDirectory(prefix='lifeinvader-thumb-test-') as tmp:
    base=Path(tmp); drive=base/'drive'; account=drive/'sample-source'; account.mkdir(parents=True)
    video=account/'sample.mp4'
    subprocess.run([str(ffmpeg),'-v','error','-f','lavfi','-i','testsrc=size=160x90:rate=5','-t','1','-c:v','mpeg4',str(video)],check=True)
    env=os.environ.copy();env.update(LIFEINVADER_DB=str(base/'db.sqlite3'),LIFEINVADER_MEDIA=str(base/'media'),LIFEINVADER_PORT='3859',LIFEINVADER_FFMPEG=str(ffmpeg));Path(env['LIFEINVADER_MEDIA']).mkdir()
    proc=start(env)
    try:
        assert json_post('/api/source-settings',{'root':str(drive),'downloader':'','photo_helper':''})[0]==204
        profiles=json.loads(request('/api/items')[1])['profiles'];sid=next(p['id'] for p in profiles if p['name']=='sample-source')
        path=urllib.parse.urlencode({'path':'sample-source/sample.mp4'})
        code,body=request(f'/api/sources/{sid}/thumbnail?{path}');assert code==200 and body.startswith(b'\xff\xd8\xff') and len(body)>500
        assert request(f'/api/sources/{sid}/thumbnail?{path}')[1]==body
        bad=urllib.parse.urlencode({'path':'../outside.mp4'});assert request(f'/api/sources/{sid}/thumbnail?{bad}')[0]==400
        print('PASS: thumbnail generation, caching, and source path boundary')
    finally:
        proc.terminate();proc.wait(timeout=5)
