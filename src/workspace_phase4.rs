//! Local APP Phase 4, with no publish or deletion authority.
use crate::{App,controller_store as store,workspace_media as media};
use serde_json::{Value,json};
const DEFAULT_SPACING_SECS:u64=3*60*60;
const PROTECTED_PREVIEW_KEY:&str="327f4e520699971aa6c96574bf03418d6331bce826dcc12074d4fbf092c8896c";
#[derive(Default)]
struct Counts {source_preview_bytes:u64, protected_derivative_bytes:u64, phase4_intake_bytes:u64}
fn totals(app:&App)->store::Result<Counts>{
    let source_preview_bytes=media::cache_bytes(app)?;
    let protected_derivative_bytes=crate::workspace_nvenc::open(app,PROTECTED_PREVIEW_KEY).ok()
        .and_then(|(_,f)|f.metadata().ok()).map_or(0,|m|m.len());
    Ok(Counts{source_preview_bytes,protected_derivative_bytes,phase4_intake_bytes:intake_bytes(app)?})
}
#[derive(Clone,Copy,PartialEq,Eq)]
enum Kind {SourceView,Protected,Derived}
struct Eligibility<'a>{
    kind:Kind,active:bool,contained:bool,lease_bound:bool,local_mp4:&'a str,backup_mp4:&'a str,
    local_receipt:&'a str,backup_receipt:&'a str
}
fn eligible(e:&Eligibility<'_>)->bool{
    e.kind==Kind::Derived&&!e.active&&e.contained&&e.lease_bound
        &&media::hex(e.local_mp4)&&e.local_mp4==e.backup_mp4
        &&media::hex(e.local_receipt)&&e.local_receipt==e.backup_receipt
}
pub(crate) fn cache_usage(app:&App)->store::Result<Value>{
    let c=totals(app)?;
    Ok(json!({"managed_logical_bytes":c.source_preview_bytes+c.protected_derivative_bytes+c.phase4_intake_bytes,
        "source_preview_bytes":c.source_preview_bytes,"protected_derivative_bytes":c.protected_derivative_bytes,"intake_bytes":c.phase4_intake_bytes,
        "deletion_enabled":false}))
}
pub(crate) fn cache_preview(app:&App)->store::Result<Value>{
    let c=totals(app)?;
    // No ordinary derivation cleanup queue is provisioned. All current source
    // previews are source-view copies; the sole NVENC receipt is protected.
    let backup_pair_verified=crate::workspace_phase1::verify_known_backup_pair(app);
    Ok(json!({"schema":"snapmatic-cache-preview-v1","mode":"preview_only","deletion_enabled":false,
        "eligible_count":0,"managed_logical_bytes":c.source_preview_bytes+c.protected_derivative_bytes+c.phase4_intake_bytes,
        "source_preview_bytes":c.source_preview_bytes,"protected_derivative_bytes":c.protected_derivative_bytes,
        "intake_bytes":c.phase4_intake_bytes,"known_protected_backup_pair_verified":backup_pair_verified,
        "reason":"No unprotected staged derivative with a matching verified backup video and receipt."}))
}
pub(crate) fn default_cadence()->Value{
    json!({"minimum_spacing_secs":DEFAULT_SPACING_SECS,"active":false,"next_due_at_unix":null,"quota_state":"not_verified"})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn intake_identity_is_rederived_and_poster_header_checked(){
        let source="fixtures/known-source.mp4";
        let id=media::intake_identity(source).unwrap();
        let mut r=IntakeRow{clip_id:id.clone(),source_rel:source.into(),media_sha256:"a".repeat(64),media_bytes:4096,
            video:format!("{id}.mp4"),poster:format!("{id}.webp"),poster_sha256:"b".repeat(64),width:540,height:960};
        assert!(r.safe());r.source_rel="../known-source.mp4".into();assert!(!r.safe());r.source_rel=source.into();
        r.poster="subfolder/anything.webp".into();assert!(!r.safe());
        let mut h=[0u8;30];h[0..4].copy_from_slice(b"RIFF");h[8..16].copy_from_slice(b"WEBPVP8X");h[24..27].copy_from_slice(&[27,2,0]);h[27..30].copy_from_slice(&[191,3,0]);
        assert_eq!(webp_dimensions(&h),Some((540,960)));assert_eq!(webp_dimensions(&h[..29]),None);
    }
    #[test] fn admission_never_removes_rewrites_or_adds_multiple_rows(){
        let row=|clip:&str|IntakeRow{clip_id:clip.into(),source_rel:format!("{clip}.mp4"),media_sha256:clip.into(),
            media_bytes:64,video:String::new(),poster:String::new(),poster_sha256:clip.into(),width:540,height:960};
        let a=row("a");let b=row("b");let c=row("c");
        assert!(monotonic_admission(std::slice::from_ref(&a),&[a.clone(),b.clone()]));
        assert!(!monotonic_admission(std::slice::from_ref(&a),&[b.clone()]));
        assert!(!monotonic_admission(std::slice::from_ref(&a),&[a.clone(),b,c]));
        let mut changed=a.clone();changed.media_bytes+=1;
        assert!(!monotonic_admission(std::slice::from_ref(&a),&[changed]));
    }
    #[test] fn only_inactive_scoped_backed_up_derivative_is_eligible(){
        let v="a".repeat(64);let r="b".repeat(64);
        let mut a=Eligibility{kind:Kind::Derived,active:false,contained:true,lease_bound:true,
            local_mp4:&v,backup_mp4:&v,local_receipt:&r,backup_receipt:&r};
        assert!(eligible(&a));
        a.kind=Kind::SourceView;assert!(!eligible(&a));a.kind=Kind::Protected;assert!(!eligible(&a));
        a.kind=Kind::Derived;a.active=true;assert!(!eligible(&a));a.active=false;
        a.backup_receipt=&v;assert!(!eligible(&a));a.backup_receipt=&r;
        a.contained=false;assert!(!eligible(&a));a.contained=true;a.lease_bound=false;assert!(!eligible(&a));
    }
}

// The descriptor is intentionally opt-in; ordinary Phase 4 startup does not
// operate on a Drive source or grant an intake lease.
const VIDEO_MAX:u64=180*1024*1024;
const POSTER_MAX:u64=2*1024*1024;
#[derive(Clone,PartialEq,Eq)]
pub(crate) struct IntakeRow {
    pub clip_id:String, pub source_rel:String, pub media_sha256:String, pub media_bytes:u64,
    pub video:String, pub poster:String, pub poster_sha256:String, pub width:u32,pub height:u32,
}
impl IntakeRow{
    fn safe(&self)->bool{
        let safe_components=!self.source_rel.is_empty()&&self.source_rel.len()<=4096
            && !self.source_rel.contains(['\\',':',char::from(0)])
            && self.source_rel.split('/').all(|part|{
                !part.is_empty()&&part!="."&&part!=".."&&!part.ends_with(['.',' '])&&
                !matches!(part.to_ascii_lowercase().as_str(),"posted clips backup"|"staging"|"app staging"|"app output"|"output"|"outputs"|"cache"|"worker"|"controller")
            });
        safe_components&&media::intake_identity(&self.source_rel).as_deref()==Some(&self.clip_id)&&media::hex(&self.clip_id)&&media::hex(&self.media_sha256)&&media::hex(&self.poster_sha256)
            &&(1..=VIDEO_MAX).contains(&self.media_bytes)&&self.video==format!("{}.mp4",self.clip_id)
            &&self.poster==format!("{}.webp",self.clip_id)&&(self.width,self.height)==(540,960)
    }
}
pub(crate) fn webp_dimensions(b:&[u8])->Option<(u32,u32)>{
    if b.len()<30||b.get(..4)!=Some(b"RIFF")||b.get(8..12)!=Some(b"WEBP"){return None;}
    let le24=|i|u32::from_le_bytes([b[i],b[i+1],b[i+2],0]);
    match b.get(12..16)?{
        b"VP8X"=>Some((1+le24(24),1+le24(27))),
        b"VP8 " if b[23..26]==[0x9d,0x01,0x2a]=>Some((u16::from_le_bytes([b[26],b[27]]) as u32&0x3fff,u16::from_le_bytes([b[28],b[29]]) as u32&0x3fff)),
        b"VP8L" if b[20]==0x2f=>{let n=u32::from_le_bytes([b[21],b[22],b[23],b[24]]);Some((1+(n&0x3fff),1+((n>>14)&0x3fff)))},
        _=>None,
    }
}
#[cfg(windows)]
fn asset(root:&std::path::Path,id:&str,r:&IntakeRow,poster:bool)->store::Result<(std::path::PathBuf,std::fs::File)>{
    use std::io::{Read,Seek,SeekFrom};
    if id!=r.clip_id||!r.safe(){return Err(store::Error("intake_preview_unavailable"));}
    let (name,pin,cap)=if poster{(&r.poster,&r.poster_sha256,POSTER_MAX)}else{(&r.video,&r.media_sha256,VIDEO_MAX)};
    let path=root.join(name);
    crate::controller_native_launcher::clean_path(&path)?;
    let mut f=crate::controller_native_process::lease(&path,pin)?;
    let n=f.metadata().map_err(|_|store::Error("intake_preview_unavailable"))?.len();
    if !(1..=cap).contains(&n)||(!poster&&n!=r.media_bytes){return Err(store::Error("intake_preview_size"));}
    if poster{
        f.seek(SeekFrom::Start(0)).map_err(|_|store::Error("intake_poster_unverified"))?;
        let mut head=[0u8;30];f.read_exact(&mut head).map_err(|_|store::Error("intake_poster_unverified"))?;
        if webp_dimensions(&head)!=Some((540,960)){return Err(store::Error("intake_poster_unverified"));}
    }
    Ok((path,f))
}

#[cfg(windows)]
mod sidecar{
    use super::*;
    use std::path::{Path,PathBuf};
    use rusqlite::{Connection,OpenFlags,OptionalExtension};
    #[derive(Clone)]
    pub(super) struct Scope{pub root:PathBuf,pub db:PathBuf,pub stage:PathBuf,pub fixture:bool}
    fn fixed(path:&Path,p:&Path)->bool {path.is_absolute()&&path.parent()==Some(p)}
    pub(super) fn scope(app:&App)->store::Result<Option<Scope>>{
        let Some(raw)=std::env::var_os("LIFEINVADER_PHASE4_SCOPE")else{return Ok(None)};
        let root=PathBuf::from(raw);
        let active=app.media.parent().ok_or(store::Error("intake_scope_unverified"))?;
        let base=active.parent().ok_or(store::Error("intake_scope_unverified"))?;
        let leaf=root.file_name().and_then(|s|s.to_str());
        let active_leaf=active.file_name().and_then(|s|s.to_str());
        let fixture=matches!(leaf,Some("snapmatic-phase4-app-fixture-01"|"snapmatic-phase4-app-fixture-02"));
        if !fixed(&root,base)||!matches!((leaf,active_leaf),
            (Some("snapmatic-phase4-app-fixture-01"),Some("snapmatic-phase4-app-runtime-01"))|
            (Some("snapmatic-phase4-app-fixture-02"),Some("snapmatic-phase4-app-runtime-02"))|
            (Some("snapmatic-phase4-intake-v1"),Some("snapmatic-review-runtime"))){
            return Err(store::Error("intake_scope_unverified"));
        }
        let db=root.join("snapmatic-phase4.sqlite3");let stage=root.join("stage");
        for p in [&root,&db,&stage]{crate::controller_native_launcher::clean_path(p)?;}
        if !root.is_dir()||!stage.is_dir(){return Err(store::Error("intake_scope_unavailable"));}
        Ok(Some(Scope{root,db,stage,fixture}))
    }
    fn db(s:&Scope)->store::Result<Connection>{
        if !s.db.is_file(){return Err(store::Error("intake_sidecar_unavailable"));}
        let d=Connection::open_with_flags(&s.db,OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        d.busy_timeout(std::time::Duration::from_millis(250))?;
        d.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF;")?;
        Ok(d)
    }
    fn record(r:&rusqlite::Row<'_>)->rusqlite::Result<IntakeRow>{
        Ok(IntakeRow{
            clip_id:r.get(0)?,source_rel:r.get(1)?,media_sha256:r.get(2)?,media_bytes:u64::try_from(r.get::<_,i64>(3)?).map_err(|_|rusqlite::Error::InvalidQuery)?,
            video:r.get(4)?,poster:r.get(5)?,poster_sha256:r.get(6)?,width:u32::try_from(r.get::<_,i64>(7)?).map_err(|_|rusqlite::Error::InvalidQuery)?,height:u32::try_from(r.get::<_,i64>(8)?).map_err(|_|rusqlite::Error::InvalidQuery)?,
        })
    }
    const FIELDS:&str="SELECT clip_id,source_rel,media_sha256,media_bytes,video_name,poster_name,poster_sha256,poster_width,poster_height
        FROM intake_items WHERE state='pending_review'";
    pub(super) fn rows(s:&Scope)->store::Result<Vec<IntakeRow>>{
        let d=db(s)?;
        let sql=format!("{FIELDS} ORDER BY admitted_at_unix DESC,clip_id LIMIT 501");let mut q=d.prepare(&sql)?;
        let all=q.query_map([],record)?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut ids=std::collections::BTreeSet::new();
        let mut digests=std::collections::BTreeSet::new();
        if all.len()>500||all.iter().any(|x|!x.safe()||!ids.insert(&x.clip_id)||!digests.insert(&x.media_sha256)){
            return Err(store::Error("intake_sidecar_unverified"));
        }
        Ok(all)
    }
    pub(super) fn visible(s:&Scope,rows:Vec<IntakeRow>)->store::Result<Vec<IntakeRow>>{
        let configured=std::env::var_os("LIFEINVADER_PHASE4_ADMISSION_SHA256").is_some();
        let ledger=s.root.join("snapmatic-phase4-supervision.sqlite3");crate::controller_native_launcher::clean_path(&ledger)?;
        if !ledger.is_file(){return if configured{Ok(Vec::new())}else{Ok(rows)};}
        let d=Connection::open_with_flags(ledger,OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        d.busy_timeout(std::time::Duration::from_millis(250))?;
        d.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF;")?;
        let mut out=Vec::new();
        for row in rows{
            let pin=d.query_row("SELECT media_sha256,poster_sha256 FROM verified_items WHERE clip_id=?1",
                [&row.clip_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?;
            if pin.as_ref().is_some_and(|(media,poster)|media==&row.media_sha256&&poster==&row.poster_sha256){out.push(row);}
        }
        Ok(out)
    }
    pub(super) fn open(s:&Scope,id:&str,poster:bool)->store::Result<(PathBuf,std::fs::File)>{
        let d=db(s)?;let sql=format!("{FIELDS} AND clip_id=?1 LIMIT 2");
        let mut q=d.prepare(&sql)?;
        let r=q.query_map([id],record)?.collect::<rusqlite::Result<Vec<_>>>()?;
        if r.len()!=1{return Err(store::Error("intake_preview_unavailable"));}
        let r=visible(s,r)?;
        let row=r.first().ok_or(store::Error("intake_preview_unavailable"))?;
        asset(&s.stage,id,row,poster)
    }
    pub(super) fn state(s:&Scope)->store::Result<Value>{
        let d=db(s)?;
        let status=d.query_row("SELECT state,checked_at_unix FROM intake_status WHERE singleton=1",[],
            |r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<i64>>(1)?))).optional()?;
        let (state,time)=status.unwrap_or_else(||("unknown".into(),None));
        let state=match state.as_str(){"ready"|"unavailable"|"reconciling"=>state.as_str(),_=>"unknown"};
        let num=d.query_row("SELECT COUNT(*) FROM intake_items WHERE state='pending_review'",[],|r|r.get::<_,i64>(0))?;
        let mut policy=d.prepare("SELECT platform,cadence_seconds,next_due_at_unix,read_failure_streak,read_retry_at_unix
           FROM platform_policy WHERE platform IN ('youtube','instagram','tiktok') ORDER BY platform LIMIT 3")?;
        let rows=policy.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,Option<i64>>(2)?,r.get::<_,i64>(3)?,r.get::<_,Option<i64>>(4)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let platforms=rows.into_iter().map(|(p,spacing,next,streak,retry)|json!({
            "platform":p,"minimum_spacing_secs":u64::try_from(spacing).ok().filter(|v|(60..=604800).contains(v)),
            "next_due_at_unix":next.filter(|t|*t>0),"read_failure_streak":u64::try_from(streak).ok(),"read_retry_at_unix":retry.filter(|t|*t>0),
            "publishing_enabled":false
        })).collect::<Vec<_>>();
        Ok(json!({"schema":"snapmatic-phase4-v1","source_state":state,"checked_at_unix":time,
            "pending_review":u64::try_from(num).ok(),"platforms":platforms,"publishing_enabled":false,
            "admission_enabled":false,"fixture":s.fixture}))
    }
    pub(super) fn validate_lengths(s:&Scope,items:&[IntakeRow])->store::Result<u64>{
        let mut n=0u64;
        for r in items{
            for (leaf,min,max) in [(&r.video,r.media_bytes,r.media_bytes),(&r.poster,1,POSTER_MAX)]{
                let p=s.stage.join(leaf);crate::controller_native_launcher::clean_path(&p)?;
                let m=std::fs::symlink_metadata(&p).map_err(|_|store::Error("intake_preview_unavailable"))?;
                if !m.is_file()||!(min..=max).contains(&m.len()){return Err(store::Error("intake_preview_size"));}
                n=n.checked_add(m.len()).ok_or(store::Error("intake_preview_size"))?;
            }
        }
        Ok(n)
    }
}
pub(crate) fn state(app:&App)->Value{
    #[cfg(windows)]
    match sidecar::scope(app){
        Ok(Some(s))=>return match sidecar::state(&s){
            Ok(mut v)=>{v["watcher"]=watcher_state();
                v["admission_enabled"]=json!(v["source_state"]=="ready"&&v["watcher"]["state"]=="pending_only_checked");
                if std::env::var_os("LIFEINVADER_PHASE4_ADMISSION_SHA256").is_some(){
                    v["pending_review"]=json!(sidecar::rows(&s).and_then(|r|sidecar::visible(&s,r)).ok().map(|r|r.len()));
                }
                v},
            Err(_)=>json!({"schema":"snapmatic-phase4-v1","source_state":"unavailable","pending_review":null,"fixture":s.fixture,
                "platforms":[],"watcher":watcher_state(),"publishing_enabled":false,"admission_enabled":false})
        },
        Err(_)=>return json!({"schema":"snapmatic-phase4-v1","source_state":"unverified","pending_review":null,
            "platforms":[],"watcher":watcher_state(),"publishing_enabled":false,"admission_enabled":false}),
        Ok(None)=>{}
    }
    let _=app;
    json!({"schema":"snapmatic-phase4-v1","source_state":"not_configured","pending_review":null,
         "cadence_default":default_cadence(),"platforms":[],"watcher":watcher_state(),
         "publishing_enabled":false,"admission_enabled":false})
}
pub(crate) fn rows(app:&App)->store::Result<Vec<Value>>{
    #[cfg(windows)]
    if let Some(s)=sidecar::scope(app)?{
        let items=sidecar::visible(&s,sidecar::rows(&s)?)?;sidecar::validate_lengths(&s,&items)?;
        return Ok(items.into_iter().map(|r|json!({"kind":"intake_original","clip_id":r.clip_id,
          "name":r.source_rel.rsplit('/').next().unwrap_or("Indexed clip"),"state":"pending_review",
          "preview_available":true,"poster_available":true,"preview_only":true,"preview_bytes":r.media_bytes,
          "preview_sha256":r.media_sha256})).collect());
    }
    let _=app;Ok(Vec::new())
}
pub(crate) fn intake_bytes(app:&App)->store::Result<u64>{
    #[cfg(windows)]
    if let Some(s)=sidecar::scope(app)?{return sidecar::validate_lengths(&s,&sidecar::visible(&s,sidecar::rows(&s)?)?);}
    let _=app;Ok(0)
}
pub(crate) fn open(app:&App,id:&str,poster:bool)->store::Result<(std::path::PathBuf,std::fs::File)>{
    #[cfg(windows)]
    if let Some(s)=sidecar::scope(app)?{return sidecar::open(&s,id,poster);}
    let _=(app,id,poster);Err(store::Error("intake_preview_unavailable"))
}

const WATCH:&str="snapmatic-phase4-observer-v1";
use std::sync::{Mutex,atomic::{AtomicBool,Ordering}};
#[derive(Clone,Copy)]
struct WatcherStatus{state:&'static str,last_check:Option<i64>}
static WATCH_STATUS:Mutex<WatcherStatus>=Mutex::new(WatcherStatus{state:"not_started",last_check:None});
static WATCH_OWNER:AtomicBool=AtomicBool::new(false);
fn watch_status(state:&'static str,time:bool){
    if let Ok(mut s)=WATCH_STATUS.lock(){s.state=state;if time{s.last_check=Some(chrono::Utc::now().timestamp());}}
}
fn watcher_state()->Value{
    WATCH_STATUS.lock().map(|s|json!({"state":s.state,"last_check_at_unix":s.last_check}))
        .unwrap_or_else(|_|json!({"state":"unknown","last_check_at_unix":null}))
}
#[cfg(windows)]
pub(crate) struct Watcher{stop:std::sync::Arc<AtomicBool>,thread:Option<std::thread::JoinHandle<()>>}
#[cfg(windows)]
impl Watcher{
    pub(crate) fn stop(mut self){
        self.stop.store(true,Ordering::Release);
        if let Some(h)=self.thread.take(){h.thread().unpark();let _=h.join();}
    }
}
#[cfg(windows)]
impl Drop for Watcher{
    fn drop(&mut self){
        self.stop.store(true,Ordering::Release);if let Some(h)=&self.thread{h.thread().unpark();}
    }
}
#[cfg(windows)]
pub(crate) fn start(app:&App)->Option<Watcher>{
    let Ok(Some(s))=sidecar::scope(app)else{watch_status("held_unconfigured",false);return None;};
    let admission_pin=std::env::var_os("LIFEINVADER_PHASE4_ADMISSION_SHA256");
    let admission=admission_pin.is_some();
    let observer_pin=std::env::var_os("LIFEINVADER_PHASE4_OBSERVER_SHA256");
    if admission&&observer_pin.is_some(){watch_status("held_phase4_conflicting_authorities",false);return None;}
    let Some(pin)=admission_pin.or(observer_pin)else{watch_status("read_only_no_observer_authority",false);return None;};
    let Some(pin)=pin.to_str().filter(|x|media::hex(x)).map(str::to_owned)else{watch_status("held_observer_pin_invalid",false);return None;};
    if WATCH_OWNER.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return None;}
    let stop=std::sync::Arc::new(AtomicBool::new(false));let flag=stop.clone();
    let thread=std::thread::Builder::new().name("snapmatic-phase4-observer".into()).spawn(move||{
        while !flag.load(Ordering::Acquire){
            watch_status(if admission{"checking_pending_only"}else{"observer_starting"},false);
            let r=if admission{admission_once(&s,&pin,&flag)}else{observe_once(&s,&pin,&flag)};
            match r{
                Ok(())=>watch_status(if admission{"pending_only_checked"}else if s.fixture{"fixture_verified"}else{"observed_read_only"},true),
                Err(e)=>{watch_status(e.0,true);break;}
            }
            if s.fixture&&!admission{break;}
            std::thread::park_timeout(std::time::Duration::from_secs(if s.fixture{4}else{45}));
        }
        WATCH_OWNER.store(false,Ordering::Release);
    });
    match thread{Ok(thread)=>Some(Watcher{stop,thread:Some(thread)}),Err(_)=>{WATCH_OWNER.store(false,Ordering::Release);watch_status("held_observer_thread",false);None}}
}

#[cfg(windows)]
fn observe_once(scope:&sidecar::Scope,pin:&str,stop:&AtomicBool)->store::Result<()>{
    use serde::Deserialize;
    use std::{io::{Read,Seek,SeekFrom},path::PathBuf,time::{Duration,Instant}};
    use crate::{controller_native_launcher::clean_path,controller_native_process as native};
    #[derive(Deserialize)] #[serde(deny_unknown_fields)]
    struct Plan{schema:String,workspace:PathBuf,executable:PathBuf,executable_sha256:String,script:PathBuf,script_sha256:String,
        mode:String,fixture_source:Option<PathBuf>,publishing_enabled:bool,admission_enabled:bool}
    let _owner=mutex_for_observer(&scope.root)?;
    let p=scope.root.join("observer-authority.json");clean_path(&p)?;
    let mut d=native::lease(&p,pin)?;
    d.seek(SeekFrom::Start(0)).map_err(|_|store::Error("held_observer_authority_read"))?;
    let mut raw=Vec::new();(&mut d).take(16385).read_to_end(&mut raw).map_err(|_|store::Error("held_observer_authority_read"))?;
    let x:Plan=serde_json::from_slice(&raw).map_err(|_|store::Error("held_observer_authority_schema"))?;
    if raw.len()>16384||x.schema!=WATCH||x.workspace!=scope.root||x.publishing_enabled||x.admission_enabled||
        x.executable.file_name().and_then(|s|s.to_str())!=Some("python.exe")||
        x.script.file_name().and_then(|s|s.to_str())!=Some("sync_intake_phase4.py")||
        x.script.parent().and_then(|s|s.file_name()).and_then(|s|s.to_str())!=Some("tools"){
        return Err(store::Error("held_observer_authority_scope"));
    }
    let fixture_root=scope.root.join("phase4-fixture-source");
    if (scope.fixture&&(x.mode!="observe-fixture"||x.fixture_source.as_ref()!=Some(&fixture_root)))||
        (!scope.fixture&&(x.mode!="observe"||x.fixture_source.is_some())){
        return Err(store::Error("held_observer_authority_scope"));
    }
    for f in [&x.executable,&x.script]{clean_path(f)?;}
    if scope.fixture{clean_path(&fixture_root)?;if !fixture_root.is_dir(){return Err(store::Error("held_fixture_source_unavailable"));}}
    if !media::hex(&x.executable_sha256)||!media::hex(&x.script_sha256){return Err(store::Error("held_observer_pin_invalid"));}
    let mut child=native::phase4_observer(&x.executable,&x.script,[&x.executable_sha256,&x.script_sha256],
       &scope.root,&scope.db,&scope.stage,scope.fixture,d)?;
    let due=Instant::now()+Duration::from_secs(35);
    loop{
        if stop.load(Ordering::Acquire)||Instant::now()>due{let _=child.cancel();return Err(store::Error("held_observer_stopped_or_timeout"));}
        if let Some(ok)=child.poll()?{
            return if ok&&child.active()?==0{Ok(())}else{Err(store::Error("held_observer_exit"))};
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}


#[cfg(windows)]
fn admission_once(scope:&sidecar::Scope,pin:&str,stop:&AtomicBool)->store::Result<()>{
    use serde::Deserialize;
    use std::{io::{Read,Seek,SeekFrom},path::{Path,PathBuf},time::{Duration,Instant}};
    use crate::{controller_native_launcher::clean_path,controller_native_process as native};
    #[derive(Deserialize)] #[serde(deny_unknown_fields)]
    struct Plan{schema:String,mode:String,workspace:PathBuf,fixture_source:Option<PathBuf>,
        executable:PathBuf,executable_sha256:String,script:PathBuf,script_sha256:String,
        baseline:PathBuf,baseline_sha256:String,ffmpeg:PathBuf,ffmpeg_sha256:String,ffprobe:PathBuf,ffprobe_sha256:String,
        publishing_enabled:bool,deletion_enabled:bool,admission_enabled:bool}
    let _owner=mutex_for_observer(&scope.root)?;
    let p=scope.root.join("admission-authority.json");clean_path(&p)?;
    let mut d=native::lease(&p,pin)?;
    d.seek(SeekFrom::Start(0)).map_err(|_|store::Error("held_admission_authority_read"))?;
    let mut bytes=Vec::new();(&mut d).take(16385).read_to_end(&mut bytes).map_err(|_|store::Error("held_admission_authority_read"))?;
    if bytes.len()>16384{return Err(store::Error("held_admission_authority_size"));}
    let x:Plan=serde_json::from_slice(&bytes).map_err(|_|store::Error("held_admission_authority_schema"))?;
    let source=scope.root.join("phase4-fixture-source");
    let filename=|p:&Path,name:&str|p.file_name().and_then(|s|s.to_str())==Some(name);
    let production_baseline=scope.root.parent().ok_or(store::Error("held_admission_scope"))?.join("snapmatic-phase4-intake-baseline-v1.json");
    if x.schema!="snapmatic-phase4-admission-v1"||x.workspace!=scope.root||!x.admission_enabled||x.publishing_enabled||x.deletion_enabled||
       !filename(&x.executable,"python.exe")||!filename(&x.script,"sync_intake_phase4.py")||
       x.script.parent().and_then(|s|s.file_name()).and_then(|s|s.to_str())!=Some("tools")||
       !filename(&x.ffmpeg,"ffmpeg.exe")||!filename(&x.ffprobe,"ffprobe.exe")||x.ffmpeg.parent()!=x.ffprobe.parent()||
       x.script_sha256!="fa2d79486f98ac30418d3ec2bca9fe6963f64e6d9ac0fb8bc2d7fe1750a94014"||
       x.executable_sha256!="4d6f5f81a4bca11191c4c7c6b43632694d0a4ce74e068619d8fdc161d469859a"||
       x.ffmpeg_sha256!="589e50b766d251afdf181dd664d40bd94407e200b019989fd7468c7d118a28d0"||
       x.ffprobe_sha256!="02264dfa4f56baaa31e443ece8aa16d397f6b008200e7fd184d22b9009e75fd3"||
       [x.executable_sha256.as_str(),&x.script_sha256,&x.baseline_sha256,&x.ffmpeg_sha256,&x.ffprobe_sha256].into_iter().any(|h|!media::hex(h)){
       return Err(store::Error("held_admission_authority_scope"));
    }
    if scope.fixture{
        if x.mode!="admit-fixture"||x.fixture_source.as_ref()!=Some(&source)||x.baseline!=scope.root.join("fixture-baseline.json"){
            return Err(store::Error("held_admission_fixture_scope"));
        }
    }else if x.mode!="admit"||x.fixture_source.is_some()||x.baseline!=production_baseline||
      x.baseline_sha256!="28c18841d882cfe0747ef3f571f72037895e884d7cdbf027fc941bfff47f03b9"{
       return Err(store::Error("held_admission_baseline_scope"));
    }
    for f in [&scope.root,&x.executable,&x.script,&x.baseline,&x.ffmpeg,&x.ffprobe]{
        if !matches!(f.components().next(),Some(std::path::Component::Prefix(p))
              if matches!(p.kind(),std::path::Prefix::Disk(b'C'|b'c'))){return Err(store::Error("held_admission_local_disk_required"));}
        clean_path(f)?;
    }
    let ledger=scope.root.join("snapmatic-phase4-supervision.sqlite3");clean_path(&ledger)?;
    let mut supervisor=rusqlite::Connection::open(&ledger)?;
    supervisor.busy_timeout(Duration::from_millis(200))?;
    supervisor.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA synchronous=FULL;
      CREATE TABLE IF NOT EXISTS admission_passes(id INTEGER PRIMARY KEY, started_at INTEGER NOT NULL,
      finished_at INTEGER, state TEXT NOT NULL CHECK(state IN ('running','finished')), authority_sha TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS verified_items(clip_id TEXT PRIMARY KEY, media_sha256 TEXT NOT NULL, poster_sha256 TEXT NOT NULL);")?;
    let unresolved:i64=supervisor.query_row("SELECT COUNT(*) FROM admission_passes WHERE state='running'",[],|r|r.get(0))?;
    if unresolved!=0{return Err(store::Error("held_admission_unconfirmed_restart"));}
    let before=if scope.db.is_file(){sidecar::rows(scope)?}else{Vec::new()};
    validate_generation(scope,&before)?;
    if sidecar::visible(scope,before.clone())?.len()!=before.len(){return Err(store::Error("held_admission_existing_stage_unverified"));}
    if stop.load(Ordering::Acquire){return Err(store::Error("held_admission_stopped"));}
    supervisor.execute("INSERT INTO admission_passes(started_at,state,authority_sha) VALUES(?1,'running',?2)",
        rusqlite::params![chrono::Utc::now().timestamp(),pin])?;
    let reservation=supervisor.last_insert_rowid();
    let mut child=native::phase4_admission(&x.executable,&x.script,[&x.executable_sha256,&x.script_sha256],
        &scope.root,&scope.db,&scope.stage,scope.fixture,d,&x.baseline,&x.baseline_sha256,
        &x.ffmpeg,&x.ffmpeg_sha256,&x.ffprobe,&x.ffprobe_sha256)?;
    let due=Instant::now()+Duration::from_secs(210);
    loop{
      if let Some(ok)=child.poll()?{
        if ok&&child.active()?==0{break;}
        return Err(store::Error("held_admission_child_exit"));
      }
      if stop.load(Ordering::Acquire)||Instant::now()>=due{let _=child.cancel();return Err(store::Error("held_admission_unconfirmed_stop_or_timeout"));}
      std::thread::sleep(Duration::from_millis(100));
    }
    let after=sidecar::rows(scope)?;
    if !monotonic_admission(&before,&after){return Err(store::Error("held_admission_index_state"));}
    let status=sidecar::state(scope)?;
    if !matches!(status["source_state"].as_str(),Some("ready"|"unavailable")){
        return Err(store::Error("held_admission_sidecar_reconciling"));
    }
    validate_generation(scope,&after)?;
    let tx=supervisor.transaction()?;
    if let Some(row)=after.iter().find(|r|!before.contains(r)){
        asset(&scope.stage,&row.clip_id,row,false)?;
        asset(&scope.stage,&row.clip_id,row,true)?;
        tx.execute("INSERT INTO verified_items(clip_id,media_sha256,poster_sha256) VALUES(?1,?2,?3)",
          rusqlite::params![row.clip_id,row.media_sha256,row.poster_sha256])?;
    }
    if tx.execute("UPDATE admission_passes SET finished_at=?1,state='finished' WHERE id=?2 AND state='running'",
      rusqlite::params![chrono::Utc::now().timestamp(),reservation])?!=1{return Err(store::Error("held_admission_reservation_unconfirmed"));}
    tx.commit()?;
    Ok(())
}
#[cfg(windows)]
fn validate_generation(scope:&sidecar::Scope,rows:&[IntakeRow])->store::Result<()>{
    sidecar::validate_lengths(scope,rows)?;
    let expected=rows.iter().flat_map(|r|[r.video.as_str(),r.poster.as_str()]).collect::<std::collections::BTreeSet<_>>();
    let dir=std::fs::read_dir(&scope.stage).map_err(|_|store::Error("held_admission_stage_read"))?;
    let mut actual=0usize;
    for e in dir{
        let e=e.map_err(|_|store::Error("held_admission_stage_read"))?;
        let name=e.file_name();
        if name.to_str().is_none_or(|name|!expected.contains(name)){return Err(store::Error("held_admission_untracked_stage"));}
        actual+=1;
        if actual>expected.len(){return Err(store::Error("held_admission_untracked_stage"));}
    }
    if actual!=expected.len(){return Err(store::Error("held_admission_missing_stage"));}
    Ok(())
}
fn monotonic_admission(before:&[IntakeRow],after:&[IntakeRow])->bool{
    after.len()>=before.len()&&after.len()-before.len()<=1&&before.iter().all(|x|after.contains(x))
}

#[cfg(windows)]
struct ScopeMutex(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl Drop for ScopeMutex{
    fn drop(&mut self){
        unsafe{
            windows_sys::Win32::System::Threading::ReleaseMutex(self.0);
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(windows)]
fn mutex_for_observer(path:&std::path::Path)->store::Result<ScopeMutex>{
    use sha2::{Digest,Sha256};
    use windows_sys::Win32::{Foundation::{CloseHandle,GetLastError},System::Threading::CreateMutexW};
    let canonical=path.to_str().ok_or(store::Error("held_observer_scope"))?.to_lowercase();
    let h=format!("{:x}",Sha256::digest(canonical.as_bytes()));
    let wide=format!(r"Local\SnapmaticPhase4-{h}").encode_utf16().chain(Some(0u16)).collect::<Vec<_>>();
    let m=unsafe{CreateMutexW(std::ptr::null(),1,wide.as_ptr())};
    if m.is_null(){return Err(store::Error("held_observer_mutex"));}
    if unsafe{GetLastError()}==183{unsafe{CloseHandle(m);}return Err(store::Error("held_observer_already_owned"));}
    Ok(ScopeMutex(m))
}
#[cfg(windows)]
pub(crate) fn fixture_enabled(app:&App)->bool{
    sidecar::scope(app).ok().flatten().is_some_and(|s|s.fixture)
}
