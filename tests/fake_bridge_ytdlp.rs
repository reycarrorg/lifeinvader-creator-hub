use std::{env, fs, path::PathBuf, process};
fn main() {
    let a: Vec<String> = env::args().collect();
    let urls: Vec<&String> = a.iter().filter(|x| x.starts_with("https://")).collect();
    if urls.iter().all(|u| !u.contains("/reel/")) {
        eprintln!("synthetic profile scan failure");
        process::exit(1)
    }
    let output = a
        .iter()
        .position(|x| x == "--output")
        .map(|i| a[i + 1].clone())
        .unwrap();
    let output = output
        .replace("%(title).120B", "test")
        .replace("%(id)s", "abcde123")
        .replace("%(id.-8:)s", "abcde123")
        .replace("%(ext)s", "mp4");
    let path = PathBuf::from(output);
    fs::write(&path, b"test-video").unwrap();
    fs::write(
        path.with_extension("info.json"),
        r#"{"id":"abcde123","upload_date":"20240101"}"#,
    )
    .unwrap();
    println!("synthetic video saved");
}
