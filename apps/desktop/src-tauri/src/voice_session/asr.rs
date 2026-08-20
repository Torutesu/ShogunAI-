//! ASR warm-up and explicit microphone-permission request boundary.

use tauri::AppHandle;

use super::lifecycle::LANE;

pub(super) fn preload_asr_bg(app: &AppHandle) {
    let app = app.clone();
    let allow_personal_dictionary_keyterms = LANE
        .lock()
        .ok()
        .and_then(|lane| {
            lane.as_ref()
                .map(|lane| lane.settings.share_personal_dictionary_with_speech_provider)
        })
        .unwrap_or(false);
    std::thread::spawn(move || {
        if let Err(error) = crate::voice_lane::preload_asr(&app, allow_personal_dictionary_keyterms)
        {
            eprintln!("[voice] asr preload failed: {error}");
        } else {
            eprintln!("[voice] dictation ASR ready");
        }
    });
}

/// Prompt only from the explicit Settings action, never from the UI thread. The probe opens and
/// immediately stops a local stream; it does not retain or send audio.
pub(super) fn request_microphone_access_bg() {
    std::thread::spawn(|| match crate::voice_lane::request_microphone_access() {
        Ok(()) => eprintln!("[voice] microphone access ready"),
        Err(error) => eprintln!("[voice] microphone access unavailable: {error}"),
    });
}
