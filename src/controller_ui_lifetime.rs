//! Isolated review UI lifetime; browser bootstrap exit is diagnostic, not ownership.
use std::{sync::Mutex,time::Instant};

#[derive(Default)]
struct State { last:Option<u64>, closed:bool }
pub struct Lifetime { enabled:bool, start:Instant, state:Mutex<State> }
impl Lifetime {
    pub fn new(enabled:bool)->Self{Self{enabled,start:Instant::now(),state:Mutex::new(State::default())}}
    pub fn enabled(&self)->bool{self.enabled}
    fn seconds(&self)->u64{self.start.elapsed().as_secs()}
    pub fn signal(&self,close:bool)->bool{self.signal_at(close,self.seconds())}
    fn signal_at(&self,close:bool,now:u64)->bool{
        if !self.enabled{return false}
        let Ok(mut s)=self.state.lock() else{return false};
        if reason(&s,now).is_some(){return false}
        if close{s.closed=true}else{s.last=Some(now)};true
    }
    pub fn ready(&self)->bool{
        !self.enabled || self.state.lock().is_ok_and(|s|s.last.is_some()&&reason(&s,self.seconds()).is_none())
    }
    fn stop_at(&self,now:u64)->Option<&'static str>{
        match self.state.lock(){Ok(s)=>reason(&s,now),Err(_)=>Some("UI_STATE_UNAVAILABLE")}
    }
}
fn reason(s:&State,now:u64)->Option<&'static str>{
    if s.closed{Some("UI_CLOSE_REQUESTED")}
    else if let Some(last)=s.last{(now.saturating_sub(last)>=90).then_some("UI_HEARTBEAT_EXPIRED")}
    else{(now>=60).then_some("UI_NOT_READY_60S")}
}
trait Bootstrap {fn poll(&mut self)->std::io::Result<Option<std::process::ExitStatus>>;}
impl Bootstrap for std::process::Child {fn poll(&mut self)->std::io::Result<Option<std::process::ExitStatus>>{self.try_wait()}}
fn observe(child:&mut Option<impl Bootstrap>){
    let result=child.as_mut().map(Bootstrap::poll);
    match result {
        Some(Ok(Some(status)))=>{eprintln!("UI_BOOTSTRAP_EXIT code={:?}; awaiting UI lifetime",status.code());*child=None;},
        Some(Err(_))=>{eprintln!("UI_BOOTSTRAP_WAIT_ERROR; awaiting UI lifetime");*child=None;},
        _=>{}
    }
}
pub async fn wait(ui:&Lifetime,mut child:Option<std::process::Child>)->&'static str{
    loop {
        observe(&mut child);
        if let Some(reason)=ui.stop_at(ui.seconds()){
            eprintln!("{reason}; stopping isolated server and cancelling owned worker");return reason
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[cfg(windows)] use std::os::windows::process::ExitStatusExt;
    #[cfg(windows)] struct Mock{polls:usize}
    #[cfg(windows)] impl Bootstrap for Mock{fn poll(&mut self)->std::io::Result<Option<std::process::ExitStatus>>{self.polls+=1;Ok(Some(std::process::ExitStatus::from_raw(0)))}}
    #[cfg(windows)] #[test] fn bootstrap_exit_does_not_close_live_ui(){
        let ui=Lifetime::new(true);let mut child=Some(Mock{polls:0});observe(&mut child);
        assert!(child.is_none());assert_eq!(ui.stop_at(1),None);
        assert!(ui.signal_at(false,2));assert_eq!(ui.stop_at(61),None);
        assert!(ui.signal_at(true,62));assert_eq!(ui.stop_at(62),Some("UI_CLOSE_REQUESTED"));
    }
    #[test] fn readiness_and_disconnect_are_bounded_and_sticky(){
        let ui=Lifetime::new(true);assert!(!ui.ready());assert_eq!(ui.stop_at(60),Some("UI_NOT_READY_60S"));assert!(!ui.signal_at(false,60));
        let ui=Lifetime::new(true);assert!(ui.signal_at(false,1));assert_eq!(ui.stop_at(91),Some("UI_HEARTBEAT_EXPIRED"));assert!(!ui.signal_at(false,91));
        let ui=Lifetime::new(false);assert!(!ui.signal(false));assert!(ui.ready());
    }
}
