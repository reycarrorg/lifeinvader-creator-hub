#![cfg(windows)]
//! Runs the exact pinned production validator in a local no-network preflight.
use std::{path::PathBuf,process::{Command,Stdio},time::{Duration,Instant},sync::OnceLock};
use std::os::windows::process::CommandExt;
use serde_json::{Value,json};
const PYTHON_SHA:&str="4d6f5f81a4bca11191c4c7c6b43632694d0a4ce74e068619d8fdc161d469859a";
const MODULE_SHA:&str="d39d3fa475094ba353ed7ecd1f3f57e77f554edbf379932b48d03757c3de2313";
static CHECK:OnceLock<Result<(),&'static str>>=OnceLock::new();
const PREFLIGHT:&str=r#"
import runpy,sys
def offline(event,args):
    if event.startswith(("socket.","subprocess.")) or event in ("os.system","os.exec","os.spawn"):
        raise RuntimeError("Offline publisher preflight cannot use transport")
sys.addaudithook(offline)
m=runpy.run_path(sys.argv[1])
check,held=m["validated_upload_target"],m["PublisherHeld"]
valid="https://rupload.facebook.com:443/ig-api-upload/v21.0/12345678?ticket=A%2FB"
if check(valid)!="/ig-api-upload/v21.0/12345678?ticket=A%2FB":
    raise RuntimeError("Signed request target was not preserved")
for rejected in (
    "http://rupload.facebook.com/ig-api-upload/v21.0/12345678",
    "https://rupload.facebook.com.evil.invalid/ig-api-upload/v21.0/12345678",
    "https://rupload.facebook.com@evil.invalid/ig-api-upload/v21.0/12345678",
    "https://rupload.facebook.com:444/ig-api-upload/v21.0/12345678",
    "https://rupload.facebook.com/elsewhere/12345678",
    "https://rupload.facebook.com/ig-api-upload/%252e%252e/12345678",
    "https://rupload.facebook.com/ig-api-upload/12345678#fragment",
    "https://rupload.facebook.com/ig-api-upload/v21.0/%00",
):
    try: check(rejected)
    except held: continue
    raise RuntimeError("Unexpectedly accepted rejected URI")
try: m["InstagramResumableUploader"](None,None).upload_unpublished(media=b"",expected_sha256="")
except held as e:
    if str(e)!="Unpublished upload is disabled": raise
else:
    raise RuntimeError("Publisher gate was not held")
"#;
fn check()->Result<(),&'static str>{
    let python=PathBuf::from(std::env::var_os("LIFEINVADER_INSTAGRAM_PREFLIGHT_PYTHON").ok_or("unconfigured")?);
    let module=PathBuf::from(std::env::var_os("LIFEINVADER_INSTAGRAM_NATIVE_MODULE").ok_or("unconfigured")?);
    if module.file_name().and_then(|s|s.to_str())!=Some("native_publisher.py"){return Err("module_path_invalid");}
    for p in [&python,&module]{crate::controller_native_launcher::clean_path(p).map_err(|_|"preflight_path_invalid")?;}
    let _python=crate::controller_native_process::lease(&python,PYTHON_SHA).map_err(|_|"python_pin_invalid")?;
    let _module=crate::controller_native_process::lease(&module,MODULE_SHA).map_err(|_|"module_pin_invalid")?;
    let system=std::env::var_os("SystemRoot").ok_or("system_root_unavailable")?;
    let mut p=Command::new(python).args(["-I","-S","-B","-c",PREFLIGHT]).arg(module)
        .env_clear().env("SystemRoot",system)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .creation_flags(0x08000000).spawn().map_err(|_|"validator_preflight_spawn_failed")?;
    let until=Instant::now()+Duration::from_secs(10);
    loop {
        match p.try_wait(){
            Ok(Some(s)) if s.success()=>return Ok(()),
            Ok(Some(_))=>return Err("validator_preflight_failed"),
            Ok(None) if Instant::now()<until=>std::thread::sleep(Duration::from_millis(25)),
            _=>{let _=p.kill();let _=p.wait();return Err("validator_preflight_timeout_or_wait_failed");}
        }
    }
}
pub(crate) fn health(run:bool)->Value{
    let result=if run{Some(CHECK.get_or_init(check))}else{CHECK.get()};
    let (state,reason)=match result{Some(Ok(()))=>("passed",None),Some(Err(e))=>("held",Some(*e)),None=>("not_run",None)};
    json!({"provider":"instagram","health":"held","offline_validator":state,"reason":reason,
        "production_module_sha256":if state=="passed"{Some(MODULE_SHA)}else{None},
        "credential_type":"facebook_page","access_expiry_kind":"unknown","last_refresh_at_unix":null,
        "access_expires_at_unix":null,"refresh_expires_at_unix":null,"data_access_expires_at_unix":null,
        "transport_enabled":false,"publishing_enabled":false})
}
