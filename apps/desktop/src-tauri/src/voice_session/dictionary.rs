//! Local voice-dictionary lookup and its Tauri command boundary.

use tauri::State;

use shogun_core::daemon::Db;
use shogun_core::voice_dictionary::{DictionaryContext, DictionaryCorrection, VoiceDictionary};
use shogun_memory::voice_terms::{NewVoiceTerm, VoiceTerm};

use super::insertion::DictationTarget;

pub(super) fn dictionary_edit_candidate(
    transcript: &str,
    dictionary: &VoiceDictionary,
    context: &DictionaryContext,
) -> DictionaryCorrection {
    dictionary.correct(transcript, context)
}

/// macOS launches inherit locale through process environment. Normalize only BCP-47-safe tags;
/// `C`/POSIX and malformed values deliberately disable locale-scoped terms rather than guessing.
pub(super) fn normalize_process_locale(value: &str) -> Option<String> {
    let value = value.trim().split(['.', '@']).next()?.replace('_', "-");
    if value.eq_ignore_ascii_case("c") || value.eq_ignore_ascii_case("posix") {
        return None;
    }
    let parts = value
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let language = parts.first()?;
    if !language.chars().all(|ch| ch.is_ascii_alphabetic()) || !(2..=3).contains(&language.len()) {
        return None;
    }
    let mut normalized = language.to_ascii_lowercase();
    for part in parts.iter().skip(1) {
        if part.len() == 2 && part.chars().all(|ch| ch.is_ascii_alphabetic()) {
            normalized.push('-');
            normalized.push_str(&part.to_ascii_uppercase());
            break;
        }
        if part.len() == 3 && part.chars().all(|ch| ch.is_ascii_digit()) {
            normalized.push('-');
            normalized.push_str(part);
            break;
        }
    }
    Some(normalized)
}

fn current_dictation_locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|name| {
        std::env::var(name)
            .ok()
            .and_then(|value| normalize_process_locale(&value))
    })
}

pub(super) fn dictionary_context_for_locale(
    target: Option<&DictationTarget>,
    locale: Option<String>,
) -> DictionaryContext {
    DictionaryContext {
        locale,
        bundle_id: target.and_then(|target| target.bundle_id.clone()),
        surface: Some("voice_dictation".into()),
    }
}

pub(super) fn dictionary_context(target: Option<&DictationTarget>) -> DictionaryContext {
    dictionary_context_for_locale(target, current_dictation_locale())
}

pub(super) fn list_voice_dictionary_terms(db: State<'_, Db>) -> Result<Vec<VoiceTerm>, String> {
    db.list_voice_terms()
}

pub(super) fn create_voice_dictionary_term(
    term: NewVoiceTerm,
    db: State<'_, Db>,
) -> Result<VoiceTerm, String> {
    db.create_voice_term(&term)
}

pub(super) fn update_voice_dictionary_term(
    id: i64,
    term: NewVoiceTerm,
    db: State<'_, Db>,
) -> Result<VoiceTerm, String> {
    db.update_voice_term(id, &term)?
        .ok_or_else(|| "voice dictionary term not found".to_string())
}

pub(super) fn delete_voice_dictionary_term(id: i64, db: State<'_, Db>) -> Result<bool, String> {
    db.delete_voice_term(id)
}
