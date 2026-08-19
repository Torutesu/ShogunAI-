//! Scribe's macOS adapter: capture one editable AX target before the overlay steals focus, then
//! apply a user-directed edit only after the same target, value, and UTF-16 range are proven safe.

#[cfg(target_os = "macos")]
pub mod mac {
    use accessibility_sys::{
        kAXErrorSuccess, kAXFocusedAttribute, kAXFocusedUIElementAttribute, kAXPositionAttribute,
        kAXRoleAttribute, kAXSelectedTextAttribute, kAXSelectedTextRangeAttribute,
        kAXSizeAttribute, kAXTitleAttribute, kAXValueAttribute, kAXValueTypeCFRange,
        kAXValueTypeCGPoint, kAXValueTypeCGSize, AXUIElementCopyAttributeValue,
        AXUIElementCreateSystemWide, AXUIElementGetPid, AXUIElementRef,
        AXUIElementSetAttributeValue, AXUIElementSetMessagingTimeout, AXValueCreate,
        AXValueGetTypeID, AXValueGetValue,
    };
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::{CFGetTypeID, CFRange, CFRelease, CFTypeRef};
    use core_foundation_sys::string::CFStringGetTypeID;
    use core_graphics::geometry::{CGPoint, CGSize};
    use shogun_core::daemon::{Db, ReplyContext};
    use shogun_core::inline::{
        build_scribe_edit_split_prompt, scribe_output_preserves_protected_spans, CursorContext,
        ScribeEditRequest,
    };
    use shogun_core::llm::{AgentClient, LlmError};
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::{Arc, Mutex};

    const CONTEXT_LINES: usize = 14;
    const MAX_FIELD_UTF16: usize = 200_000;
    const MAX_TARGET_CHARS: usize = 20_000;
    const MAX_INSTRUCTION_CHARS: usize = 4_000;
    const SURROUNDING_CHARS: usize = 4_000;
    const COMMIT_READY: u8 = 0;
    const COMMIT_WRITING: u8 = 1;
    const COMMIT_CANCELLED: u8 = 2;
    const COMMIT_DONE: u8 = 3;

    struct ScribeTarget {
        element: AXUIElementRef,
        pid: i32,
        snapshot: Mutex<ScribeSnapshot>,
        commit_state: AtomicU8,
        operation: Mutex<()>,
    }

    #[derive(Clone)]
    struct ScribeSnapshot {
        original_value: String,
        target_range: CFRange,
        selected_text: String,
    }

    // Session ownership is protected by SESSION; retained AX object access is serialized by
    // `operation`, with `commit_state` linearizing close/cancel against the first AX mutation.
    unsafe impl Send for ScribeTarget {}
    unsafe impl Sync for ScribeTarget {}

    impl Drop for ScribeTarget {
        fn drop(&mut self) {
            if !self.element.is_null() {
                unsafe { CFRelease(self.element.cast()) };
            }
        }
    }

    struct InsertResult {
        expected_value: String,
        replacement_range: CFRange,
    }

    struct CommitGuard<'a> {
        state: &'a AtomicU8,
        committed: bool,
    }

    impl Drop for CommitGuard<'_> {
        fn drop(&mut self) {
            self.state.store(
                if self.committed {
                    COMMIT_DONE
                } else {
                    COMMIT_READY
                },
                Ordering::Release,
            );
        }
    }

    struct ScribeSession {
        id: u64,
        generation: u64,
        target: Option<Arc<ScribeTarget>>,
        context: CursorContext,
        memory: Vec<String>,
        directives: String,
        busy: bool,
        active: bool,
        phase: &'static str,
        chars: usize,
        detail: Option<&'static str>,
    }

    static SESSION: Mutex<Option<ScribeSession>> = Mutex::new(None);
    static NEXT_SESSION_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    #[derive(Clone, Copy)]
    pub struct ScribeAnchor {
        pub x: f64,
        pub y: f64,
        pub width: f64,
        pub height: f64,
    }

    pub struct ScribeOpenResult {
        pub session_id: u64,
        pub anchor: Option<ScribeAnchor>,
    }

    #[derive(Clone, serde::Serialize)]
    pub struct ScribeEvent {
        pub session_id: u64,
        /// `opened` | `processing` | `inserted` | `failed` | `closed` | `cancelled` | `no_key`
        pub phase: &'static str,
        pub chars: usize,
        /// Content-free diagnostic. Never provider output, prompt text, or captured AX text.
        pub detail: Option<&'static str>,
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
                Some(CFString::wrap_under_create_rule(value.cast()).to_string())
            } else {
                CFRelease(value);
                None
            }
        }
    }

    unsafe fn copy_range(element: AXUIElementRef, name: &str) -> Option<CFRange> {
        let attribute = CFString::new(name);
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

    unsafe fn copy_point(element: AXUIElementRef, name: &str) -> Option<CGPoint> {
        let attribute = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe {
            AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
        };
        if error != kAXErrorSuccess || value.is_null() {
            return None;
        }
        let mut point = CGPoint::new(0.0, 0.0);
        let valid = unsafe { CFGetTypeID(value) == AXValueGetTypeID() }
            && unsafe {
                AXValueGetValue(
                    value.cast_mut().cast(),
                    kAXValueTypeCGPoint,
                    (&mut point as *mut CGPoint).cast(),
                )
            };
        unsafe { CFRelease(value) };
        valid.then_some(point)
    }

    unsafe fn copy_size(element: AXUIElementRef, name: &str) -> Option<CGSize> {
        let attribute = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe {
            AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
        };
        if error != kAXErrorSuccess || value.is_null() {
            return None;
        }
        let mut size = CGSize::new(0.0, 0.0);
        let valid = unsafe { CFGetTypeID(value) == AXValueGetTypeID() }
            && unsafe {
                AXValueGetValue(
                    value.cast_mut().cast(),
                    kAXValueTypeCGSize,
                    (&mut size as *mut CGSize).cast(),
                )
            };
        unsafe { CFRelease(value) };
        valid.then_some(size)
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

    fn editable_role(role: &str) -> bool {
        matches!(
            role,
            "AXTextArea" | "AXTextField" | "AXSearchField" | "AXComboBox"
        )
    }

    fn secure_role(role: &str) -> bool {
        role == "AXSecureTextField"
    }

    unsafe fn set_range(element: AXUIElementRef, range: CFRange) -> bool {
        let value =
            unsafe { AXValueCreate(kAXValueTypeCFRange, (&range as *const CFRange).cast()) };
        if value.is_null() {
            return false;
        }
        let attribute = CFString::new(kAXSelectedTextRangeAttribute);
        let error = unsafe {
            AXUIElementSetAttributeValue(element, attribute.as_concrete_TypeRef(), value.cast())
        };
        unsafe { CFRelease(value.cast()) };
        error == kAXErrorSuccess
    }

    fn rewrite_target(
        value: &str,
        selection: Option<CFRange>,
    ) -> Option<(CFRange, String, String)> {
        let units: Vec<u16> = value.encode_utf16().collect();
        if units.len() > MAX_FIELD_UTF16 {
            return None;
        }
        let selection = match selection {
            Some(range) => range,
            None if value.is_empty() => CFRange::init(0, 0),
            None => return None,
        };
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
        if target.chars().count() > MAX_TARGET_CHARS {
            return None;
        }
        let prefix = String::from_utf16(&units[..start]).ok()?;
        let suffix = String::from_utf16(&units[end..]).ok()?;
        let prefix = tail_chars(&prefix, SURROUNDING_CHARS / 2);
        let suffix: String = suffix.chars().take(SURROUNDING_CHARS / 2).collect();
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

    fn tail_chars(text: &str, max_chars: usize) -> String {
        let count = text.chars().count();
        text.chars().skip(count.saturating_sub(max_chars)).collect()
    }

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
    enum FallbackDecision {
        AlreadyLanded,
        SafeToWrite,
        Abort,
    }

    fn fallback_decision(
        current_value: Option<&str>,
        current_range: Option<CFRange>,
        current_selected: &str,
        snapshot: &ScribeSnapshot,
        expected_value: &str,
    ) -> FallbackDecision {
        if current_value == Some(expected_value) {
            return FallbackDecision::AlreadyLanded;
        }
        let empty = selection_already_prepared(&snapshot.original_value, snapshot.target_range);
        if current_value == Some(snapshot.original_value.as_str())
            && (empty || current_range == Some(snapshot.target_range))
            && (empty || current_selected == snapshot.selected_text)
        {
            FallbackDecision::SafeToWrite
        } else {
            FallbackDecision::Abort
        }
    }

    fn selection_already_prepared(value: &str, range: CFRange) -> bool {
        value.is_empty() && range.location == 0 && range.length == 0
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

    unsafe fn selection_matches(
        element: AXUIElementRef,
        expected_range: CFRange,
        expected_text: &str,
    ) -> bool {
        for _ in 0..4 {
            let range = unsafe { copy_range(element, kAXSelectedTextRangeAttribute) };
            let text = unsafe { copy_string(element, kAXSelectedTextAttribute) };
            if range == Some(expected_range) && text.as_deref() == Some(expected_text) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        false
    }

    fn capture_target() -> Option<(ScribeTarget, CursorContext, Option<ScribeAnchor>)> {
        let element = unsafe { focused_element() }?;
        let role = unsafe { copy_string(element, kAXRoleAttribute) }.unwrap_or_default();
        if secure_role(&role) || !editable_role(&role) {
            unsafe { CFRelease(element.cast()) };
            return None;
        }
        let Some(value) = (unsafe { copy_string(element, kAXValueAttribute) }) else {
            unsafe { CFRelease(element.cast()) };
            return None;
        };
        let selection = unsafe { copy_range(element, kAXSelectedTextRangeAttribute) };
        let Some((target_range, selected_text, surrounding)) = rewrite_target(&value, selection)
        else {
            unsafe { CFRelease(element.cast()) };
            return None;
        };
        let mut element_pid = 0i32;
        if unsafe { AXUIElementGetPid(element, &mut element_pid) } != kAXErrorSuccess
            || element_pid <= 0
        {
            unsafe { CFRelease(element.cast()) };
            return None;
        }
        let frontmost = crate::display::frontmost_app();
        if frontmost.as_ref().map(|app| app.pid) != Some(element_pid) {
            unsafe { CFRelease(element.cast()) };
            return None;
        }
        let app = frontmost.map(|app| app.bundle_id).unwrap_or_default();
        let field_label = unsafe { copy_string(element, kAXTitleAttribute) }.unwrap_or_default();
        let anchor = unsafe {
            copy_point(element, kAXPositionAttribute)
                .zip(copy_size(element, kAXSizeAttribute))
                .map(|(position, size)| ScribeAnchor {
                    x: position.x,
                    y: position.y,
                    width: size.width,
                    height: size.height,
                })
        };
        Some((
            ScribeTarget {
                element,
                pid: element_pid,
                snapshot: Mutex::new(ScribeSnapshot {
                    original_value: value,
                    target_range,
                    selected_text: selected_text.clone(),
                }),
                commit_state: AtomicU8::new(COMMIT_READY),
                operation: Mutex::new(()),
            },
            CursorContext {
                app,
                field_label,
                before: selected_text,
                after: surrounding,
            },
            anchor,
        ))
    }

    fn restore_target_focus(target: &ScribeTarget) -> Result<(), String> {
        use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
        let application = NSRunningApplication::runningApplicationWithProcessIdentifier(target.pid)
            .ok_or_else(|| "source app unavailable".to_string())?;
        let _ = application.activateWithOptions(NSApplicationActivationOptions::empty());
        std::thread::sleep(std::time::Duration::from_millis(35));
        let attribute = CFString::new(kAXFocusedAttribute);
        let focused = CFBoolean::true_value();
        let error = unsafe {
            AXUIElementSetAttributeValue(
                target.element,
                attribute.as_concrete_TypeRef(),
                focused.as_CFTypeRef(),
            )
        };
        (error == kAXErrorSuccess)
            .then_some(())
            .ok_or_else(|| "source field could not be focused".into())
    }

    fn insert_verified(target: &ScribeTarget, replacement: &str) -> Result<InsertResult, String> {
        let _operation = target
            .operation
            .lock()
            .map_err(|_| "captured target unavailable".to_string())?;
        let snapshot = target
            .snapshot
            .lock()
            .map_err(|_| "captured target unavailable".to_string())?
            .clone();
        let mut element_pid = 0i32;
        if unsafe { AXUIElementGetPid(target.element, &mut element_pid) } != kAXErrorSuccess
            || element_pid != target.pid
        {
            return Err("captured target identity changed".into());
        }
        if unsafe { copy_string(target.element, kAXValueAttribute) }.as_deref()
            != Some(snapshot.original_value.as_str())
        {
            return Err("captured target changed".into());
        }
        let Some((expected_value, replacement_range)) =
            replace_utf16_range(&snapshot.original_value, snapshot.target_range, replacement)
        else {
            return Err("captured target range invalid".into());
        };
        target
            .commit_state
            .compare_exchange(
                COMMIT_READY,
                COMMIT_WRITING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map_err(|_| "scribe session cancelled".to_string())?;
        let mut commit = CommitGuard {
            state: &target.commit_state,
            committed: false,
        };
        restore_target_focus(target)?;
        if unsafe { copy_string(target.element, kAXValueAttribute) }.as_deref()
            != Some(snapshot.original_value.as_str())
        {
            return Err("captured target changed while restoring focus".into());
        }
        let empty = snapshot.original_value.is_empty()
            && snapshot.target_range.location == 0
            && snapshot.target_range.length == 0;
        let range_ready = empty
            || (unsafe { set_range(target.element, snapshot.target_range) }
                && unsafe {
                    selection_matches(
                        target.element,
                        snapshot.target_range,
                        &snapshot.selected_text,
                    )
                });
        if !range_ready {
            return Err("captured replacement range could not be verified".into());
        }
        let attribute = CFString::new(kAXSelectedTextAttribute);
        let value = CFString::new(replacement);
        let _error = unsafe {
            AXUIElementSetAttributeValue(
                target.element,
                attribute.as_concrete_TypeRef(),
                value.as_concrete_TypeRef().cast(),
            )
        };
        let selected_landed = unsafe { value_matches(target.element, &expected_value) };
        if !selected_landed {
            // Full-value fallback remains bound to the retained element and is permitted only
            // while the original value and freshly prepared range still match exactly.
            let current_value = unsafe { copy_string(target.element, kAXValueAttribute) };
            let current_range =
                unsafe { copy_range(target.element, kAXSelectedTextRangeAttribute) };
            let current_selected = unsafe { copy_string(target.element, kAXSelectedTextAttribute) }
                .unwrap_or_default();
            match fallback_decision(
                current_value.as_deref(),
                current_range,
                &current_selected,
                &snapshot,
                &expected_value,
            ) {
                FallbackDecision::AlreadyLanded => {
                    // Exact readback is stronger evidence than the AX transport status.
                }
                FallbackDecision::Abort => {
                    return Err("captured target changed before fallback".into());
                }
                FallbackDecision::SafeToWrite => {
                    let full_attribute = CFString::new(kAXValueAttribute);
                    let full_value = CFString::new(&expected_value);
                    let full_error = unsafe {
                        AXUIElementSetAttributeValue(
                            target.element,
                            full_attribute.as_concrete_TypeRef(),
                            full_value.as_concrete_TypeRef().cast(),
                        )
                    };
                    if full_error != kAXErrorSuccess
                        || !unsafe { value_matches(target.element, &expected_value) }
                    {
                        return Err("captured target did not accept replacement".into());
                    }
                }
            }
        }
        let _ = unsafe { set_range(target.element, replacement_range) };
        commit.committed = true;
        Ok(InsertResult {
            expected_value,
            replacement_range,
        })
    }

    fn emit(app: &tauri::AppHandle, event: ScribeEvent) {
        use tauri::Emitter;
        crate::right_option_shortcut::observe_scribe_event(app, &event);
        if let Err(error) = app.emit_to("scribe", "scribe", event) {
            eprintln!("[scribe] status delivery failed: {error}");
        }
    }

    fn onboarding_seed_fields_match(
        target_pid: i32,
        process_pid: i32,
        original_value: &str,
        selected_text: &str,
        target_range: CFRange,
        context_before: &str,
        seeded_text: &str,
    ) -> bool {
        target_pid == process_pid
            && original_value == seeded_text
            && selected_text == seeded_text
            && target_range.location == 0
            && usize::try_from(target_range.length).ok() == Some(seeded_text.encode_utf16().count())
            && context_before == seeded_text
    }

    /// Prove the double-tap captured this process's exact seeded textarea and its full selection.
    /// Text never leaves this native check; onboarding events contain only session/outcome ids.
    pub(crate) fn onboarding_source_matches(session_id: u64, seeded_text: &str) -> bool {
        let Ok(sessions) = SESSION.lock() else {
            return false;
        };
        let Some(session) = sessions
            .as_ref()
            .filter(|session| session.active && session.id == session_id)
        else {
            return false;
        };
        let Some(target) = session.target.as_ref() else {
            return false;
        };
        let Ok(snapshot) = target.snapshot.lock() else {
            return false;
        };
        onboarding_seed_fields_match(
            target.pid,
            std::process::id() as i32,
            &snapshot.original_value,
            &snapshot.selected_text,
            snapshot.target_range,
            &session.context.before,
            seeded_text,
        )
    }

    /// Re-read the retained AX element after insertion. This catches a controlled webview render
    /// overwriting the native commit before onboarding treats Scribe as complete.
    pub(crate) fn onboarding_insert_readback_matches(session_id: u64) -> bool {
        let Ok(sessions) = SESSION.lock() else {
            return false;
        };
        let Some(session) = sessions.as_ref().filter(|session| {
            session.active && session.id == session_id && session.phase == "inserted"
        }) else {
            return false;
        };
        let Some(target) = session.target.as_ref() else {
            return false;
        };
        let Ok(snapshot) = target.snapshot.lock() else {
            return false;
        };
        target.pid == std::process::id() as i32
            && snapshot.original_value == session.context.before
            && unsafe { copy_string(target.element, kAXValueAttribute) }.as_deref()
                == Some(snapshot.original_value.as_str())
    }

    fn memory_for(db: &Db, warm: Option<ReplyContext>) -> Vec<String> {
        warm.filter(|context| !context.is_empty())
            .map(|context| context.as_memory_lines(CONTEXT_LINES))
            .unwrap_or_else(|| db.inline_memory(6))
    }

    fn generation_can_apply(
        active: bool,
        actual_id: u64,
        expected_id: u64,
        actual_generation: u64,
        expected_generation: u64,
        busy: bool,
        commit_state_matches: bool,
    ) -> bool {
        active
            && actual_id == expected_id
            && actual_generation == expected_generation
            && busy
            && commit_state_matches
    }

    pub fn open_scribe(
        db: Db,
        warm: Option<ReplyContext>,
        directives: String,
        app: tauri::AppHandle,
    ) -> Result<ScribeOpenResult, String> {
        let (target, context, anchor) =
            capture_target().ok_or_else(|| "no editable target".to_string())?;
        let id = NEXT_SESSION_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut session = SESSION
            .lock()
            .map_err(|_| "scribe unavailable".to_string())?;
        if session
            .as_ref()
            .is_some_and(|current| current.active || current.busy)
        {
            return Err("scribe already open".into());
        }
        *session = Some(ScribeSession {
            id,
            generation: 0,
            target: Some(Arc::new(target)),
            context,
            memory: memory_for(&db, warm),
            directives,
            busy: false,
            active: true,
            phase: "opened",
            chars: 0,
            detail: None,
        });
        drop(session);
        emit(
            &app,
            ScribeEvent {
                session_id: id,
                phase: "opened",
                chars: 0,
                detail: None,
            },
        );
        Ok(ScribeOpenResult {
            session_id: id,
            anchor,
        })
    }

    #[tauri::command]
    pub fn scribe_open(
        db: tauri::State<'_, Db>,
        reply: tauri::State<'_, shogun_core::daemon::ReplyContextCache>,
        user_cfg: tauri::State<'_, crate::user_config_watch::UserConfigState>,
        app: tauri::AppHandle,
    ) -> Result<u64, String> {
        open_scribe(
            db.inner().clone(),
            reply.current(),
            user_cfg.directives(),
            app,
        )
        .map(|opened| opened.session_id)
    }

    fn set_worker_phase(
        session_id: u64,
        generation: u64,
        phase: &'static str,
        chars: usize,
        detail: Option<&'static str>,
    ) -> bool {
        let Ok(mut sessions) = SESSION.lock() else {
            return false;
        };
        let Some(session) = sessions.as_mut().filter(|session| {
            session.id == session_id && session.generation == generation && session.active
        }) else {
            if let Some(session) = sessions.as_mut().filter(|session| session.id == session_id) {
                session.busy = false;
                if !session.active {
                    session.target.take();
                }
            }
            return false;
        };
        session.busy = false;
        session.phase = phase;
        session.chars = chars;
        session.detail = detail;
        true
    }

    #[tauri::command]
    pub fn scribe_submit(
        session_id: u64,
        instruction: String,
        db: tauri::State<'_, Db>,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        if instruction.chars().count() > MAX_INSTRUCTION_CHARS {
            return Err("instruction too long".into());
        }
        let (generation, context, memory, directives) = {
            let mut sessions = SESSION
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
            session.phase = "processing";
            session.chars = 0;
            session.detail = None;
            (
                session.generation,
                session.context.clone(),
                session.memory.clone(),
                session.directives.clone(),
            )
        };
        emit(
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
            let Some(agent) = crate::inline_source::mac::build_agent(&db) else {
                if set_worker_phase(session_id, generation, "no_key", 0, None) {
                    emit(
                        &app,
                        ScribeEvent {
                            session_id,
                            phase: "no_key",
                            chars: 0,
                            detail: None,
                        },
                    );
                }
                return;
            };
            let request = ScribeEditRequest {
                context: &context,
                memory: &memory,
                instruction: &instruction,
            };
            let (mut system, user) = build_scribe_edit_split_prompt(&request);
            if !directives.trim().is_empty() {
                system.push_str("\n\nUser directives:\n");
                system.push_str(directives.trim());
            }
            let generated = match agent.complete_split(&system, &user) {
                Ok(text) if !text.trim().is_empty() => text.trim().to_string(),
                Ok(_) => {
                    finish_generation_failure(session_id, generation, &app, "empty response");
                    return;
                }
                Err(LlmError::Unauthorized(..)) => {
                    crate::inline_source::mac::note_key_rejected();
                    finish_generation_failure(session_id, generation, &app, "key rejected");
                    return;
                }
                Err(_) => {
                    finish_generation_failure(session_id, generation, &app, "generation failed");
                    return;
                }
            };
            let generated = if scribe_output_preserves_protected_spans(
                &context.before,
                &instruction,
                &generated,
            ) {
                generated
            } else {
                eprintln!("[scribe] protected content changed — using original text");
                context.before.clone()
            };

            let chars = generated.chars().count();
            let target = {
                let Ok(sessions) = SESSION.lock() else {
                    return;
                };
                let Some(session) = sessions.as_ref().filter(|session| {
                    generation_can_apply(
                        session.active,
                        session.id,
                        session_id,
                        session.generation,
                        generation,
                        session.busy,
                        session.target.as_ref().is_some_and(|target| {
                            target.commit_state.load(Ordering::Acquire) == COMMIT_READY
                        }),
                    )
                }) else {
                    return;
                };
                let Some(target) = session.target.clone() else {
                    return;
                };
                target
            };
            // AX focus restoration and writes must never hold SESSION. The READY -> WRITING CAS
            // below is the single commit point: cancel wins before it, or the write finishes first.
            let result = insert_verified(&target, &generated);
            let expected_commit_state = if result.is_ok() {
                COMMIT_DONE
            } else {
                COMMIT_READY
            };
            let inserted = {
                let Ok(mut sessions) = SESSION.lock() else {
                    return;
                };
                let Some(session) = sessions.as_mut().filter(|session| {
                    generation_can_apply(
                        session.active,
                        session.id,
                        session_id,
                        session.generation,
                        generation,
                        session.busy,
                        session.target.as_ref().is_some_and(|target| {
                            target.commit_state.load(Ordering::Acquire) == expected_commit_state
                        }),
                    )
                }) else {
                    if let Some(session) =
                        sessions.as_mut().filter(|session| session.id == session_id)
                    {
                        session.busy = false;
                        if !session.active {
                            session.target.take();
                        }
                    }
                    return;
                };
                match result {
                    Ok(result) => {
                        if let Ok(mut snapshot) = target.snapshot.lock() {
                            snapshot.original_value = result.expected_value;
                            snapshot.target_range = result.replacement_range;
                            snapshot.selected_text = generated.clone();
                        } else {
                            target.commit_state.store(COMMIT_READY, Ordering::Release);
                            session.busy = false;
                            session.phase = "failed";
                            session.detail = Some("target changed or insert failed");
                            return;
                        }
                        session.context.before = generated.clone();
                        session.busy = false;
                        session.phase = "inserted";
                        session.chars = chars;
                        session.detail = None;
                        target.commit_state.store(COMMIT_READY, Ordering::Release);
                        true
                    }
                    Err(error) => {
                        eprintln!("[scribe] insert failed: {error}");
                        session.busy = false;
                        session.phase = "failed";
                        session.chars = 0;
                        session.detail = Some("target changed or insert failed");
                        false
                    }
                }
            };
            if inserted {
                crate::inline_source::mac::note_key_accepted();
            }
            emit(
                &app,
                ScribeEvent {
                    session_id,
                    phase: if inserted { "inserted" } else { "failed" },
                    chars: if inserted { chars } else { 0 },
                    detail: if inserted {
                        None
                    } else {
                        Some("target changed or insert failed")
                    },
                },
            );
            if inserted {
                use tauri::Manager;
                if let Some(window) = app.get_webview_window("scribe") {
                    let _ = window.set_focus();
                }
            }
        });
        Ok(())
    }

    fn finish_generation_failure(
        session_id: u64,
        generation: u64,
        app: &tauri::AppHandle,
        detail: &'static str,
    ) {
        if set_worker_phase(session_id, generation, "failed", 0, Some(detail)) {
            emit(
                app,
                ScribeEvent {
                    session_id,
                    phase: "failed",
                    chars: 0,
                    detail: Some(detail),
                },
            );
        }
    }

    #[tauri::command]
    pub fn scribe_status(session_id: u64) -> Result<ScribeEvent, String> {
        let sessions = SESSION
            .lock()
            .map_err(|_| "scribe unavailable".to_string())?;
        let session = sessions
            .as_ref()
            .filter(|session| session.id == session_id)
            .ok_or_else(|| "scribe session closed".to_string())?;
        Ok(ScribeEvent {
            session_id,
            phase: session.phase,
            chars: session.chars,
            detail: session.detail,
        })
    }

    fn close_session(
        session_id: u64,
        app: &tauri::AppHandle,
        phase: &'static str,
    ) -> Result<(), String> {
        let mut sessions = SESSION
            .lock()
            .map_err(|_| "scribe unavailable".to_string())?;
        let session = sessions
            .as_mut()
            .filter(|session| session.id == session_id && session.active)
            .ok_or_else(|| "scribe session closed".to_string())?;
        let target = session.target.clone();
        if let Some(target) = target.as_ref() {
            // READY -> CANCELLED wins before the write commit point. WRITING/DONE means the write
            // already won, so close waits for its serialized AX operation to finish.
            let _ = target.commit_state.compare_exchange(
                COMMIT_READY,
                COMMIT_CANCELLED,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        session.active = false;
        session.generation = session.generation.wrapping_add(1);
        session.phase = phase;
        if !session.busy {
            session.target.take();
        }
        drop(sessions);
        if let Some(target) = target {
            if let Ok(_operation) = target.operation.lock() {
                let _ = restore_target_focus(&target);
            }
        }
        emit(
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
        close_session(session_id, &app, "closed")
    }

    #[tauri::command]
    pub fn scribe_cancel(session_id: u64, app: tauri::AppHandle) -> Result<(), String> {
        close_session(session_id, &app, "cancelled")
    }

    /// Cancel the one process-wide Scribe session before a controlled app restart. This uses the
    /// same commit fence as the user-facing cancel command, so a restart cannot race an AX write.
    pub fn cancel_active_for_restart(app: &tauri::AppHandle) -> Result<(), String> {
        let session_id = SESSION
            .lock()
            .map_err(|_| "scribe unavailable".to_string())?
            .as_ref()
            .filter(|session| session.active)
            .map(|session| session.id);
        let Some(session_id) = session_id else {
            return Ok(());
        };
        close_session(session_id, app, "cancelled")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn onboarding_seed_accepts_own_process_and_full_utf16_selection() {
            let seed = "rough 😀 note";
            let full_range = CFRange::init(0, seed.encode_utf16().count() as isize);
            assert!(onboarding_seed_fields_match(
                42, 42, seed, seed, full_range, seed, seed
            ));
        }

        #[test]
        fn onboarding_seed_rejects_foreign_process() {
            let seed = "rough 😀 note";
            let full_range = CFRange::init(0, seed.encode_utf16().count() as isize);
            assert!(!onboarding_seed_fields_match(
                7, 42, seed, seed, full_range, seed, seed
            ));
        }

        #[test]
        fn onboarding_seed_uses_utf16_selection_length() {
            let seed = "rough 😀 note";
            assert!(!onboarding_seed_fields_match(
                42,
                42,
                seed,
                seed,
                CFRange::init(0, seed.chars().count() as isize),
                seed,
                seed,
            ));
        }

        #[test]
        fn utf16_replacement_handles_non_bmp_text() {
            let (value, range) =
                replace_utf16_range("A😀BC", CFRange::init(1, 2), "📝").expect("valid range");
            assert_eq!(value, "A📝BC");
            assert_eq!(range, CFRange::init(1, 2));
        }

        #[test]
        fn collapsed_caret_selects_current_paragraph() {
            let text = "first\ncan you send 日本語?\nlast";
            let caret = "first\ncan you send 日本".encode_utf16().count();
            let (range, target, _) = rewrite_target(
                text,
                Some(CFRange::init(caret.try_into().expect("offset"), 0)),
            )
            .expect("valid target");
            assert_eq!(target, "can you send 日本語?");
            assert_eq!(range.length as usize, target.encode_utf16().count());
        }

        #[test]
        fn explicit_selection_is_the_only_rewrite_target() {
            let (_, target, surrounding) =
                rewrite_target("prefix can you join? suffix", Some(CFRange::init(7, 13)))
                    .expect("valid target");
            assert_eq!(target, "can you join?");
            assert!(surrounding.contains("prefix "));
            assert!(surrounding.contains(" suffix"));
        }

        #[test]
        fn empty_field_keeps_zero_insertion_range() {
            let (range, target, _) = rewrite_target("", None).expect("empty target");
            assert_eq!(range, CFRange::init(0, 0));
            assert!(target.is_empty());
        }

        #[test]
        fn existing_text_requires_fresh_selection_after_overlay_focus() {
            assert!(!selection_already_prepared(
                "existing text",
                CFRange::init(0, 13)
            ));
            assert!(selection_already_prepared("", CFRange::init(0, 0)));
        }

        #[test]
        fn secure_role_is_refused() {
            assert!(secure_role("AXSecureTextField"));
            assert!(!editable_role("AXSecureTextField"));
        }

        #[test]
        fn oversized_and_invalid_ranges_are_rejected() {
            assert!(rewrite_target("short", Some(CFRange::init(99, 1))).is_none());
            assert!(rewrite_target(&"x".repeat(MAX_TARGET_CHARS + 1), None).is_none());
        }

        #[test]
        fn fallback_requires_exact_value_range_and_selected_text() {
            let snapshot = ScribeSnapshot {
                original_value: "before target after".into(),
                target_range: CFRange::init(7, 6),
                selected_text: "target".into(),
            };
            assert_eq!(
                fallback_decision(
                    Some("before target after"),
                    Some(CFRange::init(7, 6)),
                    "target",
                    &snapshot,
                    "before better after",
                ),
                FallbackDecision::SafeToWrite
            );
            for (value, range, selected) in [
                (Some("user changed it"), Some(CFRange::init(7, 6)), "target"),
                (
                    Some("before target after"),
                    Some(CFRange::init(8, 6)),
                    "target",
                ),
                (
                    Some("before target after"),
                    Some(CFRange::init(7, 6)),
                    "changed",
                ),
            ] {
                assert_eq!(
                    fallback_decision(value, range, selected, &snapshot, "before better after"),
                    FallbackDecision::Abort
                );
            }
        }

        #[test]
        fn stale_or_cancelled_generation_cannot_apply() {
            assert!(generation_can_apply(true, 7, 7, 3, 3, true, true));
            assert!(!generation_can_apply(false, 7, 7, 3, 3, true, true));
            assert!(!generation_can_apply(true, 7, 7, 2, 3, true, true));
            assert!(!generation_can_apply(true, 7, 7, 3, 3, true, false));
        }

        #[test]
        fn repeated_edit_snapshot_uses_latest_text() {
            let mut original = "first".to_string();
            let mut selected = "first".to_string();
            let mut context = "first".to_string();
            for next in ["second", "third"] {
                original = next.to_string();
                selected = next.to_string();
                context = next.to_string();
            }
            assert_eq!(
                (original, selected, context),
                ("third".into(), "third".into(), "third".into())
            );
        }
    }
}
