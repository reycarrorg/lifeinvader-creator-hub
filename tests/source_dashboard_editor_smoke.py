"""Source rules, category appearance and real local editing with isolated media."""
import json, os, sqlite3, subprocess, tempfile, uuid, shutil, sys
from contextlib import closing
from pathlib import Path
from smoke_helpers import ROOT, request, json_post, start

ffmpeg_candidate = os.environ.get('LIFEINVADER_FFMPEG') or shutil.which('ffmpeg') or str(ROOT/'tools'/('ffmpeg.exe' if os.name=='nt' else 'ffmpeg'))
if not Path(ffmpeg_candidate).is_file():
    print('SKIP: FFmpeg is needed for real editor smoke tests')
    sys.exit(0)

def get(path):
    code,body=request(path);assert code==200,(code,body);return json.loads(body)
def multipart(path,fields,files={}):
    boundary='----lv'+uuid.uuid4().hex;parts=[]
    for name,value in fields.items():
        parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"\r\n\r\n{value}\r\n'.encode())
    for name,(filename,body) in files.items():
        parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"; filename="{filename}"\r\nContent-Type: application/octet-stream\r\n\r\n'.encode()+body+b'\r\n')
    parts.append(f'--{boundary}--\r\n'.encode());return request(path,'POST',b''.join(parts),{'Content-Type':'multipart/form-data; boundary='+boundary})
with tempfile.TemporaryDirectory(prefix='lifeinvader-edit-') as tmp:
    base=Path(tmp);drive=base/'drive';media=base/'media';drive.mkdir();media.mkdir();folder=drive/'sample-source';folder.mkdir();video=folder/'sample.mp4'; logo=base/'logo.png'; ffmpeg=Path(ffmpeg_candidate)
    subprocess.run([str(ffmpeg),'-v','error','-f','lavfi','-i','testsrc2=s=320x240:r=10','-f','lavfi','-i','sine=frequency=440','-t','2','-c:v','libx264','-pix_fmt','yuv420p','-c:a','aac','-y',str(video)],check=True)
    subprocess.run([str(ffmpeg),'-v','error','-f','lavfi','-i','color=c=red:s=64x64','-frames:v','1','-threads','1','-y',str(logo)],check=True)
    (folder/'image.png').write_bytes(logo.read_bytes())
    (folder/'sample.info.json').write_text(json.dumps({'id':'444','upload_date':'20240102','title':'Original title','description':'Original text'}))
    env=os.environ.copy();env.update(LIFEINVADER_DB=str(base/'db.sqlite3'),LIFEINVADER_MEDIA=str(media),LIFEINVADER_PORT='3859',LIFEINVADER_FFMPEG=str(ffmpeg))
    proc=start(env)
    try:
        profiles=get('/api/items')['profiles'];source=next(p['id'] for p in profiles if p['name']=='sample-source');dead=next(p['id'] for p in profiles if p['name']=='sample-two');off=next(p['id'] for p in profiles if p['name']=='sample-three');target=next(p['id'] for p in profiles if p['role']=='destination')
        assert json_post('/api/source-settings',{'root':str(drive),'downloader':'','photo_helper':''})[0]==204
        for sid in (source,dead):assert json_post(f'/api/sources/{sid}/schedule',{'enabled':True,'interval_seconds':3600})[0]==204
        assert json_post(f'/api/sources/{dead}/dead',{'dead':True})[0]==204
        with closing(sqlite3.connect(base/'db.sqlite3')) as db:
            db.execute("insert into source_new_files(profile_id,root,path,downloaded_at,seen) values(?,?,?,unixepoch()-100,1)",(source,('\\\\?\\'+str(drive)) if os.name=='nt' else str(drive),'sample-source/sample.mp4'));db.commit()
        states={r['profile_id']:r for r in get('/api/source-schedules')['schedules']};assert states[source]['added_24h']==1 and states[dead]['dead']
        code,result=json_post('/api/source-batches',{'profile_ids':[source,dead,off],'include_photos':False});assert code==202,(code,result);result=json.loads(result);assert result['added']==1 and result['excluded']==2,result
        assert json_post('/api/categories/appearance',{'role':'source','name':'Clips','icon':'★','color':'blue','clear_image':False})[0]==204
        code,body=multipart('/api/categories/photo',{'role':'source','name':'Clips'},{'photo':('logo.png',logo.read_bytes())});assert code==204,(code,body)
        cat=next(x for x in get('/api/items')['categories'] if x['name']=='Clips');assert cat['icon']=='★' and cat['color']=='blue' and cat['image_url'].startswith('/media/category-photos/')
        options={'profile_id':source,'role':'source','path':'sample-source/sample.mp4','crop':'9:16','start':0.2,'end':1.4,'mute':True}
        code,body=multipart('/api/editor/render',{'options':json.dumps(options)},{'overlay':('logo.png',logo.read_bytes())});assert code==200,(code,body);edit=json.loads(body)
        result=drive/Path(edit['path']);assert result.is_file() and result.stat().st_size>0
        details=subprocess.run([str(ffmpeg),'-hide_banner','-i',str(result)],capture_output=True,text=True).stderr
        assert 'Audio:' not in details and '134x240' in details and 'Duration: 00:00:01.20' in details,details
        assert video.is_file() and len(get(f'/api/sources/{source}')['files'])==2
        code,body=json_post(f'/api/profiles/{target}/queue',{'source_id':source,'path':'sample-source/sample.mp4','edit_id':edit['id'],'caption':'Edited caption'});assert code==201,(code,body)
        q=get(f'/api/profiles/{target}/queue')[0];assert q['edited_path']==edit['path'] and q['caption']=='Edited caption' and q['available'] and q['title']=='Original title'
        handoff=get('/api/posting-handoff'); text=json.dumps(handoff);assert edit['path'].replace('/','\\\\').split('\\\\')[-1] in text and 'Edited caption' in text
        bad=dict(options,path='../escape.mp4');assert multipart('/api/editor/render',{'options':json.dumps(bad)})[0] in (400,403,404)
        assert json_post(f'/api/profiles/{target}/queue',{'source_id':off,'path':'sample-source/sample.mp4','edit_id':edit['id']})[0] in (400,403,404)
        no_logo=dict(options,crop='original',start=0,end=None,mute=False)
        code,body=multipart('/api/editor/render',{'options':json.dumps(no_logo)});assert code==200,(code,body)
        audio_result=drive/Path(json.loads(body)['path']);info=subprocess.run([str(ffmpeg),'-hide_banner','-i',str(audio_result)],capture_output=True,text=True).stderr;assert 'Audio:' in info and 'Duration: 00:00:02.' in info,info
        photo=dict(options,path='sample-source/image.png',start=0,end=None,crop='1:1')
        code,body=multipart('/api/editor/render',{'options':json.dumps(photo)},{'overlay':('logo.png',logo.read_bytes())});assert code==200,(code,body)
        assert (drive/Path(json.loads(body)['path'])).is_file()

        print('PASS: 24h counts, dead and auto-sync filtering, category image, real crop/trim/mute/overlay, queue and handoff')
    finally:proc.terminate();proc.wait(timeout=5)
