//! Explicit isolated manual entry only; never installed by normal startup.
use crate::{App,controller_admission as admission,controller_store as store,controller_supervisor as supervisor,controller_native_process as native};
use std::{path::{Path,PathBuf},sync::{Arc,atomic::{AtomicBool,Ordering}},fs::File};
use serde::{Serialize,Deserialize};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeBinding {
    pub isolated_root:PathBuf,pub config:PathBuf,pub config_sha256:String,
    pub manifest:PathBuf,pub manifest_sha256:String,
}
/// An independently reviewed runtime/config binding is required in addition to
/// cut admission. File hashes or a boolean configuration are not authority.
pub trait RuntimeAuthority:Send+Sync {
    fn approves_runtime(&self,request:&admission::Request,binding:&RuntimeBinding,now:i64)->bool;
}
struct IsolatedLauncher {cancel:Arc<AtomicBool>,request:admission::Request,binding:RuntimeBinding,authority:Arc<dyn RuntimeAuthority>,_config_lease:File}
pub(crate) fn clean_path(path:&Path)->store::Result<()> {
    use std::os::windows::fs::MetadataExt;
    if !path.is_absolute() || path.components().any(|c|matches!(c,std::path::Component::ParentDir)){return Err(store::Error("isolated_path_invalid"));}
    for p in path.ancestors(){if p.exists() && std::fs::symlink_metadata(p).map_err(|_|store::Error("isolated_path_unavailable"))?.file_attributes()&0x400!=0{return Err(store::Error("isolated_path_reparse"));}}
    Ok(())
}
pub(crate) fn check_config(r:&admission::Request,b:&RuntimeBinding)->store::Result<File>{
    let work=Path::new(&r.workspace);
    let disk_path=|p:&Path| p.is_absolute() && matches!(p.components().next(),
        Some(std::path::Component::Prefix(prefix)) if matches!(prefix.kind(),std::path::Prefix::Disk(_)));
    if ![b.isolated_root.as_path(),b.config.as_path(),b.manifest.as_path(),work].into_iter().all(disk_path)
        || work!=b.isolated_root.join("worker")
        || !b.config.starts_with(&b.isolated_root) || !b.manifest.starts_with(&b.isolated_root)
        || b.config==b.manifest {
        return Err(store::Error("isolated_runtime_scope"));
    }
    for p in [b.isolated_root.as_path(),b.config.as_path(),b.manifest.as_path(),work] {clean_path(p)?;}
    if !work.is_dir(){return Err(store::Error("isolated_runtime_workspace_missing"));}
    if !b.manifest.is_file(){return Err(store::Error("runtime_manifest_missing"));}
    let _manifest=native::lease(&b.manifest,&b.manifest_sha256)
        .map_err(|_|store::Error("runtime_manifest_pin_invalid"))?;
    if _manifest.metadata().map_err(|_|store::Error("runtime_manifest_read"))?.len()>8*1024*1024{
        return Err(store::Error("runtime_manifest_size"));
    }
    let file=native::lease(&b.config,&b.config_sha256)?;
    if file.metadata().map_err(|_|store::Error("runtime_config_read"))?.len()>16384{return Err(store::Error("runtime_config_size"));}
    let raw=std::fs::read(&b.config).map_err(|_|store::Error("runtime_config_read"))?;
    if raw.len()>16384{return Err(store::Error("runtime_config_size"));}
    let v:serde_json::Value=serde_json::from_slice(&raw).map_err(|_|store::Error("runtime_config_schema"))?;
    if v.get("schema").and_then(|v|v.as_str())!=Some("native-bootstrap-reviewed-job-v1") || v.get("mode").and_then(|v|v.as_str())!=Some("reviewed-job") || v.get("publishing_enabled")!=Some(&serde_json::Value::Bool(false)) || v.get("request")!=Some(&serde_json::to_value(r).map_err(|_|store::Error("runtime_request"))?) || v.get("manifest")!=Some(&serde_json::json!(b.manifest)) || v.get("manifest_sha256").and_then(|v|v.as_str())!=Some(&b.manifest_sha256){return Err(store::Error("runtime_config_binding"));}
    // A diagnostic bootstrap cannot produce a completed receipt and is refused
    // here, rather than consuming a launch and mistaking diagnostics for a job.
    Ok(file)
}
impl supervisor::Launcher for IsolatedLauncher {
    fn launch(&self,r:&admission::Request,files:&supervisor::Files,_:&supervisor::Pins)->store::Result<Box<dyn supervisor::Child>>{
        if r!=&self.request || !self.authority.approves_runtime(r,&self.binding,chrono::Utc::now().timestamp()){return Err(store::Error("runtime_admission_denied"));}
        let _check=check_config(r,&self.binding)?;
        let child=native::Owned::gated_manifest(&files.executable,&files.helper,&self.binding.config,[&r.executable_sha256,&r.helper_sha256,&self.binding.config_sha256],Path::new(&r.workspace),&self.binding.manifest,&self.binding.manifest_sha256)?;
        Ok(Box::new(Cancellable{child,cancel:self.cancel.clone()}))
    }
}
struct Cancellable {child:native::Owned,cancel:Arc<AtomicBool>}
impl supervisor::Child for Cancellable {
    fn poll(&mut self)->store::Result<Option<bool>>{
        if self.cancel.load(Ordering::SeqCst){self.child.cancel();return Err(store::Error("worker_cancelled_hold"));}
        self.child.poll()
    }
    fn stop(&mut self)->bool{self.child.cancel()}
}
/// Explicit caller supplies an already isolated App, reserved admission, and
/// two independently reviewed authorities. No env flag/HTTP/default registration.
pub fn start_isolated_manual(app:App,id:String,files:supervisor::Files,binding:RuntimeBinding,authority:Arc<dyn admission::Authority+Send+Sync>,runtime_authority:Arc<dyn RuntimeAuthority>,cancel:Arc<AtomicBool>,seconds:u64)->store::Result<std::thread::JoinHandle<store::Result<store::Job>>>{
    let r={let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
        let db_path:String=db.query_row("SELECT file FROM pragma_database_list WHERE name='main'",[],|row|row.get(0))?;
        clean_path(Path::new(&db_path))?;
        if !Path::new(&db_path).starts_with(&binding.isolated_root){return Err(store::Error("isolated_controller_db_required"));}
        admission::reserved(&db,&id)?};
    if !app.media.starts_with(&binding.isolated_root) || !runtime_authority.approves_runtime(&r,&binding,chrono::Utc::now().timestamp()){return Err(store::Error("isolated_runtime_admission_denied"));}
    let config_lease=check_config(&r,&binding)?;
    let root=Path::new(&r.workspace);
    if !root.is_dir() || std::fs::read_dir(root).map_err(|_|store::Error("workspace_unavailable"))?.next().is_some(){return Err(store::Error("fresh_worker_workspace_required"));}
    let launcher=IsolatedLauncher{cancel,request:r,binding,authority:runtime_authority,_config_lease:config_lease};
    supervisor::start(app,id,files,authority,Arc::new(launcher),seconds)
}
