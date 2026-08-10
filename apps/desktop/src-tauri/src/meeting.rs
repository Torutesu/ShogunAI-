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
//! 2. **Audio degrades, never crashes.** `Effect::StartAudio` opens the capture lane
//!    (`audio_lane`) against the interval the machine just opened; when audio is unavailable (no
//!    model, denied mic, no system tap) the lane returns nothing and the meeting still records the
//!    interval and the user's notes (FR-MT-13, MT3).

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
    use std::sync::{Arc, Mutex, RwLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde::Serialize;
    use shogun_core::meeting::detect::{self, Decision, LiveSignals, MicWatch, Signals};
use shogun_core::meeting::gate::OfferGate;
    use shogun_core::meeting::settings::{MeetingLanguage, MeetingMode, OfferContext, Settings};
    use shogun_core::meeting::statemachine::{Effect, EndReason, Input, Machine, Params, State};
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
        /// Turns "the microphone is open" into "a call is happening" (FR-MT-04 signal ②).
        mic: MicWatch,
        /// The running audio lane (MT3), when one is capturing. `None` while idle, or when audio
        /// degraded to notes-only. Held here so `StopAudio` can tear the exact same lane down.
        audio: Option<crate::audio_lane::Handle>,
        /// Set when the offer that opened this interval saw a Meet URL (FR-MT-11 tab/window end).
        opened_via_meet_url: bool,
        /// When the session browser's frontmost tab first stopped looking like a meeting.
        url_lost_since_ms: Option<i64>,
        /// When the system mic last transitioned to closed — debounces hang-up flicker (FR-MT-11).
        mic_closed_since_ms: Option<i64>,
        /// Shared with the audio lane — mode/lang changes apply to new lines mid-meeting.
        live_settings: Arc<RwLock<Settings>>,
        /// User dismissed the live overlay during recording; recording continues.
        overlay_dismissed: bool,
        /// Why the last interval closed — drives shorter Recap auto-dismiss after auto-end.
        last_end_reason: Option<EndReason>,
        /// Capture/ASR paused while the meeting interval stays open (waveform toggle).
        /// Not a machine state — Stop still ends the session; pause only holds the mic/ASR lane.
        paused: bool,
    }

    impl Lane {
        fn new() -> Self {
            let settings = Settings::default();
            Self {
                settings: settings.clone(),
                machine: Machine::new(Params::default()),
                session_id: None,
                last_session_id: None,
                title: None,
                app_bundle_id: None,
                confidence: 0.0,
                provenance: "{}".to_string(),
                since_ms: 0,
                gate: OfferGate::new(),
                mic: MicWatch::new(),
                audio: None,
                opened_via_meet_url: false,
                url_lost_since_ms: None,
                mic_closed_since_ms: None,
                live_settings: Arc::new(RwLock::new(settings)),
                overlay_dismissed: false,
                last_end_reason: None,
                paused: false,
            }
        }
    }

    static LANE: Mutex<Option<Lane>> = Mutex::new(None);
    /// Session id allowed to push `meeting_live_line` to the webview. Cleared before audio stop
    /// so late whisper flushes after Stop do not repaint a hidden overlay.
    static LIVE_EMIT_SESSION: AtomicI64 = AtomicI64::new(0);

    /// Whether the audio lane may emit live transcript lines to the overlay for `session_id`.
    pub fn live_emit_allowed(session_id: i64) -> bool {
        session_id > 0 && LIVE_EMIT_SESSION.load(Ordering::Acquire) == session_id
    }

    /// True while a meeting interval is open (capture poller uses this for screen-OCR fusion).
    pub fn is_recording() -> bool {
        LANE.lock()
            .ok()
            .and_then(|g| g.as_ref().map(|l| l.machine.state() == State::Recording))
            .unwrap_or(false)
    }

    fn set_live_emit_session(session_id: i64) {
        LIVE_EMIT_SESSION.store(session_id, Ordering::Release);
    }

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
        /// True while capture/ASR is paused; meeting interval stays open (not ended).
        pub paused: bool,
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
            paused: state == State::Recording && lane.paused,
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
                    lane.settings = saved.clone();
                    if let Ok(mut live) = lane.live_settings.write() {
                        *live = saved;
                    }
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
        // Built here because `init` runs in Tauri's setup, on the main thread.
        match build_overlay(app) {
            Some(_) => eprintln!("[meeting] overlay window ready (hidden)"),
            None => eprintln!("[meeting] overlay window unavailable — the panel will not appear"),
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

    /// Apply the machine's effects. The audio handle to stop is returned so callers can join the
    /// capture thread **after** releasing `LANE`: `StopAudio` can block on a whisper flush, and
    /// holding the lane lock while the audio thread emits live lines can deadlock the main thread
    /// on `meeting_status` / other lane commands.
    fn apply(
        app: &tauri::AppHandle,
        lane: &mut Lane,
        effects: &[Effect],
        now: i64,
    ) -> Option<crate::audio_lane::Handle> {
        let mut stop_audio = None;
        for fx in effects {
            match fx {
                Effect::Transition(state) => {
                    lane.since_ms = now;
                    if *state == State::Idle {
                        lane.overlay_dismissed = false;
                        lane.last_end_reason = None;
                        lane.paused = false;
                    }
                }
                Effect::OpenSession => {
                    lane.session_id = open_session(app, lane);
                }
                Effect::CloseSession(why) => {
                    lane.last_end_reason = Some(*why);
                    lane.paused = false;
                    if let Some(id) = lane.session_id.take() {
                        lane.last_session_id = Some(id);
                        lane.opened_via_meet_url = false;
                        lane.url_lost_since_ms = None;
                        lane.mic_closed_since_ms = None;
                        lane.overlay_dismissed = false;
                        if close_session(app, id) {
                            eprintln!("[meeting] session {id} closed ({why:?})");
                        } else {
                            // The row stays open. Say so — a silent failure here leaves an
                            // interval that never ends, and nothing else would ever mention it.
                            eprintln!("[meeting] session {id} could not be closed ({why:?})");
                        }
                    }
                }
                // MT3. Open the capture lane against the interval the machine just opened. When
                // audio degrades (no model, denied mic, no tap), `start` returns None and the
                // meeting still records notes (FR-MT-13, OPEN-07/08).
                Effect::StartAudio => {
                    if let Some(id) = lane.session_id {
                        lane.overlay_dismissed = false;
                        lane.paused = false;
                        set_live_emit_session(id);
                        if let Ok(mut live) = lane.live_settings.write() {
                            *live = lane.settings.clone();
                        }
                        lane.audio = crate::audio_lane::start(
                            app,
                            id,
                            lane.live_settings.clone(),
                        );
                    }
                }
                Effect::StopAudio => {
                    lane.paused = false;
                    set_live_emit_session(0);
                    stop_audio = lane.audio.take();
                }
                // The tick loop drives the countdown and the silence watchdog, so the machine's
                // timer requests need no separate scheduler here.
                Effect::StartTimer { .. } | Effect::CancelTimer(_) => {}
                // MT4: kick off the model Recap for the interval that just closed. The degraded
                // MT2 Recap is already readable from the closed interval, so this is pure upgrade:
                // `meeting_recap::spawn` runs the Batch on a background thread and emits
                // `meeting_recap` when the minutes are stored — a failure leaves the degraded Recap
                // untouched (FR-MT-19). `CloseSession` above moved the id into `last_session_id`.
                Effect::BuildRecap => {
                    if let Some(id) = lane.last_session_id {
                        crate::meeting_recap::spawn(app, id, lane.settings.language);
                    }
                }
            }
        }
        emit(app, lane, now);
        stop_audio
    }

    fn finish_audio_stop(handle: Option<crate::audio_lane::Handle>) {
        crate::audio_lane::stop(handle);
    }

    fn emit(app: &tauri::AppHandle, lane: &Lane, now: i64) {
        let v = view(lane, now);
        sync_window(app, lane.machine.state(), lane.settings.enabled, lane.overlay_dismissed);
        // Skip redundant webview events: the tick fires every second but Wrapping is static
        // until dismiss, and Offered/Recording only need a push when the view actually changed.
        static LAST_EMIT: Mutex<Option<MeetingView>> = Mutex::new(None);
        let changed = match LAST_EMIT.lock() {
            Ok(mut last) => {
                let changed = last.as_ref() != Some(&v);
                if changed {
                    *last = Some(v.clone());
                }
                changed
            }
            Err(_) => true,
        };
        if changed {
            let _ = app.emit("meeting", v);
        }
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
        let stop_audio = {
            let Ok(mut g) = LANE.lock() else { return };
            let Some(lane) = g.as_mut() else { return };
            let effects = lane.machine.step(input);
            if effects.is_empty() {
                return;
            }
            apply(app, lane, &effects, now)
        };
        finish_audio_stop(stop_audio);
    }

    /// Called on every focus change with the frontmost app.
    ///
    /// Returns immediately when the feature is off — the detector does not run, so nothing
    /// observes a meeting while meeting notes are disabled (FR-MT-02a).
    pub fn on_focus(
        app: &tauri::AppHandle,
        bundle_id: &str,
        window_title: Option<&str>,
        page_url: Option<&str>,
        mic_open: bool,
    ) {
        let now = now_ms();
        let Ok(mut g) = LANE.lock() else { return };
        let Some(lane) = g.as_mut() else { return };

        // Fed every tick, including while a meeting is already running: the watch measures a
        // continuous stretch, so skipping observations would make it forget the call is ongoing.
        lane.mic.observe(mic_open, now);
        let mic_sustained_ms = lane.mic.sustained_ms(now);

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

        // Signal (2) only. The AX-controls signal of FR-MT-04 needs native probes that do not
        // exist yet; claiming them here would inflate the confidence stored against the interval
        // beyond what was actually observed.
        let has_meet_url = page_url.is_some_and(detect::is_meeting_url);
        let has_zoom_bundle = detect::is_meeting_app(bundle_id);
        let meeting_context = has_zoom_bundle || has_meet_url;
        let on_media_page = page_url.is_some_and(detect::is_media_url);
        let page_host = page_url.and_then(detect::host_from_url);
        let signals = Signals {
            // Sustained mic: suppressed on media pages unless a meeting URL/app is already in
            // front; mic-only elsewhere needs 30s, not 10s (FR-MT-04 — ② alone is weak).
            mic_in_use: detect::mic_counts_as_signal(
                mic_sustained_ms,
                meeting_context,
                on_media_page,
            ),
            // Corroboration: a known meeting app, or a browser on a meeting page. Either can
            // still open an interval alone, so a call whose audio does not run through the
            // default input device is not invisible.
            meeting_app_frontmost: meeting_context,
            ..Default::default()
        };
        let ctx = detect::DetectionCtx {
            is_browser: is_browser(bundle_id),
            page_host: page_host.as_deref(),
            has_meet_url,
            has_zoom_bundle,
            window_title,
        };
        let policy = detect::OfferPolicy {
            allow_mic_only: lane.settings.allow_mic_only_detect,
        };
        if let Decision::Offer { confidence, provenance } =
            detect::evaluate_offer(&signals, &ctx, &policy)
        {
            // The window title, not the app name: "Weekly sync" is what the user calls the
            // meeting, and `zoom.us` on every row would make the whole timeline look identical.
            lane.title = window_title.map(str::to_string);
            lane.app_bundle_id = Some(bundle_id.to_string());
            lane.opened_via_meet_url = has_meet_url;
            lane.url_lost_since_ms = None;
            lane.mic_closed_since_ms = None;
            lane.confidence = confidence;
            lane.provenance = provenance;
            let effects = lane.machine.step(Input::MeetingDetected);
            let stop_audio = apply(app, lane, &effects, now);
            drop(g);
            finish_audio_stop(stop_audio);
        }
    }

    /// Frontmost-app facts for the recording watchdog (FR-MT-11). `None` when the driver could
    /// not read the frontmost app this tick.
    struct TickObservation<'a> {
        bundle_id: &'a str,
        page_url: Option<&'a str>,
        window_title: Option<&'a str>,
        is_browser: bool,
    }

    /// Bundle ids for this build. The overlay often reports an empty bundle id; both mean
    /// "SHOGUN is frontmost" and must not start the Meet-tab leave grace (FR-MT-11).
    fn is_shogun_frontmost(bundle_id: &str) -> bool {
        // Empty bundle = NSPanel quirk (always us). Otherwise match owned identifiers.
        bundle_id.is_empty() || crate::display::is_own_app(bundle_id, "")
    }

    /// Update grace timer when a Meet-URL session's browser is frontmost and no longer on Meet,
    /// or when the session browser is no longer frontmost at all (user switched to another app).
    fn observe_mic_closed(lane: &mut Lane, mic_open: bool, now: i64) {
        if mic_open {
            lane.mic_closed_since_ms = None;
        } else if lane.mic_closed_since_ms.is_none() {
            lane.mic_closed_since_ms = Some(now);
        }
    }

    fn recording_app_present(lane: &mut Lane, _obs: Option<&TickObservation<'_>>, now: i64, mic_open: bool) -> bool {
        if lane.opened_via_meet_url {
            return detect::meet_url_session_present(
                lane.url_lost_since_ms,
                now,
                mic_open,
                lane.mic_closed_since_ms,
            );
        }
        match lane.app_bundle_id.as_deref() {
            None | Some("") => lane.mic.observe(mic_open, now),
            Some(bundle_id) => crate::display::is_app_running(bundle_id),
        }
    }

    fn recap_dismiss_ms(reason: Option<EndReason>) -> i64 {
        match reason {
            Some(EndReason::UserStopped) => Machine::RECAP_DISMISS_MS,
            _ => Machine::RECAP_DISMISS_LEFT_MS,
        }
    }

    fn observe_meeting_url(lane: &mut Lane, obs: &TickObservation<'_>, now: i64) {
        if !lane.opened_via_meet_url {
            return;
        }
        let Some(session_browser) = lane.app_bundle_id.as_deref() else {
            return;
        };
        if session_browser.is_empty() {
            return;
        }
        if is_shogun_frontmost(obs.bundle_id) {
            return;
        }
        if obs.bundle_id != session_browser || !obs.is_browser {
            // Left the session browser — same grace as leaving the Meet tab (FR-MT-11).
            if lane.url_lost_since_ms.is_none() {
                lane.url_lost_since_ms = Some(now);
            }
            return;
        }
        if detect::browser_meeting_page_present(obs.page_url, obs.window_title) {
            lane.url_lost_since_ms = None;
        } else if lane.url_lost_since_ms.is_none() {
            lane.url_lost_since_ms = Some(now);
        }
    }

    /// One-second tick: advances the offer countdown, ends meetings that are over, and keeps the
    /// pill's clock moving.
    fn tick(app: &tauri::AppHandle, obs: Option<TickObservation<'_>>) {
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
                    let mic_open = crate::mic::input_in_use();
                    observe_mic_closed(lane, mic_open, now);
                    if let Some(obs) = obs.as_ref() {
                        observe_meeting_url(lane, obs, now);
                    }
                    let last_sound_at = lane
                        .audio
                        .as_ref()
                        .map(|h| h.last_audio_at())
                        .unwrap_or(now);
                    let present = recording_app_present(lane, obs.as_ref(), now, mic_open);
                    let live = LiveSignals {
                        meeting_app_present: present,
                        occurrence_ends_at: None,
                        last_sound_at,
                    };
                    match detect::end_condition(&live, now) {
                        Some(why) => Next::Step(Input::AutoEnd(why)),
                        None => Next::Emit,
                    }
                }
                State::Wrapping => {
                    let dismiss_ms = recap_dismiss_ms(lane.last_end_reason);
                    if now.saturating_sub(lane.since_ms) > dismiss_ms {
                        Next::Step(Input::Wrapped)
                    } else {
                        Next::Emit
                    }
                }
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

    /// Browsers whose current page is worth asking about (FR-MT-04). A table, so the per-tick
    /// Accessibility call is only paid where it can produce an answer.
    const BROWSER_BUNDLE_IDS: &[&str] = &[
        "com.google.Chrome",
        "com.google.Chrome.beta",
        "com.google.Chrome.canary",
        "com.apple.Safari",
        "company.thebrowser.Browser", // Arc
        "company.thebrowser.dia",     // Dia
        "com.microsoft.edgemac",
        "com.brave.Browser",
        "org.mozilla.firefox",
    ];

    fn is_browser(bundle_id: &str) -> bool {
        BROWSER_BUNDLE_IDS.contains(&bundle_id)
    }

    /// The lane's current state, for the diagnostic line. Never blocks: a state read that waited
    /// on the lock would make the log the thing that hides the problem.
    fn state_tag() -> &'static str {
        match LANE.try_lock() {
            Ok(g) => g.as_ref().map_or("none", |l| l.machine.state().tag()),
            Err(_) => "busy",
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
                // Only asked of browsers: every other app would pay an Accessibility round-trip
                // per second to answer "no".
                let url = is_browser(&front.bundle_id)
                    .then(|| crate::axcache::browser_url(front.pid))
                    .flatten();
                // Diagnostic while the browser table is confirmed on real machines: which app
                // was seen, and whether a URL could be read at all. Printed only on change so a
                // steady desktop stays quiet.
                //
                // **Host only, never the full URL.** A path and query string carry session ids,
                // document names and search terms — user content, which must not reach a log
                // (CLAUDE.md). The host is all this diagnostic needs, and it is also the only
                // part detection looks at.
                {
                    use std::sync::Mutex;
                    static LAST: Mutex<String> = Mutex::new(String::new());
                    let host = url.as_deref().map(|u| {
                        u.split_once("://")
                            .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or(""))
                            .unwrap_or("")
                            .to_string()
                    });
                    let line = format!(
                        "{} state={} mic={} browser={} host={} title={:?}",
                        front.bundle_id,
                        state_tag(),
                        crate::mic::input_in_use(),
                        is_browser(&front.bundle_id),
                        host.as_deref().unwrap_or("-"),
                        title.chars().take(40).collect::<String>()
                    );
                    if let Ok(mut g) = LAST.lock() {
                        if *g != line {
                            eprintln!("[meeting] saw {line}");
                            *g = line;
                        }
                    }
                }
                on_focus(
                    &app,
                    &front.bundle_id,
                    Some(&title),
                    url.as_deref(),
                    crate::mic::input_in_use(),
                );
                tick(
                    &app,
                    Some(TickObservation {
                        bundle_id: &front.bundle_id,
                        page_url: url.as_deref(),
                        window_title: Some(&title),
                        is_browser: is_browser(&front.bundle_id),
                    }),
                );
            } else {
                tick(&app, None);
            }
        })
    }

    // ── The floating overlay ────────────────────────────────────────────────────────────────
    //
    // A window of its own rather than the notch (Issue #7: floating near meeting controls).
    // Offer card parks top-right; in-meeting pill parks bottom-center above the mic bar.

    const WINDOW_LABEL: &str = "meeting";
    /// Offered: white horizontal pill (Meeting detected). Room for title stack + Take Notes.
    const OFFER_SIZE: (f64, f64) = (440.0, 96.0);
    /// Idle / hidden fallback size.
    const BAR_SIZE: (f64, f64) = (400.0, 88.0);
    /// In-meeting black control capsule only (notes panel closed).
    /// Height includes top inset for bar-slot tooltips (they sit above the 52px bar).
    const PILL_SIZE: (f64, f64) = (320.0, 100.0);
    /// Live captions/transcript pane + control capsule during recording (issue #93).
    /// Wide enough for one-way split columns.
    const LIVE_SIZE: (f64, f64) = (560.0, 360.0);
    /// AI Canvas alone + control capsule (Notes pill).
    const CANVAS_SIZE: (f64, f64) = (400.0, 380.0);
    /// AI side chat alone + control capsule.
    const CHAT_SIZE: (f64, f64) = (360.0, 520.0);
    /// Multiple panels stacked above the control capsule.
    const BOTH_SIZE: (f64, f64) = (520.0, 720.0);
    const RECAP_SIZE: (f64, f64) = (400.0, 280.0);
    /// Whether the live captions panel is expanded above the control pill.
    static OVERLAY_PANEL_OPEN: AtomicBool = AtomicBool::new(true);
    /// Whether the AI Canvas panel is open above the control pill (Notes button).
    static OVERLAY_CANVAS_OPEN: AtomicBool = AtomicBool::new(false);
    /// Whether the AI Chat panel is open above the control pill.
    static OVERLAY_CHAT_OPEN: AtomicBool = AtomicBool::new(false);
    /// User-resized overlay size while a panel is open (logical px). Cleared when all panels close.
    static OVERLAY_CUSTOM_SIZE: std::sync::Mutex<Option<(f64, f64)>> = std::sync::Mutex::new(None);
    const OVERLAY_SIZE_MIN: (f64, f64) = (300.0, 220.0);
    const OVERLAY_SIZE_MAX: (f64, f64) = (720.0, 900.0);
    /// Distance from screen edges, in logical pixels.
    const MARGIN: f64 = 16.0;
    /// Menu-bar height to clear for top-right parking.
    const MENUBAR_H: f64 = 28.0;
    /// Distance from the bottom of the visible screen — clears Meet/Zoom mic bar.
    const BOTTOM_MARGIN: f64 = 100.0;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ParkMode {
        TopRight,
        BottomCenter,
    }

    fn park_mode_for_state(state: State) -> ParkMode {
        match state {
            State::Recording => ParkMode::BottomCenter,
            // Offer card and Recap are notification-style surfaces, not in-meeting controls.
            State::Offered | State::Wrapping | State::Idle => ParkMode::TopRight,
        }
    }

    /// Build the overlay window, hidden. **Setup only — this must run on the main thread.**
    ///
    /// Creating a window is an AppKit call, and AppKit is main-thread-only: building it lazily
    /// from the detection thread the first time a meeting appeared took the whole app down at
    /// exactly the moment it was supposed to start working. The window is therefore made once at
    /// launch and merely shown and hidden afterwards, which is safe from any thread.
    pub fn build_overlay(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
        if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
            return Some(win);
        }
        let win = tauri::WebviewWindowBuilder::new(
            app,
            WINDOW_LABEL,
            // Same entry point as the notch window. `App("index.html")` resolved to a URL the
            // dev server did not serve, so the window existed with a webview that never ran any
            // JavaScript — shown, sized, positioned, and completely blank.
            tauri::WebviewUrl::default(),
        )
        .title("ShogunAI — meeting")
        .transparent(true)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .shadow(false)
        .skip_taskbar(true)
        .inner_size(BAR_SIZE.0, BAR_SIZE.1)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|e| eprintln!("[meeting] overlay window build failed: {e}"))
        .ok()?;
        configure_overlay_window(&win);
        // What the webview was actually pointed at. A window whose webview never runs any
        // JavaScript looks exactly like a window that was never created.
        eprintln!("[meeting] overlay url = {:?}", win.url().map(|u| u.to_string()));
        Some(win)
    }

    /// One-time NSWindow setup for the meeting overlay. Deliberately NOT `float_on_all_spaces`:
    /// that helper orders the window front and sets `canHide=false` / `movableByWindowBackground`,
    /// which left a transparent full-window hit target blocking the desktop even when the lane
    /// was Idle and `hide()` had been called.
    fn configure_overlay_window(win: &tauri::WebviewWindow) {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        use objc2::class;
        use std::sync::atomic::Ordering;

        let ptr = match win.ns_window() {
            Ok(p) if !p.is_null() => p as *mut AnyObject,
            _ => {
                eprintln!("[meeting] ns_window unavailable — overlay may not float correctly");
                return;
            }
        };
        let behavior = crate::PANEL_BEHAVIOR.load(Ordering::Relaxed);
        let level = crate::OVERLAY_LEVEL;
        // SAFETY: live NSWindow on the main thread (setup).
        unsafe {
            // Transparent webview: default NSWindow backing is opaque grey — it peeks where
            // WKWebView does not clip CSS border-radius (offer card corners).
            let _: () = msg_send![ptr, setOpaque: false];
            let clear: *mut AnyObject = msg_send![class!(NSColor), clearColor];
            let _: () = msg_send![ptr, setBackgroundColor: clear];
            let _: () = msg_send![ptr, setHasShadow: false];
            let _: () = msg_send![ptr, setCollectionBehavior: behavior];
            let _: () = msg_send![ptr, setLevel: level];
            let _: () = msg_send![ptr, setHidesOnDeactivate: false];
            // Must stay hideable — the notch overlay sets canHide=false for residency, but this
            // window must disappear entirely when the lane is Idle.
            let _: () = msg_send![ptr, setCanHide: true];
            // Drag is via the grip strip and glass headers (`-webkit-app-region: drag` in CSS, or
            // `meeting_drag` → start_dragging). A movable background turns the whole frame into
            // an invisible click-catcher — transparent padding and rounded corners still block
            // clicks behind the window at the AppKit layer.
            let _: () = msg_send![ptr, setMovableByWindowBackground: false];
            // Start click-through until sync_window shows real UI.
            let _: () = msg_send![ptr, setIgnoresMouseEvents: true];
        }
        eprintln!("[meeting] overlay window configured (hidden, click-through)");
    }

    fn overlay_ns_window(win: &tauri::WebviewWindow) -> Option<*mut objc2::runtime::AnyObject> {
        match win.ns_window() {
            Ok(p) if !p.is_null() => Some(p as *mut objc2::runtime::AnyObject),
            _ => None,
        }
    }

    /// Webview-owned desire: capture mouse on the glass card (`ignoresMouseEvents=false`).
    static OVERLAY_WANTS_INTERACTIVE: AtomicBool = AtomicBool::new(false);

    /// AppKit-only click-through toggle. Never orderOut here — visibility is sync_window's job.
    fn set_overlay_ignores_mouse(win: &tauri::WebviewWindow, ignores: bool) {
        use objc2::msg_send;
        let Some(ptr) = overlay_ns_window(win) else { return };
        // SAFETY: live NSWindow; called from the main thread via sync_window / setup.
        unsafe {
            let _: () = msg_send![ptr, setIgnoresMouseEvents: ignores];
        }
    }

    fn apply_overlay_interactive(win: &tauri::WebviewWindow) {
        let interactive = OVERLAY_WANTS_INTERACTIVE.load(Ordering::SeqCst);
        set_overlay_ignores_mouse(win, !interactive);
    }

    fn overlay_monitor(win: &tauri::WebviewWindow) -> Option<tauri::Monitor> {
        match win.current_monitor() {
            Ok(Some(m)) => Some(m),
            _ => match win.primary_monitor() {
                Ok(Some(m)) => Some(m),
                _ => {
                    eprintln!("[meeting] no monitor to park the overlay on");
                    None
                }
            },
        }
    }

    /// Park the overlay at the top-right of the screen the cursor is on (offer / recap).
    ///
    /// Computed and set entirely in **physical** pixels — see `park_bottom_center`.
    fn park_top_right(win: &tauri::WebviewWindow, size: (f64, f64)) {
        let Some(monitor) = overlay_monitor(win) else { return };
        let scale = monitor.scale_factor();
        let screen = monitor.size();
        let origin = monitor.position();
        let w = (size.0 * scale).round() as i32;
        let margin = (MARGIN * scale).round() as i32;
        // Below the menu bar, so the overlay never fights the notch for the same pixels.
        let top = ((MARGIN + MENUBAR_H) * scale).round() as i32;

        let x = origin.x + screen.width as i32 - w - margin;
        let y = origin.y + top;
        eprintln!(
            "[meeting] park top-right ({x},{y}) physical — screen {}x{} at ({},{}) scale {scale}",
            screen.width, screen.height, origin.x, origin.y
        );
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    }

    /// Park the overlay bottom-center on the screen the cursor is on — above the Meet mic bar.
    ///
    /// Re-parked when the window size changes (pill ↔ live panel) so the anchor stays centered.
    /// After that the user may drag it; we do not move it until hide, resize, or offer → live.
    ///
    /// Computed and set entirely in **physical** pixels. Mixing the two coordinate systems is
    /// how the panel ended up in the middle of the screen: the monitor answers in physical
    /// pixels, the window size is given in logical ones, and subtracting one from the other on a
    /// Retina display is off by exactly the scale factor.
    fn park_bottom_center(win: &tauri::WebviewWindow, size: (f64, f64)) {
        let Some(monitor) = overlay_monitor(win) else { return };
        let scale = monitor.scale_factor();
        let screen = monitor.size();
        let origin = monitor.position();
        let w = (size.0 * scale).round() as i32;
        let h = (size.1 * scale).round() as i32;
        let bottom = (BOTTOM_MARGIN * scale).round() as i32;

        let x = origin.x + (screen.width as i32 - w) / 2;
        let y = origin.y + screen.height as i32 - h - bottom;
        eprintln!(
            "[meeting] park bottom-center ({x},{y}) physical — screen {}x{} at ({},{}) scale {scale}",
            screen.width, screen.height, origin.x, origin.y
        );
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    }

    fn park_overlay(win: &tauri::WebviewWindow, mode: ParkMode, size: (f64, f64)) {
        match mode {
            ParkMode::TopRight => park_top_right(win, size),
            ParkMode::BottomCenter => park_bottom_center(win, size),
        }
    }

    /// Logical size for the in-meeting overlay: pill / captions / AI Canvas / chat / stacks.
    /// Honors a user corner-resize while any panel is open.
    fn recording_overlay_size() -> (f64, f64) {
        use std::sync::atomic::Ordering;
        let panel = OVERLAY_PANEL_OPEN.load(Ordering::SeqCst);
        let canvas = OVERLAY_CANVAS_OPEN.load(Ordering::SeqCst);
        let chat = OVERLAY_CHAT_OPEN.load(Ordering::SeqCst);
        if panel || canvas || chat {
            if let Ok(guard) = OVERLAY_CUSTOM_SIZE.lock() {
                if let Some(custom) = *guard {
                    return custom;
                }
            }
        }
        let open = u8::from(panel) + u8::from(canvas) + u8::from(chat);
        if open >= 2 {
            return BOTH_SIZE;
        }
        if canvas {
            return CANVAS_SIZE;
        }
        if chat {
            return CHAT_SIZE;
        }
        if panel {
            return LIVE_SIZE;
        }
        PILL_SIZE
    }

    fn clear_custom_size_if_idle() {
        use std::sync::atomic::Ordering;
        if OVERLAY_PANEL_OPEN.load(Ordering::SeqCst)
            || OVERLAY_CANVAS_OPEN.load(Ordering::SeqCst)
            || OVERLAY_CHAT_OPEN.load(Ordering::SeqCst)
        {
            return;
        }
        if let Ok(mut g) = OVERLAY_CUSTOM_SIZE.lock() {
            *g = None;
        }
    }

    fn clamp_overlay_size(width: f64, height: f64) -> (f64, f64) {
        (
            width.clamp(OVERLAY_SIZE_MIN.0, OVERLAY_SIZE_MAX.0).round(),
            height.clamp(OVERLAY_SIZE_MIN.1, OVERLAY_SIZE_MAX.1).round(),
        )
    }

    fn sync_recording_overlay(app: &tauri::AppHandle) {
        let Ok(g) = LANE.lock() else { return };
        let Some(lane) = g.as_ref() else { return };
        if lane.machine.state() == State::Recording && !lane.overlay_dismissed {
            sync_window(app, State::Recording, lane.settings.enabled, false);
        }
    }

    /// Show, hide and resize the overlay to match the lane's state.
    fn sync_window(app: &tauri::AppHandle, state: State, enabled: bool, overlay_dismissed: bool) {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            sync_window_main(&handle, state, enabled, overlay_dismissed);
        });
    }

    fn sync_window_main(app: &tauri::AppHandle, state: State, enabled: bool, overlay_dismissed: bool) {
        // `PARKED` records only whether the overlay has been placed yet — the user may drag it
        // afterwards and it must not jump back. Showing is attempted on *every* tick it should
        // be visible: `show()` is idempotent, and treating "we showed it once" as "it is on
        // screen" is what left an invisible window in the one state that has to be seen.
        static PARKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        static LAST_PARK_MODE: std::sync::Mutex<Option<ParkMode>> = std::sync::Mutex::new(None);
        use std::sync::atomic::Ordering;

        let visible = enabled && !matches!(state, State::Idle)
            && !(state == State::Recording && overlay_dismissed);
        let size = match state {
            State::Wrapping => RECAP_SIZE,
            State::Recording => recording_overlay_size(),
            State::Offered => OFFER_SIZE,
            State::Idle => BAR_SIZE,
        };
        // Skip redundant AppKit work: emit() runs every second while a meeting is active, but the
        // overlay only needs to change on visibility/state/size transitions. Hammering set_size /
        // orderFront every tick races teardown and can destabilize the webview.
        static LAST: std::sync::Mutex<Option<(bool, State, bool, f64, f64)>> =
            std::sync::Mutex::new(None);
        let prev_size = LAST
            .lock()
            .ok()
            .and_then(|l| l.as_ref().map(|(_, _, _, w, h)| (*w, *h)));
        let size_changed = prev_size
            .map(|(w, h)| w != size.0 || h != size.1)
            .unwrap_or(true);
        if let Ok(mut last) = LAST.lock() {
            if last.as_ref().is_some_and(|(v, s, dismissed, w, h)| {
                *v == visible
                    && *dismissed == overlay_dismissed
                    && (!visible || (*s == state && *w == size.0 && *h == size.1))
            }) {
                return;
            }
            *last = Some((visible, state, overlay_dismissed, size.0, size.1));
        }
        // Never builds: the window exists from launch (see `build_overlay`). If it is missing,
        // something failed at setup and the right answer is to do nothing rather than to try
        // creating an AppKit window from this thread.
        let Some(win) = app.get_webview_window(WINDOW_LABEL) else { return };
        if !visible {
            OVERLAY_WANTS_INTERACTIVE.store(false, Ordering::SeqCst);
            set_overlay_ignores_mouse(&win, true);
            let _ = win.hide();
            PARKED.store(false, Ordering::SeqCst);
            if let Ok(mut last_mode) = LAST_PARK_MODE.lock() {
                *last_mode = None;
            }
            return;
        }
        let park_mode = park_mode_for_state(state);
        let park_mode_changed = LAST_PARK_MODE
            .lock()
            .ok()
            .and_then(|m| m.as_ref().map(|prev| *prev != park_mode))
            .unwrap_or(true);
        // Corner-resize must keep the user's position — do not re-park on size-only changes.
        let custom_overlay_size = state == State::Recording
            && (OVERLAY_PANEL_OPEN.load(Ordering::SeqCst)
                || OVERLAY_CANVAS_OPEN.load(Ordering::SeqCst)
                || OVERLAY_CHAT_OPEN.load(Ordering::SeqCst))
            && OVERLAY_CUSTOM_SIZE
                .lock()
                .ok()
                .and_then(|g| *g)
                .is_some();
        let _ = win.set_size(tauri::LogicalSize::new(size.0, size.1));
        if !PARKED.load(Ordering::SeqCst)
            || park_mode_changed
            || (size_changed && !custom_overlay_size)
        {
            park_overlay(&win, park_mode, size);
            PARKED.store(true, Ordering::SeqCst);
            if let Ok(mut last_mode) = LAST_PARK_MODE.lock() {
                *last_mode = Some(park_mode);
            }
        } else if size_changed {
            PARKED.store(true, Ordering::SeqCst);
        }
        // Whole window captures clicks while any meeting surface is visible. Transparent padding
        // around `.ov` may block clicks too — better than pointermove hit-tests that flip false
        // before the first move or on pointerleave and let clicks fall through to Meet behind.
        // Product trade-off (2026-08): offer/recap park top-right; live pill parks bottom-center.
        // Transparent padding may still block controls underneath. Pointermove hit-testing was
        // tried (752612f) and reverted (0743193). Revisit with CGEventTap cursor tracking if needed.
        OVERLAY_WANTS_INTERACTIVE.store(true, Ordering::SeqCst);
        apply_overlay_interactive(&win);
        let shown = win.show();
        let _ = win.set_always_on_top(true);
        // Accessory apps do not auto-show windows — order front only when the lane needs UI.
        if let Some(ptr) = overlay_ns_window(&win) {
            use objc2::msg_send;
            // SAFETY: live NSWindow on the main thread.
            unsafe {
                let _: () = msg_send![ptr, orderFrontRegardless];
            }
        }
        let _ = app.emit("meeting_overlay_surface", ());
        eprintln!(
            "[meeting] overlay show ok={} pos={:?} size={:?} interactive={}",
            shown.is_ok(),
            win.outer_position().ok(),
            (size.0, size.1),
            OVERLAY_WANTS_INTERACTIVE.load(Ordering::SeqCst),
        );
    }

    /// Dismiss the Recap and return the lane to Idle.
    #[tauri::command]
    pub fn meeting_wrapped(app: tauri::AppHandle) {
        let wrapping = LANE
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|l| l.machine.state() == State::Wrapping))
            .unwrap_or(false);
        if !wrapping {
            return;
        }
        step(&app, Input::Wrapped);
    }

    /// Let the user move the overlay (Issue #7: draggable).
    ///
    /// `start_dragging` alone is unreliable on borderless WKWebView windows — hand the in-flight
    /// mouse event to AppKit like the notch panel's `start_panel_drag`.
    #[tauri::command]
    pub fn meeting_drag(app: tauri::AppHandle) {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            use objc2::runtime::AnyObject;
            use objc2::{class, msg_send};
            let Some(win) = handle.get_webview_window(WINDOW_LABEL) else { return };
            let Some(ptr) = overlay_ns_window(&win) else { return };
            // SAFETY: main thread; standard AppKit calls on a live NSWindow.
            unsafe {
                let ns_app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                if !ns_app.is_null() {
                    let ev: *mut AnyObject = msg_send![ns_app, currentEvent];
                    if !ev.is_null() {
                        let _: () = msg_send![ptr, performWindowDragWithEvent: ev];
                        return;
                    }
                }
            }
            let _ = win.start_dragging();
        });
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
                paused: false,
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
        let recording = LANE
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|l| l.machine.state() == State::Recording))
            .unwrap_or(false);
        if !recording {
            return;
        }
        step(&app, Input::Stop);
    }

    /// Toggle capture/ASR pause while keeping the meeting interval open.
    /// Pause stops feeding ASR (tears down the audio lane in RAM only — no waveform to disk).
    /// Resume restarts the lane against the same session. Stop still ends the meeting.
    ///
    /// Emit + unlock first; heavy audio start/stop runs after so the webview morph is not
    /// gated on device teardown / Deepgram reconnect.
    #[tauri::command]
    pub fn meeting_toggle_pause(app: tauri::AppHandle) {
        let now = now_ms();
        enum After {
            Stop(Option<crate::audio_lane::Handle>),
            Start {
                id: i64,
                live: Arc<RwLock<Settings>>,
            },
        }
        let after = {
            let Ok(mut g) = LANE.lock() else { return };
            let Some(lane) = g.as_mut() else { return };
            if lane.machine.state() != State::Recording {
                return;
            }
            if lane.paused {
                lane.paused = false;
                let start = lane.session_id.map(|id| {
                    set_live_emit_session(id);
                    if let Ok(mut live) = lane.live_settings.write() {
                        *live = lane.settings.clone();
                    }
                    After::Start {
                        id,
                        live: lane.live_settings.clone(),
                    }
                });
                eprintln!("[meeting] capture resumed (session held)");
                emit(&app, lane, now);
                start
            } else {
                lane.paused = true;
                set_live_emit_session(0);
                let handle = lane.audio.take();
                eprintln!("[meeting] capture paused (session held)");
                emit(&app, lane, now);
                Some(After::Stop(handle))
            }
        };
        match after {
            Some(After::Start { id, live }) => {
                // Resume: open devices off the command thread so invoke returns after emit.
                let app2 = app.clone();
                std::thread::spawn(move || {
                    let handle = crate::audio_lane::start(&app2, id, live);
                    if let Ok(mut g) = LANE.lock() {
                        if let Some(lane) = g.as_mut() {
                            if lane.machine.state() == State::Recording && !lane.paused {
                                lane.audio = handle;
                                return;
                            }
                        }
                    }
                    finish_audio_stop(handle);
                });
            }
            Some(After::Stop(handle)) => {
                // Pause: join audio on a worker so the command returns immediately after emit.
                std::thread::spawn(move || finish_audio_stop(handle));
            }
            None => {}
        }
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

    /// Whether the Select KK credential (`com.selectkk.shogun` / `select-kk-batch`) is in Keychain.
    /// overlay uses this so "needs key" is shown only when Rust confirms absence, not on a timeout.
    #[tauri::command]
    pub fn meeting_select_kk_configured() -> bool {
        crate::meeting_recap::select_kk_configured()
    }

    /// Deepgram API key presence for meeting live STT. Secrets are never returned in full.
    #[derive(Serialize)]
    pub struct DeepgramKeyStatus {
        pub has_key: bool,
        pub key_last4: String,
    }

    #[tauri::command]
    pub fn get_deepgram_key_status() -> DeepgramKeyStatus {
        match shogun_integrations::keychain_store::get_deepgram_asr_key() {
            Some(k) if !k.trim().is_empty() => {
                let k = k.trim();
                let n = k.chars().count();
                let last4 = if n >= 4 {
                    k.chars().skip(n - 4).collect()
                } else {
                    "····".to_string()
                };
                DeepgramKeyStatus { has_key: true, key_last4: last4 }
            }
            _ => DeepgramKeyStatus { has_key: false, key_last4: String::new() },
        }
    }

    /// Save the Deepgram API key to Keychain (meeting live STT). The key value is never logged.
    #[tauri::command]
    pub fn set_deepgram_key(key: String) -> Result<(), String> {
        shogun_integrations::keychain_store::set_deepgram_asr_key(&key)?;
        eprintln!("[meeting] deepgram api key saved to Keychain");
        Ok(())
    }

    /// Remove the Deepgram API key from Keychain.
    #[tauri::command]
    pub fn clear_deepgram_key() -> Result<(), String> {
        match shogun_integrations::keychain_store::delete_generic_secret(
            shogun_integrations::keychain_store::DEEPGRAM_ASR_ACCOUNT,
        ) {
            Ok(()) => {}
            Err(e) if e.code() == -25300 /* errSecItemNotFound */ => {}
            Err(e) => return Err(e.to_string()),
        }
        eprintln!("[meeting] deepgram api key removed");
        Ok(())
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

    /// One suggested next action, as shown in the Recap card. `owner` is who the model thought
    /// should do it, when the transcript made that clear (never invented). L1/L3 discipline: this
    /// is a *suggestion* the panel displays, never something the app will do (invariant 4) — the
    /// card carries no "send"/"do it" affordance.
    #[derive(Serialize)]
    pub struct NextActionView {
        text: String,
        owner: Option<String>,
    }

    /// The model-generated minutes for the last finished meeting, shaped for the webview.
    ///
    /// The two structured columns are stored as JSON strings; we deserialize each here and, on a
    /// parse error, fall back to an empty list rather than failing the whole read (a malformed
    /// column must not blank the card — the degraded Recap is still shown underneath).
    #[derive(Serialize)]
    pub struct MinutesView {
        summary: String,
        decisions: Vec<String>,
        next_actions: Vec<NextActionView>,
    }

    /// The model-generated minutes for the most recently finished meeting (MT4, FR-MT-19), or
    /// `None` if the Batch lane has not produced them yet.
    ///
    /// This is layered on top of [`meeting_recap`], not a replacement: the degraded Recap shows the
    /// moment the interval closes, and these minutes arrive later (the panel refetches on the
    /// `meeting_recap` event). Reads the same `last_session_id` as [`meeting_recap`].
    #[tauri::command]
    pub fn meeting_recap_minutes(app: tauri::AppHandle) -> Option<MinutesView> {
        let id = LANE.lock().ok().and_then(|g| g.as_ref().and_then(|l| l.last_session_id))?;
        let stored = db(&app).and_then(|db| db.meeting_recap_full(id))?;
        let decisions: Vec<String> =
            serde_json::from_str(&stored.decisions_json).unwrap_or_default();
        let next_actions: Vec<shogun_core::meeting::minutes::NextAction> =
            serde_json::from_str(&stored.next_actions_json).unwrap_or_default();
        Some(MinutesView {
            summary: stored.summary,
            decisions,
            next_actions: next_actions
                .into_iter()
                .map(|a| NextActionView { text: a.text, owner: a.owner })
                .collect(),
        })
    }

    /// One transcribed line for the post-meeting viewer (FR-MT-10). Shown only after Stop, never
    /// during recording.
    #[derive(Serialize)]
    pub struct TranscriptLineView {
        ts: i64,
        speaker: Option<String>,
        text: String,
    }

    /// Whisper marks silence as `[BLANK_AUDIO]`; the Recap viewer hides these but must not claim
    /// "no transcript" when they are the only rows stored.
    fn is_blank_transcript_marker(text: &str) -> bool {
        text.trim().eq_ignore_ascii_case("[BLANK_AUDIO]")
    }

    /// Displayable transcript lines plus a flag when only blank-audio markers were stored.
    #[derive(Serialize)]
    pub struct MeetingTranscriptView {
        lines: Vec<TranscriptLineView>,
        only_blanks: bool,
    }

    /// The session transcript for the most recently finished meeting, or for `session_id` when
    /// provided. Blank-audio markers are filtered out; `only_blanks` is set when that was all
    /// that was stored.
    #[tauri::command]
    pub fn get_meeting_transcript(
        app: tauri::AppHandle,
        session_id: Option<i64>,
    ) -> MeetingTranscriptView {
        let id = session_id.or_else(|| {
            LANE.lock()
                .ok()
                .and_then(|g| g.as_ref().and_then(|l| l.last_session_id))
        });
        let Some(id) = id else {
            return MeetingTranscriptView { lines: Vec::new(), only_blanks: false };
        };
        let Some(db) = db(&app) else {
            return MeetingTranscriptView { lines: Vec::new(), only_blanks: false };
        };
        let stored = db.meeting_transcript(id);
        let mut displayable = Vec::new();
        let mut saw_blank = false;
        for (ts, speaker, text) in stored {
            if text.trim().is_empty() {
                continue;
            }
            if is_blank_transcript_marker(&text) {
                saw_blank = true;
                continue;
            }
            displayable.push(TranscriptLineView { ts, speaker, text });
        }
        let only_blanks = displayable.is_empty() && saw_blank;
        eprintln!(
            "[meeting] get_meeting_transcript session={id}: {} displayable (only_blanks={only_blanks})",
            displayable.len()
        );
        MeetingTranscriptView { lines: displayable, only_blanks }
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
        lane.settings = candidate.clone();
        if let Ok(mut live) = lane.live_settings.write() {
            *live = candidate;
        }
        if !enabled {
            let effects = lane.machine.step(Input::FeatureDisabled);
            let stop_audio = apply(&app, lane, &effects, now);
            drop(g);
            finish_audio_stop(stop_audio);
        } else {
            emit(&app, lane, now);
        }
        eprintln!("[meeting] notes → {}", if enabled { "enabled" } else { "off" });
        Ok(())
    }

    /// Mic-only detection opt-in (FR-MT-04). Ships off: sustained mic alone never offers unless
    /// the user enables this in settings.
    #[tauri::command]
    pub fn set_meeting_allow_mic_only(allow: bool, app: tauri::AppHandle) -> Result<(), String> {
        let candidate = {
            let Ok(g) = LANE.lock() else { return Err("busy".into()) };
            let Some(lane) = g.as_ref() else { return Err("not ready".into()) };
            Settings { allow_mic_only_detect: allow, ..lane.settings.clone() }
        };
        save(&app, &candidate)?;

        let Ok(mut g) = LANE.lock() else { return Err("busy".into()) };
        let Some(lane) = g.as_mut() else { return Err("not ready".into()) };
        lane.settings = candidate.clone();
        if let Ok(mut live) = lane.live_settings.write() {
            *live = candidate;
        }
        eprintln!("[meeting] mic-only detect → {}", if allow { "on" } else { "off" });
        Ok(())
    }

    /// In-meeting overlay mode (issue #93). Applies to new lines when changed mid-recording.
    #[tauri::command]
    pub fn set_meeting_mode(mode: MeetingMode, app: tauri::AppHandle) -> Result<(), String> {
        let now = now_ms();
        let candidate = {
            let Ok(g) = LANE.lock() else { return Err("busy".into()) };
            let Some(lane) = g.as_ref() else { return Err("not ready".into()) };
            Settings { meeting_mode: mode, ..lane.settings.clone() }
        };
        save(&app, &candidate)?;

        let Ok(mut g) = LANE.lock() else { return Err("busy".into()) };
        let Some(lane) = g.as_mut() else { return Err("not ready".into()) };
        lane.settings = candidate.clone();
        if let Ok(mut live) = lane.live_settings.write() {
            *live = candidate;
        }
        emit(&app, lane, now);
        Ok(())
    }

    /// Language pair for one-way / two-way translation modes.
    #[tauri::command]
    pub fn set_meeting_langs(
        source_lang: Option<MeetingLanguage>,
        target_lang: Option<MeetingLanguage>,
        my_lang: Option<MeetingLanguage>,
        other_lang: Option<MeetingLanguage>,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        let now = now_ms();
        let candidate = {
            let Ok(g) = LANE.lock() else { return Err("busy".into()) };
            let Some(lane) = g.as_ref() else { return Err("not ready".into()) };
            Settings {
                source_lang: source_lang.unwrap_or(lane.settings.source_lang),
                target_lang: target_lang.unwrap_or(lane.settings.target_lang),
                my_lang: my_lang.unwrap_or(lane.settings.my_lang),
                other_lang: other_lang.unwrap_or(lane.settings.other_lang),
                ..lane.settings.clone()
            }
        };
        save(&app, &candidate)?;

        let Ok(mut g) = LANE.lock() else { return Err("busy".into()) };
        let Some(lane) = g.as_mut() else { return Err("not ready".into()) };
        lane.settings = candidate.clone();
        if let Ok(mut live) = lane.live_settings.write() {
            *live = candidate;
        }
        emit(&app, lane, now);
        Ok(())
    }

    /// Hide the live overlay during recording; notes and ASR continue.
    #[tauri::command]
    pub fn meeting_overlay_dismiss(app: tauri::AppHandle) {
        let now = now_ms();
        let Ok(mut g) = LANE.lock() else { return };
        let Some(lane) = g.as_mut() else { return };
        if lane.machine.state() == State::Recording {
            lane.overlay_dismissed = true;
            emit(&app, lane, now);
        }
    }

    /// Expand/collapse the live captions panel above the black control pill.
    #[tauri::command]
    pub fn meeting_set_overlay_panel(app: tauri::AppHandle, open: bool) {
        use std::sync::atomic::Ordering;
        OVERLAY_PANEL_OPEN.store(open, Ordering::SeqCst);
        clear_custom_size_if_idle();
        sync_recording_overlay(&app);
    }

    /// Expand/collapse the AI Canvas panel (Notes / document pill).
    #[tauri::command]
    pub fn meeting_set_overlay_canvas(app: tauri::AppHandle, open: bool) {
        use std::sync::atomic::Ordering;
        OVERLAY_CANVAS_OPEN.store(open, Ordering::SeqCst);
        clear_custom_size_if_idle();
        sync_recording_overlay(&app);
    }

    /// Expand/collapse the AI Chat panel.
    #[tauri::command]
    pub fn meeting_set_overlay_chat(app: tauri::AppHandle, open: bool) {
        use std::sync::atomic::Ordering;
        OVERLAY_CHAT_OPEN.store(open, Ordering::SeqCst);
        clear_custom_size_if_idle();
        sync_recording_overlay(&app);
    }

    /// Live corner-resize of the meeting overlay (captions / AI Canvas / chat).
    /// Keeps the top-left corner fixed (grow/shrink down and right).
    #[tauri::command]
    pub fn meeting_set_overlay_size(app: tauri::AppHandle, width: f64, height: f64) {
        use std::sync::atomic::Ordering;
        if !OVERLAY_PANEL_OPEN.load(Ordering::SeqCst)
            && !OVERLAY_CANVAS_OPEN.load(Ordering::SeqCst)
            && !OVERLAY_CHAT_OPEN.load(Ordering::SeqCst)
        {
            return;
        }
        let size = clamp_overlay_size(width, height);
        if let Ok(mut g) = OVERLAY_CUSTOM_SIZE.lock() {
            *g = Some(size);
        }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let Some(win) = handle.get_webview_window(WINDOW_LABEL) else { return };
            let prev = win.outer_position().ok();
            let _ = win.set_size(tauri::LogicalSize::new(size.0, size.1));
            if let Some(p) = prev {
                let _ = win.set_position(p);
            }
        });
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
        let effects = lane.machine.step(input);
        let stop_audio = apply(&app, lane, &effects, now);
        drop(g);
        finish_audio_stop(stop_audio);
        Ok(())
    }
}
