//! App-owned supervisor thread; no HTTP handler, default launcher or retry.
use crate::{App,controller_admission as admission,controller_store as store};
use std::{fs::File,io::Read,path::{Path,PathBuf},sync::Arc,time::{Duration,Instant}};
use sha2::{Digest,Sha256};
use rusqlite::{Connection,OpenFlags};

pub struct Files {pub executable:PathBuf,pub helper:PathBuf,pub approval:PathBuf}
pub struct Pins { _executable:File,_helper:File,_approval:File }
fn hash(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
fn read(path:&Path,cap:u64)->store::Result<(File,Vec<u8>)>{
    if !path.is_absolute(){return Err(store::Error("unsafe_worker_path"));}
    let mut options=std::fs::OpenOptions::new();options.read(true);
    #[cfg(windows)] {use std::os::windows::fs::OpenOptionsExt;options.share_mode(1).custom_flags(0x00200000);}
    let mut file=options.open(path).map_err(|_|store::Error("worker_pin_unavailable"))?;
    let meta=file.metadata().map_err(|_|store::Error("worker_pin_unavailable"))?;
    #[cfg(windows)] {use std::os::windows::fs::MetadataExt;if meta.file_attributes()&0x400!=0{return Err(store::Error("unsafe_worker_path"));}}
    if !meta.is_file() || meta.len()==0 || meta.len()>cap{return Err(store::Error("worker_pin_unavailable"));}
    let mut bytes=Vec::new();Read::by_ref(&mut file).take(cap+1).read_to_end(&mut bytes).map_err(|_|store::Error("worker_pin_unavailable"))?;
    if bytes.len() as u64!=meta.len(){return Err(store::Error("worker_pin_changed"));}Ok((file,bytes))
}
fn pins(r:&admission::Request,f:&Files)->store::Result<Pins>{
    let(e,eb)=read(&f.executable,128*1024*1024)?;let(h,hb)=read(&f.helper,8*1024*1024)?;let(a,ab)=read(&f.approval,16384)?;
    if hash(&eb)!=r.executable_sha256 || hash(&hb)!=r.helper_sha256 || hash(&ab)!=r.approval_sha256{return Err(store::Error("worker_pin_mismatch"));}
    Ok(Pins{_executable:e,_helper:h,_approval:a})
}
/// Implementations must poll without blocking and own only their child. Stop
/// requests are cooperative/bounded; false means unresolved and never retry.
pub trait Child:Send {fn poll(&mut self)->store::Result<Option<bool>>;fn stop(&mut self)->bool;}
/// No implementation is installed. A native adapter must retain module/config
/// leases, enforce unchanged GPU preflight and contain descendants on shutdown.
pub trait Launcher:Send+Sync {
    fn launch(&self,r:&admission::Request,files:&Files,pins:&Pins)->store::Result<Box<dyn Child>>;
}
struct Verified {request:admission::Request,evidence:admission::Completion,bytes_sha:String}
impl admission::Authority for Verified {
    fn approves(&self,_:&admission::Request,_:i64)->bool{false}
    fn completion(&self,r:&admission::Request,e:&admission::Completion,bytes:&[u8])->bool{
        r==&self.request && e==&self.evidence && hash(bytes)==self.bytes_sha
    }
}
/// Real native receipt verifier, always read-only. The fixed workspace, exact
/// worker job and selected cut must match the reserved admission.
fn import_receipt(app:&App,id:&str,r:&admission::Request,now:i64)->store::Result<store::Job>{
    let root=Path::new(&r.workspace);let key=r.selection.durable_cut_key.hex();
    let (_lease,raw)=read(&root.join(format!("{key}.receipt.json")),16384)?;
    let value:serde_json::Value=serde_json::from_slice(&raw).map_err(|_|store::Error("completion_receipt_unverified"))?;
    if value.get("job_id").and_then(|v|v.as_str())!=Some(&r.worker_job_id){return Err(store::Error("completion_worker_mismatch"));}
    let (receipt,_)=read(&root.join(format!("{key}.receipt.json")),16384)?;
    let (_artifact,bytes)=read(&root.join(format!("{key}.mp4")),store::PREVIEW_LIMIT as u64)?;
    let mut queue=Connection::open_with_flags(root.join("queue.sqlite"),OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let proof=crate::controller_receipt::read_native_receipt(&mut queue,receipt,&r.selection,&r.selection.source_sha256.hex(),&key,store::PREVIEW_LIMIT as u64)
        .map_err(|_|store::Error("completion_receipt_unverified"))?;
    if proof.selection!=r.selection || proof.artifact.bytes!=bytes.len() as u64 || proof.artifact.sha256.hex()!=hash(&bytes){return Err(store::Error("completion_artifact_mismatch"));}
    if proof.editorial_review.as_ref().is_some_and(|v|!v.matches(&r.selection,&r.worker_job_id,Some(&r.approval_sha256))){return Err(store::Error("completion_editorial_review_mismatch"));}
    let e=admission::Completion{worker_job_id:r.worker_job_id.clone(),receipt_sha256:hash(&raw),artifact_sha256:hash(&bytes),selection:r.selection.clone(),editorial_review:proof.editorial_review};
    let authority=Verified{request:r.clone(),evidence:e.clone(),bytes_sha:hash(&bytes)};
    let mut db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
    admission::complete(&mut db,id,&e,&bytes,&authority,now)
}
pub(crate) fn wait(child:&mut dyn Child,limit:Duration)->store::Result<()> {
    let deadline=Instant::now()+limit;
    loop {
        match child.poll(){Ok(Some(true))=>return Ok(()),Ok(Some(false))=>return Err(store::Error("worker_failed")),
            Err(_)=>{child.stop();return Err(store::Error("worker_unresolved"));},Ok(None)=>{}}
        if Instant::now()>=deadline{child.stop();return Err(store::Error("worker_timeout_hold"));}
        std::thread::sleep(Duration::from_millis(10));
    }
}
pub fn start(app:App,id:String,files:Files,authority:Arc<dyn admission::Authority+Send+Sync>,launcher:Arc<dyn Launcher>,seconds:u64)
    ->store::Result<std::thread::JoinHandle<store::Result<store::Job>>> {
    if !(1..=180).contains(&seconds){return Err(store::Error("bounded_supervisor_required"));}
    let now=chrono::Utc::now().timestamp();
    let r={let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;admission::reserved(&db,&id)?};
    if !authority.approves(&r,now){return Err(store::Error("manual_admission_denied"));}
    let leases=pins(&r,&files)?;
    let j={let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;store::job(&db,&r.job_id)?};
    let source=crate::controller_source::controller_file(&app,j.source_id,&j.source_ref).map_err(store::Error)?;
    let (source_lease,source_bytes)=read(&source,128*1024*1024)?;
    if hash(&source_bytes)!=r.selection.source_sha256.hex(){return Err(store::Error("source_changed"));}
    {let mut db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;admission::consume_launch(&mut db,&id,&r,now)?;}
    let thread_app=app.clone();let thread_id=id.clone();
    match std::thread::Builder::new().name("controller-native-supervisor".into()).spawn(move||{
        let _source_lease=source_lease;
        let result=(||{
            if !authority.approves(&r,chrono::Utc::now().timestamp()){return Err(store::Error("manual_admission_denied"));}
            let mut child=launcher.launch(&r,&files,&leases)?;
            wait(child.as_mut(),Duration::from_secs(seconds))?;
            import_receipt(&thread_app,&thread_id,&r,chrono::Utc::now().timestamp())
        })();
        if result.is_err(){if let Ok(mut db)=thread_app.db.lock(){let _=admission::hold(&mut db,&thread_id,chrono::Utc::now().timestamp());}}
        result
    }){Ok(handle)=>Ok(handle),Err(_)=>{
        if let Ok(mut db)=app.db.lock(){let _=admission::hold(&mut db,&id,now);}
        Err(store::Error("supervisor_start_failed"))
    }}
}

#[cfg(test)] mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize,Ordering};
    struct Approved(admission::Request);
    impl admission::Authority for Approved{
        fn approves(&self,r:&admission::Request,now:i64)->bool{r==&self.0 && now<r.expires_at}
        fn completion(&self,_:&admission::Request,_:&admission::Completion,_:&[u8])->bool{false}
    }
    struct FakeChild{pending:bool,stops:Arc<AtomicUsize>}
    impl Child for FakeChild {
        fn poll(&mut self)->store::Result<Option<bool>>{Ok(if self.pending{None}else{Some(true)})}
        fn stop(&mut self)->bool{self.stops.fetch_add(1,Ordering::SeqCst);true}
    }
    struct FakeLaunch{mismatch:bool,pending:bool,starts:Arc<AtomicUsize>,stops:Arc<AtomicUsize>}
    impl Launcher for FakeLaunch{
        fn launch(&self,r:&admission::Request,_:&Files,_:&Pins)->store::Result<Box<dyn Child>>{
            self.starts.fetch_add(1,Ordering::SeqCst);
            if !self.pending {
                let root=Path::new(&r.workspace);std::fs::create_dir(root).unwrap();let key=r.selection.durable_cut_key.hex();
                let bytes=b"fake completed media";
                let worker=if self.mismatch{"wrong-worker"}else{&r.worker_job_id};
                let raw=serde_json::json!({"job_id":worker,"clip_id":r.selection.source_sha256.hex(),"revision":key,"kind":"render","sha256":hash(bytes),"bytes":bytes.len(),"duration":"30.0","recipe":"recipe-v1:cut=1000-31000","caption_status":r.selection.caption_status.as_str(),"publication_enabled":false}).to_string();
                let db=Connection::open(root.join("queue.sqlite"))?;
                db.execute_batch("CREATE TABLE jobs(job_id TEXT,state TEXT,clip_id TEXT,video_hash TEXT,kind TEXT);CREATE TABLE manager(x);CREATE TABLE queue_hold(x);CREATE TABLE completion_proofs(job_id TEXT,receipt TEXT);")?;
                db.execute("INSERT INTO jobs VALUES(?,'finished',?,?,'render')",rusqlite::params![worker,r.selection.source_sha256.hex(),key])?;
                db.execute("INSERT INTO completion_proofs VALUES(?,?)",rusqlite::params![worker,raw])?;drop(db);
                std::fs::write(root.join(format!("{key}.receipt.json")),raw).unwrap();std::fs::write(root.join(format!("{key}.mp4")),bytes).unwrap();
            }
            Ok(Box::new(FakeChild{pending:self.pending,stops:self.stops.clone()}))
        }
    }
    fn fixture()->(App,admission::Request,String,Files,Arc<Approved>){
        let(app,mut r)=admission::tests::setup();let now=chrono::Utc::now().timestamp();r.expires_at=now+120;
        let files=Files{executable:app.media.join("inert-runtime"),helper:app.media.join("inert-helper"),approval:app.media.join("review.json")};
        std::fs::write(&files.executable,b"fake runtime pin").unwrap();std::fs::write(&files.helper,b"fake helper pin").unwrap();std::fs::write(&files.approval,b"independent fake review").unwrap();
        let authority=Arc::new(Approved(r.clone()));
        let id=admission::reserve(&mut app.db.lock().unwrap(),"admit_one_clip",&r,authority.as_ref(),now).unwrap();
        (app,r,id,files,authority)
    }
    fn worker(mismatch:bool,pending:bool)->Arc<FakeLaunch>{Arc::new(FakeLaunch{mismatch,pending,starts:Arc::new(AtomicUsize::new(0)),stops:Arc::new(AtomicUsize::new(0))})}
    #[test] fn supervisor_pin_mismatch_no_launch(){
        let(app,_,id,files,a)=fixture();std::fs::write(&files.helper,b"changed").unwrap();let w=worker(false,false);
        assert!(start(app.clone(),id,files,a,w.clone(),1).is_err());assert_eq!(w.starts.load(Ordering::SeqCst),0);
        assert_eq!(app.db.lock().unwrap().query_row("SELECT count(*) FROM controller_admission_launches",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
    #[test] fn supervisor_native_receipt_duplicate_and_reopen(){
        let(app,r,id,files,a)=fixture();let w=worker(false,false);
        let ready=start(app.clone(),id.clone(),files,a,w.clone(),1).unwrap().join().unwrap().unwrap();
        assert_eq!(ready.state,"READY_FOR_REVIEW");assert_eq!(ready.review,"PENDING");assert_eq!(w.starts.load(Ordering::SeqCst),1);
        let changes=app.db.lock().unwrap().total_changes();assert_eq!(import_receipt(&app,&id,&r,chrono::Utc::now().timestamp()).unwrap(),ready);assert_eq!(app.db.lock().unwrap().total_changes(),changes);
        let path=app.media.join("supervisor-reopen.sqlite");app.db.lock().unwrap().execute("VACUUM INTO ?",[path.to_str().unwrap()]).unwrap();
        let mut db=Connection::open(path).unwrap();store::init(&db).unwrap();store::recover(&mut db,chrono::Utc::now().timestamp()).unwrap();
        assert_eq!(store::preview(&db,&r.job_id).unwrap(),b"fake completed media");assert!(store::settings(&db).unwrap().paused);
    }

    #[cfg(windows)] #[test] #[ignore = "explicit retained read-only diagnosis; no worker or mutations"]
    fn retained_readonly_prelaunch_predicates(){
        use std::sync::Mutex;
        let path=std::path::PathBuf::from(std::env::var("LIFEINVADER_READONLY_DIAG_PLAN").expect("exact plan required"));
        let v:serde_json::Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let r:admission::Request=serde_json::from_value(v["request"].clone()).unwrap();
        let b:crate::controller_native_launcher::RuntimeBinding=serde_json::from_value(v["runtime"].clone()).unwrap();
        let db=Connection::open_with_flags(v["db"].as_str().unwrap(),OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let db_path:String=db.query_row("SELECT file FROM pragma_database_list WHERE name='main'",[],|row|row.get(0)).unwrap();
        let stage=|name:&str,result:store::Result<()>|{println!("READONLY_STAGE {name} {:?}",result);result.unwrap();};
        stage("DB_CLEAN",crate::controller_native_launcher::clean_path(Path::new(&db_path)));
        stage("DB_SCOPE",if Path::new(&db_path).starts_with(&b.isolated_root){Ok(())}else{Err(store::Error("isolated_controller_db_required"))});
        let media=PathBuf::from(v["media"].as_str().unwrap());
        stage("MEDIA_SCOPE",if media.starts_with(&b.isolated_root){Ok(())}else{Err(store::Error("isolated_runtime_admission_denied"))});
        let _config=crate::controller_native_launcher::check_config(&r,&b).unwrap();println!("READONLY_STAGE CONFIG Ok");
        stage("WORKSPACE",if Path::new(&r.workspace).is_dir()&&std::fs::read_dir(&r.workspace).unwrap().next().is_none(){Ok(())}else{Err(store::Error("fresh_worker_workspace_required"))});
        let files=Files{executable:v["executable"].as_str().unwrap().into(),helper:v["helper"].as_str().unwrap().into(),approval:v["approval"].as_str().unwrap().into()};
        let _pins=pins(&r,&files).unwrap();println!("READONLY_STAGE PINS Ok");
        let j=store::job(&db,&r.job_id).unwrap();
        let app=App{cache_scope:"adapted-isolated-test".into(),controller_ui:Arc::new(crate::controller_ui_lifetime::Lifetime::new(false)),controller_catalog:None,controller_manual:Arc::new(crate::controller_manual::Unavailable),controller_ops:Arc::new(crate::controller_ops::Unavailable),controller_worker:Arc::new(crate::controller_worker::Unavailable),db:Arc::new(Mutex::new(db)),media:media.clone(),log_dir:media};
        let resolved=crate::controller_source::controller_file(&app,j.source_id,&j.source_ref);
        if resolved==Err("source_unavailable") {
            let db=app.db.lock().unwrap();
            let error=crate::source::account_profile(&db,j.source_id,"source").unwrap_err();
            assert!(error.1.contains("Sync supports TikTok, Instagram and YouTube account links."));
            assert_eq!(db.total_changes(),0);
            println!("PROVEN SOURCE_PROFILE_HOST_REJECTED BEFORE_LAUNCH NO_MUTATIONS");return;
        }
        let source=resolved.unwrap();println!("READONLY_STAGE SOURCE_RESOLVE Ok");
        let (_source,bytes)=read(&source,128*1024*1024).unwrap();
        stage("SOURCE_HASH",if hash(&bytes)==r.selection.source_sha256.hex(){Ok(())}else{Err(store::Error("source_changed"))});
        let db=app.db.lock().unwrap();
        stage("SOURCE_CURRENT",if crate::controller_source::controller_current(&db,j.source_id,&r.source_binding){Ok(())}else{Err(store::Error("stale_revision"))});
        // No reserved(), reserve(), consume_launch(), authority renewal or launch.
        assert_eq!(db.total_changes(),0);println!("READONLY_COMPLETE NO_MUTATIONS_NO_DISPATCH");
    }
    #[test] #[ignore] fn prepare_fresh_review_with_real_startup_init(){
        let path=PathBuf::from(std::env::var("LIFEINVADER_PREPARE_FRESH_PLAN").expect("explicit fresh plan"));
        let v:serde_json::Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let db_path=PathBuf::from(v["db"].as_str().unwrap());let root=PathBuf::from(v["runtime"]["isolated_root"].as_str().unwrap());
        assert!(db_path.starts_with(&root));assert!(!db_path.exists(),"never repair/reuse a DB");
        let mut db=Connection::open(&db_path).unwrap();
        crate::init(&db).unwrap();store::init(&db).unwrap();store::recover(&mut db,chrono::Utc::now().timestamp()).unwrap();
        assert_eq!(db.query_row("SELECT count(*) FROM profiles",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        db.execute("INSERT INTO profiles(id,name,platform,url,role,source_folder) VALUES(1,'Isolated synthetic review','local','lifeinvader-local://synthetic-review','source','synthetic-source')",[]).unwrap();
        db.execute("INSERT OR REPLACE INTO settings(key,value) VALUES('source_root',?)",[v["media"].as_str().unwrap()]).unwrap();
        let r:admission::Request=serde_json::from_value(v["request"].clone()).unwrap();let t=chrono::Utc::now().timestamp();
        db.execute("INSERT INTO controller_jobs(id,source_id,source_ref,source_binding,platform,state,review,hold_reason,retry_safe,revision,created_at,updated_at) VALUES(?,1,'synthetic-source/original.mp4',?,'youtube','QUEUED','PENDING','',0,0,?,?)",rusqlite::params![r.job_id,r.source_binding,t,t]).unwrap();
        // Exercise the actual initialization/recovery path on the exact seeded DB.
        crate::init(&db).unwrap();store::init(&db).unwrap();store::recover(&mut db,t).unwrap();
        assert_eq!(db.query_row("SELECT schema_version FROM controller_meta WHERE singleton=1",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert_eq!(db.query_row("SELECT version FROM controller_admission_meta",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert_eq!(db.query_row("SELECT count(*) FROM controller_admissions",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        let j=store::job(&db,&r.job_id).unwrap();assert_eq!(j.state,"QUEUED");assert_eq!(j.revision,0);assert_eq!(j.review,"PENDING");
        assert!(store::settings(&db).unwrap().paused);
        db.close().unwrap();println!("REAL_STARTUP_INIT_RECOVER_SEEDED_DB_PASS VERSIONS_1 ZERO_ADMISSIONS QUEUED_REV0 NO_DISPATCH");
    }
    #[test] fn supervisor_receipt_mismatch_and_timeout_hold(){
        for pending in [false,true]{
            let(app,r,id,files,a)=fixture();let w=worker(!pending,pending);
            assert!(start(app.clone(),id.clone(),files,a,w.clone(),1).unwrap().join().unwrap().is_err());
            assert_eq!(w.starts.load(Ordering::SeqCst),1);assert_eq!(w.stops.load(Ordering::SeqCst),usize::from(pending));
            let mut db=app.db.lock().unwrap();assert_eq!(store::job(&db,&r.job_id).unwrap().state,"HELD");
            store::recover(&mut db,chrono::Utc::now().timestamp()).unwrap();assert!(admission::reserved(&db,&id).is_err());
            assert_eq!(db.query_row("SELECT count(*) FROM controller_admission_launches",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        }
    }
}
