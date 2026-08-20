//! Optional, bounded transcript-editor lane and its BYOK key handling.

use std::time::Duration;

use serde::Serialize;

use shogun_core::daemon::Db;
use shogun_core::llm::openai_compat::{OpenAiCompatAgentClient, OpenAiCompatConfig, GROQ_BASE_URL};
use shogun_core::llm::transport::ReqwestTransport;
use shogun_core::llm::{ByokKey, Secret};
use shogun_integrations::keychain_store;

const VOICE_EDIT_KEY_ACCOUNT: &str = "voice-edit-groq-byok";
const LEGACY_GROQ_KEY_ACCOUNT: &str = "groq-byok";
const VOICE_EDIT_TRACE_PURPOSE: &str = "voice_dictation_cleanup";
pub(super) const VOICE_EDIT_MODEL: &str = "openai/gpt-oss-120b";
const VOICE_EDIT_TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Clone, Serialize)]
pub struct VoiceEditSettingsView {
    pub model: &'static str,
    pub has_key: bool,
}

fn decode_voice_edit_key(bytes: Vec<u8>) -> Option<String> {
    String::from_utf8(bytes)
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

fn voice_edit_key() -> Option<String> {
    if let Some(key) = keychain_store::get_generic_secret(VOICE_EDIT_KEY_ACCOUNT)
        .ok()
        .and_then(decode_voice_edit_key)
    {
        return Some(key);
    }
    let legacy = keychain_store::get_generic_secret(LEGACY_GROQ_KEY_ACCOUNT)
        .ok()
        .and_then(decode_voice_edit_key);
    if let Some(key) = legacy.as_deref() {
        // Settings is an explicit interactive path, so its one-time migration may prompt.
        let _ = keychain_store::set_generic_secret(VOICE_EDIT_KEY_ACCOUNT, key.as_bytes());
    }
    legacy
}

/// Background dictation cleanup must never trigger a Keychain dialog. Settings warms or migrates
/// this value during the user's explicit interactive action.
fn voice_edit_key_non_interactive() -> Option<String> {
    keychain_store::get_generic_secret_non_interactive(VOICE_EDIT_KEY_ACCOUNT)
        .ok()
        .and_then(decode_voice_edit_key)
        .or_else(|| {
            keychain_store::get_generic_secret_non_interactive(LEGACY_GROQ_KEY_ACCOUNT)
                .ok()
                .and_then(decode_voice_edit_key)
        })
}

pub(super) fn block_on_timeout<F>(
    runtime: &tokio::runtime::Runtime,
    duration: Duration,
    future: F,
) -> Result<F::Output, tokio::time::error::Elapsed>
where
    F: std::future::Future,
{
    runtime.block_on(async move { tokio::time::timeout(duration, future).await })
}

pub(super) fn voice_edit_config() -> OpenAiCompatConfig {
    OpenAiCompatConfig::new(GROQ_BASE_URL, VOICE_EDIT_MODEL)
        .with_max_tokens(512)
        .with_reasoning_effort("low")
        .with_include_reasoning(false)
}

/// Optional, bounded BYOK cleanup. Every failure returns `None`, leaving local correction or raw
/// ASR intact. Traceability records egress without transcript content.
pub(super) fn edit_dictation(
    transcript: &str,
    protected_terms: &[String],
    db: &Db,
) -> Option<String> {
    let key = voice_edit_key_non_interactive()?;
    let user = crate::voice_editor::edit_user_message(transcript)?;
    let transport = ReqwestTransport::new().ok()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    let client = OpenAiCompatAgentClient::new(
        transport,
        db.traceability_sink(),
        ByokKey::new(Secret::new(key)),
        voice_edit_config(),
    );
    match block_on_timeout(
        &runtime,
        VOICE_EDIT_TIMEOUT,
        client.complete_split_with_purpose(
            crate::voice_editor::SYSTEM_PROMPT,
            &user,
            VOICE_EDIT_TRACE_PURPOSE,
        ),
    ) {
        Ok(Ok(edited))
            if crate::voice_editor::output_is_valid_with_protected(
                transcript,
                &edited,
                protected_terms,
            ) =>
        {
            Some(edited.trim().to_string())
        }
        _ => None,
    }
}

pub(super) fn get_voice_edit_settings() -> VoiceEditSettingsView {
    VoiceEditSettingsView {
        model: VOICE_EDIT_MODEL,
        has_key: voice_edit_key().is_some(),
    }
}

pub(super) fn set_voice_edit_key(key: String) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("key is empty".into());
    }
    keychain_store::set_generic_secret(VOICE_EDIT_KEY_ACCOUNT, key.as_bytes())
        .map_err(|error| error.to_string())
}

pub(super) fn clear_voice_edit_key() -> Result<(), String> {
    clear_voice_edit_accounts(keychain_store::delete_generic_secret, |error| {
        error.code() == -25300 /* errSecItemNotFound */
    })
    .map_err(|error| error.to_string())
}

/// Revoke both account names. Older installs can retain a legacy background fallback until removed.
pub(super) fn clear_voice_edit_accounts<E>(
    mut delete: impl FnMut(&str) -> Result<(), E>,
    is_not_found: impl Fn(&E) -> bool,
) -> Result<(), E> {
    for account in [VOICE_EDIT_KEY_ACCOUNT, LEGACY_GROQ_KEY_ACCOUNT] {
        if let Err(error) = delete(account) {
            if !is_not_found(&error) {
                return Err(error);
            }
        }
    }
    Ok(())
}
