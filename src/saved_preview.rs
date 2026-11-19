//! Ordinary main-app saved-video viewing. No queue, receipt authority, database,
//! worker, scheduler, import mutation, or publishing capability is installed.
use axum::{body::{Body,Bytes}, extract::State, http::{HeaderMap,StatusCode}, response::{Html,IntoResponse,Response}, routing::get, Router};
use sha2::{Digest,Sha256};
use std::{fs::OpenOptions,io::Read,path::Path};

const MAX_BYTES:u64=64*1024*1024;
const PAGE:&str=r#"<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Lifeinvader · Saved video</title><style>body{background:#15171b;color:#eee;font:16px system-ui;margin:24px}video{display:block;max-height:75vh;max-width:100%;margin:20px auto}p{max-width:760px;margin:12px auto}</style><h1>Saved video preview</h1><p>File size and SHA-256 match the values supplied at startup. This view does not establish worker completion or permission to publish.</p><video controls playsinline preload="metadata" src="/video"></video><p>Captions, if burned into this video, appear during playback. No new captions or render are generated. Close this window to end the preview.</p></html>"#;

fn snapshot(path:&Path,expected:&str,size:u64)->Result<Bytes,Box<dyn std::error::Error>> {
    if crate::controller_proof::Hash256::parse(expected).is_none() || size==0 || size>MAX_BYTES {return Err("Invalid saved-video expectations".into());}
    let mut options=OpenOptions::new();options.read(true);
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        // Permit readers only while hashing/copying; reject the leaf reparse point.
        options.share_mode(1).custom_flags(0x00200000);
    }
    let mut file=options.open(path)?;
    let before=file.metadata()?;
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        if before.file_attributes() & 0x400 != 0 {return Err("Saved video is a reparse point".into());}
    }
    if !before.is_file() || before.len()!=size {return Err("Saved-video size mismatch".into());}
    let mut bytes=Vec::with_capacity(size as usize);
    file.by_ref().take(size+1).read_to_end(&mut bytes)?;
    let after=file.metadata()?;
    if bytes.len() as u64!=size || after.len()!=size || before.modified()?!=after.modified()?
        || format!("{:x}",Sha256::digest(&bytes))!=expected {return Err("Saved-video integrity mismatch".into());}
    // Serve only the verified memory snapshot, never reopen a caller-chosen path.
    Ok(Bytes::from(bytes))
}

fn byte_range(raw:&str,len:usize)->Option<(usize,usize)> {
    let (a,b)=raw.strip_prefix("bytes=")?.split_once('-')?;
    if a.is_empty() {
        let count=b.parse::<usize>().ok()?;
        return (count>0).then_some((len.saturating_sub(count),len-1));
    }
    let start=a.parse::<usize>().ok()?;
    let end=if b.is_empty(){len-1}else{b.parse::<usize>().ok()?.min(len-1)};
    (start<=end && start<len).then_some((start,end))
}
async fn video(State(data):State<Bytes>,headers:HeaderMap)->Response {
    let len=data.len();
    let (status,body,range)=match headers.get("range") {
        None=>(StatusCode::OK,data,None),
        Some(raw)=>match raw.to_str().ok().and_then(|s|byte_range(s,len)) {
            Some((a,b))=>(StatusCode::PARTIAL_CONTENT,data.slice(a..=b),Some(format!("bytes {a}-{b}/{len}"))),
            None=>return (StatusCode::RANGE_NOT_SATISFIABLE,[("content-range",format!("bytes */{len}"))]).into_response(),
        }
    };
    let mut response=Response::builder().status(status).header("content-type","video/mp4")
        .header("accept-ranges","bytes").header("cache-control","no-store").header("content-length",body.len());
    if let Some(r)=range {response=response.header("content-range",r);}
    response.body(Body::from(body)).expect("fixed response headers")
}

pub async fn run()->Result<(),Box<dyn std::error::Error>> {
    let path=std::env::var_os("LIFEINVADER_PREVIEW_FILE").ok_or("Set LIFEINVADER_PREVIEW_FILE")?;
    let hash=std::env::var("LIFEINVADER_PREVIEW_SHA256")?;
    let size=std::env::var("LIFEINVADER_PREVIEW_BYTES")?.parse::<u64>()?;
    let data=snapshot(Path::new(&path),&hash,size)?;
    // Ephemeral loopback port avoids touching the normal app or playground.
    let listener=tokio::net::TcpListener::bind(("127.0.0.1",0)).await?;
    let url=format!("http://{}/",listener.local_addr()?);
    let router=Router::new().route("/",get(||async{Html(PAGE)})).route("/video",get(video))
        .layer(axum::middleware::from_fn(crate::local_only)).with_state(data);
    println!("Lifeinvader saved preview: {url}");
    #[cfg(windows)] {
        let profile=format!("SavedPreview-{}",uuid::Uuid::new_v4());
        let mut window=crate::launch_window_at(&url,&profile)?;
        axum::serve(listener,router).with_graceful_shutdown(async move {
            let _=tokio::task::spawn_blocking(move||window.wait()).await;
        }).await?;
    }
    #[cfg(not(windows))] {
        axum::serve(listener,router).with_graceful_shutdown(async {tokio::time::sleep(std::time::Duration::from_secs(1800)).await}).await?;
    }
    Ok(())
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn saved_preview_range_boundaries() {
        assert_eq!(byte_range("bytes=1-3",10),Some((1,3)));
        assert_eq!(byte_range("bytes=8-",10),Some((8,9)));
        assert_eq!(byte_range("bytes=-3",10),Some((7,9)));
        for s in ["bytes=10-","bytes=3-1","bytes=0-1,3-4","bytes=-0"] {assert_eq!(byte_range(s,10),None);}
    }
    #[test] fn saved_preview_rejects_wrong_expectations() {
        let path=std::env::temp_dir().join(format!("saved-preview-{}",uuid::Uuid::new_v4()));
        std::fs::write(&path,b"synthetic video bytes").unwrap();
        let hash=format!("{:x}",Sha256::digest(b"synthetic video bytes"));
        let size=b"synthetic video bytes".len() as u64;
        assert!(snapshot(&path,&hash,size+1).is_err());
        assert!(snapshot(&path,&"0".repeat(64),size).is_err());
        assert_eq!(snapshot(&path,&hash,size).unwrap().as_ref(),b"synthetic video bytes");
    }
}
