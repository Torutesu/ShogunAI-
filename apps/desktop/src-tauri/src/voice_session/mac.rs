use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use shogun_core::daemon::Db;
use shogun_memory::voice_terms::{NewVoiceTerm, VoiceTerm};

#[path = "asr.rs"]
pub mod asr;
#[path = "consent.rs"]
pub mod consent;
#[path = "dictionary.rs"]
pub mod dictionary;
#[path = "editor.rs"]
pub mod editor;
#[path = "insertion.rs"]
pub mod insertion;
#[path = "lifecycle.rs"]
pub mod lifecycle;

pub use consent::Settings;
pub use editor::VoiceEditSettingsView;

#[tauri::command]
pub fn get_voice_settings() -> Settings {
    consent::get_voice_settings()
}

#[tauri::command]
pub fn set_voice_enabled(enabled: bool, app: AppHandle) -> Result<(), String> {
    consent::set_voice_enabled(enabled, app)
}

#[tauri::command]
pub fn set_voice_dictionary_egress_consent(consent: bool, app: AppHandle) -> Result<(), String> {
    self::consent::set_voice_dictionary_egress_consent(consent, app)
}

#[tauri::command(async)]
pub fn get_voice_microphones() -> Result<Vec<String>, String> {
    consent::get_voice_microphones()
}

#[tauri::command]
pub fn set_voice_microphone(microphone: Option<String>, app: AppHandle) -> Result<(), String> {
    consent::set_voice_microphone(microphone, app)
}

#[tauri::command]
pub fn voice_dismiss(app: AppHandle) {
    lifecycle::voice_dismiss(app)
}

#[tauri::command]
pub fn voice_force_end(app: AppHandle) {
    lifecycle::voice_force_end(app)
}

#[tauri::command]
pub fn get_voice_edit_settings() -> VoiceEditSettingsView {
    editor::get_voice_edit_settings()
}

#[tauri::command]
pub fn set_voice_edit_key(key: String) -> Result<(), String> {
    editor::set_voice_edit_key(key)
}

#[tauri::command]
pub fn clear_voice_edit_key() -> Result<(), String> {
    editor::clear_voice_edit_key()
}

#[tauri::command]
pub fn list_voice_dictionary_terms(db: State<'_, Db>) -> Result<Vec<VoiceTerm>, String> {
    dictionary::list_voice_dictionary_terms(db)
}

#[tauri::command]
pub fn create_voice_dictionary_term(
    term: NewVoiceTerm,
    db: State<'_, Db>,
) -> Result<VoiceTerm, String> {
    dictionary::create_voice_dictionary_term(term, db)
}

#[tauri::command]
pub fn update_voice_dictionary_term(
    id: i64,
    term: NewVoiceTerm,
    db: State<'_, Db>,
) -> Result<VoiceTerm, String> {
    dictionary::update_voice_dictionary_term(id, term, db)
}

#[tauri::command]
pub fn delete_voice_dictionary_term(id: i64, db: State<'_, Db>) -> Result<bool, String> {
    dictionary::delete_voice_dictionary_term(id, db)
}

#[derive(Clone, Serialize)]
pub struct VoiceStateEvent {
    pub phase: &'static str,
    pub transcript: Option<String>,
    pub response: Option<String>,
}

pub(super) fn emit_state(
    app: &AppHandle,
    phase: &'static str,
    transcript: Option<String>,
    response: Option<String>,
) {
    let _ = app.emit(
        "voice_state",
        VoiceStateEvent {
            phase,
            transcript,
            response,
        },
    );
}

pub(super) fn emit_error(app: &AppHandle, message: impl Into<String>) {
    emit_state(app, "error", None, Some(message.into()));
    crate::sound::mac::play(shogun_core::sound::Cue::VoiceFailed);
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
