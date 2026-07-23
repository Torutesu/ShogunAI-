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

    use shogun_core::daemon::Db;
    use shogun_core::db_sink::DbTraceabilitySink;
    use shogun_core::inline::{compose_inline, CursorContext, CursorReader, InlineOutcome, TextInserter};
    use shogun_core::llm::anthropic::{AnthropicAgentClient, AnthropicConfig};
    use shogun_core::llm::openai_compat::{
        OpenAiCompatAgentClient, OpenAiCompatConfig, OPENAI_BASE_URL, OPENROUTER_BASE_URL,
    };
    use shogun_core::llm::transport::ReqwestTransport;
    use shogun_core::llm::{AgentClient, ByokKey, LlmError, MockAgentClient, Secret};

    /// The Keychain service the BYOK keys live under (invariant 7).
    const KEYCHAIN_SERVICE: &str = "com.selectkk.shogun";

    // ---- Agent-lane provider settings (provider + model; NON-secret, so a JSON file is fine —
    // ---- the KEY always stays in the Keychain, one account per provider) ----------------------

    /// Providers the Agent lane can run on. The Batch lane (indexing / Dream Cycle) is untouched
    /// by this choice — it stays on the Select KK lane (invariant 5).
    const PROVIDERS: [&str; 3] = ["anthropic", "openrouter", "openai"];

    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    pub struct LlmSettings {
        pub provider: String,
        /// Model id in the provider's own naming. Empty = the provider's default.
        pub model: String,
    }

    impl Default for LlmSettings {
        fn default() -> Self {
            Self { provider: "anthropic".into(), model: String::new() }
        }
    }

    /// The provider's default model when the user hasn't set one.
    fn default_model(provider: &str) -> &'static str {
        match provider {
            "openrouter" => "anthropic/claude-sonnet-4.5",
            "openai" => "gpt-4o-mini",
            _ => "claude-sonnet-5",
        }
    }

    /// One Keychain account per provider, so switching providers never overwrites another key.
    fn keychain_account(provider: &str) -> &'static str {
        match provider {
            "openrouter" => "openrouter-byok",
            "openai" => "openai-byok",
            _ => "anthropic-byok",
        }
    }

    static LLM_SETTINGS: std::sync::Mutex<Option<LlmSettings>> = std::sync::Mutex::new(None);

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
        eprintln!("[inline] agent provider = {} model = {}", s.provider, effective_model(&s));
        if let Ok(mut g) = LLM_SETTINGS.lock() {
            *g = Some(s);
        }
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
    #[tauri::command]
    pub fn set_llm_settings(provider: String, model: String, app: tauri::AppHandle) -> Result<(), String> {
        if !PROVIDERS.contains(&provider.as_str()) {
            return Err(format!("unknown provider: {provider}"));
        }
        let s = LlmSettings { provider, model: model.trim().to_string() };
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
        eprintln!("[inline] agent provider → {} model → {}", s.provider, effective_model(&s));
        if let Ok(mut g) = LLM_SETTINGS.lock() {
            *g = Some(s);
        }
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
    pub struct AxTextInserter;

    impl TextInserter for AxTextInserter {
        fn insert(&self, text: &str) -> Result<(), String> {
            let el = unsafe { focused_element() }.ok_or_else(|| "no focused field".to_string())?;
            let cf_attr = CFString::new(kAXSelectedTextAttribute);
            let cf_text = CFString::new(text);
            // SAFETY: el is a live element; attr + value are valid CFStrings.
            let err = unsafe {
                AXUIElementSetAttributeValue(el, cf_attr.as_concrete_TypeRef(), cf_text.as_concrete_TypeRef() as CFTypeRef)
            };
            unsafe { CFRelease(el as CFTypeRef) };
            if err == kAXErrorSuccess {
                Ok(())
            } else {
                Err(format!("AX set selected text failed: {err}"))
            }
        }
    }

    // ---- BYOK Agent-lane client (Keychain → real, else mock) --------------------------------

    /// The Agent-lane client for inline drafts and chat. Real when the ACTIVE provider has a key
    /// in the Keychain; otherwise a mock that echoes the prompt (so the AX read→insert loop is
    /// testable on device without a key).
    enum InlineAgent {
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
        security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, keychain_account(provider))
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
        security_framework::passwords::set_generic_password(
            KEYCHAIN_SERVICE,
            keychain_account(&provider),
            key.as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        eprintln!("[inline] BYOK key saved to Keychain (provider: {provider})");
        Ok(())
    }

    /// Remove `provider`'s BYOK key — chat and drafts fall back to the echo mock.
    #[tauri::command]
    pub fn clear_byok_key(provider: String) -> Result<(), String> {
        if !PROVIDERS.contains(&provider.as_str()) {
            return Err(format!("unknown provider: {provider}"));
        }
        security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, keychain_account(&provider))
            .map_err(|e| e.to_string())?;
        eprintln!("[inline] BYOK key removed from Keychain (provider: {provider})");
        Ok(())
    }

    /// Build the Agent client for this run from the current provider settings. Falls back to the
    /// mock (with a clear log) whenever the key is absent or the transport/runtime can't be built.
    fn build_agent(db: &Db) -> InlineAgent {
        let s = current_settings();
        let Some(key) = keychain_byok(&s.provider) else {
            eprintln!(
                "[inline] no key in Keychain for provider '{}' — using echo mock (AX path still runs)",
                s.provider
            );
            return InlineAgent::Mock(MockAgentClient::new(ByokKey::new(Secret::new("mock"))));
        };
        let model = effective_model(&s);
        match (ReqwestTransport::new(), tokio::runtime::Builder::new_current_thread().enable_all().build()) {
            (Ok(transport), Ok(rt)) => {
                let byok = ByokKey::new(Secret::new(key));
                eprintln!("[inline] live Agent lane — provider {} model {model}", s.provider);
                match s.provider.as_str() {
                    "openrouter" | "openai" => {
                        let base = if s.provider == "openrouter" { OPENROUTER_BASE_URL } else { OPENAI_BASE_URL };
                        let client = OpenAiCompatAgentClient::new(
                            transport,
                            db.traceability_sink(),
                            byok,
                            OpenAiCompatConfig::new(base, model),
                        );
                        InlineAgent::OpenAiCompat { rt, client }
                    }
                    _ => {
                        let client = AnthropicAgentClient::new(
                            transport,
                            db.traceability_sink(),
                            byok,
                            AnthropicConfig::new(model),
                        );
                        InlineAgent::Anthropic { rt, client }
                    }
                }
            }
            _ => {
                eprintln!("[inline] transport/runtime unavailable — using echo mock");
                InlineAgent::Mock(MockAgentClient::new(ByokKey::new(Secret::new("mock"))))
            }
        }
    }

    // ---- trigger ----------------------------------------------------------------------------

    /// Run the inline draft: on a dedicated thread (so the AX reads/writes and the blocking Agent
    /// call don't touch a tokio worker), read the caret context, gather confidence-gated memory,
    /// generate, and insert at the caret. Fire-and-forget — the outcome is logged (no captured text).
    pub fn run_inline_at_cursor(db: Db) {
        std::thread::spawn(move || {
            let memory = db.inline_memory(6);
            let agent = build_agent(&db);
            let outcome = compose_inline(&AxCursorReader, &agent, &AxTextInserter, &memory);
            match &outcome {
                InlineOutcome::Inserted { chars } => eprintln!("[inline] inserted {chars} chars at the cursor"),
                other => eprintln!("[inline] {other:?}"),
            }
        });
    }

    /// Tauri command: the notch "draft at cursor" action and the shortcut both call this.
    #[tauri::command]
    pub fn inline_at_cursor(db: tauri::State<'_, Db>) -> &'static str {
        run_inline_at_cursor(db.inner().clone());
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
    }

    #[tauri::command]
    pub fn shogun_status(db: tauri::State<'_, Db>) -> Status {
        let app = crate::display::frontmost_app().map(|f| f.bundle_id).unwrap_or_default();
        Status {
            app,
            commitments: db.commitments_due(db.now_ms()).len(),
            open_loops: db.open_loops().len(),
            has_key: keychain_byok(&current_settings().provider).is_some(),
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

    /// One state row for the "What I know" panel.
    #[derive(serde::Serialize)]
    pub struct StateItem {
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
        let commitments = db
            .commitments_due(now)
            .into_iter()
            .map(|c| StateItem {
                meta: if c.overdue { "overdue".into() } else { format!("{:.0}% sure", c.confidence * 100.0) },
                text: c.description,
            })
            .collect();
        let open_loops = db
            .open_loops()
            .into_iter()
            .map(|l| StateItem { meta: format!("{}d waiting", l.staleness_days), text: l.description })
            .collect();
        StateView { commitments, open_loops }
    }

    /// Build the chat prompt: the user's message grounded in confidence-gated memory (FR-ST-20).
    fn build_chat_prompt(message: &str, memory: &[String]) -> String {
        let mut p = String::from(
            "You are SHOGUN, the user's private work assistant on their Mac. Answer grounded in what \
             you remember about their work. Be concise, concrete, and useful — no filler.\n",
        );
        let facts: Vec<&str> = memory.iter().map(|m| m.trim()).filter(|m| !m.is_empty()).collect();
        if !facts.is_empty() {
            p.push_str("\nWhat you remember about their work:\n");
            for m in facts {
                p.push_str("- ");
                p.push_str(m);
                p.push('\n');
            }
        }
        p.push_str("\nUser: ");
        p.push_str(message);
        p.push_str("\nSHOGUN:");
        p
    }

    fn chat_blocking(db: &Db, message: &str) -> Result<String, String> {
        let memory = db.inline_memory(8);
        let agent = build_agent(db);
        agent.complete(&build_chat_prompt(message, &memory)).map_err(|e| e.to_string())
    }

    /// Chat with SHOGUN, grounded in memory, on the BYOK Agent lane. Runs the blocking generation on
    /// a blocking thread so it never touches a tokio worker. Records one egress trace (invariant 3).
    #[tauri::command]
    pub async fn shogun_chat(message: String, db: tauri::State<'_, Db>) -> Result<String, String> {
        let db = db.inner().clone();
        tokio::task::spawn_blocking(move || chat_blocking(&db, &message))
            .await
            .map_err(|e| e.to_string())?
    }
}
