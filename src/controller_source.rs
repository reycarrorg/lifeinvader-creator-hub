#![forbid(unsafe_code)]
//! Controller original-source admission, extracted without Stage 1 routes.
use crate::{App,source};
use rusqlite::{Connection,OptionalExtension};
use serde::{Deserialize,Serialize};
use std::path::{Path as FsPath,PathBuf};
const MAX_REQUEST:usize=16*1024;
#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct SelectionRequest {path:String}
fn parse_selection(raw:&[u8])->Option<SelectionRequest> {
    if raw.is_empty()||raw.len()>MAX_REQUEST{return None;}
    let request:SelectionRequest=serde_json::from_slice(raw).ok()?;
    if request.path.is_empty()||request.path.len()>4096||request.path.contains(['\\',':','\0'])
       || request.path.split('/').any(|p|p.is_empty()||p=="."||p=="..") {return None;}
    Some(request)
}

#[derive(Clone,PartialEq,Eq,Serialize)]
pub(crate) struct Admission {
    id:i64,url:String,platform:String,source_folder:String, source_root:PathBuf,
}
fn current(db:&Connection,id:i64)->Option<Admission> {
    let cfg=source::config(db).ok()?;
    let source_root=source::checked_root(&cfg).ok()?;
    db.query_row("SELECT url,platform,source_folder FROM profiles
                  WHERE id=?1 AND role='source' AND archived=0",[id],|row|Ok(Admission{
        id,url:row.get(0)?,platform:row.get(1)?,source_folder:row.get(2)?,source_root:source_root.clone(),
    })).optional().ok().flatten()
}
pub(crate) fn matches_admission(db:&Connection,admission:&Admission)->bool {
    current(db,admission.id).as_ref()==Some(admission)
        && admission.url!=crate::solo::SOLO_URL
}

enum Rejected {Unavailable, Unsupported, Edited, SoloUnknown}
fn edited(path:&FsPath)->bool {
    path.components().any(|c|c.as_os_str().to_str().is_some_and(|name|name.eq_ignore_ascii_case(".lifeinvader-edits")))
}
fn resolve(app:&App,id:i64,reference:&str)->Result<(Admission,PathBuf),Rejected> {
    let admission={
        let db=app.db.lock().map_err(|_|Rejected::Unavailable)?;
        current(&db,id).ok_or(Rejected::Unavailable)?
    };
    let path=source::resolve_file(app,id,reference,"source").map_err(|_|Rejected::Unavailable)?;
    let suffix=path.extension().and_then(|x|x.to_str()).unwrap_or("").to_ascii_lowercase();
    if !matches!(suffix.as_str(),"mp4"|"webm"|"mov"|"mkv"){return Err(Rejected::Unsupported);}
    if edited(FsPath::new(reference))||edited(&path){return Err(Rejected::Edited);}
    // Display .info.json, filename, virtual profile, post ID and thumbnail do
    // not independently prove the exact original Solo post selection or rights.
    // Main has no trusted per-file Solo review source; hold all such requests.
    if admission.url==crate::solo::SOLO_URL{return Err(Rejected::SoloUnknown);}
    {
        let db=app.db.lock().map_err(|_|Rejected::Unavailable)?;
        if current(&db,id).as_ref()!=Some(&admission){return Err(Rejected::Unavailable);}
    }
    Ok((admission,path))
}

// Stage 2 local control intake reuses the original-source restrictions; no worker call.
pub(crate) fn controller_selection(app:&App,id:i64,path:&str)->Result<Admission,&'static str>{
    let raw=serde_json::to_vec(&serde_json::json!({"path":path})).map_err(|_|"invalid_selection")?;
    parse_selection(&raw).ok_or("invalid_selection")?;
    resolve(app,id,path).map(|(admission,_)|admission).map_err(|e|match e{
        Rejected::Unavailable=>"source_unavailable",Rejected::Unsupported=>"unsupported_source",
        Rejected::Edited=>"original_source_required",Rejected::SoloUnknown=>"solo_provenance_unverified",
    })
}

pub(crate) fn controller_binding(admission:&Admission)->String{
    use sha2::{Digest,Sha256};
    format!("{:x}",Sha256::digest(serde_json::to_vec(admission).expect("serializable admission")))
}
pub(crate) fn controller_current(db:&Connection,id:i64,binding:&str)->bool{
    current(db,id).is_some_and(|a|a.url!=crate::solo::SOLO_URL && controller_binding(&a)==binding)
}
