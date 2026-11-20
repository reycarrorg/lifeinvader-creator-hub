//! Explicit local API review mode. Never inferred from server-only or an env flag.
pub fn enabled(args:&[String],configured:bool)->Result<bool,&'static str>{
    let count=args.iter().filter(|s|s.as_str()=="--headless-review").count();
    if count==0{return Ok(false)}
    if count!=1 || !configured || !args.iter().any(|s|s=="--server-only"){
        return Err("headless_review_requires_explicit_server_and_pinned_plan")
    }
    Ok(true)
}
#[derive(Debug,PartialEq,Eq)]
pub struct BackgroundPolicy {pub source_scheduler:bool,pub auto_queue:bool,pub cache_deletion:bool}
pub fn background_policy(headless:bool)->BackgroundPolicy{
    BackgroundPolicy{source_scheduler:!headless,auto_queue:!headless,cache_deletion:false}
}
pub fn scheduler_allowed(headless:bool)->bool{background_policy(headless).source_scheduler}
/// Permanent adapted-delivery prohibition; no environment/config switch enables deletion.
pub fn cache_deletion_allowed()->bool{false}
/// No-authority package check: explicit existing root, server-only, no render plan.
pub fn package_root(args:&[String])->Result<Option<std::path::PathBuf>, &'static str>{
    let count=args.iter().filter(|s|s.as_str()=="--package-review-root").count();
    if count==0{return Ok(None)}
    let values:Vec<_>=args.windows(2).filter(|w|w[0]=="--package-review-root").collect();
    if count!=1 || values.len()!=1 || !args.iter().any(|s|s=="--server-only") || args.iter().any(|s|s=="--headless-review" || s=="--isolated-review-plan" || s=="--isolated-review-sha256") {return Err("package_review_scope")}
    let root=std::path::PathBuf::from(&values[0][1]);
    if !root.is_absolute() || !root.is_dir() || root.components().any(|c|matches!(c,std::path::Component::ParentDir)){return Err("package_review_root")}
    for part in root.ancestors(){let m=std::fs::symlink_metadata(part).map_err(|_|"package_review_root")?;
        if m.file_type().is_symlink(){return Err("package_review_reparse")}
        #[cfg(windows)] {use std::os::windows::fs::MetadataExt;if m.file_attributes()&0x400!=0{return Err("package_review_reparse")}}
    }
    Ok(Some(root))
}
#[cfg(test)] mod tests{
    use super::*;
    fn args(v:&[&str])->Vec<String>{v.iter().map(|s|s.to_string()).collect()}
    #[test] fn background_policy_disables_both_timers_and_all_cache_deletion(){
        assert_eq!(background_policy(true),BackgroundPolicy{source_scheduler:false,auto_queue:false,cache_deletion:false});
        assert_eq!(background_policy(false),BackgroundPolicy{source_scheduler:true,auto_queue:true,cache_deletion:false});
        assert!(!cache_deletion_allowed());
    }
    #[test] fn package_review_root_is_explicit_and_authority_free(){
        let root=env!("CARGO_MANIFEST_DIR").to_owned();
        let good=vec!["--server-only".to_owned(),"--package-review-root".to_owned(),root.clone()];
        assert_eq!(package_root(&good).unwrap(),Some(std::path::PathBuf::from(&root)));
        assert_eq!(package_root(&[]).unwrap(),None);
        assert!(package_root(&args(&["--server-only","--package-review-root"])).is_err());
        assert!(package_root(&args(&["--server-only","--package-review-root","relative"])).is_err());
        let mut missing_server=good.clone();missing_server.remove(0);assert!(package_root(&missing_server).is_err());
        for extra in ["--headless-review","--isolated-review-plan","--isolated-review-sha256","--package-review-root"]{let mut a=good.clone();a.push(extra.into());assert!(package_root(&a).is_err());}
    }
    #[test] fn headless_mode_requires_explicit_scoped_plan_and_suppresses_scheduler(){
        assert_eq!(enabled(&[],false),Ok(false));
        assert_eq!(enabled(&args(&["--server-only"]),true),Ok(false));
        assert!(enabled(&args(&["--headless-review"]),true).is_err());
        assert!(enabled(&args(&["--headless-review","--server-only"]),false).is_err());
        assert!(enabled(&args(&["--headless-review","--headless-review","--server-only"]),true).is_err());
        let mode=enabled(&args(&["--headless-review","--server-only"]),true).unwrap();
        assert!(mode);assert!(!scheduler_allowed(mode));
    }
}
