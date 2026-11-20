#![forbid(unsafe_code)]
//! Controller original-source admission, extracted without Stage 1 routes.
use crate::{App,source};
use rusqlite::{Connection,OptionalExtension};
use serde::{Deserialize,Serialize};
use std::path::{Path as FsPath,PathBuf};
const MAX_REQUEST:usize=16*1024;
#[derive(Clone,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthorizedSource {
    source_id:String,revision:String,topic:String,origin:String,source_sha256:String,
    rights_basis:String,authorization_ref:String,
}
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
pub(crate) struct Catalog {schema:String,sources:Vec<AuthorizedSource>}
impl Catalog {
    pub(crate) fn parse(raw:&[u8],pin:&str)->Result<Self,&'static str>{
        use sha2::{Digest,Sha256};
        if raw.len()>1048576 || pin.len()!=64 || format!("{:x}",Sha256::digest(raw))!=pin{return Err("catalog_pin_mismatch")}
        let c:Self=serde_json::from_slice(raw).map_err(|_|"catalog_schema")?;
        if c.schema!="authorized-source-catalog-v1" || c.sources.len()>1000{return Err("catalog_schema")}
        let mut seen=std::collections::BTreeSet::new();
        for s in &c.sources {
            if [&s.source_id,&s.revision,&s.topic,&s.origin,&s.source_sha256,&s.rights_basis,&s.authorization_ref].iter().any(|v|v.is_empty()||v.len()>512||v.chars().any(|c|c<' '))
                || !matches!(s.origin.as_str(),"local-owned"|"authorized-export")
                || !matches!(s.rights_basis.as_str(),"owned"|"licensed"|"permission")
                || crate::controller_proof::Hash256::parse(&s.source_sha256).is_none()
                || !seen.insert(s.source_id.clone()) {return Err("catalog_authorization")}
        }
        Ok(c)
    }
    fn permits(&self,a:&Admission,path:&FsPath)->bool{
        use sha2::{Digest,Sha256};use std::io::Read;
        let Some(s)=self.sources.iter().find(|s|s.topic=="gta6"&&s.source_id==a.id.to_string()&&s.revision==controller_binding(a)) else{return false};
        let Ok(mut file)=std::fs::File::open(path) else{return false};
        let mut hash=Sha256::new();let mut buf=[0u8;65536];
        loop {match file.read(&mut buf){Ok(0)=>break,Ok(n)=>hash.update(&buf[..n]),Err(_)=>return false}}
        format!("{:x}",hash.finalize())==s.source_sha256
    }
    pub(crate) fn available(&self,db:&Connection)->Vec<AuthorizedSource>{
        self.sources.iter().filter(|s|s.topic=="gta6"&&s.source_id.parse::<i64>().ok().and_then(|id|current(db,id)).is_some_and(|a|controller_binding(&a)==s.revision)).cloned().collect()
    }
}
pub(crate) fn provision_catalog(args:&[String])->Result<Option<std::sync::Arc<Catalog>>,&'static str>{
    let paths:Vec<_>=args.windows(2).filter(|v|v[0]=="--authorized-source-catalog").collect();
    let pins:Vec<_>=args.windows(2).filter(|v|v[0]=="--authorized-source-catalog-sha256").collect();
    if !args.iter().any(|s|s=="--authorized-source-catalog"||s=="--authorized-source-catalog-sha256"){return Ok(None)}
    if paths.len()!=1||pins.len()!=1{return Err("catalog_arguments")}
    let path=FsPath::new(&paths[0][1]);
    if !path.is_absolute(){return Err("catalog_absolute_path_required")}
    #[cfg(windows)] let _lease=crate::controller_native_process::lease(path,&pins[0][1]).map_err(|_|"catalog_lease")?;
    #[cfg(not(windows))] return Err("catalog_windows_required");
    #[cfg(windows)] {
        use std::io::Read;
        let mut raw=Vec::new();std::fs::File::open(path).map_err(|_|"catalog_read")?.take(1048577).read_to_end(&mut raw).map_err(|_|"catalog_read")?;
        Ok(Some(std::sync::Arc::new(Catalog::parse(&raw,&pins[0][1])?)))
    }
}
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
    if app.controller_catalog.as_ref().is_some_and(|c|!c.permits(&admission,&path)){return Err(Rejected::Unavailable)}
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
    #[test] fn catalog_binding_checks_actual_selection_and_stale_revision(){
        use sha2::{Digest,Sha256};
        let mut app=crate::controller::tests::app();
        {let db=app.db.lock().unwrap();db.execute("UPDATE profiles SET platform='local',url=? WHERE id=1",[LOCAL_SYNTHETIC_URL]).unwrap();}
        let a={let db=app.db.lock().unwrap();current(&db,1).unwrap()};
        let raw=serde_json::to_vec(&serde_json::json!({"schema":"authorized-source-catalog-v1","sources":[{
            "source_id":"1","revision":controller_binding(&a),"topic":"gta6","origin":"local-owned",
            "source_sha256":format!("{:x}",Sha256::digest(b"inert source")),"rights_basis":"owned","authorization_ref":"synthetic-review-only"
        }]})).unwrap();
        let pin=format!("{:x}",Sha256::digest(&raw));
        assert!(Catalog::parse(&raw,&"0".repeat(64)).is_err());
        app.controller_catalog=Some(std::sync::Arc::new(Catalog::parse(&raw,&pin).unwrap()));
        assert!(controller_selection(&app,1,"sample-source/original.mp4").is_ok());
        {let db=app.db.lock().unwrap();assert_eq!(app.controller_catalog.as_ref().unwrap().available(&db).len(),1);db.execute("UPDATE profiles SET source_folder='changed' WHERE id=1",[]).unwrap();assert!(app.controller_catalog.as_ref().unwrap().available(&db).is_empty());}
        assert!(controller_selection(&app,1,"sample-source/original.mp4").is_err());
        let db=app.db.lock().unwrap();assert_eq!(db.query_row("SELECT COUNT(*) FROM controller_admission_launches",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
    #[test] fn catalog_rejects_duplicate_fields_and_unreviewed_rights(){
        use sha2::{Digest,Sha256};
        for raw in [br#"{"schema":"authorized-source-catalog-v1","schema":"authorized-source-catalog-v1","sources":[]}"#.as_slice(),br#"{"schema":"authorized-source-catalog-v1","sources":[{"source_id":"1","revision":"x","topic":"gta6","origin":"local-owned","source_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","rights_basis":"public","authorization_ref":"x"}]}"#.as_slice()]{
            assert!(Catalog::parse(raw,&format!("{:x}",Sha256::digest(raw))).is_err());
        }
        assert!(provision_catalog(&["--authorized-source-catalog".into()]).is_err());
        assert!(provision_catalog(&[]).unwrap().is_none());
    }
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
