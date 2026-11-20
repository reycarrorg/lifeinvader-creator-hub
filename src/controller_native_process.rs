//! Windows process-containment primitive. Not registered as a production Launcher.
use std::{fs::File,io::{Read,Write},os::windows::{io::AsRawHandle,process::CommandExt,fs::OpenOptionsExt},path::Path,
    process::{Command,Stdio,Child},time::{Duration,Instant}};
use crate::controller_store as store;
use sha2::{Digest,Sha256};
type Handle=isize;
#[repr(C)] #[derive(Default)] struct Limits {process_time:i64,job_time:i64,flags:u32,min:usize,max:usize,active:u32,affinity:usize,priority:u32,scheduling:u32}
#[repr(C)] #[derive(Default)] struct Extended {basic:Limits,io:[u64;6],process_memory:usize,job_memory:usize,peak_process:usize,peak_job:usize}
#[repr(C)] #[derive(Default)] struct Accounting {times:[i64;4],faults:u32,total:u32,active:u32,terminated:u32}
#[link(name="kernel32")] unsafe extern "system" {
    fn CreateJobObjectW(attributes:*const std::ffi::c_void,name:*const u16)->Handle;
    fn SetInformationJobObject(job:Handle,class:u32,data:*const std::ffi::c_void,size:u32)->i32;
    fn AssignProcessToJobObject(job:Handle,process:Handle)->i32;
    fn QueryInformationJobObject(job:Handle,class:u32,data:*mut std::ffi::c_void,size:u32,written:*mut u32)->i32;
    fn TerminateJobObject(job:Handle,code:u32)->i32;
    fn CloseHandle(handle:Handle)->i32;
}
#[link(name="shell32")] unsafe extern "system" {
    fn SHGetFolderPathW(hwnd:Handle,folder:i32,token:Handle,flags:u32,path:*mut u16)->i32;
}
fn program_files_directory()->store::Result<std::path::PathBuf>{
    // CSIDL_PROGRAM_FILES in this x64 owner: Windows resolves the machine folder.
    // No inherited ProgramFiles/PATH/profile fallback.
    let mut buf=[0u16;260];
    if unsafe{SHGetFolderPathW(0,0x26,0,0,buf.as_mut_ptr())}!=0{return Err(store::Error("program_files_unavailable"));}
    let n=buf.iter().position(|c|*c==0).ok_or(store::Error("program_files_unavailable"))?;
    let path=std::path::PathBuf::from(String::from_utf16(&buf[..n]).map_err(|_|store::Error("program_files_unavailable"))?);
    if !path.is_absolute() || !path.is_dir(){return Err(store::Error("program_files_unavailable"));}
    use std::os::windows::fs::MetadataExt;
    for p in path.ancestors(){if std::fs::symlink_metadata(p).map_err(|_|store::Error("program_files_unavailable"))?.file_attributes()&0x400!=0{return Err(store::Error("program_files_reparse"));}}
    Ok(path)
}
fn minimal_environment(command:&mut Command,system_root:&std::ffi::OsStr,root:&Path,program_files:&Path){
    command.env_clear().env("SystemRoot",system_root).env("TEMP",root).env("TMP",root).env("ProgramFiles",program_files);
}
struct Job(Handle);
impl Job {
    fn new()->store::Result<Self>{unsafe{
        let handle=CreateJobObjectW(std::ptr::null(),std::ptr::null());if handle==0{return Err(store::Error("job_create_failed"));}
        let job=Self(handle);let mut limits=Extended::default();limits.basic.flags=0x2000; // KILL_ON_JOB_CLOSE; no breakaway
        if SetInformationJobObject(handle,9,&limits as *const _ as _,std::mem::size_of::<Extended>() as u32)==0{return Err(store::Error("job_limits_failed"));}Ok(job)
    }}
    fn active(&self)->store::Result<u32>{unsafe{let mut a=Accounting::default();
        if QueryInformationJobObject(self.0,1,&mut a as *mut _ as _,std::mem::size_of::<Accounting>() as u32,std::ptr::null_mut())==0{return Err(store::Error("job_query_failed"));}Ok(a.active)}}
    fn terminate(&self)->bool{unsafe{TerminateJobObject(self.0,30)!=0}}
}
impl Drop for Job {fn drop(&mut self){unsafe{CloseHandle(self.0);}}}
pub(crate) fn lease(path:&Path,pin:&str)->store::Result<File>{
    if !path.is_absolute(){return Err(store::Error("pin_path"));}
    let mut f=std::fs::OpenOptions::new().read(true).share_mode(1).custom_flags(0x00200000).open(path).map_err(|_|store::Error("pin_open"))?;
    use std::os::windows::fs::MetadataExt;
    let m=f.metadata().map_err(|_|store::Error("pin_open"))?;
    if !m.is_file() || m.file_attributes()&0x400!=0 || m.len()>512*1024*1024{return Err(store::Error("pin_kind"));}
    let mut hash=Sha256::new();let mut buf=[0u8;65536];loop{let n=f.read(&mut buf).map_err(|_|store::Error("pin_read"))?;if n==0{break;}hash.update(&buf[..n]);}
    if format!("{:x}",hash.finalize())!=pin{return Err(store::Error("pin_mismatch"));}Ok(f)
}
pub struct Owned {child:Child,job:Job,_leases:[File;3],_dependencies:Vec<File>,finished:bool,pub forced:bool}
impl Owned {
    pub fn gated_manifest(python:&Path,helper:&Path,config:&Path,pins:[&str;3],root:&Path,manifest:&Path,manifest_pin:&str)->store::Result<Self>{
        let started=Instant::now();let mut trace=std::fs::OpenOptions::new().write(true).create_new(true).open(root.join("native-phases.txt")).map_err(|_|store::Error("diagnostic_collision"))?;
        let mut mark=|label:&str|->store::Result<()>{writeln!(trace,"{:.6} {}",started.elapsed().as_secs_f64(),label).map_err(|_|store::Error("diagnostic_write"))?;trace.flush().map_err(|_|store::Error("diagnostic_write"))};
        mark("MANIFEST_LEASE_BEGIN")?;
        let manifest_lease=lease(manifest,manifest_pin)?;
        mark("MANIFEST_LEASE_COMPLETE")?;
        #[derive(serde::Deserialize)] #[serde(deny_unknown_fields)] struct Manifest {schema:String,files:std::collections::BTreeMap<String,String>}
        let raw=std::fs::read(manifest).map_err(|_|store::Error("manifest_read"))?;
        if raw.len()>8*1024*1024{return Err(store::Error("manifest_size"));}
        let m:Manifest=serde_json::from_slice(&raw).map_err(|_|store::Error("manifest_schema"))?;
        let pin_at=|target:&Path|m.files.iter().find(|(p,_)|Path::new(p.as_str())==target).map(|(_,pin)|pin.as_str());
        if m.schema!="native-runtime-files-v1" || m.files.len()>20000 || pin_at(python)!=Some(pins[0]) || pin_at(helper)!=Some(pins[1]) {return Err(store::Error("manifest_identity"));}
        let mut dependencies=vec![manifest_lease];
        mark("DEPENDENCY_LEASES_BEGIN")?;
        for (path,pin) in m.files {dependencies.push(lease(Path::new(&path),&pin)?);}
        mark("DEPENDENCY_LEASES_COMPLETE")?;
        let mut child=Self::gated(python,helper,config,pins,root)?;child._dependencies=dependencies;mark(&format!("CHILD_STARTED_PID_{}",child.pid()))?;Ok(child)
    }
    /// Trusted pinned bootstrap MUST block on GO before importing/launching work.
    /// -I -S disables user/site startup hooks; no shell or inherited Python env.
    /// This constructor is not a general arbitrary-script Launcher.
    pub fn gated(python:&Path,helper:&Path,config:&Path,pins:[&str;3],root:&Path)->store::Result<Self>{
        let leases=[lease(python,pins[0])?,lease(helper,pins[1])?,lease(config,pins[2])?];
        if !root.is_absolute() || !root.is_dir(){return Err(store::Error("workspace_missing"));}
        let job=Job::new()?;
        let system_root=std::env::var_os("SystemRoot").ok_or(store::Error("system_root_missing"))?;
        let program_files=program_files_directory()?;
        let mut command=Command::new(python);
        minimal_environment(&mut command,&system_root,root,&program_files);
        let mut child=command.args(["-I","-S","-B","-u"]).arg(helper).arg(config).current_dir(root)
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(0x08000000)
            .spawn().map_err(|_|store::Error("child_start_failed"))?;
        if unsafe{AssignProcessToJobObject(job.0,child.as_raw_handle() as Handle)}==0{
            let _=child.kill();let _=child.wait();return Err(store::Error("job_assignment_failed"));
        }
        let mut owned=Self{child,job,_leases:leases,_dependencies:Vec::new(),finished:false,forced:false};
        owned.child.stdin.as_mut().ok_or(store::Error("child_control_missing"))?.write_all(b"GO\n").map_err(|_|store::Error("child_control_failed"))?;
        Ok(owned)
    }
    pub fn pid(&self)->u32{self.child.id()}
    pub fn active(&self)->store::Result<u32>{self.job.active()}
    pub fn poll(&mut self)->store::Result<Option<bool>>{
        match self.child.try_wait().map_err(|_|store::Error("child_status_unknown"))? {
            None=>Ok(None),Some(status)=>{
                let clean=self.job.active()?==0;
                // Root exit can precede the last owned process/accounting update.
                // Reuse the existing cooperative drain bound; never adopt or detach.
                if !clean && !self.drained(Duration::from_millis(500)) {self.cleanup();}
                self.finished=self.job.active()?==0;Ok(Some(status.success() && !self.forced && self.finished))
            }
        }
    }
    fn drained(&mut self,limit:Duration)->bool{
        let end=Instant::now()+limit;loop{
            let exited=self.child.try_wait().ok().flatten().is_some();
            if exited && self.job.active()==Ok(0){self.finished=true;return true;}
            if Instant::now()>=end{return false;}std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn cleanup(&mut self)->bool{self.forced=true;if !self.job.terminate(){return false;}self.drained(Duration::from_secs(2))}
    pub fn cancel(&mut self)->bool{
        if self.finished{return true;}
        if let Some(mut stdin)=self.child.stdin.take(){let _=stdin.write_all(b"STOP\n");}
        if self.drained(Duration::from_millis(500)){true}else{self.cleanup()}
    }
}
impl crate::controller_supervisor::Child for Owned {
    fn poll(&mut self)->store::Result<Option<bool>>{Owned::poll(self)}
    fn stop(&mut self)->bool{self.cancel()}
}
impl Drop for Owned {fn drop(&mut self){if !self.finished{self.cancel();}}}

#[cfg(test)] mod tests {
    use super::*;
    fn hash(p:&Path)->String{format!("{:x}",Sha256::digest(std::fs::read(p).unwrap()))}
    #[test] fn native_programfiles_allowlist_is_machine_only(){
        let known=program_files_directory().unwrap();assert!(known.is_absolute());assert!(known.is_dir());
        let mut cmd=Command::new("inert-not-executed");cmd.env("UNRELATED","must-clear");
        minimal_environment(&mut cmd,std::ffi::OsStr::new("C:\\Windows"),Path::new("C:\\isolated"),&known);
        let env=cmd.get_envs().map(|(k,v)|(k.to_string_lossy().to_string(),v.unwrap().to_os_string())).collect::<std::collections::BTreeMap<_,_>>();
        assert_eq!(env.keys().map(String::as_str).collect::<Vec<_>>(),vec!["ProgramFiles","SystemRoot","TEMP","TMP"]);
        assert_eq!(env["ProgramFiles"],known.as_os_str());
    }
    #[test] #[ignore = "requires explicitly scoped local runtime manifest and native diagnostic approval"] fn instrumented_bootstrap_once(){
        let repo=Path::new(env!("CARGO_MANIFEST_DIR"));let root=std::env::var_os("LIFEINVADER_TEST_DIAGNOSTIC_ROOT").map(std::path::PathBuf::from).unwrap_or_else(||repo.join("tools/bootstrap-instrumented-02"));
        let python=std::path::PathBuf::from(std::env::var_os("LIFEINVADER_TEST_PYTHON").unwrap());
        let helper=std::env::var_os("LIFEINVADER_TEST_HELPER").map(std::path::PathBuf::from).unwrap_or_else(||repo.join("helpers/clip_worker/native_bootstrap.py"));let config=root.join("config.json");let manifest=root.join("manifest.json");
        let hp=std::env::var("LIFEINVADER_TEST_HELPER_SHA256").unwrap();let mp=std::env::var("LIFEINVADER_TEST_MANIFEST_SHA256").unwrap();let cp=std::env::var("LIFEINVADER_TEST_CONFIG_SHA256").unwrap();
        let pp=std::env::var("LIFEINVADER_TEST_PYTHON_SHA256").unwrap();
        let mut child=Owned::gated_manifest(&python,&helper,&config,[&pp,&hp,&cp],&root,&manifest,&mp).unwrap();
        let started=Instant::now();let mut complete=false;
        let seconds=std::env::var("LIFEINVADER_TEST_IMPORT_BOUND").ok().map(|v|v.parse::<u64>().unwrap()).unwrap_or(60);assert!((1..=60).contains(&seconds));
        while started.elapsed()<Duration::from_secs(seconds){
            if let Some(ok)=child.poll().unwrap(){complete=ok;break;}
            std::thread::sleep(Duration::from_millis(20));
        }
        let before_cancel=started.elapsed().as_secs_f64();let cleanup=child.cancel();
        let record=serde_json::json!({"pid":child.pid(),"completed":complete,"elapsed":before_cancel,"cleanup":cleanup,"active_after":child.active().unwrap(),"forced":child.forced,"exit":child.child.try_wait().unwrap().and_then(|s|s.code()),"at":chrono::Utc::now().to_rfc3339()});
        std::fs::write(root.join("exit.json"),record.to_string()).unwrap();println!("{record}");
        assert!(complete,"instrumented diagnostic HOLD; inspect fixed phase logs");
    }
    #[test] #[ignore = "requires explicitly scoped local runtime manifest and native diagnostic approval"] fn production_bootstrap_diagnostic_pinned_imports_and_cancel(){
        let repo=Path::new(env!("CARGO_MANIFEST_DIR"));let root=repo.join("tools/bootstrap-qualification-01");
        let python=std::path::PathBuf::from(std::env::var_os("LIFEINVADER_TEST_PYTHON").unwrap());
        let pp="4d6f5f81a4bca11191c4c7c6b43632694d0a4ce74e068619d8fdc161d469859a";
        let helper=repo.join("helpers/clip_worker/native_bootstrap.py");let hp="efec91116a4e62d0f9bd33e35a9826d1ed614f8878401189b7cd22fb8e558af1";
        let manifest=root.join("manifest.json");let mp="a34606511e5b5e88116624d6463566ec6d152526cb22e13c5444885a0a270b90";
        for mode in ["imports","cancel"] {
            let work=root.join(mode);let config=work.join("config.json");let config_pin=hash(&config);
            assert!(Owned::gated(&python,&helper,&config,[pp,hp,&"0".repeat(64)],&work).is_err());
            let mut child=Owned::gated_manifest(&python,&helper,&config,[pp,hp,&config_pin],&work,&manifest,mp).unwrap();
            let end=Instant::now()+Duration::from_secs(60);let evidence=work.join("result.json");
            loop {if evidence.exists(){break;}if let Some(status)=child.poll().unwrap(){panic!("diagnostic ended before evidence: {status}, exit={:?}",child.child.try_wait().unwrap());}assert!(Instant::now()<end);std::thread::sleep(Duration::from_millis(20));}
            let result:serde_json::Value=serde_json::from_slice(&std::fs::read(evidence).unwrap()).unwrap();
            assert_eq!(result["gpu_queried"],false);assert_eq!(result["queue_started"],false);assert_eq!(result["model_loaded"],false);
            if mode=="cancel" {assert!(child.cancel());assert!(!child.forced);}else{assert!(crate::controller_supervisor::wait(&mut child,Duration::from_secs(5)).is_ok());}
            assert_eq!(child.active().unwrap(),0);let exit=child.child.try_wait().unwrap().unwrap().code();assert_eq!(exit,Some(if mode=="cancel"{31}else{0}));
            println!("{}",serde_json::json!({"mode":mode,"pid":child.pid(),"exit":exit,"active_after":0,"forced":child.forced,"result":result,"at":chrono::Utc::now().to_rfc3339()}));
        }
    }
    #[test] fn native_job_clean_failure_cooperative_timeout_no_descendants(){
        let python=std::path::PathBuf::from(std::env::var_os("LIFEINVADER_TEST_PYTHON").expect("explicit installed interpreter"));
        let expected=std::env::var("LIFEINVADER_TEST_PYTHON_SHA256").expect("independent interpreter pin");assert_eq!(hash(&python),expected);
        let base=Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/native-process-qualification").join(uuid::Uuid::new_v4().to_string());std::fs::create_dir_all(&base).unwrap();
        let helper=base.join("probe.py");std::fs::write(&helper,include_bytes!("../tests/fixtures/native_process_probe.py")).unwrap();
        for mode in ["clean","delayed_exit","failure","cooperative","stubborn"] {
            let root=base.join(mode);std::fs::create_dir(&root).unwrap();let config=root.join("config.json");let evidence=root.join("pids.json");
            std::fs::write(&config,serde_json::json!({"mode":mode,"evidence":evidence}).to_string()).unwrap();
            let mut child=Owned::gated(&python,&helper,&config,[&expected,&hash(&helper),&hash(&config)],&root).unwrap();
            let deadline=Instant::now()+Duration::from_secs(5);
            while !evidence.exists(){assert!(Instant::now()<deadline,"fixture never started");std::thread::sleep(Duration::from_millis(10));}
            let ok=if mode=="clean" || mode=="delayed_exit" || mode=="failure"{loop{if let Some(ok)=child.poll().unwrap(){break ok;}assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(10));}}
                else if mode=="stubborn"{assert_eq!(crate::controller_supervisor::wait(&mut child,Duration::from_millis(30)),Err(store::Error("worker_timeout_hold")));false}
                else{assert!(child.cancel());false};
            assert_eq!(ok,mode=="clean" || mode=="delayed_exit");assert_eq!(child.active().unwrap(),0);assert!(child.child.try_wait().unwrap().is_some());
            assert_eq!(child.forced,mode=="failure" || mode=="stubborn");
            let record=serde_json::json!({"mode":mode,"root_pid":child.pid(),"descendants":serde_json::from_slice::<serde_json::Value>(&std::fs::read(&evidence).unwrap()).unwrap(),"forced_owned_job_cleanup":child.forced,"active_after":0,"root_exit":child.child.try_wait().unwrap().unwrap().code(),"at":chrono::Utc::now().to_rfc3339()});
            std::fs::write(root.join("result.json"),record.to_string()).unwrap();println!("{record}");
        }
        println!("evidence={}",base.display());
    }
}
