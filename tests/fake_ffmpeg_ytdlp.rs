use std::{env,fs::{self,OpenOptions},io::Write,path::PathBuf};
fn main(){
 let args:Vec<String>=env::args().collect();let pos=args.iter().position(|x|x=="--ffmpeg-location");let sep=args.iter().position(|x|x=="--");
 let actual=pos.and_then(|i|args.get(i+1)).map(PathBuf::from).and_then(|p|p.canonicalize().ok());let expected=env::var_os("LV_EXPECT_FFMPEG").map(PathBuf::from).and_then(|p|p.canonicalize().ok());
 if actual.is_none()||actual!=expected||sep.is_some_and(|s|pos.unwrap_or(usize::MAX)>s){eprintln!("FFmpeg argument is missing, incorrect, or follows the URL separator");std::process::exit(71);}
 if let Ok(path)=env::var("LV_FAKE_LOG"){let mut f=OpenOptions::new().create(true).append(true).open(path).unwrap();writeln!(f,"{}",if args.iter().any(|x|x=="--flat-playlist"){"preview"}else if args.iter().any(|x|x=="--skip-download"){"metadata"}else{"download"}).unwrap();}
 if args.iter().any(|x|x=="--flat-playlist"){println!("{{\"id\":\"123\",\"title\":\"Test\"}}");}
 let _=fs::metadata(actual.unwrap()).unwrap();
}
