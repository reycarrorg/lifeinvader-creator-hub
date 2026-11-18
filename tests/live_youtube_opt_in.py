"""Opt-in live YouTube test. Uses a test channel and a bounded partial download."""
import json,os,subprocess,tempfile,time
from pathlib import Path
from smoke_helpers import request,json_post,start,ROOT

def get(path):
 code,body=request(path);assert code==200,(code,body);return json.loads(body)
with tempfile.TemporaryDirectory(prefix='lifeinvader-live-youtube-') as t:
 root=Path(t);drive=root/'drive';drive.mkdir();media=root/'media';media.mkdir();wrapper=root/'limited-ytdlp.exe'
 subprocess.run(['rustc']+(['+stable-x86_64-pc-windows-msvc'] if os.name=='nt' else [])+[str(ROOT/'tests'/'limited_ytdlp_test.rs'),'-o',str(wrapper)],check=True)
 real=os.environ.get("LIFEINVADER_LIVE_YTDLP")
 if not real or not Path(real).is_file():raise SystemExit("Set LIFEINVADER_LIVE_YTDLP to an existing yt-dlp executable to opt in.")
 env=os.environ.copy();env.update(LIFEINVADER_DB=str(root/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859',LV_TEST_REAL_YTDLP=real)
 proc=start(env,seed=False)
 try:
  assert json_post('/api/source-settings',{'root':str(drive),'downloader':str(wrapper),'photo_helper':''})[0]==204
  code,body=json_post('/api/profiles',{'name':'ytdl-test','platform':'youtube','url':'https://www.youtube.com/channel/UCLqxVugv74EIW3VWh2NOa3Q','role':'source','tags':[]});assert code==201,(code,body);pid=json.loads(body)['id']
  code,body=json_post(f'/api/sources/{pid}/sync',{'include_photos':False});assert code==202,(code,body)
  for _ in range(300):
   result=get(f'/api/sources/{pid}')
   if result['last_run'] and result['last_run']['state']!='running':break
   time.sleep(.2)
  else:raise RuntimeError('live sync timed out')
  assert result['last_run']['state']=='finished',result['last_run']['log'][-800:]
  assert result['files'],result['last_run']['log'][-800:]
  assert all((drive/f['path']).stat().st_size for f in result['files'])
  print('PASS: app sync retrieved a bounded partial video from a live YouTube test channel; data remained in a temporary folder')
 finally:proc.terminate();proc.wait(timeout=5)
