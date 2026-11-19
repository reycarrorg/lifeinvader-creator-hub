use std::{env,fs,path::PathBuf,io::Write};
fn main(){
 let a:Vec<String>=env::args().collect();
 if a.iter().any(|x|x=="--flat-playlist"){
  for (id,date) in [("300","20261003"),("200","20261002"),("100","20261001")] { println!("{{\"id\":\"{id}\",\"title\":\"post {id}\",\"upload_date\":\"{date}\",\"thumbnail\":\"https://example.com/{id}.jpg\"}}"); }
  return
 }
 let pos=|s:&str|a.iter().position(|x|x==s).unwrap();
 let archive=PathBuf::from(&a[pos("--download-archive")+1]);let template=PathBuf::from(&a[pos("--output")+1]);let mut output=template.parent().unwrap().to_path_buf(); if output.to_string_lossy().contains("%(uploader") { output=output.parent().unwrap().join("creator"); } fs::create_dir_all(&output).unwrap();
 let filter=a.iter().position(|x|x=="--match-filters").map(|i|a[i+1].clone()).unwrap_or_default();
 let initial=filter.contains("id ~= '"); let skipped=filter.contains("id !~= '");
 let history=fs::read_to_string(&archive).unwrap_or_default();let mut archive_file=fs::OpenOptions::new().create(true).append(true).open(&archive).unwrap();
 let before_source=a.iter().any(|s|s.contains("second-source"));
 for id in if before_source {vec!["300","200","100","600"]} else {vec!["300","200","100","400"]} {
  let in_re=filter.contains(id);
  let allowed=if initial {in_re} else if skipped {!in_re} else {true};
  if allowed && !history.lines().any(|x|x==format!("tiktok {id}")){
   fs::write(output.join(format!("post [{id}].mp4")),b"video").unwrap();
   fs::write(output.join(format!("post [{id}].info.json")),format!("{{\"id\":\"{id}\",\"extractor_key\":\"TikTok\",\"uploader_id\":\"creator\",\"uploader_url\":\"https://www.tiktok.com/@creator\"}}")).unwrap();
   writeln!(archive_file,"tiktok {id}").unwrap();
  }
 }
}
