use crate::{App, controller_store as store};
use axum::{body::Bytes, extract::{DefaultBodyLimit,Path,State}, http::{header,HeaderMap,StatusCode}, response::{Html,IntoResponse,Response}, routing::{get,post}, Json,Router};
use serde::Deserialize;
use serde_json::json;
fn now()->i64 {chrono::Utc::now().timestamp()}
fn fail(e:store::Error)->Response {
    let status=match e.0 {"not_found"=>StatusCode::NOT_FOUND,"invalid_selection"|"invalid_action"=>StatusCode::BAD_REQUEST,"storage_unavailable"=>StatusCode::SERVICE_UNAVAILABLE,_=>StatusCode::CONFLICT};
    (status,[(header::CACHE_CONTROL,"no-store")],Json(json!({"schema":"controller-v1","state":"HELD","reason":e.0,"publish":false}))).into_response()
}
fn single<'a>(h:&'a HeaderMap,k:&str)->Option<&'a str>{let mut v=h.get_all(k).iter();let s=v.next()?.to_str().ok()?;v.next().is_none().then_some(s)}
fn same_origin(h:&HeaderMap)->bool {
    let Some(host)=single(h,"host") else{return false};
    host.strip_prefix("127.0.0.1:").is_some_and(|p|p.parse::<u16>().is_ok_and(|p|p>0))
        && single(h,"origin").is_some_and(|o|o.strip_prefix("http://")==Some(host))
}
fn denied()->Response { (StatusCode::FORBIDDEN,Json(json!({"reason":"same_origin_required"}))).into_response() }
pub fn routes()->Router<App>{
    Router::new().route("/controller",get(page)).route("/controller.js",get(script))
        .route("/api/controller",get(snapshot))
        .route("/api/controller/settings",post(configure))
        .route("/api/controller/jobs",post(enqueue))
        .route("/api/controller/jobs/{id}",get(job))
        .route("/api/controller/jobs/{id}/review",post(review))
        .route("/api/controller/jobs/{id}/preview",get(preview))
        .route("/api/controller/jobs/{id}/worker-evidence",get(worker_evidence))
        .route("/api/controller/quota",get(quota))
        .layer(DefaultBodyLimit::max(16*1024))
}
async fn page()->Html<&'static str>{Html(include_str!("../web/controller.html"))}
async fn script()->impl IntoResponse{ ([(header::CONTENT_TYPE,"text/javascript; charset=utf-8"),(header::CACHE_CONTROL,"no-store")],include_str!("../web/controller.js")) }
async fn ops_status(app:&App,t:i64)->crate::controller_ops::View{
    let reader=app.controller_ops.clone();
    tokio::time::timeout(std::time::Duration::from_secs(1),tokio::task::spawn_blocking(move||reader.read(t))).await
        .ok().and_then(Result::ok).unwrap_or_else(crate::controller_ops::View::unavailable)
}
async fn snapshot(State(app):State<App>)->Response{
    let t=now();let ops=ops_status(&app,t).await;
    let result=(||{let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
        let settings=store::settings(&db)?;let jobs=store::jobs(&db)?;
        let count=db.query_row("SELECT COUNT(*) FROM controller_jobs",[],|r|r.get::<_,i64>(0))?;
        Ok::<_,store::Error>(json!({"schema":"controller-v1","settings":settings,"jobs":jobs,"total_jobs":count,"listed_limit":500,"quota":{"local_receipts":store::quota(&db,t)?,"ops":ops},"worker_connected":false,"publishing_connected":false,"review_required":true}))})();
    match result{Ok(v)=>([(header::CACHE_CONTROL,"no-store")],Json(v)).into_response(),Err(e)=>fail(e)}
}
async fn job(State(app):State<App>,Path(id):Path<String>)->Response{
    let result=app.db.lock().map_err(|_|store::Error("storage_unavailable")).and_then(|db|store::job(&db,&id));
    match result{Ok(v)=>([(header::CACHE_CONTROL,"no-store")],Json(v)).into_response(),Err(e)=>fail(e)}
}
async fn quota(State(app):State<App>)->Response{
    let t=now();let ops=ops_status(&app,t).await;
    let result=app.db.lock().map_err(|_|store::Error("storage_unavailable")).and_then(|db|store::quota(&db,t));
    match result{Ok(v)=>([(header::CACHE_CONTROL,"no-store")],Json(json!({"schema":"controller-quota-adapter-v1","local_receipts":v,"ops":ops,"actions_enabled":false}))).into_response(),Err(e)=>fail(e)}
}
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
struct Action {revision:i64,action:String}
async fn configure(State(app):State<App>,headers:HeaderMap,Json(a):Json<Action>)->Response{
    if !same_origin(&headers){return denied();}
    let result=app.db.lock().map_err(|_|store::Error("storage_unavailable")).and_then(|mut db|store::configure(&mut db,a.revision,&a.action,now()));
    match result{Ok(v)=>Json(v).into_response(),Err(e)=>fail(e)}
}
async fn review(State(app):State<App>,Path(id):Path<String>,headers:HeaderMap,Json(a):Json<Action>)->Response{
    if !same_origin(&headers){return denied();}
    let result=app.db.lock().map_err(|_|store::Error("storage_unavailable")).and_then(|mut db|{
        let j=store::job(&db,&id)?;
        if !crate::controller_source::controller_current(&db,j.source_id,&j.source_binding){return Err(store::Error("source_unavailable"));}
        store::review(&mut db,&id,a.revision,&a.action,now())
    });
    match result{Ok(v)=>Json(v).into_response(),Err(e)=>fail(e)}
}
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
struct NewJob {source_id:i64,path:String,platform:String}
async fn enqueue(State(app):State<App>,headers:HeaderMap,Json(n):Json<NewJob>)->Response{
    if !same_origin(&headers){return denied();}
    let result=tokio::task::spawn_blocking(move||{
        let admission=crate::controller_source::controller_selection(&app,n.source_id,&n.path).map_err(store::Error)?;
        let mut db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
        if !crate::controller_source::matches_admission(&db,&admission){return Err(store::Error("source_unavailable"));}
        store::enqueue(&mut db,n.source_id,&n.path,&n.platform,&crate::controller_source::controller_binding(&admission),now())
    }).await.unwrap_or(Err(store::Error("storage_unavailable")));
    match result{Ok(v)=>(StatusCode::CREATED,Json(v)).into_response(),Err(e)=>fail(e)}
}
// This GET handler checks current source identity and the SELECT/hash path, never clips::preview,
// submit, worker RPC or event persistence. Source-root validation is read-only.
async fn preview(State(app):State<App>,Path(id):Path<String>)->Response{
    let result=app.db.lock().map_err(|_|store::Error("storage_unavailable")).and_then(|db|{
        let j=store::job(&db,&id)?;
        if !crate::controller_source::controller_current(&db,j.source_id,&j.source_binding){return Err(store::Error("source_unavailable"));}
        store::preview(&db,&id)
    });
    match result {Ok(v)=>([(header::CONTENT_TYPE,"video/mp4"),(header::CACHE_CONTROL,"no-store"),(header::X_CONTENT_TYPE_OPTIONS,"nosniff")],Bytes::from(v)).into_response(),Err(e)=>fail(e)}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc,Mutex};
    use rusqlite::Connection;
    use tower::ServiceExt;
    use axum::{body::{Body,to_bytes},http::Request};
    use sha2::{Digest,Sha256};
    fn app()->App{
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("controller-test-data").join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(root.join("sample-source")).unwrap();std::fs::write(root.join("sample-source/original.mp4"),b"inert source").unwrap();
        let db=Connection::open_in_memory().unwrap();crate::init(&db).unwrap();store::init(&db).unwrap();
        db.execute("INSERT INTO profiles(id,name,platform,url,role,source_folder) VALUES(1,'sample-source','tiktok','https://www.tiktok.com/@sample-source','source','sample-source')",[]).unwrap();
        db.execute("INSERT INTO settings(key,value) VALUES('source_root',?)",[root.to_str().unwrap()]).unwrap();
        App{db:Arc::new(Mutex::new(db)),media:root.clone(),log_dir:root,controller_ops:Arc::new(crate::controller_ops::Unavailable),controller_worker:Arc::new(crate::controller_worker::Unavailable),
}
    }
    fn ready(app:&App)->store::Job{
        let admission=crate::controller_source::controller_selection(app,1,"sample-source/original.mp4").unwrap();let mut db=app.db.lock().unwrap();
        let j=store::enqueue(&mut db,1,"sample-source/original.mp4","tiktok",&crate::controller_source::controller_binding(&admission),2_000_000).unwrap();
        store::configure(&mut db,0,"resume",2_000_000).unwrap();let j=store::claim(&mut db,&j.id,0,2_000_000).unwrap();
        store::ready(&mut db,&j.id,j.revision,b"inert preview",&format!("{:x}",Sha256::digest(b"inert preview")),2_000_000).unwrap()
    }
    fn request(method:&str,uri:&str,body:&str,origin:bool)->Request<Body>{
        let mut b=Request::builder().method(method).uri(uri).header("host","127.0.0.1:43847").header("content-type","application/json");
        if origin{b=b.header("origin","http://127.0.0.1:43847");}b.body(Body::from(body.to_owned())).unwrap()
    }
    #[tokio::test] async fn controller_preview_http_reopen_is_readonly_and_never_enqueues(){
        let app=app();let j=ready(&app);let api=routes().with_state(app.clone());let url=format!("/api/controller/jobs/{}/preview",j.id);
        let before={let db=app.db.lock().unwrap();db.execute_batch("PRAGMA query_only=ON").unwrap();db.query_row("SELECT total_changes()",[],|r|r.get::<_,i64>(0)).unwrap()};
        for _ in 0..3{let response=api.clone().oneshot(request("GET",&url,"",false)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(to_bytes(response.into_body(),1024).await.unwrap(),b"inert preview"[..]);}
        assert_eq!(api.clone().oneshot(request("POST",&url,"{}",true)).await.unwrap().status(),StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(api.clone().oneshot(request("GET","/api/controller/quota","",false)).await.unwrap().status(),StatusCode::OK);
        let db=app.db.lock().unwrap();assert_eq!(before,db.query_row("SELECT total_changes()",[],|r|r.get::<_,i64>(0)).unwrap());assert_eq!(db.query_row("SELECT COUNT(*) FROM controller_jobs",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    }
    #[tokio::test] async fn controller_archive_and_source_change_revoke_preview_and_review(){
        let app=app();let j=ready(&app);let api=routes().with_state(app.clone());
        app.db.lock().unwrap().execute("UPDATE profiles SET archived=1 WHERE id=1",[]).unwrap();
        let response=api.clone().oneshot(request("GET",&format!("/api/controller/jobs/{}/preview",j.id),"",false)).await.unwrap();assert_eq!(response.status(),StatusCode::CONFLICT);
        app.db.lock().unwrap().execute("UPDATE profiles SET archived=0,url='https://www.tiktok.com/@changed' WHERE id=1",[]).unwrap();
        let response=api.oneshot(request("POST",&format!("/api/controller/jobs/{}/review",j.id),&format!(r#"{{"revision":{},"action":"approve"}}"#,j.revision),true)).await.unwrap();assert_eq!(response.status(),StatusCode::CONFLICT);
        assert_eq!(store::job(&app.db.lock().unwrap(),&j.id).unwrap().review,"PENDING");
    }
    #[tokio::test] async fn controller_actions_require_same_origin_and_reject_worker_claims(){
        let app=app();let api=routes().with_state(app.clone());
        assert_eq!(api.clone().oneshot(request("POST","/api/controller/settings",r#"{"revision":0,"action":"resume"}"#,false)).await.unwrap().status(),StatusCode::FORBIDDEN);
        assert_eq!(api.clone().oneshot(request("POST","/api/controller/settings",r#"{"revision":0,"action":"resume","publish":true}"#,true)).await.unwrap().status(),StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(api.oneshot(request("POST","/api/controller/jobs/x/posted",r#"{"receipt":"forged"}"#,true)).await.unwrap().status(),StatusCode::NOT_FOUND);
        assert!(store::settings(&app.db.lock().unwrap()).unwrap().paused);
    }
    #[tokio::test] async fn controller_intake_keeps_original_archived_and_solo_restrictions(){
        let app=app();let root=app.media.clone();std::fs::create_dir_all(root.join("sample-source/.lifeinvader-edits")).unwrap();std::fs::write(root.join("sample-source/.lifeinvader-edits/edit.mp4"),b"edit").unwrap();
        std::fs::create_dir_all(root.join("foreign")).unwrap();std::fs::write(root.join("foreign/other.mp4"),b"foreign").unwrap();
        std::fs::create_dir_all(root.join("Solo Posts/Downloads/channel")).unwrap();std::fs::write(root.join("Solo Posts/Downloads/channel/post.mp4"),b"unverified solo").unwrap();
        app.db.lock().unwrap().execute("INSERT INTO profiles(id,name,platform,url,role,source_folder) VALUES(2,'Solo Posts','other','https://lifeinvader.local/solo-posts','source','')",[]).unwrap();
        let api=routes().with_state(app.clone());
        for (id,path) in [(1,"sample-source/.lifeinvader-edits/edit.mp4"),(1,"foreign/other.mp4"),(2,"Solo Posts/Downloads/channel/post.mp4"),(1,"../outside.mp4")] {
            let body=json!({"source_id":id,"path":path,"platform":"tiktok"}).to_string();
            assert!(!api.clone().oneshot(request("POST","/api/controller/jobs",&body,true)).await.unwrap().status().is_success());
        }
        app.db.lock().unwrap().execute("UPDATE profiles SET archived=1 WHERE id=1",[]).unwrap();
        let body=json!({"source_id":1,"path":"sample-source/original.mp4","platform":"tiktok"}).to_string();
        assert_eq!(api.oneshot(request("POST","/api/controller/jobs",&body,true)).await.unwrap().status(),StatusCode::CONFLICT);
        assert_eq!(app.db.lock().unwrap().query_row("SELECT COUNT(*) FROM controller_jobs",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    }

    #[tokio::test] async fn worker_bridge_route_is_readonly_and_source_guarded(){
        let app=app();let j=ready(&app);let api=routes().with_state(app.clone());
        app.db.lock().unwrap().execute_batch("PRAGMA query_only=ON").unwrap();
        let before=app.db.lock().unwrap().total_changes();
        let path=format!("/api/controller/jobs/{}/worker-evidence",j.id);
        let response=api.clone().oneshot(request("GET",&path,"",false)).await.unwrap();
        assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["cache-control"],"no-store");
        let body=to_bytes(response.into_body(),4096).await.unwrap();assert!(String::from_utf8_lossy(&body).contains("UNKNOWN"));
        assert_eq!(api.clone().oneshot(request("POST",&path,"{}",true)).await.unwrap().status(),StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(before,app.db.lock().unwrap().total_changes());assert_eq!(store::job(&app.db.lock().unwrap(),&j.id).unwrap(),j);
        app.db.lock().unwrap().execute_batch("PRAGMA query_only=OFF; UPDATE profiles SET archived=1 WHERE id=1").unwrap();
        assert_eq!(api.oneshot(request("GET",&path,"",false)).await.unwrap().status(),StatusCode::CONFLICT);
    }

}

// Observational projection only: no controller transition or worker/preview submit.
async fn worker_evidence(State(app):State<App>,Path(id):Path<String>)->Response {
    let result=tokio::task::spawn_blocking(move||{
        let read_job=||{let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
            let j=store::job(&db,&id)?;
            if !crate::controller_source::controller_current(&db,j.source_id,&j.source_binding){return Err(store::Error("source_unavailable"));}Ok(j)};
        let before=read_job()?;
        let view=app.controller_worker.read(&before,now());
        if read_job()?!=before {return Err(store::Error("selection_changed"));}
        Ok::<_,store::Error>(view)
    }).await.unwrap_or(Err(store::Error("storage_unavailable")));
    match result{Ok(v)=>([(header::CACHE_CONTROL,"no-store")],Json(v)).into_response(),Err(e)=>fail(e)}
}
