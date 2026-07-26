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
    use shogun_core::meeting::detect::{self, Decision, LiveSignals, Signals};
use shogun_core::meeting::gate::OfferGate;
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
        /// The interval that just finished — what Recap reads. Kept separately from
        /// `session_id`, which is cleared the moment the interval closes.
        last_session_id: Option<i64>,
        title: Option<String>,
        app_bundle_id: Option<String>,
        /// Carried from the detector so the stored interval records what was actually observed,
        /// rather than a constant that would claim more (or less) than the evidence (FR-MT-04).
        confidence: f64,
        provenance: String,
        /// Epoch ms of the transition into the current state — the pill's clock.
        since_ms: i64,
        /// What the user has already declined, and until when (FR-MT-02c). Deliberately not in
        /// `settings`: a decline changes no settings and must not outlive the process.
        gate: OfferGate,
    }

    impl Lane {
        fn new() -> Self {
            Self {
                settings: Settings::default(),
                machine: Machine::new(Params::default()),
                session_id: None,
                last_session_id: None,
                title: None,
                app_bundle_id: None,
                confidence: 0.0,
                provenance: "{}".to_string(),
                since_ms: 0,
                gate: OfferGate::new(),
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
        /// The app the offer is about — what "never for this app" (FR-MT-02b) would exclude.
        pub app_bundle_id: Option<String>,
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
            app_bundle_id: lane.app_bundle_id.clone(),
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
        // An interval left open by a crash, a force-quit or a power cut would otherwise stay
        // `ended_at IS NULL` forever, and `active()` assumes at most one open row. Close it at
        // its last known moment rather than pretending it is still running.
        if let Some(db) = app.try_state::<shogun_core::daemon::Db>() {
            let closed = db.close_abandoned_meetings();
            if closed > 0 {
                eprintln!("[meeting] closed {closed} interval(s) left open by a previous run");
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
                        lane.last_session_id = Some(id);
                        if close_session(app, id) {
                            eprintln!("[meeting] session {id} closed ({why:?})");
                        } else {
                            // The row stays open. Say so — a silent failure here leaves an
                            // interval that never ends, and nothing else would ever mention it.
                            eprintln!("[meeting] session {id} could not be closed ({why:?})");
                        }
                    }
                }
                // MT3. Not silently ignored: until the audio lane exists, the honest behaviour is
                // to record intervals and notes without listening (FR-MT-13, OPEN-07/08).
                Effect::StartAudio | Effect::StopAudio => {}
                // The tick loop drives the countdown and the silence watchdog, so the machine's
                // timer requests need no separate scheduler here.
                Effect::StartTimer { .. } | Effect::CancelTimer(_) => {}
                // MT2 shows the degraded Recap; `meeting_recap` reads it from the closed
                // interval, so nothing to do here beyond having closed the session above.
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

    fn close_session(app: &tauri::AppHandle, id: i64) -> bool {
        db(app).is_some_and(|db| db.close_meeting(id))
    }

    fn step(app: &tauri::AppHandle, input: Input) {
        let now = now_ms();
        let Ok(mut g) = LANE.lock() else { return };
        let Some(lane) = g.as_mut() else { return };
        let effects = lane.machine.step_completing_wrap(input);
        apply(app, lane, &effects, now);
    }

    /// Called on every focus change with the frontmost app.
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
        // Switching apps ends the cooldown on the one left behind: coming back later is a new
        // meeting and deserves to be asked about again.
        lane.gate.observe_front(bundle_id);
        if lane.machine.state() != State::Idle {
            return;
        }
        // The user already said no to this app recently. Without this the decline lasts exactly
        // one tick: the machine returns to Idle, the meeting app is still in front, and the offer
        // comes straight back — "Not now" would buy one second and Stop would be followed by a
        // fresh offer that starts again ten seconds later (FR-MT-02c).
        if !lane.gate.may_offer(bundle_id, now) {
            return;
        }
        if !lane.settings.may_offer(&OfferContext {
            app_bundle_id: Some(bundle_id),
            occurrence_external_id: None,
        }) {
            return;
        }

        // Signal (2) only. The microphone-in-use and AX-controls signals of FR-MT-04 need native
        // probes that do not exist yet; claiming them here would inflate the confidence stored
        // against the interval beyond what was actually observed.
        let signals = Signals {
            meeting_app_frontmost: detect::is_meeting_app(bundle_id)
                || window_title.is_some_and(detect::title_looks_like_meeting),
            ..Default::default()
        };
        if let Decision::Offer { confidence, provenance } = detect::decide(&signals) {
            // The window title, not the app name: "Weekly sync" is what the user calls the
            // meeting, and `zoom.us` on every row would make the whole timeline look identical.
            lane.title = window_title.map(str::to_string);
            lane.app_bundle_id = Some(bundle_id.to_string());
            lane.confidence = confidence;
            lane.provenance = provenance;
            let effects = lane.machine.step_completing_wrap(Input::MeetingDetected);
            apply(app, lane, &effects, now);
        }
    }

    /// One-second tick: advances the offer countdown, ends meetings that are over, and keeps the
    /// pill's clock moving.
    pub fn tick(app: &tauri::AppHandle) {
        let now = now_ms();
        enum Next {
            Nothing,
            Emit,
            Step(Input),
        }

        // Decide and act under one lock. Reading the state, releasing, and stepping later leaves
        // a window in which the user's "Not now" lands in between — and a late GraceExpired then
        // matches the *new* Offered, starting a meeting the user just declined with no countdown
        // at all.
        let next = {
            let Ok(mut g) = LANE.lock() else { return };
            let Some(lane) = g.as_mut() else { return };
            match lane.machine.state() {
                State::Idle => Next::Nothing,
                State::Offered => {
                    // `checked_sub`-style guard: a clock that jumped backwards (NTP, wake from
                    // sleep) would otherwise freeze the countdown until real time caught up.
                    let elapsed = now.saturating_sub(lane.since_ms);
                    if !(0..Params::default().offer_grace_ms as i64).contains(&elapsed) {
                        Next::Step(Input::GraceExpired)
                    } else {
                        Next::Emit
                    }
                }
                State::Recording => {
                    // FR-MT-11. Without this the interval never closes on its own: the user quits
                    // the meeting app and the pill keeps counting until they think to press Stop.
                    //
                    // `last_sound_at` is pinned to `now` because there is no audio lane yet, so
                    // the silence condition cannot fire and must not pretend to. It becomes real
                    // in MT3.
                    let present = lane
                        .app_bundle_id
                        .as_deref()
                        // `map_or(true, ..)`: `is_none_or` needs Rust 1.82, MSRV here is 1.80.
                        .map_or(true, crate::display::is_app_running);
                    let live = LiveSignals {
                        meeting_app_present: present,
                        occurrence_ends_at: None,
                        last_sound_at: now,
                    };
                    match detect::end_condition(&live, now) {
                        Some(why) => Next::Step(Input::AutoEnd(why)),
                        None => Next::Emit,
                    }
                }
                State::Wrapping => Next::Emit,
            }
        };

        match next {
            Next::Nothing => {}
            Next::Emit => {
                if let Ok(g) = LANE.lock() {
                    if let Some(lane) = g.as_ref() {
                        emit(app, lane, now);
                    }
                }
            }
            Next::Step(input) => step(app, input),
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
                // The window title is what names the meeting; the app name is the fallback when
                // Accessibility has nothing (permission not granted, or a window with no title).
                let title = crate::axcache::focused_window(front.pid)
                    .and_then(|w| w.title())
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| front.name.clone());
                on_focus(&app, &front.bundle_id, Some(&title));
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
                app_bundle_id: None,
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
    }

    /// Save the note typed during the meeting (FR-MT-10). Silently does nothing when no interval
    /// is open — there is nothing to attach a note to, and losing the text is better than
    /// inventing a session for it.
    #[tauri::command]
    pub fn meeting_save_note(body: String, app: tauri::AppHandle) -> Result<(), String> {
        let id = LANE.lock().ok().and_then(|g| g.as_ref().and_then(|l| l.session_id));
        let Some(id) = id else { return Ok(()) };
        let db = db(&app).ok_or("no database")?;
        // Report the failure. Swallowing it would tell the webview the note is safe while the
        // text the user typed is gone — the one piece of a meeting record that cannot be
        // regenerated (FR-MT-10).
        if db.save_meeting_note(id, &body) {
            Ok(())
        } else {
            Err("could not save the note".into())
        }
    }

    /// The Recap for the most recently finished meeting (FR-MT-19), if there is one.
    ///
    /// Degraded by construction in MT2: assembled locally from the interval, the user's note and
    /// what was captured, with no model and no network.
    #[tauri::command]
    pub fn meeting_recap(app: tauri::AppHandle) -> Option<shogun_core::meeting::recap::Recap> {
        let id = LANE.lock().ok().and_then(|g| g.as_ref().and_then(|l| l.last_session_id))?;
        db(&app).and_then(|db| db.meeting_recap(id))
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
    ///
    /// **Persist first, then apply.** The other order lets a failed write leave the backend
    /// enabled while the settings screen (which rolls its toggle back on error) reads "Off" — the
    /// exact "off but something is running" state FR-MT-02a exists to forbid. It also means a
    /// user's "off" that failed to reach disk cannot come back as "on" after a restart.
    #[tauri::command]
    pub fn set_meeting_enabled(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
        let now = now_ms();
        let candidate = {
            let Ok(g) = LANE.lock() else { return Err("busy".into()) };
            let Some(lane) = g.as_ref() else { return Err("not ready".into()) };
            Settings { enabled, ..lane.settings.clone() }
        };
        save(&app, &candidate)?;

        let Ok(mut g) = LANE.lock() else { return Err("busy".into()) };
        let Some(lane) = g.as_mut() else { return Err("not ready".into()) };
        lane.settings = candidate;
        if !enabled {
            let effects = lane.machine.step_completing_wrap(Input::FeatureDisabled);
            apply(&app, lane, &effects, now);
        } else {
            emit(&app, lane, now);
        }
        eprintln!("[meeting] notes → {}", if enabled { "enabled" } else { "off" });
        Ok(())
    }

    /// Undo tier (b): offer for this app again.
    ///
    /// Exclusions have to be reversible from the settings screen. A list the user can add to but
    /// never remove from turns one impatient tap during a meeting into a permanent blind spot.
    #[tauri::command]
    pub fn meeting_include_app(bundle_id: String, app: tauri::AppHandle) -> Result<(), String> {
        let settings = {
            let Ok(mut g) = LANE.lock() else { return Err("busy".into()) };
            let Some(lane) = g.as_mut() else { return Err("not ready".into()) };
            lane.settings.excluded_apps.remove(&bundle_id);
            lane.settings.clone()
        };
        save(&app, &settings)
    }

    /// Tier (b): never offer for this app again (FR-MT-02b).
    #[tauri::command]
    pub fn meeting_exclude_app(bundle_id: String, app: tauri::AppHandle) -> Result<(), String> {
        let now = now_ms();
        let candidate = {
            let Ok(g) = LANE.lock() else { return Err("busy".into()) };
            let Some(lane) = g.as_ref() else { return Err("not ready".into()) };
            let mut next = lane.settings.clone();
            next.exclude_app(&bundle_id);
            next
        };
        save(&app, &candidate)?;

        let Ok(mut g) = LANE.lock() else { return Err("busy".into()) };
        let Some(lane) = g.as_mut() else { return Err("not ready".into()) };
        lane.settings = candidate;
        // Excluding from the offer panel also declines whatever prompted it — from Offered that
        // is the pending offer, and from Recording the meeting in progress.
        let input = if lane.machine.state() == State::Recording { Input::Stop } else { Input::NotNow };
        let effects = lane.machine.step_completing_wrap(input);
        apply(&app, lane, &effects, now);
        Ok(())
    }
}
