//! Memory API opt-in, profile, and bearer verifier storage.
//!
//! `memory_api.json` contains no secrets. Bearers live only in the macOS Keychain; this module
//! serializes SHA-256 verifiers so a process can authenticate without persisting plaintext tokens.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub const TOKEN_VERIFIER_PREFIX: &str = "sha256:";
pub const TOKENS_KEYCHAIN_ACCOUNT: &str = "memory-api-tokens";
pub const SETTINGS_FILE: &str = "memory_api.json";
pub const SETTINGS_ENV: &str = "SHOGUN_MEMORY_API_SETTINGS";
pub const MAX_PROFILE_FIELD_BYTES: usize = 4_096;
pub const MAX_TOKEN_NAME_BYTES: usize = 120;
pub const MAX_TOKENS: usize = 64;

/// Hash a bearer before persistence. The plaintext bearer must never be serialized.
pub fn token_verifier(token: &str) -> String {
    format!(
        "{TOKEN_VERIFIER_PREFIX}{:x}",
        Sha256::digest(token.as_bytes())
    )
}

fn valid_verifier(verifier: &str) -> bool {
    verifier
        .strip_prefix(TOKEN_VERIFIER_PREFIX)
        .is_some_and(|hex| hex.len() == 64 && hex.as_bytes().iter().all(u8::is_ascii_hexdigit))
}

/// Decode a persisted SHA-256 verifier. Never accepts a raw bearer.
pub fn persisted_verifier_bytes(verifier: &str) -> Result<[u8; 32], String> {
    let Some(hex) = verifier.strip_prefix(TOKEN_VERIFIER_PREFIX) else {
        return Err("invalid token verifier".into());
    };
    if hex.len() != 64 || !hex.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return Err("invalid token verifier".into());
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub prefs: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub profile: Profile,
}

/// A Keychain token record. `secret` exists only while migrating a legacy blob or immediately
/// after minting; custom serialization guarantees it never reaches Keychain/disk output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedToken {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub secret: String,
    pub verifier: String,
}

impl serde::Serialize for IssuedToken {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut value = serializer.serialize_struct("IssuedToken", 4)?;
        value.serialize_field("id", &self.id)?;
        value.serialize_field("name", &self.name)?;
        value.serialize_field("created_at", &self.created_at)?;
        value.serialize_field("verifier", &self.verifier)?;
        value.end()
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
        if wire.id.is_empty()
            || wire.id.len() > 128
            || wire.name.trim().is_empty()
            || wire.name.len() > MAX_TOKEN_NAME_BYTES
        {
            return Err(serde::de::Error::custom("invalid token metadata"));
        }
        let (secret, verifier) = match (wire.verifier, wire.secret) {
            (Some(verifier), None) if valid_verifier(&verifier) => (String::new(), verifier),
            (None, Some(secret)) if !secret.is_empty() && secret.len() <= 512 => {
                let verifier = token_verifier(&secret);
                (secret, verifier)
            }
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

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct TokenBlob {
    #[serde(default)]
    pub tokens: Vec<IssuedToken>,
}

pub fn resolve_settings_path(db_path: &str) -> PathBuf {
    if let Ok(path) = std::env::var(SETTINGS_ENV) {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    Path::new(db_path)
        .parent()
        .map(|parent| parent.join(SETTINGS_FILE))
        .unwrap_or_else(|| PathBuf::from(SETTINGS_FILE))
}

/// Unreadable/missing settings are disabled: external API remains fail-closed.
pub fn load_settings(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_settings(path: &Path, settings: &Settings) -> Result<(), String> {
    validate_profile(&settings.profile)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create memory API settings directory: {error}"))?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(SETTINGS_FILE);
    let temp = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    std::fs::write(&temp, json)
        .map_err(|error| format!("save memory API settings temp file: {error}"))?;
    std::fs::rename(&temp, path).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        format!("replace memory API settings: {error}")
    })
}

pub fn require_enabled(path: &Path) -> Result<Settings, String> {
    if !path.exists() {
        return Err("Memory API is disabled. Enable it in SHOGUN Settings.".into());
    }
    let settings = load_settings(path);
    if !settings.enabled {
        return Err("Memory API is disabled. Enable it in SHOGUN Settings.".into());
    }
    Ok(settings)
}

pub fn validate_profile(profile: &Profile) -> Result<(), String> {
    for value in [&profile.display_name, &profile.role, &profile.prefs] {
        if value.len() > MAX_PROFILE_FIELD_BYTES {
            return Err("profile field exceeds 4096 bytes".into());
        }
        if value.chars().any(char::is_control) {
            return Err("profile field contains control characters".into());
        }
    }
    Ok(())
}

pub fn parse_token_blob_with_migration(bytes: &[u8]) -> Result<(TokenBlob, bool), String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "invalid token blob".to_string())?;
    let Some(tokens) = value.get("tokens").and_then(serde_json::Value::as_array) else {
        return Err("invalid token blob".into());
    };
    if tokens.len() > MAX_TOKENS {
        return Err("too many token records".into());
    }
    let migrated = tokens
        .iter()
        .any(|token| token.get("secret").is_some() && token.get("verifier").is_none());
    let blob: TokenBlob =
        serde_json::from_value(value).map_err(|_| "invalid token blob".to_string())?;
    Ok((blob, migrated))
}

pub fn serialize_token_blob(blob: &TokenBlob) -> Result<Vec<u8>, String> {
    if blob.tokens.len() > MAX_TOKENS {
        return Err("too many token records".into());
    }
    serde_json::to_vec(blob).map_err(|error| error.to_string())
}

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
    fn default_is_disabled_and_bearer_never_serializes() {
        assert!(!Settings::default().enabled);
        let raw = "shogun_secret";
        let bytes = serialize_token_blob(&TokenBlob {
            tokens: vec![IssuedToken {
                id: "id".into(),
                name: "client".into(),
                created_at: 1,
                secret: raw.into(),
                verifier: token_verifier(raw),
            }],
        })
        .unwrap();
        assert!(!String::from_utf8(bytes).unwrap().contains(raw));
    }

    #[test]
    fn legacy_plaintext_migrates_and_invalid_data_fails_closed() {
        let raw =
            br#"{"tokens":[{"id":"id","name":"client","created_at":1,"secret":"old-secret"}]}"#;
        let (blob, migrated) = parse_token_blob_with_migration(raw).unwrap();
        assert!(migrated);
        assert_eq!(blob.tokens[0].verifier, token_verifier("old-secret"));
        assert!(parse_token_blob_with_migration(b"{}").is_err());
    }

    #[test]
    fn profile_and_token_metadata_have_bounds() {
        assert!(validate_profile(&Profile {
            prefs: "x".repeat(MAX_PROFILE_FIELD_BYTES + 1),
            ..Profile::default()
        })
        .is_err());
        let invalid = format!(
            r#"{{\"tokens\":[{{\"id\":\"id\",\"name\":\"{}\",\"created_at\":1,\"verifier\":\"{}\"}}]}}"#,
            "x".repeat(MAX_TOKEN_NAME_BYTES + 1),
            token_verifier("s")
        );
        assert!(parse_token_blob_with_migration(invalid.as_bytes()).is_err());
    }
}
