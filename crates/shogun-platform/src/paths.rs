//! App-data directory. Not a secret store.
//!
//! Windows: `%LOCALAPPDATA%\ShogunAI` (per-user, no ProgramData, no roaming of memory files).
//! Linux: `$XDG_DATA_HOME/shogunai` falling back to `~/.local/share/shogunai`.
//!
//! Callers may put logs, the licence token (not a secret — see CLAUDE.md 2026-08-13), and later
//! the encrypted DB here. OAuth tokens and BYOK keys must not.

use std::io;
use std::path::PathBuf;

/// Folder name on Windows — matches the product, and Explorer users expect PascalCase.
pub const WINDOWS_APP_DIR: &str = "ShogunAI";
/// Folder name on Unix — XDG convention is lowercase.
pub const UNIX_APP_DIR: &str = "shogunai";

#[derive(Debug)]
pub enum PathError {
    /// `dirs`-equivalent lookup failed (no LOCALAPPDATA / HOME).
    NoHome,
    Io(io::Error),
}

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHome => f.write_str("no per-user application data directory on this OS"),
            Self::Io(e) => write!(f, "app data directory: {e}"),
        }
    }
}

impl std::error::Error for PathError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::NoHome => None,
        }
    }
}

impl From<io::Error> for PathError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Known path, created or not.
pub fn app_data_dir() -> Result<PathBuf, PathError> {
    let base = data_local_dir().ok_or(PathError::NoHome)?;
    Ok(base.join(app_dir_name()))
}

pub fn app_dir_name() -> &'static str {
    if cfg!(windows) {
        WINDOWS_APP_DIR
    } else {
        UNIX_APP_DIR
    }
}

/// Creates the directory (and parents) if missing. Does not write any file.
pub fn ensure_app_data_dir() -> Result<PathBuf, PathError> {
    let path = app_data_dir()?;
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// `%LOCALAPPDATA%` / `XDG_DATA_HOME` without pulling the `dirs` crate: one less registry crate
/// in a tree that already has three `dirs` versions, and the lookup is a single env var.
fn data_local_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()).map(PathBuf::from)
    } else if let Some(xdg) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(PathBuf::from(xdg))
    } else {
        let home = std::env::var_os("HOME").filter(|v| !v.is_empty())?;
        Some(PathBuf::from(home).join(".local").join("share"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_dir_name_matches_os() {
        if cfg!(windows) {
            assert_eq!(app_dir_name(), "ShogunAI");
        } else {
            assert_eq!(app_dir_name(), "shogunai");
        }
    }

    #[test]
    fn app_data_dir_ends_with_product_folder() {
        let path = app_data_dir().expect("test env has a home/localappdata");
        assert_eq!(path.file_name().and_then(|n| n.to_str()), Some(app_dir_name()));
        let s = path.to_string_lossy();
        assert!(
            !s.contains("Roaming"),
            "Windows app data must be Local, not Roaming: {s}"
        );
    }
}
