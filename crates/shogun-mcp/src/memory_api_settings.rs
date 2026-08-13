//! Memory API enable gate + profile prefs (`memory_api.json`).
//!
//! Soft Pro gate for now: the `enabled` toggle is the product gate (Stripe plan WP5.1 TODO).
//! Trial is Pro-equivalent — do **not** block on a "trial" string alone. Fail closed when
//! `enabled` is false or the settings file is missing / unreadable as disabled.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub const TOKEN_VERIFIER_PREFIX: &str = "sha256:";

/// Hash bearer once. Plain bearer never serialized; digest is safe to persist.
pub fn token_verifier(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("{TOKEN_VERIFIER_PREFIX}{digest:x}")
}

fn valid_verifier(verifier: &str) -> bool {
    verifier
        .strip_prefix(TOKEN_VERIFIER_PREFIX)
        .is_some_and(|hex| hex.len() == 64 && hex.as_bytes().iter().all(u8::is_ascii_hexdigit))
}

pub fn persisted_verifier_bytes(verifier: &str) -> Result<[u8; 32], String> {
    let hex = verifier
        .strip_prefix(TOKEN_VERIFIER_PREFIX)
        .filter(|hex| hex.len() == 64)
        .filter(|hex| hex.as_bytes().iter().all(u8::is_ascii_hexdigit))
        .ok_or_else(|| "invalid token verifier".to_string())?;
    let mut out = [0u8; 32];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        let high = (pair[0] as char)
            .to_digit(16)
            .ok_or_else(|| "invalid token verifier".to_string())?;
        let low = (pair[1] as char)
            .to_digit(16)
            .ok_or_else(|| "invalid token verifier".to_string())?;
        out[index] = ((high << 4) | low) as u8;
    }
    Ok(out)
}

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
        Self {
            enabled: false,
            profile: Profile::default(),
        }
    }
}

/// One issued API token. `secret` is populated only for a newly issued or legacy token in RAM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedToken {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    /// Raw bearer only for legacy in-process callers; never serialized or logged.
    pub secret: String,
    pub verifier: String,
}

impl serde::Serialize for IssuedToken {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut out = serializer.serialize_struct("IssuedToken", 4)?;
        out.serialize_field("id", &self.id)?;
        out.serialize_field("name", &self.name)?;
        out.serialize_field("created_at", &self.created_at)?;
        out.serialize_field("verifier", &self.verifier)?;
        out.end()
    }
}

impl<'de> serde::Deserialize<'de> for IssuedToken {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Wire {
            id: String,
            name: String,
            created_at: i64,
            verifier: Option<String>,
            secret: Option<String>,
        }
        let wire = Wire::deserialize(deserializer)?;
        let (secret, verifier) = match (wire.verifier, wire.secret) {
            (Some(v), None) if valid_verifier(&v) => (String::new(), v),
            (None, Some(raw)) if !raw.is_empty() => (raw.clone(), token_verifier(&raw)),
            _ => return Err(serde::de::Error::custom("invalid token verifier")),
        };
        Ok(Self {
            id: wire.id,
            name: wire.name,
            created_at: wire.created_at,
            secret,
            verifier,
        })
    }
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

/// Parse a Keychain / JSON token blob. Invalid data fails closed.
pub fn parse_token_blob(bytes: &[u8]) -> Result<TokenBlob, String> {
    parse_token_blob_with_migration(bytes).map(|(blob, _)| blob)
}

/// Parse blob and report legacy plaintext migration. Invalid data fails closed.
pub fn parse_token_blob_with_migration(bytes: &[u8]) -> Result<(TokenBlob, bool), String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "invalid token blob".to_string())?;
    if !value.get("tokens").is_some_and(serde_json::Value::is_array) {
        return Err("invalid token blob".into());
    }
    let migrated = value
        .get("tokens")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|tokens| {
            tokens
                .iter()
                .any(|t| t.get("secret").is_some() && t.get("verifier").is_none())
        });
    let blob: TokenBlob =
        serde_json::from_value(value).map_err(|_| "invalid token blob".to_string())?;
    Ok((blob, migrated))
}

/// Serialize tokens for Keychain storage.
pub fn serialize_token_blob(blob: &TokenBlob) -> Result<Vec<u8>, String> {
    serde_json::to_vec(blob).map_err(|e| e.to_string())
}

/// Load token storage and durably migrate legacy plaintext entries before returning them.
/// `None` means no Keychain item; malformed data and rewrite failures abort loading.
pub fn load_token_blob_with_migration<Read, Write>(
    read: Read,
    write: Write,
) -> Result<TokenBlob, String>
where
    Read: FnOnce() -> Result<Option<Vec<u8>>, String>,
    Write: FnOnce(&[u8]) -> Result<(), String>,
{
    let Some(bytes) = read()? else {
        return Ok(TokenBlob::default());
    };
    let (blob, migrated) = parse_token_blob_with_migration(&bytes)?;
    if migrated {
        let rewritten = serialize_token_blob(&blob)?;
        write(&rewritten)?;
    }
    Ok(blob)
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

        let on = Settings {
            enabled: true,
            profile: Profile {
                display_name: "A".into(),
                ..Default::default()
            },
        };
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
            resolve_settings_path(
                "/Users/x/Library/Application Support/com.selectkk.shogun/memory.db"
            ),
            PathBuf::from(
                "/Users/x/Library/Application Support/com.selectkk.shogun/memory_api.json"
            )
        );
        assert_eq!(
            resolve_settings_path("memory.db"),
            PathBuf::from(SETTINGS_FILE)
        );
    }

    #[test]
    fn serialization_stores_verifier_not_bearer() {
        let raw = "shogun_test_secret";
        let blob = TokenBlob {
            tokens: vec![IssuedToken {
                id: "id".into(),
                name: "n".into(),
                created_at: 1,
                secret: raw.into(),
                verifier: token_verifier(raw),
            }],
        };
        let bytes = serialize_token_blob(&blob).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains(raw));
        assert!(text.contains("sha256:"));
    }

    #[test]
    fn legacy_plaintext_migrates_to_digest_and_invalid_fails_closed() {
        let old = br#"{"tokens":[{"id":"id","name":"n","created_at":1,"secret":"old-secret"}]}"#;
        let (blob, migrated) = parse_token_blob_with_migration(old).unwrap();
        assert!(migrated);
        assert_eq!(blob.tokens[0].verifier, token_verifier("old-secret"));
        assert!(String::from_utf8(serialize_token_blob(&blob).unwrap())
            .unwrap()
            .contains("verifier"));
        assert!(!parse_token_blob_with_migration(b"{}").is_ok());
        assert!(parse_token_blob(b"not-json").is_err());
    }

    #[test]
    fn shared_loader_aborts_on_malformed_blob_without_rewrite() {
        let result = load_token_blob_with_migration(
            || Ok(Some(b"not-json".to_vec())),
            |_| panic!("malformed blob must not rewrite"),
        );
        assert_eq!(result, Err("invalid token blob".into()));
    }

    #[test]
    fn shared_loader_rewrites_legacy_blob_without_plaintext() {
        let raw = br#"{"tokens":[{"id":"id","name":"n","created_at":1,"secret":"old-secret"}]}"#;
        let result = load_token_blob_with_migration(
            || Ok(Some(raw.to_vec())),
            |rewritten| {
                let text = std::str::from_utf8(rewritten).unwrap();
                assert!(!text.contains("old-secret"));
                assert!(text.contains(&token_verifier("old-secret")));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(result.tokens[0].secret, "old-secret");
    }

    #[test]
    fn shared_loader_fails_closed_when_legacy_rewrite_fails() {
        let raw = br#"{"tokens":[{"id":"id","name":"n","created_at":1,"secret":"old-secret"}]}"#;
        let result = load_token_blob_with_migration(
            || Ok(Some(raw.to_vec())),
            |_| Err("keychain unavailable".into()),
        );
        assert_eq!(result, Err("keychain unavailable".into()));
    }

    #[test]
    fn shared_loader_propagates_keychain_read_errors() {
        let result = load_token_blob_with_migration(
            || Err("keychain locked".into()),
            |_| panic!("read failure must not rewrite"),
        );
        assert_eq!(result, Err("keychain locked".into()));
    }

    #[test]
    fn shared_loader_treats_only_missing_item_as_empty() {
        let result = load_token_blob_with_migration(
            || Ok(None),
            |_| panic!("missing item must not rewrite"),
        )
        .unwrap();
        assert!(result.tokens.is_empty());
    }
}
