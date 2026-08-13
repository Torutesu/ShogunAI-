//! Memory API settings + token issuance (Settings → Memory API).
//!
//! Enable gate + profile live in `memory_api.json` next to `memory.db`.
//! Issued bearer tokens live in the Keychain (`memory-api-tokens` blob) — never a file/DB/log
//! (invariant 7). Soft Pro gate: `enabled` is the product gate until Stripe WP5.1; trial is
//! Pro-equivalent.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde::Serialize;
    use shogun_integrations::keychain_store;
    use shogun_mcp::memory_api_settings::{
        self, IssuedToken, Profile, Settings, TokenBlob, TOKENS_KEYCHAIN_ACCOUNT,
    };
    use tauri::Manager;

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    }

    fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        app.path()
            .app_data_dir()
            .ok()
            .map(|d| crate::memory_data_dir(d).join(memory_api_settings::SETTINGS_FILE))
    }

    fn load(app: &tauri::AppHandle) -> Settings {
        settings_path(app)
            .as_deref()
            .map(memory_api_settings::load_settings)
            .unwrap_or_default()
    }

    fn save(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
        let Some(path) = settings_path(app) else {
            return Err("app data dir unavailable".into());
        };
        memory_api_settings::save_settings(&path, settings)
    }

    fn load_token_blob() -> Result<TokenBlob, String> {
        memory_api_settings::load_token_blob_with_migration(
            || match keychain_store::get_generic_secret(TOKENS_KEYCHAIN_ACCOUNT) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(error) if error.code() == -25300 => Ok(None),
                Err(error) => Err(format!("read Memory API token blob: {error}")),
            },
            |bytes| {
                keychain_store::set_generic_secret(TOKENS_KEYCHAIN_ACCOUNT, bytes)
                    .map_err(|e| e.to_string())
            },
        )
    }

    fn save_token_blob(blob: &TokenBlob) -> Result<(), String> {
        let bytes = memory_api_settings::serialize_token_blob(blob)?;
        keychain_store::set_generic_secret(TOKENS_KEYCHAIN_ACCOUNT, &bytes)
            .map_err(|e| e.to_string())
    }

    fn mint_secret() -> String {
        let mut buf = [0u8; 24];
        // getrandom is already a desktop dep (DB key mint).
        getrandom::getrandom(&mut buf).ok();
        let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
        format!("shogun_{hex}")
    }

    fn mint_id() -> String {
        let mut buf = [0u8; 8];
        getrandom::getrandom(&mut buf).ok();
        buf.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Token metadata returned to the UI (never includes the secret after issue).
    #[derive(Debug, Clone, Serialize)]
    pub struct TokenMeta {
        pub id: String,
        pub name: String,
        pub created_at: i64,
    }

    /// Settings view for the Memory API section.
    #[derive(Debug, Clone, Serialize)]
    pub struct MemoryApiSettingsView {
        pub enabled: bool,
        pub profile: Profile,
        pub tokens: Vec<TokenMeta>,
        /// Soft Pro gate note for the UI (Stripe WP5.1 TODO).
        pub gate_note: String,
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct IssuedTokenView {
        pub id: String,
        pub name: String,
        pub created_at: i64,
        /// Shown once at issue time — never returned by `memory_api_settings` again.
        pub token: String,
    }

    #[tauri::command]
    pub fn memory_api_settings(app: tauri::AppHandle) -> MemoryApiSettingsView {
        let settings = load(&app);
        let tokens = load_token_blob()
            .unwrap_or_default()
            .tokens
            .into_iter()
            .map(|t| TokenMeta {
                id: t.id,
                name: t.name,
                created_at: t.created_at,
            })
            .collect();
        MemoryApiSettingsView {
            enabled: settings.enabled,
            profile: settings.profile,
            tokens,
            gate_note: "Memory API is a Pro feature. The Enable toggle is the product gate until billing (WP5.1). Trial is Pro-equivalent — turning this on during trial is allowed.".into(),
        }
    }

    #[tauri::command]
    pub fn set_memory_api_enabled(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
        let mut settings = load(&app);
        settings.enabled = enabled;
        save(&app, &settings)?;
        eprintln!(
            "[memory_api] {}",
            if enabled {
                "enabled"
            } else {
                "disabled (fail closed for MCP/API)"
            }
        );
        Ok(())
    }

    #[tauri::command]
    pub fn set_memory_api_profile(
        display_name: String,
        role: String,
        prefs: String,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        let mut settings = load(&app);
        settings.profile = Profile {
            display_name: display_name.trim().to_string(),
            role: role.trim().to_string(),
            prefs: prefs.trim().to_string(),
        };
        save(&app, &settings)
    }

    #[tauri::command]
    pub fn issue_memory_api_token(
        name: String,
        app: tauri::AppHandle,
    ) -> Result<IssuedTokenView, String> {
        let _ = app; // path not needed; tokens are Keychain-only
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("token name is empty".into());
        }
        let id = mint_id();
        let secret = mint_secret();
        let created_at = now_ms();
        let mut blob = load_token_blob()?;
        blob.tokens.push(IssuedToken {
            id: id.clone(),
            name: name.clone(),
            created_at,
            secret: secret.clone(),
            verifier: memory_api_settings::token_verifier(&secret),
        });
        save_token_blob(&blob)?;
        eprintln!("[memory_api] token issued id={id}");
        Ok(IssuedTokenView {
            id,
            name,
            created_at,
            token: secret,
        })
    }

    #[tauri::command]
    pub fn revoke_memory_api_token(id: String) -> Result<(), String> {
        let mut blob = load_token_blob()?;
        let before = blob.tokens.len();
        blob.tokens.retain(|t| t.id != id);
        if blob.tokens.len() == before {
            return Err("token not found".into());
        }
        if blob.tokens.is_empty() {
            match keychain_store::delete_generic_secret(TOKENS_KEYCHAIN_ACCOUNT) {
                Ok(()) => {}
                Err(e) if e.code() == -25300 => {}
                Err(e) => return Err(e.to_string()),
            }
        } else {
            save_token_blob(&blob)?;
        }
        eprintln!("[memory_api] token revoked id={id}");
        Ok(())
    }
}
