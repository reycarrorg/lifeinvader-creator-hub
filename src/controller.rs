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
        .route("/api/controller/editor-render-reviewed",post(editor_render_reviewed))
        .route("/api/controller/editor-candidate",post(editor_candidate))
        .route("/api/controller/authorized-sources",get(authorized_sources))
        .route("/api/controller/ui-lifetime",post(ui_lifetime))
        .route("/api/controller/settings",post(configure))
        .route("/api/controller/jobs",post(enqueue))
        .route("/api/controller/jobs/{id}",get(job))
        .route("/api/controller/jobs/{id}/review",post(review))
        .route("/api/controller/jobs/{id}/prepare",post(prepare_clip))
        .route("/api/controller/jobs/{id}/render-reviewed",post(render_reviewed))
        .route("/api/controller/jobs/{id}/import-completed",post(import_clip))
        .route("/api/controller/jobs/{id}/preview",get(preview))
        .route("/api/controller/jobs/{id}/worker-evidence",get(worker_evidence))
        .route("/api/controller/jobs/{id}/status-details",get(status_details))
        .route("/api/controller/provider-auth",get(provider_auth))
        .route("/api/controller/youtube/pre-upload",post(youtube_pre_upload))
        .route("/api/controller/instagram/offline-preflight",post(instagram_offline_preflight))
        .route("/api/controller/quota",get(quota))
        .layer(DefaultBodyLimit::max(16*1024))
}
/// Package qualification cannot dispatch, import, enqueue, publish or delete.
pub fn package_routes()->Router<App>{
    Router::new().route("/controller",get(page)).route("/controller.js",get(script))
        .route("/api/controller",get(package_snapshot))
        .route("/api/controller/authorized-sources",get(authorized_sources))
        .route("/api/controller/provider-auth",get(provider_auth))
        .route("/api/controller/jobs/{id}/status-details",get(status_details))
        .route("/api/controller/ui-lifetime",post(ui_lifetime))
        .route("/api/controller/jobs/{id}",get(job))
        .route("/api/controller/jobs/{id}/preview",get(preview))
        .route("/api/controller/jobs/{id}/worker-evidence",get(worker_evidence))
        .layer(DefaultBodyLimit::max(16*1024))
}
async fn package_snapshot(State(app):State<App>)->Response{
    let result=(||{let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
        Ok::<_,store::Error>(json!({"schema":"controller-v1","settings":store::settings(&db)?,"jobs":store::jobs(&db)?,"total_jobs":db.query_row("SELECT COUNT(*) FROM controller_jobs",[],|r|r.get::<_,i64>(0))?,"listed_limit":500,"quota":{"local_receipts":store::quota(&db,now())?,"ops":{"ok":false,"reason":"not_configured","actions_enabled":false,"cleanup_eligible":false}},"worker_connected":false,"manual_native_enabled":false,"isolated_ui_lifetime":app.controller_ui.enabled(),"publishing_connected":false,"review_required":true,"package_read_only":true}))})();
    match result{Ok(v)=>([(header::CACHE_CONTROL,"no-store")],Json(v)).into_response(),Err(e)=>fail(e)}
}
async fn page()->Html<&'static str>{Html(include_str!("../web/controller.html"))}
fn youtube_default()->serde_json::Value {
    json!({"provider":"youtube","health":"unconfigured","automatic_refresh_configured":false,"refreshed":false,"publishing_enabled":false})
}
fn tiktok_inbox_status()->serde_json::Value {
    #[cfg(feature="snapmatic-hud")]
    return crate::controller_tiktok_inbox::status();
    #[cfg(not(feature="snapmatic-hud"))]
    json!({"provider":"tiktok","health":"unconfigured"})
}
async fn youtube_health(prepare:bool)->serde_json::Value {
    #[cfg(windows)]
    {
        static REFRESH:tokio::sync::Mutex<()>=tokio::sync::Mutex::const_new(());
        let _guard=if prepare{Some(REFRESH.lock().await)}else{None};
        return tokio::task::spawn_blocking(move||crate::controller_youtube::health(prepare)).await.unwrap_or_else(|_|youtube_default());
    }
    #[cfg(not(windows))]
    { let _=prepare;youtube_default() }
}
#[cfg(windows)]
pub(crate) async fn youtube_guardian(){
    let mut cadence=tokio::time::interval(std::time::Duration::from_secs(21_600));
    cadence.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop{cadence.tick().await;let _=youtube_health(true).await;}
}
fn instagram_default()->serde_json::Value{
    json!({"provider":"instagram","health":"held","offline_validator":"not_run","transport_enabled":false,"publishing_enabled":false})
}
async fn instagram_health(run:bool)->serde_json::Value{
    #[cfg(windows)]
    return tokio::task::spawn_blocking(move||crate::controller_instagram::health(run)).await.unwrap_or_else(|_|instagram_default());
    #[cfg(not(windows))]
    {let _=run;instagram_default()}
}
async fn instagram_offline_preflight(headers:HeaderMap)->Response{
    if !same_origin(&headers){return denied();}
    let status=instagram_health(true).await;
    let code=if status["offline_validator"]=="passed"{StatusCode::OK}else{StatusCode::CONFLICT};
    (code,[(header::CACHE_CONTROL,"no-store")],Json(status)).into_response()
}
async fn provider_auth()->Response{
    let (youtube,instagram)=tokio::join!(youtube_health(false),instagram_health(false));
    ([(header::CACHE_CONTROL,"no-store")],Json(json!({"schema":"controller-provider-auth-v1","active_provider":null,"connect_enabled":false,"providers":[
        youtube,instagram,tiktok_inbox_status()
    ],"publishing_enabled":false}))).into_response()
}
async fn youtube_pre_upload(headers:HeaderMap)->Response {
    if !same_origin(&headers){return denied();}
    // This only evaluates the scoped refresh. No upload, publish or token is returned.
    let status=youtube_health(true).await;
    let code=if status["health"]=="ready"{StatusCode::OK}else{StatusCode::CONFLICT};
    (code,[(header::CACHE_CONTROL,"no-store")],Json(status)).into_response()
}
async fn status_details(State(app):State<App>,Path(id):Path<String>)->Response{
    let result=tokio::task::spawn_blocking(move||{
        let read=||{let db=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
            let j=store::job(&db,&id)?;
            if !crate::controller_source::controller_current(&db,j.source_id,&j.source_binding){return Err(store::Error("source_unavailable"))}Ok(j)};
        let j=read()?;let worker=app.controller_worker.read(&j,now());
        if read()?!=j{return Err(store::Error("selection_changed"))}
        Ok::<_,store::Error>(json!({"schema":"controller-status-details-v1","job_id":j.id,"revision":j.revision,
            "durable_state":j.state,"hold_reason":j.hold_reason,"worker":worker,
            "recorded_preview_sha256":j.preview_sha256,"preview_requires_integrity_check":true,
            "publishing_enabled":false,"deletion_enabled":false}))
    }).await.unwrap_or(Err(store::Error("storage_unavailable")));
    match result{Ok(v)=>([(header::CACHE_CONTROL,"no-store")],Json(v)).into_response(),Err(e)=>fail(e)}
}
async fn authorized_sources(State(app):State<App>)->Response{
    let Some(catalog)=&app.controller_catalog else{return Json(json!({"configured":false,"sources":[],"render_authorized":false})).into_response()};
    let Ok(db)=app.db.lock() else{return fail(store::Error("storage_unavailable"))};
    ([(header::CACHE_CONTROL,"no-store")],Json(json!({"configured":true,"sources":catalog.available(&db),"render_authorized":false}))).into_response()
}
async fn script()->impl IntoResponse{ ([(header::CONTENT_TYPE,"text/javascript; charset=utf-8"),(header::CACHE_CONTROL,"no-store")],include_str!("../web/controller.js")) }
#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct UiSignal{action:String}
async fn ui_lifetime(State(app):State<App>,headers:HeaderMap,Json(signal):Json<UiSignal>)->Response{
    if !same_origin(&headers){return denied()}
    let close=match signal.action.as_str(){"ready"=>false,"close"=>true,_=>return fail(store::Error("invalid_action"))};
    if !app.controller_ui.signal(close){return fail(store::Error("review_ui_unavailable"))}
    Json(json!({"ok":true,"closing":close})).into_response()
}
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
        Ok::<_,store::Error>(json!({"schema":"controller-v1","settings":settings,"jobs":jobs,"total_jobs":count,"listed_limit":500,"quota":{"local_receipts":store::quota(&db,t)?,"ops":ops},"worker_connected":app.controller_manual.configured(),"manual_native_enabled":app.controller_manual.configured(),"isolated_ui_lifetime":app.controller_ui.enabled(),"publishing_connected":false,"review_required":true}))})();
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
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
struct PrepareClip {revision:i64}
async fn import_clip(State(app):State<App>,Path(id):Path<String>,headers:HeaderMap,Json(a):Json<PrepareClip>)->Response{
    if !same_origin(&headers){return denied();}
    let result=tokio::task::spawn_blocking(move||crate::controller_clip::import_completed(&app,&id,a.revision,now())).await.unwrap_or(Err(store::Error("worker_preparation_failed")));
    match result{Ok(v)=>Json(v).into_response(),Err(e)=>fail(e)}
}
async fn render_reviewed(State(app):State<App>,Path(id):Path<String>,headers:HeaderMap,Json(a):Json<PrepareClip>)->Response{
    if !same_origin(&headers){return denied();}
    if !app.controller_ui.ready(){return fail(store::Error("review_ui_not_ready"));}
    let result=tokio::task::spawn_blocking(move||app.controller_manual.admit(&app,&id,a.revision)).await.unwrap_or(Err(store::Error("native_admission_unavailable")));
    match result{Ok(value)=>(StatusCode::ACCEPTED,Json(value)).into_response(),Err(error)=>fail(error)}
}
async fn editor_render_reviewed(State(app):State<App>,headers:HeaderMap,Json(a):Json<crate::controller_manual::EditorSelection>)->Response{
    if !same_origin(&headers){return denied();}
    if !app.controller_ui.ready(){return fail(store::Error("review_ui_not_ready"));}
    let result=tokio::task::spawn_blocking(move||app.controller_manual.admit_editor(&app,&a)).await.unwrap_or(Err(store::Error("native_admission_unavailable")));
    match result{Ok(v)=>(StatusCode::ACCEPTED,Json(v)).into_response(),Err(e)=>fail(e)}
}
pub(crate) async fn editor_candidate(State(app):State<App>,headers:HeaderMap,Json(a):Json<crate::controller_manual::EditorSelection>)->Response{
    if !same_origin(&headers){return denied();}
    let result=tokio::task::spawn_blocking(move||crate::controller_candidate::save(&app,&a)).await.unwrap_or(Err(store::Error("candidate_unavailable")));
    match result{Ok(v)=>Json(v).into_response(),Err(e)=>fail(e)}
}
async fn prepare_clip(State(app):State<App>,Path(id):Path<String>,headers:HeaderMap,Json(a):Json<PrepareClip>)->Response{
    if !same_origin(&headers){return denied();}
    let result=tokio::task::spawn_blocking(move||crate::controller_clip::prepare(&app,&id,a.revision)).await
        .unwrap_or(Err(store::Error("worker_preparation_failed")));
    match result{Ok(v)=>Json(v).into_response(),Err(e)=>fail(e)}
}
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
        Ok((store::preview(&db,&id)?,j.caption_status))
    });
    match result {
        Ok((v,status))=>{
            let mut response=([(header::CONTENT_TYPE,"video/mp4"),(header::CACHE_CONTROL,"no-store"),(header::X_CONTENT_TYPE_OPTIONS,"nosniff")],Bytes::from(v)).into_response();
            match status.as_deref() {
                Some("no_speech")=>{response.headers_mut().insert("x-lifeinvader-caption-status",axum::http::HeaderValue::from_static("no_speech"));},
                Some("captioned")=>{response.headers_mut().insert("x-lifeinvader-caption-status",axum::http::HeaderValue::from_static("captioned"));},
                Some(_)=>return fail(store::Error("preview_integrity_failed")),
                None=>(),
            }
            response
        },
        Err(e)=>fail(e)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{Arc,Mutex};
    use rusqlite::Connection;
    use tower::ServiceExt;
    use axum::{body::{Body,to_bytes},http::Request};
    use sha2::{Digest,Sha256};
    #[tokio::test] async fn editor_native_route_denies_unconfigured_and_cross_origin_without_claims(){
        let a=app();let api=routes().with_state(a.clone());
        let path="/api/controller/editor-render-reviewed";
        let body=r#"{"source_id":1,"source_ref":"sample-source/original.mp4","start_ms":1000,"end_ms":31000}"#;
        let response=api.clone().oneshot(request("POST",path,body,false)).await.unwrap();assert_eq!(response.status(),StatusCode::FORBIDDEN);
        let response=api.clone().oneshot(request("POST",path,body,true)).await.unwrap();assert_eq!(response.status(),StatusCode::CONFLICT);
        let v:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),16384).await.unwrap()).unwrap();assert_eq!(v["reason"],"native_manual_unavailable");
        let response=package_routes().with_state(a.clone()).oneshot(request("POST",path,body,true)).await.unwrap();assert_eq!(response.status(),StatusCode::NOT_FOUND);
        let db=a.db.lock().unwrap();assert_eq!(db.query_row("SELECT COUNT(*) FROM controller_admissions",[],|r|r.get::<_,i64>(0)).unwrap(),0);assert_eq!(db.query_row("SELECT COUNT(*) FROM controller_admission_launches",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
    #[tokio::test] async fn status_details_and_auth_are_read_only_and_default_denied(){
        let a=app();let api=routes().with_state(a.clone());
        let admission=crate::controller_source::controller_selection(&a,1,"sample-source/original.mp4").unwrap();
        let j={let mut db=a.db.lock().unwrap();store::enqueue(&mut db,1,"sample-source/original.mp4","youtube",&crate::controller_source::controller_binding(&admission),100).unwrap()};
        let before={let db=a.db.lock().unwrap();db.execute_batch("PRAGMA query_only=ON").unwrap();db.total_changes()};
        let path=format!("/api/controller/jobs/{}/status-details",j.id);
        for _ in 0..2 {
            let response=api.clone().oneshot(request("GET",&path,"",false)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
            let v:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),16384).await.unwrap()).unwrap();
            assert_eq!(v["worker"]["state"],"UNKNOWN");assert_eq!(v["deletion_enabled"],false);assert_eq!(v["publishing_enabled"],false);
        }
        let response=api.clone().oneshot(request("GET","/api/controller/provider-auth","",false)).await.unwrap();
        let v:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),16384).await.unwrap()).unwrap();
        assert_eq!(v["connect_enabled"],false);assert_eq!(v["publishing_enabled"],false);
        let providers=v["providers"].as_array().unwrap();
        for (id,state) in [("youtube","unconfigured"),("instagram","held"),("tiktok",if cfg!(feature="snapmatic-hud"){"manual"}else{"unconfigured"})]{
            assert_eq!(providers.iter().find(|p|p["provider"]==id).unwrap()["health"],state);
        }
        assert_eq!(providers.iter().find(|p|p["provider"]=="instagram").unwrap()["transport_enabled"],false);
        assert_eq!(api.oneshot(request("POST",&path,"{}",true)).await.unwrap().status(),StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(a.db.lock().unwrap().total_changes(),before);
    }
    pub(crate) fn app()->App{
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("controller-test-data").join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(root.join("sample-source")).unwrap();std::fs::write(root.join("sample-source/original.mp4"),b"inert source").unwrap();
        let db=Connection::open_in_memory().unwrap();crate::init(&db).unwrap();store::init(&db).unwrap();
        db.execute("INSERT INTO profiles(id,name,platform,url,role,source_folder) VALUES(1,'sample-source','tiktok','https://www.tiktok.com/@sample-source','source','sample-source')",[]).unwrap();
        db.execute("INSERT INTO settings(key,value) VALUES('source_root',?)",[root.to_str().unwrap()]).unwrap();
        App{cache_scope:"adapted-isolated-test".into(),controller_ui:Arc::new(crate::controller_ui_lifetime::Lifetime::new(false)),controller_catalog:None,controller_manual:Arc::new(crate::controller_manual::Unavailable),db:Arc::new(Mutex::new(db)),media:root.clone(),log_dir:root,controller_ops:Arc::new(crate::controller_ops::Unavailable),controller_worker:Arc::new(crate::controller_worker::Unavailable),
}
    }
    #[tokio::test] async fn package_review_has_no_dispatch_or_ops(){
        struct RefuseOps;impl crate::controller_ops::StatusReader for RefuseOps{fn read(&self,_:i64)->crate::controller_ops::View{panic!("OPS must not be consulted")}}
        let mut a=app();a.controller_ops=Arc::new(RefuseOps);let before=a.db.lock().unwrap().total_changes();let api=package_routes().with_state(a.clone());
        let response=api.clone().oneshot(request("GET","/api/controller","",false)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
        let body=to_bytes(response.into_body(),65536).await.unwrap();let v:serde_json::Value=serde_json::from_slice(&body).unwrap();assert_eq!(v["manual_native_enabled"],false);assert_eq!(v["package_read_only"],true);assert!(!String::from_utf8(body.to_vec()).unwrap().contains("metricool"));
        for path in ["/api/controller/jobs","/api/controller/settings","/api/controller/jobs/x/render-reviewed","/api/controller/jobs/x/import-completed","/api/controller/quota"]{assert_eq!(api.clone().oneshot(request("POST",path,"{}",true)).await.unwrap().status(),StatusCode::NOT_FOUND);}
        assert_eq!(a.db.lock().unwrap().total_changes(),before);
    }
    #[tokio::test] async fn manual_native_route_is_default_deny_and_explicit_only(){
        use std::sync::atomic::{AtomicUsize,Ordering};
        struct Recording(Arc<AtomicUsize>);
        impl crate::controller_manual::Manual for Recording {
            fn configured(&self)->bool{true}
            fn admit(&self,_:&App,id:&str,revision:i64)->store::Result<serde_json::Value>{
                assert_eq!((id,revision),("reviewed-one",7));self.0.fetch_add(1,Ordering::SeqCst);
                Ok(json!({"state":"PROCESSING","publish":false}))
            }
        }
        let mut app=app();let before=app.db.lock().unwrap().total_changes();
        let path="/api/controller/jobs/reviewed-one/render-reviewed";
        let denied_route=routes().with_state(app.clone());
        assert_eq!(denied_route.oneshot(request("POST",path,r#"{"revision":7}"#,true)).await.unwrap().status(),StatusCode::CONFLICT);
        assert_eq!(before,app.db.lock().unwrap().total_changes());
        let calls=Arc::new(AtomicUsize::new(0));app.controller_manual=Arc::new(Recording(calls.clone()));let api=routes().with_state(app);
        assert_eq!(api.clone().oneshot(request("POST",path,r#"{"revision":7}"#,false)).await.unwrap().status(),StatusCode::FORBIDDEN);
        assert_eq!(api.clone().oneshot(request("POST",path,r#"{"revision":7,"publish":true}"#,true)).await.unwrap().status(),StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(calls.load(Ordering::SeqCst),0);
        assert_eq!(api.oneshot(request("POST",path,r#"{"revision":7}"#,true)).await.unwrap().status(),StatusCode::ACCEPTED);
        assert_eq!(calls.load(Ordering::SeqCst),1);
    }
    #[tokio::test] async fn isolated_ui_gates_manual_dispatch_before_ready_and_after_close(){
        use std::sync::atomic::{AtomicUsize,Ordering};
        struct Recording(Arc<AtomicUsize>);
        impl crate::controller_manual::Manual for Recording {
            fn configured(&self)->bool{true}
            fn admit(&self,_:&App,id:&str,revision:i64)->store::Result<serde_json::Value>{
                assert_eq!((id,revision),("reviewed-only",0));
                self.0.fetch_add(1,Ordering::SeqCst);Ok(json!({"publish":false}))
            }
        }
        let mut app=app();let calls=Arc::new(AtomicUsize::new(0));
        app.controller_manual=Arc::new(Recording(calls.clone()));
        app.controller_ui=Arc::new(crate::controller_ui_lifetime::Lifetime::new(true));
        let before=app.db.lock().unwrap().total_changes();
        let api=routes().layer(axum::middleware::from_fn(crate::local_only)).with_state(app.clone());
        let render="/api/controller/jobs/reviewed-only/render-reviewed";
        let signal="/api/controller/ui-lifetime";
        assert_eq!(api.clone().oneshot(request("POST",render,r#"{"revision":0}"#,true)).await.unwrap().status(),StatusCode::CONFLICT);
        assert_eq!(calls.load(Ordering::SeqCst),0);
        assert_eq!(api.clone().oneshot(request("POST",signal,r#"{"action":"ready"}"#,true)).await.unwrap().status(),StatusCode::OK);
        assert_eq!(api.clone().oneshot(request("POST",render,r#"{"revision":0}"#,true)).await.unwrap().status(),StatusCode::ACCEPTED);
        assert_eq!(calls.load(Ordering::SeqCst),1);
        assert_eq!(api.clone().oneshot(request("POST",signal,r#"{"action":"close"}"#,true)).await.unwrap().status(),StatusCode::OK);
        assert_eq!(api.oneshot(request("POST",render,r#"{"revision":0}"#,true)).await.unwrap().status(),StatusCode::CONFLICT);
        assert_eq!(calls.load(Ordering::SeqCst),1);
        assert_eq!(before,app.db.lock().unwrap().total_changes());
        assert_eq!(app.db.lock().unwrap().query_row("SELECT COUNT(*) FROM controller_admissions",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
    #[tokio::test] async fn review_ui_lifetime_origin_close_and_no_database_mutation(){
        let mut app=app();app.controller_ui=Arc::new(crate::controller_ui_lifetime::Lifetime::new(true));
        let before=app.db.lock().unwrap().total_changes();let api=routes().with_state(app.clone());
        let path="/api/controller/ui-lifetime";
        assert_eq!(api.clone().oneshot(request("POST",path,r#"{"action":"ready"}"#,false)).await.unwrap().status(),StatusCode::FORBIDDEN);
        assert!(!app.controller_ui.ready());
        assert_eq!(api.clone().oneshot(request("POST",path,r#"{"action":"ready","publish":true}"#,true)).await.unwrap().status(),StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(api.clone().oneshot(request("POST",path,r#"{"action":"ready"}"#,true)).await.unwrap().status(),StatusCode::OK);
        assert!(app.controller_ui.ready());
        assert_eq!(api.clone().oneshot(request("POST",path,r#"{"action":"close"}"#,true)).await.unwrap().status(),StatusCode::OK);
        assert!(!app.controller_ui.ready());
        assert_eq!(api.oneshot(request("POST",path,r#"{"action":"ready"}"#,true)).await.unwrap().status(),StatusCode::CONFLICT);
        assert_eq!(before,app.db.lock().unwrap().total_changes());
    }
    #[tokio::test] async fn controller_reachability_inprocess_real_middleware(){
        let app=app();let before=app.db.lock().unwrap().total_changes();
        let api=routes().layer(axum::middleware::from_fn(crate::local_only)).with_state(app.clone());
        for path in ["/controller","/controller.js","/api/controller"]{
            let response=tokio::time::timeout(std::time::Duration::from_secs(2),api.clone().oneshot(request("GET",path,"",false))).await.unwrap().unwrap();
            assert_eq!(response.status(),StatusCode::OK,"{path}");
            assert!(!to_bytes(response.into_body(),256*1024).await.unwrap().is_empty());
        }
        let bad_origin=Request::builder().uri("/controller").header("host","127.0.0.1:43847").header("origin","https://example.invalid").body(Body::empty()).unwrap();
        assert_eq!(api.oneshot(bad_origin).await.unwrap().status(),StatusCode::FORBIDDEN);
        assert_eq!(before,app.db.lock().unwrap().total_changes());
    }
    #[tokio::test] async fn controller_reachability_same_process_loopback_no_worker(){
        use std::io::{Read,Write};
        use std::time::Duration;
        let mut app=app();app.controller_ui=Arc::new(crate::controller_ui_lifetime::Lifetime::new(true));
        let before=app.db.lock().unwrap().total_changes();
        let listener=tokio::net::TcpListener::bind(("127.0.0.1",0)).await.unwrap();
        let address=listener.local_addr().unwrap();
        let ui=app.controller_ui.clone();
        let api=routes().layer(axum::middleware::from_fn(crate::local_only)).with_state(app.clone());
        let mut server=tokio::spawn(async move{
            axum::serve(listener,api).with_graceful_shutdown(async move{let _=crate::controller_ui_lifetime::wait(&ui,None).await;}).await
        });
        // Same test process, fresh ephemeral port; never probes a held app origin.
        let client=tokio::task::spawn_blocking(move||{
            for (method,path,body) in [("GET","/controller",""),("GET","/controller.js",""),("GET","/api/controller",""),("POST","/api/controller/ui-lifetime",r#"{"action":"ready"}"#),("POST","/api/controller/ui-lifetime",r#"{"action":"close"}"#)]{
                let mut socket=std::net::TcpStream::connect_timeout(&address,Duration::from_secs(2)).unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();socket.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
                write!(socket,"{method} {path} HTTP/1.1\r\nHost: {address}\r\nOrigin: http://{address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                let mut response=String::new();socket.take(256*1024).read_to_string(&mut response).unwrap();
                assert!(response.starts_with("HTTP/1.1 200"),"{path}: unexpected status");
                if path=="/controller"{assert!(response.contains("controller-page"));}
                if path=="/api/controller"{assert!(response.contains("\"manual_native_enabled\":false"));}
            }
        });
        let result=tokio::time::timeout(Duration::from_secs(12),client).await;
        // Ensure even failed diagnostic clients stop only their in-process test server.
        app.controller_ui.signal(true);
        let stopped=tokio::time::timeout(Duration::from_secs(2),&mut server).await;
        if stopped.is_err(){server.abort();}
        result.unwrap().unwrap();stopped.unwrap().unwrap().unwrap();
        assert_eq!(before,app.db.lock().unwrap().total_changes());
        assert_eq!(app.db.lock().unwrap().query_row("SELECT COUNT(*) FROM controller_admissions",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        eprintln!("TEST_LOOPBACK_GET_READY_CLOSE_PASS no_worker=true");
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
    #[tokio::test] async fn clip_prepare_route_rejects_untrusted_inputs_without_writes(){
        let app=app();let api=routes().with_state(app.clone());
        let before=app.db.lock().unwrap().total_changes();
        assert_eq!(api.clone().oneshot(request("POST","/api/controller/jobs/x/prepare",r#"{"revision":0}"#,false)).await.unwrap().status(),StatusCode::FORBIDDEN);
        assert_eq!(api.clone().oneshot(request("POST","/api/controller/jobs/x/prepare",r#"{"revision":0,"render":true}"#,true)).await.unwrap().status(),StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(api.oneshot(request("GET","/api/controller/jobs/x/prepare","",false)).await.unwrap().status(),StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(before,app.db.lock().unwrap().total_changes());
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
