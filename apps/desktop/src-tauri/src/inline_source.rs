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
        kAXErrorSuccess, kAXFocusedUIElementAttribute, kAXSelectedTextAttribute, kAXTitleAttribute,
        kAXValueAttribute, AXUIElementCopyAttributeValue, AXUIElementCreateSystemWide,
        AXUIElementRef, AXUIElementSetAttributeValue, AXUIElementSetMessagingTimeout,
    };
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::{CFGetTypeID, CFRelease, CFTypeRef};
    use core_foundation_sys::string::{CFStringGetTypeID, CFStringRef};

    use shogun_core::daemon::{ContextPack, Db};
    use shogun_core::db_sink::DbTraceabilitySink;
    use shogun_core::inline::{compose_inline, CursorContext, CursorReader, InlineOutcome, TextInserter};
    use shogun_core::llm::anthropic::{AnthropicAgentClient, AnthropicConfig};
    use shogun_core::llm::openai_compat::{
        OpenAiCompatAgentClient, OpenAiCompatConfig, GEMINI_BASE_URL, OPENAI_BASE_URL,
        OPENROUTER_BASE_URL,
    };
    use shogun_core::llm::subscription::{
        self, Delegate, DelegateState, ProcessRunner, SubscriptionAgentClient,
    };
    use shogun_core::llm::transport::ReqwestTransport;
    use shogun_core::llm::{AgentClient, ByokKey, LlmError, MockAgentClient, Secret};

    /// The Keychain service the BYOK keys live under (invariant 7).
    use shogun_integrations::keychain_store;

    // ---- Agent-lane provider settings (provider + model; NON-secret, so a JSON file is fine —
    // ---- the KEY always stays in the Keychain, one account per provider) ----------------------

    /// BYOK providers the Agent lane can run on with the user's own API key. The Batch lane
    /// (indexing / Dream Cycle) is untouched by this choice — it stays on the Select KK lane
    /// (invariant 5).
    const BYOK_PROVIDERS: [&str; 4] = ["anthropic", "openrouter", "openai", "gemini"];

    /// A provider id is valid if it is a BYOK provider or a subscription delegate (Issue #110).
    fn is_known_provider(p: &str) -> bool {
        BYOK_PROVIDERS.contains(&p) || Delegate::parse(p).is_some()
    }

    /// The delegate behind a provider id, if this provider spends the user's own subscription
    /// rather than an API key. `None` for every BYOK provider.
    fn delegate_for(provider: &str) -> Option<Delegate> {
        Delegate::parse(provider)
    }

    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    pub struct LlmSettings {
        pub provider: String,
        /// Model id in the provider's own naming. Empty = the provider's default. Ignored for
        /// subscription delegates — the vendor CLI picks the model its plan allows.
        pub model: String,
        /// The user has read the subscription-route disclosure and opted in (Issue #110).
        ///
        /// Delegating means the prompt leaves through a vendor CLI on a **consumer plan**, whose
        /// data handling is not the same as the metered API path SHOGUN's privacy copy describes.
        /// That difference is the user's to accept, so it is stored as an explicit flag and
        /// `build_agent` refuses a delegate without it — a defaulted-on disclosure is not consent.
        #[serde(default)]
        pub subscription_consent: bool,
    }

    impl Default for LlmSettings {
        fn default() -> Self {
            Self { provider: "anthropic".into(), model: String::new(), subscription_consent: false }
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
        let known = BYOK_PROVIDERS.iter().any(|p| default_model(p) == model);
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
            // A subscription delegate runs whatever model its own CLI defaults to on that plan.
            // Naming one here would be a guess SHOGUN has no way to honour.
            _ if delegate_for(provider).is_some() => "",
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

    /// Clear the rejected latch after any successful Agent-lane completion: the provider just
    /// accepted the key, so a stale verdict (a transient 401 from clock skew or an outage) must
    /// not keep telling the user their key is bad until they re-save it.
    pub fn note_key_accepted() {
        if KEY_REJECTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
            eprintln!("[inline] provider accepted the key again — clearing the rejected flag");
        }
    }

    /// Whether the active provider has something to authenticate with: a Keychain key for a BYOK
    /// provider, or a present-and-consented delegate for a subscription provider.
    ///
    /// The delegate check is `--version` only — no network, no quota, no credential read. Sign-in
    /// is deliberately NOT probed here: that costs the user's quota, and this runs on every
    /// provider change.
    fn refresh_has_key() {
        let s = current_settings();
        let present = match delegate_for(&s.provider) {
            Some(d) => s.subscription_consent && subscription::detect(&ProcessRunner, d).is_installed(),
            None => keychain_byok(&s.provider).is_some(),
        };
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
                    if is_known_provider(&saved.provider) {
                        s = saved;
                    }
                }
            }
        }
        eprintln!("[inline] agent provider = {} model = {}", s.provider, loggable_model(&effective_model(&s)));
        if let Ok(mut g) = LLM_SETTINGS.lock() {
            *g = Some(s);
        }
        refresh_has_key();
    }

    fn current_settings() -> LlmSettings {
        LLM_SETTINGS.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
    }

    fn effective_model(s: &LlmSettings) -> String {
        let m = s.model.trim();
        if m.is_empty() { default_model(&s.provider).to_string() } else { m.to_string() }
    }

    /// Current provider settings for the Settings UI (model echoes the effective default when
    /// unset, so the placeholder the user sees is what will actually run).
    #[tauri::command]
    pub fn get_llm_settings() -> LlmSettings {
        current_settings()
    }

    /// Change provider/model. The key is NOT touched — each provider keeps its own Keychain
    /// account, entered separately in Settings.
    ///
    /// `subscription_consent` carries the Issue #110 disclosure decision. It is `Option` so an
    /// ordinary provider/model change (from a screen that has no consent control) leaves the
    /// existing decision alone instead of silently revoking or granting it.
    #[tauri::command]
    pub fn set_llm_settings(
        provider: String,
        model: String,
        subscription_consent: Option<bool>,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        if !is_known_provider(&provider) {
            return Err(format!("unknown provider: {provider}"));
        }
        let s = LlmSettings {
            provider,
            model: model.trim().to_string(),
            subscription_consent: subscription_consent.unwrap_or(current_settings().subscription_consent),
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
        eprintln!("[inline] agent provider → {} model → {}", s.provider, loggable_model(&effective_model(&s)));
        if let Ok(mut g) = LLM_SETTINGS.lock() {
            *g = Some(s);
        }
        refresh_has_key();
        Ok(())
    }

    // ---- Privacy preferences (opt-in analytics; #28 §9-1, contract point for #62) ------------
    //
    // Mirrors the `LlmSettings`/`llm.json` store above: a static Mutex cache, an `init_*` loader
    // called once at setup, and a path helper under the app-data dir. The KEY difference is intent
    // — this is a NON-secret user preference, so a JSON file is the right home (secrets stay in the
    // Keychain, invariant 7). Default is OFF: analytics is opt-IN, never on until the user says so.

    #[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
    pub struct PrivacyPrefs {
        /// Anonymous, aggregated usage stats. OFF by default (opt-in) — #28 §9-1. The `bool`
        /// default (`false`) IS the opt-out default, so `#[derive(Default)]` is load-bearing here:
        /// a fresh install, or a `privacy.json` that fails to parse, lands on analytics OFF.
        pub analytics_enabled: bool,
    }

    static PRIVACY_PREFS: std::sync::Mutex<Option<PrivacyPrefs>> = std::sync::Mutex::new(None);

    fn privacy_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        use tauri::Manager;
        app.path().app_data_dir().ok().map(|d| d.join("privacy.json"))
    }

    /// Load persisted privacy prefs into the in-memory copy. Called once at setup (mirrors
    /// `init_llm_settings`). A missing or unreadable file leaves the opt-out default in place.
    pub fn init_privacy_prefs(app: &tauri::AppHandle) {
        let mut p = PrivacyPrefs::default();
        if let Some(path) = privacy_path(app) {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(saved) = serde_json::from_str::<PrivacyPrefs>(&text) {
                    p = saved;
                }
            }
        }
        eprintln!("[inline] analytics enabled = {}", p.analytics_enabled);
        if let Ok(mut g) = PRIVACY_PREFS.lock() {
            *g = Some(p);
        }
    }

    /// The one gate every analytics/telemetry send MUST pass through (#28; the contract point for
    /// the #62 PostHog work). Fails closed: if the cache is unreadable or unset, analytics is OFF.
    #[allow(dead_code)]
    pub fn analytics_enabled() -> bool {
        PRIVACY_PREFS
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_default()
            .analytics_enabled
    }

    /// Current privacy prefs for the Settings UI. Defaults (analytics OFF) when unset.
    #[tauri::command]
    pub fn get_privacy_prefs() -> PrivacyPrefs {
        PRIVACY_PREFS.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
    }

    /// Turn anonymous usage stats on or off, persisted to `privacy.json`. The value the user sets
    /// here is the only thing `analytics_enabled()` (and therefore any future send) reads.
    #[tauri::command]
    pub fn set_analytics_enabled(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
        let p = PrivacyPrefs { analytics_enabled: enabled };
        if let Some(path) = privacy_path(&app) {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let json = serde_json::to_string_pretty(&p).map_err(|e| e.to_string())?;
            std::fs::write(&path, json).map_err(|e| format!("save failed: {e}"))?;
        }
        if let Ok(mut g) = PRIVACY_PREFS.lock() {
            *g = Some(p);
        }
        eprintln!("[inline] analytics_enabled → {enabled}");
        Ok(())
    }

    // ---- AX helpers -------------------------------------------------------------------------

    /// Copy a string attribute off an element (create rule; released here). `None` if absent or not
    /// a string. Sets the 100ms messaging timeout on the systemwide element by the caller.
    unsafe fn copy_string(el: AXUIElementRef, name: &str) -> Option<String> {
        let cf_name = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        let err = unsafe { AXUIElementCopyAttributeValue(el, cf_name.as_concrete_TypeRef(), &mut value) };
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

    /// Copy the focused UI element (create rule → caller releases).
    unsafe fn focused_element() -> Option<AXUIElementRef> {
        let sys = unsafe { AXUIElementCreateSystemWide() };
        if sys.is_null() {
            return None;
        }
        unsafe { AXUIElementSetMessagingTimeout(sys, 0.25) };
        let cf_name = CFString::new(kAXFocusedUIElementAttribute);
        let mut value: CFTypeRef = std::ptr::null();
        let err = unsafe { AXUIElementCopyAttributeValue(sys, cf_name.as_concrete_TypeRef(), &mut value) };
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
        fn CGEventCreateKeyboardEvent(source: *const std::ffi::c_void, keycode: u16, key_down: bool) -> *mut std::ffi::c_void;
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
    unsafe fn paste_at_cursor(text: &str, el: AXUIElementRef, before: Option<&str>) -> Result<(), String> {
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

    /// Reads the focused field's text (AX). v1 treats the whole value as the text *before* the caret
    /// (drafting at the end of a field is the common case); precise caret splitting via
    /// `kAXSelectedTextRangeAttribute` is a device refinement. Never a screenshot (invariant 2).
    pub struct AxCursorReader;

    impl CursorReader for AxCursorReader {
        fn read(&self) -> Option<CursorContext> {
            // SAFETY: focused_element returns a live +1 element we release before returning.
            let el = unsafe { focused_element() }?;
            // The presence of the AXValue ATTRIBUTE is the "this is a text-carrying field" signal —
            // Some("") is a focused EMPTY field (the most common draft target: a fresh reply, a
            // blank doc) and must produce a context. Only an element with no value attribute at
            // all (a button, an icon) yields None.
            let value = unsafe { copy_string(el, kAXValueAttribute) };
            let field_label = unsafe { copy_string(el, kAXTitleAttribute) }.unwrap_or_default();
            unsafe { CFRelease(el as CFTypeRef) };
            let app = crate::display::frontmost_app().map(|f| f.bundle_id).unwrap_or_default();
            value.map(|before| CursorContext { app, field_label, before, after: String::new() })
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
            let cf_attr = CFString::new(kAXSelectedTextAttribute);
            let cf_text = CFString::new(text);
            // SAFETY: el is a live element; attr + value are valid CFStrings.
            let err = unsafe {
                AXUIElementSetAttributeValue(el, cf_attr.as_concrete_TypeRef(), cf_text.as_concrete_TypeRef() as CFTypeRef)
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
            let app = crate::display::frontmost_app().map(|f| f.bundle_id).unwrap_or_default();
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
        /// Delegated to a vendor CLI on the user's own subscription (Issue #110). No tokio runtime:
        /// the delegate is a subprocess, and the whole inline flow already runs on its own std
        /// thread, so the blocking wait is exactly where it belongs.
        Subscription(SubscriptionAgentClient<ProcessRunner, DbTraceabilitySink>),
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
                InlineAgent::Subscription(client) => client.complete(prompt),
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
        // Only BYOK providers, deliberately: a subscription delegate has no key, and accepting one
        // under its id would file a real credential under an account nothing ever reads — a secret
        // at rest for no purpose (invariant 7).
        if !BYOK_PROVIDERS.contains(&provider.as_str()) {
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
        if !BYOK_PROVIDERS.contains(&provider.as_str()) {
            return Err(format!("unknown provider: {provider}"));
        }
        keychain_store::delete_generic_secret(keychain_account(&provider))
            .map_err(|e| e.to_string())?;
        eprintln!("[inline] BYOK key removed from Keychain (provider: {provider})");
        refresh_has_key();
        Ok(())
    }

    /// Last 4 characters of the ACTIVE provider's BYOK key for the Settings read-back echo, or
    /// `None` when no key is set. Only the 4-char suffix crosses to the webview — the full key
    /// stays in Rust and is NEVER returned or logged (invariant 7 / NFR-SEC-02). A key too short
    /// to have 4 chars is masked entirely rather than echoed.
    #[tauri::command]
    pub fn byok_key_last4() -> Option<String> {
        let provider = current_settings().provider;
        keychain_byok(&provider).and_then(|k| {
            let k = k.trim();
            if k.is_empty() {
                return None;
            }
            // Mask entirely rather than echo a key too short to have a 4-char suffix, so the
            // full secret is never returned (invariant 7). Otherwise take the last 4 only.
            if k.chars().count() < 4 {
                Some("····".to_string())
            } else {
                Some(Secret::new(k).last4())
            }
        })
    }

    // ---- subscription delegates (Issue #110) -------------------------------------------------

    /// One delegate as the onboarding / Settings UI sees it. Carries no credential and no captured
    /// text — a name, a plan label, a state tag, and the CLI's version string.
    #[derive(serde::Serialize)]
    pub struct DelegateInfo {
        pub id: String,
        pub label: String,
        /// Whose quota a run is billed against, e.g. "Claude Pro / Max".
        pub plan: String,
        /// The binary looked up on PATH — shown in the "not installed" instructions.
        pub binary: String,
        /// `not_installed` / `installed` / `ready` / `needs_login` / `rate_limited`.
        pub state: String,
        pub version: String,
    }

    fn delegate_info(d: Delegate, state: &DelegateState) -> DelegateInfo {
        DelegateInfo {
            id: d.id().to_string(),
            label: d.label().to_string(),
            plan: d.plan_label().to_string(),
            binary: d.binary().to_string(),
            state: state.tag().to_string(),
            version: match state {
                DelegateState::NotInstalled => String::new(),
                DelegateState::Installed { version }
                | DelegateState::Ready { version }
                | DelegateState::NeedsLogin { version }
                | DelegateState::RateLimited { version } => version.clone(),
            },
        }
    }

    /// Which vendor CLIs are installed, so onboarding can lead with "use the plan you already pay
    /// for" instead of an API-key field.
    ///
    /// Presence only (`--version`): no network, no quota, and no reading of the delegates' stored
    /// credentials. A delegate never reports `ready` from this call — that needs
    /// [`verify_subscription_delegate`], which the user triggers.
    #[tauri::command]
    pub fn subscription_delegates() -> Vec<DelegateInfo> {
        let found = subscription::detect_all(&ProcessRunner);
        let infos: Vec<DelegateInfo> =
            found.iter().map(|(d, state)| delegate_info(*d, state)).collect();
        eprintln!(
            "[inline] subscription delegates: {}",
            infos.iter().map(|i| format!("{}={}", i.id, i.state)).collect::<Vec<_>>().join(" ")
        );
        infos
    }

    /// Run one minimal completion through `id` to find out whether it is actually signed in.
    ///
    /// User-triggered ("Test connection") because it spends a token or two of their quota. It is
    /// also the only honest check available: the alternative is reading the delegate's credential
    /// file, which SHOGUN does not do.
    #[tauri::command]
    pub fn verify_subscription_delegate(id: String) -> Result<DelegateInfo, String> {
        let d = Delegate::parse(&id).ok_or_else(|| format!("unknown delegate: {id}"))?;
        let state = subscription::verify(&ProcessRunner, d);
        eprintln!("[inline] verified {} → {}", d.id(), state.tag());
        // A successful verify clears a stale "credential rejected" pill for the active provider.
        if state.is_usable() && current_settings().provider == d.id() {
            KEY_REJECTED.store(false, std::sync::atomic::Ordering::Relaxed);
            HAS_KEY.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(delegate_info(d, &state))
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

        // Subscription delegation (Issue #110): no key to read, so this branch comes first.
        if let Some(d) = delegate_for(&s.provider) {
            if !s.subscription_consent {
                // The disclosure was never accepted. Running anyway would send the user's text
                // through a consumer plan they never agreed to use for this.
                eprintln!("[inline] {} selected but the disclosure is unaccepted — not drafting", d.id());
                return None;
            }
            eprintln!("[inline] live Agent lane — delegating to {} on the user's own plan", d.id());
            return Some(InlineAgent::Subscription(SubscriptionAgentClient::new(
                ProcessRunner,
                db.traceability_sink(),
                d,
            )));
        }

        let Some(key) = keychain_byok(&s.provider) else {
            if mock_agent_enabled() {
                eprintln!("[inline] SHOGUN_MOCK_AGENT=1 — echo mock (AX path still runs)");
                return Some(InlineAgent::Mock(MockAgentClient::new(ByokKey::new(Secret::new("mock")))));
            }
            eprintln!("[inline] no key in Keychain for provider '{}' — not drafting", s.provider);
            return None;
        };
        let model = effective_model(&s);
        match (ReqwestTransport::new(), tokio::runtime::Builder::new_current_thread().enable_all().build()) {
            (Ok(transport), Ok(rt)) => {
                let byok = ByokKey::new(Secret::new(key));
                eprintln!("[inline] live Agent lane — provider {} model {}", s.provider, loggable_model(&model));
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
        push_inline(&app, InlineStatus { phase: "drafting", chars: 0, detail: None });
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
                push_inline(&app, InlineStatus { phase: "no_key", chars: 0, detail: None });
                return;
            };
            let outcome = compose_inline(&AxCursorReader, &agent, &AxTextInserter, &memory);
            match &outcome {
                InlineOutcome::Inserted { chars } => {
                    eprintln!("[inline] inserted {chars} chars at the cursor");
                    note_key_accepted();
                    push_inline(&app, InlineStatus { phase: "inserted", chars: *chars, detail: None });
                }
                // Nothing gets inserted on a rejected key, so without this the tap is silent and
                // the reasonable next move is to press it again. Latch it for the status poll.
                InlineOutcome::KeyRejected(why) => {
                    eprintln!("[inline] {why}");
                    note_key_rejected();
                    push_inline(&app, InlineStatus { phase: "key_rejected", chars: 0, detail: Some(why.clone()) });
                }
                InlineOutcome::NoContext => {
                    eprintln!("[inline] no editable field under the caret");
                    push_inline(&app, InlineStatus { phase: "no_context", chars: 0, detail: None });
                }
                other => {
                    eprintln!("[inline] {other:?}");
                    // The reason, not the content: these carry provider/AX errors, never the
                    // draft or anything the user typed.
                    let detail = match other {
                        InlineOutcome::GenerationFailed(e) | InlineOutcome::InsertFailed(e) => Some(e.clone()),
                        _ => None,
                    };
                    push_inline(&app, InlineStatus { phase: "failed", chars: 0, detail });
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
        let app = crate::display::frontmost_app().map(|f| f.bundle_id).unwrap_or_default();
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
                    meta: if overdue { "overdue".into() } else { format!("{:.0}% sure", c.confidence * 100.0) },
                    text: c.description,
                }
            })
            .collect();
        let open_loops = db
            .open_loop_rows()
            .into_iter()
            .filter(|l| l.status != "closed")
            .map(|l| StateItem { id: l.id, meta: format!("{}d waiting", l.staleness_days), text: l.description })
            .collect();
        StateView { commitments, open_loops }
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

    /// Delete user data captured within the last `range` window (#28). `range` is "1h" or "24h".
    /// Local and immediate — nothing is sent anywhere. Returns the per-table deletion report.
    #[tauri::command]
    pub fn delete_data_since(range: String, db: tauri::State<'_, Db>) -> Result<String, String> {
        let window_ms: i64 = match range.as_str() {
            "1h" => 60 * 60 * 1000,
            "24h" => 24 * 60 * 60 * 1000,
            other => return Err(format!("unknown range: {other}")),
        };
        let cutoff = db.now_ms() - window_ms;
        let report = db.delete_since(cutoff).ok_or_else(|| "deletion failed".to_string())?;
        eprintln!("[shell] delete_data_since {range} — events={} people={} commitments={}",
            report.events, report.people, report.commitments);
        serde_json::to_string(&report).map_err(|e| e.to_string())
    }

    /// Delete ALL user data and every user-held secret, then clear the account's local state (#28).
    ///
    /// Wipes the memory DB rows (schema + encrypted DB file are KEPT so the app keeps working) and
    /// removes, best-effort, every user secret from the Keychain:
    ///   - all BYOK provider keys (`<provider>-byok`),
    ///   - all OAuth token sets for connected services (`<source>-tokenset`),
    ///   - the Composio API key (`composio-api-key`).
    ///
    /// The local DB encryption key (`memory-db-key`) is intentionally KEPT: `delete_all` clears the
    /// rows but leaves the encrypted DB file and schema in place, so deleting its key would brick a
    /// database the app still needs to open. A missing Keychain entry is a no-op.
    #[tauri::command]
    pub fn delete_all_and_account(db: tauri::State<'_, Db>) -> Result<String, String> {
        let report = db.delete_all().ok_or_else(|| "deletion failed".to_string())?;
        // BYOK provider keys.
        for provider in PROVIDERS {
            let _ = security_framework::passwords::delete_generic_password(
                KEYCHAIN_SERVICE, keychain_account(provider));
        }
        // OAuth token sets for every connectable service (`<source>-tokenset`).
        let token_store = shogun_integrations::KeychainTokenStore::new(KEYCHAIN_SERVICE);
        for service in shogun_mcp::scope::ALL_SERVICES {
            use shogun_integrations::TokenStore;
            let _ = token_store.delete(*service);
        }
        // The Composio API key (reuse the approvals command's not-found-tolerant helper).
        let _ = crate::approvals::mac::clear_composio_key();
        refresh_has_key();
        eprintln!(
            "[shell] delete_all_and_account — all user data, BYOK keys, OAuth token sets and the \
             Composio key removed (DB encryption key kept)"
        );
        serde_json::to_string(&report).map_err(|e| e.to_string())
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
        let facts: Vec<&str> = ctx.facts.iter().map(|m| m.trim()).filter(|m| !m.is_empty()).collect();
        if !facts.is_empty() {
            p.push_str("\nWhat you remember about their work:\n");
            for m in facts {
                p.push_str("- ");
                p.push_str(m);
                p.push('\n');
            }
        }
        if !ctx.screen_frames.is_empty() {
            p.push_str("\nStored screen captures (JPEG, ≤72 h, linked to OCR events):\n");
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
            let Some(rec) = db.get_screen_frame(frame.frame_id) else { continue };
            let Some(text) = crate::screen_ocr::ocr_jpeg_bytes(&rec.jpeg) else { continue };
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
        let Some(agent) = build_agent(db) else { return no_key() };
        if !agent.is_live() {
            return no_key();
        }
        let text = agent.complete(&build_chat_prompt(message, &ctx)).map_err(|e| {
            // Same latch as the ⌥-tap path: chat surfaces the error text, but Settings is where
            // the fix is, and it needs to know the key is the problem.
            if matches!(e, LlmError::Unauthorized(..)) {
                note_key_rejected();
            }
            e.to_string()
        })?;
        // The provider just accepted the key — clear a stale rejected verdict (transient 401s
        // must not stick until the user re-saves the same key).
        note_key_accepted();
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

    #[cfg(test)]
    mod tests {
        use super::{analytics_enabled, PrivacyPrefs, PRIVACY_PREFS};

        /// Analytics is opt-IN: a fresh `PrivacyPrefs` (a new install, or a `privacy.json` that
        /// fails to parse and falls back to the default) must have stats OFF. The `bool` default
        /// carries this, so the test guards against anyone changing the field default under it.
        #[test]
        fn default_prefs_have_analytics_off() {
            assert!(!PrivacyPrefs::default().analytics_enabled, "opt-in: default must be OFF");
        }

        /// Fail-closed: with the cache unset (`None`) — before `init_privacy_prefs` runs, or if the
        /// lock is poisoned — the gate every send passes through must report OFF, never ON. The
        /// static starts as `None`; no test in this module ever populates it, so this reads the
        /// genuine unset path.
        #[test]
        fn analytics_gate_is_closed_when_cache_is_unset() {
            assert!(
                PRIVACY_PREFS.lock().map(|g| g.is_none()).unwrap_or(true),
                "precondition: the prefs cache is unset in this test process",
            );
            assert!(!analytics_enabled(), "fail-closed: unset cache must gate analytics OFF");
        }
    }
}
