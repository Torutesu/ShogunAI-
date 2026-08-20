//! Persisted voice settings and explicit personal-dictionary egress consent.

use tauri::{AppHandle, Manager};

use super::asr;
use super::emit_state;
use super::lifecycle::{cancel_active_session, stop_cancelled_audio, LANE};

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub microphone: Option<String>,
    /// Explicit opt-in for sending personal vocabulary as speech-provider keyterm hints. Local
    /// correction remains enabled when false; older `voice.json` files default closed.
    #[serde(default)]
    pub share_personal_dictionary_with_speech_provider: bool,
}

fn normalize_microphone_selection(microphone: Option<String>) -> Option<String> {
    microphone.filter(|name| !name.trim().is_empty())
}

fn settings_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|directory| directory.join("voice.json"))
}

pub(super) fn load_settings(app: &AppHandle) -> Settings {
    settings_path(app)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app).ok_or("voice settings unavailable")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "voice settings unavailable")?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|_| "voice settings unavailable")?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, json).map_err(|_| "voice settings unavailable")?;
    std::fs::rename(&temporary, &path).map_err(|_| "voice settings unavailable")?;
    Ok(())
}

pub(super) fn get_voice_settings() -> Settings {
    LANE.lock()
        .ok()
        .and_then(|lane| lane.as_ref().map(|lane| lane.settings.clone()))
        .unwrap_or_default()
}

pub(super) fn set_voice_enabled(enabled: bool, app: AppHandle) -> Result<(), String> {
    let mut lane = LANE
        .lock()
        .map_err(|_| "voice lane lock poisoned".to_string())?;
    let settings = lane.as_mut().ok_or("voice not initialized")?;
    let mut next = settings.settings.clone();
    next.enabled = enabled;
    save_settings(&app, &next)?;
    let microphone = next.microphone.clone();
    settings.settings = next;
    drop(lane);
    if enabled {
        asr::preload_asr_bg(&app);
        asr::request_microphone_access_bg(microphone);
        eprintln!("[voice] enabled={enabled}");
        return Ok(());
    }
    if let Some(audio) = cancel_active_session() {
        stop_cancelled_audio(audio);
    }
    emit_state(&app, "idle", None, None);
    eprintln!("[voice] enabled={enabled}");
    Ok(())
}

pub(super) fn get_voice_microphones() -> Result<Vec<String>, String> {
    shogun_core::audio::capture::mic::input_device_names()
}

pub(super) fn set_voice_microphone(
    microphone: Option<String>,
    app: AppHandle,
) -> Result<(), String> {
    let mut lane = LANE
        .lock()
        .map_err(|_| "voice lane lock poisoned".to_string())?;
    let settings = lane.as_mut().ok_or("voice not initialized")?;
    let mut next = settings.settings.clone();
    next.microphone = normalize_microphone_selection(microphone);
    save_settings(&app, &next)?;
    settings.settings = next;
    Ok(())
}

/// Stores consent only for speech-provider keyterms. Dictionary correction remains local either way.
pub(super) fn set_voice_dictionary_egress_consent(
    consent: bool,
    app: AppHandle,
) -> Result<(), String> {
    let mut lane = LANE
        .lock()
        .map_err(|_| "voice lane lock poisoned".to_string())?;
    let settings = lane.as_mut().ok_or("voice not initialized")?;
    let mut next = settings.settings.clone();
    next.share_personal_dictionary_with_speech_provider = consent;
    save_settings(&app, &next)?;
    let voice_enabled = next.enabled;
    settings.settings = next;
    drop(lane);
    if voice_enabled {
        asr::preload_asr_bg(&app);
    }
    Ok(())
}
