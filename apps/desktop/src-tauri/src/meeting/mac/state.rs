//! State ownership and effect application for the desktop meeting lane.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use shogun_core::meeting::detect::MicWatch;
use shogun_core::meeting::gate::OfferGate;
use shogun_core::meeting::settings::Settings;
use shogun_core::meeting::statemachine::{Effect, EndReason, Input, Machine, Params, State};
use tauri::{Emitter, Manager};

use super::overlay::sync_window;

/// Settings + machine, behind one lock. They are always read together (an offer needs both
/// "is this allowed?" and "what state am I in?"), so splitting the locks would only create
/// the chance to read a half-updated pair.
pub(super) struct Lane {
    pub(super) settings: Settings,
    pub(super) machine: Machine,
    /// The interval currently open, and what to title it.
    pub(super) session_id: Option<i64>,
    /// The interval that just finished — what Recap reads. Kept separately from
    /// `session_id`, which is cleared the moment the interval closes.
    pub(super) last_session_id: Option<i64>,
    pub(super) title: Option<String>,
    pub(super) app_bundle_id: Option<String>,
    /// Carried from the detector so the stored interval records what was actually observed,
    /// rather than a constant that would claim more (or less) than the evidence (FR-MT-04).
    pub(super) confidence: f64,
    pub(super) provenance: String,
    /// Epoch ms of the transition into the current state — the pill's clock.
    pub(super) since_ms: i64,
    /// What the user has already declined, and until when (FR-MT-02c). Deliberately not in
    /// `settings`: a decline changes no settings and must not outlive the process.
    pub(super) gate: OfferGate,
    /// Turns "the microphone is open" into "a call is happening" (FR-MT-04 signal ②).
    pub(super) mic: MicWatch,
    /// The running audio lane (MT3), when one is capturing. `None` while idle, or when audio
    /// degraded to notes-only. Held here so `StopAudio` can tear the exact same lane down.
    pub(super) audio: Option<crate::audio_lane::Handle>,
    /// Set when the offer that opened this interval saw a Meet URL (FR-MT-11 tab/window end).
    pub(super) opened_via_meet_url: bool,
    /// When the session browser's frontmost tab first stopped looking like a meeting.
    pub(super) url_lost_since_ms: Option<i64>,
    /// Set when the offer that opened this interval was a Slack huddle hint (Plan A-4).
    pub(super) opened_via_huddle: bool,
    /// When Slack's title/AX text first stopped looking like a huddle — the huddle mirror of
    /// `url_lost_since_ms` (Plan A-4).
    pub(super) huddle_hint_lost_since_ms: Option<i64>,
    /// When the system mic last transitioned to closed — debounces hang-up flicker (FR-MT-11).
    pub(super) mic_closed_since_ms: Option<i64>,
    /// Shared with the audio lane — mode/lang changes apply to new lines mid-meeting.
    pub(super) live_settings: Arc<RwLock<Settings>>,
    /// User dismissed the live overlay during recording; recording continues.
    pub(super) overlay_dismissed: bool,
    /// Why the last interval closed — drives shorter Recap auto-dismiss after auto-end.
    pub(super) last_end_reason: Option<EndReason>,
    /// Capture/ASR paused while the meeting interval stays open (waveform toggle).
    /// Not a machine state — Stop still ends the session; pause only holds the mic/ASR lane.
    pub(super) paused: bool,
    /// Visible reason the meeting fell back to typed notes. Never hide a rejected microphone.
    pub(super) audio_error: Option<String>,
}

impl Lane {
    pub(super) fn new() -> Self {
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
            opened_via_huddle: false,
            huddle_hint_lost_since_ms: None,
            mic_closed_since_ms: None,
            live_settings: Arc::new(RwLock::new(settings)),
            overlay_dismissed: false,
            last_end_reason: None,
            paused: false,
            audio_error: None,
        }
    }
}

pub(super) static LANE: Mutex<Option<Lane>> = Mutex::new(None);
/// Session id allowed to push `meeting_live_line` to the webview. Cleared before audio stop
/// so late whisper flushes after Stop do not repaint a hidden overlay.
pub(super) static LIVE_EMIT_SESSION: AtomicI64 = AtomicI64::new(0);

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

pub(super) fn set_live_emit_session(session_id: i64) {
    LIVE_EMIT_SESSION.store(session_id, Ordering::Release);
}

pub(super) fn now_ms() -> i64 {
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
    /// Capture failure shown by meeting UI while typed notes remain available.
    pub audio_error: Option<String>,
}

pub(super) fn view(lane: &Lane, now: i64) -> MeetingView {
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
        audio_error: lane.audio_error.clone(),
    }
}

pub(super) fn apply(
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
                    lane.audio_error = None;
                }
                if *state == State::Offered {
                    // The only UI that starts recording if it is ignored, so it is worth a
                    // sound (#49). In practice the mic is usually already hot by now and the
                    // hot-mic rule keeps this silent — the pill stays the primary channel.
                    crate::sound::mac::play(shogun_core::sound::Cue::MeetingOffered);
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
                    lane.opened_via_huddle = false;
                    lane.huddle_hint_lost_since_ms = None;
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
                    // MCP/CLI/REST writes share `meeting.json`. Refresh only this next-session
                    // choice; an API client must not silently toggle the feature mid-meeting.
                    lane.settings.microphone =
                        shogun_core::meeting::settings_store::load().microphone;
                    lane.overlay_dismissed = false;
                    lane.paused = false;
                    lane.audio_error = None;
                    set_live_emit_session(id);
                    if let Ok(mut live) = lane.live_settings.write() {
                        *live = lane.settings.clone();
                    }
                    match crate::audio_lane::start(app, id, lane.live_settings.clone()) {
                        Ok(handle) => lane.audio = Some(handle),
                        Err(error) => {
                            eprintln!("[meeting] {error}; typed notes only");
                            lane.audio_error = Some(error);
                        }
                    }
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

pub(super) fn finish_audio_stop(handle: Option<crate::audio_lane::Handle>) {
    crate::audio_lane::stop(handle);
}

pub(super) fn emit(app: &tauri::AppHandle, lane: &Lane, now: i64) {
    let v = view(lane, now);
    sync_window(
        app,
        lane.machine.state(),
        lane.settings.enabled,
        lane.overlay_dismissed,
    );
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
pub(super) fn db(app: &tauri::AppHandle) -> Option<tauri::State<'_, shogun_core::daemon::Db>> {
    app.try_state::<shogun_core::daemon::Db>()
}

pub(super) fn open_session(app: &tauri::AppHandle, lane: &Lane) -> Option<i64> {
    let db = db(app)?;
    db.open_meeting(
        lane.title.as_deref(),
        lane.app_bundle_id.as_deref(),
        lane.confidence,
        &lane.provenance,
    )
    .inspect(|id| eprintln!("[meeting] session {id} opened"))
}

pub(super) fn close_session(app: &tauri::AppHandle, id: i64) -> bool {
    db(app).is_some_and(|db| db.close_meeting(id))
}

pub(super) fn step(app: &tauri::AppHandle, input: Input) {
    let now = now_ms();
    let stop_audio = {
        let Ok(mut g) = LANE.lock() else { return };
        let Some(lane) = g.as_mut() else { return };
        // A "no" must outlive the state transition. Without recording the decline, the machine
        // returns to Idle, the meeting app is still frontmost, and the next 1s tick re-offers —
        // ten seconds later the recording the user just refused starts by itself. Stop counts as
        // the same "no": the meeting is still going, and stopping it is declining the rest of it
        // (FR-MT-02c).
        if matches!(input, Input::NotNow | Input::Stop) {
            if let Some(bid) = lane.app_bundle_id.clone() {
                lane.gate.decline(&bid, now);
            }
        }
        let effects = lane.machine.step(input);
        if effects.is_empty() {
            return;
        }
        apply(app, lane, &effects, now)
    };
    finish_audio_stop(stop_audio);
}
