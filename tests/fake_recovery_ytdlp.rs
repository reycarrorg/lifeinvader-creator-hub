use std::{env,fs,path::PathBuf,io::Write};
fn main(){
 let a:Vec<String>=env::args().collect();let url=a.last().unwrap();
 if a.iter().any(|x|x=="--print") { println!("{}",env::var("LV_FAKE_OWNER").unwrap_or("demo-account".into()));println!("MS4wLjABAAAA_testverified12345");return; }
 if !url.starts_with("tiktokuser:"){eprintln!("ERROR: unable to extract secondary user ID");std::process::exit(1);}
 let i=|n:&str|a.iter().position(|s|s==n).unwrap();let archive=PathBuf::from(&a[i("--download-archive")+1]);let template=PathBuf::from(&a[i("--output")+1]);
 let history=fs::read_to_string(&archive).unwrap_or_default();if !history.contains("recovered-id") {let parent=template.parent().unwrap();fs::write(parent.join("Recovered [recovered-id].mp4"),b"video").unwrap();fs::write(parent.join("Recovered [recovered-id].info.json"),r#"{"id":"recovered-id","title":"Recovered"}"#).unwrap();writeln!(fs::OpenOptions::new().create(true).append(true).open(archive).unwrap(),"tiktok recovered-id").unwrap();}
 println!("Fake recovered profile sync finished");
}
