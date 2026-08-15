//! On-device wiring for inline draft-at-cursor (macOS). This is the device half of
//! `shogun_core::inline`: the two AX seams (read the focused field, write at the caret), the BYOK
//! Agent-lane client read from the Keychain, and the trigger that runs the composition.
//!
//! Everything here is `cfg(target_os = "macos")` — it does not compile on Linux CI. The pure core
//! (`compose_inline`, `build_prompt`) and the memory gate (`Db::inline_memory`) are Linux-tested; this
//! file is verified on device (build, paste errors, iterate — see docs/phase1-ondevice-runbook.md).
//!
//! Invariants: generation is the BYOK Agent lane (`AnthropicAgentClient`, invariant 5); the key is
//! read from the Keychain only (invariant 7); inserting at the caret is device-local, never a send
//! (invariant 4); the client records the one digest-only egress trace (invariant 3 / G8); AX text
//! only, no screenshot (invariant 2).

#[cfg(target_os = "macos")]
pub mod mac {
    use accessibility_sys::{
        AXUIElementCopyAttributeValue, AXUIElementCreateSystemWide, AXUIElementRef,
        AXUIElementSetAttributeValue, AXUIElementSetMessagingTimeout, AXValueCreate,
        AXValueGetTypeID, AXValueGetValue, kAXErrorSuccess, kAXFocusedUIElementAttribute,
        kAXRoleAttribute, kAXSelectedTextAttribute, kAXSelectedTextRangeAttribute,
        kAXTitleAttribute, kAXValueAttribute, kAXValueTypeCFRange,
    };
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::{CFGetTypeID, CFRange, CFRelease, CFTypeRef};
    use core_foundation_sys::string::{CFStringGetTypeID, CFStringRef};

    use shogun_core::daemon::{ContextPack, Db, ReplyContext};
    use shogun_core::db_sink::DbTraceabilitySink;
    use shogun_core::inline::{
        CursorContext, CursorReader, InlineOutcome, ScribeEditRequest, TextInserter,
        build_scribe_edit_prompt, compose_inline,
    };
    use shogun_core::llm::anthropic::{AnthropicAgentClient, AnthropicConfig};
    use shogun_core::llm::openai_compat::{
        GEMINI_BASE_URL, OPENAI_BASE_URL, OPENROUTER_BASE_URL, OpenAiCompatAgentClient,
        OpenAiCompatConfig,
    };
    use shogun_core::llm::transport::ReqwestTransport;
    use shogun_core::llm::{AgentClient, ByokKey, LlmError, MockAgentClient, Secret};

    /// The Keychain service the BYOK keys live under (invariant 7).
    use shogun_integrations::keychain_store;

    // ---- Agent-lane provider settings (provider + model; NON-secret, so a JSON file is fine —
    // ---- the KEY always stays in the Keychain, one account per provider) ----------------------

    /// Providers the Agent lane can run on. The Batch lane (indexing / Dream Cycle) is untouched
    /// by this choice — it stays on the Select KK lane (invariant 5).
    const PROVIDERS: [&str; 4] = ["anthropic", "openrouter", "openai", "gemini"];

    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    pub struct LlmSettings {
        pub provider: String,
        /// Model id in the provider's own naming. Empty = the provider's default.
        pub model: String,
    }

    impl Default for LlmSettings {
        fn default() -> Self {
            Self {
                provider: "anthropic".into(),
                model: String::new(),
            }
        }
    }

    /// The provider's default model when the user hasn't set one.
    /// A model name that is safe to log.
    ///
    /// The model field used to be free text, and a user who pasted their API key into it had that
    /// key written to the log in plaintext — a direct breach of invariant 7 (secrets never reach a
    /// file, DB or log). The field is a picker now, but the log must not depend on the UI being
    /// the only writer: anything that doesn't look like one of our model ids is redacted rather
    /// than printed.
    fn loggable_model(model: &str) -> String {
        let known = PROVIDERS.iter().any(|p| default_model(p) == model);
        let plausible = model.len() <= 48
            && model
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '/' | ':' | '_'));
        if known || plausible && !looks_like_secret(model) {
            model.to_string()
        } else {
            "<redacted>".to_string()
        }
    }

    /// Heuristics for "this is a credential, not a model id". Deliberately eager: a redacted model
    /// name costs a debugging detail, a logged key costs the key.
    fn looks_like_secret(v: &str) -> bool {
        let v = v.trim();
        v.len() >= 32
            || v.starts_with("sk-")
            || v.starts_with("AQ.")
            || v.starts_with("AIza")
            || v.starts_with("sk_")
    }

    fn default_model(provider: &str) -> &'static str {
        match provider {
            "openrouter" => "anthropic/claude-sonnet-4.5",
            "openai" => "gpt-4o-mini",
            "gemini" => "gemini-2.5-flash",
            _ => "claude-sonnet-5",
        }
    }

    /// One Keychain account per provider, so switching providers never overwrites another key.
    fn keychain_account(provider: &str) -> &'static str {
        match provider {
            "openrouter" => "openrouter-byok",
            "openai" => "openai-byok",
            "gemini" => "gemini-byok",
            _ => "anthropic-byok",
        }
    }

    static LLM_SETTINGS: std::sync::Mutex<Option<LlmSettings>> = std::sync::Mutex::new(None);
    /// Cached "does the active provider have a key" — the 3s status poll must not hit the Keychain
    /// every tick. Refreshed on init and on every key/provider change.
    static HAS_KEY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    struct PendingRewrite {
        original_value: String,
        selected_text: String,
    }

    /// Selection prepared by [`AxCursorReader`] and consumed by [`AxTextInserter`]. Inline
    /// generation runs off the UI thread, so the inserter must prove the field stayed unchanged
    /// before replacing anything.
    static PENDING_REWRITE: std::sync::Mutex<Option<PendingRewrite>> = std::sync::Mutex::new(None);

    /// One Scribe session owns one captured AX target. The pointer is a retained CF object and is
    /// used only by the session worker; keeping it avoids writing into whichever field the user
    /// happened to focus while the Scribe prompt was open.
    struct ScribeTarget {
        element: AXUIElementRef,
        original_value: String,
        target_range: CFRange,
        selected_text: String,
    }

    // AXUIElementRef is a retained CoreFoundation object. Scribe serializes all access through
    // SCRIBE_SESSION, so moving its owner to the dedicated worker is safe.
    unsafe impl Send for ScribeTarget {}

    impl Drop for ScribeTarget {
        fn drop(&mut self) {
            if !self.element.is_null() {
                unsafe { CFRelease(self.element as CFTypeRef) };
            }
        }
    }

    struct ScribeSession {
        id: u64,
        generation: u64,
        target: Option<ScribeTarget>,
        context: CursorContext,
        memory: Vec<String>,
        busy: bool,
        active: bool,
    }

    static SCRIBE_SESSION: std::sync::Mutex<Option<ScribeSession>> = std::sync::Mutex::new(None);
    static NEXT_SCRIBE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    /// The active provider rejected its key (HTTP 401/403). Sticky until the key or the provider
    /// changes, because the failure is silent everywhere else: a ⌥-tap that 401s inserts nothing,
    /// which is indistinguishable from the shortcut not working — pressing it five more times is
    /// the reasonable response, and that is exactly what happens.
    static KEY_REJECTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    /// Record that the provider refused the key. Called from every Agent-lane failure path.
    pub fn note_key_rejected() {
        if !KEY_REJECTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!("[inline] provider rejected the key — settings will say so");
        }
    }

    fn refresh_has_key() {
        let present = keychain_byok(&current_settings().provider).is_some();
        HAS_KEY.store(present, std::sync::atomic::Ordering::Relaxed);
        // A new key, or a different provider, deserves a fresh verdict.
        KEY_REJECTED.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        use tauri::Manager;
        app.path().app_data_dir().ok().map(|d| d.join("llm.json"))
    }

    /// Load persisted provider settings into the in-memory copy. Called once at setup.
    pub fn init_llm_settings(app: &tauri::AppHandle) {
        let mut s = LlmSettings::default();
        if let Some(p) = settings_path(app) {
            if let Ok(text) = std::fs::read_to_string(p) {
                if let Ok(saved) = serde_json::from_str::<LlmSettings>(&text) {
                    if PROVIDERS.contains(&saved.provider.as_str()) {
                        s = saved;
                    }
                }
            }
        }
        eprintln!(
            "[inline] agent provider = {} model = {}",
            s.provider,
            loggable_model(&effective_model(&s))
        );
        if let Ok(mut g) = LLM_SETTINGS.lock() {
            *g = Some(s);
        }
        refresh_has_key();
    }

    fn current_settings() -> LlmSettings {
        LLM_SETTINGS
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_default()
    }

    fn effective_model(s: &LlmSettings) -> String {
        let m = s.model.trim();
        if m.is_empty() {
            default_model(&s.provider).to_string()
        } else {
            m.to_string()
        }
    }

    /// Current provider settings for the Settings UI (model echoes the effective default when
    /// unset, so the placeholder the user sees is what will actually run).
    #[tauri::command]
    pub fn get_llm_settings() -> LlmSettings {
        current_settings()
    }

    /// Change provider/model. The key is NOT touched — each provider keeps its own Keychain
    /// account, entered separately in Settings.
    #[tauri::command]
    pub fn set_llm_settings(
        provider: String,
        model: String,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        if !PROVIDERS.contains(&provider.as_str()) {
            return Err(format!("unknown provider: {provider}"));
        }
        let s = LlmSettings {
            provider,
            model: model.trim().to_string(),
        };
        if let Some(p) = settings_path(&app) {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            match serde_json::to_string_pretty(&s) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(&p, json) {
                        return Err(format!("save failed: {e}"));
                    }
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        eprintln!(
            "[inline] agent provider → {} model → {}",
            s.provider,
            loggable_model(&effective_model(&s))
        );
        if let Ok(mut g) = LLM_SETTINGS.lock() {
            *g = Some(s);
        }
        refresh_has_key();
        Ok(())
    }

    // ---- AX helpers -------------------------------------------------------------------------

    /// Copy a string attribute off an element (create rule; released here). `None` if absent or not
    /// a string. Sets the 100ms messaging timeout on the systemwide element by the caller.
    unsafe fn copy_string(el: AXUIElementRef, name: &str) -> Option<String> {
        let cf_name = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        let err =
            unsafe { AXUIElementCopyAttributeValue(el, cf_name.as_concrete_TypeRef(), &mut value) };
        if err != kAXErrorSuccess || value.is_null() {
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

    unsafe fn copy_range(el: AXUIElementRef, name: &str) -> Option<CFRange> {
        let cf_name = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        let err =
            unsafe { AXUIElementCopyAttributeValue(el, cf_name.as_concrete_TypeRef(), &mut value) };
        if err != kAXErrorSuccess || value.is_null() {
            return None;
        }
        let mut range = CFRange::init(0, 0);
        let is_range = unsafe { CFGetTypeID(value) == AXValueGetTypeID() }
            && unsafe {
                AXValueGetValue(
                    value.cast_mut().cast(),
                    kAXValueTypeCFRange,
                    (&mut range as *mut CFRange).cast(),
                )
            };
        unsafe { CFRelease(value) };
        is_range.then_some(range)
    }

    unsafe fn is_secure_text_field(el: AXUIElementRef) -> bool {
        unsafe { copy_string(el, kAXRoleAttribute) }.as_deref() == Some("AXSecureTextField")
    }

    unsafe fn set_range(el: AXUIElementRef, range: CFRange) -> bool {
        let value =
            unsafe { AXValueCreate(kAXValueTypeCFRange, (&range as *const CFRange).cast()) };
        if value.is_null() {
            return false;
        }
        let attr = CFString::new(kAXSelectedTextRangeAttribute);
        let err =
            unsafe { AXUIElementSetAttributeValue(el, attr.as_concrete_TypeRef(), value.cast()) };
        unsafe { CFRelease(value.cast()) };
        err == kAXErrorSuccess
    }

    /// AX text ranges use UTF-16 code-unit offsets. Select an existing highlight, or the current
    /// paragraph when the caret is collapsed. Returns target plus fixed surrounding text.
    fn rewrite_target(value: &str, selection: CFRange) -> Option<(CFRange, String, String)> {
        let units: Vec<u16> = value.encode_utf16().collect();
        let location = usize::try_from(selection.location).ok()?;
        let length = usize::try_from(selection.length).ok()?;
        let selected_end = location.checked_add(length)?;
        if selected_end > units.len() {
            return None;
        }

        let (start, end) = if length > 0 {
            (location, selected_end)
        } else {
            let start = units[..location]
                .iter()
                .rposition(|unit| *unit == u16::from(b'\n'))
                .map_or(0, |index| index + 1);
            let end = units[location..]
                .iter()
                .position(|unit| *unit == u16::from(b'\n'))
                .map_or(units.len(), |offset| location + offset);
            (start, end)
        };
        let target = String::from_utf16(&units[start..end]).ok()?;
        let prefix = String::from_utf16(&units[..start]).ok()?;
        let suffix = String::from_utf16(&units[end..]).ok()?;
        let surrounding = if prefix.is_empty() && suffix.is_empty() {
            String::new()
        } else {
            format!("before target:\n{prefix}\n\nafter target:\n{suffix}")
        };
        Some((
            CFRange::init(start.try_into().ok()?, (end - start).try_into().ok()?),
            target,
            surrounding,
        ))
    }

    /// Replace an AX UTF-16 range without slicing Rust strings at byte offsets. Returns exact
    /// replacement value and range, so Scribe can reselect generated text for the next edit.
    fn replace_utf16_range(
        value: &str,
        range: CFRange,
        replacement: &str,
    ) -> Option<(String, CFRange)> {
        let units: Vec<u16> = value.encode_utf16().collect();
        let start = usize::try_from(range.location).ok()?;
        let length = usize::try_from(range.length).ok()?;
        let end = start.checked_add(length)?;
        if end > units.len() {
            return None;
        }
        let replacement_units: Vec<u16> = replacement.encode_utf16().collect();
        let mut output = Vec::with_capacity(start + replacement_units.len() + units.len() - end);
        output.extend_from_slice(&units[..start]);
        output.extend_from_slice(&replacement_units);
        output.extend_from_slice(&units[end..]);
        Some((
            String::from_utf16(&output).ok()?,
            CFRange::init(range.location, replacement_units.len().try_into().ok()?),
        ))
    }

    #[derive(Debug, PartialEq, Eq)]
    enum FullValueFallbackDecision {
        AlreadyLanded,
        SafeToWrite,
        Abort,
    }

    fn full_value_fallback_decision(
        current_value: Option<&str>,
        current_range: Option<CFRange>,
        current_selected: &str,
        original_value: &str,
        target_range: CFRange,
        selected_text: &str,
        expected: &str,
    ) -> FullValueFallbackDecision {
        if current_value == Some(expected) {
            FullValueFallbackDecision::AlreadyLanded
        } else if current_value == Some(original_value)
            && current_range == Some(target_range)
            && current_selected == selected_text
        {
            FullValueFallbackDecision::SafeToWrite
        } else {
            FullValueFallbackDecision::Abort
        }
    }

    #[cfg(test)]
    mod rewrite_target_tests {
        use super::*;

        #[test]
        fn utf16_replacement_handles_non_bmp_text() {
            let value = "A😀BC";
            let range = CFRange::init(1, 2);
            let (replaced, selected) =
                replace_utf16_range(value, range, "📝").expect("valid range");
            assert_eq!(replaced, "A📝BC");
            assert_eq!(selected.location, 1);
            assert_eq!(selected.length, 2);
        }

        #[test]
        fn utf16_replacement_supports_empty_insertion() {
            let (replaced, selected) =
                replace_utf16_range("😀B", CFRange::init(2, 0), "").expect("valid range");
            assert_eq!(replaced, "😀B");
            assert_eq!(selected, CFRange::init(2, 0));
        }

        #[test]
        fn explicit_selection_is_the_only_rewrite_target() {
            let (_, target, surrounding) =
                rewrite_target("prefix can you join? suffix", CFRange::init(7, 13))
                    .expect("valid AX range");

            assert_eq!(target, "can you join?");
            assert!(surrounding.contains("prefix ") && surrounding.contains(" suffix"));
        }

        #[test]
        fn collapsed_caret_selects_current_paragraph_using_utf16_offsets() {
            let text = "first\ncan you send 日本語?\nlast";
            let caret = "first\ncan you send 日本".encode_utf16().count();

            let (range, target, _) = rewrite_target(
                text,
                CFRange::init(caret.try_into().expect("test offset fits"), 0),
            )
            .expect("valid AX range");

            assert_eq!(target, "can you send 日本語?");
            assert_eq!(range.length as usize, target.encode_utf16().count());
        }

        #[test]
        fn collapsed_caret_on_empty_line_keeps_an_insertion_point() {
            let text = "before\n\nafter";
            let caret = "before\n".encode_utf16().count();

            let (range, target, _) = rewrite_target(
                text,
                CFRange::init(caret.try_into().expect("test offset fits"), 0),
            )
            .expect("valid AX range");

            assert!(target.is_empty());
            assert_eq!(range.location as usize, caret);
            assert_eq!(range.length, 0);
        }

        #[test]
        fn invalid_ax_range_is_rejected() {
            assert!(rewrite_target("short", CFRange::init(99, 1)).is_none());
        }

        #[test]
        fn full_value_fallback_accepts_unchanged_snapshot() {
            let range = CFRange::init(2, 4);
            assert_eq!(
                full_value_fallback_decision(
                    Some("before target after"),
                    Some(range),
                    "target",
                    "before target after",
                    range,
                    "target",
                    "before replacement after",
                ),
                FullValueFallbackDecision::SafeToWrite
            );
        }

        #[test]
        fn full_value_fallback_accepts_expected_value_already_landed() {
            let range = CFRange::init(2, 4);
            assert_eq!(
                full_value_fallback_decision(
                    Some("before replacement after"),
                    Some(CFRange::init(2, 9)),
                    "replacement",
                    "before target after",
                    range,
                    "target",
                    "before replacement after",
                ),
                FullValueFallbackDecision::AlreadyLanded
            );
        }

        #[test]
        fn full_value_fallback_rejects_any_snapshot_change() {
            let range = CFRange::init(2, 4);
            for (value, current_range, selected) in [
                (Some("user edit"), Some(range), "target"),
                (
                    Some("before target after"),
                    Some(CFRange::init(3, 4)),
                    "target",
                ),
                (Some("before target after"), Some(range), "changed"),
            ] {
                assert_eq!(
                    full_value_fallback_decision(
                        value,
                        current_range,
                        selected,
                        "before target after",
                        range,
                        "target",
                        "before replacement after",
                    ),
                    FullValueFallbackDecision::Abort
                );
            }
        }
    }

    /// Copy the focused UI element (create rule → caller releases).
    unsafe fn focused_element() -> Option<AXUIElementRef> {
        let sys = unsafe { AXUIElementCreateSystemWide() };
        if sys.is_null() {
            return None;
        }
        unsafe { AXUIElementSetMessagingTimeout(sys, 0.25) };
        let cf_name = CFString::new(kAXFocusedUIElementAttribute);
        let mut value: CFTypeRef = std::ptr::null();
        let err = unsafe {
            AXUIElementCopyAttributeValue(sys, cf_name.as_concrete_TypeRef(), &mut value)
        };
        unsafe { CFRelease(sys as CFTypeRef) };
        if err != kAXErrorSuccess || value.is_null() {
            return None;
        }
        let el = value as AXUIElementRef;
        unsafe { AXUIElementSetMessagingTimeout(el, 0.25) };
        Some(el)
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        // std::ffi::c_void — spelled in full so the extern block needs no import.
        /// Create rule — the caller releases.
        fn CGEventCreateKeyboardEvent(
            source: *const std::ffi::c_void,
            keycode: u16,
            key_down: bool,
        ) -> *mut std::ffi::c_void;
        fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);
        fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
    }

    /// Insert `text` by putting it on the pasteboard and synthesising ⌘V, then putting the user's
    /// clipboard back.
    ///
    /// The fallback for apps that accept an AX write and ignore it. Paste is the mechanism the
    /// user would have reached for, and it works wherever a caret does — the app cannot tell it
    /// apart from a real ⌘V. Nothing leaves the device (invariant 3): the pasteboard is local and
    /// the keystroke is synthesised into the app that already has focus.
    ///
    /// The clipboard is the user's, not ours, so it is saved and restored. Restoring happens AFTER
    /// the result is verified — put back too early and the paste races with the restore and lands
    /// the old contents instead.
    unsafe fn paste_at_cursor(
        text: &str,
        el: AXUIElementRef,
        before: Option<&str>,
    ) -> Result<(), String> {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        use objc2_foundation::NSString;

        const KVK_ANSI_V: u16 = 0x09;
        const FLAG_COMMAND: u64 = 1 << 20;
        const HID_EVENT_TAP: u32 = 0;

        let pb: *mut AnyObject = unsafe { msg_send![class!(NSPasteboard), generalPasteboard] };
        if pb.is_null() {
            return Err("no pasteboard".into());
        }
        let utf8 = NSString::from_str("public.utf8-plain-text");

        // Save first: everything after this point can fail, and the user's clipboard must survive.
        let saved: *mut AnyObject = unsafe { msg_send![pb, stringForType: &*utf8] };
        let saved: Option<String> = if saved.is_null() {
            None
        } else {
            let s: *const NSString = saved.cast();
            Some(unsafe { &*s }.to_string())
        };

        let ours = NSString::from_str(text);
        let _: isize = unsafe { msg_send![pb, clearContents] };
        let ok: bool = unsafe { msg_send![pb, setString: &*ours, forType: &*utf8] };
        if !ok {
            return Err("could not write the pasteboard".into());
        }

        let restore = || unsafe {
            let _: isize = msg_send![pb, clearContents];
            if let Some(prev) = &saved {
                let s = NSString::from_str(prev);
                let _: bool = msg_send![pb, setString: &*s, forType: &*utf8];
            }
        };

        // ⌘V as two events. The flag goes on both: an app reading the keyUp without the modifier
        // can treat the chord as cancelled.
        for down in [true, false] {
            let ev = unsafe { CGEventCreateKeyboardEvent(std::ptr::null(), KVK_ANSI_V, down) };
            if ev.is_null() {
                restore();
                return Err("could not synthesise the paste".into());
            }
            unsafe {
                CGEventSetFlags(ev, FLAG_COMMAND);
                CGEventPost(HID_EVENT_TAP, ev);
                CFRelease(ev as CFTypeRef);
            }
        }

        let landed = unsafe { value_changed(el, before) };
        restore();
        if landed {
            Ok(())
        } else {
            Err("the paste did not change the field either".into())
        }
    }

    /// Did the field actually change? Re-reads `AXValue` a few times before giving up.
    ///
    /// Some apps apply an AX write on their next run-loop turn, so a single immediate read would
    /// call a working insert a failure. A few short retries cost nothing on the success path (the
    /// first read almost always wins) and are the difference between a false negative and a true
    /// one. A field with no readable value cannot be verified either way — treated as landed, so
    /// this check can only ever downgrade a claim we could disprove, never invent a failure.
    unsafe fn value_changed(el: AXUIElementRef, before: Option<&str>) -> bool {
        let Some(before) = before else { return true };
        for _ in 0..4 {
            match unsafe { copy_string(el, kAXValueAttribute) } {
                Some(now) if now != before => return true,
                None => return true,
                _ => std::thread::sleep(std::time::Duration::from_millis(30)),
            }
        }
        false
    }

    /// Reads and selects the rewrite target: current selection when present, otherwise current
    /// paragraph. The prepared range is later verified before replacement. Never a screenshot.
    pub struct AxCursorReader;

    impl CursorReader for AxCursorReader {
        fn read(&self) -> Option<CursorContext> {
            if let Ok(mut pending) = PENDING_REWRITE.lock() {
                *pending = None;
            }
            // SAFETY: focused_element returns a live +1 element we release before returning.
            let el = unsafe { focused_element() }?;
            // The presence of the AXValue ATTRIBUTE is the "this is a text-carrying field" signal —
            // Some("") is a focused EMPTY field (the most common draft target: a fresh reply, a
            // blank doc) and must produce a context. Only an element with no value attribute at
            // all (a button, an icon) yields None.
            if unsafe { is_secure_text_field(el) } {
                unsafe { CFRelease(el as CFTypeRef) };
                return None;
            }
            let value = unsafe { copy_string(el, kAXValueAttribute) };
            let field_label = unsafe { copy_string(el, kAXTitleAttribute) }.unwrap_or_default();
            let selection = unsafe { copy_range(el, kAXSelectedTextRangeAttribute) };
            let prepared = value.as_deref().zip(selection).and_then(|(value, range)| {
                let (target_range, target, surrounding) = rewrite_target(value, range)?;
                unsafe { set_range(el, target_range) }.then_some((target, surrounding))
            });
            unsafe { CFRelease(el as CFTypeRef) };
            let (target, surrounding) = prepared?;
            if let Ok(mut pending) = PENDING_REWRITE.lock() {
                *pending = Some(PendingRewrite {
                    original_value: value?,
                    selected_text: target.clone(),
                });
            } else {
                return None;
            }
            let app = crate::display::frontmost_app()
                .map(|f| f.bundle_id)
                .unwrap_or_default();
            Some(CursorContext {
                app,
                field_label,
                before: target,
                after: surrounding,
            })
        }
    }

    /// Writes text at the caret by setting `AXSelectedText` on the focused element — inserts at the
    /// insertion point (or replaces the selection), exactly like a paste. Device-local (invariant 4).
    ///
    /// The write is VERIFIED by reading the field back. `AXUIElementSetAttributeValue` returning
    /// `kAXErrorSuccess` only means the message was accepted, not that the app applied it: plenty
    /// of apps (web views in particular) return success and ignore the write. Trusting the return
    /// code made the product claim "Drafted" while nothing appeared in the document — a success
    /// report the app had no evidence for.
    pub struct AxTextInserter;

    impl TextInserter for AxTextInserter {
        fn insert(&self, text: &str) -> Result<(), String> {
            let el = unsafe { focused_element() }.ok_or_else(|| "no focused field".to_string())?;
            let before = unsafe { copy_string(el, kAXValueAttribute) };
            let selected = unsafe { copy_string(el, kAXSelectedTextAttribute) }.unwrap_or_default();
            let pending = PENDING_REWRITE
                .lock()
                .map_err(|_| "rewrite target unavailable".to_string())?
                .take()
                .ok_or_else(|| "rewrite target unavailable".to_string())?;
            if before.as_deref() != Some(pending.original_value.as_str())
                || selected != pending.selected_text
            {
                unsafe { CFRelease(el as CFTypeRef) };
                return Err("focused rewrite target changed".into());
            }
            let cf_attr = CFString::new(kAXSelectedTextAttribute);
            let cf_text = CFString::new(text);
            // SAFETY: el is a live element; attr + value are valid CFStrings.
            let err = unsafe {
                AXUIElementSetAttributeValue(
                    el,
                    cf_attr.as_concrete_TypeRef(),
                    cf_text.as_concrete_TypeRef() as CFTypeRef,
                )
            };
            if err != kAXErrorSuccess {
                unsafe { CFRelease(el as CFTypeRef) };
                return Err(format!("AX set selected text failed: {err}"));
            }
            if unsafe { value_changed(el, before.as_deref()) } {
                unsafe { CFRelease(el as CFTypeRef) };
                return Ok(());
            }
            // The app took the message and ignored it. Measured on device: Chrome and the terminal
            // both do this, and they are not exotic targets — AXSelectedText is honoured by little
            // beyond native NSTextView/NSTextField. Fall back to the mechanism that works
            // everywhere, because it is the one the user would have used: paste.
            let app = crate::display::frontmost_app()
                .map(|f| f.bundle_id)
                .unwrap_or_default();
            eprintln!("[inline] {app} ignored the AX write — falling back to paste");
            let pasted = unsafe { paste_at_cursor(text, el, before.as_deref()) };
            unsafe { CFRelease(el as CFTypeRef) };
            match pasted {
                Ok(()) => Ok(()),
                // The bundle id, never the text: which app refuses BOTH paths is the fact that
                // makes this fixable, and it is the first thing to know next time.
                Err(e) => Err(format!("{app}: {e}")),
            }
        }
    }

    /// Capture Scribe's editable target once. Secure text fields are never read, even if their
    /// AX value happens to be exposed by an application.
    fn capture_scribe_target() -> Option<(ScribeTarget, CursorContext)> {
        let el = unsafe { focused_element() }?;
        if unsafe { is_secure_text_field(el) } {
            unsafe { CFRelease(el as CFTypeRef) };
            return None;
        }
        let value = unsafe { copy_string(el, kAXValueAttribute) };
        let selection = unsafe { copy_range(el, kAXSelectedTextRangeAttribute) };
        let Some(value) = value else {
            unsafe { CFRelease(el as CFTypeRef) };
            return None;
        };
        let Some(selection) = selection else {
            unsafe { CFRelease(el as CFTypeRef) };
            return None;
        };
        let Some((target_range, selected_text, surrounding)) = rewrite_target(&value, selection)
        else {
            unsafe { CFRelease(el as CFTypeRef) };
            return None;
        };
        if !unsafe { set_range(el, target_range) } {
            unsafe { CFRelease(el as CFTypeRef) };
            return None;
        }
        let app = crate::display::frontmost_app()
            .map(|f| f.bundle_id)
            .unwrap_or_default();
        let field_label = unsafe { copy_string(el, kAXTitleAttribute) }.unwrap_or_default();
        Some((
            ScribeTarget {
                element: el,
                original_value: value,
                target_range,
                selected_text: selected_text.clone(),
            },
            CursorContext {
                app,
                field_label,
                before: selected_text,
                after: surrounding,
            },
        ))
    }

    /// Insert into captured target, not current focus. Re-read both value and selection first so
    /// a changed document or caret cannot receive a stale generated result.
    fn insert_scribe(target: &ScribeTarget, text: &str) -> Result<CFRange, String> {
        let current = unsafe { copy_string(target.element, kAXValueAttribute) };
        let current_range = unsafe { copy_range(target.element, kAXSelectedTextRangeAttribute) };
        let selected =
            unsafe { copy_string(target.element, kAXSelectedTextAttribute) }.unwrap_or_default();
        if current.as_deref() != Some(target.original_value.as_str())
            || current_range != Some(target.target_range)
            || selected != target.selected_text
        {
            return Err("captured target changed".into());
        }
        let Some((expected, replacement_range)) =
            replace_utf16_range(&target.original_value, target.target_range, text)
        else {
            return Err("captured target range invalid".into());
        };
        let attr = CFString::new(kAXSelectedTextAttribute);
        let value = CFString::new(text);
        let err = unsafe {
            AXUIElementSetAttributeValue(
                target.element,
                attr.as_concrete_TypeRef(),
                value.as_concrete_TypeRef() as CFTypeRef,
            )
        };
        if err != kAXErrorSuccess {
            return Err(format!("AX set selected text failed: {err}"));
        }
        if !unsafe { value_matches(target.element, &expected) } {
            // Browser/Electron AX often returns success while ignoring AXSelectedText. Set the
            // complete retained target value instead; never paste into current focus.
            let current = unsafe { copy_string(target.element, kAXValueAttribute) };
            let current_range =
                unsafe { copy_range(target.element, kAXSelectedTextRangeAttribute) };
            let selected = unsafe { copy_string(target.element, kAXSelectedTextAttribute) }
                .unwrap_or_default();
            match full_value_fallback_decision(
                current.as_deref(),
                current_range,
                &selected,
                &target.original_value,
                target.target_range,
                &target.selected_text,
                &expected,
            ) {
                FullValueFallbackDecision::AlreadyLanded => {}
                FullValueFallbackDecision::Abort => {
                    return Err("Scribe target changed before fallback".into());
                }
                FullValueFallbackDecision::SafeToWrite => {
                    let attr = CFString::new(kAXValueAttribute);
                    let full_value = CFString::new(&expected);
                    let err = unsafe {
                        AXUIElementSetAttributeValue(
                            target.element,
                            attr.as_concrete_TypeRef(),
                            full_value.as_concrete_TypeRef() as CFTypeRef,
                        )
                    };
                    if err != kAXErrorSuccess
                        || !unsafe { value_matches(target.element, &expected) }
                    {
                        return Err("Scribe target did not accept replacement".into());
                    }
                }
            }
        }
        if !unsafe { set_range(target.element, replacement_range) } {
            return Err("Scribe could not select replacement".into());
        }
        Ok(replacement_range)
    }

    /// Read AXValue until it matches expected, allowing browser run loops to apply writes.
    unsafe fn value_matches(el: AXUIElementRef, expected: &str) -> bool {
        for _ in 0..4 {
            if unsafe { copy_string(el, kAXValueAttribute) }.as_deref() == Some(expected) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        false
    }

    fn refresh_scribe_snapshot(
        original_value: &mut String,
        selected_text: &mut String,
        context_before: &mut String,
        value: Option<String>,
        selected: Option<String>,
        generated: &str,
    ) -> bool {
        let (Some(value), Some(selected)) = (value, selected) else {
            return false;
        };
        *original_value = value;
        *selected_text = selected;
        *context_before = generated.to_string();
        true
    }

    // ---- BYOK Agent-lane client (Keychain → real, else nothing) -----------------------------

    /// The Agent-lane client for inline drafts and chat. Real when the ACTIVE provider has a key
    /// in the Keychain; the echo `Mock` exists only for exercising the AX read→insert loop on
    /// device and is reachable solely via `SHOGUN_MOCK_AGENT=1`, never by a user with no key.
    /// `pub(crate)` so the send producers (`crate::approvals`, e.g. Reply Drafter) draft through
    /// the SAME BYOK Agent-lane client (invariant 5).
    pub(crate) enum InlineAgent {
        Mock(MockAgentClient),
        Anthropic {
            rt: tokio::runtime::Runtime,
            client: AnthropicAgentClient<ReqwestTransport, DbTraceabilitySink>,
        },
        OpenAiCompat {
            rt: tokio::runtime::Runtime,
            client: OpenAiCompatAgentClient<ReqwestTransport, DbTraceabilitySink>,
        },
    }

    impl InlineAgent {
        /// True when a real provider client is behind this agent (a key was found in the Keychain).
        /// The mock echoes its prompt back, which is useful for proving the inline AX insert path
        /// but must never be surfaced as a chat answer — that would print SHOGUN's whole internal
        /// prompt into the user's thread.
        pub(crate) fn is_live(&self) -> bool {
            !matches!(self, InlineAgent::Mock(_))
        }
    }

    impl AgentClient for InlineAgent {
        fn complete(&self, prompt: &str) -> Result<String, LlmError> {
            match self {
                InlineAgent::Mock(m) => m.complete(prompt),
                // block_on is safe here: the whole inline flow runs on a dedicated std thread
                // (never a tokio worker), so there is no runtime already driving this thread.
                InlineAgent::Anthropic { rt, client } => rt.block_on(client.complete(prompt)),
                InlineAgent::OpenAiCompat { rt, client } => rt.block_on(client.complete(prompt)),
            }
        }
    }

    /// Read the ACTIVE provider's BYOK key from the Keychain (invariant 7 — never a
    /// file/env/DB/log). `None` if unset.
    fn keychain_byok(provider: &str) -> Option<String> {
        keychain_store::get_generic_secret(keychain_account(provider))
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    }

    /// Save the BYOK key for `provider` (Settings → "Your key"). The provider comes EXPLICITLY
    /// from the UI — reading the backend's "current" provider here raced the async provider
    /// switch and could file a key under the wrong Keychain account. Overwrites any existing key
    /// for that provider. The key itself is NEVER logged (invariant 7).
    #[tauri::command]
    pub fn set_byok_key(provider: String, key: String) -> Result<(), String> {
        if !PROVIDERS.contains(&provider.as_str()) {
            return Err(format!("unknown provider: {provider}"));
        }
        let key = key.trim();
        if key.is_empty() {
            return Err("key is empty".into());
        }
        keychain_store::set_generic_secret(keychain_account(&provider), key.as_bytes())
            .map_err(|e| e.to_string())?;
        eprintln!("[inline] BYOK key saved to Keychain (provider: {provider})");
        refresh_has_key();
        Ok(())
    }

    /// Remove `provider`'s BYOK key — chat and drafts stop until a new one is added.
    #[tauri::command]
    pub fn clear_byok_key(provider: String) -> Result<(), String> {
        if !PROVIDERS.contains(&provider.as_str()) {
            return Err(format!("unknown provider: {provider}"));
        }
        keychain_store::delete_generic_secret(keychain_account(&provider))
            .map_err(|e| e.to_string())?;
        eprintln!("[inline] BYOK key removed from Keychain (provider: {provider})");
        refresh_has_key();
        Ok(())
    }

    /// Opt-in echo mock, for exercising the AX read→insert loop on device without a key.
    ///
    /// Off unless `SHOGUN_MOCK_AGENT=1`. It used to be the automatic fallback whenever a key was
    /// missing, which meant a user with no key got the mock's echo written into their own document
    /// and reported as a successful draft. A development aid must never be the default path a real
    /// user falls down.
    fn mock_agent_enabled() -> bool {
        std::env::var("SHOGUN_MOCK_AGENT").is_ok_and(|v| v == "1")
    }

    /// Build the Agent client for this run from the current provider settings, or `None` when
    /// there is no usable one — no key for the active provider, or no transport/runtime.
    ///
    /// `None` means callers MUST NOT produce output. Returning an `Option` rather than a silent
    /// mock is the point: the caret sits in the user's own document, and the type now forces every
    /// call site to decide what to do about a missing key instead of inheriting a default that
    /// writes. `pub(crate)` so the send producers (`crate::approvals`) share the one BYOK
    /// Agent-lane client construction (invariant 5) rather than re-deriving it.
    pub(crate) fn build_agent(db: &Db) -> Option<InlineAgent> {
        let s = current_settings();
        let Some(key) = keychain_byok(&s.provider) else {
            if mock_agent_enabled() {
                eprintln!("[inline] SHOGUN_MOCK_AGENT=1 — echo mock (AX path still runs)");
                return Some(InlineAgent::Mock(MockAgentClient::new(ByokKey::new(
                    Secret::new("mock"),
                ))));
            }
            eprintln!(
                "[inline] no key in Keychain for provider '{}' — not drafting",
                s.provider
            );
            return None;
        };
        let model = effective_model(&s);
        match (
            ReqwestTransport::new(),
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build(),
        ) {
            (Ok(transport), Ok(rt)) => {
                let byok = ByokKey::new(Secret::new(key));
                eprintln!(
                    "[inline] live Agent lane — provider {} model {}",
                    s.provider,
                    loggable_model(&model)
                );
                match s.provider.as_str() {
                    "openrouter" | "openai" | "gemini" => {
                        let base = match s.provider.as_str() {
                            "openrouter" => OPENROUTER_BASE_URL,
                            "gemini" => GEMINI_BASE_URL,
                            _ => OPENAI_BASE_URL,
                        };
                        let client = OpenAiCompatAgentClient::new(
                            transport,
                            db.traceability_sink(),
                            byok,
                            OpenAiCompatConfig::new(base, model),
                        );
                        Some(InlineAgent::OpenAiCompat { rt, client })
                    }
                    _ => {
                        let client = AnthropicAgentClient::new(
                            transport,
                            db.traceability_sink(),
                            byok,
                            AnthropicConfig::new(model),
                        );
                        Some(InlineAgent::Anthropic { rt, client })
                    }
                }
            }
            // A key is present but we cannot build a client to use it. Falling back to the
            // echo mock here would write mock output into the document of a user who HAS paid the
            // setup cost — the most confusing version of this bug. Draft nothing.
            _ => {
                eprintln!("[inline] transport/runtime unavailable — not drafting");
                None
            }
        }
    }

    // ---- trigger ----------------------------------------------------------------------------

    /// Run the inline draft: on a dedicated thread (so the AX reads/writes and the blocking Agent
    /// call don't touch a tokio worker), read the caret context, gather confidence-gated memory,
    /// generate, and insert at the caret. Fire-and-forget — the outcome is logged (no captured text).
    /// How many context lines the inline draft carries. Enough for the thread to be recognisable,
    /// bounded so the prompt stays small on the latency-critical path.
    const INLINE_CONTEXT_LINES: usize = 14;

    /// Draft at the cursor, preferring the pre-assembled reply context for the thread the user is
    /// looking at.
    ///
    /// `warm` is the pack the focus path built ahead of the press (the 150ms budget forbids
    /// collecting it here). When there is none — a thread not yet warmed — this falls back to the
    /// plain state facts rather than building inline, so a miss costs context, never latency.
    /// What the notch shows while and after a ⌥-tap. Pushed to the webview so the keystroke is
    /// acknowledged: without this every outcome — drafting, inserted, no field, rejected key —
    /// looks identical from the outside, which is to say it looks like the shortcut is broken.
    #[derive(Clone, serde::Serialize)]
    pub struct InlineStatus {
        /// `drafting` | `inserted` | `no_context` | `no_key` | `key_rejected` | `failed`
        pub phase: &'static str,
        /// Chars written at the caret, for `inserted`.
        pub chars: usize,
        /// A short reason for `failed`; never the generated text or the user's content.
        pub detail: Option<String>,
    }

    fn push_inline(app: &tauri::AppHandle, status: InlineStatus) {
        use tauri::Emitter;
        let _ = app.emit("inline", status);
    }

    pub fn run_inline_at_cursor(
        db: Db,
        warm: Option<shogun_core::daemon::ReplyContext>,
        app: tauri::AppHandle,
    ) {
        // Emitted before the thread starts so the pill reacts to the press itself, not to the
        // generation finishing — the whole point is that the tap feels answered immediately.
        push_inline(
            &app,
            InlineStatus {
                phase: "drafting",
                chars: 0,
                detail: None,
            },
        );
        std::thread::spawn(move || {
            let memory = match warm {
                Some(ctx) if !ctx.is_empty() => {
                    eprintln!(
                        "[inline] using the warm reply context ({} turn(s), built in {}ms)",
                        ctx.turns.len(),
                        ctx.build_ms
                    );
                    ctx.as_memory_lines(INLINE_CONTEXT_LINES)
                }
                _ => db.inline_memory(6),
            };
            // No key means no draft AND no write. The caret sits in the user's own document;
            // putting anything there that they did not ask for is worse than doing nothing.
            let Some(agent) = build_agent(&db) else {
                push_inline(
                    &app,
                    InlineStatus {
                        phase: "no_key",
                        chars: 0,
                        detail: None,
                    },
                );
                return;
            };
            let outcome = compose_inline(&AxCursorReader, &agent, &AxTextInserter, &memory);
            match &outcome {
                InlineOutcome::Inserted { chars } => {
                    eprintln!("[inline] inserted {chars} chars at the cursor");
                    push_inline(
                        &app,
                        InlineStatus {
                            phase: "inserted",
                            chars: *chars,
                            detail: None,
                        },
                    );
                }
                // Nothing gets inserted on a rejected key, so without this the tap is silent and
                // the reasonable next move is to press it again. Latch it for the status poll.
                InlineOutcome::KeyRejected(why) => {
                    eprintln!("[inline] {why}");
                    note_key_rejected();
                    push_inline(
                        &app,
                        InlineStatus {
                            phase: "key_rejected",
                            chars: 0,
                            detail: Some(why.clone()),
                        },
                    );
                }
                InlineOutcome::NoContext => {
                    eprintln!("[inline] no editable field under the caret");
                    push_inline(
                        &app,
                        InlineStatus {
                            phase: "no_context",
                            chars: 0,
                            detail: None,
                        },
                    );
                }
                other => {
                    eprintln!("[inline] {other:?}");
                    // The reason, not the content: these carry provider/AX errors, never the
                    // draft or anything the user typed.
                    let detail = match other {
                        InlineOutcome::GenerationFailed(e) | InlineOutcome::InsertFailed(e) => {
                            Some(e.clone())
                        }
                        _ => None,
                    };
                    push_inline(
                        &app,
                        InlineStatus {
                            phase: "failed",
                            chars: 0,
                            detail,
                        },
                    );
                }
            }
        });
    }

    /// Tauri command: the notch "draft at cursor" action and the shortcut both call this.
    #[tauri::command]
    pub fn inline_at_cursor(
        db: tauri::State<'_, Db>,
        reply: tauri::State<'_, shogun_core::daemon::ReplyContextCache>,
        app: tauri::AppHandle,
    ) -> &'static str {
        run_inline_at_cursor(db.inner().clone(), reply.current(), app);
        "started"
    }

    // ---- Scribe session ---------------------------------------------------------------------

    #[derive(Clone, serde::Serialize)]
    pub struct ScribeEvent {
        pub session_id: u64,
        /// `opened` | `processing` | `inserted` | `failed` | `closed` | `cancelled` | `no_key`
        pub phase: &'static str,
        pub chars: usize,
        /// Content-free diagnostic. Never provider text, prompt text, or AX field text.
        pub detail: Option<&'static str>,
    }

    fn push_scribe(app: &tauri::AppHandle, event: ScribeEvent) {
        use tauri::Emitter;
        let _ = app.emit("scribe", event);
    }

    fn finish_scribe_worker(session_id: u64, generation: u64) {
        if let Ok(mut sessions) = SCRIBE_SESSION.lock() {
            if let Some(session) = sessions.as_mut().filter(|session| {
                session.id == session_id && (session.generation == generation || !session.active)
            }) {
                session.busy = false;
                if !session.active {
                    session.target.take();
                }
            }
        }
    }

    /// Open Scribe and capture focused editable context exactly once. Warm context is already
    /// ranked by the core focus path; only when absent do we use bounded, confidence-gated facts.
    pub fn open_scribe(
        db: Db,
        warm: Option<ReplyContext>,
        app: tauri::AppHandle,
    ) -> Result<u64, String> {
        let (target, context) =
            capture_scribe_target().ok_or_else(|| "no editable target".to_string())?;
        let memory = warm
            .filter(|ctx| !ctx.is_empty())
            .map(|ctx| ctx.as_memory_lines(INLINE_CONTEXT_LINES))
            .unwrap_or_else(|| db.inline_memory(6));
        let id = NEXT_SCRIBE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut sessions = SCRIBE_SESSION
            .lock()
            .map_err(|_| "scribe unavailable".to_string())?;
        if sessions
            .as_ref()
            .is_some_and(|session| session.active || session.busy)
        {
            return Err("scribe already open".into());
        }
        *sessions = Some(ScribeSession {
            id,
            generation: 0,
            target: Some(target),
            context,
            memory,
            busy: false,
            active: true,
        });
        drop(sessions);
        push_scribe(
            &app,
            ScribeEvent {
                session_id: id,
                phase: "opened",
                chars: 0,
                detail: None,
            },
        );
        Ok(id)
    }

    #[tauri::command]
    pub fn scribe_open(
        db: tauri::State<'_, Db>,
        reply: tauri::State<'_, shogun_core::daemon::ReplyContextCache>,
        app: tauri::AppHandle,
    ) -> Result<u64, String> {
        open_scribe(db.inner().clone(), reply.current(), app)
    }

    /// Submit one typed Scribe instruction. Session stays alive after success for another edit.
    #[tauri::command]
    pub fn scribe_submit(
        session_id: u64,
        instruction: String,
        db: tauri::State<'_, Db>,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        if instruction.chars().count() > 4_000 {
            return Err("instruction too long".into());
        }
        let (generation, context, memory) = {
            let mut sessions = SCRIBE_SESSION
                .lock()
                .map_err(|_| "scribe unavailable".to_string())?;
            let session = sessions
                .as_mut()
                .filter(|session| session.active && session.id == session_id)
                .ok_or_else(|| "scribe session closed".to_string())?;
            if session.busy {
                return Err("scribe submission already running".into());
            }
            session.busy = true;
            session.generation = session.generation.wrapping_add(1);
            (
                session.generation,
                session.context.clone(),
                session.memory.clone(),
            )
        };
        push_scribe(
            &app,
            ScribeEvent {
                session_id,
                phase: "processing",
                chars: 0,
                detail: None,
            },
        );
        let db = db.inner().clone();
        std::thread::spawn(move || {
            let Some(agent) = build_agent(&db) else {
                finish_scribe_worker(session_id, generation);
                push_scribe(
                    &app,
                    ScribeEvent {
                        session_id,
                        phase: "no_key",
                        chars: 0,
                        detail: None,
                    },
                );
                return;
            };
            let prompt = build_scribe_edit_prompt(&ScribeEditRequest {
                context: &context,
                memory: &memory,
                instruction: &instruction,
            });
            let generated = match agent.complete(&prompt) {
                Ok(text) if !text.trim().is_empty() => text,
                Ok(_) => {
                    finish_scribe_worker(session_id, generation);
                    push_scribe(
                        &app,
                        ScribeEvent {
                            session_id,
                            phase: "failed",
                            chars: 0,
                            detail: Some("empty response"),
                        },
                    );
                    return;
                }
                Err(_) => {
                    finish_scribe_worker(session_id, generation);
                    push_scribe(
                        &app,
                        ScribeEvent {
                            session_id,
                            phase: "failed",
                            chars: 0,
                            detail: Some("generation failed"),
                        },
                    );
                    return;
                }
            };
            let generated = generated.trim().to_string();
            let chars = generated.chars().count();
            let outcome = {
                let mut sessions = match SCRIBE_SESSION.lock() {
                    Ok(sessions) => sessions,
                    Err(_) => return,
                };
                let Some(session) = sessions.as_mut().filter(|session| {
                    session.active && session.id == session_id && session.generation == generation
                }) else {
                    drop(sessions);
                    finish_scribe_worker(session_id, generation);
                    return;
                };
                let Some(target) = session.target.as_ref() else {
                    session.busy = false;
                    return;
                };
                match insert_scribe(target, &generated) {
                    Ok(replacement_range) => {
                        // Keep session alive. Next instruction edits latest generated text, while
                        // target verification still rejects changes made outside Scribe.
                        let refreshed = session.target.as_mut().is_some_and(|target| {
                            target.target_range = replacement_range;
                            let value = unsafe { copy_string(target.element, kAXValueAttribute) };
                            let selected =
                                unsafe { copy_string(target.element, kAXSelectedTextAttribute) };
                            refresh_scribe_snapshot(
                                &mut target.original_value,
                                &mut target.selected_text,
                                &mut session.context.before,
                                value,
                                selected,
                                &generated,
                            )
                        });
                        if refreshed {
                            session.busy = false;
                            true
                        } else {
                            session.target.take();
                            session.active = false;
                            session.busy = false;
                            false
                        }
                    }
                    Err(_) => {
                        session.busy = false;
                        false
                    }
                }
            };
            push_scribe(
                &app,
                ScribeEvent {
                    session_id,
                    phase: if outcome { "inserted" } else { "failed" },
                    chars: if outcome { chars } else { 0 },
                    detail: if outcome {
                        None
                    } else {
                        Some("target changed or insert failed")
                    },
                },
            );
        });
        Ok(())
    }

    #[cfg(test)]
    mod scribe_state_tests {
        use super::refresh_scribe_snapshot;

        #[test]
        fn repeated_insert_refresh_keeps_latest_selected_result() {
            let mut original = "first".to_string();
            let mut selected = "first".to_string();
            let mut before = "first".to_string();

            assert!(refresh_scribe_snapshot(
                &mut original,
                &mut selected,
                &mut before,
                Some("second".into()),
                Some("second".into()),
                "second"
            ));
            assert!(refresh_scribe_snapshot(
                &mut original,
                &mut selected,
                &mut before,
                Some("third".into()),
                Some("third".into()),
                "third"
            ));
            assert_eq!(
                (original, selected, before),
                ("third".into(), "third".into(), "third".into())
            );
        }

        #[test]
        fn missing_selection_invalidates_refresh() {
            let mut original = "first".to_string();
            let mut selected = "first".to_string();
            let mut before = "first".to_string();
            assert!(!refresh_scribe_snapshot(
                &mut original,
                &mut selected,
                &mut before,
                Some("second".into()),
                None,
                "second"
            ));
            assert_eq!(
                (original, selected, before),
                ("first".into(), "first".into(), "first".into())
            );
        }
    }

    fn close_scribe(
        session_id: u64,
        app: &tauri::AppHandle,
        phase: &'static str,
    ) -> Result<(), String> {
        let mut sessions = SCRIBE_SESSION
            .lock()
            .map_err(|_| "scribe unavailable".to_string())?;
        let session = sessions
            .as_mut()
            .filter(|session| session.id == session_id && session.active)
            .ok_or_else(|| "scribe session closed".to_string())?;
        session.active = false;
        session.generation = session.generation.wrapping_add(1);
        if !session.busy {
            session.target.take();
        }
        drop(sessions);
        push_scribe(
            app,
            ScribeEvent {
                session_id,
                phase,
                chars: 0,
                detail: None,
            },
        );
        Ok(())
    }

    #[tauri::command]
    pub fn scribe_close(session_id: u64, app: tauri::AppHandle) -> Result<(), String> {
        close_scribe(session_id, &app, "closed")
    }

    #[tauri::command]
    pub fn scribe_cancel(session_id: u64, app: tauri::AppHandle) -> Result<(), String> {
        close_scribe(session_id, &app, "cancelled")
    }

    // ---- live status / state / chat (the product window's real data) -----------------------

    /// A live snapshot of what SHOGUN sees and knows — for the window header.
    #[derive(serde::Serialize)]
    pub struct Status {
        /// The app SHOGUN is currently reading (frontmost bundle id).
        pub app: String,
        pub commitments: usize,
        pub open_loops: usize,
        /// Whether a BYOK key is in the Keychain (live generation vs. echo).
        pub has_key: bool,
        /// The provider refused the key — the UI says so rather than leaving ⌥-tap silently dead.
        pub key_rejected: bool,
    }

    #[tauri::command]
    pub fn shogun_status(db: tauri::State<'_, Db>) -> Status {
        // Never report our own process as the "reading" target — Idle would say reading ShogunAI.
        let app = crate::display::frontmost_app()
            .filter(|f| !crate::display::is_own_app(&f.bundle_id, &f.name))
            .map(|f| f.bundle_id)
            .unwrap_or_default();
        Status {
            app,
            commitments: db.commitments_due(db.now_ms()).len(),
            open_loops: db.open_loops().len(),
            has_key: HAS_KEY.load(std::sync::atomic::Ordering::Relaxed),
            key_rejected: KEY_REJECTED.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Quit the app immediately. `app.exit` can hang if a window is mid-operation, so go straight to
    /// process exit — the user pressing Quit wants it gone now. Cmd+Q / Dock quit also work now that
    /// the default window is a normal (Regular) app.
    #[tauri::command]
    pub fn quit_app() {
        eprintln!("[shell] quit_app — exiting");
        std::process::exit(0);
    }

    /// Webview-side error channel. Silent `.catch(() => {})` swallowed real failures (the missing
    /// window-API permissions looked like "buttons do nothing") — UI errors must reach the
    /// terminal log. Never carries captured content, only UI diagnostics.
    #[tauri::command]
    pub fn ui_log(msg: String) {
        eprintln!("[ui] {msg}");
    }

    /// One state row for the "What I know" panel. Carries the row `id` so the UI can resolve it
    /// (mark a commitment done / close an open loop) with a click.
    #[derive(serde::Serialize)]
    pub struct StateItem {
        pub id: i64,
        pub text: String,
        pub meta: String,
    }
    #[derive(serde::Serialize)]
    pub struct StateView {
        pub commitments: Vec<StateItem>,
        pub open_loops: Vec<StateItem>,
    }

    #[tauri::command]
    pub fn shogun_state(db: tauri::State<'_, Db>) -> StateView {
        let now = db.now_ms();
        // Read the rows WITH ids and hide already-resolved ones (done / cancelled / closed) so a
        // click-to-resolve makes the row disappear.
        let commitments = db
            .commitment_rows()
            .into_iter()
            .filter(|c| c.status != "done" && c.status != "cancelled")
            .map(|c| {
                let overdue = c.status == "overdue" || c.due_at.is_some_and(|d| d < now);
                StateItem {
                    id: c.id,
                    meta: if overdue {
                        "overdue".into()
                    } else {
                        format!("{:.0}% sure", c.confidence * 100.0)
                    },
                    text: c.description,
                }
            })
            .collect();
        let open_loops = db
            .open_loop_rows()
            .into_iter()
            .filter(|l| l.status != "closed")
            .map(|l| StateItem {
                id: l.id,
                meta: format!("{}d waiting", l.staleness_days),
                text: l.description,
            })
            .collect();
        StateView {
            commitments,
            open_loops,
        }
    }

    /// Resolve a state row the user clicked: `kind` is "commitment" (→ done) or "open_loop"
    /// (→ closed). Idempotent; unknown ids are a no-op.
    #[tauri::command]
    pub fn resolve_state_item(kind: String, id: i64, db: tauri::State<'_, Db>) -> bool {
        match kind.as_str() {
            "commitment" => db.resolve_commitment(id),
            "open_loop" => db.resolve_open_loop(id),
            _ => false,
        }
    }

    /// Clear all extracted state (commitments + open loops). The event log, people, and projects
    /// are untouched. The reset for when low-confidence extraction has accumulated noise.
    #[tauri::command]
    pub fn clear_memory(db: tauri::State<'_, Db>) -> bool {
        let ok = db.clear_state();
        eprintln!("[shell] clear_memory — extracted state cleared: {ok}");
        ok
    }

    /// How much retrieved evidence the chat prompt carries. Six excerpts of ~600 chars keeps the
    /// grounded half well under a page of context while covering a thread's worth of hits.
    const CHAT_EVIDENCE_HITS: usize = 6;
    const CHAT_EVIDENCE_CHARS: usize = 600;

    /// Build the chat prompt: the user's message grounded in confidence-gated state facts
    /// (FR-ST-20) AND the evidence retrieved for that message (Phase R1). Evidence is dated and
    /// attributed so the model answers from what was actually seen, and can say which item it
    /// used rather than asserting from nowhere.
    fn build_chat_prompt(message: &str, ctx: &ContextPack) -> String {
        let mut p = String::from(
            "You are SHOGUN, the user's private work assistant on their Mac. Answer grounded in what \
             you remember about their work. Be concise, concrete, and useful — no filler.\n\
             Prefer the retrieved evidence over your own assumptions. Cite the item you used when it \
             matters (e.g. \"per the Gmail thread\"). If the evidence does not answer the question, \
             say what you do know and what is missing — never invent specifics.\n",
        );
        let facts: Vec<&str> = ctx
            .facts
            .iter()
            .map(|m| m.trim())
            .filter(|m| !m.is_empty())
            .collect();
        if !facts.is_empty() {
            p.push_str("\nWhat you remember about their work:\n");
            for m in facts {
                p.push_str("- ");
                p.push_str(m);
                p.push('\n');
            }
        }
        if !ctx.screen_frames.is_empty() {
            p.push_str(
                "\nStored screen captures (JPEG, rolling local retention, linked to OCR events):\n",
            );
            for f in &ctx.screen_frames {
                p.push_str("- [screen frame ");
                p.push_str(&f.frame_id.to_string());
                p.push_str(" · ");
                if let Some(app) = f.app_bundle_id.as_deref().filter(|s| !s.is_empty()) {
                    p.push_str(app);
                    p.push_str(" · ");
                }
                if let Some(w) = f.window_title.as_deref().filter(|s| !s.is_empty()) {
                    p.push_str(w);
                    p.push_str(" · ");
                }
                p.push_str(&format!("{}×{}", f.width, f.height));
                if f.needs_rescan {
                    p.push_str(" · thin OCR — re-scanned text may appear in evidence");
                }
                p.push_str("] ");
                p.push_str(&f.ocr_excerpt);
                p.push('\n');
            }
        }
        if !ctx.evidence.is_empty() {
            p.push_str("\nRetrieved from their history (most relevant first):\n");
            for e in &ctx.evidence {
                p.push_str("- [");
                p.push_str(&shogun_memory::search::evidence_source_label(&e.source));
                if let Some(t) = e.title.as_deref().filter(|t| !t.is_empty()) {
                    p.push_str(" · ");
                    p.push_str(t);
                }
                if let Some(fid) = e.frame_id {
                    p.push_str(&format!(" · frame {fid}"));
                }
                p.push_str("] ");
                p.push_str(&e.excerpt);
                p.push('\n');
            }
        }
        p.push_str("\nUser: ");
        p.push_str(message);
        p.push_str("\nSHOGUN:");
        p
    }

    #[cfg(all(target_os = "macos", feature = "visual-recall-ocr"))]
    fn enrich_thin_frame_ocr(db: &Db, ctx: &mut ContextPack) {
        for frame in &ctx.screen_frames {
            if !frame.needs_rescan {
                continue;
            }
            let Some(rec) = db.get_screen_frame(frame.frame_id) else {
                continue;
            };
            let Some(text) = crate::screen_ocr::ocr_jpeg_bytes(&rec.jpeg) else {
                continue;
            };
            let excerpt = shogun_memory::search::excerpt(&text, "", 400);
            for ev in &mut ctx.evidence {
                if ev.frame_id == Some(frame.frame_id) {
                    ev.excerpt.push_str("\n[re-scanned from stored frame]: ");
                    ev.excerpt.push_str(&excerpt);
                }
            }
        }
    }

    #[cfg(not(all(target_os = "macos", feature = "visual-recall-ocr")))]
    fn enrich_thin_frame_ocr(_db: &Db, _ctx: &mut ContextPack) {}

    /// One source behind an answer, for the citation line under it.
    #[derive(serde::Serialize)]
    pub struct Citation {
        pub event_id: i64,
        pub source: String,
        pub title: Option<String>,
    }

    /// A chat answer plus what it was grounded in. Showing the sources is what lets the user
    /// check SHOGUN rather than take its word — the answer is only as good as its evidence.
    #[derive(serde::Serialize)]
    pub struct ChatAnswer {
        pub text: String,
        pub citations: Vec<Citation>,
    }

    /// Context-aware chat for the voice dialogue lane (#44). Same BYOK path as `shogun_chat`.
    pub(crate) fn voice_chat(db: &Db, message: &str) -> Result<ChatAnswer, String> {
        chat_blocking(db, message)
    }

    fn chat_blocking(db: &Db, message: &str) -> Result<ChatAnswer, String> {
        use shogun_memory::thread::Referent;

        // A question that refers to something without naming it ("how's that going?") can't be
        // answered by search — the words that would match aren't in it. Resolve which thread it
        // means first, and if two are equally plausible, ask instead of guessing: a confident
        // answer about the wrong piece of the user's work is the failure that loses their trust.
        let mut query = message.to_string();
        // The resolved thread(s) get their stored summary fed in as candidates when the
        // compressed path is active — a high-relevance block that survives budget pressure.
        let mut resolved_threads: Vec<String> = Vec::new();
        if shogun_memory::thread::is_referring(message) {
            let outcome = db.resolve_referent(message, None);
            match outcome.verdict {
                Referent::Ambiguous => {
                    let options: Vec<String> = outcome
                        .candidates
                        .iter()
                        .take(3)
                        .filter_map(|c| c.title.clone())
                        .collect();
                    if !options.is_empty() {
                        return Ok(ChatAnswer {
                            text: format!("Which one — {}?", options.join(", or ")),
                            citations: Vec::new(),
                        });
                    }
                }
                Referent::Resolved => {
                    // Fold the resolved thread's own words into the query so retrieval has
                    // something to match on.
                    if let Some(t) = outcome.candidates.first().and_then(|c| c.title.as_deref()) {
                        query = format!("{message} {t}");
                    }
                    if let Some(c) = outcome.candidates.first() {
                        resolved_threads.push(c.thread_key.clone());
                    }
                }
                Referent::None => {}
            }
        }

        // Retrieval + state facts: the question decides what history comes along, so "what
        // happened with X" can actually be answered from the event log.
        let ctx = match db.compression_config() {
            Some(cfg) if cfg.enabled => {
                db.assemble_context_compressed(
                    &query,
                    CHAT_EVIDENCE_HITS,
                    CHAT_EVIDENCE_CHARS,
                    &resolved_threads,
                    cfg,
                )
                .0
            }
            _ => db.assemble_context(&query, CHAT_EVIDENCE_HITS, CHAT_EVIDENCE_CHARS),
        };
        let mut ctx = ctx;
        enrich_thin_frame_ocr(db, &mut ctx);
        // Without a key there is nothing to answer with; with the dev mock the "answer" is the
        // prompt itself, and printing that dumps SHOGUN's entire internal prompt at the user. Say
        // what is actually wrong instead. (The UI also pre-empts this from `has_key`.)
        let no_key = || {
            Ok(ChatAnswer {
                text: "No key yet — add your provider key in Settings to get real answers."
                    .to_string(),
                citations: Vec::new(),
            })
        };
        let Some(agent) = build_agent(db) else {
            return no_key();
        };
        if !agent.is_live() {
            return no_key();
        }
        let text = agent
            .complete(&build_chat_prompt(message, &ctx))
            .map_err(|e| {
                // Same latch as the ⌥-tap path: chat surfaces the error text, but Settings is where
                // the fix is, and it needs to know the key is the problem.
                if matches!(e, LlmError::Unauthorized(..)) {
                    note_key_rejected();
                }
                e.to_string()
            })?;
        let citations = ctx
            .evidence
            .iter()
            .map(|e| Citation {
                event_id: e.event_id,
                source: e.source.clone(),
                title: e.title.clone(),
            })
            .collect();
        Ok(ChatAnswer { text, citations })
    }

    /// Chat with SHOGUN, grounded in memory, on the BYOK Agent lane. Runs the blocking generation on
    /// a blocking thread so it never touches a tokio worker. Records one egress trace (invariant 3).
    #[tauri::command]
    pub async fn shogun_chat(
        message: String,
        db: tauri::State<'_, Db>,
        app: tauri::AppHandle,
    ) -> Result<ChatAnswer, String> {
        use tauri::Manager;
        let db = db.inner().clone();
        let started = std::time::Instant::now();
        let answered = tokio::task::spawn_blocking(move || chat_blocking(&db, &message))
            .await
            .map_err(|e| e.to_string())?;

        // Grounding (spec §D2) is the share of answers that cited a source, so it can only be
        // counted where answers are produced. Failures aren't counted at all: an answer that never
        // arrived is not an ungrounded one, and folding errors in would quietly depress the rate.
        if let Ok(a) = &answered {
            let m = app.state::<crate::metrics::SloRegister>();
            m.record_answer(!a.citations.is_empty());
            // Not first-token latency yet — this path is non-streaming, so it measures the whole
            // answer. Recorded against the same SLO row because it is strictly worse than the
            // number the SLO asks for: if this passes, first-token would too.
            m.record_first_token_ms(started.elapsed().as_secs_f64() * 1000.0);
        }
        answered
    }
}
