//! Memory API enable gate + profile prefs (`memory_api.json`).
//!
//! Soft Pro gate for now: the `enabled` toggle is the product gate (Stripe plan WP5.1 TODO).
//! Trial is Pro-equivalent — do **not** block on a "trial" string alone. Fail closed when
//! `enabled` is false or the settings file is missing / unreadable as disabled.

use std::path::{Path, PathBuf};

/// Keychain account for the Memory API token blob (desktop + macOS bins).
pub const TOKENS_KEYCHAIN_ACCOUNT: &str = "memory-api-tokens";

/// Filename next to `memory.db` / app-data.
pub const SETTINGS_FILE: &str = "memory_api.json";

/// Env override for the settings file path (standalone `shogun-mcp` / `shogun-api`).
pub const SETTINGS_ENV: &str = "SHOGUN_MEMORY_API_SETTINGS";

/// User-owned profile text surfaced by `profile.whoami` / `profile.set`.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub prefs: String,
}

/// Persisted Memory API settings. Default `enabled: false` (fail closed).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub profile: Profile,
}

impl Default for Settings {
    fn default() -> Self {
        Self { enabled: false, profile: Profile::default() }
    }
}

/// One issued API token. `secret` is the bearer value (stored in Keychain, never returned after issue).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IssuedToken {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub secret: String,
}

/// Keychain / file blob shape for issued tokens.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct TokenBlob {
    #[serde(default)]
    pub tokens: Vec<IssuedToken>,
}

/// Resolve `memory_api.json`: `SHOGUN_MEMORY_API_SETTINGS`, else next to the DB path's parent.
pub fn resolve_settings_path(db_path: &str) -> PathBuf {
    if let Ok(p) = std::env::var(SETTINGS_ENV) {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    Path::new(db_path)
        .parent()
        .map(|p| p.join(SETTINGS_FILE))
        .unwrap_or_else(|| PathBuf::from(SETTINGS_FILE))
}

/// Load settings from `path`, or default (disabled) when missing/unreadable.
pub fn load_settings(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Persist settings (creates parent dirs when possible). Atomic replace via temp file.
pub fn save_settings(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("create memory_api settings directory: {e}"))?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(SETTINGS_FILE);
    let temp_path = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    std::fs::write(&temp_path, json)
        .map_err(|e| format!("save memory_api settings temp file: {e}"))?;
    std::fs::rename(&temp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        format!("replace memory_api settings: {e}")
    })
}

/// Fail-closed gate for `shogun-mcp` / `shogun-api` boot. Returns `Ok(settings)` only when enabled.
pub fn require_enabled(path: &Path) -> Result<Settings, String> {
    if !path.exists() {
        return Err(format!(
            "Memory API is disabled (no settings file at {}). Enable it in SHOGUN Settings → Memory API.",
            path.display()
        ));
    }
    let settings = load_settings(path);
    if !settings.enabled {
        return Err(format!(
            "Memory API is disabled (enabled=false in {}). Enable it in SHOGUN Settings → Memory API.",
            path.display()
        ));
    }
    Ok(settings)
}

/// Parse a Keychain / JSON token blob. Empty / invalid → no tokens.
pub fn parse_token_blob(bytes: &[u8]) -> TokenBlob {
    serde_json::from_slice(bytes).unwrap_or_default()
}

/// Serialize tokens for Keychain storage.
pub fn serialize_token_blob(blob: &TokenBlob) -> Result<Vec<u8>, String> {
    serde_json::to_vec(blob).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_disabled() {
        let s = Settings::default();
        assert!(!s.enabled);
        assert!(s.profile.display_name.is_empty());
    }

    #[test]
    fn require_enabled_fails_when_missing_or_disabled() {
        let dir = std::env::temp_dir().join(format!("shogun_mem_api_gate_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SETTINGS_FILE);
        assert!(require_enabled(&path).is_err());

        save_settings(&path, &Settings::default()).unwrap();
        assert!(require_enabled(&path).is_err());

        let on = Settings { enabled: true, profile: Profile { display_name: "A".into(), ..Default::default() } };
        save_settings(&path, &on).unwrap();
        let loaded = require_enabled(&path).unwrap();
        assert!(loaded.enabled);
        assert_eq!(loaded.profile.display_name, "A");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_falls_back_next_to_db() {
        // Clear override if a parallel test left it set.
        std::env::remove_var(SETTINGS_ENV);
        assert_eq!(
            resolve_settings_path("/Users/x/Library/Application Support/com.selectkk.shogun/memory.db"),
            PathBuf::from("/Users/x/Library/Application Support/com.selectkk.shogun/memory_api.json")
        );
        assert_eq!(resolve_settings_path("memory.db"), PathBuf::from(SETTINGS_FILE));
    }
}
