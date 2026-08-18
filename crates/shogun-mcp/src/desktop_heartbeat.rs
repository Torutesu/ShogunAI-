//! Desktop liveness marker for headless L3 preflight. A stale marker never authorizes a send.
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const FILE_NAME: &str = "desktop_heartbeat";
pub const MAX_AGE_MS: u64 = 4_000;
pub fn resolve_path(db_path: &str) -> PathBuf {
    Path::new(db_path)
        .parent()
        .map(|dir| dir.join(FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(FILE_NAME))
}
pub fn write(path: &Path, now_ms: u64) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "cannot create heartbeat directory".to_string())?;
    }
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    #[cfg(unix)]
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    #[cfg(unix)]
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|_| "cannot write desktop heartbeat".to_string())?;
    #[cfg(not(unix))]
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temp)
        .map_err(|_| "cannot write desktop heartbeat".to_string())?;
    #[cfg(unix)]
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| "cannot protect desktop heartbeat".to_string())?;
    file.write_all(now_ms.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "cannot persist desktop heartbeat".to_string())?;
    fs::rename(temp, path).map_err(|_| "cannot replace desktop heartbeat".to_string())?;
    Ok(())
}
pub fn fresh(path: &Path, now_ms: u64) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .is_some_and(|stamp| now_ms.saturating_sub(stamp) <= MAX_AGE_MS)
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}
