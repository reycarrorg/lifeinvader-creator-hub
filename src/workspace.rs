use crate::{App,controller_store as store,workspace_media as media,workspace_events};
use axum::{Extension,Json,Router,body::{Body,Bytes},extract::{State,Path as AxumPath,Request},http::{Method,StatusCode,header},response::{Html,IntoResponse,Response},routing::{get,post}};
use futures_util::StreamExt;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use serde_json::{Value,json};

pub fn routes(app:App)->Router<App>{
    Router::new().route("/workspace",get(page)).route("/workspace/manage",get(manage)).route("/workspace.css",get(css)).route("/workspace.js",get(js))
        .route("/manifest.webmanifest",get(manifest)).route("/favicon.ico",get(favicon))
        .route("/icon-192.png",get(icon_192)).route("/icon-512.png",get(icon_512))
        .route("/workspace-phase5.js",get(||async{([(header::CONTENT_TYPE,"text/javascript; charset=utf-8")],include_str!("../web/workspace-phase5.js"))}))
 .route("/workspace-edit.js",get(||async{([(header::CONTENT_TYPE,"text/javascript; charset=utf-8")],include_str!("../web/workspace-edit.js"))}))
        .route("/workspace-platform-draft.js",get(||async{([(header::CONTENT_TYPE,"text/javascript; charset=utf-8")],include_str!("../web/workspace-platform-draft.js"))}))
        .route("/api/clips/{id}/platform-draft",get(crate::workspace_platform_drafts::read).put(crate::workspace_platform_drafts::save))
        .route("/workspace-preflight.js",get(||async{([(header::CONTENT_TYPE,"text/javascript; charset=utf-8")],include_str!("../web/workspace-preflight.js"))}))
        .route("/api/workspace/media-facts",get(||async{([(header::CONTENT_TYPE,"application/json"),(header::CACHE_CONTROL,"no-store")],include_str!("../web/workspace-media-facts.json"))}))
        .route("/api/workspace",get(data))
        .route("/api/workspace/local-status",get(crate::workspace_film_roll::local_status))
        .route("/api/workspace/export-history",get(crate::workspace_film_roll::export_history))
        .route("/api/workspace/thumbnail-cache/clear",post(crate::workspace_film_roll::clear_thumbnail_cache))
        .route("/api/workspace/caption-presets",get(crate::workspace_film_roll::list_presets).post(crate::workspace_film_roll::save_preset))
        .route("/api/workspace/caption-presets/{id}",axum::routing::delete(crate::workspace_film_roll::delete_preset))
        .route("/api/oauth/status",get(crate::workspace_oauth::status))
        .route("/api/oauth/{provider}/config",post(crate::workspace_oauth::save_config))
        .route("/api/oauth/{provider}/start",post(crate::workspace_oauth::start_oauth))
        .route("/api/oauth/{provider}/callback",get(crate::workspace_oauth::callback))
        .route("/api/oauth/{provider}/refresh",post(crate::workspace_oauth::refresh))
        .route("/api/oauth/{provider}/disconnect",post(crate::workspace_oauth::disconnect))
        .route("/api/oauth/instagram/token",post(crate::workspace_oauth::connect_instagram_token))
        .route("/api/oauth/youtube/upload-draft/{clip_id}",post(crate::workspace_oauth::upload_youtube_private_draft))
        .route("/api/workspace/film-roll",get(crate::workspace_film_roll::list).post(crate::workspace_film_roll::add))
        .route("/api/workspace/film-roll/batch",post(crate::workspace_film_roll::batch_action))
        .route("/api/workspace/film-roll/clear",post(crate::workspace_film_roll::clear))
        .route("/api/workspace/film-roll/open-exports",post(crate::workspace_film_roll::open_exports))
        .route("/api/workspace/film-roll/export-progress",get(crate::workspace_film_roll::export_progress))
        .route("/api/workspace/film-roll/export-cancel",post(crate::workspace_film_roll::cancel_export))
        .route("/api/workspace/film-roll/export-batch",post(crate::workspace_film_roll::export_batch))
        .route("/api/workspace/film-roll/exports/{id}/reveal",post(crate::workspace_film_roll::reveal_export))
        .route("/api/workspace/film-roll/{id}",axum::routing::delete(crate::workspace_film_roll::remove))
        .route("/api/workspace/film-roll/{id}/probe",get(crate::workspace_film_roll::probe_clip))
        .route("/api/workspace/film-roll/{id}/state",post(crate::workspace_film_roll::set_state))
        .route("/api/workspace/film-roll/{id}/export",get(crate::workspace_film_roll::stream_export).head(crate::workspace_film_roll::stream_export).post(crate::workspace_film_roll::export_clip).layer(axum::extract::DefaultBodyLimit::max(14*1024*1024)))
        .route("/api/workspace/intake",get(phase4_state))
        .route("/api/workspace/tiktok/export/{id}",get(crate::workspace_tiktok_export::status).post(crate::workspace_tiktok_export::export).layer(axum::extract::DefaultBodyLimit::max(5*1024*1024)))
        .route("/api/workspace/tiktok/export-folder",post(crate::workspace_tiktok_export::open_folder))
        .route("/api/workspace/tiktok/inbox-draft",post(crate::controller_tiktok_inbox::draft).layer(axum::extract::DefaultBodyLimit::max(4096)))
        .route("/api/workspace/cache-preview",get(phase4_cache)).route("/api/workspace/cache",get(phase4_cache_usage))
        .route("/api/workspace/local-preview",get(nvenc_status).post(nvenc_start))
        .route("/api/workspace/local-preview/{id}/video",get(nvenc_stream).head(nvenc_stream))
        .route("/api/clips/{id}/draft",get(crate::workspace_drafts::read).put(crate::workspace_drafts::save))
        .route("/api/workspace/open-backup",post(crate::workspace_drafts::open_backup))
        .route("/api/workspace/triage",get(crate::workspace_triage::read).post(crate::workspace_triage::write)
            .layer(axum::extract::DefaultBodyLimit::max(8192)))
        .route("/api/workspace/channel-profiles",get(crate::workspace_film_roll::channel_profiles))
        .route("/api/workspace/restore/{kind}",post(crate::workspace_phase1::restore_open))
        .route("/api/workspace/diagnostics",post(crate::workspace_phase1::export))
        .route("/api/workspace/ledger",get(ledger)).route("/api/workspace/system",get(system))
        .route("/api/clips/{id}/video",get(clip_stream).head(clip_stream))
        .route("/api/clips/{id}/poster",get(poster).head(poster)).route("/api/workspace/clips/{id}/stream",get(clip_stream).head(clip_stream))
        .route("/api/workspace/clips/{id}/poster",get(poster).head(poster))
        .route("/api/workspace/publish",post(block_publish)).route("/api/events",get(workspace_events::sse))
        .layer(Extension(workspace_events::Feed::start(app)))
}
async fn manage()->Html<String>{Html(include_str!("../web/index.html").replace("</head>","<style>.layout>.sidebar,#sidebar-toggle{display:none!important}.layout{display:block!important}</style></head>"))}
async fn page()->Html<&'static str>{Html(include_str!("../web/workspace.html"))}
async fn css()->impl IntoResponse{([(header::CONTENT_TYPE,"text/css; charset=utf-8")],include_str!("../web/workspace.css"))}
async fn js()->impl IntoResponse{([(header::CONTENT_TYPE,"text/javascript; charset=utf-8")],include_str!("../web/workspace.js"))}
async fn manifest()->impl IntoResponse {
    ([(header::CONTENT_TYPE,"application/manifest+json"),(header::CACHE_CONTROL,"no-cache")],
        include_str!("../web/manifest.webmanifest"))
}
async fn favicon()->impl IntoResponse { asset("image/x-icon",include_bytes!("../web/snapmatic.ico")) }
async fn icon_192()->impl IntoResponse { asset("image/png",include_bytes!("../web/icon-192.png")) }
async fn icon_512()->impl IntoResponse { asset("image/png",include_bytes!("../web/icon-512.png")) }
fn asset(ty:&'static str, bytes:&'static [u8])->impl IntoResponse{
    ([(header::CONTENT_TYPE,ty),(header::CACHE_CONTROL,"public, max-age=86400"),(header::X_CONTENT_TYPE_OPTIONS,"nosniff")],bytes)
}
async fn block_publish()->impl IntoResponse{
    (StatusCode::FORBIDDEN,Json(json!({"reason":"public_publish_disabled","publishing_enabled":false})))
}
fn no_store(v:Value)->Response{([(header::CACHE_CONTROL,"no-store")],Json(v)).into_response()}
async fn ledger(State(app):State<App>)->Response{
    match tokio::task::spawn_blocking(move||crate::workspace_ledger::data(&app)).await {
        Ok(Ok(v))=>no_store(v),_=>safe_error(store::Error("ledger_unavailable")),
    }
}
async fn system(State(app):State<App>)->Response{
    let db=match app.db.lock(){Ok(v)=>v,Err(_)=>return safe_error(store::Error("storage_unavailable"))};
    let cfg=crate::source::config(&db).ok();
    let root=cfg.as_ref().map(|v|v.root.clone()).filter(|s|!s.is_empty());
    let exports=crate::workspace_film_roll::exports_dir(&app).to_string_lossy().into_owned();
    no_store(json!({"schema":"snapmatic-system-v1","loopback_only":true,"publishing_enabled":false,"cleanup_enabled":false,
        "preview_mode":"local source & Film Roll references","workers_in_eta_mode":false,
        "source_path":root,
        "backup_path":Some(exports)}))
}
fn safe_error(e:store::Error)->Response{
    (StatusCode::SERVICE_UNAVAILABLE,Json(json!({"reason":e.0}))).into_response()
}
async fn phase4_state(State(app):State<App>)->Response{
    tokio::task::spawn_blocking(move||no_store(crate::workspace_phase4::state(&app))).await
        .unwrap_or_else(|_|safe_error(store::Error("intake_status_unavailable")))
}
async fn phase4_cache_usage(State(app):State<App>)->Response{
    tokio::task::spawn_blocking(move||match crate::workspace_phase4::cache_usage(&app){
        Ok(v)=>no_store(v),Err(e)=>safe_error(e)
    }).await.unwrap_or_else(|_|safe_error(store::Error("cache_usage_unavailable")))
}
async fn phase4_cache(State(app):State<App>)->Response{
    tokio::task::spawn_blocking(move||match crate::workspace_phase4::cache_preview(&app){
        Ok(v)=>no_store(v),Err(e)=>safe_error(e)
    }).await.unwrap_or_else(|_|safe_error(store::Error("cache_preview_unavailable")))
}
async fn data(State(app):State<App>)->Response{
    let result=tokio::task::spawn_blocking(move||{
        let mut index=media::index(&app)?;
        if let Ok(add)=crate::workspace_phase4::rows(&app) {
            for row in add{
                if !index.iter().any(|r|r["clip_id"]==row["clip_id"]||
                    (r["preview_sha256"].is_string()&&r["preview_sha256"]==row["preview_sha256"])){
                    index.push(row);
                }
            }
        }
        if let Ok(add)=crate::workspace_film_roll::rows(&app) {
            for row in add{
                if !index.iter().any(|r|r["clip_id"]==row["clip_id"]){
                    index.push(row);
                }
            }
        }
        let d=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
        let jobs=store::jobs(&d)?.into_iter().map(|j|{
            json!({"kind":"rendered","clip_id":j.id,"id":j.id,
                "name":j.source_ref.rsplit(['\\','/']).next().unwrap_or("Rendered clip"),
                "state":j.state,"caption_status":j.caption_status,"review":j.review,"revision":j.revision,
                "preview_sha256":j.preview_sha256,"poster_available":false,
                "preview_available":j.preview_sha256.is_some()&&matches!(j.state.as_str(),"READY_FOR_REVIEW"|"POSTED"|"BACKED_UP"),
                "preview_only":true})
        }).collect::<Vec<_>>();
        Ok::<_,store::Error>(json!({"schema":"snapmatic-workspace-v1","index":index,"jobs":jobs,
                                   "publishing_enabled":false,"cleanup_enabled":false,"draft_submit_enabled":false}))
    }).await.unwrap_or(Err(store::Error("workspace_unavailable")));
    match result{Ok(v)=>no_store(v),Err(e)=>safe_error(e)}
}
async fn nvenc_status(State(app):State<App>)->Response{
    match tokio::task::spawn_blocking(move||crate::workspace_nvenc::status(&app)).await{
        Ok(v)=>no_store(v),Err(_)=>safe_error(store::Error("preview_status_unavailable")),
    }
}
async fn nvenc_start(State(app):State<App>,headers:axum::http::HeaderMap,Json(r):Json<crate::workspace_nvenc::Request>)->Response{
    if headers.get(header::ORIGIN).and_then(|x|x.to_str().ok())!=Some("http://127.0.0.1:3847"){
        return (StatusCode::FORBIDDEN,Json(json!({"reason":"local_origin_required"}))).into_response();
    }
    match tokio::task::spawn_blocking(move||crate::workspace_nvenc::dispatch(app,r)).await{
        Ok(Ok(v))=>(StatusCode::ACCEPTED,Json(v)).into_response(),
        Ok(Err(e))=>(StatusCode::CONFLICT,Json(json!({"reason":e.0}))).into_response(),
        Err(_)=>safe_error(store::Error("preview_dispatch_unavailable")),
    }
}
async fn nvenc_stream(State(app):State<App>,AxumPath(id):AxumPath<String>,request:Request)->Response{
    let (path,lease)=match tokio::task::spawn_blocking(move||crate::workspace_nvenc::open(&app,&id)).await{
        Ok(Ok(x))=>x,_=>return missing(),
    };
    let r=ServeFile::new(path).oneshot(request).await.unwrap_or_else(|never|match never{});
    let (mut parts,body)=r.into_response().into_parts();
    parts.headers.insert(header::CACHE_CONTROL,header::HeaderValue::from_static("no-store"));
    parts.headers.insert(header::X_CONTENT_TYPE_OPTIONS,header::HeaderValue::from_static("nosniff"));
    Response::from_parts(parts,Body::from_stream(body.into_data_stream().map(move|c|{let _=&lease;c})))
}

fn missing()->Response{(StatusCode::NOT_FOUND,Json(json!({"reason":"preview_unavailable"}))).into_response()}
async fn staged_asset(app:App,id:String,request:Request,poster:bool)->Response{
    let resolved=tokio::task::spawn_blocking(move||match media::open(&app,&id,poster){
        Err(e) if matches!(e.0,"preview_unavailable"|"manifest_unavailable")=>{
            if media::index(&app)?.iter().any(|r|r["clip_id"]==id){Err(e)}
            else{crate::workspace_phase4::open(&app,&id,poster).map_err(|_|store::Error("preview_unavailable"))}
        }
        result=>result,
    })
        .await.unwrap_or(Err(store::Error("preview_unavailable")));
    let (path,lease)=match resolved{Ok(v)=>v,Err(e) if e.0=="preview_unavailable"=>return missing(),Err(e)=>return safe_error(e)};
    let r=ServeFile::new(path).oneshot(request).await.unwrap_or_else(|never|match never{});
    let (mut parts,body)=r.into_response().into_parts();
    parts.headers.insert(header::CACHE_CONTROL,header::HeaderValue::from_static("no-store"));
    parts.headers.insert(header::X_CONTENT_TYPE_OPTIONS,header::HeaderValue::from_static("nosniff"));
    let owned=body.into_data_stream().map(move|chunk|{let _=&lease;chunk});
    Response::from_parts(parts,Body::from_stream(owned))
}
async fn poster(State(app):State<App>,AxumPath(id):AxumPath<String>,r:Request)->Response {
    if !media::hex(&id){return missing();}
    if let Some((source_id,path))=crate::workspace_film_roll::resolve_source_ref(&app,&id){
        return crate::source::thumbnail_for(app,source_id,crate::source::FileQuery{path},"source").await.into_response();
    }
    staged_asset(app,id,r,true).await
}
fn one_range(raw:&str,n:usize)->Option<(usize,usize)>{
    let i=raw.strip_prefix("bytes=")?;let (left,right)=i.split_once('-')?;
    if n==0||i.contains(','){return None;}
    if left.is_empty(){
        let num=right.parse::<usize>().ok()?;
        return (num>0).then_some((n.saturating_sub(num),n-1));
    }
    let start=left.parse::<usize>().ok()?;
    let end=if right.is_empty(){n-1}else{right.parse::<usize>().ok()?.min(n-1)};
    (start<n&&start<=end).then_some((start,end))
}
fn rendered_bytes(bytes:Vec<u8>,request:Request)->Response{
    let len=bytes.len();let head=*request.method()==Method::HEAD;
    let mut b=Response::builder().header(header::ACCEPT_RANGES,"bytes")
        .header(header::CONTENT_TYPE,"video/mp4").header(header::CACHE_CONTROL,"no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS,"nosniff");
    let body=if let Some(r)=request.headers().get(header::RANGE){
        let Some((a,z))=r.to_str().ok().and_then(|r|one_range(r,len)) else {
            return b.status(StatusCode::RANGE_NOT_SATISFIABLE).header(header::CONTENT_RANGE,format!("bytes */{len}"))
                .header(header::CONTENT_LENGTH,0).body(Body::empty()).unwrap();
        };
        b=b.status(StatusCode::PARTIAL_CONTENT).header(header::CONTENT_RANGE,format!("bytes {a}-{z}/{len}"))
            .header(header::CONTENT_LENGTH,z-a+1);
        if head{Body::empty()}else{Body::from(Bytes::from(bytes).slice(a..=z))}
    }else{
        b=b.header(header::CONTENT_LENGTH,len);
        if head{Body::empty()}else{Body::from(bytes)}
    };
    b.body(body).unwrap()
}
async fn clip_stream(State(app):State<App>,AxumPath(id):AxumPath<String>,r:Request)->Response {
    if media::hex(&id){
        if let Some(path)=crate::workspace_film_roll::resolve_video(&app,&id){
            let resp=ServeFile::new(path).oneshot(r).await.unwrap_or_else(|never|match never{});
            let (mut parts,body)=resp.into_response().into_parts();
            parts.headers.insert(header::CACHE_CONTROL,header::HeaderValue::from_static("no-store"));
            parts.headers.insert(header::X_CONTENT_TYPE_OPTIONS,header::HeaderValue::from_static("nosniff"));
            return Response::from_parts(parts,body);
        }
        return staged_asset(app,id,r,false).await;
    }
    if uuid::Uuid::parse_str(&id).ok().is_none_or(|v|v.to_string()!=id){return missing();}
    let result=tokio::task::spawn_blocking(move||{
        let d=app.db.lock().map_err(|_|store::Error("storage_unavailable"))?;
        let j=store::job(&d,&id)?;
        if !crate::controller_source::controller_current(&d,j.source_id,&j.source_binding){return Err(store::Error("source_unavailable"));}
        store::preview(&d,&id)
    }).await.unwrap_or(Err(store::Error("preview_unavailable")));
    match result{Ok(bytes)=>rendered_bytes(bytes,r),Err(_)=>missing()}
}
