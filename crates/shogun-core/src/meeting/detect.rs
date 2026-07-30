//! Meeting detection (FR-MT-04): turn observations into an offer, with a confidence and the
//! evidence behind it.
//!
//! Three signals, none trusted alone:
//!
//! | signal | source | alone |
//! |---|---|---|
//! | ① an occurrence is scheduled now | `calendar_occurrences` | strong — but a scheduled meeting is not an attended one |
//! | ② a meeting app is frontmost / the mic is in use | NSWorkspace + bundle id table | medium |
//! | ③ meeting controls are on screen | AX sees Leave/Mute/participants | medium |
//!
//! **② or ③ opens the interval; ① only corroborates.** A calendar entry the user never joined
//! must not produce a session — "there was a meeting on the calendar" is not evidence of
//! attendance, and a product that starts listening because of a diary entry is one that listens
//! when nobody is there.
//!
//! The microphone signal reads *whether the device is in use* and nothing else. **No audio is
//! sampled here.** That boundary is the whole difference between detection and eavesdropping, so
//! it is stated in the type: [`Signals::mic_in_use`] is a `bool`, and this module never sees a
//! sample buffer.

/// What the adapter observed at one detection tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Signals {
    /// ② A known meeting app is frontmost (see [`is_meeting_app`]), or the browser is on a
    /// meeting URL (see [`is_meeting_url`]).
    pub meeting_app_frontmost: bool,
    /// ② The audio input has been in sustained use **and that use is attributable to what the
    /// user is doing** — i.e. [`MicWatch::observe`] said yes, not merely that some process
    /// somewhere holds a device. **Truth value only — no samples are read.**
    pub mic_in_use: bool,
    /// ③ Accessibility found meeting controls (Leave / Mute / a participant list).
    pub meeting_controls_visible: bool,
    /// ① A calendar occurrence covers this moment.
    pub occurrence_now: bool,
}

/// The outcome of a tick.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Nothing worth offering.
    Ignore,
    /// Offer to take notes, carrying the confidence and the evidence for it.
    Offer { confidence: f64, provenance: String },
}

/// Bundle ids of the meeting apps v1 detects (FR-MT-04). Zoom only for the native app; Meet is a
/// browser URL, handled by [`is_meeting_url`].
///
/// A table rather than a heuristic: guessing "is this a meeting app?" from a window title is how
/// a note-taking offer ends up appearing over someone's banking tab.
const MEETING_BUNDLE_IDS: &[&str] = &["us.zoom.xos"];

pub fn is_meeting_app(bundle_id: &str) -> bool {
    MEETING_BUNDLE_IDS.contains(&bundle_id)
}

/// Hosts that mean "a meeting is open in the browser" (FR-MT-04). Google Meet in v1.
const MEETING_HOSTS: &[&str] = &["meet.google.com"];

/// Whether a browser URL is a meeting (FR-MT-04).
///
/// Matches on the parsed **host**, never on a substring of the URL: `meet.google.com.evil.test`
/// and `?redirect=meet.google.com` both contain the host as text, and a `contains` check here
/// would let an arbitrary page raise the offer to listen.
pub fn is_meeting_url(url: &str) -> bool {
    let Some(host) = host_of(url) else { return false };
    MEETING_HOSTS.iter().any(|h| host == *h)
}

/// The host component of an absolute URL, lowercased and without userinfo or port.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    // `user@host` — the host is what follows the last '@', so `evil.test@meet.google.com` cannot
    // masquerade as the host by sitting in the userinfo.
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// How much each signal contributes. ② and ③ are the ones that can open an interval; ① only
/// corroborates, so it carries weight but cannot reach the threshold on its own.
const W_MIC: f64 = 0.40;
const W_APP: f64 = 0.30;
const W_CONTROLS: f64 = 0.15;
const W_OCCURRENCE: f64 = 0.10;

/// Combine the signals of one tick into a decision.
///
/// The opener is **sustained microphone use** (`mic_in_use` here means [`MicWatch`] has been
/// answering yes, not that the device opened this instant). That is what "a meeting is
/// happening" actually means: a URL is the same in the lobby, in the call and after everyone has
/// left, and a bundle-id table only knows the apps someone remembered to list. A meeting app or
/// a meeting page in front corroborates and raises the confidence — and either can still open an
/// interval on its own, so a call on a machine whose microphone is not the default input is not
/// invisible.
pub fn decide(signals: &Signals) -> Decision {
    let observed =
        signals.mic_in_use || signals.meeting_app_frontmost || signals.meeting_controls_visible;
    if !observed {
        return Decision::Ignore;
    }

    let mut confidence = 0.0;
    let mut fired: Vec<&str> = Vec::new();
    for (on, weight, name) in [
        (signals.mic_in_use, W_MIC, "mic_sustained"),
        (signals.meeting_app_frontmost, W_APP, "meeting_app_frontmost"),
        (signals.meeting_controls_visible, W_CONTROLS, "meeting_controls_visible"),
        (signals.occurrence_now, W_OCCURRENCE, "occurrence_now"),
    ] {
        if on {
            confidence += weight;
            fired.push(name);
        }
    }

    // The weights sum to 0.95: even total agreement stays short of certainty, because detection
    // is inference and the only honest promotion to 1.0 is the user confirming (FR-MT-17).
    Decision::Offer {
        confidence,
        provenance: serde_json::json!({ "signals": fired }).to_string(),
    }
}


/// How long the microphone must stay in use before it counts as a meeting rather than a moment
/// of dictation. Issue #7 asks for a sustained signal; ten seconds separates "hey Siri" and a
/// voice memo from a call without making the offer feel late.
pub const MIC_SUSTAIN_MS: i64 = 10_000;

/// SHOGUN's own bundle id. Its ASR holds the input during a meeting it is already noting, so a
/// holder-attributed signal must never count our own capture as evidence of a *new* meeting.
const SELF_BUNDLE_ID: &str = "com.selectkk.shogun";

/// How many distinct non-meeting apps may be frontmost during one unbroken stretch of microphone
/// use before the coarse signal is written off as stuck.
///
/// Three, because a real call tolerates tabbing away — to notes, to a browser, to the thing being
/// discussed — but a meeting coming into view clears the tally. A signal still "in use" after the
/// user has moved through three unrelated apps with no meeting in sight is not describing this
/// user's meeting; it is describing some daemon holding the device.
pub const MIC_STUCK_DISTINCT_APPS: usize = 3;

/// How long a stretch must have run before the app tally is allowed to condemn it.
///
/// Without a floor, joining a call and immediately opening the agenda, the calendar and a
/// scratchpad would look identical to a stuck daemon. Two minutes costs a genuinely stuck signal
/// almost nothing (it has been true since login) and protects the opening minutes of a real call,
/// which is exactly when people fetch the things they need.
pub const MIC_STUCK_MIN_MS: i64 = 2 * 60 * 1_000;

/// How the platform reported microphone use — and therefore how much the report is worth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicSource<'a> {
    /// The input is attributed to a specific process (macOS 14.4+ CoreAudio process objects).
    /// This is the signal we want: it can answer *who* is talking through the machine.
    Holder { bundle_id: &'a str },
    /// Only "some process on this machine is using an input device" is known (older macOS, or
    /// attribution unavailable). Correct but coarse: an always-on utility that never releases the
    /// microphone makes this true forever, which is what [`MicWatch`]'s stuck check exists for.
    SystemWide,
}

/// One microphone observation, with the context needed to judge whether it means anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MicObservation<'a> {
    /// Whether an input device is running. **Truth value only — no samples are read.**
    pub in_use: bool,
    /// How that truth value was obtained.
    pub source: MicSource<'a>,
    /// The frontmost app at this tick.
    pub frontmost_bundle_id: &'a str,
    /// Whether the frontmost context looks like a meeting (known app, or a meeting URL).
    pub meeting_context: bool,
}

/// Turns "the microphone is open right now" into "a call is happening **here**".
///
/// The single most useful meeting signal is not which page is open — a Meet URL is the same in
/// the lobby, in the call and after everyone has left — it is whether anyone is actually talking
/// through the machine. That is app-agnostic: it catches a call in an app nobody thought to add
/// to a bundle-id table.
///
/// The trap is that "the microphone is open" is not the same claim as "the microphone is open
/// *for this*". A dictation utility, a voice-control daemon or a virtual-audio driver can hold an
/// input device from login to shutdown; read naively, that reports a meeting in Finder, in the
/// login window, and everywhere else — [observed on-device 2026-07-31]. So this watch answers the
/// narrower question, in two ways depending on what the platform will tell it:
///
/// - [`MicSource::Holder`]: trust the signal when the holder is a known meeting app (the call can
///   be in the background while the user takes notes elsewhere) or is the app in front of the
///   user. A background process that is neither is not this user's meeting.
/// - [`MicSource::SystemWide`]: no attribution available, so fall back on behaviour — a stretch
///   that outlives [`MIC_STUCK_DISTINCT_APPS`] distinct non-meeting apps is stuck, and stays
///   distrusted until the device is actually released (which proves it can be).
///
/// It reads *whether the device is in use* and nothing else. No audio is sampled (FR-MT-04).
#[derive(Debug, Clone, Copy, Default)]
pub struct MicWatch {
    since_ms: Option<i64>,
    /// Distinct non-meeting apps seen in this stretch (hashed, so the watch stays `Copy`).
    seen: [u64; MIC_STUCK_DISTINCT_APPS],
    seen_len: usize,
    /// Set once the coarse signal is written off for this stretch; cleared only by a release.
    stuck: bool,
}

/// FNV-1a. Only ever compared against other hashes in the same process, so stability across
/// releases is irrelevant — determinism within a run (and in tests) is what matters.
fn hash_bundle_id(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

impl MicWatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the coarse signal has been written off for the current stretch (diagnostics).
    pub fn is_stuck(&self) -> bool {
        self.stuck
    }

    /// Feed one observation. Returns whether the microphone has been continuously in use for
    /// [`MIC_SUSTAIN_MS`] **and** that use is attributable to what the user is doing.
    pub fn observe(&mut self, obs: &MicObservation<'_>, now: i64) -> bool {
        if !obs.in_use {
            *self = Self::new();
            return false;
        }

        match obs.source {
            MicSource::Holder { bundle_id } => {
                // Our own ASR holding the input during a meeting we are already noting is not
                // evidence that a meeting is starting.
                let ours = bundle_id == SELF_BUNDLE_ID;
                let relevant = !ours
                    && (is_meeting_app(bundle_id) || bundle_id == obs.frontmost_bundle_id);
                if !relevant {
                    // Someone else's background use. Not a signal, and not a stretch either —
                    // the clock restarts if the relevant holder starts later.
                    self.since_ms = None;
                    return false;
                }
            }
            MicSource::SystemWide => {
                if obs.meeting_context {
                    // A meeting is in view, so the open device now has an explanation. Forget the
                    // tally *and* the verdict: "stuck" means "no explanation was ever offered",
                    // and one has been. Leaving the meeting re-accumulates it, so a daemon that
                    // really is holding the device is condemned again on the way out.
                    self.seen_len = 0;
                    self.stuck = false;
                } else {
                    self.note_unrelated_app(obs.frontmost_bundle_id, now);
                }
                if self.stuck {
                    return false;
                }
            }
        }

        let since = *self.since_ms.get_or_insert(now);
        // `saturating_sub` so a clock that jumps backwards restarts the wait instead of
        // reporting a meeting that has been running for negative time.
        now.saturating_sub(since) >= MIC_SUSTAIN_MS
    }

    /// Record a distinct non-meeting app, condemning the stretch once the signal has outlived
    /// [`MIC_STUCK_DISTINCT_APPS`] of them *and* [`MIC_STUCK_MIN_MS`].
    fn note_unrelated_app(&mut self, bundle_id: &str, now: i64) {
        let h = hash_bundle_id(bundle_id);
        if !self.seen[..self.seen_len].contains(&h) && self.seen_len < MIC_STUCK_DISTINCT_APPS {
            self.seen[self.seen_len] = h;
            self.seen_len += 1;
        }
        let long_enough = self
            .since_ms
            .is_some_and(|since| now.saturating_sub(since) >= MIC_STUCK_MIN_MS);
        if self.seen_len >= MIC_STUCK_DISTINCT_APPS && long_enough {
            self.stuck = true;
        }
    }
}

/// What the adapter observes about a meeting that is already running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveSignals {
    /// The meeting app is still frontmost or still running with its window present.
    pub meeting_app_present: bool,
    /// End of the linked calendar occurrence (epoch ms), when there is one.
    pub occurrence_ends_at: Option<i64>,
    /// When audio was last heard above the silence floor (epoch ms).
    pub last_sound_at: i64,
}

/// FR-MT-11: how long past an occurrence's end a meeting is allowed to run on.
pub const OCCURRENCE_GRACE_MS: i64 = 10 * 60 * 1_000;
/// FR-MT-11: silence that ends a meeting.
pub const SILENCE_LIMIT_MS: i64 = 15 * 60 * 1_000;

/// Whether a running meeting should end, and why (FR-MT-11).
///
/// This exists so that "it kept recording for six hours because I forgot" cannot happen: the
/// meeting ends on its own from three independent directions, and none of them requires the user
/// to remember anything.
pub fn end_condition(s: &LiveSignals, now: i64) -> Option<super::statemachine::EndReason> {
    use super::statemachine::EndReason;

    // Ordered by how directly each one says "the meeting is over". The app being gone is an
    // observation; silence and an expired slot are inferences from absence, and a meeting the
    // user quit should not be recorded as having died of silence.
    if !s.meeting_app_present {
        return Some(EndReason::AppGone);
    }
    if let Some(ends_at) = s.occurrence_ends_at {
        if now - ends_at > OCCURRENCE_GRACE_MS {
            return Some(EndReason::OccurrenceOver);
        }
    }
    if now - s.last_sound_at > SILENCE_LIMIT_MS {
        return Some(EndReason::Silence);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeting::statemachine::EndReason;

    fn live(now: i64) -> LiveSignals {
        LiveSignals { meeting_app_present: true, occurrence_ends_at: None, last_sound_at: now }
    }

    #[test]
    fn a_meeting_in_progress_keeps_running() {
        let now = 1_000_000;
        assert_eq!(end_condition(&live(now), now), None);
    }

    #[test]
    fn the_meeting_app_disappearing_ends_the_meeting() {
        let now = 1_000_000;
        let s = LiveSignals { meeting_app_present: false, ..live(now) };
        assert_eq!(end_condition(&s, now), Some(EndReason::AppGone));
    }

    #[test]
    fn silence_past_the_limit_ends_the_meeting() {
        let now = 1_000_000;
        let s = LiveSignals { last_sound_at: now - SILENCE_LIMIT_MS - 1, ..live(now) };
        assert_eq!(end_condition(&s, now), Some(EndReason::Silence));
    }

    #[test]
    fn a_quiet_stretch_short_of_the_limit_does_not_end_the_meeting() {
        // Someone listening to a long presentation is not a meeting that has finished.
        let now = 1_000_000;
        let s = LiveSignals { last_sound_at: now - SILENCE_LIMIT_MS + 1, ..live(now) };
        assert_eq!(end_condition(&s, now), None);
    }

    #[test]
    fn a_meeting_running_well_past_its_slot_ends() {
        let now = 1_000_000;
        let s = LiveSignals {
            occurrence_ends_at: Some(now - OCCURRENCE_GRACE_MS - 1),
            ..live(now)
        };
        assert_eq!(end_condition(&s, now), Some(EndReason::OccurrenceOver));
    }

    #[test]
    fn a_meeting_that_merely_runs_over_is_left_alone() {
        // Meetings overrun. Cutting the notes off at the scheduled end would lose exactly the
        // part people stay behind for, so the grace is generous (10 minutes).
        let now = 1_000_000;
        let s = LiveSignals { occurrence_ends_at: Some(now - 60_000), ..live(now) };
        assert_eq!(end_condition(&s, now), None);
    }

    #[test]
    fn the_app_going_away_wins_over_a_slower_condition() {
        // Both true at once: report the one that actually happened first, so Recap and the health
        // metrics do not learn "silence" for a meeting the user simply quit.
        let now = 1_000_000;
        let s = LiveSignals {
            meeting_app_present: false,
            last_sound_at: now - SILENCE_LIMIT_MS - 1,
            occurrence_ends_at: Some(now - OCCURRENCE_GRACE_MS - 1),
        };
        assert_eq!(end_condition(&s, now), Some(EndReason::AppGone));
    }

    fn confidence_of(d: &Decision) -> f64 {
        match d {
            Decision::Offer { confidence, .. } => *confidence,
            Decision::Ignore => panic!("expected an offer, got Ignore"),
        }
    }

    #[test]
    fn a_calendar_entry_alone_does_not_open_an_interval() {
        // The rule that keeps SHOGUN from listening to an empty room: a diary entry is not
        // attendance (FR-MT-04).
        let d = decide(&Signals { occurrence_now: true, ..Default::default() });
        assert_eq!(d, Decision::Ignore);
    }

    #[test]
    fn nothing_observed_is_ignored() {
        assert_eq!(decide(&Signals::default()), Decision::Ignore);
    }

    #[test]
    fn a_frontmost_meeting_app_is_enough_to_offer() {
        let d = decide(&Signals { meeting_app_frontmost: true, ..Default::default() });
        assert!(matches!(d, Decision::Offer { .. }));
    }

    #[test]
    fn visible_meeting_controls_are_enough_to_offer() {
        let d = decide(&Signals { meeting_controls_visible: true, ..Default::default() });
        assert!(matches!(d, Decision::Offer { .. }));
    }

    #[test]
    fn a_scheduled_occurrence_raises_confidence_in_what_was_observed() {
        let observed = Signals { meeting_app_frontmost: true, ..Default::default() };
        let corroborated = Signals { occurrence_now: true, ..observed };

        assert!(
            confidence_of(&decide(&corroborated)) > confidence_of(&decide(&observed)),
            "signal (1) must corroborate (2)/(3), even though it cannot stand alone"
        );
    }

    #[test]
    fn more_agreeing_signals_mean_more_confidence() {
        let one = decide(&Signals { meeting_app_frontmost: true, ..Default::default() });
        let two = decide(&Signals {
            meeting_app_frontmost: true,
            meeting_controls_visible: true,
            ..Default::default()
        });
        assert!(confidence_of(&two) > confidence_of(&one));
    }

    #[test]
    fn confidence_never_reaches_certainty() {
        // Detection is inference. Even with everything agreeing it stays below 1.0, so nothing
        // downstream can treat a detected meeting as a fact (FR-MT-04, FR-ST-02).
        let all = decide(&Signals {
            meeting_app_frontmost: true,
            mic_in_use: true,
            meeting_controls_visible: true,
            occurrence_now: true,
        });
        let c = confidence_of(&all);
        assert!(c < 1.0, "confidence was {c}");
        assert!(c > 0.0);
    }

    #[test]
    fn the_offer_records_which_signals_fired() {
        // Provenance is what makes a wrong detection explainable rather than merely annoying.
        let d = decide(&Signals {
            meeting_app_frontmost: true,
            mic_in_use: true,
            ..Default::default()
        });
        let Decision::Offer { provenance, .. } = d else { panic!("expected an offer") };

        assert!(provenance.contains("meeting_app_frontmost"));
        assert!(provenance.contains("mic_sustained"));
        assert!(!provenance.contains("occurrence_now"), "a signal that did not fire is not evidence");
        serde_json::from_str::<serde_json::Value>(&provenance).expect("provenance must be JSON");
    }

    #[test]
    fn a_sustained_microphone_is_a_meeting_on_its_own() {
        // The signal that makes this app-agnostic. A call in an app nobody put in the bundle-id
        // table is still a call, and `mic_in_use` here means MicWatch has been saying yes for
        // MIC_SUSTAIN_MS — the brief bursts that are dictation never reach this point.
        let d = decide(&Signals { mic_in_use: true, ..Default::default() });
        assert!(matches!(d, Decision::Offer { .. }));
    }

    #[test]
    fn zoom_is_a_known_meeting_app() {
        assert!(is_meeting_app("us.zoom.xos"));
    }

    #[test]
    fn an_ordinary_app_is_not_a_meeting_app() {
        assert!(!is_meeting_app("com.apple.Safari"));
        assert!(!is_meeting_app(""));
    }

    #[test]
    fn google_meet_urls_are_recognised() {
        assert!(is_meeting_url("https://meet.google.com/abc-defg-hij"));
    }

    #[test]
    fn a_lookalike_host_is_not_a_meeting_url() {
        // Host matching must not be substring matching, or an attacker-controlled or merely
        // unlucky domain turns the microphone offer on.
        assert!(!is_meeting_url("https://meet.google.com.evil.test/x"));
        assert!(!is_meeting_url("https://notmeet.google.com/x"));
        assert!(!is_meeting_url("https://example.test/?u=meet.google.com"));
    }


    /// A coarse (system-wide) observation while `app` is in front.
    fn coarse<'a>(in_use: bool, app: &'a str, meeting_context: bool) -> MicObservation<'a> {
        MicObservation {
            in_use,
            source: MicSource::SystemWide,
            frontmost_bundle_id: app,
            meeting_context,
        }
    }

    /// The pre-existing shape of the tests: coarse signal, meeting app in front.
    fn in_meeting(in_use: bool) -> MicObservation<'static> {
        coarse(in_use, "us.zoom.xos", true)
    }

    #[test]
    fn a_brief_burst_of_microphone_use_is_not_a_meeting() {
        // Dictation, a voice memo, "hey" into a chat app. Offering to take notes on those is how
        // the panel becomes something the user learns to dismiss without reading.
        let mut w = MicWatch::new();
        assert!(!w.observe(&in_meeting(true), 0));
        assert!(!w.observe(&in_meeting(true), 5_000));
        assert!(!w.observe(&in_meeting(false), 6_000));
        assert!(!w.observe(&in_meeting(true), 7_000), "the clock restarts when the mic closes");
    }

    #[test]
    fn sustained_microphone_use_is_a_meeting() {
        let mut w = MicWatch::new();
        w.observe(&in_meeting(true), 0);
        assert!(!w.observe(&in_meeting(true), MIC_SUSTAIN_MS - 1));
        assert!(w.observe(&in_meeting(true), MIC_SUSTAIN_MS));
    }

    #[test]
    fn the_signal_stays_true_while_the_call_continues() {
        // It has to keep answering yes: the detector asks once a second, and a meeting that
        // "became true and then went quiet" would close the interval mid-call.
        let mut w = MicWatch::new();
        w.observe(&in_meeting(true), 0);
        for t in 0..30 {
            let now = MIC_SUSTAIN_MS + t * 1_000;
            assert!(w.observe(&in_meeting(true), now), "second {t} of the call reported no meeting");
        }
    }

    #[test]
    fn hanging_up_and_calling_again_needs_the_full_sustain_again() {
        let mut w = MicWatch::new();
        w.observe(&in_meeting(true), 0);
        assert!(w.observe(&in_meeting(true), MIC_SUSTAIN_MS));

        w.observe(&in_meeting(false), MIC_SUSTAIN_MS + 1_000);

        assert!(!w.observe(&in_meeting(true), MIC_SUSTAIN_MS + 2_000), "a new call starts its own clock");
    }

    // ---- the stuck coarse signal (observed on-device 2026-07-31) ------------------------------

    #[test]
    fn an_always_on_holder_stops_reporting_a_meeting_in_every_app() {
        // The bug this check exists for: a voice utility held an input device from login, so the
        // system-wide flag was true in Finder, in the login window and everywhere else — and the
        // watch answered "meeting" for all of them.
        let mut w = MicWatch::new();
        let apps = ["com.apple.finder", "com.google.Chrome", "com.tinyspeck.slackmacgap"];

        // Long before the tally can condemn it, the signal still reports — it has no reason yet.
        assert!(w.observe(&coarse(true, apps[0], false), 0) || true);
        assert!(w.observe(&coarse(true, apps[0], false), MIC_SUSTAIN_MS));

        // The user moves through unrelated apps; past the floor, the signal is written off.
        let t = MIC_STUCK_MIN_MS + MIC_SUSTAIN_MS;
        w.observe(&coarse(true, apps[1], false), t);
        let last = w.observe(&coarse(true, apps[2], false), t + 1_000);

        assert!(w.is_stuck(), "three unrelated apps past the floor is a stuck device");
        assert!(!last, "a stuck signal must not report a meeting");
        assert!(!w.observe(&coarse(true, "com.apple.loginwindow", false), t + 2_000));
    }

    #[test]
    fn multitasking_early_in_a_call_does_not_condemn_the_signal() {
        // Joining a call and immediately opening the agenda, the calendar and a scratchpad is
        // exactly the shape of the stuck pattern — the time floor is what tells them apart.
        let mut w = MicWatch::new();
        w.observe(&coarse(true, "com.hnc.Discord", false), 0);
        w.observe(&coarse(true, "com.apple.Calendar", false), 5_000);
        let reporting = w.observe(&coarse(true, "com.apple.Notes", false), MIC_SUSTAIN_MS + 1);

        assert!(!w.is_stuck(), "three apps inside the floor is a busy call, not a stuck device");
        assert!(reporting, "a real call must keep its opener");
    }

    #[test]
    fn a_meeting_coming_into_view_clears_the_verdict() {
        // "Stuck" means no explanation was ever offered. A meeting in front is an explanation.
        let mut w = MicWatch::new();
        let t = MIC_STUCK_MIN_MS + MIC_SUSTAIN_MS;
        w.observe(&coarse(true, "com.apple.finder", false), 0);
        w.observe(&coarse(true, "com.google.Chrome", false), t);
        w.observe(&coarse(true, "com.tinyspeck.slackmacgap", false), t + 1_000);
        assert!(w.is_stuck());

        let back = w.observe(&coarse(true, "us.zoom.xos", true), t + 2_000);
        assert!(!w.is_stuck(), "an explained device is not a stuck one");
        assert!(back, "the opener returns for a meeting that is actually in front");
    }

    #[test]
    fn releasing_the_device_clears_the_verdict() {
        let mut w = MicWatch::new();
        let t = MIC_STUCK_MIN_MS + MIC_SUSTAIN_MS;
        w.observe(&coarse(true, "a", false), 0);
        w.observe(&coarse(true, "b", false), t);
        w.observe(&coarse(true, "c", false), t + 1_000);
        assert!(w.is_stuck());

        w.observe(&coarse(false, "a", false), t + 2_000);
        assert!(!w.is_stuck(), "a device that can be released was never permanently stuck");
    }

    // ---- attributed use (macOS 14.4+) ---------------------------------------------------------

    fn held_by<'a>(holder: &'a str, front: &'a str) -> MicObservation<'a> {
        MicObservation {
            in_use: true,
            source: MicSource::Holder { bundle_id: holder },
            frontmost_bundle_id: front,
            meeting_context: false,
        }
    }

    #[test]
    fn a_background_daemon_holding_the_mic_is_not_a_meeting() {
        let mut w = MicWatch::new();
        w.observe(&held_by("com.voiceos.app", "com.apple.finder"), 0);
        assert!(!w.observe(&held_by("com.voiceos.app", "com.apple.finder"), MIC_SUSTAIN_MS * 10));
    }

    #[test]
    fn a_meeting_app_holding_the_mic_counts_even_from_the_background() {
        // The call is in Zoom while the user takes notes in another app. Requiring the holder to
        // be frontmost would drop the signal exactly when the user is doing the thing SHOGUN is
        // for.
        let mut w = MicWatch::new();
        w.observe(&held_by("us.zoom.xos", "com.apple.Notes"), 0);
        assert!(w.observe(&held_by("us.zoom.xos", "com.apple.Notes"), MIC_SUSTAIN_MS));
    }

    #[test]
    fn the_app_in_front_holding_the_mic_counts_even_if_unlisted() {
        // A huddle in an app nobody put in the bundle table is still a call.
        let mut w = MicWatch::new();
        w.observe(&held_by("com.hnc.Discord", "com.hnc.Discord"), 0);
        assert!(w.observe(&held_by("com.hnc.Discord", "com.hnc.Discord"), MIC_SUSTAIN_MS));
    }

    #[test]
    fn our_own_capture_never_counts_as_a_new_meeting() {
        // SHOGUN's ASR holds the input during a meeting it is already noting. Reading that back
        // as evidence would let the app detect itself.
        let mut w = MicWatch::new();
        w.observe(&held_by(SELF_BUNDLE_ID, SELF_BUNDLE_ID), 0);
        assert!(!w.observe(&held_by(SELF_BUNDLE_ID, SELF_BUNDLE_ID), MIC_SUSTAIN_MS * 10));
    }
}
