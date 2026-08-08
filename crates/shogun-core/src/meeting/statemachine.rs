//! The meeting-notes lifecycle (FR-MT-07).
//!
//! ```text
//!   Idle
//!    │  detection (FR-MT-04)
//!    ▼
//!   Offered ──"Not now"───────────────────────────────────┐
//!    │  10s grace (start if nothing happens)              │
//!    ▼                                                    │
//!   Recording ──"Stop"──→ Wrapping ──→ Recap ──────────────┤
//!    │                                                    │
//!    └─ app quit / occurrence ended +10min / 15min silence ┘
//!                                                         ▼
//!                                                       Idle
//! ```
//!
//! **Offered is not a formality.** Recording is reachable only through it (FR-MT-08): the feature
//! starts on its own, but never before showing itself once, and refusal is always one tap away.
//! A transition straight from Idle to Recording would make "it was recording and I never knew"
//! representable — so the machine does not have that edge, and a test holds the line.

/// Timers the adapter schedules on request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timer {
    /// The 10s countdown shown in Offered (FR-MT-08).
    OfferGrace,
    /// Silence watchdog — 15 minutes without audio ends the meeting (FR-MT-11).
    Silence,
}

/// Named states of FR-MT-07. `Wrapping` is the interval closing: audio has stopped, Recap has not
/// yet been assembled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Offered,
    Recording,
    Wrapping,
}

impl State {
    pub fn tag(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Offered => "offered",
            State::Recording => "recording",
            State::Wrapping => "wrapping",
        }
    }
}

/// Why an interval ended. Kept on the transition because Recap and the health metrics want to know
/// whether the user stopped it or the machine gave up on it (FR-MT-11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    /// The user pressed Stop.
    UserStopped,
    /// The meeting app quit, or its window/tab disappeared.
    AppGone,
    /// The linked calendar occurrence ended more than 10 minutes ago.
    OccurrenceOver,
    /// 15 minutes of silence.
    Silence,
}

/// Inputs. Detection signals arrive from [`super::detect`]; the rest from the UI and the adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// Detection crossed the offer threshold (FR-MT-04).
    MeetingDetected,
    /// The 10s grace expired with no answer → start (FR-MT-08).
    GraceExpired,
    /// "Start" pressed during the grace.
    Start,
    /// "Not now" — this meeting only, settings untouched (FR-MT-02c).
    NotNow,
    /// "Stop" pressed while recording. No confirmation dialog (FR-MT-09).
    Stop,
    /// An automatic end condition fired (FR-MT-11).
    AutoEnd(EndReason),
    /// Recap finished (or degraded Recap was shown) → back to Idle.
    Wrapped,
    /// The whole feature was switched off (FR-MT-02a) — must stop everything, everywhere.
    FeatureDisabled,
}

/// Side effects for the adapter. The machine never touches the database or the audio device
/// itself; it says what must happen and the caller does it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Transition(State),
    StartTimer { timer: Timer, ms: u64 },
    CancelTimer(Timer),
    /// Open the interval (`sessions` row) — FR-MT-05.
    OpenSession,
    /// Close the interval at the current time, recording why.
    CloseSession(EndReason),
    /// Begin/stop capturing audio. **The only two effects that touch the microphone**, so the
    /// audio lane cannot be opened from anywhere but the Recording state (FR-MT-12).
    StartAudio,
    StopAudio,
    /// Assemble the Recap (FR-MT-16), degraded if need be (FR-MT-19).
    BuildRecap,
}

/// Timing constants. Named rather than inlined so the spec value is greppable from the code.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// FR-MT-08: how long Offered waits before starting on its own.
    pub offer_grace_ms: u64,
    /// FR-MT-11: silence that ends a meeting.
    pub silence_ms: u64,
}

impl Default for Params {
    fn default() -> Self {
        Self { offer_grace_ms: 10_000, silence_ms: 15 * 60 * 1_000 }
    }
}

/// The lifecycle machine. Holds only the current state — everything else lives in the caller.
#[derive(Debug)]
pub struct Machine {
    state: State,
    params: Params,
}

impl Machine {
    pub fn new(params: Params) -> Self {
        Self { state: State::Idle, params }
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// Apply an input, returning the effects to perform in order.
    pub fn step(&mut self, input: Input) -> Vec<Effect> {
        use Input as I;
        use State as S;

        match (self.state, input) {
            // ── Idle ────────────────────────────────────────────────────────────────────────
            // Detection offers; it never starts. There is deliberately no edge from Idle to
            // Recording (FR-MT-08) — see `recording_is_unreachable_without_passing_through_offered`.
            (S::Idle, I::MeetingDetected) => {
                self.state = S::Offered;
                vec![
                    Effect::Transition(S::Offered),
                    Effect::StartTimer { timer: Timer::OfferGrace, ms: self.params.offer_grace_ms },
                ]
            }

            // ── Offered ─────────────────────────────────────────────────────────────────────
            // Pressing Start and letting the countdown run out are the same transition: the
            // grace is a chance to refuse, not a different way of consenting.
            (S::Offered, I::Start | I::GraceExpired) => {
                self.state = S::Recording;
                vec![
                    Effect::CancelTimer(Timer::OfferGrace),
                    Effect::Transition(S::Recording),
                    Effect::OpenSession,
                    Effect::StartAudio,
                    Effect::StartTimer { timer: Timer::Silence, ms: self.params.silence_ms },
                ]
            }
            // "Not now" declines this meeting and nothing else: no session was opened, so there
            // is nothing to close, and no audio was started, so there is nothing to stop.
            (S::Offered, I::NotNow | I::FeatureDisabled) => {
                self.state = S::Idle;
                vec![Effect::CancelTimer(Timer::OfferGrace), Effect::Transition(S::Idle)]
            }

            // ── Recording ───────────────────────────────────────────────────────────────────
            (S::Recording, I::Stop) => self.end(EndReason::UserStopped),
            (S::Recording, I::AutoEnd(why)) => self.end(why),
            // Switching the feature off mid-meeting ends it like any other stop, on the same
            // path — so no route out of Recording can leave the microphone open.
            (S::Recording, I::FeatureDisabled) => self.end(EndReason::UserStopped),

            // ── Wrapping ────────────────────────────────────────────────────────────────────
            (S::Wrapping, I::Wrapped) => {
                self.state = S::Idle;
                vec![Effect::Transition(S::Idle)]
            }

            // A late timer or a repeated Stop must not move the
            // machine, and the daemon cannot afford to panic on an unexpected input (CLAUDE.md).
            _ => Vec::new(),
        }
    }

    /// How long the Recap may sit on screen before the lane returns to Idle on its own.
    pub const RECAP_DISMISS_MS: i64 = 90_000;
    /// Shorter auto-dismiss when the user has already left the call (FR-MT-11).
    pub const RECAP_DISMISS_LEFT_MS: i64 = 60_000;

    /// Recording → Wrapping. Audio stops first, so that the instant the interval is over the
    /// microphone is already closed — before any slower work (closing the row, building Recap).
    fn end(&mut self, why: EndReason) -> Vec<Effect> {
        self.state = State::Wrapping;
        vec![
            Effect::StopAudio,
            Effect::CancelTimer(Timer::Silence),
            Effect::Transition(State::Wrapping),
            Effect::CloseSession(why),
            Effect::BuildRecap,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine() -> Machine {
        Machine::new(Params::default())
    }

    #[test]
    fn detection_offers_rather_than_starting() {
        let mut m = machine();
        let fx = m.step(Input::MeetingDetected);

        assert_eq!(m.state(), State::Offered);
        assert!(
            !fx.contains(&Effect::StartAudio),
            "detection alone must never open the microphone (FR-MT-08)"
        );
        assert!(fx.contains(&Effect::StartTimer { timer: Timer::OfferGrace, ms: 10_000 }));
    }

    #[test]
    fn ignoring_the_offer_starts_recording_after_the_grace() {
        let mut m = machine();
        m.step(Input::MeetingDetected);

        let fx = m.step(Input::GraceExpired);

        assert_eq!(m.state(), State::Recording);
        assert!(fx.contains(&Effect::OpenSession));
        assert!(fx.contains(&Effect::StartAudio));
    }

    #[test]
    fn not_now_declines_without_opening_a_session() {
        let mut m = machine();
        m.step(Input::MeetingDetected);

        let fx = m.step(Input::NotNow);

        assert_eq!(m.state(), State::Idle);
        assert!(!fx.contains(&Effect::OpenSession));
        assert!(!fx.contains(&Effect::StartAudio));
    }

    #[test]
    fn recording_is_unreachable_without_passing_through_offered() {
        // The guarantee behind "it was recording and I never knew" being impossible: no input
        // applied to Idle produces audio or a session (FR-MT-08).
        for input in [
            Input::Start,
            Input::GraceExpired,
            Input::Stop,
            Input::Wrapped,
            Input::AutoEnd(EndReason::Silence),
            Input::NotNow,
            Input::FeatureDisabled,
        ] {
            let mut m = machine();
            let fx = m.step(input);
            assert_eq!(m.state(), State::Idle, "{input:?} must leave Idle alone");
            assert!(fx.is_empty(), "{input:?} must have no effect from Idle");
        }
    }

    #[test]
    fn stop_closes_the_interval_and_the_microphone() {
        let mut m = machine();
        m.step(Input::MeetingDetected);
        m.step(Input::Start);

        let fx = m.step(Input::Stop);

        assert_eq!(m.state(), State::Wrapping);
        assert_eq!(fx.first(), Some(&Effect::StopAudio), "audio stops first");
        assert!(fx.contains(&Effect::CloseSession(EndReason::UserStopped)));
        assert!(fx.contains(&Effect::BuildRecap));
    }

    #[test]
    fn every_automatic_end_condition_stops_the_microphone() {
        // FR-MT-11: the user forgetting to stop must not leave it running.
        for why in [EndReason::AppGone, EndReason::OccurrenceOver, EndReason::Silence] {
            let mut m = machine();
            m.step(Input::MeetingDetected);
            m.step(Input::Start);

            let fx = m.step(Input::AutoEnd(why));

            assert_eq!(m.state(), State::Wrapping);
            assert!(fx.contains(&Effect::StopAudio), "{why:?} must stop audio");
            assert!(fx.contains(&Effect::CloseSession(why)), "{why:?} must be recorded as the reason");
        }
    }

    #[test]
    fn disabling_the_feature_mid_meeting_stops_the_microphone() {
        // FR-MT-02a: the global off switch must reach a meeting already in progress.
        let mut m = machine();
        m.step(Input::MeetingDetected);
        m.step(Input::Start);

        let fx = m.step(Input::FeatureDisabled);

        assert!(fx.contains(&Effect::StopAudio));
        assert_eq!(m.state(), State::Wrapping);
    }

    #[test]
    fn a_repeated_stop_does_not_close_the_interval_twice() {
        let mut m = machine();
        m.step(Input::MeetingDetected);
        m.step(Input::Start);
        m.step(Input::Stop);

        let fx = m.step(Input::Stop);

        assert!(fx.is_empty(), "a second Stop while wrapping is a no-op");
        assert_eq!(m.state(), State::Wrapping);
    }

    #[test]
    fn wrapping_returns_to_idle_ready_for_the_next_meeting() {
        let mut m = machine();
        m.step(Input::MeetingDetected);
        m.step(Input::Start);
        m.step(Input::Stop);

        m.step(Input::Wrapped);
        assert_eq!(m.state(), State::Idle);

        // and the next meeting is offered again, not resumed
        let fx = m.step(Input::MeetingDetected);
        assert_eq!(m.state(), State::Offered);
        assert!(!fx.contains(&Effect::StartAudio));
    }

    #[test]
    fn every_ending_can_be_dismissed_back_to_idle() {
        // Detection only acts from Idle, so a machine parked in Wrapping has stopped noticing
        // meetings. Whichever way a meeting ended, dismissing the Recap must revive the lane.
        for ending in [
            Input::Stop,
            Input::FeatureDisabled,
            Input::AutoEnd(EndReason::AppGone),
            Input::AutoEnd(EndReason::OccurrenceOver),
            Input::AutoEnd(EndReason::Silence),
        ] {
            let mut m = machine();
            m.step(Input::MeetingDetected);
            m.step(Input::Start);

            m.step(ending);
            assert_eq!(m.state(), State::Wrapping, "{ending:?}: the Recap needs a state to live in");

            m.step(Input::Wrapped);
            assert_eq!(m.state(), State::Idle, "{ending:?} left the lane stuck");
        }
    }
}
