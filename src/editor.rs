use crate::{ApiResult,App,bad,internal,source};
use axum::{Json,extract::{Multipart,State},http::StatusCode};
use rusqlite::params;
use serde::Deserialize;
use std::{path::PathBuf,process::Command};
use uuid::Uuid;

#[derive(Deserialize)]
struct Input { profile_id:i64, role:String, path:String, crop:String, start:f64, end:Option<f64>, mute:bool }
pub async fn render(State(app):State<App>,mut form:Multipart)->ApiResult<Json<serde_json::Value>> {
 let mut input=None;let mut overlay=Vec::new();
 while let Some(field)=form.next_field().await.map_err(bad)? { match field.name().unwrap_or("") { "options"=>{let text=field.text().await.map_err(bad)?;input=Some(serde_json::from_str::<Input>(&text).map_err(bad)?);},"overlay"=>overlay=field.bytes().await.map_err(bad)?.to_vec(),_=>{} } }
 let input=input.ok_or_else(||bad("Missing editor options."))?;
 if !["source","destination"].contains(&input.role.as_str()) || !["original","9:16","1:1","16:9"].contains(&input.crop.as_str()) || !input.start.is_finite() || input.start<0.0 || input.start>604800.0 || input.end.is_some_and(|end|!end.is_finite()||end<=input.start||end>604800.0) { return Err(bad("Check the crop and trim values.")); }
 if overlay.len()>5*1024*1024 || (!overlay.is_empty() && !overlay.starts_with(b"\x89PNG\r\n\x1a\n") && !overlay.starts_with(b"\xff\xd8\xff")) { return Err(bad("Overlay must be a PNG or JPEG under 5 MB.")); }
 {
  let db=app.db.lock().map_err(internal)?;
  if source::drive_profile(&db,input.profile_id,&input.role) || source::authorized_review_profile(&db,input.profile_id,&input.role) {
   return Err(bad("This source is read-only. Use Make a clip with an independently reviewed cut."));
  }
 }
 let original=source::resolve_file(&app,input.profile_id,&input.path,&input.role)?;
 if original.components().any(|c|c.as_os_str()==".lifeinvader-edits") {return Err(bad("Open the original media to make another edit."));}
 let ext=original.extension().and_then(|s|s.to_str()).unwrap_or("").to_lowercase();let video=["mp4","webm","mov","mkv"].contains(&ext.as_str());
 if !video && (input.start>0.0 || input.end.is_some()) {return Err(bad("Trim is available for videos."));}
 let root={let db=app.db.lock().map_err(internal)?;source::checked_root(&source::config(&db).map_err(internal)?)?};
 let parent=original.parent().ok_or_else(||bad("Invalid media location."))?;let edit_dir=parent.join(".lifeinvader-edits");std::fs::create_dir_all(&edit_dir).map_err(internal)?;let edit_dir=edit_dir.canonicalize().map_err(internal)?;if !edit_dir.starts_with(parent)||!edit_dir.starts_with(&root){return Err(bad("Invalid edit folder."));}
 let id=Uuid::new_v4().to_string();let output=edit_dir.join(format!("{id}.{}",if video{"mp4"}else{"jpg"}));let relative=output.strip_prefix(&root).map_err(internal)?.to_string_lossy().replace('\\',"/");
 let overlay_path=if overlay.is_empty(){None}else{let dir=app.media.join("editor-temp");std::fs::create_dir_all(&dir).map_err(internal)?;let path=dir.join(format!("{id}.{}",if overlay.starts_with(b"\x89PNG"){"png"}else{"jpg"}));std::fs::write(&path,overlay).map_err(internal)?;Some(path)};
 let name=if cfg!(windows){"ffmpeg.exe"}else{"ffmpeg"};
 let ffmpeg=std::env::var_os("LIFEINVADER_FFMPEG").map(PathBuf::from).or_else(||std::env::current_exe().ok().and_then(|p|p.parent().map(|d|d.join("tools").join(name))).filter(|p|p.is_file())).or_else(||{let local=PathBuf::from("tools").join(name);if local.is_file(){Some(local)}else{None}}).or_else(||std::env::var_os("PATH").and_then(|value|std::env::split_paths(&value).map(|dir|dir.join(name)).find(|p|p.is_file()))).unwrap_or_default();
 if !ffmpeg.is_file(){if let Some(p)=overlay_path{let _=std::fs::remove_file(p);}return Err(bad("FFmpeg is missing. Put it in the app's tools folder."));}
 let crop=match input.crop.as_str(){"9:16"=>"crop='min(iw,ih*9/16)':'min(ih,iw*16/9)'", "1:1"=>"crop='min(iw,ih)':'min(iw,ih)'", "16:9"=>"crop='min(iw,ih*16/9)':'min(ih,iw*9/16)'",_=>"null"}.to_owned();
 let result_output=output.clone();let result_overlay=overlay_path.clone();let start=input.start;let end=input.end;let mute=input.mute;
 let output_result=tokio::task::spawn_blocking(move||{
  let mut cmd=Command::new(ffmpeg);#[cfg(windows)]{use std::os::windows::process::CommandExt;cmd.creation_flags(0x08000000);}
  cmd.args(["-hide_banner","-loglevel","error","-n"]);if video && start>0.0{cmd.args(["-ss",&start.to_string()]);}if !video {cmd.args(["-loop","1"]);}cmd.arg("-i").arg(&original);
  if let Some(ref logo)=result_overlay {cmd.args(["-loop","1","-i"]).arg(logo);let filter=format!("[0:v]{crop},setsar=1[base];[1:v][base]scale2ref=w=main_w*0.28:h=ow/mdar[scaled][base2];[scaled]format=rgba,colorchannelmixer=aa=0.5[logo];[base2][logo]overlay=(W-w)/2:H-h-20:shortest=1[v]");cmd.args(["-filter_complex",&filter,"-map","[v]"]);}else{cmd.args(["-vf",&crop,"-map","0:v:0"]);}
  if video{if let Some(end)=end{cmd.args(["-t",&(end-start).to_string()]);}if mute{cmd.arg("-an");}else{cmd.args(["-map","0:a?","-c:a","aac"]);}cmd.args(["-c:v","libx264","-preset","veryfast","-crf","23","-pix_fmt","yuv420p","-movflags","+faststart"]);}else{cmd.args(["-frames:v","1","-q:v","2","-pix_fmt","yuvj420p","-threads","1"]);}
  cmd.args(["-progress","pipe:1","-nostats"]);cmd.arg(&result_output).output()
 }).await.map_err(internal)?;
 if let Some(path)=overlay_path{let _=std::fs::remove_file(path);}let result=output_result.map_err(internal)?;
 let has_frames=String::from_utf8_lossy(&result.stdout).lines().filter_map(|line|line.strip_prefix("frame=")).filter_map(|x|x.trim().parse::<u64>().ok()).any(|n|n>0);
 if !result.status.success() || !has_frames {let _=std::fs::remove_file(&output);let msg=String::from_utf8_lossy(&result.stderr);return Err((StatusCode::BAD_REQUEST,format!("Could not render this edit: {}",msg.chars().take(800).collect::<String>())));}
 if !output.is_file() || output.metadata().map(|m|m.len()==0).unwrap_or(true){return Err(bad("The edit produced no media."));}
 let db=app.db.lock().map_err(internal)?;db.execute("INSERT INTO media_edits(id,profile_id,role,root,original_path,edited_path) VALUES(?,?,?,?,?,?)",params![id,input.profile_id,input.role,root.to_string_lossy().as_ref(),input.path,relative]).map_err(internal)?;
 Ok(Json(serde_json::json!({"id":id,"path":relative,"role":input.role,"profile_id":input.profile_id})))
}
