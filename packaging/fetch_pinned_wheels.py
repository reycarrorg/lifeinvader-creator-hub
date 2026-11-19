"""Fetch exact previously installed official wheel bytes; no install/execution/retries."""
import hashlib,json,urllib.request,urllib.parse
from pathlib import Path
repo=Path(__file__).resolve().parents[1]
manifest=repo/'tools/embedded-review-folder-01/required-offline-wheels.json'
entries=json.loads(manifest.read_text(encoding='utf-8'));root=repo/'tools/speech-wheelhouse-01';root.mkdir(exist_ok=False)
class OnlyOfficial(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,req,fp,code,msg,headers,newurl):
        p=urllib.parse.urlparse(newurl)
        if p.scheme!='https' or p.hostname!='files.pythonhosted.org' or p.username or p.password:raise ValueError('redirect scope')
        return super().redirect_request(req,fp,code,msg,headers,newurl)
opener=urllib.request.build_opener(OnlyOfficial())
records=[]
with (root/'retrieval.jsonl').open('x',encoding='utf-8') as log:
    for entry in entries:
        url=urllib.parse.urlparse(entry['url']);name=Path(urllib.parse.unquote(url.path)).name
        if url.scheme!='https' or url.hostname!='files.pythonhosted.org' or url.username or url.password or not name.endswith('.whl') or any(x in name for x in ('/','\\',':')):raise ValueError('wheel provenance')
        target=root/name;digest=hashlib.sha256();size=0
        with opener.open(entry['url'],timeout=30) as response,target.open('xb') as f:
            while data:=response.read(1024*1024):
                size+=len(data)
                if size>256*1024*1024:raise ValueError('wheel size')
                digest.update(data);f.write(data)
        if digest.hexdigest()!=entry['sha256']:raise ValueError('wheel digest mismatch')
        record=dict(name=entry['name'],version=entry['version'],filename=name,sha256=digest.hexdigest(),bytes=size,source_host='files.pythonhosted.org')
        records.append(record);log.write(json.dumps(record)+'\n');log.flush();print(json.dumps({'wheel':entry['name'],'verified':True,'bytes':size}),flush=True)
with (root/'manifest.json').open('x',encoding='utf-8') as f:json.dump({'schema':'pinned-official-wheelhouse-v1','files':records,'installed':False},f,indent=2)
print(json.dumps({'complete':True,'wheels':len(records),'bytes':sum(x['bytes'] for x in records),'manifest_sha256':hashlib.sha256((root/'manifest.json').read_bytes()).hexdigest()}))
