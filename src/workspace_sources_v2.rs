//! Sources v2 eligibility is independent of any launch or publication.
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::collections::HashSet;

#[derive(Clone,Copy,Debug,Deserialize,Serialize,PartialEq,Eq,Hash)]
#[serde(rename_all="snake_case")]
pub(crate) enum Provider{TikTok,YouTubeShorts,InstagramReels}
impl Provider{
 pub(crate) fn atom(self)->&'static str{match self{Self::TikTok=>"tiktok",Self::YouTubeShorts=>"youtube",Self::InstagramReels=>"instagram"}}
}
#[derive(Clone,Copy,Debug,Deserialize,Serialize,PartialEq,Eq)]
#[serde(rename_all="snake_case")]
pub(crate) enum Duplicates{SkipArchived,ShowArchived}
#[derive(Clone,Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Filter{
 pub browse_latest:usize,pub maximum:usize,pub min_duration_ms:Option<u64>,pub max_duration_ms:Option<u64>,
 pub within_days:Option<u64>,pub central_from:Option<String>,pub central_through:Option<String>,pub min_views:Option<u64>,pub min_likes:Option<u64>,
 pub keyword:Option<String>,pub hashtag:Option<String>,pub duplicates:Duplicates,
}
impl Filter{
 fn valid(&self)->bool{
  (1..=50).contains(&self.browse_latest)&&(1..=10).contains(&self.maximum)&&self.maximum<=self.browse_latest
  &&self.min_duration_ms.is_none_or(|m|(1..=900000).contains(&m))
  &&self.max_duration_ms.is_none_or(|m|(1..=900000).contains(&m))
  &&self.min_duration_ms.zip(self.max_duration_ms).is_none_or(|(a,b)|a<=b)
  &&calendar_range(self).is_ok()
  &&self.within_days.is_none_or(|n|(1..=366).contains(&n))
  &&self.min_views.is_none_or(|n|n<=10_000_000_000)
  &&self.min_likes.is_none_or(|n|n<=10_000_000_000)
  &&self.keyword.as_ref().is_none_or(|s|term(s,100))
  &&self.hashtag.as_ref().is_none_or(|s|term(s.trim_start_matches('#'),80)&&s.trim_start_matches('#').chars().all(|c|c.is_alphanumeric()||c=='_'))
 }
}
fn central_midnight(s:&str)->Option<i64>{
 use chrono::{Datelike,Days,NaiveDate,Weekday};
 let date=NaiveDate::parse_from_str(s,"%Y-%m-%d").ok()?;
 if !(2007..=2099).contains(&date.year())||date.to_string()!=s{return None;}
 let sunday=|month,week:u32| -> Option<NaiveDate>{
  let first=NaiveDate::from_ymd_opt(date.year(),month,1)?;
  first.checked_add_days(Days::new(u64::from((7+Weekday::Sun.num_days_from_sunday()-first.weekday().num_days_from_sunday())%7+7*(week-1))))
 };
 // US Central midnight still precedes the local 2 a.m. DST transition.
 let daylight=date>sunday(3,2)?&&date<=sunday(11,1)?;
 Some(date.and_hms_opt(if daylight{5}else{6},0,0)?.and_utc().timestamp())
}
fn calendar_range(f:&Filter)->Result<Option<(i64,i64)>,()>{
 match (&f.central_from,&f.central_through){
  (None,None)=>Ok(None),
  (Some(from),Some(through))=>{
   use chrono::{Days,NaiveDate};
   let a=central_midnight(from).ok_or(())?;
   let inclusive=central_midnight(through).ok_or(())?;
   if inclusive<a||inclusive-a>366*86400{return Err(());}
   let next=NaiveDate::parse_from_str(through,"%Y-%m-%d").map_err(|_|())?.checked_add_days(Days::new(1)).ok_or(())?;
   Ok(Some((a,central_midnight(&next.to_string()).ok_or(())?)))
  },_=>Err(()),
 }
}
fn term(s:&str,cap:usize)->bool{
 !s.trim().is_empty()&&s.chars().count()<=cap&&s.chars().all(|c|!c.is_control())
}
fn token(s:&str,lo:usize,hi:usize,extra:&[u8])->bool{
 (lo..=hi).contains(&s.len())&&s.bytes().all(|c|c.is_ascii_alphanumeric()||extra.contains(&c))
}
pub(crate) fn post_id(provider:Provider,s:&str)->bool{
 match provider{
  Provider::TikTok=>(15..=22).contains(&s.len())&&s.bytes().all(|c|c.is_ascii_digit()),
  Provider::YouTubeShorts=>token(s,11,11,b"_-"),
  Provider::InstagramReels=>token(s,1,40,b"_-"),
 }
}
pub(crate) fn normalize_profile(raw:&str)->Option<(Provider,String)>{
 if let Some(handle)=raw.strip_prefix('@'){
  return (token(handle,1,24,b"_.")&&handle.bytes().any(|c|c.is_ascii_alphanumeric()||c==b'_')&&!handle.ends_with('.'))
    .then(||(Provider::TikTok,format!("https://www.tiktok.com/@{}",handle.to_ascii_lowercase())));
 }
 if raw.len()>256||!raw.is_ascii()||raw.bytes().any(|c|c<=32||b"%\\?#".contains(&c)){return None;}
 let u:axum::http::Uri=raw.parse().ok()?;
 if u.scheme_str()!=Some("https")||u.port_u16().is_some(){return None;}
 let host=u.host()?.to_ascii_lowercase();if u.authority()?.as_str().to_ascii_lowercase()!=host{return None;}let path=u.path().strip_suffix('/').unwrap_or(u.path());
 match host.as_str(){
  "tiktok.com"|"www.tiktok.com"=>{
   let handle=path.strip_prefix("/@")?;
   normalize_profile(&format!("@{handle}"))
  },
  "youtube.com"|"www.youtube.com"=>{
   let p=path.strip_suffix("/shorts").unwrap_or(path);
   let account=if let Some(h)=p.strip_prefix("/@"){
    if !token(h,3,30,b"_.-"){return None;} format!("@{}",h.to_ascii_lowercase())
   }else{
    let id=p.strip_prefix("/channel/UC")?;
    if !token(id,22,22,b"_-"){return None;}format!("channel/UC{id}")
   };
   Some((Provider::YouTubeShorts,format!("https://www.youtube.com/{account}/shorts")))
  },
  "instagram.com"|"www.instagram.com"=>{
   let path=path.strip_suffix("/reels").unwrap_or(path);
   let h=path.strip_prefix('/')?;
   let name=h.to_ascii_lowercase();
   if [".","..","p","reel","reels","accounts","explore","direct","stories","api"].contains(&name.as_str())||!token(h,1,30,b"_."){return None;}
   Some((Provider::InstagramReels,format!("https://www.instagram.com/{name}/reels/")))
  },_=>None
 }
}
#[derive(Clone,Copy,Debug,Deserialize,Serialize,PartialEq,Eq)]
#[serde(rename_all="snake_case")]
pub(crate) enum Access{PublicVerified,Unknown,Restricted}
#[derive(Clone,Debug,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Candidate{
 pub provider:Provider,pub source_identity:String,pub post_id:String,pub title:Option<String>,
 pub hashtags:Option<Vec<String>>,pub duration_ms:Option<u64>,pub published_at_unix:Option<i64>,
 pub views:Option<u64>,pub likes:Option<u64>,pub platform_type_verified:bool,pub public_access:Access,
}
impl Candidate{
 fn valid(&self)->bool{
  normalize_profile(&self.source_identity).as_ref()==Some(&(self.provider,self.source_identity.clone()))&&post_id(self.provider,&self.post_id)
   &&self.title.as_ref().is_none_or(|s|term(s,350))
   &&self.hashtags.as_ref().is_none_or(|v|v.len()<=60&&v.iter().all(|s|term(s,90)))
   &&self.duration_ms.is_none_or(|m|m>0&&m<=24*3600*1000)
 }
 fn archive_key(&self)->String{
  let mut hash=Sha256::new();
  for f in ["snapmatic-source-archive-v2",self.provider.atom(),&self.source_identity,&self.post_id]{hash.update(f.as_bytes());hash.update([0]);}
  format!("{:x}",hash.finalize())
 }
}
#[derive(Clone,Debug,Serialize,PartialEq,Eq)]
#[serde(rename_all="snake_case")]
pub(crate) enum State{Match,Excluded,Restricted,MetadataUnknown,AlreadyArchived,OverLimit}
#[derive(Clone,Debug,Serialize)]
pub(crate) struct Review{pub post_id:String,pub title:Option<String>,pub key:String,pub state:State,pub reasons:Vec<&'static str>}
fn check(valid:Option<bool>,label:&'static str,bad:&mut Vec<&'static str>,unknown:&mut Vec<&'static str>){
 match valid{Some(true)=>{},Some(false)=>bad.push(label),None=>unknown.push(label)}
}
pub(crate) fn review(provider:Provider,source:&str,items:&[Candidate],archive:&HashSet<String>,f:&Filter,now:i64)->Result<Vec<Review>,&'static str>{
 if !f.valid()||normalize_profile(source).as_ref()!=Some(&(provider,source.to_owned()))||now<86400||items.len()>50{return Err("invalid_sources_preview");}
 let mut seen=HashSet::new();
 for e in items{
  if !e.valid()||e.provider!=provider||e.source_identity!=source||!seen.insert(&e.post_id){return Err("candidate_source_mismatch");}
 }
 // Unknown dates are never assumed recent, and ties remain deterministic.
 let mut recent=items.iter().collect::<Vec<_>>();
 recent.sort_by(|a,b|b.published_at_unix.cmp(&a.published_at_unix).then(a.post_id.cmp(&b.post_id)));
 let mut available=0;
 let mut out=Vec::new();
 for a in recent.into_iter().take(f.browse_latest){
  let key=a.archive_key();
  let archived=archive.contains(&key);
  if archived&&f.duplicates==Duplicates::SkipArchived{continue;}
  let mut bad=vec![];let mut unknown=vec![];
  if !a.platform_type_verified{unknown.push("platform_type");}
  if a.public_access==Access::Unknown{unknown.push("public_access");}
  if let Some(v)=f.min_duration_ms{check(a.duration_ms.map(|n|n>=v),"minimum_duration",&mut bad,&mut unknown);}
  if let Some(v)=f.max_duration_ms{check(a.duration_ms.map(|n|n<=v),"maximum_duration",&mut bad,&mut unknown);}
  if let Some(days)=f.within_days{check(a.published_at_unix.filter(|t|*t>0&&*t<=now).map(|t|t>=now-i64::try_from(days*86400).unwrap_or(i64::MAX)),"posted_within",&mut bad,&mut unknown);}
  if let Ok(Some((start,end)))=calendar_range(f){ check(a.published_at_unix.filter(|t|*t>0&&*t<=now).map(|t|t>=start&&t<end),"posted_central_range",&mut bad,&mut unknown); }
  if let Some(v)=f.min_views{check(a.views.map(|n|n>=v),"minimum_views",&mut bad,&mut unknown);}
  if let Some(v)=f.min_likes{check(a.likes.map(|n|n>=v),"minimum_likes",&mut bad,&mut unknown);}
  if let Some(v)=f.keyword.as_ref(){check(a.title.as_ref().map(|s|s.to_lowercase().contains(&v.trim().to_lowercase())),"keyword",&mut bad,&mut unknown);}
  if let Some(v)=f.hashtag.as_ref(){let want=v.trim_start_matches('#').to_lowercase();
   check(a.hashtags.as_ref().map(|list|list.iter().any(|s|s.trim_start_matches('#').to_lowercase()==want)),"hashtag",&mut bad,&mut unknown);}
  let (state,reasons)=if a.public_access==Access::Restricted{(State::Restricted,vec!["provider_access_restricted"])}else if archived{(State::AlreadyArchived,vec!["archive_match"])}
   else if !bad.is_empty(){(State::Excluded,bad)}
   else if !unknown.is_empty(){(State::MetadataUnknown,unknown)}
   else if available>=f.maximum{(State::OverLimit,vec!["confirmation_batch_limit"])}
   else{available+=1;(State::Match,vec![])};
  out.push(Review{post_id:a.post_id.clone(),title:a.title.clone(),key,state,reasons});
 }
 Ok(out)
}
#[cfg(test)]
mod tests{
 use super::*;
 fn source(p:Provider)->String{match p{Provider::TikTok=>"https://www.tiktok.com/@account",Provider::YouTubeShorts=>"https://www.youtube.com/@account/shorts",Provider::InstagramReels=>"https://www.instagram.com/account/reels/"}.into()}
 fn item(p:Provider,id:&str)->Candidate{ let post_id=match p{Provider::TikTok=>format!("10000000000000{:02}",id.as_bytes()[0]-b'a'),Provider::YouTubeShorts=>format!("a_________{}",id.as_bytes()[0] as char),Provider::InstagramReels=>id.into()};Candidate{provider:p,source_identity:source(p),post_id,title:Some("A Funny Clip".into()),hashtags:Some(vec!["#GTA5".into()]),duration_ms:Some(8000),published_at_unix:Some(1_795_001_000),views:Some(900),likes:Some(50),platform_type_verified:true,public_access:Access::PublicVerified} }
 fn filter()->Filter{Filter{browse_latest:10,maximum:1,min_duration_ms:Some(6000),max_duration_ms:Some(10000),within_days:Some(5),central_from:None,central_through:None,min_views:Some(100),min_likes:Some(5),keyword:Some("fUnNy".into()),hashtag:Some("#gta5".into()),duplicates:Duplicates::ShowArchived}}
 #[test] fn provider_parity_missing_and_archive_are_fail_closed(){
  for p in [Provider::TikTok,Provider::YouTubeShorts,Provider::InstagramReels]{
   let a=item(p,"a");let mut b=item(p,"b");let mut c=item(p,"c");let d=item(p,"d");
   b.views=None;c.duration_ms=Some(2000);
   let archive=HashSet::from([d.archive_key()]);
   let r=review(p,&source(p),&[a,b,c,d],&archive,&filter(),1_795_001_050).unwrap();
   assert_eq!(r.iter().map(|x|x.state.clone()).collect::<Vec<_>>(),[State::Match,State::MetadataUnknown,State::Excluded,State::AlreadyArchived]);
   assert_eq!(r[1].reasons,["minimum_views"]);
  }
  assert_ne!(item(Provider::TikTok,"id").archive_key(),item(Provider::InstagramReels,"id").archive_key());
  let mut b=item(Provider::TikTok,"id");b.source_identity="https://www.tiktok.com/@another".into();
  assert_ne!(b.archive_key(),item(Provider::TikTok,"id").archive_key());
  assert!(review(Provider::TikTok,&source(Provider::TikTok),&[b],&HashSet::new(),&filter(),1_795_001_050).is_err());
 }
 #[test] fn central_inclusive_across_dst_and_strict_handle(){
  let mut f=filter();f.central_from=Some("2026-11-18".into());f.central_through=Some("2026-11-20".into());
  let (a,b)=calendar_range(&f).unwrap().unwrap();
  let utc=|s| chrono::DateTime::parse_from_rfc3339(s).unwrap().timestamp();
  assert_eq!((a,b),(utc("2026-11-18T06:00:00Z"),utc("2026-11-21T06:00:00Z")));
  assert_eq!(central_midnight("2026-03-08"),Some(utc("2026-03-08T06:00:00Z")));
  assert_eq!(central_midnight("2026-03-09"),Some(utc("2026-03-09T05:00:00Z")));
  assert_eq!(central_midnight("2026-11-01"),Some(utc("2026-11-01T05:00:00Z")));
  assert_eq!(central_midnight("2026-11-02"),Some(utc("2026-11-02T06:00:00Z")));
  f.central_through=Some("2026-11-17".into());assert!(calendar_range(&f).is_err());
  assert_eq!(normalize_profile("@Demo_Creator_One"),Some((Provider::TikTok,"https://www.tiktok.com/@demo_creator_one".into())));
  for bad in ["@@x","@x.","@x/","https://www.tiktok.com:443/@x","https://www.tiktok.com/@x/video/1","https://www.tiktok.com/@x?k=v"]{assert!(normalize_profile(bad).is_none(),"{bad}");}
 }
 #[test] fn capped_confirmation_never_converts_unknown_to_match(){
  let p=Provider::YouTubeShorts;let a=item(p,"a");let b=item(p,"b");let mut c=item(p,"c");c.platform_type_verified=false;
  let r=review(p,&source(p),&[a,b,c],&HashSet::new(),&filter(),1_795_001_050).unwrap();
  assert_eq!(r.iter().map(|s|s.state.clone()).collect::<Vec<_>>(),[State::Match,State::OverLimit,State::MetadataUnknown]);
 }
}


use crate::App;
use axum::{extract::{Path,State as AxumState},http::{HeaderMap,StatusCode,header},response::{IntoResponse,Response},Json};
use serde_json::json;

const STOCK_WHEEL:&str="a05c5b5d8562f7e8a62f5334348b406af4930d56d3a65f8021d14af0e22339ae";
const STOCK_CHECK:&str="e14175de1b335b34d955a1d8a00cab955269258fbfaafadcd1ab418acb55d547";
const STOCK_HOLD:&str="stock_extractor_requests_impersonation";
fn stock_result(v:&serde_json::Value)->bool{
 v.as_object().is_some_and(|m|m.len()==4)&&v["schema"]=="tiktok-stock-preflight-v1"
 &&v["state"]=="HOLD"&&v["reason"]==STOCK_HOLD&&v.get("argv").is_some_and(|a|a.is_null())
}
#[cfg(windows)]
fn stock_diagnostic(app:&App)->Option<&'static str>{
 use std::io::{Read,Seek,SeekFrom};
 let pin=std::env::var("LIFEINVADER_TIKTOK_STOCK_EVIDENCE_SHA256").ok()?;
 if !crate::workspace_media::hex(&pin){return None;}
 let dir=app.media.parent()?.join("stock-tiktok-proof-v1");
 let packet=dir.join("proof.json");
 for (p,n) in [(&packet,8192),( &dir.join("yt_dlp-2026.10.26-py3-none-any.whl"),16*1024*1024),(&dir.join("tiktok_stock_preflight_v1.py"),16384)] {
  crate::controller_native_launcher::clean_path(p).ok()?;
  let m=std::fs::symlink_metadata(p).ok()?;
  if !m.is_file()||m.len()==0||m.len()>n{return None;}
 }
 let mut f=crate::controller_native_process::lease(&packet,&pin).ok()?;
 f.seek(SeekFrom::Start(0)).ok()?;
 let mut raw=Vec::new();f.take(8193).read_to_end(&mut raw).ok()?;
 if raw.len()>8192{return None;}
 let v:serde_json::Value=serde_json::from_slice(&raw).ok()?;
 if !v.as_object().is_some_and(|m|m.len()==5)||v["schema"]!="snapmatic-stock-download-evidence-v1"
  ||v["wheel_sha256"]!=STOCK_WHEEL||v["preflight_sha256"]!=STOCK_CHECK||!stock_result(&v["scan"])||!stock_result(&v["download"]){return None;}
 let _wheel=crate::controller_native_process::lease(&dir.join("yt_dlp-2026.10.26-py3-none-any.whl"),STOCK_WHEEL).ok()?;
 let _check=crate::controller_native_process::lease(&dir.join("tiktok_stock_preflight_v1.py"),STOCK_CHECK).ok()?;
 Some(STOCK_HOLD)
}
#[cfg(not(windows))]
fn stock_diagnostic(_: &App)->Option<&'static str>{None}
async fn diagnostic(app:App)->Option<&'static str>{
 tokio::task::spawn_blocking(move||stock_diagnostic(&app)).await.ok().flatten()
}
#[test]
fn stock_hold_proof_cannot_carry_a_command(){
 let mut v=json!({"schema":"tiktok-stock-preflight-v1","state":"HOLD","reason":STOCK_HOLD,"argv":null});
 assert!(stock_result(&v));
 v["argv"]=json!(["anything"]);assert!(!stock_result(&v));
 v["argv"]=json!(null);v["extra"]=json!(false);assert!(!stock_result(&v));
}
pub(crate) async fn catalogue(AxumState(app):AxumState<App>)->Response{
 let read=||->rusqlite::Result<Vec<serde_json::Value>>{
  let db=app.db.lock().map_err(|_|rusqlite::Error::InvalidQuery)?;
  let mut q=db.prepare("SELECT id,name,platform FROM profiles WHERE role='source' AND archived=0 AND platform='tiktok' ORDER BY id LIMIT 100")?;
  let rows=q.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"name":r.get::<_,String>(1)?,"provider":match r.get::<_,String>(2)?.as_str(){
      "youtube"=>"youtube_shorts","instagram"=>"instagram_reels",_=>"tiktok"}})))?;
  rows.collect()
 };
 let sources=read();
 let reason=diagnostic(app).await;
 match sources{
  Ok(s)=>([(header::CACHE_CONTROL,"no-store")],Json(json!({"schema":"snapmatic-sources-v2","sources":s,
    "discovery_state":if reason.is_some(){"held_stock_preflight"}else{"held_adapter_unavailable"},"reason":reason,"download_state":"held_permission_transport_unverified","requires_exact_provider_permission":true,"publishing_enabled":false}))).into_response(),
  Err(_)=>(StatusCode::SERVICE_UNAVAILABLE,Json(json!({"reason":"sources_unavailable"}))).into_response()
 }
}
pub(crate) async fn preview(AxumState(app):AxumState<App>,Path(id):Path<i64>,h:HeaderMap,Json(filter):Json<Filter>)->Response{
 if h.get("origin").and_then(|o|o.to_str().ok())!=Some("http://127.0.0.1:3847"){
  return (StatusCode::FORBIDDEN,Json(json!({"reason":"explicit_local_preview_required"}))).into_response();
 }
 let scope = app.db.lock().ok().and_then(|d|d.query_row(
     "SELECT EXISTS(SELECT 1 FROM profiles WHERE id=?1 AND role='source' AND archived=0 AND platform='tiktok')",
     [id],|r|r.get::<_,bool>(0)).ok()).unwrap_or(false);
 if !scope{return (StatusCode::FORBIDDEN,Json(json!({"reason":"tiktok_source_only"}))).into_response();}
 if !filter.valid(){return (StatusCode::UNPROCESSABLE_ENTITY,Json(json!({"reason":"invalid_sources_preview"}))).into_response();}
 let reason=diagnostic(app).await.unwrap_or("sealed_discovery_adapter_unavailable");
 (StatusCode::SERVICE_UNAVAILABLE,Json(json!({"reason":reason,"download_enabled":false,"publishing_enabled":false}))).into_response()
}
