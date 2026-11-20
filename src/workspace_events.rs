use crate::{App,controller_store as store};
use axum::{Extension,http::HeaderMap,response::sse::{Event,KeepAlive,Sse}};
use futures_util::stream;
use serde_json::{Value,json};
use std::{collections::VecDeque,convert::Infallible,sync::{Arc,Mutex},time::Duration};
use tokio::sync::broadcast;
const MAX_HISTORY:usize=128;
#[derive(Clone)]
struct Notice{ seq:u64,event:&'static str,data:Value }
#[derive(Default)]
struct Cache{ seq:u64,snapshot:Vec<Value>,history:VecDeque<Notice> }
#[derive(Clone)]
pub(crate) struct Feed {
    tag:String,cache:Arc<Mutex<Cache>>,tx:broadcast::Sender<Notice>
}
fn snapshot(app:&App)->store::Result<Vec<Value>>{
    let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
    let jobs=store::jobs(&db)?;
    let mut entries=jobs.into_iter().take(127).map(|j|{
        let stage=match j.state.as_str(){
            "QUEUED"=>"queued","PROCESSING"=>"processing_evidence_pending",
            "READY_FOR_REVIEW" if j.preview_sha256.is_some()=>"ready",
            "HELD"|"FAILED"=>"hold","POSTED"|"BACKED_UP"=>"posted",_=>"unverified",
        };
        json!({"job_id":j.id,"revision":j.revision,"stage":stage,"controller_state":j.state})
    }).collect::<Vec<_>>();
    entries.extend(crate::workspace_nvenc::events(&db)?);
    drop(db);
    if let Ok(rows)=crate::workspace_phase4::rows(app){
        entries.extend(rows.into_iter().take(64).map(|r|json!({"job_id":r["clip_id"],"clip_id":r["clip_id"],
            "stage":"intake_pending_review","controller_state":"PENDING_REVIEW"})));
    }
    if let Ok(rows)=crate::workspace_source_download::rows(app){
        entries.extend(rows.into_iter().take(64).map(|r|json!({"job_id":r["clip_id"],"clip_id":r["clip_id"],
            "stage":"intake_pending_review","controller_state":"PENDING_REVIEW"})));
    }
    if let Ok(rows)=crate::workspace_local_import::rows(app){
        entries.extend(rows.into_iter().take(64).map(|r|json!({"job_id":r["clip_id"],"clip_id":r["clip_id"],
            "stage":"intake_pending_review","controller_state":"PENDING_REVIEW"})));
    }
    Ok(entries)
}
impl Feed{
    pub(crate) fn start(app:App)->Self{
        let (tx,_)=broadcast::channel(MAX_HISTORY);
        let f=Self{tag:uuid::Uuid::new_v4().to_string(),cache:Arc::new(Mutex::new(Cache::default())),tx};
        let task=f.clone();
        tokio::spawn(async move{
            let mut tick=tokio::time::interval(Duration::from_millis(700));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                let a=app.clone();
                let Ok(Ok(current))=tokio::task::spawn_blocking(move||snapshot(&a)).await else{continue;};
                let Ok(mut c)=task.cache.lock() else{break;};
                if c.snapshot==current{continue;}
                for value in &current{
                    if c.snapshot.iter().any(|old|old==value){continue;}
                    c.seq=c.seq.saturating_add(1);
                    let event=if value["stage"]=="intake_pending_review"{"clip_ingested"}else{"controller_state"};
                    let n=Notice{seq:c.seq,event,data:value.clone()};
                    if c.history.len()==MAX_HISTORY{c.history.pop_front();}
                    c.history.push_back(n.clone());let _=task.tx.send(n);
                }
                c.snapshot=current;
            }
        });
        f
    }
    fn snapshot_event(&self)->Notice{
        let c=self.cache.lock().ok();
        Notice{seq:c.as_ref().map_or(0,|x|x.seq),event:"snapshot",
            data:json!({"schema":"snapmatic-event-v2","jobs":c.as_ref().map(|x|x.snapshot.as_slice()).unwrap_or(&[]),
                        "posting_enabled":false})}
    }
    fn replay(&self,last:Option<&str>)->VecDeque<Notice>{
        let Some(seq)=last.and_then(|v|v.split_once(':')).and_then(|(prefix,n)|(prefix==self.tag).then_some(n))
            .and_then(|n|n.parse::<u64>().ok()) else{return VecDeque::from([self.snapshot_event()]);};
        let Ok(cache)=self.cache.lock() else{return VecDeque::new();};
        let oldest=cache.history.front().map_or(cache.seq,|v|v.seq);
        if seq<oldest.saturating_sub(1)||seq>cache.seq{
            drop(cache);return VecDeque::from([self.snapshot_event()]);
        }
        cache.history.iter().filter(|v|v.seq>seq).cloned().collect()
    }
}
pub(crate) async fn sse(Extension(feed):Extension<Feed>,h:HeaderMap)->Sse<impl futures_util::Stream<Item=Result<Event,Infallible>>>{
    let last=h.get("last-event-id").and_then(|x|x.to_str().ok());
    let rx=feed.tx.subscribe();let history=feed.replay(last);
    let events=stream::unfold((feed,rx,history,None),|(feed,mut rx,mut history,mut last)|async move{
        loop {
            let n=if let Some(n)=history.pop_front(){n}else{
                match rx.recv().await{
                    Ok(n)=>n,
                    Err(broadcast::error::RecvError::Lagged(_))=>feed.snapshot_event(),
                    Err(broadcast::error::RecvError::Closed)=>return None,
                }
            };
            if last.is_some_and(|seq|n.seq<=seq) && n.event!="snapshot"{continue;}
            last=Some(n.seq);
            let ev=Event::default().id(format!("{}:{}",feed.tag,n.seq)).event(n.event).data(n.data.to_string());
            return Some((Ok(ev),(feed,rx,history,last)));
        }
    });
    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(12)).text("snapmatic-heartbeat"))
}
