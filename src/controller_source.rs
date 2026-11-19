#![forbid(unsafe_code)]
//! Controller original-source admission, extracted without Stage 1 routes.
use crate::{App,source};
use rusqlite::{Connection,OptionalExtension};
use serde::{Deserialize,Serialize};
use std::path::{Path as FsPath,PathBuf};
const MAX_REQUEST:usize=16*1024;
pub(crate) const LOCAL_SYNTHETIC_URL:&str="lifeinvader-local://synthetic-review";
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
// Explicit local synthetic intake only. This does not register a downloader,
// real platform account, publisher identity, or independent render authority.
fn local_synthetic_file(a:&Admission,reference:&str)->Result<PathBuf,Rejected>{
    if a.url!=LOCAL_SYNTHETIC_URL || a.source_folder.is_empty() || a.source_folder.len()>128
        || matches!(a.source_folder.as_str(),"."|"..")
        || !a.source_folder.chars().all(|c|c.is_ascii_alphanumeric()||"._-".contains(c)){
        return Err(Rejected::Unavailable)
    }
    let relative=FsPath::new(reference);
    if relative.is_absolute() || relative.components().any(|c|!matches!(c,std::path::Component::Normal(_))){return Err(Rejected::Unavailable)}
    let folder=a.source_root.join(&a.source_folder);
    let target=a.source_root.join(relative);
    for p in target.ancestors().take_while(|p|*p!=a.source_root.as_path()){
        let m=std::fs::symlink_metadata(p).map_err(|_|Rejected::Unavailable)?;
        if m.file_type().is_symlink(){return Err(Rejected::Unavailable)}
        #[cfg(windows)] {use std::os::windows::fs::MetadataExt;if m.file_attributes()&0x400!=0{return Err(Rejected::Unavailable)}}
    }
    let folder=folder.canonicalize().map_err(|_|Rejected::Unavailable)?;
    let target=target.canonicalize().map_err(|_|Rejected::Unavailable)?;
    if !folder.starts_with(&a.source_root)||!target.starts_with(&folder)||!target.is_file(){return Err(Rejected::Unavailable)}
    Ok(target)
}
fn edited(path:&FsPath)->bool {
    path.components().any(|c|c.as_os_str().to_str().is_some_and(|name|name.eq_ignore_ascii_case(".lifeinvader-edits")))
}
fn resolve(app:&App,id:i64,reference:&str)->Result<(Admission,PathBuf),Rejected> {
    let admission={
        let db=app.db.lock().map_err(|_|Rejected::Unavailable)?;
        current(&db,id).ok_or(Rejected::Unavailable)?
    };
    let path=if admission.platform=="local" {local_synthetic_file(&admission,reference)?}
        else {source::resolve_file(app,id,reference,"source").map_err(|_|Rejected::Unavailable)?};
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

pub(crate) fn controller_file(app:&App,id:i64,path:&str)->Result<PathBuf,&'static str>{
    controller_selection(app,id,path)?;
    resolve(app,id,path).map(|(_,p)|p).map_err(|_|"source_unavailable")
}
pub(crate) fn controller_current(db:&Connection,id:i64,binding:&str)->bool{
    current(db,id).is_some_and(|a|a.url!=crate::solo::SOLO_URL && controller_binding(&a)==binding)
}
pub(crate) fn local_selection_allowed(db:&Connection,id:i64,provider:&str,provider_id:&str)->bool{
    current(db,id).is_some_and(|a|a.platform!="local" ||
        (a.url==LOCAL_SYNTHETIC_URL && provider=="synthetic" && provider_id==id.to_string()))
}

#[cfg(test)] mod synthetic_profile_regression {
    use super::*;
    #[test] fn synthetic_profile_host_rejection_and_corrected_metadata_preserve_source_guard(){
        let app=crate::controller::tests::app();
        let reference="sample-source/original.mp4";
        {
            let db=app.db.lock().unwrap();
            db.execute("UPDATE profiles SET platform='youtube',url='https://lifeinvader.local/synthetic-review' WHERE id=1",[]).unwrap();
        }
        let old={let db=app.db.lock().unwrap();let a=current(&db,1).unwrap();
            let binding=controller_binding(&a);assert!(controller_current(&db,1,&binding));binding};
        assert_eq!(controller_file(&app,1,reference),Err("source_unavailable"));
        // Explicit truthful local scheme; remote download/account handlers must
        // continue rejecting this identity instead of treating it as YouTube.
        {
            let db=app.db.lock().unwrap();
            db.execute("UPDATE profiles SET platform='local',url=? WHERE id=1",[LOCAL_SYNTHETIC_URL]).unwrap();
            assert!(!controller_current(&db,1,&old));
            assert!(source::account_profile(&db,1,"source").is_err());
            assert!(local_selection_allowed(&db,1,"synthetic","1"));
            assert!(!local_selection_allowed(&db,1,"youtube","1"));
            assert!(!local_selection_allowed(&db,1,"synthetic","2"));
        }
        let selection=controller_selection(&app,1,reference).unwrap();
        let path=controller_file(&app,1,reference).unwrap();
        assert_eq!(std::fs::read(path).unwrap(),b"inert source");
        assert_ne!(controller_binding(&selection),old);
        assert!(controller_file(&app,1,"../outside.mp4").is_err());
        let db=app.db.lock().unwrap();
        assert_eq!(db.query_row("SELECT COUNT(*) FROM controller_admission_launches",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
}
