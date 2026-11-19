"""Confirm every downloader invocation receives the configured FFmpeg path."""
import json,os,subprocess,tempfile,time
from pathlib import Path
from smoke_helpers import ROOT,request,json_post,start

def get(path):
 code,body=request(path);assert code==200,(code,body);return json.loads(body)
def wait(sid):
 for i in range(250):
  run=get(f'/api/sources/{sid}/status')['last_run']
  if run and run['state']!='running':return run
  time.sleep(.04)
 raise AssertionError('fake downloader timed out')
with tempfile.TemporaryDirectory(prefix='lifeinvader-ffmpeg-args-') as tmp:
 base=Path(tmp);drive=base/'drive';drive.mkdir();media=base/'media';media.mkdir();fake=base/('fake.exe' if os.name=='nt' else 'fake');log=base/'calls.txt';ffmpeg=base/('ffmpeg fixture.exe' if os.name=='nt' else 'ffmpeg fixture');ffmpeg.write_bytes(b'test fixture')
 subprocess.run(['rustc']+(['+stable-x86_64-pc-windows-msvc'] if os.name=='nt' else [])+[str(ROOT/'tests/fake_ffmpeg_ytdlp.rs'),'-o',str(fake)],check=True)
 env=os.environ.copy();env.update(LIFEINVADER_DB=str(base/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859',LIFEINVADER_FFMPEG=str(ffmpeg),LV_EXPECT_FFMPEG=str(ffmpeg),LV_FAKE_LOG=str(log));proc=start(env)
 try:
  assert json_post('/api/source-settings',{'root':str(drive),'downloader':str(fake),'photo_helper':''})[0]==204
  sid=next(p['id'] for p in get('/api/items')['profiles'] if p['role']=='source')
  code,body=json_post(f'/api/sources/{sid}/preview',{});assert code==200,(code,body)
  code,body=json_post(f'/api/sources/{sid}/sync',{'include_photos':False});assert code==202,(code,body);assert wait(sid)['state']=='finished'
  code,body=json_post('/api/solo-posts',{'links':['https://www.instagram.com/p/Test123/']});assert code==202,(code,body);solo=json.loads(body)['id'];run=wait(solo);assert run['state']=='finished',run
  calls=log.read_text().splitlines();assert calls.count('preview')==1 and calls.count('download')==2 and calls.count('metadata')==1,calls
  print('PASS: source preview and sync plus both Instagram solo downloader calls receive FFmpeg')
 finally:proc.terminate();proc.wait(timeout=5)
