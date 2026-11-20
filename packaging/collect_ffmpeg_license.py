import hashlib,json,subprocess
from pathlib import Path
repo=Path.cwd();out=repo/'tools/embedded-ffmpeg-license-evidence-01';out.mkdir(exist_ok=False)
exe=repo/'tools/embedded-review-folder-01/runtime/ffmpeg/ffmpeg.exe'
assert hashlib.sha256(exe.read_bytes()).hexdigest()=='589e50b766d251afdf181dd664d40bd94407e200b019989fd7468c7d118a28d0'
records=[]
for label,arg in [('version','-version'),('buildconf','-buildconf'),('license','-L')]:
 p=subprocess.run([str(exe),arg],stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=5,creationflags=0x08000000)
 if p.returncode!=0 or len(p.stdout)>65536:raise RuntimeError('metadata query HOLD')
 (out/(label+'.txt')).write_bytes(p.stdout);records.append({'query':arg,'exit':p.returncode,'bytes':len(p.stdout),'sha256':hashlib.sha256(p.stdout).hexdigest()})
text=(out/'buildconf.txt').read_text(errors='replace');nonfree='--enable-nonfree' in text
summary={'queries':records,'nonfree':nonfree,'enable_gpl':'--enable-gpl' in text,'enable_version3':'--enable-version3' in text,'static':'--enable-static' in text,'disable_shared':'--disable-shared' in text,'redistribution_cleared':False,'corresponding_source_pending':True}
(out/'summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary))
