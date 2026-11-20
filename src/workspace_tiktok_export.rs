use axum::{extract::{Path,State},http::{HeaderMap,StatusCode},response::{IntoResponse,Response},Json};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest,Sha256};
use std::{io::Write,path::{Path as FsPath,PathBuf}};
use crate::{App,controller_store as store};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Seal{id:String,sha256:String,width:u32,height:u32,duration_ms:u32,container:String,video:String,audio:String}
impl Seal {
 fn valid(&self,id:&str)->bool{
  uuid::Uuid::parse_str(id).is_ok_and(|x|x.to_string()==id)&&self.id==id
  &&id!="00000000-0000-4000-8000-000000000001"&&crate::workspace_media::hex(&self.sha256)
  &&self.width==1080&&self.height==1920&&(3000..=600000).contains(&self.duration_ms)
  &&self.container=="mp4"&&self.video=="h264"&&self.audio=="aac"
 }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Export{sha256:String,caption:String,cover_png_hex:Option<String>}
fn local(h:&HeaderMap)->bool{h.get("origin").and_then(|x|x.to_str().ok())==Some("http://127.0.0.1:3847")}
fn failure(code:StatusCode,reason:&'static str)->Response{(code,Json(json!({"reason":reason,"publishing_enabled":false}))).into_response()}
fn seal(id:&str)->Option<Seal>{
 // Separately approved exact render facts, independently verified before operator pinning.
 // No HTTP request, draft, triage, or preview authority can supply this seal.
 let s:Seal=serde_json::from_str(&std::env::var("SNAPMATIC_TIKTOK_LOCAL_EXPORT_SEAL").ok()?).ok()?;
 s.valid(id).then_some(s)
}
fn verified(app:&App,s:&Seal)->Option<Vec<u8>>{
 let db=app.db.lock().ok()?;let j=store::job(&db,&s.id).ok()?;
 if j.state!="READY_FOR_REVIEW"||j.review!="APPROVED"||j.preview_sha256.as_deref()!=Some(&s.sha256)
  ||!crate::controller_source::controller_current(&db,j.source_id,&j.source_binding){return None;}
 store::preview(&db,&s.id).ok()
}
fn folder()->Option<PathBuf>{
 let base=PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
 crate::controller_native_launcher::clean_path(&base).ok()?;
 let dir=base.join("Snapmatic-TikTok-Exports");
 if !dir.exists(){std::fs::create_dir(&dir).ok()?;}
 crate::controller_native_launcher::clean_path(&dir).ok()?;
 dir.is_dir().then_some(dir)
}
fn png(raw:&str)->Option<Vec<u8>>{
 if raw.len()>4*1024*1024||raw.len()%2!=0||!raw.bytes().all(|b|b.is_ascii_hexdigit()){return None;}
 let b:Vec<_>=(0..raw.len()).step_by(2).map(|i|u8::from_str_radix(&raw[i..i+2],16)).collect::<Result<_,_>>().ok()?;
 (b.len()>=45&&b.starts_with(b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR")&&b[16..24]==[0,0,4,56,0,0,7,128]&&b.ends_with(b"\0\0\0\0IEND\xaeB\x60\x82")).then_some(b)
}
fn sha(b:&[u8])->String{format!("{:x}",Sha256::digest(b))}
fn create_bundle(s:&Seal,v:&[u8],e:&Export,root:&FsPath,leaf:&str)->Option<()>{
 if !s.valid(&s.id)||s.sha256!=e.sha256||sha(v)!=s.sha256||v.len()>180*1024*1024||e.caption.len()>16000||e.caption.contains('\0'){return None;}
 let cover=png(e.cover_png_hex.as_deref()?)?;
 let m=serde_json::to_vec_pretty(&json!({"schema":"snapmatic-tiktok-manual-export-v1","manual_only":true,"uploaded":false,
  "media":{"file":"video.mp4","sha256":s.sha256,"bytes":v.len(),"width":s.width,"height":s.height,"duration_ms":s.duration_ms,"video":"h264","audio":"aac"},
  "caption":{"file":"caption_and_hashtags.txt","sha256":sha(e.caption.as_bytes())},
  "cover":{"file":"cover.png","sha256":sha(&cover),"width":1080,"height":1920}})).ok()?;
 let dir=root.join(leaf);
 std::fs::create_dir(&dir).ok()?;
 crate::controller_native_launcher::clean_path(&dir).ok()?;
 for (name,bytes) in [("video.mp4",v),("caption_and_hashtags.txt",e.caption.as_bytes()),("cover.png",&cover),("manifest.json",&m)]{
  let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(dir.join(name)).ok()?;
  f.write_all(bytes).ok()?;f.sync_all().ok()?;
 }
 Some(())
}
pub(crate) async fn status(State(app):State<App>,Path(id):Path<String>)->Response{
 let h=tokio::task::spawn_blocking(move||{let s=seal(&id)?;verified(&app,&s)?;Some(s.sha256)}).await.ok().flatten();
 ([(axum::http::header::CACHE_CONTROL,"no-store")],Json(json!({"mode":"manual","ready":h.is_some(),"sha256":h,"publishing_enabled":false}))).into_response()
}
pub(crate) async fn export(State(app):State<App>,Path(id):Path<String>,h:HeaderMap,Json(e):Json<Export>)->Response{
 if !local(&h){return failure(StatusCode::FORBIDDEN,"local_origin_required");}
 if seal(&id).is_none(){return failure(StatusCode::FORBIDDEN,"manual_export_requires_separate_review");}
 let r=tokio::task::spawn_blocking(move||{
  let s=seal(&id)?;let v=verified(&app,&s)?;let dir=folder()?;
  let leaf=format!("snapmatic-tiktok-{}",uuid::Uuid::new_v4());
  create_bundle(&s,&v,&e,&dir,&leaf)?;Some(leaf)
 }).await.ok().flatten();
 match r{Some(file)=>Json(json!({"file":file,"uploaded":false,"publishing_enabled":false})).into_response(),None=>failure(StatusCode::CONFLICT,"local_export_not_verified")}
}
pub(crate) async fn open_folder(h:HeaderMap)->Response{
 if !local(&h){return failure(StatusCode::FORBIDDEN,"local_origin_required");}
 #[cfg(windows)]
 if let Some(dir)=folder(){
  use std::os::windows::process::CommandExt;
  let p=dir.to_string_lossy();
  if !p.contains(['"','\r','\n'])&&std::process::Command::new(r"C:\Windows\explorer.exe").raw_arg(format!("\"{p}\"")).spawn().is_ok(){
   return Json(json!({"opened":true})).into_response();
  }
 }
 failure(StatusCode::SERVICE_UNAVAILABLE,"export_folder_unavailable")
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test] fn held_and_unverified_artifacts_are_rejected(){
  let hold="00000000-0000-4000-8000-000000000001";
  let mut s=Seal{id:hold.into(),sha256:sha(b"fixture"),width:1080,height:1920,duration_ms:8000,container:"mp4".into(),video:"h264".into(),audio:"aac".into()};
  assert!(!s.valid(hold));s.id=uuid::Uuid::new_v4().to_string();assert!(s.valid(&s.id));
  assert!(png("00").is_none());
  s.video="av1".into();assert!(!s.valid(&s.id));
 }
 #[test] fn compliant_manual_bundle_fixture(){
  let Ok(root)=std::env::var("SNAPMATIC_WORKFLOW_TEST_FIXTURE") else{return};
  let root=PathBuf::from(root);let video=std::fs::read(root.join("fixture.mp4")).unwrap();
  let cover=std::fs::read(root.join("fixture.png")).unwrap();
  let s=Seal{id:uuid::Uuid::new_v4().to_string(),sha256:sha(&video),width:1080,height:1920,duration_ms:3200,container:"mp4".into(),video:"h264".into(),audio:"aac".into()};
  let e=Export{sha256:s.sha256.clone(),caption:"Fixture GTA #GTA5".into(),cover_png_hex:Some(cover.iter().map(|b|format!("{b:02x}")).collect())};
  let leaf=uuid::Uuid::new_v4().to_string();
  assert!(create_bundle(&s,&video,&e,&root,&leaf).is_some());
  assert!(create_bundle(&s,&video,&e,&root,&leaf).is_none());
  let dir=root.join(leaf);
  assert_eq!(sha(&std::fs::read(dir.join("video.mp4")).unwrap()),s.sha256);
  assert_eq!(std::fs::read(dir.join("cover.png")).unwrap(),cover);
  assert_eq!(std::fs::read_to_string(dir.join("caption_and_hashtags.txt")).unwrap(),e.caption);
  let m:serde_json::Value=serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
  assert_eq!(m["uploaded"],false);
  println!("Fixture-only exact media, caption, cover and manifest verified.");
 }
}