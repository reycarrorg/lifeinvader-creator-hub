use std::{env, process, thread, time::Duration};
fn main() {
    let args: Vec<String> = env::args().collect();
    if !args.iter().any(|x| x == "--skip-download")
        || args.iter().any(|x| x == "--download-archive")
    {
        eprintln!("unsafe test arguments");
        process::exit(2)
    }
    if env::var("LV_DATE_SLOW_FLAG")
        .ok()
        .is_some_and(|p| std::path::Path::new(&p).is_file())
    {
        thread::sleep(Duration::from_secs(8));
    }
    if let Ok(v) = env::var("LV_DATE_DELAY_MS") {
        thread::sleep(Duration::from_millis(v.parse().unwrap_or(0)));
    }
    let url = args.last().unwrap();
    let id = url.trim_end_matches('/').rsplit('/').next().unwrap();
    println!("{}\t20240102\tNA", id);
}
