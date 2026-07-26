//! End-to-end integration of the meeting-notes lane: the settings gate, the detector, the
//! lifecycle machine and the database are driven together along the path the macOS adapter takes,
//! and the result is asserted on the stored interval rather than on the machine's internal state.
//!
//! The unit tests hold each piece to its contract. What can only be checked here is that the
//! pieces *compose* into the promises the feature makes to the user:
//!
//! - off means nothing happens (FR-MT-01/02a)
//! - listening is never reached without an offer first (FR-MT-08)
//! - a meeting that nobody stops still ends, and the interval closes (FR-MT-11)
//! - a meeting always comes back as something (FR-MT-19)
//!
//! It also pins the audio invariant that survives into MT3: **every path that opens the microphone
//! closes it again**. The machine already emits StartAudio/StopAudio (the adapter ignores them
//! until the ASR lane exists), so the balance is checkable now, before there is anything real to
//! leave running.

use shogun_core::daemon::{Clock, Db};
use shogun_core::meeting::detect::{self, Decision, LiveSignals, Signals};
use shogun_core::meeting::settings::{OfferContext, Settings};
use shogun_core::meeting::statemachine::{EndReason, Effect, Input, Machine, Params, State};
use std::sync::Arc;

fn clock(v: i64) -> Clock {
    Arc::new(move || v)
}

/// The adapter's loop, condensed: settings gate → detector → machine.
///
/// Mirrors `apps/desktop/src-tauri/src/meeting.rs::on_focus`. Keeping it in the same shape here
/// is what makes this an integration test of the real path rather than of a convenient one.
fn on_focus(
    settings: &Settings,
    machine: &mut Machine,
    bundle_id: &str,
) -> (Vec<Effect>, Option<f64>) {
    if !settings.enabled {
        return (Vec::new(), None);
    }
    if machine.state() != State::Idle {
        return (Vec::new(), None);
    }
    if !settings.may_offer(&OfferContext {
        app_bundle_id: Some(bundle_id),
        occurrence_external_id: None,
    }) {
        return (Vec::new(), None);
    }
    let signals = Signals {
        meeting_app_frontmost: detect::is_meeting_app(bundle_id),
        ..Default::default()
    };
    match detect::decide(&signals) {
        Decision::Offer { confidence, .. } => {
            (machine.step(Input::MeetingDetected), Some(confidence))
        }
        Decision::Ignore => (Vec::new(), None),
    }
}

fn enabled() -> Settings {
    Settings { enabled: true, ..Default::default() }
}

fn machine() -> Machine {
    Machine::new(Params::default())
}

/// A stand-in for the microphone: counts opens and closes so the test can assert they balance.
#[derive(Default, Debug)]
struct Mic {
    open: bool,
    ever_opened: bool,
}

/// Apply the effects a real adapter would honour.
fn run(db: &Db, session: &mut Option<i64>, mic: &mut Mic, effects: &[Effect]) {
    for fx in effects {
        match fx {
            Effect::OpenSession => {
                *session = db.open_meeting(Some("Weekly sync"), Some("us.zoom.xos"), 0.35, "{}");
            }
            Effect::CloseSession(_) => {
                if let Some(id) = session.take() {
                    db.close_meeting(id);
                }
            }
            Effect::StartAudio => {
                mic.open = true;
                mic.ever_opened = true;
            }
            Effect::StopAudio => mic.open = false,
            _ => {}
        }
    }
}

#[test]
fn with_the_feature_off_a_meeting_app_produces_nothing_at_all() {
    // FR-MT-01/02a end to end: the shipped default is off, and off is not "detect but stay
    // quiet" — the detector is never consulted and no interval exists to be found later.
    let db = Db::open_in_memory(clock(1_000)).unwrap();
    let mut m = machine();
    let mut session = None;

    let (effects, confidence) = on_focus(&Settings::default(), &mut m, "us.zoom.xos");
    let mut mic = Mic::default();
    run(&db, &mut session, &mut mic, &effects);

    assert!(effects.is_empty());
    assert_eq!(confidence, None, "the detector must not even run while off");
    assert_eq!(m.state(), State::Idle);
    assert!(!mic.ever_opened, "off means the microphone is never even asked for");
    assert_eq!(session, None);
}

#[test]
fn an_excluded_app_produces_nothing_even_with_the_feature_on() {
    // Tier (b): the user said "not this app", and that answer holds without them repeating it.
    let db = Db::open_in_memory(clock(1_000)).unwrap();
    let mut settings = enabled();
    settings.exclude_app("us.zoom.xos");
    let mut m = machine();
    let mut session = None;

    let (effects, _) = on_focus(&settings, &mut m, "us.zoom.xos");
    run(&db, &mut session, &mut Mic::default(), &effects);

    assert_eq!(m.state(), State::Idle);
    assert_eq!(session, None);
}

#[test]
fn a_meeting_the_user_ignores_is_noted_and_comes_back_as_a_recap() {
    // The main path, with nobody touching anything: detected → offered → the grace runs out →
    // the meeting ends on its own → a Recap exists.
    let db = Db::open_in_memory(clock(1_000)).unwrap();
    let settings = enabled();
    let mut m = machine();
    let mut session = None;

    let (offered, confidence) = on_focus(&settings, &mut m, "us.zoom.xos");
    run(&db, &mut session, &mut Mic::default(), &offered);
    assert_eq!(m.state(), State::Offered, "detection offers; it does not start");
    assert!(confidence.is_some_and(|c| c > 0.0 && c < 1.0), "detection is never certain");
    assert_eq!(session, None, "no interval exists until the offer is answered or expires");

    run(&db, &mut session, &mut Mic::default(), &m.step(Input::GraceExpired));
    assert_eq!(m.state(), State::Recording);
    let id = session.expect("the interval opens when recording starts");

    db.save_meeting_note(id, "- vendor renewal agreed at 12k");

    // Nobody presses Stop; the meeting app quits.
    let live = LiveSignals {
        meeting_app_present: false,
        occurrence_ends_at: None,
        last_sound_at: 1_000,
    };
    let why = detect::end_condition(&live, 1_000).expect("the app disappearing ends the meeting");
    assert_eq!(why, EndReason::AppGone);
    run(&db, &mut session, &mut Mic::default(), &m.step(Input::AutoEnd(why)));

    let recap = db.meeting_recap(id).expect("a finished meeting always has a recap");
    assert_eq!(recap.title, "Weekly sync");
    assert_eq!(recap.notes.as_deref(), Some("- vendor renewal agreed at 12k"));
    assert!(recap.degraded, "MT2 ships the degraded recap; the summary arrives in MT4");
}

#[test]
fn declining_the_offer_leaves_no_trace_of_the_meeting() {
    // "Not now" must not leave a half-open interval behind that a later query would surface as
    // "you had a meeting" — the user said no (FR-MT-02c).
    let db = Db::open_in_memory(clock(1_000)).unwrap();
    let mut m = machine();
    let mut session = None;

    run(&db, &mut session, &mut Mic::default(), &on_focus(&enabled(), &mut m, "us.zoom.xos").0);
    run(&db, &mut session, &mut Mic::default(), &m.step(Input::NotNow));

    assert_eq!(m.state(), State::Idle);
    assert_eq!(session, None);
    let conn_count = db.meeting_recap(1);
    assert!(conn_count.is_none(), "no interval was ever created");
}

#[test]
fn every_route_that_opens_the_microphone_also_closes_it() {
    // The promise behind "one tap always stops it": whatever ends a meeting — the user, the app
    // quitting, silence, the slot expiring, the feature being switched off — the microphone is
    // closed on the way out. Walked for every ending, because the one that leaks is always the
    // one nobody thought to check (FR-MT-07/11/12).
    let db = Db::open_in_memory(clock(1_000)).unwrap();
    let settings = enabled();

    for ending in [
        Input::Stop,
        Input::AutoEnd(EndReason::AppGone),
        Input::AutoEnd(EndReason::Silence),
        Input::AutoEnd(EndReason::OccurrenceOver),
        Input::FeatureDisabled,
    ] {
        let mut m = machine();
        let mut session = None;
        let mut mic = Mic::default();

        run(&db, &mut session, &mut mic, &on_focus(&settings, &mut m, "us.zoom.xos").0);
        assert!(!mic.ever_opened, "{ending:?}: the offer must not open the microphone");

        run(&db, &mut session, &mut mic, &m.step(Input::Start));
        assert!(mic.open, "{ending:?}: recording opens it");

        run(&db, &mut session, &mut mic, &m.step(ending));
        assert!(!mic.open, "{ending:?}: this ending left the microphone open");

        run(&db, &mut session, &mut mic, &m.step(Input::Wrapped));
        assert!(!mic.open, "{ending:?}: wrapping must not reopen it");
        assert_eq!(m.state(), State::Idle);
        assert_eq!(session, None, "{ending:?}: the interval must be closed");
    }
}

#[test]
fn a_second_meeting_after_the_first_is_offered_again_rather_than_resumed() {
    // Consent does not carry over. Each meeting is asked about on its own (FR-MT-08).
    let db = Db::open_in_memory(clock(1_000)).unwrap();
    let settings = enabled();
    let mut m = machine();
    let mut session = None;

    run(&db, &mut session, &mut Mic::default(), &on_focus(&settings, &mut m, "us.zoom.xos").0);
    run(&db, &mut session, &mut Mic::default(), &m.step(Input::Start));
    let first = session;
    run(&db, &mut session, &mut Mic::default(), &m.step(Input::Stop));
    run(&db, &mut session, &mut Mic::default(), &m.step(Input::Wrapped));

    let (effects, _) = on_focus(&settings, &mut m, "us.zoom.xos");

    assert_eq!(m.state(), State::Offered);
    assert!(!effects.contains(&Effect::OpenSession), "the second meeting is offered, not opened");
    assert!(first.is_some_and(|id| db.meeting_recap(id).is_some()), "the first is still readable");
}

#[test]
fn turning_the_feature_off_mid_meeting_closes_the_interval() {
    // The global off switch has to reach a meeting already in progress, not merely prevent the
    // next one (FR-MT-02a).
    let db = Db::open_in_memory(clock(1_000)).unwrap();
    let mut m = machine();
    let mut session = None;

    run(&db, &mut session, &mut Mic::default(), &on_focus(&enabled(), &mut m, "us.zoom.xos").0);
    run(&db, &mut session, &mut Mic::default(), &m.step(Input::Start));
    let id = session.expect("recording opened an interval");

    run(&db, &mut session, &mut Mic::default(), &m.step(Input::FeatureDisabled));

    assert_eq!(session, None);
    assert!(
        db.meeting_recap(id).is_some_and(|r| r.duration_minutes.is_some()),
        "the interval must be closed, not left open forever"
    );
}
