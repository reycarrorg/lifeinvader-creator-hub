use std::process::{Command, exit};
fn main() {
    let tool = std::env::var("LV_TEST_REAL_YTDLP").unwrap();
    let status = Command::new(tool)
        .args([
            "--playlist-items",
            "1",
            "--test",
            "--socket-timeout",
            "12",
            "--retries",
            "1",
        ])
        .args(std::env::args().skip(1))
        .status()
        .unwrap();
    exit(status.code().unwrap_or(1));
}
