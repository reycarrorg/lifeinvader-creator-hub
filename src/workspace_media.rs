use crate::{App,controller_store::{self as store,Error}};
use serde::Deserialize;
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::{collections::BTreeSet,fs::File,io::{Read,Seek,SeekFrom},path::{Path,PathBuf}};
const PIN:&str="ef1cc1847673438a3ea063a52d4575761251ae1558c6d064dd9b7fd68cea429c";
const MP4_CAP:u64=180*1024*1024;
const WEBP_CAP:u64=2*1024*1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest{schema:String,review_only:bool,index_source_id:i64,items:Vec<Item>}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Item{
    clip_id:String,source_id:i64,source_hint:Option<String>,source_path_exact:String,source_path:String,title:String,
    bytes:u64,state:String,preview_only:bool,non_public:bool,preview_mp4:Option<String>,poster_webp:Option<String>,
    sha256:Option<String>,poster_sha256:Option<String>,poster_width:Option<u32>,poster_height:Option<u32>,
    youtube_private_test_id:Option<String>,
}
struct Row{id:String,source:i64,path:String,hint:Option<String>,bytes:u64,state:String}
pub fn hex(s:&str)->bool{s.len()==64 && s.bytes().all(|c|c.is_ascii_digit()||(b'a'..=b'f').contains(&c))}
fn sha(v:&[u8])->String{format!("{:x}",Sha256::digest(v))}
fn nfc(raw:&str)->Option<String>{
    #[cfg(windows)]{
        use windows_sys::Win32::Globalization::{NormalizeString,NormalizationC};
        let src=raw.encode_utf16().collect::<Vec<_>>();let n=i32::try_from(src.len()).ok()?;
        let cap=unsafe{NormalizeString(NormalizationC,src.as_ptr(),n,std::ptr::null_mut(),0)};
        if !(1..=8192).contains(&cap){return None;}
        let mut out=vec![0u16;cap as usize];let used=unsafe{NormalizeString(NormalizationC,src.as_ptr(),n,out.as_mut_ptr(),cap)};
        if used<=0{return None;}out.truncate(used as usize);String::from_utf16(&out).ok()
    }
    #[cfg(not(windows))]
    raw.is_ascii().then(||raw.to_owned())
}
fn normalized(path:&str)->Option<String>{
    if path.is_empty()||path.len()>4096||path.contains(':')||path.as_bytes().contains(&0u8){return None;}
    let n=nfc(&path.replace('\\',"/"))?;
    if n.split('/').any(|x|x.is_empty()||x=="."||x==".."||x.ends_with(['.',' '])||
       matches!(x.to_ascii_lowercase().as_str(),"posted clips backup"|"ready to delete"|"ready to post"|"staging"|
           "app staging"|"cache"|"output"|"outputs"|"app output"|"worker"|"controller"|".lifeinvader-edits")){return None;}
    Some(n)
}
fn stable(source:i64,relative:&str)->String{
    let mut v=b"lifeinvader-source-v1".to_vec();v.push(0u8);v.extend(source.to_string().bytes());
    v.push(0u8);v.extend(relative.bytes());sha(&v)
}
pub(crate) fn intake_identity(relative:&str)->Option<String>{
    let normalized=normalized(relative)?;
    if normalized!=relative{return None;}
    Some(stable(2,&normalized))
}
fn rows(app:&App)->store::Result<Vec<Row>>{
    let d=app.db.lock().map_err(|_|Error("storage_unavailable"))?;
    if !d.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ops_source_review_index')",[],|r|r.get::<_,bool>(0))?{return Ok(Vec::new());}
    let mut st=d.prepare("SELECT i.source_id,i.path,i.source_hint,i.size,i.state FROM ops_source_review_index i JOIN profiles p ON p.id=i.source_id WHERE p.archived=0 AND p.role='source' AND p.url=? ORDER BY i.modified_ns DESC,i.path LIMIT 500")?;
    let mut q=st.query([crate::source::DRIVE_SOURCE_URL])?;
    let mut seen=BTreeSet::new();let mut out=Vec::new();
    while let Some(r)=q.next()?{
        let source=r.get::<_,i64>(0)?;let path=r.get::<_,String>(1)?;
        let hint=r.get::<_,Option<String>>(2)?;let bytes=u64::try_from(r.get::<_,i64>(3)?).map_err(|_|Error("review_index_invalid"))?;
        let state=r.get::<_,String>(4)?;
        let id=stable(source,&normalized(&path).ok_or(Error("review_index_path_invalid"))?);
        if !seen.insert(id.clone()){return Err(Error("review_index_ambiguous_clip_id"));}
        out.push(Row{id,source,path,hint,bytes,state});
    }
    Ok(out)
}
fn lease(path:&Path,pin:&str,cap:u64)->store::Result<File>{
    if !path.is_absolute()||!hex(pin){return Err(Error("preview_pin_invalid"));}
    #[cfg(windows)]
    let f={
        crate::controller_native_launcher::clean_path(path).map_err(|_|Error("preview_path_unsafe"))?;
        crate::controller_native_process::lease(path,pin).map_err(|_|Error("preview_digest_unverified"))?
    };
    #[cfg(not(windows))]
    let f={
        let mut f=File::open(path).map_err(|_|Error("preview_unavailable"))?;let mut all=Vec::new();
        (&mut f).take(cap+1).read_to_end(&mut all).map_err(|_|Error("preview_unavailable"))?;
        if sha(&all)!=pin{return Err(Error("preview_digest_unverified"));} f
    };
    let m=f.metadata().map_err(|_|Error("preview_unavailable"))?;
    if !m.is_file()||m.len()==0||m.len()>cap{return Err(Error("preview_size_invalid"));}
    Ok(f)
}
fn load(rows:&[Row])->store::Result<Option<(PathBuf,Manifest)>>{
    let Some(p)=std::env::var_os("LIFEINVADER_SNAPMATIC_PREVIEW_MANIFEST")else{return Ok(None)};
    let p=PathBuf::from(p);let dir=p.parent().ok_or(Error("manifest_path_unverified"))?;
    if p.file_name().and_then(|s|s.to_str())!=Some("snapmatic-preview-v2.json")
        || dir.file_name().and_then(|s|s.to_str())!=Some("snapmatic-preview-staging-drive-v1"){return Err(Error("manifest_path_unverified"));}
    let mut f=lease(&p,PIN,131072)?;f.seek(SeekFrom::Start(0)).map_err(|_|Error("manifest_read"))?;
    let mut raw=Vec::new();f.take(131073).read_to_end(&mut raw).map_err(|_|Error("manifest_read"))?;
    if sha(&raw)!=PIN{return Err(Error("manifest_digest_unverified"));}
    let m:Manifest=serde_json::from_slice(&raw).map_err(|_|Error("manifest_schema"))?;
    if m.schema!="snapmatic-local-preview-v2"||!m.review_only||m.index_source_id!=2||m.items.len()!=rows.len(){return Err(Error("manifest_scope"));}
    let mut seen=BTreeSet::new();
    for v in &m.items{
        let r=rows.iter().find(|r|r.id==v.clip_id).ok_or(Error("manifest_index_mismatch"))?;
        if !seen.insert(&v.clip_id)||!v.preview_only||!v.non_public||r.source!=m.index_source_id||v.source_id!=r.source||
            r.path!=v.source_path_exact||r.hint!=v.source_hint||r.bytes!=v.bytes||r.state!=v.state||
            normalized(&r.path).as_deref()!=Some(v.source_path.as_str())||v.title.is_empty()||v.title.len()>4096 {
            return Err(Error("manifest_index_mismatch"));
        }
        if v.state=="test_hold" {
            if v.preview_mp4.is_some()||v.poster_webp.is_some()||v.sha256.is_some()||v.poster_sha256.is_some()||
                v.poster_width.is_some()||v.poster_height.is_some()||v.youtube_private_test_id.as_deref()!=Some("demoVid1234"){return Err(Error("manifest_hold_mismatch"));}
        }else if v.state!="pending_review"||v.youtube_private_test_id.is_some()||
            v.preview_mp4!=Some(format!("{}.mp4",v.clip_id))||v.poster_webp!=Some(format!("{}.webp",v.clip_id))||
            !v.sha256.as_deref().is_some_and(hex)||!v.poster_sha256.as_deref().is_some_and(hex)||
            (v.poster_width,v.poster_height)!=(Some(540),Some(960))||!(1..=MP4_CAP).contains(&v.bytes) {
            return Err(Error("manifest_asset_mismatch"));
        }
    }
    Ok(Some((dir.to_owned(),m)))
}
pub fn index(app:&App)->store::Result<Vec<Value>>{
    let rows=rows(app)?;let m=load(&rows)?;
    Ok(rows.into_iter().map(|r|{
        let v=m.as_ref().and_then(|(_,m)|m.items.iter().find(|v|v.clip_id==r.id));
        let display=v.map(|v|v.title.as_str()).unwrap_or_else(||r.path.rsplit(['\\','/']).next().unwrap_or("Indexed clip"));
        json!({"kind":"original","clip_id":r.id,"name":display,"state":r.state,"preview_only":true,
               "preview_available":v.is_some_and(|v|v.preview_mp4.is_some()),"poster_available":v.is_some_and(|v|v.poster_webp.is_some()),
               "preview_bytes":v.map(|v|v.bytes),"preview_sha256":v.and_then(|v|v.sha256.as_deref())})
    }).collect())
}
pub(crate) fn cache_bytes(app:&App)->store::Result<u64>{
    let rows=rows(app)?;
    let Some((root,m))=load(&rows)? else{return Ok(0);};
    let mut bytes=0u64;
    for item in m.items.into_iter().filter(|v|v.state=="pending_review"){
        for name in [item.preview_mp4,item.poster_webp].into_iter().flatten(){
            let path=root.join(name);
            #[cfg(windows)]
            crate::controller_native_launcher::clean_path(&path).map_err(|_|Error("preview_path_unsafe"))?;
            let meta=std::fs::symlink_metadata(path).map_err(|_|Error("preview_unavailable"))?;
            if !meta.is_file()||meta.file_type().is_symlink(){return Err(Error("preview_path_unsafe"));}
            bytes=bytes.checked_add(meta.len()).ok_or(Error("preview_size_invalid"))?;
        }
    }
    Ok(bytes)
}
pub fn open(app:&App,id:&str,poster:bool)->store::Result<(PathBuf,File)>{
    let rows=rows(app)?;let (dir,m)=load(&rows)?.ok_or(Error("manifest_unavailable"))?;
    let v=m.items.into_iter().find(|v|v.clip_id==id&&v.state=="pending_review").ok_or(Error("preview_unavailable"))?;
    let (name,pin,lim)=if poster{(v.poster_webp,v.poster_sha256,WEBP_CAP)}else{(v.preview_mp4,v.sha256,MP4_CAP)};
    let path=dir.join(name.ok_or(Error("preview_unavailable"))?);
    let f=lease(&path,pin.as_deref().ok_or(Error("preview_unavailable"))?,lim)?;
    if !poster&&f.metadata().map_err(|_|Error("preview_unavailable"))?.len()!=v.bytes{return Err(Error("preview_size_invalid"));}
    Ok((path,f))
}
