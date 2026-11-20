//! Workspace annotations only. No controller eligibility or dispatch.
use crate::{App,workspace_media as media,controller_store as store};
use axum::{Json,extract::{Query,State},http::{HeaderMap,StatusCode,header},response::{IntoResponse,Response}};
use rusqlite::{Connection,OptionalExtension};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::collections::{BTreeMap,BTreeSet};
const PROFILE:&str="demo-channel";
const TABLE:&str="snapmatic_triage_v1";
const SCOPE:&str="snapmatic-review-runtime";
#[derive(Clone,Copy,Serialize,Deserialize,PartialEq,Eq,Debug)]
enum Label{Approve,Transcode,Hold,Archive}
impl Label{
    fn text(self)->&'static str{match self{Self::Approve=>"Approve",Self::Transcode=>"Transcode",Self::Hold=>"Hold",Self::Archive=>"Archive"}}
    fn from(s:&str)->Option<Self>{match s{"Approve"=>Some(Self::Approve),"Transcode"=>Some(Self::Transcode),"Hold"=>Some(Self::Hold),"Archive"=>Some(Self::Archive),_=>None}}
}
#[derive(Clone,Serialize,Deserialize,Debug)]
#[serde(deny_unknown_fields)]
struct Annotation{clip_id:String,media_sha256:String,label:Label}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProfileQuery{profile_id:String}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request{profile_id:String,annotations:Vec<Annotation>}
type Bound=BTreeMap<String,String>;
fn validate(a:&[Annotation],known:&Bound)->bool{
    if !(1..=32).contains(&a.len()){return false;}
    let mut seen=BTreeSet::new();
    a.iter().all(|x|media::hex(&x.clip_id)&&media::hex(&x.media_sha256)&&known.get(&x.clip_id)==Some(&x.media_sha256)&&seen.insert(&x.clip_id))
}
fn table_exists(db:&Connection)->store::Result<bool>{
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",[TABLE],|r|r.get(0))?)
}
fn put(db:&mut Connection,profile:&str,a:&[Annotation],known:&Bound)->store::Result<()>{
    if !validate(a,known){return Err(store::Error("triage_clip_binding"));}
    let tx=db.transaction()?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS snapmatic_triage_v1(
      profile_id TEXT NOT NULL,clip_id TEXT NOT NULL,media_sha256 TEXT NOT NULL,label TEXT NOT NULL
      CHECK(label IN ('Approve','Transcode','Hold','Archive')), PRIMARY KEY(profile_id,clip_id,media_sha256));")?;
    for row in a{
        tx.execute("INSERT INTO snapmatic_triage_v1 VALUES(?1,?2,?3,?4)
          ON CONFLICT(profile_id,clip_id,media_sha256) DO UPDATE SET label=excluded.label
          WHERE snapmatic_triage_v1.label<>excluded.label",
          rusqlite::params![profile,row.clip_id,row.media_sha256,row.label.text()])?;
    }
    tx.commit()?;
    Ok(())
}
fn project(db:&Connection,profile:&str,known:&Bound)->store::Result<Vec<Annotation>>{
    if !table_exists(db)?{return Ok(Vec::new());}
    let mut q=db.prepare("SELECT label FROM snapmatic_triage_v1 WHERE profile_id=?1 AND clip_id=?2 AND media_sha256=?3")?;
    let mut out=Vec::new();
    for (id,hash) in known {
        if let Some(label)=q.query_row(rusqlite::params![profile,id,hash],|r|r.get::<_,String>(0)).optional()?{
            let label=Label::from(&label).ok_or(store::Error("triage_unknown_label"))?;
            out.push(Annotation{clip_id:id.clone(),media_sha256:hash.clone(),label});
        }
    }
    Ok(out)
}
fn scope(app:&App)->bool{
    app.media.parent().and_then(std::path::Path::file_name).and_then(|x|x.to_str())==Some(SCOPE)
}
fn fixture(app:&App)->bool{
    scope(app)&&app.media.parent().and_then(std::path::Path::parent).and_then(std::path::Path::file_name)
        .and_then(|x|x.to_str())==Some("snapmatic-batch-fixture-v1")
}
pub(crate) fn fixture_enabled(app:&App)->bool{fixture(app)}
fn bound(app:&App)->store::Result<Bound>{
    let mut m=Bound::new();
    let base=media::index(app)?;
    let intake=crate::workspace_phase4::rows(app)?;
    for item in base.into_iter().chain(intake){
        if item["state"]!="pending_review"||item["preview_available"]!=true {continue;}
        let id=item["clip_id"].as_str().filter(|s|media::hex(s)).ok_or(store::Error("triage_clip_binding"))?;
        let hash=item["preview_sha256"].as_str().filter(|s|media::hex(s)).ok_or(store::Error("triage_clip_binding"))?;
        if m.insert(id.into(),hash.into()).is_some_and(|old|old!=hash){return Err(store::Error("triage_clip_binding"));}
    }
    Ok(m)
}
fn run(app:&App,profile:&str,batch:Option<Vec<Annotation>>)->Result<Value,(StatusCode,&'static str)>{
    let generic=(StatusCode::SERVICE_UNAVAILABLE,"Local annotation storage is unavailable");
    let bad=(StatusCode::UNPROCESSABLE_ENTITY,"Each label must match a distinct current review clip and exact media revision");
    if !scope(app)||profile!=PROFILE{return Err((StatusCode::NOT_FOUND,"No configured local review profile"));}
    let known=bound(app).map_err(|_|generic)?;
    let mut db=app.db.lock().map_err(|_|generic)?;
    let exists=db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='snapmatic_channel_profiles_v1')",
        [],|r|r.get::<_,bool>(0)).map_err(|_|generic)?;
    let configured=exists&&db.query_row("SELECT EXISTS(SELECT 1 FROM snapmatic_channel_profiles_v1
        WHERE profile_id=?1 AND source_profile_id=2 AND drive_scope='everything-clips-v1' AND enabled=1)",
        [profile],|r|r.get::<_,bool>(0)).map_err(|_|generic)?;
    if !configured{return Err((StatusCode::NOT_FOUND,"No configured local review profile"));}
    if let Some(labels)=batch{
        if !validate(&labels,&known){return Err(bad);}
        put(&mut db,profile,&labels,&known).map_err(|_|generic)?;
    }
    let rows=project(&db,profile,&known).map_err(|_|generic)?;
    Ok(json!({"schema":"snapmatic-triage-annotations-v1","profile_id":profile,"annotations":rows,
        "changes_review_eligibility":false,"dispatch_enabled":false,"publishing_enabled":false}))
}
async fn response(app:App,profile:String,batch:Option<Vec<Annotation>>)->Response{
    match tokio::task::spawn_blocking(move||run(&app,&profile,batch)).await{
        Ok(Ok(v))=>([(header::CACHE_CONTROL,"no-store")],Json(v)).into_response(),
        Ok(Err((s,r)))=>(s,Json(json!({"reason":r}))).into_response(),
        _=>(StatusCode::SERVICE_UNAVAILABLE,Json(json!({"reason":"Local annotation storage is unavailable"}))).into_response()
    }
}
pub(crate) async fn read(State(app):State<App>,Query(q):Query<ProfileQuery>)->Response{
    response(app,q.profile_id,None).await
}
pub(crate) async fn write(State(app):State<App>,h:HeaderMap,Json(r):Json<Request>)->Response{
    let origin=h.get("origin").and_then(|v|v.to_str().ok());
    if origin!=Some("http://127.0.0.1:3847")&&!(fixture(&app)&&origin==Some("http://127.0.0.1:43850")){
        return (StatusCode::FORBIDDEN,Json(json!({"reason":"local_origin_required"}))).into_response();
    }
    response(app,r.profile_id,Some(r.annotations)).await
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn mixed_annotations_are_idempotent_revision_bound_and_never_partial(){
        let a="a".repeat(64);let b="b".repeat(64);let h="c".repeat(64);let g="d".repeat(64);
        let known=Bound::from([(a.clone(),h.clone()),(b.clone(),g.clone())]);
        let mut db=Connection::open_in_memory().unwrap();
        let x=vec![Annotation{clip_id:a.clone(),media_sha256:h.clone(),label:Label::Archive},
            Annotation{clip_id:b.clone(),media_sha256:g.clone(),label:Label::Transcode}];
        put(&mut db,PROFILE,&x,&known).unwrap();
        let changes=db.total_changes();put(&mut db,PROFILE,&x,&known).unwrap();assert_eq!(changes,db.total_changes());
        let mut bad=x.clone();bad[0].label=Label::Hold;bad[1].media_sha256=h.clone();
        assert!(put(&mut db,PROFILE,&bad,&known).is_err());assert_eq!(project(&db,PROFILE,&known).unwrap()[0].label,Label::Archive);
        assert!(project(&db,"other-profile",&known).unwrap().is_empty());
        let changed=Bound::from([(a,g)]);assert!(project(&db,PROFILE,&changed).unwrap().is_empty());
    }
}