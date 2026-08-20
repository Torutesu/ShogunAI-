//! Voice hold-to-talk session: overlay, settings, mic lifecycle, dictation output (#44).
//!
//! On release: Deepgram Nova-3 (when configured) or Whisper fallback → insert at the captured caret, else clipboard → idle.
//! Chat response is deferred; this path is dictation-first per product ask.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use accessibility_sys::{
        kAXEnabledAttribute, kAXErrorSuccess, kAXFocusedUIElementAttribute, kAXIsEditableAttribute,
        kAXRoleAttribute, kAXSelectedTextAttribute, kAXSelectedTextRangeAttribute,
        kAXValueAttribute, kAXValueTypeCFRange, AXUIElementCopyAttributeValue,
        AXUIElementCreateSystemWide, AXUIElementGetPid, AXUIElementIsAttributeSettable,
        AXUIElementRef, AXUIElementSetAttributeValue, AXUIElementSetMessagingTimeout,
        AXValueCreate, AXValueGetTypeID, AXValueGetValue,
    };
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::{CFEqual, CFGetTypeID, CFRange, CFRelease, CFTypeRef};
    use core_foundation_sys::number::{CFBooleanGetTypeID, CFBooleanGetValue, CFBooleanRef};
    use core_foundation_sys::string::{CFStringGetTypeID, CFStringRef};

    use serde::Serialize;
    use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

    use crate::voice_lane::{self, TranscriptOutcome};
    use shogun_core::daemon::Db;
    use shogun_core::llm::openai_compat::{
        OpenAiCompatAgentClient, OpenAiCompatConfig, GROQ_BASE_URL,
    };
    use shogun_core::llm::transport::ReqwestTransport;
    use shogun_core::llm::{ByokKey, Secret};
    use shogun_integrations::keychain_store;

    const WINDOW_LABEL: &str = "voice";
    const VOICE_EDIT_KEY_ACCOUNT: &str = "voice-edit-groq-byok";
    const LEGACY_GROQ_KEY_ACCOUNT: &str = "groq-byok";
    const VOICE_EDIT_TRACE_PURPOSE: &str = "voice_dictation_cleanup";
    const VOICE_EDIT_MODEL: &str = "openai/gpt-oss-120b";
    const VOICE_EDIT_TIMEOUT: Duration = Duration::from_millis(1500);

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: *const std::ffi::c_void,
            keycode: u16,
            key_down: bool,
        ) -> *mut std::ffi::c_void;
        fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);
        fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
    }

    #[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
    pub struct Settings {
        #[serde(default)]
        pub enabled: bool,
        #[serde(default)]
        pub microphone: Option<String>,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum SessionPhase {
        Opening,
        Recording,
        Processing,
        Finishing,
    }

    struct ActiveSession {
        id: u64,
        phase: SessionPhase,
        audio: Option<voice_lane::Handle>,
        target: Option<Arc<DictationTarget>>,
        delivery: Arc<DeliveryFence>,
    }

    struct Lane {
        settings: Settings,
        active: Option<ActiveSession>,
    }

    static LANE: Mutex<Option<Lane>> = Mutex::new(None);

    #[derive(Clone, Serialize)]
    pub struct VoiceStateEvent {
        pub phase: &'static str,
        pub transcript: Option<String>,
        pub response: Option<String>,
    }

    #[derive(Clone, Serialize)]
    pub struct VoiceEditSettingsView {
        pub model: &'static str,
        pub has_key: bool,
    }

    /// Monotonic session id so old ASR workers cannot affect a later hold.
    static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

    const DELIVERY_READY: u8 = 0;
    const DELIVERY_WRITING: u8 = 1;
    const DELIVERY_CANCELLED: u8 = 2;
    const DELIVERY_DONE: u8 = 3;

    struct DictationTarget {
        element: AXUIElementRef,
        pid: i32,
        role: String,
        original_value: String,
        caret: CFRange,
        direct_ax_writable: bool,
        operation: Mutex<()>,
    }

    // SAFETY: the retained AX element is only accessed while `operation` is held. The atomic
    // state linearizes cancellation against the first AX mutation.
    unsafe impl Send for DictationTarget {}
    unsafe impl Sync for DictationTarget {}

    impl Drop for DictationTarget {
        fn drop(&mut self) {
            if !self.element.is_null() {
                unsafe { CFRelease(self.element.cast()) };
            }
        }
    }

    struct DeliveryGuard<'a> {
        state: &'a AtomicU8,
    }

    struct DeliveryFence {
        state: AtomicU8,
        operation: Mutex<()>,
    }

    impl Drop for DeliveryGuard<'_> {
        fn drop(&mut self) {
            self.state.store(DELIVERY_DONE, Ordering::Release);
        }
    }

    struct SessionCleanup {
        session: u64,
    }

    impl Drop for SessionCleanup {
        fn drop(&mut self) {
            abandon_session(self.session);
        }
    }

    fn settings_path(app: &AppHandle) -> Option<std::path::PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join("voice.json"))
    }

    fn load_settings(app: &AppHandle) -> Settings {
        settings_path(app)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save_settings(app: &AppHandle, settings: &Settings) {
        let Some(p) = settings_path(app) else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(settings) {
            if let Err(e) = std::fs::write(&p, json) {
                eprintln!("[voice] settings save failed: {e}");
            }
        }
    }

    fn normalize_microphone_selection(microphone: Option<String>) -> Option<String> {
        microphone.filter(|name| !name.trim().is_empty())
    }

    fn emit_state(
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

    fn emit_error(app: &AppHandle, message: impl Into<String>) {
        let msg = message.into();
        emit_state(app, "error", None, Some(msg));
        // Push-to-talk failing quietly is the worst outcome: the user held a key, said something,
        // and nothing happened (#49, push-to-talk design §5).
        crate::sound::mac::play(shogun_core::sound::Cue::VoiceFailed);
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

    /// Dictation cleanup runs in the background and must never trigger a Keychain dialog. The
    /// interactive settings path above warms or migrates the value when the user connects it.
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

    fn block_on_timeout<F>(
        runtime: &tokio::runtime::Runtime,
        duration: Duration,
        future: F,
    ) -> Result<F::Output, tokio::time::error::Elapsed>
    where
        F: std::future::Future,
    {
        runtime.block_on(async move { tokio::time::timeout(duration, future).await })
    }

    fn voice_edit_config() -> OpenAiCompatConfig {
        OpenAiCompatConfig::new(GROQ_BASE_URL, VOICE_EDIT_MODEL)
            .with_max_tokens(512)
            .with_reasoning_effort("low")
            .with_include_reasoning(false)
    }

    fn dictionary_edit_candidate(
        transcript: &str,
    ) -> shogun_core::voice_dictionary::DictionaryCorrection {
        shogun_core::voice_dictionary::VoiceDictionary::with_defaults().correct(
            transcript,
            &shogun_core::voice_dictionary::DictionaryContext::default(),
        )
    }

    /// Optional, bounded BYOK cleanup. Every failure returns `None`, so the caller keeps the raw
    /// ASR transcript. The trace sink records the Groq egress without logging transcript content.
    fn edit_dictation(transcript: &str, protected_terms: &[String], db: &Db) -> Option<String> {
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

    /// Leave transcript on the general pasteboard (no restore — user wants the text).
    fn copy_to_clipboard(text: &str) -> Result<(), String> {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        use objc2_foundation::NSString;

        let pb: *mut AnyObject = unsafe { msg_send![class!(NSPasteboard), generalPasteboard] };
        if pb.is_null() {
            return Err("no pasteboard".into());
        }
        let utf8 = NSString::from_str("public.utf8-plain-text");
        let ours = NSString::from_str(text);
        let _: isize = unsafe { msg_send![pb, clearContents] };
        let ok: bool = unsafe { msg_send![pb, setString: &*ours, forType: &*utf8] };
        if ok {
            Ok(())
        } else {
            Err("could not write the pasteboard".into())
        }
    }

    fn editable_role(role: &str) -> bool {
        matches!(
            role,
            "AXTextArea" | "AXTextField" | "AXSearchField" | "AXComboBox"
        )
    }

    fn secure_role(role: &str) -> bool {
        role == "AXSecureTextField"
    }

    unsafe fn copy_string(element: AXUIElementRef, name: &str) -> Option<String> {
        let attribute = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe {
            AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
        };
        if error != kAXErrorSuccess || value.is_null() {
            return None;
        }
        unsafe {
            if CFGetTypeID(value) == CFStringGetTypeID() {
                Some(CFString::wrap_under_create_rule(value as CFStringRef).to_string())
            } else {
                CFRelease(value);
                None
            }
        }
    }

    unsafe fn copy_bool(element: AXUIElementRef, name: &str) -> Option<bool> {
        let attribute = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe {
            AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
        };
        if error != kAXErrorSuccess || value.is_null() {
            return None;
        }
        let result = if unsafe { CFGetTypeID(value) == CFBooleanGetTypeID() } {
            Some(unsafe { CFBooleanGetValue(value as CFBooleanRef) })
        } else {
            None
        };
        unsafe { CFRelease(value) };
        result
    }

    unsafe fn selected_text_is_settable(element: AXUIElementRef) -> bool {
        let attribute = CFString::new(kAXSelectedTextAttribute);
        let mut settable = 0u8;
        let result = unsafe {
            AXUIElementIsAttributeSettable(element, attribute.as_concrete_TypeRef(), &mut settable)
        };
        result == kAXErrorSuccess && settable != 0
    }

    unsafe fn target_is_writable(element: AXUIElementRef) -> bool {
        let enabled = unsafe { copy_bool(element, kAXEnabledAttribute) };
        let editable = unsafe { copy_bool(element, kAXIsEditableAttribute) };
        writable_attributes(enabled, editable, unsafe {
            selected_text_is_settable(element)
        })
    }

    fn writable_attributes(enabled: Option<bool>, editable: Option<bool>, settable: bool) -> bool {
        enabled == Some(true) && editable == Some(true) && settable
    }

    /// Web editors commonly omit `AXIsEditable` and selected-text settability even though they
    /// expose a stable text value and caret. That is enough to retain a safe keyboard-paste target;
    /// an explicit disabled/non-editable value still fails closed.
    fn paste_target_attributes(role: &str, enabled: Option<bool>, editable: Option<bool>) -> bool {
        editable_role(role)
            && !secure_role(role)
            && enabled != Some(false)
            && editable != Some(false)
    }

    unsafe fn copy_range(element: AXUIElementRef) -> Option<CFRange> {
        let attribute = CFString::new(kAXSelectedTextRangeAttribute);
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe {
            AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
        };
        if error != kAXErrorSuccess || value.is_null() {
            return None;
        }
        let mut range = CFRange::init(0, 0);
        let valid = unsafe { CFGetTypeID(value) == AXValueGetTypeID() }
            && unsafe {
                AXValueGetValue(
                    value.cast_mut().cast(),
                    kAXValueTypeCFRange,
                    (&mut range as *mut CFRange).cast(),
                )
            };
        unsafe { CFRelease(value) };
        valid.then_some(range)
    }

    unsafe fn focused_element() -> Option<AXUIElementRef> {
        let system = unsafe { AXUIElementCreateSystemWide() };
        if system.is_null() {
            return None;
        }
        unsafe { AXUIElementSetMessagingTimeout(system, 0.25) };
        let attribute = CFString::new(kAXFocusedUIElementAttribute);
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe {
            AXUIElementCopyAttributeValue(system, attribute.as_concrete_TypeRef(), &mut value)
        };
        unsafe { CFRelease(system.cast()) };
        if error != kAXErrorSuccess || value.is_null() {
            return None;
        }
        let element = value.cast_mut().cast();
        unsafe { AXUIElementSetMessagingTimeout(element, 0.25) };
        Some(element)
    }

    fn valid_collapsed_caret(value: &str, range: CFRange) -> bool {
        let Ok(location) = usize::try_from(range.location) else {
            return false;
        };
        range.length == 0 && location <= value.encode_utf16().count()
    }

    fn capture_dictation_target() -> Result<DictationTarget, &'static str> {
        let element = unsafe { focused_element() }.ok_or("no editable text field is focused")?;
        let role = unsafe { copy_string(element, kAXRoleAttribute) }.unwrap_or_default();
        let enabled = unsafe { copy_bool(element, kAXEnabledAttribute) };
        let editable = unsafe { copy_bool(element, kAXIsEditableAttribute) };
        if !paste_target_attributes(&role, enabled, editable) {
            unsafe { CFRelease(element.cast()) };
            return Err("place the caret in an editable text field before dictating");
        }
        let Some(value) = (unsafe { copy_string(element, kAXValueAttribute) }) else {
            unsafe { CFRelease(element.cast()) };
            return Err("the focused text field cannot be verified");
        };
        let Some(caret) = (unsafe { copy_range(element) }) else {
            unsafe { CFRelease(element.cast()) };
            return Err("place a single caret before dictating");
        };
        if !valid_collapsed_caret(&value, caret) {
            unsafe { CFRelease(element.cast()) };
            return Err("dictation only inserts at a single caret; clear the selection first");
        }
        let mut pid = 0i32;
        if unsafe { AXUIElementGetPid(element, &mut pid) } != kAXErrorSuccess || pid <= 0 {
            unsafe { CFRelease(element.cast()) };
            return Err("the focused text field identity cannot be verified");
        }
        if crate::display::frontmost_app().map(|app| app.pid) != Some(pid) {
            unsafe { CFRelease(element.cast()) };
            return Err("the focused application changed before dictation started");
        }
        Ok(DictationTarget {
            element,
            pid,
            role,
            original_value: value,
            caret,
            direct_ax_writable: writable_attributes(enabled, editable, unsafe {
                selected_text_is_settable(element)
            }),
            operation: Mutex::new(()),
        })
    }

    unsafe fn set_caret(element: AXUIElementRef, caret: CFRange) -> bool {
        let value =
            unsafe { AXValueCreate(kAXValueTypeCFRange, (&caret as *const CFRange).cast()) };
        if value.is_null() {
            return false;
        }
        let attribute = CFString::new(kAXSelectedTextRangeAttribute);
        let result = unsafe {
            AXUIElementSetAttributeValue(element, attribute.as_concrete_TypeRef(), value.cast())
        };
        unsafe { CFRelease(value.cast()) };
        result == kAXErrorSuccess
    }

    fn same_focused_element(target: &DictationTarget) -> bool {
        let Some(focused) = (unsafe { focused_element() }) else {
            return false;
        };
        let same = unsafe { CFEqual(focused.cast(), target.element.cast()) };
        unsafe { CFRelease(focused.cast()) };
        same != 0
    }

    fn expected_insert(value: &str, caret: CFRange, transcript: &str) -> Option<String> {
        let units: Vec<u16> = value.encode_utf16().collect();
        let location = usize::try_from(caret.location).ok()?;
        if caret.length != 0 || location > units.len() {
            return None;
        }
        let transcript_units: Vec<u16> = transcript.encode_utf16().collect();
        let mut inserted = Vec::with_capacity(units.len() + transcript_units.len());
        inserted.extend_from_slice(&units[..location]);
        inserted.extend_from_slice(&transcript_units);
        inserted.extend_from_slice(&units[location..]);
        String::from_utf16(&inserted).ok()
    }

    unsafe fn value_matches(element: AXUIElementRef, expected: &str) -> bool {
        for _ in 0..4 {
            if unsafe { copy_string(element, kAXValueAttribute) }.as_deref() == Some(expected) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        false
    }

    fn paste_target_state_matches(
        target: &DictationTarget,
        role: Option<&str>,
        value: Option<&str>,
        caret: Option<CFRange>,
        focused: bool,
        enabled: Option<bool>,
        editable: Option<bool>,
    ) -> bool {
        focused
            && role == Some(target.role.as_str())
            && paste_target_attributes(&target.role, enabled, editable)
            && value == Some(target.original_value.as_str())
            && caret == Some(target.caret)
    }

    /// Paste into the retained target, then restore the user's text clipboard. Caller has already
    /// proven this exact field/value/caret still owns focus and claimed the delivery fence.
    unsafe fn paste_text_at_target(
        target: &DictationTarget,
        transcript: &str,
        expected: &str,
    ) -> Result<(), String> {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        use objc2_foundation::NSString;

        const KVK_ANSI_V: u16 = 0x09;
        const FLAG_COMMAND: u64 = 1 << 20;
        const HID_EVENT_TAP: u32 = 0;

        let pasteboard: *mut AnyObject =
            unsafe { msg_send![class!(NSPasteboard), generalPasteboard] };
        if pasteboard.is_null() {
            return Err("no pasteboard".into());
        }
        let utf8 = NSString::from_str("public.utf8-plain-text");
        let saved: *mut AnyObject = unsafe { msg_send![pasteboard, stringForType: &*utf8] };
        let saved = if saved.is_null() {
            None
        } else {
            let value: *const NSString = saved.cast();
            Some(unsafe { &*value }.to_string())
        };
        let restore = || unsafe {
            let _: isize = msg_send![pasteboard, clearContents];
            if let Some(previous) = &saved {
                let previous = NSString::from_str(previous);
                let _: bool = msg_send![pasteboard, setString: &*previous, forType: &*utf8];
            }
        };

        let ours = NSString::from_str(transcript);
        let _: isize = unsafe { msg_send![pasteboard, clearContents] };
        let wrote: bool = unsafe { msg_send![pasteboard, setString: &*ours, forType: &*utf8] };
        if !wrote {
            restore();
            return Err("could not write the pasteboard".into());
        }

        for key_down in [true, false] {
            let event =
                unsafe { CGEventCreateKeyboardEvent(std::ptr::null(), KVK_ANSI_V, key_down) };
            if event.is_null() {
                restore();
                return Err("could not synthesise the paste".into());
            }
            unsafe {
                CGEventSetFlags(event, FLAG_COMMAND);
                CGEventPost(HID_EVENT_TAP, event);
                CFRelease(event.cast());
            }
        }

        let landed = unsafe { value_matches(target.element, expected) };
        restore();
        landed
            .then_some(())
            .ok_or_else(|| "the paste did not change the captured field".into())
    }

    fn target_state_matches(
        target: &DictationTarget,
        role: Option<&str>,
        value: Option<&str>,
        caret: Option<CFRange>,
        focused: bool,
        writable: bool,
    ) -> bool {
        focused
            && role == Some(target.role.as_str())
            && editable_role(&target.role)
            && !secure_role(&target.role)
            && writable
            && value == Some(target.original_value.as_str())
            && caret == Some(target.caret)
    }

    enum InsertAttempt {
        Cancelled,
        UnsafeBeforeClaim,
        UnsafeAfterClaim,
        Inserted,
    }

    enum PasteAttempt {
        Cancelled,
        UnsafeBeforeClaim,
        FailedAfterClaim(String),
        Inserted,
    }

    fn paste_at_captured_caret(
        session: u64,
        target: &DictationTarget,
        delivery_state: &AtomicU8,
        transcript: &str,
    ) -> PasteAttempt {
        let Ok(_operation) = target.operation.lock() else {
            return PasteAttempt::UnsafeBeforeClaim;
        };
        let mut pid = 0i32;
        if unsafe { AXUIElementGetPid(target.element, &mut pid) } != kAXErrorSuccess
            || pid != target.pid
            || crate::display::frontmost_app().map(|app| app.pid) != Some(target.pid)
            || !paste_target_state_matches(
                target,
                unsafe { copy_string(target.element, kAXRoleAttribute) }.as_deref(),
                unsafe { copy_string(target.element, kAXValueAttribute) }.as_deref(),
                unsafe { copy_range(target.element) },
                same_focused_element(target),
                unsafe { copy_bool(target.element, kAXEnabledAttribute) },
                unsafe { copy_bool(target.element, kAXIsEditableAttribute) },
            )
        {
            return PasteAttempt::UnsafeBeforeClaim;
        }
        let Some(expected) = expected_insert(&target.original_value, target.caret, transcript)
        else {
            return PasteAttempt::UnsafeBeforeClaim;
        };
        if !session_is_processing(session) {
            return PasteAttempt::Cancelled;
        }
        if delivery_state
            .compare_exchange(
                DELIVERY_READY,
                DELIVERY_WRITING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return PasteAttempt::Cancelled;
        }
        // Validation and the session fence are adjacent to the first clipboard mutation. A focus,
        // value, caret, or cancellation change before this claim leaves the external app untouched.
        match unsafe { paste_text_at_target(target, transcript, &expected) } {
            Ok(()) => PasteAttempt::Inserted,
            Err(error) => PasteAttempt::FailedAfterClaim(error),
        }
    }

    /// Directly inserts only into the retained field. Failure intentionally has no paste fallback.
    fn insert_at_captured_caret(
        session: u64,
        target: &DictationTarget,
        delivery_state: &AtomicU8,
        transcript: &str,
    ) -> InsertAttempt {
        let _operation = target
            .operation
            .lock()
            .map_err(|_| InsertAttempt::UnsafeBeforeClaim);
        let Ok(_operation) = _operation else {
            return InsertAttempt::UnsafeBeforeClaim;
        };
        let mut pid = 0i32;
        if unsafe { AXUIElementGetPid(target.element, &mut pid) } != kAXErrorSuccess
            || pid != target.pid
        {
            return InsertAttempt::UnsafeBeforeClaim;
        }
        if !target_state_matches(
            target,
            unsafe { copy_string(target.element, kAXRoleAttribute) }.as_deref(),
            unsafe { copy_string(target.element, kAXValueAttribute) }.as_deref(),
            unsafe { copy_range(target.element) },
            same_focused_element(target),
            unsafe { target_is_writable(target.element) },
        ) {
            return InsertAttempt::UnsafeBeforeClaim;
        }
        if !session_is_processing(session) {
            return InsertAttempt::Cancelled;
        }
        if delivery_state
            .compare_exchange(
                DELIVERY_READY,
                DELIVERY_WRITING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return InsertAttempt::Cancelled;
        }
        // The fresh validation above and this gate are deliberately adjacent to the first AX
        // mutation. A dismiss that wins READY -> CANCELLED leaves the retained element untouched.
        if !unsafe { set_caret(target.element, target.caret) }
            || unsafe { copy_range(target.element) } != Some(target.caret)
        {
            return InsertAttempt::UnsafeAfterClaim;
        }
        let Some(expected) = expected_insert(&target.original_value, target.caret, transcript)
        else {
            return InsertAttempt::UnsafeAfterClaim;
        };
        let attribute = CFString::new(kAXSelectedTextAttribute);
        let value = CFString::new(transcript);
        let _result = unsafe {
            AXUIElementSetAttributeValue(
                target.element,
                attribute.as_concrete_TypeRef(),
                value.as_concrete_TypeRef().cast(),
            )
        };
        if !unsafe { value_matches(target.element, &expected) } {
            return InsertAttempt::UnsafeAfterClaim;
        }
        InsertAttempt::Inserted
    }

    fn session_is_processing(session: u64) -> bool {
        let Ok(lane) = LANE.lock() else {
            return false;
        };
        lane.as_ref()
            .and_then(|lane| lane.active.as_ref())
            .is_some_and(|active| active.id == session && active.phase == SessionPhase::Processing)
    }

    fn claim_terminal(session: u64, expected: SessionPhase) -> bool {
        let Ok(mut lane) = LANE.lock() else {
            return false;
        };
        let Some(lane) = lane.as_mut() else {
            return false;
        };
        if lane
            .active
            .as_ref()
            .is_some_and(|active| active.id == session && active.phase == expected)
        {
            if let Some(active) = lane.active.as_mut() {
                active.phase = SessionPhase::Finishing;
            }
            true
        } else {
            false
        }
    }

    fn clear_terminal(session: u64) {
        let Ok(mut lane) = LANE.lock() else {
            return;
        };
        let Some(lane) = lane.as_mut() else {
            return;
        };
        if lane
            .active
            .as_ref()
            .is_some_and(|active| active.id == session && active.phase == SessionPhase::Finishing)
        {
            lane.active = None;
        }
    }

    fn abandon_session(session: u64) {
        let Ok(mut lane) = LANE.lock() else {
            return;
        };
        let Some(lane) = lane.as_mut() else {
            return;
        };
        if lane
            .active
            .as_ref()
            .is_some_and(|active| active.id == session)
        {
            lane.active = None;
        }
    }

    fn complete_terminal<F>(session: u64, expected: SessionPhase, emit: F) -> bool
    where
        F: FnOnce(),
    {
        if !claim_terminal(session, expected) {
            return false;
        }
        emit();
        clear_terminal(session);
        true
    }

    fn cancel_delivery_fence(delivery: &DeliveryFence) {
        if delivery.state.load(Ordering::Acquire) == DELIVERY_READY {
            let _ = delivery.state.compare_exchange(
                DELIVERY_READY,
                DELIVERY_CANCELLED,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        // This waits both AX writes and clipboard fallback. If cancellation took this lock first,
        // a queued worker observes CANCELLED before it can perform either side effect.
        drop(delivery.operation.lock());
    }

    fn cancel_active_session() -> Option<voice_lane::Handle> {
        let (audio, delivery) = {
            let Ok(mut lane) = LANE.lock() else {
                return None;
            };
            let lane = lane.as_mut()?;
            let mut active = lane.active.take()?;
            (active.audio.take(), Arc::clone(&active.delivery))
        };
        cancel_delivery_fence(&delivery);
        audio
    }

    fn stop_cancelled_audio(audio: voice_lane::Handle) {
        let retained = Arc::new(Mutex::new(Some(audio)));
        let worker = Arc::clone(&retained);
        let spawned = std::thread::Builder::new()
            .name("voice-cancel".into())
            .spawn(move || {
                if let Some(audio) = worker.lock().ok().and_then(|mut audio| audio.take()) {
                    let _ = voice_lane::stop(audio);
                }
            });
        if spawned.is_err() {
            if let Some(audio) = retained.lock().ok().and_then(|mut audio| audio.take()) {
                let _ = voice_lane::stop(audio);
            }
        }
    }

    enum DeliveryOutcome {
        Inserted,
        Copied,
        CopyFailed(String),
    }

    fn claim_clipboard_delivery(
        session: u64,
        delivery: &DeliveryFence,
    ) -> Option<DeliveryGuard<'_>> {
        if !session_is_processing(session) {
            return None;
        }
        if delivery
            .state
            .compare_exchange(
                DELIVERY_READY,
                DELIVERY_WRITING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return None;
        }
        Some(DeliveryGuard {
            state: &delivery.state,
        })
    }

    fn copy_claimed(transcript: &str) -> DeliveryOutcome {
        match copy_to_clipboard(transcript) {
            Ok(()) => DeliveryOutcome::Copied,
            Err(error) => DeliveryOutcome::CopyFailed(error),
        }
    }

    fn deliver_dictation(
        session: u64,
        target: Option<&DictationTarget>,
        delivery: &DeliveryFence,
        transcript: &str,
    ) -> Option<DeliveryOutcome> {
        let _delivery_operation = delivery.operation.lock().ok()?;
        match target {
            Some(target) if !target.direct_ax_writable => {
                match paste_at_captured_caret(session, target, &delivery.state, transcript) {
                    PasteAttempt::Inserted => {
                        let _delivery = DeliveryGuard {
                            state: &delivery.state,
                        };
                        Some(DeliveryOutcome::Inserted)
                    }
                    PasteAttempt::FailedAfterClaim(error) => {
                        eprintln!("[voice] guarded paste failed: {error}; keeping transcript on clipboard");
                        let _delivery = DeliveryGuard {
                            state: &delivery.state,
                        };
                        Some(copy_claimed(transcript))
                    }
                    PasteAttempt::UnsafeBeforeClaim => {
                        let _delivery = claim_clipboard_delivery(session, delivery)?;
                        Some(copy_claimed(transcript))
                    }
                    PasteAttempt::Cancelled => None,
                }
            }
            Some(target) => {
                match insert_at_captured_caret(session, target, &delivery.state, transcript) {
                    InsertAttempt::Inserted => {
                        let _delivery = DeliveryGuard {
                            state: &delivery.state,
                        };
                        Some(DeliveryOutcome::Inserted)
                    }
                    InsertAttempt::UnsafeAfterClaim => {
                        let _delivery = DeliveryGuard {
                            state: &delivery.state,
                        };
                        Some(copy_claimed(transcript))
                    }
                    InsertAttempt::UnsafeBeforeClaim => {
                        let _delivery = claim_clipboard_delivery(session, delivery)?;
                        Some(copy_claimed(transcript))
                    }
                    InsertAttempt::Cancelled => None,
                }
            }
            None => {
                let _delivery = claim_clipboard_delivery(session, delivery)?;
                Some(copy_claimed(transcript))
            }
        }
    }

    fn preload_asr_bg(app: &AppHandle) {
        let app = app.clone();
        std::thread::spawn(move || {
            if let Err(e) = voice_lane::preload_asr(&app) {
                eprintln!("[voice] asr preload failed: {e}");
            } else {
                eprintln!("[voice] dictation ASR ready");
            }
        });
    }

    /// Prompt for microphone access from the explicit Settings action, never from the UI thread.
    /// The probe opens and immediately stops a local stream; it does not retain or send audio.
    fn request_microphone_access_bg(microphone: Option<String>) {
        std::thread::spawn(move || {
            match voice_lane::request_microphone_access(microphone.as_deref()) {
                Ok(()) => eprintln!("[voice] microphone access ready"),
                Err(error) => eprintln!("[voice] microphone access unavailable: {error}"),
            }
        });
    }

    pub fn init(app: &AppHandle) {
        let settings = load_settings(app);
        let enabled_log = settings.enabled;
        let _ = build_overlay(app);
        if let Ok(mut lane) = LANE.lock() {
            *lane = Some(Lane {
                settings: settings.clone(),
                active: None,
            });
        }
        if settings.enabled {
            preload_asr_bg(app);
        }
        eprintln!(
            "[voice] dialogue {}",
            if enabled_log {
                "enabled"
            } else {
                "off (beta default)"
            }
        );
    }

    /// Begin hold-to-talk capture. Returns `true` when the mic lane is live (UI shows recording).
    pub fn on_hold_start(app: AppHandle) -> bool {
        let (enabled, microphone) = LANE
            .lock()
            .ok()
            .and_then(|g| {
                g.as_ref()
                    .map(|lane| (lane.settings.enabled, lane.settings.microphone.clone()))
            })
            .unwrap_or((false, None));
        if !enabled {
            return false;
        }
        if crate::meeting::mac::is_recording() {
            emit_error(
                &app,
                "Voice is unavailable while meeting notes are recording.",
            );
            return false;
        }
        let existing_phase = LANE.lock().ok().and_then(|lane| {
            lane.as_ref()
                .and_then(|lane| lane.active.as_ref().map(|active| active.phase))
        });
        match existing_phase {
            Some(SessionPhase::Recording | SessionPhase::Opening) => return true,
            Some(SessionPhase::Processing | SessionPhase::Finishing) => return false,
            None => {}
        }
        // A missing or selected caret is still a useful recording: keep its transcript on the
        // clipboard, never replace a selection. Web editors that omit optional AX writability
        // attributes retain a guarded target and receive a verified keyboard paste instead.
        let target = match capture_dictation_target() {
            Ok(target) => Some(Arc::new(target)),
            Err(reason) => {
                eprintln!("[voice] no safe insertion target: {reason}");
                None
            }
        };
        // Reserve the lane before opening the mic. Processing remains an owned state, so a
        // second hold can never open a fresh microphone while an older ASR worker is live.
        let session = {
            let mut lane = match LANE.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            let Some(lane) = lane.as_mut() else {
                return false;
            };
            match lane.active.as_ref().map(|active| active.phase) {
                Some(SessionPhase::Recording | SessionPhase::Opening) => return true,
                Some(SessionPhase::Processing | SessionPhase::Finishing) => return false,
                None => {}
            }
            let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
            lane.active = Some(ActiveSession {
                id,
                phase: SessionPhase::Opening,
                audio: None,
                target,
                delivery: Arc::new(DeliveryFence {
                    state: AtomicU8::new(DELIVERY_READY),
                    operation: Mutex::new(()),
                }),
            });
            id
        };

        // BEFORE the mic opens, deliberately (#49 §5). Our own capture cannot pick up a cue that
        // has already played, and meeting recording blocks this path entirely — so the only thing
        // left that could hear it is another app's live call, which the hot-mic rule catches.
        crate::sound::mac::play(shogun_core::sound::Cue::VoiceStart);

        let handle = match voice_lane::start(&app, microphone.as_deref()) {
            Ok(h) => h,
            Err(e) => {
                let _ = complete_terminal(session, SessionPhase::Opening, || emit_error(&app, e));
                return false;
            }
        };

        let mut lane = match LANE.lock() {
            Ok(g) => g,
            Err(_) => {
                // Lane gone — stop the mic we just opened.
                let _ = voice_lane::stop(handle);
                return false;
            }
        };
        let Some(lane) = lane.as_mut() else {
            let _ = voice_lane::stop(handle);
            return false;
        };
        let Some(active) = lane.active.as_mut() else {
            let _ = voice_lane::stop(handle);
            return false;
        };
        if active.id != session || active.phase != SessionPhase::Opening {
            let _ = voice_lane::stop(handle);
            return false;
        }
        active.audio = Some(handle);
        active.phase = SessionPhase::Recording;
        emit_state(&app, "recording", None, None);
        eprintln!("[voice] hold start — mic open");
        true
    }

    /// True when a hold is still live (mic handle or UI recording flag). Used by the release failsafe.
    pub fn is_ui_recording() -> bool {
        let Ok(lane) = LANE.lock() else {
            return false;
        };
        lane.as_ref()
            .and_then(|lane| lane.active.as_ref())
            .is_some_and(|active| active.phase == SessionPhase::Recording)
    }

    /// If still recording 500ms after a release signal, force `on_hold_end` again. Returns true when
    /// it had to act (stuck path).
    pub fn force_end_if_recording(app: AppHandle) -> bool {
        if !is_ui_recording() {
            return false;
        }
        eprintln!("[voice] force_end_if_recording — ending stuck hold");
        on_hold_end(app);
        true
    }

    /// End hold: stop mic → Deepgram or Whisper → dictation inject/clipboard → idle.
    ///
    /// ASR runs on a dedicated thread so the voice-hold worker is never blocked.
    pub fn on_hold_end(app: AppHandle) {
        let (session, audio, target, delivery) = {
            let mut lane = match LANE.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let Some(lane) = lane.as_mut() else {
                return;
            };
            let Some(active) = lane.active.as_mut() else {
                return;
            };
            if active.phase != SessionPhase::Recording {
                return;
            }
            let Some(audio) = active.audio.take() else {
                lane.active = None;
                emit_state(&app, "idle", None, None);
                return;
            };
            active.phase = SessionPhase::Processing;
            (
                active.id,
                audio,
                active.target.clone(),
                Arc::clone(&active.delivery),
            )
        };

        // Signal release to the frontend before ASR so a stuck recording chrome can failsafe.
        let _ = app.emit("voice_hold_released", ());
        // Leave recording immediately so the notch meter cannot stick while ASR runs.
        emit_state(&app, "processing", None, None);
        eprintln!("[voice] hold end — transcribing (dictation)");

        let shared_audio = Arc::new(Mutex::new(Some(audio)));
        let worker_audio = Arc::clone(&shared_audio);
        let worker_app = app.clone();
        let spawned = std::thread::Builder::new()
            .name("voice-asr".into())
            .spawn(move || {
                let _cleanup = SessionCleanup { session };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> bool {
                    let Some(audio) = worker_audio.lock().ok().and_then(|mut audio| audio.take())
                    else {
                        return false;
                    };
                    // Cue after `stop`, so our own mic is already closed and cannot hear its own end
                    // cue — and only on success: a failure plays its own sound from `emit_error`, and
                    // two cues back to back would say less than either one alone (#49).
                    let transcript = match voice_lane::stop(audio) {
                        TranscriptOutcome::Ok(t) => t,
                        TranscriptOutcome::Empty => {
                            let _ = complete_terminal(session, SessionPhase::Processing, || {
                                emit_error(&worker_app, "Didn't catch that — try again.")
                            });
                            return true;
                        }
                        TranscriptOutcome::Err(e) => {
                            let _ = complete_terminal(session, SessionPhase::Processing, || {
                                emit_error(&worker_app, e)
                            });
                            return true;
                        }
                    };

                    if !session_is_processing(session) {
                        return true;
                    }
                    crate::sound::mac::play(shogun_core::sound::Cue::VoiceEnd);
                    // The local exact-alias pass only prepares a candidate for the optional editor;
                    // if Groq is unavailable, slow, or returns unsafe output, delivery keeps the
                    // original ASR text exactly. Cancellation during the request wins before any
                    // AX or clipboard mutation.
                    let correction = dictionary_edit_candidate(&transcript);
                    let transcript = worker_app
                        .try_state::<Db>()
                        .and_then(|db| {
                            edit_dictation(
                                &correction.text,
                                &correction.protected_terms,
                                db.inner(),
                            )
                        })
                        .unwrap_or(transcript);
                    if !session_is_processing(session) {
                        return true;
                    }
                    let Some(outcome) =
                        deliver_dictation(session, target.as_deref(), &delivery, &transcript)
                    else {
                        return true;
                    };
                    match outcome {
                        DeliveryOutcome::Inserted => {
                            let _ = complete_terminal(session, SessionPhase::Processing, || {
                                emit_state(&worker_app, "idle", Some(transcript), None);
                            });
                        }
                        DeliveryOutcome::Copied => {
                            let _ = complete_terminal(session, SessionPhase::Processing, || {
                                emit_state(&worker_app, "idle", Some(transcript), None);
                            });
                        }
                        DeliveryOutcome::CopyFailed(error) => {
                            let _ = complete_terminal(session, SessionPhase::Processing, || {
                                emit_error(
                                    &worker_app,
                                    format!("Could not copy dictation: {error}"),
                                );
                            });
                        }
                    }
                    true
                }));
                if !matches!(result, Ok(true)) {
                    let _ = complete_terminal(session, SessionPhase::Processing, || {
                        emit_error(&worker_app, "Voice transcription failed.")
                    });
                }
            });
        if spawned.is_err() {
            let audio = shared_audio.lock().ok().and_then(|mut audio| audio.take());
            if let Some(audio) = audio {
                let _ = voice_lane::stop(audio);
            }
            let _ = complete_terminal(session, SessionPhase::Processing, || {
                emit_error(&app, "Voice transcription could not start.")
            });
        }
    }

    #[tauri::command]
    pub fn get_voice_settings() -> Settings {
        LANE.lock()
            .ok()
            .and_then(|g| g.as_ref().map(|l| l.settings.clone()))
            .unwrap_or_default()
    }

    /// Enumerate selectable inputs off the main thread: CoreAudio walks every device and queries
    /// each name, which is unbounded and can stall while a device is mid-(dis)connect. The rest of
    /// this lane keeps mic work off the UI thread for the same reason (`request_microphone_access_bg`).
    #[tauri::command(async)]
    pub fn get_voice_microphones() -> Result<Vec<String>, String> {
        shogun_core::audio::capture::mic::input_device_names()
    }

    #[tauri::command]
    pub fn set_voice_microphone(microphone: Option<String>, app: AppHandle) -> Result<(), String> {
        let microphone = normalize_microphone_selection(microphone);
        let mut lane = LANE
            .lock()
            .map_err(|_| "voice lane lock poisoned".to_string())?;
        let settings = lane.as_mut().ok_or("voice not initialized")?;
        settings.settings.microphone = microphone;
        save_settings(&app, &settings.settings);
        Ok(())
    }

    #[tauri::command]
    pub fn get_voice_edit_settings() -> VoiceEditSettingsView {
        VoiceEditSettingsView {
            model: VOICE_EDIT_MODEL,
            has_key: voice_edit_key().is_some(),
        }
    }

    #[tauri::command]
    pub fn set_voice_edit_key(key: String) -> Result<(), String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("key is empty".into());
        }
        keychain_store::set_generic_secret(VOICE_EDIT_KEY_ACCOUNT, key.as_bytes())
            .map_err(|error| error.to_string())
    }

    #[tauri::command]
    pub fn clear_voice_edit_key() -> Result<(), String> {
        clear_voice_edit_accounts(keychain_store::delete_generic_secret, |error| {
            error.code() == -25300 /* errSecItemNotFound */
        })
        .map_err(|error| error.to_string())
    }

    /// Revoke both account names. Older installs can still have the legacy account, which remains
    /// a valid background fallback until it is deleted too.
    fn clear_voice_edit_accounts<E>(
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

    #[tauri::command]
    pub fn set_voice_enabled(enabled: bool, app: AppHandle) -> Result<(), String> {
        let mut lane = LANE
            .lock()
            .map_err(|_| "voice lane lock poisoned".to_string())?;
        let settings = lane.as_mut().ok_or("voice not initialized")?;
        settings.settings.enabled = enabled;
        save_settings(&app, &settings.settings);
        if enabled {
            preload_asr_bg(&app);
            request_microphone_access_bg(settings.settings.microphone.clone());
        }
        if !enabled {
            drop(lane);
            if let Some(audio) = cancel_active_session() {
                stop_cancelled_audio(audio);
            }
            emit_state(&app, "idle", None, None);
            eprintln!("[voice] enabled={enabled}");
            return Ok(());
        }
        eprintln!("[voice] enabled={enabled}");
        Ok(())
    }

    #[tauri::command]
    pub fn voice_dismiss(app: AppHandle) {
        if let Some(audio) = cancel_active_session() {
            stop_cancelled_audio(audio);
        }
        emit_state(&app, "idle", None, None);
    }

    /// Frontend failsafe: force-end a hold that stayed in recording after release.
    #[tauri::command]
    pub fn voice_force_end(app: AppHandle) {
        let _ = force_end_if_recording(app);
    }

    fn build_overlay(app: &AppHandle) -> Option<WebviewWindow> {
        if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
            return Some(win);
        }
        let win = tauri::WebviewWindowBuilder::new(app, WINDOW_LABEL, tauri::WebviewUrl::default())
            .title("ShogunAI — voice")
            .transparent(true)
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .shadow(false)
            .skip_taskbar(true)
            .inner_size(1.0, 1.0)
            .visible(false)
            .focused(false)
            .build()
            .map_err(|e| eprintln!("[voice] overlay build failed: {e}"))
            .ok()?;
        configure_overlay(&win);
        Some(win)
    }

    fn configure_overlay(win: &WebviewWindow) {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        use std::sync::atomic::Ordering;

        let ptr = match win.ns_window() {
            Ok(p) if !p.is_null() => p as *mut AnyObject,
            _ => return,
        };
        let behavior = crate::PANEL_BEHAVIOR.load(Ordering::Relaxed);
        let level = crate::OVERLAY_LEVEL;
        // SAFETY: live NSWindow on main thread (setup).
        unsafe {
            let _: () = msg_send![ptr, setCollectionBehavior: behavior];
            let _: () = msg_send![ptr, setLevel: level];
            let _: () = msg_send![ptr, setHidesOnDeactivate: false];
            let _: () = msg_send![ptr, setCanHide: true];
            let _: () = msg_send![ptr, setMovableByWindowBackground: false];
            let _: () = msg_send![ptr, setIgnoresMouseEvents: false];
        }
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    mod tests {
        use super::*;

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum TestPhase {
            Recording,
            Processing,
            Finishing,
        }

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        struct TestSession {
            id: u64,
            phase: TestPhase,
        }

        fn begin_processing(active: &mut Option<TestSession>, id: u64) -> bool {
            let Some(session) = active.as_mut() else {
                return false;
            };
            if session.id != id || session.phase != TestPhase::Recording {
                return false;
            }
            session.phase = TestPhase::Processing;
            true
        }

        fn claim_terminal(active: &mut Option<TestSession>, id: u64) -> bool {
            let Some(session) = active.as_mut() else {
                return false;
            };
            if session.id != id || session.phase != TestPhase::Processing {
                return false;
            }
            session.phase = TestPhase::Finishing;
            true
        }

        fn clear_terminal(active: &mut Option<TestSession>, id: u64) {
            if active
                .is_some_and(|session| session.id == id && session.phase == TestPhase::Finishing)
            {
                *active = None;
            }
        }

        fn can_start(active: Option<TestSession>) -> bool {
            active.is_none()
        }

        fn generation_is_processing(active: Option<TestSession>, id: u64) -> bool {
            active.is_some_and(|session| session.id == id && session.phase == TestPhase::Processing)
        }

        fn target_for_test() -> DictationTarget {
            DictationTarget {
                element: std::ptr::null_mut(),
                pid: 1,
                role: "AXTextField".into(),
                original_value: "before".into(),
                caret: CFRange::init(3, 0),
                direct_ax_writable: true,
                operation: Mutex::new(()),
            }
        }

        #[test]
        fn collapsed_caret_is_required_for_dictation_capture() {
            assert!(valid_collapsed_caret("hello", CFRange::init(2, 0)));
            assert!(!valid_collapsed_caret("hello", CFRange::init(2, 1)));
            assert!(!valid_collapsed_caret("hello", CFRange::init(9, 0)));
        }

        #[test]
        fn dictation_insert_preserves_adjacent_utf16_text() {
            let caret = CFRange::init(1, 0);
            assert_eq!(
                expected_insert("a🙂b", caret, " hello"),
                Some("a hello🙂b".into())
            );
        }

        #[test]
        fn changed_value_focus_or_secure_role_never_passes_target_verification() {
            let target = target_for_test();
            assert!(!target_state_matches(
                &target,
                Some("AXTextField"),
                Some("changed"),
                Some(CFRange::init(3, 0)),
                true,
                true,
            ));
            assert!(!target_state_matches(
                &target,
                Some("AXSecureTextField"),
                Some("before"),
                Some(CFRange::init(3, 0)),
                true,
                true,
            ));
            assert!(!target_state_matches(
                &target,
                Some("AXTextField"),
                Some("before"),
                Some(CFRange::init(3, 0)),
                false,
                true,
            ));
            assert!(!target_state_matches(
                &target,
                Some("AXTextField"),
                Some("before"),
                Some(CFRange::init(3, 0)),
                true,
                false,
            ));
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
        fn processing_session_blocks_a_second_hold() {
            let mut active = Some(TestSession {
                id: 7,
                phase: TestPhase::Recording,
            });
            assert!(begin_processing(&mut active, 7));
            assert_eq!(
                active,
                Some(TestSession {
                    id: 7,
                    phase: TestPhase::Processing
                })
            );
            assert!(!can_start(active));
        }

        #[test]
        fn stale_terminal_cannot_unlock_a_newer_session() {
            let mut active = Some(TestSession {
                id: 8,
                phase: TestPhase::Processing,
            });
            assert!(!claim_terminal(&mut active, 7));
            clear_terminal(&mut active, 7);
            assert_eq!(
                active,
                Some(TestSession {
                    id: 8,
                    phase: TestPhase::Processing
                })
            );
        }

        #[test]
        fn stale_generation_is_rejected_before_the_voice_end_cue() {
            let active = Some(TestSession {
                id: 8,
                phase: TestPhase::Processing,
            });
            assert!(!generation_is_processing(active, 7));
            assert!(generation_is_processing(active, 8));
        }

        #[test]
        fn terminal_claim_blocks_reentry_until_its_event_is_emitted() {
            let mut active = Some(TestSession {
                id: 9,
                phase: TestPhase::Processing,
            });
            assert!(claim_terminal(&mut active, 9));
            assert_eq!(
                active,
                Some(TestSession {
                    id: 9,
                    phase: TestPhase::Finishing
                })
            );
            clear_terminal(&mut active, 9);
            assert_eq!(active, None);
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
        fn cancellation_after_a_claim_cannot_reopen_the_delivery_gate() {
            let gate = AtomicU8::new(DELIVERY_READY);
            assert!(gate
                .compare_exchange(
                    DELIVERY_READY,
                    DELIVERY_WRITING,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok());
            assert!(gate
                .compare_exchange(
                    DELIVERY_READY,
                    DELIVERY_CANCELLED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_err());
            assert_eq!(gate.load(Ordering::Acquire), DELIVERY_WRITING);
        }

        #[test]
        fn cancellation_waits_for_shared_fence_for_ax_or_clipboard_delivery() {
            use std::sync::mpsc;
            use std::time::Duration;

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
            assert_eq!(config.base_url, GROQ_BASE_URL);
            assert_eq!(config.model, VOICE_EDIT_MODEL);
            assert_eq!(config.reasoning_effort.as_deref(), Some("low"));
            assert_eq!(config.include_reasoning, Some(false));
        }

        #[test]
        fn dictionary_candidate_preserves_built_in_aliases_for_editing() {
            let correction = dictionary_edit_candidate("open shogun ai with g rock");
            assert_eq!(correction.text, "open ShogunAI with Groq");
            assert_eq!(correction.protected_terms, vec!["ShogunAI", "Groq"]);
        }

        #[test]
        fn microphone_selection_treats_empty_names_as_default_input() {
            assert_eq!(normalize_microphone_selection(None), None);
            assert_eq!(normalize_microphone_selection(Some("  ".into())), None);
            assert_eq!(
                normalize_microphone_selection(Some("Studio Mic".into())),
                Some("Studio Mic".into())
            );
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
            assert_eq!(
                deleted,
                vec![VOICE_EDIT_KEY_ACCOUNT, LEGACY_GROQ_KEY_ACCOUNT]
            );
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
            assert_eq!(
                deleted,
                vec![VOICE_EDIT_KEY_ACCOUNT, LEGACY_GROQ_KEY_ACCOUNT]
            );
        }

        #[test]
        fn cancelled_session_after_edit_cannot_enter_delivery() {
            let active = Some(TestSession {
                id: 12,
                phase: TestPhase::Finishing,
            });
            assert!(!generation_is_processing(active, 12));
        }

        #[test]
        fn worker_panic_spawn_or_audio_failure_clears_only_its_matching_owner() {
            let mut active = Some(TestSession {
                id: 10,
                phase: TestPhase::Processing,
            });
            assert!(claim_terminal(&mut active, 10));
            clear_terminal(&mut active, 10);
            assert_eq!(active, None);

            active = Some(TestSession {
                id: 11,
                phase: TestPhase::Processing,
            });
            assert!(!claim_terminal(&mut active, 10));
            assert_eq!(
                active,
                Some(TestSession {
                    id: 11,
                    phase: TestPhase::Processing
                })
            );
        }
    }
}
