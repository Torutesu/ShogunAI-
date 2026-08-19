//! Shared native observer for production solo-modifier shortcuts and onboarding proof.

use std::cell::RefCell;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::onboarding::state::OnboardingStep;

pub const DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(300);
pub const DISPATCH_GRACE: Duration = Duration::from_millis(20);
pub const FLAG_ALT: usize = 1 << 19;
pub const RIGHT_OPTION_KEY_CODE: u16 = 61;
pub const POISON_EVENT_MASK: usize = (1 << 1)
    | (1 << 2)
    | (1 << 3)
    | (1 << 4)
    | (1 << 5)
    | (1 << 6)
    | (1 << 7)
    | (1 << 8)
    | (1 << 9)
    | (1 << 18)
    | (1 << 19)
    | (1 << 20)
    | (1 << 22)
    | (1 << 23)
    | (1 << 24)
    | (1 << 25)
    | (1 << 26)
    | (1 << 27)
    | (1 << 29)
    | (1 << 30)
    | (1 << 31)
    | (1usize << 32)
    | (1usize << 34)
    | (1usize << 37);

const MASK_KEY_DOWN: usize = 1 << 10;
const MASK_FLAGS_CHANGED: usize = 1 << 12;
const FLAG_ALL_MODS: usize = (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 23);
const ONBOARDING_SHORTCUT_EVENT: &str = "onboarding-shortcut";
const GLOBAL_MONITOR_RETRY: Duration = Duration::from_secs(3);
const SCRIBE_DEMO_SEED: &str =
    "hi team, can we move our review to friday morning? i can share notes after";

pub fn tap_flag(combo: &str) -> Option<usize> {
    match combo.strip_prefix("Tap+")? {
        "Shift" => Some(1 << 17),
        "Control" => Some(1 << 18),
        "Alt" => Some(FLAG_ALT),
        "Super" => Some(1 << 20),
        "Fn" => Some(1 << 23),
        _ => None,
    }
}

pub fn correct_modifier_key(target: usize, key_code: u16) -> bool {
    target != FLAG_ALT || key_code == RIGHT_OPTION_KEY_CODE
}

pub fn clean_release(armed: bool, poisoned: bool, correct_key: bool, held_ms: u128) -> bool {
    armed && !poisoned && correct_key && held_ms <= 500
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanTapAction {
    QueueDraft {
        generation: u64,
        superseded_draft: Option<u64>,
    },
    StartScribe,
}

#[derive(Default)]
pub struct TapSequence {
    pending_draft: Option<(Instant, u64)>,
    next_generation: u64,
}

impl TapSequence {
    pub fn clean_tap(&mut self, now: Instant) -> CleanTapAction {
        if let Some((first, _)) = self.pending_draft {
            if now.saturating_duration_since(first) <= DOUBLE_TAP_WINDOW {
                self.pending_draft = None;
                return CleanTapAction::StartScribe;
            }
        }
        let superseded_draft = self.pending_draft.take().map(|(_, generation)| generation);
        self.next_generation = self.next_generation.wrapping_add(1);
        let generation = self.next_generation;
        self.pending_draft = Some((now, generation));
        CleanTapAction::QueueDraft {
            generation,
            superseded_draft,
        }
    }

    pub fn take_due_draft(&mut self, now: Instant) -> Option<u64> {
        let due = self
            .pending_draft
            .filter(|(started, _)| now.saturating_duration_since(*started) >= DOUBLE_TAP_WINDOW);
        if due.is_some() {
            self.pending_draft = None;
        }
        due.map(|(_, generation)| generation)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DemoStage {
    RightOption,
    ScribeDemo,
    DictationDemo,
}

impl DemoStage {
    fn from_step(step: OnboardingStep) -> Option<Self> {
        match step {
            OnboardingStep::RightOption => Some(Self::RightOption),
            OnboardingStep::ScribeDemo => Some(Self::ScribeDemo),
            OnboardingStep::DictationDemo => Some(Self::DictationDemo),
            _ => None,
        }
    }

    fn step(self) -> OnboardingStep {
        match self {
            Self::RightOption => OnboardingStep::RightOption,
            Self::ScribeDemo => OnboardingStep::ScribeDemo,
            Self::DictationDemo => OnboardingStep::DictationDemo,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DemoOutcome {
    SingleTap,
    ScribeOpened,
    ScribeInserted,
    DictationInserted,
    DictationCopied,
    NoKey,
    Failed,
    Cancelled,
    Stale,
}

impl DemoOutcome {
    fn advances(self, stage: DemoStage, session_matches: bool, field_verified: bool) -> bool {
        match (self, stage) {
            (Self::SingleTap, DemoStage::RightOption) => true,
            (Self::ScribeInserted, DemoStage::ScribeDemo) => session_matches && field_verified,
            (Self::DictationInserted, DemoStage::DictationDemo) => {
                session_matches && field_verified
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DemoArm {
    pub generation: u64,
    pub nonce: String,
    pub stage: DemoStage,
    pub binding: String,
    pub supports_demo: bool,
    pub supports_scribe: bool,
    pub voice_enabled: bool,
    pub seeded_text: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DemoEvent {
    pub generation: u64,
    pub nonce: String,
    pub stage: DemoStage,
    pub session_id: Option<u64>,
    pub outcome: DemoOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DemoScope {
    generation: u64,
    nonce: String,
    revision: u64,
    stage: DemoStage,
    binding: String,
    ready: bool,
    surface_generation: Option<u64>,
    scribe_session_id: Option<u64>,
    dictation_session_id: Option<u64>,
}

#[derive(Default)]
struct DemoRuntime {
    next_generation: u64,
    scope: Option<DemoScope>,
}

#[derive(Default)]
struct ObserverState {
    armed: bool,
    poisoned: bool,
    target_was_down: bool,
    down_at: Option<Instant>,
    taps: TapSequence,
}

#[derive(Clone, Copy)]
enum EventSource {
    Global,
    Local,
}

static DEMO: LazyLock<Mutex<DemoRuntime>> = LazyLock::new(|| Mutex::new(DemoRuntime::default()));
static OBSERVER: LazyLock<Mutex<ObserverState>> =
    LazyLock::new(|| Mutex::new(ObserverState::default()));
static INSTALL_STARTED: AtomicBool = AtomicBool::new(false);
static GLOBAL_TRUST_WATCHER_RUNNING: AtomicBool = AtomicBool::new(false);

type GlobalMonitorHandler = block2::RcBlock<dyn Fn(NonNull<objc2_app_kit::NSEvent>)>;

#[derive(Default)]
struct GlobalMonitorOwnership {
    poison: Option<(
        objc2::rc::Retained<objc2::runtime::AnyObject>,
        GlobalMonitorHandler,
    )>,
    flags: Option<(
        objc2::rc::Retained<objc2::runtime::AnyObject>,
        GlobalMonitorHandler,
    )>,
}

thread_local! {
    static GLOBAL_MONITORS: RefCell<GlobalMonitorOwnership> = RefCell::new(GlobalMonitorOwnership::default());
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GlobalMonitorState {
    poison_installed: bool,
    flags_installed: bool,
}

impl GlobalMonitorState {
    fn complete(self) -> bool {
        self.poison_installed && self.flags_installed
    }

    fn any(self) -> bool {
        self.poison_installed || self.flags_installed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlobalMonitorAction {
    Keep,
    InstallMissing,
    RemoveAll,
}

fn global_monitor_action(trusted: bool, state: GlobalMonitorState) -> GlobalMonitorAction {
    match (trusted, state.any(), state.complete()) {
        (false, false, _) | (true, _, true) => GlobalMonitorAction::Keep,
        (false, true, _) => GlobalMonitorAction::RemoveAll,
        (true, _, false) => GlobalMonitorAction::InstallMissing,
    }
}

fn random_nonce() -> Result<String, String> {
    use std::fmt::Write as _;

    let mut bytes = [0_u8; 16];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| format!("shortcut demo nonce unavailable: {error}"))?;
    let mut nonce = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut nonce, "{byte:02x}")
            .map_err(|_| "shortcut demo nonce formatting failed".to_owned())?;
    }
    Ok(nonce)
}

fn current_onboarding_state(
    app: &AppHandle,
) -> Result<crate::onboarding::state::OnboardingState, String> {
    app.try_state::<crate::onboarding::mac::Store>()
        .ok_or_else(|| "onboarding state unavailable".to_owned())?
        .snapshot()
}

fn scope_state_is_current(app: &AppHandle, scope: &DemoScope) -> bool {
    current_onboarding_state(app).is_ok_and(|state| {
        state.revision == scope.revision && state.step == scope.stage.step() && !state.completed
    })
}

fn scope_is_current(app: &AppHandle, scope: &DemoScope) -> bool {
    if !scope_state_is_current(app, scope) {
        return false;
    }
    let Some(surface_generation) = scope.surface_generation else {
        return !scope.ready;
    };
    let Some(window) = app.get_webview_window(crate::onboarding::mac::ONBOARDING_LABEL) else {
        return false;
    };
    crate::onboarding_windows::mac::onboarding_window_surface(
        surface_generation,
        window,
        app.clone(),
    )
    .is_ok_and(|surface| {
        surface.surface == crate::onboarding_windows::OnboardingSurfaceKind::Interactive
    })
}

fn same_scope(actual: &DemoScope, expected: &DemoScope) -> bool {
    actual.generation == expected.generation
        && actual.nonce == expected.nonce
        && actual.revision == expected.revision
        && actual.stage == expected.stage
}

fn ready_local_scope() -> Option<DemoScope> {
    DEMO.lock()
        .ok()
        .and_then(|runtime| runtime.scope.as_ref().filter(|scope| scope.ready).cloned())
}

fn source_scope(source: EventSource) -> Option<DemoScope> {
    match source {
        EventSource::Global => None,
        EventSource::Local => ready_local_scope(),
    }
}

fn source_enabled(source: EventSource) -> bool {
    matches!(source, EventSource::Global) || ready_local_scope().is_some()
}

fn emit_event(app: &AppHandle, scope: &DemoScope, outcome: DemoOutcome, session_id: Option<u64>) {
    let event = DemoEvent {
        generation: scope.generation,
        nonce: scope.nonce.clone(),
        stage: scope.stage,
        session_id,
        outcome,
    };
    if let Err(error) = app.emit_to(
        crate::onboarding::mac::ONBOARDING_LABEL,
        ONBOARDING_SHORTCUT_EVENT,
        event,
    ) {
        eprintln!("[onboarding] shortcut outcome delivery failed: {error}");
    }
}

fn stale_scope(app: &AppHandle, scope: &DemoScope) {
    if let Ok(mut runtime) = DEMO.lock() {
        if runtime
            .scope
            .as_ref()
            .is_some_and(|actual| same_scope(actual, scope))
        {
            runtime.scope = None;
        }
    }
    crate::voice_session::mac::disarm_onboarding_dictation_target(scope.generation, &scope.nonce);
    emit_event(
        app,
        scope,
        DemoOutcome::Stale,
        scope.scribe_session_id.or(scope.dictation_session_id),
    );
}

fn emit_if_current(
    app: &AppHandle,
    scope: &DemoScope,
    outcome: DemoOutcome,
    session_id: Option<u64>,
) -> bool {
    if !scope_is_current(app, scope) {
        stale_scope(app, scope);
        return false;
    }
    let matches = DEMO.lock().ok().is_some_and(|runtime| {
        runtime
            .scope
            .as_ref()
            .is_some_and(|actual| same_scope(actual, scope))
    });
    if !matches {
        return false;
    }
    emit_event(app, scope, outcome, session_id);
    true
}

#[tauri::command]
pub fn onboarding_shortcut_arm(
    expected_revision: u64,
    step: OnboardingStep,
    app: AppHandle,
) -> Result<DemoArm, String> {
    let state = current_onboarding_state(&app)?;
    if state.completed || state.revision != expected_revision || state.step != step {
        return Err("shortcut demo does not match current onboarding state".to_owned());
    }
    let stage = DemoStage::from_step(step)
        .ok_or_else(|| "current onboarding step has no shortcut demo".to_owned())?;
    let binding = match stage {
        DemoStage::DictationDemo => {
            crate::shortcuts::binding(&app, "voice").unwrap_or_else(|| "Control+Alt+KeyV".into())
        }
        DemoStage::RightOption | DemoStage::ScribeDemo => {
            crate::shortcuts::binding(&app, "draft").unwrap_or_else(|| "Tap+Alt".into())
        }
    };
    let nonce = random_nonce()?;
    let mut runtime = DEMO
        .lock()
        .map_err(|_| "shortcut demo unavailable".to_owned())?;
    runtime.next_generation = runtime.next_generation.wrapping_add(1);
    let generation = runtime.next_generation;
    runtime.scope = Some(DemoScope {
        generation,
        nonce: nonce.clone(),
        revision: expected_revision,
        stage,
        binding: binding.clone(),
        ready: false,
        surface_generation: None,
        scribe_session_id: None,
        dictation_session_id: None,
    });
    let supports_scribe = binding == "Tap+Alt";
    let supports_demo = match stage {
        DemoStage::RightOption => tap_flag(&binding).is_some(),
        DemoStage::ScribeDemo => supports_scribe,
        DemoStage::DictationDemo => crate::voice_shortcut::binding_supported(&binding),
    };
    Ok(DemoArm {
        generation,
        nonce,
        stage,
        supports_demo,
        supports_scribe,
        voice_enabled: crate::voice_session::mac::get_voice_settings().enabled,
        binding,
        seeded_text: (stage == DemoStage::ScribeDemo).then_some(SCRIBE_DEMO_SEED),
    })
}

#[tauri::command]
pub fn onboarding_shortcut_ready(
    generation: u64,
    nonce: String,
    surface_generation: u64,
    window: tauri::WebviewWindow,
    app: AppHandle,
) -> Result<(), String> {
    let surface = crate::onboarding_windows::mac::onboarding_window_surface(
        surface_generation,
        window,
        app.clone(),
    )?;
    if surface.surface != crate::onboarding_windows::OnboardingSurfaceKind::Interactive {
        return Err("shortcut demo requires the interactive onboarding surface".to_owned());
    }
    let prepared = DEMO
        .lock()
        .map_err(|_| "shortcut demo unavailable".to_owned())?
        .scope
        .as_ref()
        .filter(|scope| scope.generation == generation && scope.nonce == nonce)
        .cloned()
        .ok_or_else(|| "stale shortcut demo scope".to_owned())?;
    if !scope_state_is_current(&app, &prepared) {
        return Err("shortcut demo onboarding state changed".to_owned());
    }
    match prepared.stage {
        DemoStage::RightOption if tap_flag(&prepared.binding).is_none() => {
            return Err("current draft binding is not a solo modifier tap".to_owned());
        }
        DemoStage::ScribeDemo if prepared.binding != "Tap+Alt" => {
            return Err("Scribe practice requires the Right Option binding".to_owned());
        }
        DemoStage::DictationDemo => {
            if !crate::voice_shortcut::binding_supported(&prepared.binding) {
                return Err("current dictation binding is not supported".to_owned());
            }
            if !crate::voice_session::mac::get_voice_settings().enabled {
                return Err("dictation must be enabled before practice".to_owned());
            }
            crate::voice_session::mac::prepare_onboarding_dictation_target(generation, &nonce)?;
        }
        DemoStage::RightOption | DemoStage::ScribeDemo => {}
    }
    let mut runtime = DEMO
        .lock()
        .map_err(|_| "shortcut demo unavailable".to_owned())?;
    let scope = runtime
        .scope
        .as_mut()
        .filter(|scope| same_scope(scope, &prepared))
        .ok_or_else(|| "stale shortcut demo scope".to_owned())?;
    scope.ready = true;
    scope.surface_generation = Some(surface_generation);
    drop(runtime);
    if let Ok(mut observer) = OBSERVER.lock() {
        *observer = ObserverState::default();
    }
    Ok(())
}

#[tauri::command]
pub fn onboarding_shortcut_disarm(generation: u64, nonce: String) -> Result<(), String> {
    let mut runtime = DEMO
        .lock()
        .map_err(|_| "shortcut demo unavailable".to_owned())?;
    if runtime
        .scope
        .as_ref()
        .is_some_and(|scope| scope.generation == generation && scope.nonce == nonce)
    {
        runtime.scope = None;
    }
    crate::voice_session::mac::disarm_onboarding_dictation_target(generation, &nonce);
    Ok(())
}

fn poison(source: EventSource) {
    if !source_enabled(source) {
        return;
    }
    if let Ok(mut observer) = OBSERVER.lock() {
        observer.poisoned = true;
        observer.armed = false;
    }
}

fn queue_draft(app: AppHandle, generation: u64, scope: Option<DemoScope>) {
    let _ = std::thread::Builder::new()
        .name("right-option-single".into())
        .spawn(move || {
            std::thread::sleep(DOUBLE_TAP_WINDOW + DISPATCH_GRACE);
            let due = OBSERVER
                .lock()
                .ok()
                .and_then(|mut observer| observer.taps.take_due_draft(Instant::now()))
                == Some(generation);
            if !due {
                return;
            }
            crate::run_inline_draft(&app);
            if let Some(scope) = scope.filter(|scope| scope.stage == DemoStage::RightOption) {
                let _ = emit_if_current(&app, &scope, DemoOutcome::SingleTap, None);
            }
        });
}

fn set_scribe_session(scope: &DemoScope, session_id: u64) -> bool {
    DEMO.lock().ok().is_some_and(|mut runtime| {
        let Some(actual) = runtime
            .scope
            .as_mut()
            .filter(|actual| same_scope(actual, scope))
        else {
            return false;
        };
        actual.ready = false;
        actual.scribe_session_id = Some(session_id);
        true
    })
}

fn start_scribe(app: &AppHandle, demo_scope: Option<DemoScope>) {
    let Some(db) = app.try_state::<shogun_core::daemon::Db>() else {
        if let Some(scope) = demo_scope.as_ref() {
            let _ = emit_if_current(app, scope, DemoOutcome::Failed, None);
        }
        eprintln!("[shell] Scribe open skipped: database unavailable");
        return;
    };
    let warm = app
        .try_state::<shogun_core::daemon::ReplyContextCache>()
        .and_then(|cache| cache.current());
    let directives = app
        .try_state::<crate::user_config_watch::UserConfigState>()
        .map(|state| state.directives())
        .unwrap_or_default();
    let opened =
        match crate::scribe::mac::open_scribe(db.inner().clone(), warm, directives, app.clone()) {
            Ok(opened) => opened,
            Err(error) => {
                if let Some(scope) = demo_scope.as_ref() {
                    let _ = emit_if_current(app, scope, DemoOutcome::Failed, None);
                }
                eprintln!("[shell] Scribe open failed: {error}");
                return;
            }
        };
    let session_id = opened.session_id;
    if let Some(scope) = demo_scope.as_ref() {
        if scope.stage != DemoStage::ScribeDemo
            || !crate::scribe::mac::onboarding_source_matches(session_id, SCRIBE_DEMO_SEED)
            || !set_scribe_session(scope, session_id)
        {
            let _ = crate::scribe::mac::scribe_cancel(session_id, app.clone());
            let _ = emit_if_current(app, scope, DemoOutcome::Failed, Some(session_id));
            return;
        }
        let _ = emit_if_current(app, scope, DemoOutcome::ScribeOpened, Some(session_id));
    }
    if let Err(error) = crate::build_scribe_window(app, opened) {
        let _ = crate::scribe::mac::scribe_cancel(session_id, app.clone());
        eprintln!("[shell] Scribe overlay failed: {error}");
    }
}

fn clean_tap(app: &AppHandle, source: EventSource, target: usize) {
    let scope = source_scope(source);
    if target != FLAG_ALT {
        crate::run_inline_draft(app);
        if let Some(scope) = scope.filter(|scope| scope.stage == DemoStage::RightOption) {
            let _ = emit_if_current(app, &scope, DemoOutcome::SingleTap, None);
        }
        return;
    }
    let action = OBSERVER
        .lock()
        .ok()
        .map(|mut observer| observer.taps.clean_tap(Instant::now()));
    match action {
        Some(CleanTapAction::StartScribe) => {
            eprintln!("[shell] right ⌥ double-tap — Scribe");
            start_scribe(app, scope);
        }
        Some(CleanTapAction::QueueDraft {
            generation,
            superseded_draft,
        }) => {
            if superseded_draft.is_some() {
                crate::run_inline_draft(app);
            }
            queue_draft(app.clone(), generation, scope);
        }
        None => {}
    }
}

fn flags_changed(app: &AppHandle, source: EventSource, event: *mut objc2::runtime::AnyObject) {
    use objc2::msg_send;

    if event.is_null() || !source_enabled(source) {
        return;
    }
    let combo = crate::shortcuts::binding(app, "draft").unwrap_or_else(|| "Tap+Alt".into());
    let Some(target) = tap_flag(&combo) else {
        return;
    };
    // SAFETY: event comes from AppKit's monitor callback and lives for this call.
    let flags: usize = unsafe { msg_send![event, modifierFlags] };
    // SAFETY: same callback lifetime as above.
    let key_code: u16 = unsafe { msg_send![event, keyCode] };
    let target_down = flags & target != 0;
    let others_down = flags & (FLAG_ALL_MODS & !target) != 0;
    let mut fire = false;
    {
        let Ok(mut observer) = OBSERVER.lock() else {
            return;
        };
        let was_down = observer.target_was_down;
        observer.target_was_down = target_down;
        if others_down {
            observer.poisoned = true;
            observer.armed = false;
            return;
        }
        if target_down && !was_down {
            if !correct_modifier_key(target, key_code) {
                observer.poisoned = true;
                observer.armed = false;
                observer.down_at = None;
                return;
            }
            observer.poisoned = false;
            observer.armed = true;
            observer.down_at = Some(Instant::now());
        } else if !target_down && was_down {
            let armed = std::mem::take(&mut observer.armed);
            let poisoned = std::mem::take(&mut observer.poisoned);
            let held = observer
                .down_at
                .take()
                .map(|started| started.elapsed().as_millis());
            fire = held.is_some_and(|milliseconds| {
                clean_release(
                    armed,
                    poisoned,
                    correct_modifier_key(target, key_code),
                    milliseconds,
                )
            });
        }
    }
    if fire {
        clean_tap(app, source, target);
    }
}

/// Observe Scribe lifecycle only for session captured from matching native demo double-tap.
pub fn observe_scribe_event(app: &AppHandle, event: &crate::scribe::mac::ScribeEvent) {
    let scope = DEMO.lock().ok().and_then(|runtime| {
        runtime
            .scope
            .as_ref()
            .filter(|scope| {
                scope.stage == DemoStage::ScribeDemo
                    && scope.scribe_session_id == Some(event.session_id)
            })
            .cloned()
    });
    let Some(scope) = scope else {
        return;
    };
    let outcome = match event.phase {
        "inserted" => {
            if crate::scribe::mac::onboarding_insert_readback_matches(event.session_id) {
                DemoOutcome::ScribeInserted
            } else {
                DemoOutcome::Failed
            }
        }
        "no_key" => DemoOutcome::NoKey,
        "failed" => DemoOutcome::Failed,
        "cancelled" | "closed" => DemoOutcome::Cancelled,
        _ => return,
    };
    let field_verified = outcome == DemoOutcome::ScribeInserted;
    let advances = outcome.advances(DemoStage::ScribeDemo, true, field_verified);
    if advances || matches!(outcome, DemoOutcome::Cancelled) {
        if let Ok(mut runtime) = DEMO.lock() {
            if let Some(actual) = runtime
                .scope
                .as_mut()
                .filter(|actual| same_scope(actual, &scope))
            {
                actual.ready = false;
            }
        }
    }
    let _ = emit_if_current(app, &scope, outcome, Some(event.session_id));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DictationDemoOutcome {
    Inserted,
    Copied,
    Failed,
    Cancelled,
}

/// Bind production voice session to exact armed onboarding generation before mic capture starts.
pub fn bind_dictation_session(
    app: &AppHandle,
    generation: u64,
    nonce: &str,
    session_id: u64,
) -> bool {
    let scope = DEMO.lock().ok().and_then(|mut runtime| {
        let scope = runtime.scope.as_mut().filter(|scope| {
            scope.generation == generation
                && scope.nonce == nonce
                && scope.stage == DemoStage::DictationDemo
                && scope.ready
                && scope.dictation_session_id.is_none()
        })?;
        scope.ready = false;
        scope.dictation_session_id = Some(session_id);
        Some(scope.clone())
    });
    scope.is_some_and(|scope| scope_is_current(app, &scope))
}

pub fn reject_dictation_target(app: &AppHandle, generation: u64, nonce: &str) {
    let scope = DEMO.lock().ok().and_then(|mut runtime| {
        let scope = runtime.scope.as_mut().filter(|scope| {
            scope.generation == generation
                && scope.nonce == nonce
                && scope.stage == DemoStage::DictationDemo
        })?;
        scope.ready = false;
        Some(scope.clone())
    });
    if let Some(scope) = scope {
        let _ = emit_if_current(app, &scope, DemoOutcome::Failed, None);
    }
}

/// Content-free terminal delivery proof. Inserted is emitted only after AX value readback passed.
pub fn observe_dictation_outcome(app: &AppHandle, session_id: u64, delivery: DictationDemoOutcome) {
    let scope = DEMO.lock().ok().and_then(|runtime| {
        runtime
            .scope
            .as_ref()
            .filter(|scope| {
                scope.stage == DemoStage::DictationDemo
                    && scope.dictation_session_id == Some(session_id)
            })
            .cloned()
    });
    let Some(scope) = scope else {
        return;
    };
    let outcome = match delivery {
        DictationDemoOutcome::Inserted => DemoOutcome::DictationInserted,
        DictationDemoOutcome::Copied => DemoOutcome::DictationCopied,
        DictationDemoOutcome::Failed => DemoOutcome::Failed,
        DictationDemoOutcome::Cancelled => DemoOutcome::Cancelled,
    };
    let field_verified = delivery == DictationDemoOutcome::Inserted;
    let _advances = outcome.advances(DemoStage::DictationDemo, true, field_verified);
    let _ = emit_if_current(app, &scope, outcome, Some(session_id));
}

fn global_monitor_state() -> GlobalMonitorState {
    GLOBAL_MONITORS.with(|monitors| {
        let monitors = monitors.borrow();
        GlobalMonitorState {
            poison_installed: monitors.poison.is_some(),
            flags_installed: monitors.flags.is_some(),
        }
    })
}

fn remove_global_monitors_main() {
    use objc2_app_kit::NSEvent;

    if objc2::MainThreadMarker::new().is_none() {
        return;
    }
    let (poison, flags) = GLOBAL_MONITORS.with(|monitors| {
        let mut monitors = monitors.borrow_mut();
        (monitors.poison.take(), monitors.flags.take())
    });
    unsafe {
        if let Some((token, _handler)) = poison {
            NSEvent::removeMonitor(&token);
        }
        if let Some((token, _handler)) = flags {
            NSEvent::removeMonitor(&token);
        }
    }
    if let Ok(mut observer) = OBSERVER.lock() {
        *observer = ObserverState::default();
    }
}

fn install_global_monitors_main(app: &AppHandle) {
    use objc2_app_kit::{NSEvent, NSEventMask};

    if objc2::MainThreadMarker::new().is_none() {
        return;
    }
    let state = global_monitor_state();
    if !state.poison_installed {
        let handler: GlobalMonitorHandler =
            block2::RcBlock::new(move |_event: NonNull<objc2_app_kit::NSEvent>| {
                crate::daily_summaries::note_global_input(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_millis() as i64)
                        .unwrap_or(0),
                );
                poison(EventSource::Global);
            });
        let mask = NSEventMask::from_bits_retain((MASK_KEY_DOWN | POISON_EVENT_MASK) as u64);
        if let Some(token) = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &handler)
        {
            GLOBAL_MONITORS.with(|monitors| {
                monitors.borrow_mut().poison = Some((token, handler));
            });
        }
    }
    if !state.flags_installed {
        let app = app.clone();
        let handler: GlobalMonitorHandler =
            block2::RcBlock::new(move |event: NonNull<objc2_app_kit::NSEvent>| {
                flags_changed(&app, EventSource::Global, event.as_ptr().cast());
            });
        if let Some(token) = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::FlagsChanged,
            &handler,
        ) {
            GLOBAL_MONITORS.with(|monitors| {
                monitors.borrow_mut().flags = Some((token, handler));
            });
        }
    }
}

fn reconcile_global_monitors_main(app: &AppHandle) {
    match global_monitor_action(crate::axcache::ax_trusted_silent(), global_monitor_state()) {
        GlobalMonitorAction::Keep => {}
        GlobalMonitorAction::InstallMissing => install_global_monitors_main(app),
        GlobalMonitorAction::RemoveAll => remove_global_monitors_main(),
    }
}

fn start_global_trust_watcher(app: AppHandle) {
    if GLOBAL_TRUST_WATCHER_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("right-option-monitor-trust".into())
        .spawn(move || loop {
            std::thread::sleep(GLOBAL_MONITOR_RETRY);
            let handle = app.clone();
            if app
                .run_on_main_thread(move || reconcile_global_monitors_main(&handle))
                .is_err()
            {
                eprintln!("[shell] right Option monitor reconcile could not reach AppKit");
            }
        });
    if spawned.is_err() {
        GLOBAL_TRUST_WATCHER_RUNNING.store(false, Ordering::SeqCst);
    }
}

fn install_local_monitors_main(app: &AppHandle) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    // SAFETY: setup runs on AppKit main thread. Local callbacks return original event unchanged.
    unsafe {
        let local_poison = block2::RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject {
            poison(EventSource::Local);
            event
        });
        let local_poison_token: *mut AnyObject = msg_send![
            class!(NSEvent),
            addLocalMonitorForEventsMatchingMask: MASK_KEY_DOWN | POISON_EVENT_MASK,
            handler: &*local_poison
        ];
        std::mem::forget(local_poison);

        let local_app = app.clone();
        let local_flags = block2::RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject {
            flags_changed(&local_app, EventSource::Local, event);
            event
        });
        let local_flag_token: *mut AnyObject = msg_send![
            class!(NSEvent),
            addLocalMonitorForEventsMatchingMask: MASK_FLAGS_CHANGED,
            handler: &*local_flags
        ];
        std::mem::forget(local_flags);

        if local_poison_token.is_null() || local_flag_token.is_null() {
            eprintln!("[shell] solo-modifier monitor incomplete (Accessibility permission?)");
        }
    }
}

/// Install recoverable global production monitors plus pass-through local onboarding monitors.
pub fn install(app: &tauri::App) {
    if INSTALL_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let handle = app.handle().clone();
    install_local_monitors_main(&handle);
    reconcile_global_monitors_main(&handle);
    start_global_trust_watcher(handle);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_tap_at_exact_window_starts_scribe_and_cancels_single() {
        let start = Instant::now();
        let mut state = TapSequence::default();
        assert!(matches!(
            state.clean_tap(start),
            CleanTapAction::QueueDraft { generation: 1, .. }
        ));
        assert_eq!(
            state.clean_tap(start + DOUBLE_TAP_WINDOW),
            CleanTapAction::StartScribe
        );
        assert_eq!(state.take_due_draft(start + DOUBLE_TAP_WINDOW), None);
    }

    #[test]
    fn first_tap_is_due_only_after_window() {
        let start = Instant::now();
        let mut state = TapSequence::default();
        state.clean_tap(start);
        assert_eq!(
            state.take_due_draft(start + Duration::from_millis(299)),
            None
        );
        assert_eq!(state.take_due_draft(start + DOUBLE_TAP_WINDOW), Some(1));
    }

    #[test]
    fn late_second_tap_preserves_first_draft() {
        let start = Instant::now();
        let mut state = TapSequence::default();
        state.clean_tap(start);
        assert_eq!(
            state.clean_tap(start + DOUBLE_TAP_WINDOW + Duration::from_nanos(1)),
            CleanTapAction::QueueDraft {
                generation: 2,
                superseded_draft: Some(1),
            }
        );
    }

    #[test]
    fn right_option_single_advances_only_matching_stage() {
        assert!(DemoOutcome::SingleTap.advances(DemoStage::RightOption, false, false));
        assert!(!DemoOutcome::SingleTap.advances(DemoStage::ScribeDemo, false, false));
    }

    #[test]
    fn scribe_insert_requires_matching_session_and_verified_field() {
        assert!(DemoOutcome::ScribeInserted.advances(DemoStage::ScribeDemo, true, true));
        assert!(!DemoOutcome::ScribeInserted.advances(DemoStage::ScribeDemo, false, true));
        assert!(!DemoOutcome::ScribeInserted.advances(DemoStage::ScribeDemo, true, false));
    }

    #[test]
    fn every_non_success_scribe_outcome_stays_retry() {
        for outcome in [
            DemoOutcome::ScribeOpened,
            DemoOutcome::NoKey,
            DemoOutcome::Failed,
            DemoOutcome::Cancelled,
            DemoOutcome::Stale,
        ] {
            assert!(!outcome.advances(DemoStage::ScribeDemo, true, true));
        }
    }

    #[test]
    fn dictation_insert_requires_matching_session_and_verified_field() {
        assert!(DemoOutcome::DictationInserted.advances(DemoStage::DictationDemo, true, true));
        assert!(!DemoOutcome::DictationInserted.advances(DemoStage::DictationDemo, false, true));
        assert!(!DemoOutcome::DictationInserted.advances(DemoStage::DictationDemo, true, false));
    }

    #[test]
    fn copied_failed_cancelled_and_stale_dictation_stay_retry() {
        for outcome in [
            DemoOutcome::DictationCopied,
            DemoOutcome::Failed,
            DemoOutcome::Cancelled,
            DemoOutcome::Stale,
        ] {
            assert!(!outcome.advances(DemoStage::DictationDemo, true, true));
        }
    }

    #[test]
    fn dictation_event_contract_contains_no_transcript() {
        let event = DemoEvent {
            generation: 8,
            nonce: "voice-nonce".to_owned(),
            stage: DemoStage::DictationDemo,
            session_id: Some(11),
            outcome: DemoOutcome::DictationInserted,
        };
        assert_eq!(
            serde_json::to_value(event).expect("event serializes"),
            serde_json::json!({
                "generation": 8,
                "nonce": "voice-nonce",
                "stage": "dictation_demo",
                "session_id": 11,
                "outcome": "dictation_inserted",
            })
        );
    }

    #[test]
    fn demo_event_contract_contains_only_scope_session_and_outcome() {
        let event = DemoEvent {
            generation: 7,
            nonce: "nonce".to_owned(),
            stage: DemoStage::ScribeDemo,
            session_id: Some(9),
            outcome: DemoOutcome::ScribeInserted,
        };
        assert_eq!(
            serde_json::to_value(event).expect("event serializes"),
            serde_json::json!({
                "generation": 7,
                "nonce": "nonce",
                "stage": "scribe_demo",
                "session_id": 9,
                "outcome": "scribe_inserted",
            })
        );
    }

    #[test]
    fn only_right_option_is_allowed_for_alt_binding() {
        assert!(correct_modifier_key(FLAG_ALT, RIGHT_OPTION_KEY_CODE));
        assert!(!correct_modifier_key(FLAG_ALT, 58));
        assert!(correct_modifier_key(1 << 17, 56));
    }

    #[test]
    fn other_tap_modifiers_work_and_normal_chords_are_inert() {
        assert_eq!(tap_flag("Tap+Shift"), Some(1 << 17));
        assert_eq!(tap_flag("Tap+Control"), Some(1 << 18));
        assert_eq!(tap_flag("Control+Alt+KeyD"), None);
    }

    #[test]
    fn every_pointer_scroll_and_gesture_family_poisons_hold() {
        for bit in [
            1usize, 2, 3, 4, 5, 6, 7, 8, 9, 18, 19, 20, 22, 23, 24, 25, 26, 27, 29, 30, 31, 32, 34,
            37,
        ] {
            assert_ne!(
                POISON_EVENT_MASK & (1usize << bit),
                0,
                "missing event bit {bit}"
            );
        }
    }

    #[test]
    fn poisoned_or_long_hold_never_fires() {
        assert!(!clean_release(true, true, true, 100));
        assert!(!clean_release(true, false, true, 501));
        assert!(!clean_release(true, false, false, 100));
        assert!(clean_release(true, false, true, 500));
    }

    #[test]
    fn global_monitor_reconciles_permission_revoke_and_regrant() {
        let installed = GlobalMonitorState {
            poison_installed: true,
            flags_installed: true,
        };
        assert_eq!(
            global_monitor_action(false, installed),
            GlobalMonitorAction::RemoveAll
        );
        assert_eq!(
            global_monitor_action(true, GlobalMonitorState::default()),
            GlobalMonitorAction::InstallMissing
        );
        assert_eq!(
            global_monitor_action(true, installed),
            GlobalMonitorAction::Keep
        );
    }
}
