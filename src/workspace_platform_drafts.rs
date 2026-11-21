use crate::{App,workspace_media as media,controller_store as store};
use axum::{Json,extract::{Path,State},http::{HeaderMap,StatusCode},response::{IntoResponse,Response}};
use rusqlite::OptionalExtension;
use serde::{Deserialize,Serialize};
use serde_json::{Value,json};
const PROFILE:&str="demo-channel";

#[derive(Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cover {frame_ms:u64,thumb_offset_ms:u64,trim_start_ms:u64,trim_end_ms:u64}
#[derive(Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
struct Overrides {youtube:Option<String>,instagram:Option<String>,tiktok:Option<String>}
#[derive(Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlatformDraft {
    media_sha256:String,title:String,caption:String,tags:String,overrides:Overrides,
    shorts:bool,trial:bool,cover:Option<Cover>,
    #[serde(default)] share_to_feed:bool,
    #[serde(default)] youtube_api_tags:String,
}
fn valid(d:&PlatformDraft)->bool {
    let bounded=|s:&str,max:usize|s.len()<=max&&!s.contains(char::from(0));
    media::hex(&d.media_sha256)&&bounded(&d.title,400)&&bounded(&d.caption,22000)&&bounded(&d.tags,1600)&&bounded(&d.youtube_api_tags,3000)&&youtube_tag_count(&d.youtube_api_tags)<=500&&
    [&d.overrides.youtube,&d.overrides.instagram,&d.overrides.tiktok].into_iter().flatten().all(|s|bounded(s,22000))
}
fn youtube_tag_count(raw:&str)->usize{
    let tags=raw.split(',').map(str::trim).filter(|s|!s.is_empty()).collect::<Vec<_>>();
    tags.len().saturating_sub(1)+tags.iter().map(|tag|tag.chars().count()+if tag.contains(' '){2}else{0}).sum::<usize>()
}
fn active_scope(app:&App)->bool{
    app.media.parent().and_then(std::path::Path::file_name).and_then(|x|x.to_str())==Some("snapmatic-review-runtime")
}
fn cover_allowed(c:&Cover,media_hash:&str)->bool {
    let Ok(facts)=serde_json::from_str::<Value>(include_str!("../web/workspace-media-facts.json")) else{return false;};
    cover_allowed_with_facts(c,media_hash,&facts)
}
fn cover_allowed_with_facts(c:&Cover,media_hash:&str,facts:&Value)->bool{
    let Some(duration)=facts[media_hash]["format"]["duration"].as_str().and_then(|v|v.parse::<f64>().ok())
        .filter(|v|v.is_finite()&&*v>0.0) else{return false;};
    let limit=(duration*1000.0).ceil() as u64;
    c.trim_end_ms<=limit&&c.trim_start_ms<c.trim_end_ms&&c.frame_ms<c.trim_end_ms&&
    c.frame_ms.checked_sub(c.trim_start_ms)==Some(c.thumb_offset_ms)
}
fn access(app:&App,id:&str,mut draft:Option<PlatformDraft>)->Result<Option<PlatformDraft>,StatusCode>{
    let unavailable=StatusCode::SERVICE_UNAVAILABLE;let missing=StatusCode::NOT_FOUND;
    let in_film_roll=media::hex(id)&&crate::workspace_film_roll::exists(app,id);
    if !in_film_roll && !active_scope(app){return Err(missing);}
    let original=media::hex(id);
    let source=if in_film_roll {
        Some(id.to_owned())
    } else if original {
        Some(media::index(app).map_err(|_|unavailable)?.into_iter().find(|r|
            r["clip_id"]==id&&r["state"]=="pending_review"&&r["preview_available"]==true)
            .and_then(|r|r["preview_sha256"].as_str().filter(|s|media::hex(s)).map(str::to_owned)).ok_or(missing)?)
    } else if uuid::Uuid::parse_str(id).is_ok_and(|v|v.to_string()==id){None}else{return Err(missing);};
    let db=app.db.lock().map_err(|_|unavailable)?;
    let hash=if let Some(s)=source{s}else{
        let j=store::job(&db,id).map_err(|_|missing)?;
        if !matches!(j.state.as_str(),"READY_FOR_REVIEW"|"POSTED"|"BACKED_UP")||
            !crate::controller_source::controller_current(&db,j.source_id,&j.source_binding){return Err(missing);}
        j.preview_sha256.filter(|v|media::hex(v)).ok_or(missing)?
    };
    if let Some(d)=draft.take(){
        let cover_ok=match &d.cover {
            None=>true,
            Some(c) if in_film_roll=>c.trim_start_ms<c.trim_end_ms&&c.trim_end_ms<=86_400_000&&c.frame_ms<=c.trim_end_ms&&c.frame_ms.checked_sub(c.trim_start_ms)==Some(c.thumb_offset_ms),
            Some(c)=>cover_allowed(c,&hash),
        };
        if !valid(&d)||d.media_sha256!=hash||!cover_ok{
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        let raw=serde_json::to_string(&d).map_err(|_|unavailable)?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS snapmatic_platform_drafts_v1(
            profile_id TEXT NOT NULL,clip_id TEXT NOT NULL,media_sha256 TEXT NOT NULL,draft_json TEXT NOT NULL,
            PRIMARY KEY(profile_id,clip_id,media_sha256));").map_err(|_|unavailable)?;
        db.execute("INSERT INTO snapmatic_platform_drafts_v1 VALUES (?1,?2,?3,?4)
            ON CONFLICT(profile_id,clip_id,media_sha256) DO UPDATE SET draft_json=excluded.draft_json",
            rusqlite::params![PROFILE,id,hash,raw]).map_err(|_|unavailable)?;
        return Ok(Some(d));
    }
    if !db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='snapmatic_platform_drafts_v1')",
        [],|r|r.get::<_,bool>(0)).map_err(|_|unavailable)?{return Ok(None);}
    let raw=db.query_row("SELECT draft_json FROM snapmatic_platform_drafts_v1 WHERE profile_id=?1 AND clip_id=?2 AND media_sha256=?3",
        rusqlite::params![PROFILE,id,hash],|r|r.get::<_,String>(0)).optional().map_err(|_|unavailable)?;
    raw.map(|s|serde_json::from_str(&s).map_err(|_|unavailable)).transpose()
}
async fn run(app:App,id:String,draft:Option<PlatformDraft>)->Response{
    match tokio::task::spawn_blocking(move||access(&app,&id,draft)).await {
        Ok(Ok(d))=>([(axum::http::header::CACHE_CONTROL,"no-store")],Json(json!({"draft":d,"publishing_enabled":false}))).into_response(),
        Ok(Err(s))=>(s,Json(json!({"reason":"Local draft or exact preview is unavailable"}))).into_response(),
        _=>(StatusCode::SERVICE_UNAVAILABLE,Json(json!({"reason":"Local draft storage is unavailable"}))).into_response(),
    }
}
pub(crate) async fn read(State(app):State<App>,Path(id):Path<String>)->Response{run(app,id,None).await}
pub(crate) async fn save(State(app):State<App>,Path(id):Path<String>,h:HeaderMap,Json(d):Json<PlatformDraft>)->Response{
    if !h.get("origin").and_then(|v|v.to_str().ok()).is_some_and(|o|o=="http://127.0.0.1"||o.starts_with("http://127.0.0.1:")){
        return (StatusCode::FORBIDDEN,Json(json!({"reason":"local_origin_required"}))).into_response();
    }
    run(app,id,Some(d)).await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn cover_cannot_escape_verified_clip_or_trim(){
        let fixture_hash="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let facts=serde_json::json!({(fixture_hash):{"format":{"duration":"8.0"}}});
        let cover_allowed=|c:&Cover, h:&str| cover_allowed_with_facts(c,h,&facts);
        let hash=fixture_hash;
        let mut c=Cover{frame_ms:3456,thumb_offset_ms:2456,trim_start_ms:1000,trim_end_ms:7000};
        assert!(cover_allowed(&c,hash));
        assert!(!cover_allowed(&c,&"0".repeat(64)));
        c.thumb_offset_ms=3456;assert!(!cover_allowed(&c,hash));
        c.thumb_offset_ms=2456;c.trim_end_ms=3456;assert!(!cover_allowed(&c,hash));
        c.trim_end_ms=86400000;assert!(!cover_allowed(&c,hash));
    }
}
