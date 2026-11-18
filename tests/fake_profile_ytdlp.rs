use std::{env, fs, io::Write, path::PathBuf, thread, time::Duration};
fn main() {
    let args: Vec<String> = env::args().collect();
    assert!(args.contains(&"--no-overwrites".to_string()));
    assert!(args.contains(&"--write-info-json".to_string()));
    let archive =
        PathBuf::from(&args[args.iter().position(|x| x == "--download-archive").unwrap() + 1]);
    let output = PathBuf::from(&args[args.iter().position(|x| x == "--output").unwrap() + 1]);
    assert!(output.to_string_lossy().contains("%(id)s"));
    assert_eq!(archive.parent(), output.parent());
    let dir = output.parent().unwrap();
    println!("__LV_PROGRESS__ 43.2%");
    thread::sleep(Duration::from_millis(300));
    let old = fs::read_to_string(&archive).unwrap_or_default();
    if !old.contains("tiktok 1234567890123456789") {
        fs::write(
            dir.join("test post [1234567890123456789].mp4"),
            b"video-bytes",
        )
        .unwrap();
        fs::write(
            dir.join("test post [1234567890123456789].info.json"),
            br#"{"id":"1234567890123456789","upload_date":"20240102"}"#,
        )
        .unwrap();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&archive)
            .unwrap();
        writeln!(file, "tiktok 1234567890123456789").unwrap();
    }
}
