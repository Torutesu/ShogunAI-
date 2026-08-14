//! Cross-process desktop liveness signal used by standalone MCP.

use std::path::{Path, PathBuf};

/// Heartbeat file shared by desktop and MCP processes.
pub const FILE_NAME: &str = "desktop_heartbeat";
/// Desktop refreshes every second; four seconds covers brief scheduler stalls.
pub const MAX_AGE_MS: i64 = 4_000;

/// Resolve heartbeat beside the memory database unless explicitly overridden.
pub fn resolve_path(db_path: &str) -> PathBuf {
    std::env::var("SHOGUN_DESKTOP_HEARTBEAT")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            Path::new(db_path)
                .parent()
                .map(|parent| parent.join(FILE_NAME))
        })
        .unwrap_or_else(|| PathBuf::from(FILE_NAME))
}

/// Write one heartbeat atomically. Content is wall-clock milliseconds; no user data or secrets.
pub fn write(path: &Path, now_ms: i64) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temporary, now_ms.to_string())?;
    std::fs::rename(temporary, path)
}

/// Missing, unreadable, future-dated, or stale heartbeat means desktop is unavailable.
pub fn is_fresh(path: &Path, now_ms: i64, max_age_ms: i64) -> bool {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(updated_ms) = raw.trim().parse::<i64>() else {
        return false;
    };
    let Some(age) = now_ms.checked_sub(updated_ms) else {
        return false;
    };
    (0..=max_age_ms).contains(&age)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("shogun-heartbeat-{}-{label}", std::process::id()))
    }

    #[test]
    fn missing_and_stale_heartbeats_are_not_fresh() {
        let path = test_path("stale");
        let _ = std::fs::remove_file(&path);
        assert!(!is_fresh(&path, 10_000, MAX_AGE_MS));
        write(&path, 1_000).unwrap();
        assert!(!is_fresh(&path, 10_000, MAX_AGE_MS));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn recent_heartbeat_is_fresh() {
        let path = test_path("fresh");
        write(&path, 9_000).unwrap();
        assert!(is_fresh(&path, 10_000, MAX_AGE_MS));
        let _ = std::fs::remove_file(path);
    }
}
