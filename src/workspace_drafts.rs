
use crate::{App,workspace_media as media,controller_store as store};
use axum::{Json,extract::{Path,State},http::{HeaderMap,StatusCode},response::{IntoResponse,Response}};
use rusqlite::OptionalExtension;
use serde::{Deserialize,Serialize};
use serde_json::json;

#[derive(Default,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Draft {
    title:String, caption:String, tags:String,
    cover_ms:Option<u64>, in_ms:Option<u64>, out_ms:Option<u64>,
}
fn fail(s:StatusCode,reason:&str)->Response{(s,Json(json!({"reason":reason}))).into_response()}
fn ok(d:Option<Draft>)->Response{
    ([(axum::http::header::CACHE_CONTROL,"no-store")],Json(json!({"draft":d}))).into_response()
}
fn origin(h:&HeaderMap)->bool{
    h.get("origin").and_then(|x|x.to_str().ok()).is_some_and(|o|o=="http://127.0.0.1"||o.starts_with("http://127.0.0.1:"))
}
impl Draft {
    fn valid(&self)->bool {
        self.title.chars().count()<=150 && self.caption.chars().count()<=10000 &&
        self.tags.chars().count()<=1000 &&
        [&self.title,&self.caption,&self.tags].iter().all(|v|!v.contains(char::from(0))) &&
        [self.cover_ms,self.in_ms,self.out_ms].into_iter().flatten().all(|ms|ms<=86_400_000) &&
        !matches!((self.in_ms,self.out_ms),(Some(a),Some(b)) if a>=b)
    }
}
fn access(app:&App,id:&str,write:Option<Draft>)->Result<Option<Draft>,StatusCode>{
    let indexed=media::hex(id);
    if indexed {
        let in_film_roll=crate::workspace_film_roll::exists(app,id);
        if !in_film_roll && !media::index(app).map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?.iter().any(|r|
            r["clip_id"].as_str()==Some(id) && r["preview_available"]==true){return Err(StatusCode::NOT_FOUND);}
    }else if !uuid::Uuid::parse_str(id).is_ok_and(|u|u.to_string()==id){return Err(StatusCode::NOT_FOUND);}
    let db=app.db.lock().map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
    if !indexed {
        let j=store::job(&db,id).map_err(|_|StatusCode::NOT_FOUND)?;
        if j.preview_sha256.is_none()||!matches!(j.state.as_str(),"READY_FOR_REVIEW"|"POSTED"|"BACKED_UP")
            ||!crate::controller_source::controller_current(&db,j.source_id,&j.source_binding){
            return Err(StatusCode::NOT_FOUND);
        }
    }
    let storage=|_:serde_json::Error|StatusCode::SERVICE_UNAVAILABLE;
    if let Some(d)=write {
        let raw=serde_json::to_string(&d).map_err(storage)?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS snapmatic_clip_drafts_v1 (clip_id TEXT PRIMARY KEY NOT NULL, draft_json TEXT NOT NULL);")
            .map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
        db.execute("INSERT INTO snapmatic_clip_drafts_v1 VALUES (?1,?2) ON CONFLICT(clip_id) DO UPDATE SET draft_json=excluded.draft_json",
            rusqlite::params![id,raw]).map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
        return Ok(Some(d));
    }
    let exists=db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='snapmatic_clip_drafts_v1')",
        [],|r|r.get::<_,bool>(0)).map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
    if !exists{return Ok(None);}
    let raw=db.query_row("SELECT draft_json FROM snapmatic_clip_drafts_v1 WHERE clip_id=?1",[id],|r|r.get::<_,String>(0))
        .optional().map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
    raw.map(|v|serde_json::from_str(&v).map_err(storage)).transpose()
}
async fn run(app:App,id:String,write:Option<Draft>)->Response{
    match tokio::task::spawn_blocking(move||access(&app,&id,write)).await {
        Ok(Ok(d))=>ok(d),Ok(Err(StatusCode::NOT_FOUND))=>fail(StatusCode::NOT_FOUND,"draft_clip_unavailable"),
        _=>fail(StatusCode::SERVICE_UNAVAILABLE,"draft_storage_unavailable"),
    }
}
pub(crate) async fn read(State(app):State<App>,Path(id):Path<String>)->Response{run(app,id,None).await}
pub(crate) async fn save(State(app):State<App>,Path(id):Path<String>,h:HeaderMap,Json(d):Json<Draft>)->Response{
    if !origin(&h){return fail(StatusCode::FORBIDDEN,"local_origin_required");}
    if !d.valid(){return fail(StatusCode::UNPROCESSABLE_ENTITY,"draft_fields_invalid");}
    run(app,id,Some(d)).await
}
pub(crate) async fn open_backup(State(app):State<App>,h:HeaderMap)->Response{
    if !origin(&h){return fail(StatusCode::FORBIDDEN,"local_origin_required");}
    #[cfg(windows)]
    {
        let drive=app.db.lock().ok().and_then(|db|crate::source::config(&db).ok())
            .is_some_and(|c|c.root==crate::source::DRIVE_SOURCE_ROOT);
        let backup=std::path::Path::new(r"Z:\SnapmaticReview\Local Media\Posted Clips Backup");
        if !drive||!backup.is_dir(){return fail(StatusCode::NOT_FOUND,"backup_folder_unavailable");}
        use std::os::windows::fs::MetadataExt;
        if !matches!(backup.symlink_metadata(), Ok(meta) if meta.file_attributes()&0x400==0) {
            return fail(StatusCode::FORBIDDEN,"backup_folder_unverified");
        }
        let exe=std::path::Path::new(r"C:\Windows\explorer.exe");
        if std::process::Command::new(exe).arg(backup).spawn().is_ok(){return StatusCode::NO_CONTENT.into_response();}
    }
    fail(StatusCode::SERVICE_UNAVAILABLE,"backup_folder_open_failed")
}
