"""Check the installed yt-dlp progress template and archive using local synthetic bytes."""
import functools, http.server, os, re, shutil, subprocess, tempfile, threading
from pathlib import Path

yt=Path(os.environ.get('YT_DLP') or shutil.which('yt-dlp') or '')
if not yt.is_file(): raise SystemExit('Set YT_DLP to an existing yt-dlp executable, or add it to PATH.')
class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*args): pass
    def copyfile(self, source, outputfile):
        try: super().copyfile(source,outputfile)
        except (BrokenPipeError,ConnectionResetError): pass
with tempfile.TemporaryDirectory(prefix='lv-ytdlp-check-') as tmp:
    root=Path(tmp); serve=root/'serve'; serve.mkdir(); dest=root/'downloaded'; dest.mkdir()
    (serve/'sample.mp4').write_bytes(b'\x00\x00\x00\x18ftypmp42'+bytes(2*1024*1024))
    http=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Quiet,directory=str(serve)))
    threading.Thread(target=http.serve_forever,daemon=True).start()
    try:
        archive=root/'archive.txt'; output=str(dest/'%(id)s.%(ext)s')
        command=[str(yt),'--ignore-config','--ignore-errors','--windows-filenames','--no-overwrites','--write-description','--color','never','--newline','--progress','--progress-delta','1','--progress-template','download:__LV_PROGRESS__%(progress._percent_str)s','--download-archive',str(archive),'--output',output,'--',f'http://127.0.0.1:{http.server_port}/sample.mp4']
        first=subprocess.run(command,capture_output=True,text=True,encoding='utf8',errors='replace',timeout=60)
        assert first.returncode==0,(first.stdout,first.stderr)
        assert re.search(r'__LV_PROGRESS__\s*\d+(?:\.\d+)?%',first.stdout),first.stdout
        assert archive.is_file() and len(list(dest.glob('*.mp4')))==1
        before=archive.read_bytes(); existing=next(dest.glob('*.mp4')); timestamp=existing.stat().st_mtime_ns
        second=subprocess.run(command,capture_output=True,text=True,encoding='utf8',errors='replace',timeout=60)
        assert second.returncode==0,(second.stdout,second.stderr)
        assert archive.read_bytes()==before and existing.stat().st_mtime_ns==timestamp
        print('PASS: installed yt-dlp emitted progress and skipped an archived local test video on repeat')
    finally:http.shutdown(); http.server_close()
