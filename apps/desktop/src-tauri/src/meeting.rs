//! Meeting notes: the adapter between the pure lifecycle (`shogun_core::meeting`) and macOS.
//!
//! The machine and the detection rules live in the core and are tested there. This file does the
//! three things that cannot be pure: it persists the settings, it drives a one-second tick so the
//! pill can show elapsed time and the offer can count down, and it projects the state into the
//! webview.
//!
//! Two invariants are kept structurally rather than by discipline:
//!
//! 1. **Off means the detector never runs.** [`on_focus`] returns before touching the machine when
//!    the feature is disabled, so nothing downstream can observe a meeting while it is off
//!    (FR-MT-02a).
//! 2. **Audio has no code path yet.** `Effect::StartAudio` is deliberately not implemented — MT1/
//!    MT2 ship without listening (FR-MT-13 arrives in MT3), and an unimplemented effect is a
//!    louder statement than a comment saying "not yet".

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde::Serialize;
    use shogun_core::meeting::detect::{self, Decision, Signals};
    use shogun_core::meeting::settings::{OfferContext, Settings};
    use shogun_core::meeting::statemachine::{Effect, Input, Machine, Params, State};
    use tauri::{Emitter, Manager};

    /// Settings + machine, behind one lock. They are always read together (an offer needs both
    /// "is this allowed?" and "what state am I in?"), so splitting the locks would only create
    /// the chance to read a half-updated pair.
    struct Lane {
        settings: Settings,
        machine: Machine,
        /// The interval currently open, and what to title it.
        session_id: Option<i64>,
        title: Option<String>,
        app_bundle_id: Option<String>,
        /// Carried from the detector so the stored interval records what was actually observed,
        /// rather than a constant that would claim more (or less) than the evidence (FR-MT-04).
        confidence: f64,
        provenance: String,
        /// Epoch ms of the transition into the current state — the pill's clock.
        since_ms: i64,
    }

    impl Lane {
        fn new() -> Self {
            Self {
                settings: Settings::default(),
                machine: Machine::new(Params::default()),
                session_id: None,
                title: None,
                app_bundle_id: None,
                confidence: 0.0,
                provenance: "{}".to_string(),
                since_ms: 0,
            }
        }
    }

    static LANE: Mutex<Option<Lane>> = Mutex::new(None);

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    /// What the pill needs to draw itself (FR-MT-09).
    #[derive(Debug, Clone, Serialize, PartialEq)]
    pub struct MeetingView {
        /// "idle" | "offered" | "recording" | "wrapping".
        pub state: &'static str,
        /// Whether the feature is on at all — the pill is hidden entirely when it is not, so the
        /// user never sees something meeting-shaped while meeting notes are off (FR-MT-02a).
        pub enabled: bool,
        pub title: Option<String>,
        /// Milliseconds recorded so far. The pill shows this as mm:ss and must keep moving: a
        /// state toggle alone does not answer "is it still going?" (FR-MT-09).
        pub elapsed_ms: i64,
        /// Milliseconds left in the Offered grace, so the countdown is visible (FR-MT-08).
        pub countdown_ms: i64,
    }

    fn view(lane: &Lane, now: i64) -> MeetingView {
        let since = now.saturating_sub(lane.since_ms).max(0);
        let state = lane.machine.state();
        MeetingView {
            state: state.tag(),
            enabled: lane.settings.enabled,
            title: lane.title.clone(),
            elapsed_ms: if state == State::Recording { since } else { 0 },
            countdown_ms: if state == State::Offered {
                (Params::default().offer_grace_ms as i64 - since).max(0)
            } else {
                0
            },
        }
    }

    fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join("meeting.json"))
    }

    /// Load persisted settings. Called once at setup.
    ///
    /// Any failure — missing file, unreadable file, half-written JSON — leaves the default in
    /// place, and the default is off (FR-MT-01). Failing to read settings can only ever result in
    /// *not* listening.
    pub fn init(app: &tauri::AppHandle) {
        let mut lane = Lane::new();
        if let Some(p) = settings_path(app) {
            if let Ok(text) = std::fs::read_to_string(p) {
                if let Ok(saved) = serde_json::from_str::<Settings>(&text) {
                    lane.settings = saved;
                }
            }
        }
        eprintln!(
            "[meeting] notes {}",
            if lane.settings.enabled { "enabled" } else { "off (default)" }
        );
        if let Ok(mut g) = LANE.lock() {
            *g = Some(lane);
        }
    }

    fn save(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
        let Some(p) = settings_path(app) else { return Ok(()) };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        std::fs::write(&p, json).map_err(|e| format!("save failed: {e}"))
    }

    /// Apply the machine's effects. Returns the effects it could not honour, so a caller adding
    /// audio later cannot forget that this list exists.
    fn apply(app: &tauri::AppHandle, lane: &mut Lane, effects: &[Effect], now: i64) {
        for fx in effects {
            match fx {
                Effect::Transition(_) => lane.since_ms = now,
                Effect::OpenSession => {
                    lane.session_id = open_session(app, lane);
                }
                Effect::CloseSession(why) => {
                    if let Some(id) = lane.session_id.take() {
                        close_session(app, id);
                        eprintln!("[meeting] session {id} closed ({why:?})");
                    }
                }
                // MT3. Not silently ignored: until the audio lane exists, the honest behaviour is
                // to record intervals and notes without listening (FR-MT-13, OPEN-07/08).
                Effect::StartAudio | Effect::StopAudio => {}
                // The tick loop drives the countdown and the silence watchdog, so the machine's
                // timer requests need no separate scheduler here.
                Effect::StartTimer { .. } | Effect::CancelTimer(_) => {}
                Effect::BuildRecap => {}
            }
        }
        emit(app, lane, now);
    }

    fn emit(app: &tauri::AppHandle, lane: &Lane, now: i64) {
        let _ = app.emit("meeting", view(lane, now));
    }

    /// The database, when it is up. Meeting notes must not be the reason the app fails to start,
    /// so every path here degrades to "no interval recorded" rather than erroring at the user.
    fn db(app: &tauri::AppHandle) -> Option<tauri::State<'_, shogun_core::daemon::Db>> {
        app.try_state::<shogun_core::daemon::Db>()
    }

    fn open_session(app: &tauri::AppHandle, lane: &Lane) -> Option<i64> {
        let db = db(app)?;
        db.open_meeting(
            lane.title.as_deref(),
            lane.app_bundle_id.as_deref(),
            lane.confidence,
            &lane.provenance,
        )
        .inspect(|id| eprintln!("[meeting] session {id} opened"))
    }

    fn close_session(app: &tauri::AppHandle, id: i64) {
        if let Some(db) = db(app) {
            db.close_meeting(id);
        }
    }

    fn step(app: &tauri::AppHandle, input: Input) {
        let now = now_ms();
        let Ok(mut g) = LANE.lock() else { return };
        let Some(lane) = g.as_mut() else { return };
        let effects = lane.machine.step(input);
        apply(app, lane, &effects, now);
    }

    /// Called on every focus change with the frontmost app (from the capture poller).
    ///
    /// Returns immediately when the feature is off — the detector does not run, so nothing
    /// observes a meeting while meeting notes are disabled (FR-MT-02a).
    pub fn on_focus(app: &tauri::AppHandle, bundle_id: &str, window_title: Option<&str>) {
        let now = now_ms();
        let Ok(mut g) = LANE.lock() else { return };
        let Some(lane) = g.as_mut() else { return };

        if !lane.settings.enabled {
            return;
        }
        if lane.machine.state() != State::Idle {
            return;
        }
        if !lane.settings.may_offer(&OfferContext {
            app_bundle_id: Some(bundle_id),
            occurrence_external_id: None,
        }) {
            return;
        }

        // MT1 sees one signal: a known meeting app in front. The microphone-in-use and
        // AX-controls signals (FR-MT-04 (2)/(3)) need native probes that do not exist yet, and
        // guessing them would inflate the confidence stored against the interval.
        let signals = Signals {
            meeting_app_frontmost: detect::is_meeting_app(bundle_id),
            ..Default::default()
        };
        if let Decision::Offer { confidence, provenance } = detect::decide(&signals) {
            lane.title = window_title.map(str::to_string);
            lane.app_bundle_id = Some(bundle_id.to_string());
            lane.confidence = confidence;
            lane.provenance = provenance;
            let effects = lane.machine.step(Input::MeetingDetected);
            apply(app, lane, &effects, now);
        }
    }

    /// One-second tick: advances the offer countdown and the pill's clock.
    pub fn tick(app: &tauri::AppHandle) {
        let now = now_ms();
        let expired = {
            let Ok(mut g) = LANE.lock() else { return };
            let Some(lane) = g.as_mut() else { return };
            match lane.machine.state() {
                State::Idle => return,
                State::Offered => {
                    now.saturating_sub(lane.since_ms) >= Params::default().offer_grace_ms as i64
                }
                _ => false,
            }
        };
        if expired {
            step(app, Input::GraceExpired);
            return;
        }
        // Recording: re-emit so the elapsed time on the pill keeps moving.
        if let Ok(g) = LANE.lock() {
            if let Some(lane) = g.as_ref() {
                emit(app, lane, now);
            }
        }
    }

    /// One-second driver: reads the frontmost app, offers when it is a meeting, and keeps the
    /// pill's clock moving.
    ///
    /// A second is the right granularity for both jobs — the countdown and the elapsed time are
    /// both displayed in whole seconds — and it costs a `frontmostApplication()` call per tick,
    /// which is the same signal the capture poller already reads. When the feature is off,
    /// [`on_focus`] returns before doing any of that work (FR-MT-02a).
    pub fn spawn_meeting_driver(app: tauri::AppHandle) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if let Some(front) = crate::display::frontmost_app() {
                on_focus(&app, &front.bundle_id, Some(&front.name));
            }
            tick(&app);
        })
    }

    /// The pill's current contents (FR-MT-09). Also the webview's first read at boot.
    #[tauri::command]
    pub fn meeting_status() -> MeetingView {
        let now = now_ms();
        LANE.lock()
            .ok()
            .and_then(|g| g.as_ref().map(|l| view(l, now)))
            .unwrap_or(MeetingView {
                state: "idle",
                enabled: false,
                title: None,
                elapsed_ms: 0,
                countdown_ms: 0,
            })
    }

    /// "Start" during the grace (FR-MT-08).
    #[tauri::command]
    pub fn meeting_start(app: tauri::AppHandle) {
        step(&app, Input::Start);
    }

    /// "Not now" — this meeting only; settings untouched (FR-MT-02c).
    #[tauri::command]
    pub fn meeting_not_now(app: tauri::AppHandle) {
        step(&app, Input::NotNow);
    }

    /// "Stop" — immediate, no confirmation dialog (FR-MT-09).
    #[tauri::command]
    pub fn meeting_stop(app: tauri::AppHandle) {
        step(&app, Input::Stop);
        // MT2 has no Recap window yet; close the lifecycle so the next meeting is offered.
        step(&app, Input::Wrapped);
    }

    /// Save the note typed during the meeting (FR-MT-10). Silently does nothing when no interval
    /// is open — there is nothing to attach a note to, and losing the text is better than
    /// inventing a session for it.
    #[tauri::command]
    pub fn meeting_save_note(body: String, app: tauri::AppHandle) -> Result<(), String> {
        let id = LANE.lock().ok().and_then(|g| g.as_ref().and_then(|l| l.session_id));
        let Some(id) = id else { return Ok(()) };
        let db = db(&app).ok_or("no database")?;
        db.save_meeting_note(id, &body);
        Ok(())
    }

    /// Current settings for the Settings UI.
    #[tauri::command]
    pub fn get_meeting_settings() -> Settings {
        LANE.lock()
            .ok()
            .and_then(|g| g.as_ref().map(|l| l.settings.clone()))
            .unwrap_or_default()
    }

    /// Tier (a): the whole feature on or off (FR-MT-02a).
    ///
    /// Switching off while a meeting is running ends it on the spot, through the same path as
    /// Stop — so the off switch reaches a meeting already in progress rather than only preventing
    /// the next one.
    #[tauri::command]
    pub fn set_meeting_enabled(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
        let now = now_ms();
        let settings = {
            let Ok(mut g) = LANE.lock() else { return Err("busy".into()) };
            let Some(lane) = g.as_mut() else { return Err("not ready".into()) };
            lane.settings.enabled = enabled;
            if !enabled {
                let effects = lane.machine.step(Input::FeatureDisabled);
                apply(&app, lane, &effects, now);
            } else {
                emit(&app, lane, now);
            }
            lane.settings.clone()
        };
        eprintln!("[meeting] notes → {}", if enabled { "enabled" } else { "off" });
        save(&app, &settings)
    }

    /// Tier (b): never offer for this app again (FR-MT-02b).
    #[tauri::command]
    pub fn meeting_exclude_app(bundle_id: String, app: tauri::AppHandle) -> Result<(), String> {
        let now = now_ms();
        let settings = {
            let Ok(mut g) = LANE.lock() else { return Err("busy".into()) };
            let Some(lane) = g.as_mut() else { return Err("not ready".into()) };
            lane.settings.exclude_app(&bundle_id);
            // Excluding from the offer panel also declines the meeting that prompted it.
            let effects = lane.machine.step(Input::NotNow);
            apply(&app, lane, &effects, now);
            lane.settings.clone()
        };
        save(&app, &settings)
    }
}
