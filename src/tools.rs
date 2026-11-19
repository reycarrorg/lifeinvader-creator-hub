use std::{path::PathBuf,process::Command};

/// Resolve the actual executable so child tools do not depend on the system PATH.
pub(crate) fn ffmpeg_path() -> Option<PathBuf> {
 let name=if cfg!(windows){"ffmpeg.exe"}else{"ffmpeg"};
 let from_env=std::env::var_os("LIFEINVADER_FFMPEG").map(PathBuf::from).map(|p|if p.is_dir(){p.join(name)}else{p});
 let bundled=std::env::current_exe().ok().and_then(|p|p.parent().map(|d|d.join("tools").join(name)));
 let local=std::env::current_dir().ok().map(|d|d.join("tools").join(name));
 let path=std::env::var_os("PATH").and_then(|v|std::env::split_paths(&v).map(|d|d.join(name)).find(|p|p.is_file()));
 [from_env,bundled,local,path].into_iter().flatten().find(|p|p.is_file()).and_then(|p|p.canonicalize().ok())
}
pub(crate) fn configure_ytdlp(command:&mut Command){
 if let Some(path)=ffmpeg_path(){command.arg("--ffmpeg-location").arg(path);}
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn missing_path_does_not_prevent_command_construction(){let mut cmd=Command::new("yt-dlp");configure_ytdlp(&mut cmd);if let Some(p)=ffmpeg_path(){assert!(p.is_file());}}
}
