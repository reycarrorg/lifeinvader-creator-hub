"""Isolated Windows Snapmatic import, preview and export regression check."""
import json, os, socket, subprocess, tempfile, time, urllib.error, urllib.request
from pathlib import Path

root=Path(__file__).resolve().parents[1]
exe=root/'target'/'release'/'lifeinvader_hub.exe'
ffmpeg=root/'tools'/'ffmpeg.exe'
if not exe.is_file() or not ffmpeg.is_file():
 raise SystemExit('Build the snapmatic-hud release and provide tools/ffmpeg.exe first.')
with socket.socket() as sock:
 sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
with tempfile.TemporaryDirectory(prefix='snapmatic-windows-smoke-') as tmp:
 base=Path(tmp);media=base/'media';media.mkdir();video=base/'synthetic.mp4'
 subprocess.run([str(ffmpeg),'-hide_banner','-loglevel','error','-f','lavfi','-i','testsrc=size=160x240:rate=12:duration=2','-f','lavfi','-i','sine=frequency=440:duration=2','-shortest','-c:v','libx264','-pix_fmt','yuv420p','-c:a','aac','-y',str(video)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
 env={k:v for k,v in os.environ.items() if not k.upper().startswith('LIFEINVADER_')};env.update(LIFEINVADER_DB=str(base/'test.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT=str(port))
 proc=subprocess.Popen([str(exe),'--server-only'],cwd=root,env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW)
 def request(path,data=None,headers=None):
  try:
   with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{port}'+path,data=data,headers=headers or {}),timeout=120) as res:return res.status,res.read()
  except urllib.error.HTTPError as e:return e.code,e.read()
 try:
  for _ in range(100):
   if proc.poll() is not None:raise RuntimeError('Server exited during startup')
   try:
    status,body=request('/api/workspace/system')
    if status==200:break
   except OSError:pass
   time.sleep(.1)
  else:raise RuntimeError('Server did not start')
  assert json.loads(body)['publishing_enabled'] is False
  status,body=request('/api/workspace/local-status');tools=json.loads(body);assert status==200 and tools['ffmpeg_ready'] and tools['ffprobe_ready']
  boundary='----SnapmaticLocalFixture'
  payload=(f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="synthetic.mp4"\r\nContent-Type: video/mp4\r\n\r\n'.encode()+video.read_bytes()+f'\r\n--{boundary}--\r\n'.encode())
  clips=[]
  for _ in range(2):
   status,body=request('/api/workspace/film-roll/import-file',payload,{'Content-Type':'multipart/form-data; boundary='+boundary});assert status==200,(status,body[:500]);clips.append(json.loads(body))
  assert clips[0]['file_name']!=clips[1]['file_name'] and clips[0]['clip_id']!=clips[1]['clip_id']
  for clip in clips:
   status,body=request('/api/workspace/film-roll/'+clip['clip_id']+'/probe');assert status==200,(status,body[:500])
  status,body=request('/api/workspace/film-roll/'+clips[0]['clip_id']+'/export',json.dumps({'framing':'original','quality':'fast'}).encode(),{'Content-Type':'application/json'});assert status==200,(status,body[:500])
  files=list((media/'exports').glob('*.mp4'));assert files and files[0].stat().st_size>0
  assert len(list((media/'sources'/'Snapmatic Drops').glob('*.mp4')))==2
  print('PASS: separate workspace, tools, same-name imports, video probing and local MP4 export; publishing disabled')
 finally:proc.terminate();proc.wait(timeout=10)
