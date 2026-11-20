//! APP adapter for OPS wave-4 status(path, now). Never call reserve/outcome/reset.
//! A concrete host bridge is deliberately not installed by this isolated patch.
use serde::{Deserialize,Serialize};
use std::collections::BTreeMap;
#[derive(Serialize,Debug)]
pub struct View {pub ok:bool,pub reason:&'static str,pub actions_enabled:bool,pub cleanup_eligible:bool,
    #[serde(skip_serializing_if="Option::is_none")] pub status:Option<Status>}
#[derive(Serialize,Deserialize,Debug)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub ok:bool,pub revision:i64,pub paused:bool,pub consecutive_errors:u64,
    pub platform_used:BTreeMap<String,u64>,pub platform_remaining:BTreeMap<String,u64>,
    pub metricool_used:u64,pub metricool_remaining:u64,pub unresolved:u64,pub restore_proofs:u64,
    pub cleanup_eligible:bool,pub actions_enabled:bool,
}
impl View {pub fn unavailable()->Self{Self{ok:false,reason:"ops_status_unavailable",actions_enabled:false,cleanup_eligible:false,status:None}}}
pub fn normalize(raw:serde_json::Value)->View {
    // Failure records deliberately have no counters; never synthesize zeroes.
    let Ok(status)=serde_json::from_value::<Status>(raw) else{return View::unavailable()};
    if !status.ok || status.actions_enabled || status.cleanup_eligible || status.revision<0 || status.metricool_remaining>20{return View::unavailable();}
    let keys=["instagram","tiktok","youtube"];
    if status.platform_used.keys().map(String::as_str).collect::<Vec<_>>()!=keys
        || status.platform_remaining.keys().map(String::as_str).collect::<Vec<_>>()!=keys
        || status.platform_remaining.iter().any(|(p,n)|*n>15u64.saturating_sub(status.platform_used[p])) {return View::unavailable();}
    let reason=if status.paused {"ops_paused"} else {"ops_status_read_only"};
    View{ok:true,reason,actions_enabled:false,cleanup_eligible:false,status:Some(status)}
}
pub trait StatusReader:Send+Sync {fn read(&self,now_unix:i64)->View;}
pub struct Unavailable;
impl StatusReader for Unavailable {fn read(&self,_:i64)->View{View::unavailable()}}
// Supply a bounded trusted bridge to OPS status only; raw observations are checked
// before reaching the UI. This adapter grants no reservation/write capability.
pub struct JsonStatusReader<F:Fn(i64)->serde_json::Value+Send+Sync>(pub F);
impl<F:Fn(i64)->serde_json::Value+Send+Sync> StatusReader for JsonStatusReader<F>{fn read(&self,t:i64)->View{normalize((self.0)(t))}}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn controller_ops_unknown_is_not_zero(){let v=normalize(serde_json::json!({"ok":false,"reason":"accounting_unknown","actions_enabled":false}));assert!(!v.ok);assert!(v.status.is_none());assert!(!serde_json::to_string(&v).unwrap().contains("platform_used"));}
    #[test] fn controller_ops_preserves_pause_and_rejects_write_authority(){
        let mut raw=serde_json::json!({"ok":true,"revision":4,"paused":true,"consecutive_errors":3,"platform_used":{"instagram":0,"tiktok":15,"youtube":2},"platform_remaining":{"instagram":15,"tiktok":0,"youtube":13},"metricool_used":17,"metricool_remaining":3,"unresolved":1,"restore_proofs":0,"cleanup_eligible":false,"actions_enabled":false});
        let v=normalize(raw.clone());assert!(v.ok);assert!(v.status.unwrap().paused);assert!(!v.actions_enabled);
        raw["actions_enabled"]=true.into();assert!(!normalize(raw).ok);
    }
}
