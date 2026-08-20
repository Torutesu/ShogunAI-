//! Settings commands for the explicitly opt-in Memory API.
//!
//! Profiles are local non-secret preferences. Token metadata and SHA-256 verifiers are Keychain
//! only; plaintext bearer is returned exactly once from `issue_memory_api_token`.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde::Serialize;
    use shogun_integrations::keychain_store;
    use shogun_mcp::memory_api_settings::{
        self, validate_profile, IssuedToken, Profile, Settings, TokenBlob, MAX_TOKENS,
        MAX_TOKEN_NAME_BYTES, TOKENS_KEYCHAIN_ACCOUNT,
    };
    use tauri::Manager;

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    }

    fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        app.path()
            .app_data_dir()
            .ok()
            .map(crate::memory_data_dir)
            .map(|dir| dir.join(memory_api_settings::SETTINGS_FILE))
    }

    fn load(app: &tauri::AppHandle) -> Settings {
        settings_path(app)
            .as_deref()
            .map(memory_api_settings::load_settings)
            .unwrap_or_default()
    }

    fn save(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
        let Some(path) = settings_path(app) else {
            return Err("app data directory unavailable".into());
        };
        memory_api_settings::save_settings(&path, settings)
    }

    fn load_token_blob() -> Result<TokenBlob, String> {
        memory_api_settings::load_token_blob_with_migration(
            || match keychain_store::get_generic_secret(TOKENS_KEYCHAIN_ACCOUNT) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(error) if error.code() == -25300 => Ok(None),
                Err(_) => Err("could not read Memory API tokens from Keychain".to_string()),
            },
            |bytes| {
                keychain_store::set_generic_secret(TOKENS_KEYCHAIN_ACCOUNT, bytes)
                    .map_err(|_| "could not migrate Memory API token verifiers".to_string())
            },
        )
    }

    fn save_token_blob(blob: &TokenBlob) -> Result<(), String> {
        let bytes = memory_api_settings::serialize_token_blob(blob)?;
        keychain_store::set_generic_secret(TOKENS_KEYCHAIN_ACCOUNT, &bytes)
            .map_err(|_| "could not save Memory API token".to_string())
    }

    fn random_hex(bytes: usize) -> Result<String, String> {
        let mut raw = vec![0u8; bytes];
        getrandom::getrandom(&mut raw)
            .map_err(|_| "secure random generation failed".to_string())?;
        Ok(raw.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct TokenMeta {
        pub id: String,
        pub name: String,
        pub created_at: i64,
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct MemoryApiSettingsView {
        pub enabled: bool,
        pub profile: Profile,
        pub tokens: Vec<TokenMeta>,
        pub gate_note: String,
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct IssuedTokenView {
        pub id: String,
        pub name: String,
        pub created_at: i64,
        pub token: String,
    }

    #[tauri::command]
    pub fn memory_api_settings(app: tauri::AppHandle) -> MemoryApiSettingsView {
        let settings = load(&app);
        let tokens = load_token_blob()
            .unwrap_or_default()
            .tokens
            .into_iter()
            .map(|token| TokenMeta {
                id: token.id,
                name: token.name,
                created_at: token.created_at,
            })
            .collect();
        MemoryApiSettingsView {
            enabled: settings.enabled,
            profile: settings.profile,
            tokens,
            gate_note: "Requires active Pro or trial and this explicit opt-in. Turning it off stops standalone MCP and REST at startup.".into(),
        }
    }

    #[tauri::command]
    pub fn set_memory_api_enabled(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
        let mut settings = load(&app);
        settings.enabled = enabled;
        save(&app, &settings)
    }

    #[tauri::command]
    pub fn set_memory_api_profile(
        display_name: String,
        role: String,
        prefs: String,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        let profile = Profile {
            display_name: display_name.trim().into(),
            role: role.trim().into(),
            prefs: prefs.trim().into(),
        };
        validate_profile(&profile)?;
        let mut settings = load(&app);
        settings.profile = profile;
        save(&app, &settings)
    }

    #[tauri::command]
    pub fn issue_memory_api_token(
        name: String,
        app: tauri::AppHandle,
    ) -> Result<IssuedTokenView, String> {
        let name = name.trim().to_string();
        if name.is_empty()
            || name.len() > MAX_TOKEN_NAME_BYTES
            || name.chars().any(char::is_control)
        {
            return Err("token name must be 1-120 printable bytes".into());
        }
        let mut blob = load_token_blob()?;
        if blob.tokens.len() >= MAX_TOKENS {
            return Err("token limit reached".into());
        }
        let id = random_hex(8)?;
        let secret = format!("shogun_{}", random_hex(24)?);
        let created_at = now_ms();
        blob.tokens.push(IssuedToken {
            id: id.clone(),
            name: name.clone(),
            created_at,
            verifier: memory_api_settings::token_verifier(&secret),
            secret: String::new(),
        });
        save_token_blob(&blob)?;
        let _ = app;
        Ok(IssuedTokenView {
            id,
            name,
            created_at,
            token: secret,
        })
    }

    #[tauri::command]
    pub fn revoke_memory_api_token(id: String) -> Result<(), String> {
        if id.is_empty() || id.len() > 128 {
            return Err("invalid token id".into());
        }
        let mut blob = load_token_blob()?;
        let before = blob.tokens.len();
        blob.tokens.retain(|token| token.id != id);
        if before == blob.tokens.len() {
            return Err("token not found".into());
        }
        if blob.tokens.is_empty() {
            match keychain_store::delete_generic_secret(TOKENS_KEYCHAIN_ACCOUNT) {
                Ok(()) => {}
                Err(error) if error.code() == -25300 => {}
                Err(_) => return Err("could not revoke Memory API token".into()),
            }
        } else {
            save_token_blob(&blob)?;
        }
        Ok(())
    }
}
