use std::{env, fs, io::Write, path::PathBuf, thread, time::Duration};
fn main() {
    let args: Vec<String> = env::args().collect();
    assert!(args.contains(&"--ignore-config".to_string()));
    assert!(args.contains(&"--no-overwrites".to_string()));
    assert!(args.contains(&"--write-info-json".to_string()));
    let archive =
        PathBuf::from(&args[args.iter().position(|x| x == "--download-archive").unwrap() + 1]);
    let template = PathBuf::from(&args[args.iter().position(|x| x == "--output").unwrap() + 1]);
    assert!(args.contains(&"--progress".to_string()));
    assert!(args.iter().any(|x| x.contains("__LV_PROGRESS__")));
    println!("__LV_PROGRESS__ 43.2%");
    let delay = env::var("LV_FAKE_DELAY_MS")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or(2500);
    thread::sleep(Duration::from_millis(delay));
    let history = fs::read_to_string(&archive).unwrap_or_default();
    if !history.lines().any(|x| x == "tiktok fake-new-id") {
        fs::write(
            template.parent().unwrap().join("Fake sample [fake-id].mp4"),
            b"fake-video",
        )
        .unwrap();
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&archive)
            .unwrap();
        writeln!(f, "tiktok fake-new-id").unwrap();
        println!("Downloaded one fake test file");
    } else {
        println!("Already in test archive");
    }
}
