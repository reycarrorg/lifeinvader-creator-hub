//! Mac-only, local generated-preview cache. Never acts on source media or legacy caches.
use crate::App;
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub(crate) static THUMB_LOCK: Mutex<()> = Mutex::new(());
static CURSOR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
const HIGH: i64 = 512 * 1024 * 1024;
const LOW: i64 = 384 * 1024 * 1024;
const MIN_IDLE: i64 = 24 * 60 * 60;
const MAX_AGE: i64 = 30 * 24 * 60 * 60;
const PAGE: i64 = 500;
const REMOVE_LIMIT: usize = 100;
const ACTIVE_QUEUE_LIMIT: usize = 500;
const INITIAL_DELAY_SECONDS: u64 = 60;
const PERIOD_SECONDS: u64 = 6 * 60 * 60;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceIdentity {
    dev: String,
    ino: String,
    size: i64,
    mtime: i64,
    mtime_ns: i64,
}

impl SourceIdentity {
    fn object_key(&self) -> (String, String) {
        (self.dev.clone(), self.ino.clone())
    }
}

pub(crate) fn init(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS thumbnail_cache_entries (scope TEXT NOT NULL,name TEXT NOT NULL,dev TEXT NOT NULL,ino TEXT NOT NULL,size INTEGER NOT NULL,mtime INTEGER NOT NULL,mtime_ns INTEGER NOT NULL,last_used INTEGER NOT NULL,PRIMARY KEY(scope,name));
 CREATE TABLE IF NOT EXISTS thumbnail_cache_links (scope TEXT NOT NULL,name TEXT NOT NULL,role TEXT NOT NULL,profile_id INTEGER NOT NULL,root TEXT NOT NULL,path TEXT NOT NULL,source_dev TEXT NOT NULL,source_ino TEXT NOT NULL,source_size INTEGER NOT NULL,source_mtime INTEGER NOT NULL,source_mtime_ns INTEGER NOT NULL,PRIMARY KEY(scope,name,role,profile_id,root,path),FOREIGN KEY(scope,name) REFERENCES thumbnail_cache_entries(scope,name) ON DELETE CASCADE);
 CREATE INDEX IF NOT EXISTS thumbnail_cache_age ON thumbnail_cache_entries(scope,last_used,name);
 CREATE INDEX IF NOT EXISTS thumbnail_cache_queue_paths ON posting_queue(root,path);")
}
fn valid_name(name: &str) -> bool {
    name.len() == 20
        && name.ends_with(".jpg")
        && name.as_bytes()[..16]
            .iter()
            .all(|v| v.is_ascii_digit() || (*v >= b'a' && *v <= b'f'))
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|v| v.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::os::macos::fs::MetadataExt as MacMetadataExt;
    use std::{
        ffi::{CStr, CString},
        fs::{self, File, OpenOptions},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{
                ffi::OsStrExt,
                fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
            },
        },
    };

    fn os_cache() -> Option<PathBuf> {
        let n = unsafe { libc::confstr(libc::_CS_DARWIN_USER_CACHE_DIR, std::ptr::null_mut(), 0) };
        if n == 0 || n > 4096 {
            return None;
        }
        let mut buf = vec![0u8; n];
        if unsafe { libc::confstr(libc::_CS_DARWIN_USER_CACHE_DIR, buf.as_mut_ptr().cast(), n) }
            != n
        {
            return None;
        }
        let val = CStr::from_bytes_with_nul(&buf).ok()?;
        let path = PathBuf::from(std::ffi::OsStr::from_bytes(val.to_bytes()));
        let base = fs::canonicalize(path).ok()?;
        // Restrict to the OS-designated per-user cache tree on the local APFS system volume.
        if !base.starts_with("/private/var/folders") {
            return None;
        }
        let m = fs::symlink_metadata(&base).ok()?;
        if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || (m.mode() & 0o077) != 0 {
            return None;
        }
        let c = CString::new(base.as_os_str().as_bytes()).ok()?;
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0
            || st.f_flags & libc::MNT_LOCAL as u32 == 0
        {
            return None;
        }
        let kind = unsafe { CStr::from_ptr(st.f_fstypename.as_ptr()) }.to_bytes();
        if kind != b"apfs" {
            return None;
        }
        // A File Provider/cloud marker on any ancestor makes the cache location ambiguous.
        for p in base.ancestors() {
            if !no_cloud_marker(p) {
                return None;
            }
        }
        Some(base)
    }
    fn no_cloud_marker(path: &Path) -> bool {
        let Ok(name) = CString::new(path.as_os_str().as_bytes()) else {
            return false;
        };
        let size = unsafe {
            libc::listxattr(name.as_ptr(), std::ptr::null_mut(), 0, libc::XATTR_NOFOLLOW)
        };
        if size < 0 || size > 65536 {
            return false;
        }
        if size == 0 {
            return true;
        }
        let mut names = vec![0u8; size as usize];
        let read = unsafe {
            libc::listxattr(
                name.as_ptr(),
                names.as_mut_ptr().cast(),
                names.len(),
                libc::XATTR_NOFOLLOW,
            )
        };
        if read < 0 || read as usize != names.len() {
            return false;
        }
        for value in names.split(|x| *x == 0) {
            let lower = String::from_utf8_lossy(value).to_ascii_lowercase();
            if [
                "fileprovider",
                "drivefs",
                "icloud",
                "ubiquit",
                "onedrive",
                "dropbox",
            ]
            .iter()
            .any(|x| lower.contains(x))
            {
                return false;
            }
        }
        true
    }
    #[cfg(test)]
    pub fn fixture_no_cloud_marker(path: &Path) -> bool {
        no_cloud_marker(path)
    }
    #[cfg(test)]
    pub fn fixture_private_child(parent: &Path, name: &str) -> Option<PathBuf> {
        private_child(parent, name)
    }
    fn private_child(parent: &Path, name: &str) -> Option<PathBuf> {
        let path = parent.join(name);
        match fs::symlink_metadata(&path) {
            Ok(m)
                if m.file_type().is_dir()
                    && m.uid() == unsafe { libc::geteuid() }
                    && m.mode() & 0o077 == 0 => {}
            Ok(_) => return None,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut b = fs::DirBuilder::new();
                b.mode(0o700);
                b.create(&path).ok()?;
            }
            Err(_) => return None,
        }
        let canonical = fs::canonicalize(&path).ok()?;
        if canonical.parent() != Some(parent)
            || !no_cloud_marker(&canonical)
            || fs::metadata(&canonical).ok()?.dev() != fs::metadata(parent).ok()?.dev()
        {
            return None;
        }
        Some(canonical)
    }
    pub struct Guard {
        pub dir: PathBuf,
        pub directory: File,
        _lock: File,
    }
    pub struct Inspector {
        dir: PathBuf,
        directory: File,
    }
    pub fn inspector(guard: &Guard) -> Option<Inspector> {
        Some(Inspector {
            dir: guard.dir.clone(),
            directory: guard.directory.try_clone().ok()?,
        })
    }
    #[cfg(test)]
    static FIXTURE_BASE: Mutex<Option<PathBuf>> = Mutex::new(None);
    #[cfg(test)]
    pub fn set_fixture_base(path: Option<PathBuf>) {
        *FIXTURE_BASE.lock().unwrap() = path;
    }
    pub fn acquire(scope: &str) -> Option<Guard> {
        if scope.len() != 16
            || !scope
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return None;
        }
        #[cfg(test)]
        let base = FIXTURE_BASE.lock().ok()?.clone().or_else(os_cache)?;
        #[cfg(not(test))]
        let base = os_cache()?;
        acquire_at_base(&base, scope)
    }
    #[cfg(test)]
    pub fn fixture_acquire(base: &Path, scope: &str) -> Option<Guard> {
        acquire_at_base(base, scope)
    }
    fn acquire_at_base(base: &Path, scope: &str) -> Option<Guard> {
        let app = private_child(base, "lifeinvader-generated-previews-v1")?;
        let dir = private_child(&app, scope)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(dir.join(".lock"))
            .ok()?;
        let metadata = lock.metadata().ok()?;
        if !metadata.is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.nlink() != 1
            || metadata.mode() & 0o077 != 0
            || !no_cloud_marker(&dir.join(".lock"))
        {
            return None;
        }
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return None;
        }
        let c = CString::new(dir.as_os_str().as_bytes()).ok()?;
        let fd = unsafe {
            libc::open(
                c.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return None;
        }
        let directory = unsafe { File::from_raw_fd(fd) };
        Some(Guard {
            dir,
            directory,
            _lock: lock,
        })
    }
    #[cfg(test)]
    pub fn fixture_guard(dir: &Path) -> Option<Guard> {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .open(dir.join(".fixture-lock"))
            .ok()?;
        let c = CString::new(dir.as_os_str().as_bytes()).ok()?;
        let fd = unsafe {
            libc::open(
                c.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return None;
        }
        Some(Guard {
            dir: dir.to_owned(),
            directory: unsafe { File::from_raw_fd(fd) },
            _lock: lock,
        })
    }
    pub fn identity(path: &Path) -> Option<(String, String, i64, i64, i64)> {
        let m = fs::symlink_metadata(path).ok()?;
        if !m.file_type().is_file()
            || m.nlink() != 1
            || m.uid() != unsafe { libc::geteuid() }
            || m.len() == 0
            || m.len() > i64::MAX as u64
            || (m.st_flags() & 0x4000_0000) != 0
            || !no_cloud_marker(path)
        {
            return None;
        }
        Some((
            m.dev().to_string(),
            m.ino().to_string(),
            m.len() as i64,
            m.mtime(),
            m.mtime_nsec(),
        ))
    }
    pub fn source_identity(path: &Path) -> Option<SourceIdentity> {
        let m = fs::metadata(path).ok()?;
        source_identity_metadata(&m)
    }
    pub fn source_identity_metadata(m: &fs::Metadata) -> Option<SourceIdentity> {
        if !m.is_file() || m.len() > i64::MAX as u64 {
            return None;
        }
        Some(SourceIdentity {
            dev: m.dev().to_string(),
            ino: m.ino().to_string(),
            size: m.len() as i64,
            mtime: m.mtime(),
            mtime_ns: m.mtime_nsec(),
        })
    }
    pub fn inspect_if_matches(
        guard: &Inspector,
        name: &str,
        expected: &(String, String, i64, i64, i64),
    ) -> bool {
        if !super::valid_name(name) || !no_cloud_marker(&guard.dir.join(name)) {
            return false;
        }
        let name = match CString::new(name) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let mut m: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe {
            libc::fstatat(
                guard.directory.as_raw_fd(),
                name.as_ptr(),
                &mut m,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return false;
        }
        if m.st_mode & libc::S_IFMT != libc::S_IFREG
            || m.st_nlink != 1
            || m.st_uid != unsafe { libc::geteuid() }
            || m.st_size <= 0
            || (m.st_flags & 0x4000_0000) != 0
            || (
                m.st_dev.to_string(),
                m.st_ino.to_string(),
                m.st_size,
                m.st_mtime,
                m.st_mtime_nsec,
            ) != *expected
        {
            return false;
        }
        true
    }
    // Name-based unlink is on the pinned private local directory. A same-user
    // process able to rename its files can still race an inode check and unlink.
    pub fn unlink_name(guard: &Guard, name: &str) -> bool {
        if !super::valid_name(name) {
            return false;
        }
        let Ok(name) = CString::new(name) else {
            return false;
        };
        (unsafe { libc::unlinkat(guard.directory.as_raw_fd(), name.as_ptr(), 0) }) == 0
    }
}
#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;
    pub struct Guard {
        pub dir: PathBuf,
    }
    pub struct Inspector;
    pub fn inspector(_: &Guard) -> Option<Inspector> {
        None
    }
    pub fn acquire(_: &str) -> Option<Guard> {
        None
    }
    pub fn identity(_: &Path) -> Option<(String, String, i64, i64, i64)> {
        None
    }
    pub fn source_identity(_: &Path) -> Option<SourceIdentity> {
        None
    }
    pub fn source_identity_metadata(_: &std::fs::Metadata) -> Option<SourceIdentity> {
        None
    }
    pub fn inspect_if_matches(_: &Inspector, _: &str, _: &(String, String, i64, i64, i64)) -> bool {
        false
    }
    pub fn unlink_name(_: &Guard, _: &str) -> bool {
        false
    }
}

pub(crate) fn acquire(scope: &str) -> Option<platform::Guard> {
    platform::acquire(scope)
}

pub(crate) fn source_identity(path: &Path) -> Option<SourceIdentity> {
    platform::source_identity(path)
}

pub(crate) fn source_identity_metadata(metadata: &std::fs::Metadata) -> Option<SourceIdentity> {
    platform::source_identity_metadata(metadata)
}

type CacheIdentity = (String, String, i64, i64, i64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RegistrationInspection {
    SourceUnavailable,
    SourceChanged,
    CacheFileUntrusted,
}

#[derive(Clone, Copy)]
#[repr(usize)]
pub(crate) enum PreviewIssue {
    SourceChanged,
    SourceUnavailable,
    CacheFileUntrusted,
    Database,
    CacheWrite,
    SlotsFull,
    CacheUnavailable,
}

impl PreviewIssue {
    fn message(self) -> &'static str {
        match self {
            Self::SourceChanged => {
                "source metadata changed while making a preview; retrying or skipping registration"
            }
            Self::SourceUnavailable => {
                "source identity could not be verified; skipping registration"
            }
            Self::CacheFileUntrusted => {
                "a generated preview failed cache safety checks; leaving it unregistered"
            }
            Self::Database => "the preview cache database could not be checked or updated",
            Self::CacheWrite => {
                "a managed preview could not be saved; the preview remains available"
            }
            Self::SlotsFull => {
                "all safe preview cache slots are occupied; the preview remains available"
            }
            Self::CacheUnavailable => {
                "the private preview cache was unavailable; the preview remains available"
            }
        }
    }
}

// Fixed messages contain no media paths, URLs, cookies, or database values.
// Rate limiting prevents a problematic source from flooding the launcher log.
pub(crate) fn report_preview_issue(issue: PreviewIssue) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: [AtomicU64; 7] = [const { AtomicU64::new(0) }; 7];
    let clock = now().max(0) as u64;
    let last = &LAST[issue as usize];
    let previous = last.load(Ordering::Relaxed);
    if clock > 0
        && (previous == 0 || clock.saturating_sub(previous) >= 60)
        && last
            .compare_exchange(previous, clock, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        eprintln!("Lifeinvader preview cache: {}.", issue.message());
    }
}

// This can touch Drive metadata and must be called before acquiring the shared DB lock.
pub(crate) fn inspect_registration(
    file: &Path,
    root: &str,
    path: &str,
    source: &SourceIdentity,
) -> Result<CacheIdentity, RegistrationInspection> {
    let actual = resolved_source(root, path).ok_or(RegistrationInspection::SourceUnavailable)?;
    if &actual != source {
        return Err(RegistrationInspection::SourceChanged);
    }
    platform::identity(file).ok_or(RegistrationInspection::CacheFileUntrusted)
}

// Resolve without reading file contents. A missing, escaping, malformed, or otherwise
// unresolved queue path is unknown and must prevent cleanup, not be treated as absent.
fn resolved_source(root: &str, path: &str) -> Option<SourceIdentity> {
    let root = Path::new(root);
    let relative = Path::new(path);
    if !root.is_absolute()
        || relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return None;
    }
    let root = std::fs::canonicalize(root).ok()?;
    let target = root.join(relative).canonicalize().ok()?;
    if !target.starts_with(root) {
        return None;
    }
    source_identity(&target)
}

/// Only a newly produced file may establish provenance. The macOS cache never
/// adopts, serves, or removes an existing unregistered file.
pub(crate) fn record(
    db: &mut Connection,
    scope: &str,
    name: &str,
    identity: &CacheIdentity,
    role: &str,
    profile: i64,
    root: &str,
    path: &str,
    source: &SourceIdentity,
    created: bool,
) -> rusqlite::Result<bool> {
    if !valid_name(name) || now() == 0 {
        return Ok(false);
    }
    let (dev, ino, size, mtime, mtime_ns) = identity;
    let tx = db.transaction()?;
    if created {
        tx.execute(
            "DELETE FROM thumbnail_cache_entries WHERE scope=?1 AND name=?2",
            params![scope, name],
        )?;
        tx.execute(
            "INSERT INTO thumbnail_cache_entries VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![scope, name, dev, ino, size, mtime, mtime_ns, now()],
        )?;
    } else {
        let known:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM thumbnail_cache_entries WHERE scope=? AND name=? AND dev=? AND ino=? AND size=? AND mtime=? AND mtime_ns=?)",params![scope,name,dev,ino,size,mtime,mtime_ns],|r|r.get(0))?;
        if !known {
            return Ok(false);
        }
        let previous:Option<(String,String,i64,i64,i64)>=tx.query_row("SELECT source_dev,source_ino,source_size,source_mtime,source_mtime_ns FROM thumbnail_cache_links WHERE scope=? AND name=? AND role=? AND profile_id=? AND root=? AND path=?",params![scope,name,role,profile,root,path],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
        if previous.is_some_and(|old| {
            old != (
                source.dev.clone(),
                source.ino.clone(),
                source.size,
                source.mtime,
                source.mtime_ns,
            )
        }) {
            return Ok(false);
        }
        tx.execute(
            "UPDATE thumbnail_cache_entries SET last_used=? WHERE scope=? AND name=?",
            params![now(), scope, name],
        )?;
    }
    tx.execute(
        "INSERT OR IGNORE INTO thumbnail_cache_links VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            scope,
            name,
            role,
            profile,
            root,
            path,
            source.dev,
            source.ino,
            source.size,
            source.mtime,
            source.mtime_ns
        ],
    )?;
    tx.commit()?;
    Ok(true)
}

// Resolve every outstanding queue reference before sweeping. Comparing device/inode
// catches symlink and hard-link aliases, including aliases under different roots.
// If any active path cannot be resolved or there are too many, skip the entire pass.
fn active_queue_identities(db: &Connection) -> rusqlite::Result<Option<HashSet<(String, String)>>> {
    let mut query=db.prepare("SELECT q.root,q.path,q.edit_id,e.edited_path FROM posting_queue q LEFT JOIN media_edits e ON e.id=q.edit_id AND e.root=q.root WHERE q.state<>'posted' OR TRIM(q.post_url)='' OR EXISTS(SELECT 1 FROM posting_queue_claims c WHERE c.queue_id=q.id) OR NOT EXISTS(SELECT 1 FROM posting_queue_events ev WHERE ev.queue_id=q.id AND ev.state='posted' AND ev.post_url=q.post_url AND ev.post_url<>'') OR COALESCE((SELECT MAX(ev.id) FROM posting_queue_events ev WHERE ev.queue_id=q.id AND ev.event_type IN ('needs_review','failed_attempt')),0)>COALESCE((SELECT MAX(ev.id) FROM posting_queue_events ev WHERE ev.queue_id=q.id AND ev.state='posted' AND ev.previous_state<>'posted' AND ev.post_url=q.post_url AND ev.post_url<>'' AND ev.event_type NOT IN ('needs_review','failed_attempt')),0) LIMIT ?")?;
    let rows = query
        .query_map([ACTIVE_QUEUE_LIMIT as i64 + 1], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > ACTIVE_QUEUE_LIMIT {
        return Ok(None);
    }
    let mut protected = HashSet::new();
    for (root, path, edit_id, edited_path) in rows {
        let Some(original) = resolved_source(&root, &path) else {
            return Ok(None);
        };
        protected.insert(original.object_key());
        if !edit_id.is_empty() {
            let Some(edited) = edited_path.and_then(|p| resolved_source(&root, &p)) else {
                return Ok(None);
            };
            protected.insert(edited.object_key());
        }
    }
    Ok(Some(protected))
}

/// Only locally recorded, unambiguous source uses qualify. All active work, any
/// queued/claimed/review state, unknown/checked status, and incomplete post URLs block.
fn link_eligible(
    db: &Connection,
    role: &str,
    profile: i64,
    root: &str,
    path: &str,
    source: &SourceIdentity,
    protected: &HashSet<(String, String)>,
) -> rusqlite::Result<bool> {
    if role != "source" || root.is_empty() || path.is_empty() {
        return Ok(false);
    }
    let configured: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key='source_root'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let same_root = configured
        .as_deref()
        .filter(|value| Path::new(value).is_absolute())
        .and_then(|value| std::fs::canonicalize(value).ok())
        .is_some_and(|configured| configured == Path::new(root));
    if !same_root {
        return Ok(false);
    }
    if source.mtime <= 0
        || source.size <= 0
        || resolved_source(root, path).as_ref() != Some(source)
        || protected.contains(&source.object_key())
    {
        return Ok(false);
    }
    let known_source: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM profiles WHERE id=? AND role='source' AND archived=0)",
        [profile],
        |r| r.get(0),
    )?;
    if !known_source {
        return Ok(false);
    }
    let busy:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM source_sync_runs WHERE state='running') OR EXISTS(SELECT 1 FROM source_queue WHERE state IN ('pending','starting','running')) OR EXISTS(SELECT 1 FROM source_date_audits WHERE state='running')",[],|r|r.get(0))?;
    if busy {
        return Ok(false);
    }
    let mut stmt=db.prepare("SELECT q.profile_id,q.state,q.post_url,EXISTS(SELECT 1 FROM posting_queue_claims c WHERE c.queue_id=q.id),COALESCE((SELECT MAX(e.id) FROM posting_queue_events e WHERE e.queue_id=q.id AND e.event_type IN ('needs_review','failed_attempt')),0)>COALESCE((SELECT MAX(e.id) FROM posting_queue_events e WHERE e.queue_id=q.id AND e.state='posted' AND e.previous_state<>'posted' AND e.post_url=q.post_url AND e.post_url<>'' AND e.event_type NOT IN ('needs_review','failed_attempt')),0),(SELECT MAX(CAST(strftime('%s',e.created_at) AS INTEGER)) FROM posting_queue_events e WHERE e.queue_id=q.id AND e.state='posted' AND e.post_url=q.post_url AND e.post_url<>'') FROM posting_queue q LEFT JOIN media_edits e ON e.id=q.edit_id AND e.root=q.root WHERE q.root=?1 AND (q.path=?2 OR e.edited_path=?2)")?;
    let rows = stmt
        .query_map(params![root, path], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, Option<i64>>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows
        .iter()
        .any(|(_, state, url, claim, unresolved_review, reported)| {
            state != "posted"
                || url.trim().is_empty()
                || *claim
                || reported.is_none_or(|time| time <= source.mtime || time > now())
                || *unresolved_review
        })
    {
        return Ok(false);
    }
    // Enabled auto-queue destinations without a reported posted entry remain unknown.
    let mut enabled = db.prepare("SELECT profile_id FROM auto_queue_rules WHERE enabled=1")?;
    let enabled = enabled
        .query_map([], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if enabled
        .iter()
        .any(|id| !rows.iter().any(|(p, _, _, _, _, _)| p == id))
    {
        return Ok(false);
    }
    if !rows.is_empty() {
        return Ok(true);
    }
    let date:Option<(String,String,i64,i64)>=db.query_row("SELECT posted_date,date_kind,file_size,file_modified FROM source_date_results WHERE profile_id=? AND root=? AND path=?",params![profile,root,path],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let cutoff: Option<String> = db
        .query_row("SELECT value FROM settings WHERE key='cutoff'", [], |r| {
            r.get(0)
        })
        .optional()?;
    let Some((date, kind, audited_size, audited_modified)) = date else {
        return Ok(false);
    };
    if (kind != "verified" && kind != "metadata")
        || audited_size != source.size
        || audited_modified != source.mtime
    {
        return Ok(false);
    }
    let Some(cutoff) = cutoff.and_then(|v| chrono::DateTime::parse_from_rfc3339(&v).ok()) else {
        return Ok(false);
    };
    let parsed = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok();
    Ok(parsed.is_some_and(|d| d < cutoff.date_naive()))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DbStamp {
    data_version: i64,
    total_changes: u64,
}

fn db_stamp(db: &Connection) -> rusqlite::Result<DbStamp> {
    Ok(DbStamp {
        data_version: db.query_row("PRAGMA data_version", [], |row| row.get(0))?,
        total_changes: db.total_changes(),
    })
}

// Copy only to volatile memory, with a hard size cap and no retry loop. The lock is
// held for SQLite work only; no source/cache filesystem inspection happens here.
fn snapshot_db(db: &Connection) -> rusqlite::Result<Option<(Connection, DbStamp)>> {
    const MAX_BYTES: i64 = 64 * 1024 * 1024;
    let pages: i64 = db.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let size: i64 = db.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    if pages <= 0 || size <= 0 || pages.saturating_mul(size) > MAX_BYTES {
        return Ok(None);
    }
    let before = db_stamp(db)?;
    let mut snapshot = Connection::open_in_memory()?;
    {
        let backup = rusqlite::backup::Backup::new(db, &mut snapshot)?;
        if !matches!(backup.step(-1)?, rusqlite::backup::StepResult::Done) {
            return Ok(None);
        }
    }
    if db_stamp(db)? != before {
        return Ok(None);
    }
    snapshot.execute_batch("PRAGMA temp_store=MEMORY")?;
    Ok(Some((snapshot, before)))
}

struct Removal {
    name: String,
    identity: CacheIdentity,
}
struct Plan {
    removals: Vec<Removal>,
    examined: usize,
}

// All path resolution, Drive metadata and cache identity checks take place here,
// against a private in-memory SQLite snapshot and without the shared Hub DB lock.
fn prepare_plan(
    db: &Connection,
    cache: &platform::Inspector,
    scope: &str,
    current: i64,
    offset: usize,
) -> rusqlite::Result<Option<Plan>> {
    let mut total: i64 = db.query_row(
        "SELECT COALESCE(SUM(size),0) FROM thumbnail_cache_entries WHERE scope=?",
        [scope],
        |r| r.get(0),
    )?;
    let pressure = total > HIGH;
    if current <= 0 {
        return Ok(None);
    }
    let Some(protected) = active_queue_identities(db)? else {
        return Ok(None);
    };
    let mut q=db.prepare("SELECT name,dev,ino,size,mtime,mtime_ns,last_used FROM thumbnail_cache_entries WHERE scope=?1 AND last_used>0 AND last_used<=?2 ORDER BY last_used,name LIMIT ?3 OFFSET ?4")?;
    let records = q
        .query_map(
            params![
                scope,
                current.saturating_sub(MIN_IDLE),
                PAGE,
                offset.min(i64::MAX as usize) as i64
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(q);
    let examined = records.len();
    let mut removals = Vec::new();
    for (name, dev, ino, size, mtime, mtime_ns, last) in records {
        if removals.len() >= REMOVE_LIMIT {
            break;
        }
        if !valid_name(&name) || size <= 0 || last > current {
            continue;
        }
        if current - last < MAX_AGE && !(pressure && total > LOW) {
            continue;
        }
        let mut links=db.prepare("SELECT role,profile_id,root,path,source_dev,source_ino,source_size,source_mtime,source_mtime_ns FROM thumbnail_cache_links WHERE scope=? AND name=?")?;
        let links = links
            .query_map(params![scope, name], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    SourceIdentity {
                        dev: r.get(4)?,
                        ino: r.get(5)?,
                        size: r.get(6)?,
                        mtime: r.get(7)?,
                        mtime_ns: r.get(8)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if links.is_empty() {
            continue;
        }
        let mut eligible = true;
        for (role, profile, root, path, source) in links {
            if !link_eligible(db, &role, profile, &root, &path, &source, &protected)? {
                eligible = false;
                break;
            }
        }
        let identity = (dev, ino, size, mtime, mtime_ns);
        if eligible && platform::inspect_if_matches(cache, &name, &identity) {
            removals.push(Removal { name, identity });
            total = total.saturating_sub(size);
        }
    }
    Ok(Some(Plan { removals, examined }))
}

// If a provider is slow/hung, abandon the pass. At most one detached inspection
// thread may remain outstanding; it holds no DB lock and cannot delete anything.
static INSPECTION_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn timed_plan(
    snapshot: Connection,
    cache: platform::Inspector,
    scope: String,
    current: i64,
    offset: usize,
) -> Option<Plan> {
    plan_with_deadline(
        move || {
            prepare_plan(&snapshot, &cache, &scope, current, offset)
                .ok()
                .flatten()
        },
        std::time::Duration::from_secs(3),
    )
}

fn plan_with_deadline<F>(work: F, timeout: std::time::Duration) -> Option<Plan>
where
    F: FnOnce() -> Option<Plan> + Send + 'static,
{
    use std::sync::atomic::Ordering;
    if INSPECTION_BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return None;
    }
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    if std::thread::Builder::new()
        .name("lifeinvader-preview-inspect".into())
        .spawn(move || {
            struct Reset;
            impl Drop for Reset {
                fn drop(&mut self) {
                    INSPECTION_BUSY.store(false, Ordering::Release);
                }
            }
            let _reset = Reset;
            let result = work();
            let _ = sender.send(result);
        })
        .is_err()
    {
        INSPECTION_BUSY.store(false, Ordering::Release);
        return None;
    }
    receiver.recv_timeout(timeout).ok().flatten()
}

// One small transaction per file: only the already-validated private local cache
// is rechecked here. Drive/source inspection never happens under the DB lock.
fn apply_one(
    db: &mut Connection,
    cache: &platform::Guard,
    inspector: &platform::Inspector,
    scope: &str,
    stamp: DbStamp,
    removal: &Removal,
) -> rusqlite::Result<Option<DbStamp>> {
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if db_stamp(&tx)? != stamp {
        return Ok(None);
    }
    if platform::inspect_if_matches(inspector, &removal.name, &removal.identity)
        && platform::unlink_name(cache, &removal.name)
    {
        tx.execute(
            "DELETE FROM thumbnail_cache_entries WHERE scope=? AND name=?",
            params![scope, removal.name],
        )?;
    }
    let next = DbStamp {
        data_version: stamp.data_version,
        total_changes: tx.total_changes(),
    };
    tx.commit()?;
    Ok(Some(next))
}

fn sweep_with<F>(app: &App, planner: F)
where
    F: FnOnce(Connection, platform::Inspector, String, i64, usize) -> Option<Plan>,
{
    let Ok(_guard) = THUMB_LOCK.try_lock() else {
        return;
    };
    let Some(cache) = acquire(&app.cache_scope) else {
        return;
    };
    let Some(inspector) = platform::inspector(&cache) else {
        return;
    };
    let Some(final_inspector) = platform::inspector(&cache) else {
        return;
    };
    let (snapshot, stamp) = {
        let Ok(db) = app.db.lock() else {
            return;
        };
        let Ok(Some(result)) = snapshot_db(&db) else {
            return;
        };
        result
    };
    let offset = CURSOR.load(std::sync::atomic::Ordering::Relaxed);
    let Some(plan) = planner(snapshot, inspector, app.cache_scope.clone(), now(), offset) else {
        return;
    };
    let mut expected = stamp;
    for removal in &plan.removals {
        let next = {
            let Ok(mut db) = app.db.lock() else {
                return;
            };
            apply_one(
                &mut db,
                &cache,
                &final_inspector,
                &app.cache_scope,
                expected,
                removal,
            )
            .ok()
            .flatten()
        };
        let Some(next) = next else {
            return;
        };
        expected = next;
    }
    CURSOR.store(
        if plan.examined < PAGE as usize {
            0
        } else {
            offset.saturating_add(plan.examined)
        },
        std::sync::atomic::Ordering::Relaxed,
    );
}
fn sweep(app: &App) {
    sweep_with(app, timed_plan);
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn fixture_cache_base(path: Option<PathBuf>) {
    platform::set_fixture_base(path);
}
async fn worker_with_delays(app: App, initial: std::time::Duration, period: std::time::Duration) {
    tokio::time::sleep(initial).await;
    loop {
        let a = app.clone();
        let _ = tokio::task::spawn_blocking(move || sweep(&a)).await;
        tokio::time::sleep(period).await;
    }
}

pub(crate) async fn worker(app: App) {
    worker_with_delays(
        app,
        std::time::Duration::from_secs(INITIAL_DELAY_SECONDS),
        std::time::Duration::from_secs(PERIOD_SECONDS),
    )
    .await;
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) async fn fixture_worker(app: App) {
    worker_with_delays(
        app,
        std::time::Duration::from_millis(10),
        std::time::Duration::from_secs(3600),
    )
    .await;
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};
    const PATH: &str = "account/video.mp4";
    const SCOPE: &str = "0123456789abcdef";
    struct Fixture {
        dir: PathBuf,
        root: String,
        db: Connection,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "lifeinvader-preview-fixture-{}",
                uuid::Uuid::new_v4()
            ));
            fs::create_dir(&dir).unwrap();
            let root = dir.join("library");
            fs::create_dir_all(root.join("account")).unwrap();
            fs::write(root.join(PATH), b"synthetic source media").unwrap();
            let file = fs::OpenOptions::new()
                .write(true)
                .open(root.join(PATH))
                .unwrap();
            file.set_times(std::fs::FileTimes::new().set_modified(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000),
            ))
            .unwrap();
            let root = fs::canonicalize(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let db = Connection::open_in_memory().unwrap();
            crate::init(&db).unwrap();
            db.execute(
                "INSERT INTO settings(key,value) VALUES('source_root',?)",
                [root.as_str()],
            )
            .unwrap();
            db.execute("INSERT INTO profiles(id,name,platform,url,role,tags) VALUES(1,'source','tiktok','https://www.tiktok.com/@source','source','[]')",[]).unwrap();
            db.execute("INSERT INTO profiles(id,name,platform,url,role,tags) VALUES(2,'dest','tiktok','https://www.tiktok.com/@dest','destination','[]')",[]).unwrap();
            Self { dir, root, db }
        }
        fn identity(&self) -> SourceIdentity {
            resolved_source(&self.root, PATH).unwrap()
        }
        fn eligible(&self, role: &str, root: &str, path: &str) -> bool {
            let Some(protected) = active_queue_identities(&self.db).unwrap() else {
                return false;
            };
            link_eligible(&self.db, role, 1, root, path, &self.identity(), &protected).unwrap()
        }
        fn posted(&self) {
            self.db.execute("INSERT INTO posting_queue(id,profile_id,source_id,platform,identity_key,root,path,state,post_url) VALUES(1,2,1,'tiktok','post:1',?,?,'posted','https://www.tiktok.com/@dest/video/1')",params![self.root,PATH]).unwrap();
            self.db.execute("INSERT INTO posting_queue_events(queue_id,event_type,state,post_url) VALUES(1,'updated','posted','https://www.tiktok.com/@dest/video/1')",[]).unwrap();
        }
        fn preview(&mut self, name: &str) {
            let path = self.dir.join(name);
            fs::write(&path, b"generated fixture thumbnail").unwrap();
            let source = self.identity();
            let preview_identity = inspect_registration(&path, &self.root, PATH, &source).unwrap();
            record(
                &mut self.db,
                SCOPE,
                name,
                &preview_identity,
                "source",
                1,
                &self.root,
                PATH,
                &source,
                true,
            )
            .unwrap();
            self.db
                .execute(
                    "UPDATE thumbnail_cache_entries SET last_used=1 WHERE scope=? AND name=?",
                    params![SCOPE, name],
                )
                .unwrap();
        }
        fn sweep(&mut self) {
            let g = platform::fixture_guard(&self.dir).unwrap();
            let (snapshot, mut stamp) = snapshot_db(&self.db).unwrap().unwrap();
            let inspector = platform::inspector(&g).unwrap();
            if let Some(plan) =
                prepare_plan(&snapshot, &inspector, SCOPE, MAX_AGE + MIN_IDLE + 2, 0).unwrap()
            {
                for removal in &plan.removals {
                    let Some(next) =
                        apply_one(&mut self.db, &g, &inspector, SCOPE, stamp, removal).unwrap()
                    else {
                        break;
                    };
                    stamp = next;
                }
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
    #[test]
    fn eligibility_refuses_pending_claimed_unknown_and_running() {
        let f = Fixture::new();
        assert!(!f.eligible("source", &f.root, PATH));
        f.posted();
        assert!(f.eligible("source", &f.root, PATH));
        f.db.execute("UPDATE posting_queue SET state='queued' WHERE id=1", [])
            .unwrap();
        assert!(!f.eligible("source", &f.root, PATH));
        f.db.execute("UPDATE posting_queue SET state='posted' WHERE id=1", [])
            .unwrap();
        f.db.execute("INSERT INTO posting_queue_claims(queue_id,profile_id,token) VALUES(1,2,'fixture-token')",[]).unwrap();
        assert!(!f.eligible("source", &f.root, PATH));
        f.db.execute("DELETE FROM posting_queue_claims", [])
            .unwrap();
        f.db.execute("UPDATE posting_queue SET post_url='' WHERE id=1", [])
            .unwrap();
        assert!(!f.eligible("source", &f.root, PATH));
        f.db.execute(
            "UPDATE posting_queue SET post_url='https://example.invalid/1' WHERE id=1",
            [],
        )
        .unwrap();
        f.db.execute("INSERT INTO source_sync_runs(profile_id,state,log_file) VALUES(1,'running','fixture.log')",[]).unwrap();
        assert!(!f.eligible("source", &f.root, PATH));
        assert!(!f.eligible("destination", &f.root, PATH));
        assert!(!f.eligible("source", "/different", PATH));
    }
    #[test]
    fn cutoff_requires_verified_date_and_no_queued_use() {
        let f = Fixture::new();
        f.db.execute("INSERT INTO source_date_results(profile_id,root,path,posted_date,date_kind) VALUES(1,?,?, '2020-01-01','estimated_id')",params![f.root,PATH]).unwrap();
        assert!(!f.eligible("source", &f.root, PATH));
        let source = f.identity();
        f.db.execute(
            "UPDATE source_date_results SET date_kind='verified',file_size=?,file_modified=?",
            params![source.size, source.mtime],
        )
        .unwrap();
        assert!(f.eligible("source", &f.root, PATH));
        f.db.execute("UPDATE source_date_results SET file_size=file_size+1", [])
            .unwrap();
        assert!(!f.eligible("source", &f.root, PATH));
        f.db.execute("UPDATE source_date_results SET file_size=file_size-1", [])
            .unwrap();
        f.db.execute("INSERT INTO posting_queue(id,profile_id,source_id,platform,identity_key,root,path,state) VALUES(1,2,1,'tiktok','post:1',?,?,'queued')",params![f.root,PATH]).unwrap();
        assert!(!f.eligible("source", &f.root, PATH));
    }
    #[test]
    fn sweep_only_removes_registered_old_posted_previews() {
        let mut f = Fixture::new();
        f.posted();
        f.preview("0123456789abcdef.jpg");
        fs::write(f.dir.join("bbbbbbbbbbbbbbbb.jpg"), b"unknown original").unwrap();
        fs::write(f.dir.join("metadata.json"), b"{} ").unwrap();
        fs::write(f.dir.join("avatar.jpg"), b"avatar").unwrap();
        let original = f.dir.join("original.mp4");
        fs::write(&original, b"video").unwrap();
        f.sweep();
        assert!(!f.dir.join("0123456789abcdef.jpg").exists());
        for n in [
            "bbbbbbbbbbbbbbbb.jpg",
            "metadata.json",
            "avatar.jpg",
            "original.mp4",
        ] {
            assert!(f.dir.join(n).exists());
        }
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM thumbnail_cache_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn queued_and_unregistered_previews_survive() {
        let mut f = Fixture::new();
        f.preview("0123456789abcdef.jpg");
        let other = f.dir.join("bbbbbbbbbbbbbbbb.jpg");
        fs::write(&other, b"other").unwrap();
        let source = f.identity();
        let preview_identity = inspect_registration(&other, &f.root, PATH, &source).unwrap();
        record(
            &mut f.db,
            SCOPE,
            "bbbbbbbbbbbbbbbb.jpg",
            &preview_identity,
            "source",
            1,
            &f.root,
            PATH,
            &source,
            false,
        )
        .unwrap();
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM thumbnail_cache_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        f.sweep();
        assert!(f.dir.join("0123456789abcdef.jpg").exists());
        assert!(other.exists());
    }
    #[test]
    fn replaced_files_symlinks_and_hardlinks_are_not_removed() {
        use std::os::unix::fs::symlink;
        let mut f = Fixture::new();
        f.posted();
        f.preview("0123456789abcdef.jpg");
        let target = f.dir.join("original.mp4");
        fs::write(&target, b"original").unwrap();
        fs::remove_file(f.dir.join("0123456789abcdef.jpg")).unwrap();
        symlink(&target, f.dir.join("0123456789abcdef.jpg")).unwrap();
        f.sweep();
        assert!(
            fs::symlink_metadata(f.dir.join("0123456789abcdef.jpg"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&target).unwrap(), b"original");
        fs::remove_file(f.dir.join("0123456789abcdef.jpg")).unwrap();
        fs::hard_link(&target, f.dir.join("0123456789abcdef.jpg")).unwrap();
        f.sweep();
        assert!(f.dir.join("0123456789abcdef.jpg").exists());
        assert_eq!(fs::read(&target).unwrap(), b"original");
    }
    #[test]
    fn enabled_auto_destination_and_review_events_are_protected() {
        let f = Fixture::new();
        f.posted();
        f.db.execute("INSERT INTO profiles(id,name,platform,url,role,tags) VALUES(3,'dest2','instagram','https://www.instagram.com/dest2','destination','[]')",[]).unwrap();
        f.db.execute("INSERT INTO auto_queue_rules(profile_id,enabled,interval_seconds,max_pending,filters) VALUES(3,1,3600,5,'{}')",[]).unwrap();
        assert!(!f.eligible("source", &f.root, PATH));
        f.db.execute(
            "UPDATE auto_queue_rules SET enabled=0 WHERE profile_id=3",
            [],
        )
        .unwrap();
        f.db.execute("INSERT INTO posting_queue_events(queue_id,event_type,state) VALUES(1,'needs_review','posted')",[]).unwrap();
        assert!(!f.eligible("source", &f.root, PATH));
        f.db.execute(
            "INSERT INTO posting_queue_events(queue_id,event_type,state) VALUES(1,'note','posted')",
            [],
        )
        .unwrap();
        assert!(!f.eligible("source", &f.root, PATH));
        f.db.execute("INSERT INTO posting_queue_events(queue_id,event_type,previous_state,state,post_url) VALUES(1,'updated','queued','posted','https://www.tiktok.com/@dest/video/1')",[]).unwrap();
        assert!(f.eligible("source", &f.root, PATH));
    }
    #[test]
    fn old_cache_entry_with_new_inode_is_protected() {
        let mut f = Fixture::new();
        f.posted();
        f.preview("0123456789abcdef.jpg");
        let p = f.dir.join("0123456789abcdef.jpg");
        fs::remove_file(&p).unwrap();
        fs::write(&p, b"different user image").unwrap();
        f.sweep();
        assert_eq!(fs::read(&p).unwrap(), b"different user image");
    }
    #[test]
    fn no_cleanup_when_any_link_unknown() {
        let mut f = Fixture::new();
        f.posted();
        f.preview("0123456789abcdef.jpg");
        f.db.execute("INSERT INTO thumbnail_cache_links SELECT scope,name,'destination',2,root,path,source_dev,source_ino,source_size,source_mtime,source_mtime_ns FROM thumbnail_cache_links WHERE scope=? AND name='0123456789abcdef.jpg'",params![SCOPE]).unwrap();
        f.sweep();
        assert!(f.dir.join("0123456789abcdef.jpg").exists());
    }
    #[test]
    fn cloud_markers_and_symlinked_cache_directories_fail_closed() {
        use std::{
            ffi::CString,
            os::unix::{
                ffi::OsStrExt,
                fs::{PermissionsExt, symlink},
            },
        };
        let f = Fixture::new();
        let parent = fs::canonicalize(&f.dir).unwrap();
        let child = parent.join("cloud-marker");
        fs::create_dir(&child).unwrap();
        fs::set_permissions(&child, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(platform::fixture_private_child(&parent, "cloud-marker").is_some());
        let p = CString::new(child.as_os_str().as_bytes()).unwrap();
        let n = CString::new("com.google.drivefs.item-id#S").unwrap();
        assert_eq!(
            unsafe { libc::setxattr(p.as_ptr(), n.as_ptr(), b"test".as_ptr().cast(), 4, 0, 0) },
            0
        );
        assert!(!platform::fixture_no_cloud_marker(&child));
        assert!(platform::fixture_private_child(&parent, "cloud-marker").is_none());
        let target = f.dir.join("target");
        fs::create_dir(&target).unwrap();
        symlink(&target, f.dir.join("redirect")).unwrap();
        assert!(platform::fixture_private_child(&parent, "redirect").is_none());
        assert!(target.exists());
    }
    #[test]
    fn managed_cache_file_lock_excludes_a_second_writer() {
        let f = Fixture::new();
        let parent = fs::canonicalize(&f.dir).unwrap();
        let first = platform::fixture_acquire(&parent, SCOPE).unwrap();
        assert!(platform::fixture_acquire(&parent, SCOPE).is_none());
        drop(first);
        assert!(platform::fixture_acquire(&parent, SCOPE).is_some());
    }
    #[test]
    fn generated_file_with_later_provider_marker_is_preserved() {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let mut f = Fixture::new();
        f.posted();
        f.preview("0123456789abcdef.jpg");
        let p = f.dir.join("0123456789abcdef.jpg");
        let c = CString::new(p.as_os_str().as_bytes()).unwrap();
        let n = CString::new("com.google.drivefs.item-id#S").unwrap();
        assert_eq!(
            unsafe { libc::setxattr(c.as_ptr(), n.as_ptr(), b"test".as_ptr().cast(), 4, 0, 0) },
            0
        );
        f.sweep();
        assert!(p.exists());
    }
    #[test]
    fn posted_without_matching_activity_history_is_unknown() {
        let f = Fixture::new();
        f.posted();
        f.db.execute("DELETE FROM posting_queue_events", [])
            .unwrap();
        assert!(!f.eligible("source", &f.root, PATH));
    }
    #[test]
    fn pending_symlink_and_hardlink_aliases_and_missing_paths_stop_cleanup() {
        use std::os::unix::fs::symlink;
        let mut f = Fixture::new();
        f.posted();
        f.preview("0123456789abcdef.jpg");
        let root = Path::new(&f.root);
        symlink("video.mp4", root.join("account/symlink.mp4")).unwrap();
        fs::hard_link(root.join(PATH), root.join("account/hardlink.mp4")).unwrap();
        f.db.execute("INSERT INTO posting_queue(id,profile_id,source_id,platform,identity_key,root,path,state) VALUES(2,2,1,'tiktok','post:2',?,'account/symlink.mp4','queued')", [&f.root]).unwrap();
        f.sweep();
        assert!(f.dir.join("0123456789abcdef.jpg").exists());
        f.db.execute(
            "UPDATE posting_queue SET path='account/hardlink.mp4' WHERE id=2",
            [],
        )
        .unwrap();
        f.sweep();
        assert!(f.dir.join("0123456789abcdef.jpg").exists());
        f.db.execute(
            "UPDATE posting_queue SET path='account/missing.mp4' WHERE id=2",
            [],
        )
        .unwrap();
        assert!(active_queue_identities(&f.db).unwrap().is_none());
        f.sweep();
        assert!(f.dir.join("0123456789abcdef.jpg").exists());
        f.db.execute("DELETE FROM posting_queue WHERE id=2", [])
            .unwrap();
        f.sweep();
        assert!(!f.dir.join("0123456789abcdef.jpg").exists());
    }
    #[test]
    fn source_changed_after_registration_or_report_is_protected() {
        let mut f = Fixture::new();
        f.posted();
        f.preview("0123456789abcdef.jpg");
        fs::write(
            Path::new(&f.root).join(PATH),
            b"different synthetic replacement",
        )
        .unwrap();
        f.sweep();
        assert!(f.dir.join("0123456789abcdef.jpg").exists());
        // Even a fresh registration cannot use an older posted report as proof.
        f.preview("1111111111111111.jpg");
        f.db.execute(
            "UPDATE posting_queue_events SET created_at='2000-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
        f.sweep();
        assert!(f.dir.join("1111111111111111.jpg").exists());
    }
    #[test]
    fn source_root_symlink_is_compared_by_its_resolved_location() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        f.posted();
        let alias = f.dir.join("source-root-alias");
        symlink(&f.root, &alias).unwrap();
        f.db.execute(
            "UPDATE settings SET value=? WHERE key='source_root'",
            [alias.to_string_lossy().as_ref()],
        )
        .unwrap();
        assert!(f.eligible("source", &f.root, PATH));
    }
    #[test]
    fn cache_hits_require_unchanged_registered_preview_and_source() {
        let mut f = Fixture::new();
        f.preview("0123456789abcdef.jpg");
        let preview = f.dir.join("0123456789abcdef.jpg");
        let source = f.identity();
        let preview_identity = inspect_registration(&preview, &f.root, PATH, &source).unwrap();
        assert!(
            record(
                &mut f.db,
                SCOPE,
                "0123456789abcdef.jpg",
                &preview_identity,
                "source",
                1,
                &f.root,
                PATH,
                &source,
                false
            )
            .unwrap()
        );
        assert!(
            f.db.query_row("SELECT last_used>1 FROM thumbnail_cache_entries", [], |r| r
                .get::<_, bool>(0))
                .unwrap()
        );
        fs::write(&preview, b"unregistered replacement contents").unwrap();
        let changed_identity = inspect_registration(&preview, &f.root, PATH, &source).unwrap();
        assert!(
            !record(
                &mut f.db,
                SCOPE,
                "0123456789abcdef.jpg",
                &changed_identity,
                "source",
                1,
                &f.root,
                PATH,
                &source,
                false
            )
            .unwrap()
        );
        assert_eq!(
            fs::read(&preview).unwrap(),
            b"unregistered replacement contents"
        );
    }
    #[test]
    fn startup_and_period_delays_are_bounded() {
        assert_eq!(INITIAL_DELAY_SECONDS, 60);
        assert_eq!(PERIOD_SECONDS, 6 * 60 * 60);
    }
    #[test]
    fn work_per_sweep_is_bounded() {
        let mut f = Fixture::new();
        f.posted();
        for i in 0..120 {
            f.preview(&format!("{:016x}.jpg", i));
        }
        f.sweep();
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM thumbnail_cache_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            20
        );
    }
    #[test]
    fn too_many_outstanding_queue_entries_skip_the_pass() {
        let mut f = Fixture::new();
        f.posted();
        f.preview("0123456789abcdef.jpg");
        for i in 0..=ACTIVE_QUEUE_LIMIT {
            f.db.execute("INSERT INTO posting_queue(profile_id,source_id,platform,identity_key,root,path,state) VALUES(2,1,'tiktok',?,?,?,'queued')", params![format!("extra:{i}"), f.root, PATH]).unwrap();
        }
        assert!(active_queue_identities(&f.db).unwrap().is_none());
        f.sweep();
        assert!(f.dir.join("0123456789abcdef.jpg").exists());
    }
    #[test]
    fn slow_inspection_keeps_db_available_and_db_changes_cancel_cleanup() {
        use std::sync::{Arc, Mutex, mpsc};
        let mut f = Fixture::new();
        f.posted();
        let base = fs::canonicalize(&f.dir).unwrap();
        let cache = platform::fixture_acquire(&base, SCOPE).unwrap();
        let preview = cache.dir.join("0123456789abcdef.jpg");
        fs::write(&preview, b"synthetic preview").unwrap();
        let source = f.identity();
        let identity = inspect_registration(&preview, &f.root, PATH, &source).unwrap();
        record(
            &mut f.db,
            SCOPE,
            "0123456789abcdef.jpg",
            &identity,
            "source",
            1,
            &f.root,
            PATH,
            &source,
            true,
        )
        .unwrap();
        f.db.execute("UPDATE thumbnail_cache_entries SET last_used=1", [])
            .unwrap();
        drop(cache);
        fixture_cache_base(Some(base));
        let db = std::mem::replace(&mut f.db, Connection::open_in_memory().unwrap());
        let app = App {
            db: Arc::new(Mutex::new(db)),
            media: f.dir.clone(),
            log_dir: f.dir.clone(),
            cache_scope: SCOPE.into(),
        };
        let (started_tx, started_rx) = mpsc::channel();
        let (continue_tx, continue_rx) = mpsc::channel();
        let other = app.clone();
        let worker = std::thread::spawn(move || {
            sweep_with(&other, |snapshot, inspector, scope, current, offset| {
                started_tx.send(()).unwrap();
                continue_rx.recv().unwrap();
                prepare_plan(&snapshot, &inspector, &scope, current, offset).unwrap()
            });
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        {
            let db = app
                .db
                .try_lock()
                .expect("DB was locked during filesystem inspection");
            db.execute("UPDATE posting_queue SET state='queued' WHERE id=1", [])
                .unwrap();
        }
        continue_tx.send(()).unwrap();
        worker.join().unwrap();
        assert!(
            preview.exists(),
            "queue changed after snapshot, so cleanup must stop"
        );
        fixture_cache_base(None);
    }
    #[test]
    fn final_local_recheck_protects_replaced_preview() {
        let mut f = Fixture::new();
        f.posted();
        f.preview("0123456789abcdef.jpg");
        let guard = platform::fixture_guard(&f.dir).unwrap();
        let inspector = platform::inspector(&guard).unwrap();
        let (snapshot, stamp) = snapshot_db(&f.db).unwrap().unwrap();
        let plan = prepare_plan(&snapshot, &inspector, SCOPE, MAX_AGE + MIN_IDLE + 2, 0)
            .unwrap()
            .unwrap();
        assert_eq!(plan.removals.len(), 1);
        let path = f.dir.join("0123456789abcdef.jpg");
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"user replacement must stay").unwrap();
        assert!(
            apply_one(
                &mut f.db,
                &guard,
                &inspector,
                SCOPE,
                stamp,
                &plan.removals[0]
            )
            .unwrap()
            .is_some()
        );
        assert_eq!(fs::read(&path).unwrap(), b"user replacement must stay");
    }
    #[test]
    fn queue_change_between_files_stops_next_transaction() {
        let mut f = Fixture::new();
        f.posted();
        f.preview("0000000000000001.jpg");
        f.preview("0000000000000002.jpg");
        let guard = platform::fixture_guard(&f.dir).unwrap();
        let inspector = platform::inspector(&guard).unwrap();
        let (snapshot, stamp) = snapshot_db(&f.db).unwrap().unwrap();
        let plan = prepare_plan(&snapshot, &inspector, SCOPE, MAX_AGE + MIN_IDLE + 2, 0)
            .unwrap()
            .unwrap();
        assert_eq!(plan.removals.len(), 2);
        let next = apply_one(
            &mut f.db,
            &guard,
            &inspector,
            SCOPE,
            stamp,
            &plan.removals[0],
        )
        .unwrap()
        .unwrap();
        f.db.execute("UPDATE posting_queue SET state='queued' WHERE id=1", [])
            .unwrap();
        assert!(
            apply_one(
                &mut f.db,
                &guard,
                &inspector,
                SCOPE,
                next,
                &plan.removals[1]
            )
            .unwrap()
            .is_none()
        );
        assert!(f.dir.join(&plan.removals[1].name).exists());
    }
    #[test]
    fn other_sqlite_connection_change_cancels_cleanup() {
        let mut f = Fixture::new();
        f.posted();
        f.preview("0123456789abcdef.jpg");
        let guard = platform::fixture_guard(&f.dir).unwrap();
        let inspector = platform::inspector(&guard).unwrap();
        let file = f.dir.join("synthetic-shared.sqlite");
        let mut live = Connection::open(&file).unwrap();
        {
            let backup = rusqlite::backup::Backup::new(&f.db, &mut live).unwrap();
            assert!(matches!(
                backup.step(-1).unwrap(),
                rusqlite::backup::StepResult::Done
            ));
        }
        let external = Connection::open(&file).unwrap();
        let (snapshot, stamp) = snapshot_db(&live).unwrap().unwrap();
        let plan = prepare_plan(&snapshot, &inspector, SCOPE, MAX_AGE + MIN_IDLE + 2, 0)
            .unwrap()
            .unwrap();
        assert_eq!(plan.removals.len(), 1);
        external
            .execute("UPDATE posting_queue SET state='queued' WHERE id=1", [])
            .unwrap();
        assert!(
            apply_one(
                &mut live,
                &guard,
                &inspector,
                SCOPE,
                stamp,
                &plan.removals[0]
            )
            .unwrap()
            .is_none()
        );
        assert!(f.dir.join("0123456789abcdef.jpg").exists());
    }
    #[test]
    fn timed_out_inspection_cannot_apply_and_does_not_spawn_repeated_workers() {
        use std::{
            sync::{atomic::Ordering, mpsc},
            time::{Duration, Instant},
        };
        let (release_tx, release_rx) = mpsc::channel();
        let result = plan_with_deadline(
            move || {
                release_rx.recv().unwrap();
                Some(Plan {
                    removals: Vec::new(),
                    examined: 0,
                })
            },
            Duration::from_millis(30),
        );
        assert!(result.is_none());
        assert!(INSPECTION_BUSY.load(Ordering::Acquire));
        assert!(
            plan_with_deadline(
                || Some(Plan {
                    removals: Vec::new(),
                    examined: 0
                }),
                Duration::from_millis(30)
            )
            .is_none()
        );
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while INSPECTION_BUSY.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!INSPECTION_BUSY.load(Ordering::Acquire));
    }
    #[test]
    fn preview_error_messages_do_not_include_paths_or_urls() {
        for issue in [
            PreviewIssue::SourceChanged,
            PreviewIssue::SourceUnavailable,
            PreviewIssue::CacheFileUntrusted,
            PreviewIssue::Database,
            PreviewIssue::CacheWrite,
            PreviewIssue::SlotsFull,
            PreviewIssue::CacheUnavailable,
        ] {
            let message = issue.message();
            assert!(!message.contains('/'));
            assert!(!message.contains('\\'));
            assert!(!message.contains(":"));
        }
    }
}
