use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use core_foundation_sys::base::CFRange;

use super::consent::Settings;
use super::dictionary::{
    dictionary_context_for_locale, dictionary_edit_candidate, normalize_process_locale,
};
use super::editor::{
    block_on_timeout, clear_voice_edit_accounts, voice_edit_config, VOICE_EDIT_MODEL,
};
use super::insertion::{
    cancel_delivery_fence, expected_insert, paste_target_attributes, valid_collapsed_caret,
    writable_attributes, DeliveryFence, DELIVERY_CANCELLED, DELIVERY_READY, DELIVERY_WRITING,
};

#[test]
fn personal_dictionary_egress_defaults_closed() {
    assert!(!Settings::default().share_personal_dictionary_with_speech_provider);
}

#[test]
fn collapsed_caret_is_required_for_dictation_capture() {
    assert!(valid_collapsed_caret("hello", CFRange::init(2, 0)));
    assert!(!valid_collapsed_caret("hello", CFRange::init(2, 1)));
    assert!(!valid_collapsed_caret("hello", CFRange::init(9, 0)));
}

#[test]
fn dictation_insert_preserves_adjacent_utf16_text() {
    assert_eq!(
        expected_insert("a🙂b", CFRange::init(1, 0), " hello"),
        Some("a hello🙂b".into())
    );
}

#[test]
fn writable_target_requires_enabled_editable_and_settable_attributes() {
    assert!(writable_attributes(Some(true), Some(true), true));
    assert!(!writable_attributes(None, Some(true), true));
    assert!(!writable_attributes(Some(true), None, true));
    assert!(!writable_attributes(Some(true), Some(false), true));
    assert!(!writable_attributes(Some(true), Some(true), false));
}

#[test]
fn web_editor_without_optional_ax_flags_keeps_a_guarded_paste_target() {
    assert!(paste_target_attributes("AXTextArea", Some(true), None));
    assert!(paste_target_attributes("AXTextField", None, None));
    assert!(!paste_target_attributes(
        "AXSecureTextField",
        Some(true),
        Some(true)
    ));
    assert!(!paste_target_attributes(
        "AXTextArea",
        Some(false),
        Some(true)
    ));
    assert!(!paste_target_attributes(
        "AXTextArea",
        Some(true),
        Some(false)
    ));
}

#[test]
fn cancelled_delivery_gate_never_claims_a_write() {
    let gate = AtomicU8::new(DELIVERY_CANCELLED);
    assert!(gate
        .compare_exchange(
            DELIVERY_READY,
            DELIVERY_WRITING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err());
}

#[test]
fn cancellation_waits_for_shared_fence_for_ax_or_clipboard_delivery() {
    use std::sync::mpsc;

    let fence = Arc::new(DeliveryFence {
        state: AtomicU8::new(DELIVERY_WRITING),
        operation: Mutex::new(()),
    });
    let guard = fence.operation.lock().unwrap();
    let (sent, received) = mpsc::channel();
    let waiting_fence = Arc::clone(&fence);
    std::thread::spawn(move || {
        cancel_delivery_fence(&waiting_fence);
        let _ = sent.send(());
    });
    assert!(received.recv_timeout(Duration::from_millis(20)).is_err());
    drop(guard);
    assert!(received.recv_timeout(Duration::from_secs(1)).is_ok());
}

#[test]
fn voice_edit_config_targets_groq_oss_without_reasoning_output() {
    let config = voice_edit_config();
    assert_eq!(
        config.base_url,
        shogun_core::llm::openai_compat::GROQ_BASE_URL
    );
    assert_eq!(config.model, VOICE_EDIT_MODEL);
    assert_eq!(config.reasoning_effort.as_deref(), Some("low"));
    assert_eq!(config.include_reasoning, Some(false));
}

#[test]
fn dictionary_candidate_preserves_built_in_aliases_for_editing() {
    let dictionary = shogun_core::voice_dictionary::VoiceDictionary::with_defaults();
    let correction = dictionary_edit_candidate(
        "open shogun ai with g rock",
        &dictionary,
        &shogun_core::voice_dictionary::DictionaryContext::default(),
    );
    assert_eq!(correction.text, "open ShogunAI with Groq");
    assert_eq!(correction.protected_terms, vec!["ShogunAI", "Groq"]);
}

#[test]
fn dictionary_context_carries_a_safe_normalized_process_locale() {
    let context = dictionary_context_for_locale(None, normalize_process_locale("en_US.UTF-8"));
    assert_eq!(context.locale.as_deref(), Some("en-US"));
    assert_eq!(normalize_process_locale("C"), None);
    assert_eq!(normalize_process_locale("not a locale"), None);
}

#[test]
fn formatter_timeout_helper_completes_a_ready_future() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    let result = block_on_timeout(&runtime, Duration::from_millis(10), async { 7 });
    assert_eq!(result.ok(), Some(7));
}

#[test]
fn clear_voice_edit_accounts_revokes_current_and_legacy_keys() {
    let mut deleted = Vec::new();
    clear_voice_edit_accounts(
        |account| {
            deleted.push(account.to_string());
            Ok::<_, i32>(())
        },
        |_| false,
    )
    .expect("both key accounts should delete");
    assert_eq!(deleted, vec!["voice-edit-groq-byok", "groq-byok"]);
}

#[test]
fn clear_voice_edit_accounts_ignores_missing_accounts() {
    let mut deleted = Vec::new();
    clear_voice_edit_accounts(
        |account| {
            deleted.push(account.to_string());
            Err::<(), _>(-25300)
        },
        |error| *error == -25300,
    )
    .expect("missing accounts are already revoked");
    assert_eq!(deleted, vec!["voice-edit-groq-byok", "groq-byok"]);
}
