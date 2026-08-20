//! Settings persistence and meeting detection driver.

use shogun_core::meeting::detect::{self, Decision, LiveSignals, Signals};
use shogun_core::meeting::settings::{OfferContext, Settings};
use shogun_core::meeting::statemachine::{EndReason, Input, Machine, Params, State};
use tauri::Manager;

use super::overlay::build_overlay;
use super::state::{apply, emit, finish_audio_stop, now_ms, step, Lane, LANE};

fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("meeting.json"))
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
        // Boot cutoff: only rows from BEFORE this run are abandoned (a later call must never
        // catch a meeting this run just opened).
        let closed = db.close_abandoned_meetings(now_ms());
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
        if lane.settings.enabled {
            "enabled"
        } else {
            "off (default)"
        }
    );
    if let Ok(mut g) = LANE.lock() {
        *g = Some(lane);
    }
}

pub(super) fn save(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
    let Some(p) = settings_path(app) else {
        return Ok(());
    };
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

fn slack_huddle_hint(bundle_id: &str, window_title: Option<&str>) -> bool {
    if bundle_id != detect::SLACK_BUNDLE_ID {
        return false;
    }
    let snippets = crate::capture_source::latest_slack_ax_snippets();
    let refs: Vec<&str> = snippets.iter().map(String::as_str).collect();
    detect::huddle_hint(window_title, &refs)
}

/// Called on every focus change with the frontmost app.
///
/// Returns immediately when the feature is off — the detector does not run, so nothing
/// observes a meeting while meeting notes are disabled (FR-MT-02a).
pub fn on_focus(
    app: &tauri::AppHandle,
    bundle_id: &str,
    process_name: Option<&str>,
    window_title: Option<&str>,
    page_url: Option<&str>,
    mic_open: bool,
) {
    let now = now_ms();
    let Ok(mut g) = LANE.lock() else { return };
    let Some(lane) = g.as_mut() else { return };

    // Computed before the mic observation because the watch needs to know whether the open
    // device has an explanation in front of it — see `MicObservation::meeting_context`.
    let url_hint = page_url.and_then(detect::host_hint);
    // Bundle ids are authoritative. A missing/unknown id may fall back to the app process
    // name or a product-specific title, but those are Weak only and still need the mic.
    // Neither fallback treats arbitrary "meeting" text as evidence (detect.rs tests lock it).
    let surface_hint = detect::bundle_hint(bundle_id)
        .or_else(|| process_name.and_then(detect::process_hint))
        .or_else(|| window_title.and_then(detect::title_hint));
    let has_meet_url = url_hint == Some(detect::MeetingHint::Strong);
    let has_strong_bundle = surface_hint == Some(detect::MeetingHint::Strong);
    // Weak surfaces (Teams, Webex — Plan A-2): one corroborating vote in the detector, never
    // an opener, so they ride in the ctx rather than in `meeting_app_frontmost`.
    let has_weak_meeting_signal = surface_hint == Some(detect::MeetingHint::Weak)
        || url_hint == Some(detect::MeetingHint::Weak);
    // Slack huddles (Plan A-4): same bundle id as ordinary Slack, so the hint reads the
    // window title plus the capture poller's most recent AX text for Slack.
    //
    // Guarded by `enabled` because these hints moved above the feature gate, and the AX
    // snippets are captured user content: FR-MT-02a says nothing observes a meeting while
    // meeting notes are off, and reading them here would break that. `&&` short-circuits, so
    // a disabled build never touches the snippets at all. The rest of `on_focus` only uses
    // this after the same gate, so nothing downstream changes.
    let has_huddle_hint = lane.settings.enabled && slack_huddle_hint(bundle_id, window_title);
    let meeting_context =
        has_strong_bundle || has_meet_url || has_weak_meeting_signal || has_huddle_hint;

    // Fed every tick, including while a meeting is already running: the watch measures a
    // continuous stretch, so skipping observations would make it forget the call is ongoing.
    //
    // `SystemWide` because `mic::input_in_use` reports the device, not its holder. That is
    // permanently true on a machine where any utility keeps an input open, so the watch is
    // given the frontmost app and the meeting context and decides for itself whether the
    // signal still describes this user (observed on-device 2026-07-31: a held input meant an
    // offer in Finder and in the login window). Attribution belongs in `mic.rs`; when it
    // lands, this becomes `MicSource::Holder` and the behavioural check stops mattering.
    lane.mic.observe(
        &detect::MicObservation {
            in_use: mic_open,
            source: detect::MicSource::SystemWide,
            frontmost_bundle_id: bundle_id,
            meeting_context,
        },
        now,
    );
    let mic_sustained_ms = lane.mic.sustained_ms(now);

    if !lane.settings.enabled {
        return;
    }
    // A sustained switch away ends the cooldown on the app left behind: coming back later is
    // a new meeting and deserves to be asked about again (a momentary flick clears nothing).
    lane.gate.observe_front(bundle_id, now);
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
    // beyond what was actually observed. (The hints themselves are computed above, because
    // the mic watch needs `meeting_context` before it can judge the observation.)
    let on_media_page = page_url.is_some_and(detect::is_media_url);
    let page_host = page_url.and_then(detect::host_from_url);
    let signals = Signals {
        // Sustained mic: suppressed on media pages unless a meeting URL/app is already in
        // front; mic-only elsewhere needs 30s, not 10s (FR-MT-04 — ② alone is weak).
        mic_in_use: detect::mic_counts_as_signal(mic_sustained_ms, meeting_context, on_media_page),
        // Corroboration: a Strong meeting app, or a browser on a Strong meeting page. Either
        // can still open an interval alone, so a call whose audio does not run through the
        // default input device is not invisible.
        meeting_app_frontmost: has_strong_bundle || has_meet_url,
        ..Default::default()
    };
    let ctx = detect::DetectionCtx {
        is_browser: is_browser(bundle_id),
        page_host: page_host.as_deref(),
        has_meet_url,
        has_strong_bundle,
        has_weak_meeting_signal,
        has_huddle_hint,
        window_title,
    };
    let policy = detect::OfferPolicy {
        allow_mic_only: lane.settings.allow_mic_only_detect,
    };
    if let Decision::Offer {
        confidence,
        provenance,
    } = detect::evaluate_offer(&signals, &ctx, &policy)
    {
        // The window title, not the app name: "Weekly sync" is what the user calls the
        // meeting, and `zoom.us` on every row would make the whole timeline look identical.
        lane.title = window_title.map(str::to_string);
        lane.app_bundle_id = Some(bundle_id.to_string());
        lane.opened_via_meet_url = has_meet_url;
        lane.url_lost_since_ms = None;
        lane.opened_via_huddle = has_huddle_hint;
        lane.huddle_hint_lost_since_ms = None;
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

fn recording_app_present(
    lane: &mut Lane,
    obs: Option<&TickObservation<'_>>,
    now: i64,
    mic_open: bool,
) -> bool {
    if lane.opened_via_meet_url {
        return detect::meet_url_session_present(
            lane.url_lost_since_ms,
            now,
            mic_open,
            lane.mic_closed_since_ms,
        );
    }
    if lane.opened_via_huddle {
        // Slack stays running after the huddle ends (a Weak, resident app), so app presence
        // proves nothing — the hint-loss grace is the end signal, mirroring the Meet-tab rule.
        // The silence limit in `end_condition` still applies; whichever fires first ends it.
        return detect::huddle_session_present(
            lane.huddle_hint_lost_since_ms,
            now,
            mic_open,
            lane.mic_closed_since_ms,
        );
    }
    match lane.app_bundle_id.as_deref() {
        // `meeting_context: true` — a recording is running, and that IS the explanation for
        // the open device. The stuck check exists to stop an unexplained signal from OPENING
        // a meeting; letting it fire here would hang up on a call already in progress because
        // the user tabbed through three apps. Still fed rather than read, so a mic that
        // closes on a tick with no readable frontmost app still resets the stretch.
        None | Some("") => lane.mic.observe(
            &detect::MicObservation {
                in_use: mic_open,
                source: detect::MicSource::SystemWide,
                frontmost_bundle_id: obs.map(|o| o.bundle_id).unwrap_or(""),
                meeting_context: true,
            },
            now,
        ),
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

/// Track when a huddle session's hint was last seen (Plan A-4) — the huddle mirror of
/// [`observe_meeting_url`]. Slack's huddle UI is only observable while Slack is frontmost,
/// so leaving Slack starts the same grace as the hint disappearing; an open mic past the
/// grace keeps the session alive (see [`detect::huddle_session_present`]).
fn observe_huddle_hint(lane: &mut Lane, obs: &TickObservation<'_>, now: i64) {
    if !lane.opened_via_huddle {
        return;
    }
    if is_shogun_frontmost(obs.bundle_id) {
        return;
    }
    if obs.bundle_id != detect::SLACK_BUNDLE_ID {
        // Left Slack — same grace as the hint disappearing (FR-MT-11 shape).
        if lane.huddle_hint_lost_since_ms.is_none() {
            lane.huddle_hint_lost_since_ms = Some(now);
        }
        return;
    }
    if slack_huddle_hint(obs.bundle_id, obs.window_title) {
        lane.huddle_hint_lost_since_ms = None;
    } else if lane.huddle_hint_lost_since_ms.is_none() {
        lane.huddle_hint_lost_since_ms = Some(now);
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
                let elapsed = now - lane.since_ms;
                if elapsed < 0 {
                    // The clock jumped backwards (NTP, wake from sleep). Re-anchor the
                    // countdown instead of expiring it: expiry STARTS a recording, and a
                    // clock jump is not the user's consent.
                    lane.since_ms = now;
                    Next::Emit
                } else if elapsed >= Params::default().offer_grace_ms as i64 {
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
                    observe_huddle_hint(lane, obs, now);
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
    let bundle_id = bundle_id.trim();
    BROWSER_BUNDLE_IDS
        .iter()
        .any(|known| bundle_id.eq_ignore_ascii_case(known))
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
            // FR-MT-02a: with the feature off, this loop must not pay the per-second
            // Accessibility round-trips (focused window title, browser URL) — on_focus's own
            // gate sits AFTER those reads, so the check has to happen here. The cheap
            // frontmost/mic observations still flow so the mic watch stays warm.
            let enabled = LANE
                .try_lock()
                .ok()
                .and_then(|g| g.as_ref().map(|l| l.settings.enabled))
                .unwrap_or(false);
            // The window title is what names the meeting; the app name is the fallback when
            // Accessibility has nothing (permission not granted, or a window with no title).
            let title = if enabled {
                crate::axcache::focused_window(front.pid)
                    .and_then(|w| w.title())
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| front.name.clone())
            } else {
                front.name.clone()
            };
            // Only asked of browsers: every other app would pay an Accessibility round-trip
            // per second to answer "no".
            let url = (enabled && is_browser(&front.bundle_id))
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
                // title_len, not the title: window titles are captured user content (document
                // names, mail subjects) and must not reach the log — same rule as the URL path.
                let line = format!(
                    "{} state={} mic={} browser={} host={} title_len={}",
                    front.bundle_id,
                    state_tag(),
                    crate::mic::input_in_use(),
                    is_browser(&front.bundle_id),
                    host.as_deref().unwrap_or("-"),
                    title.chars().count()
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
                Some(&front.name),
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
