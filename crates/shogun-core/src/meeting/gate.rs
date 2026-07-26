//! Remembering a "no" for longer than an instant (FR-MT-02c).
//!
//! Detection runs on a timer, so the state machine returning to Idle after a decline is not the
//! end of the story: the meeting app is still frontmost on the next tick, and the offer comes
//! straight back. Without this gate, "Not now" buys the user one second, and Stop is followed by
//! a fresh offer that starts recording again ten seconds later — the decline is real in the state
//! machine and meaningless in practice.
//!
//! The design's answer is a cooldown: after a decline, the same meeting is not offered again for
//! a while (Issue #7 puts it at ten minutes). Two things end the cooldown early or late:
//!
//! - a *different* app coming to the front means the meeting was left, so a later return is a new
//!   meeting and may be offered again
//! - the cooldown is per app, so declining a call does not silence a different meeting app
//!
//! This is deliberately not part of [`super::settings::Settings`]: a decline must not be
//! persisted (FR-MT-02c says it changes no settings), so it lives only in memory and dies with
//! the process.

use std::collections::HashMap;

/// Issue #7: how long a declined meeting stays declined.
pub const COOLDOWN_MS: i64 = 10 * 60 * 1_000;

/// In-memory record of what the user has said no to, and until when.
#[derive(Debug, Default, Clone)]
pub struct OfferGate {
    /// bundle id → the moment the cooldown lapses.
    declined_until: HashMap<String, i64>,
}

impl OfferGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a decline for `bundle_id` at `now`.
    ///
    /// Called for every way a meeting ends without the user wanting more of it: "Not now", and
    /// also Stop — someone who just stopped a meeting is not asking to be offered it again while
    /// they are still looking at the same window.
    pub fn decline(&mut self, bundle_id: &str, now: i64) {
        self.declined_until.insert(bundle_id.to_string(), now + COOLDOWN_MS);
    }

    /// Whether `bundle_id` may be offered at `now`.
    pub fn may_offer(&self, bundle_id: &str, now: i64) -> bool {
        // `map_or(true, ..)` rather than `is_none_or`: the latter is stable only from 1.82
        // and the crate's MSRV is 1.80.
        self.declined_until.get(bundle_id).map_or(true, |until| now >= *until)
    }

    /// Note which app is frontmost. Leaving a meeting app clears its cooldown: coming back later
    /// is a new meeting, and the user should be asked about it rather than silently ignored.
    pub fn observe_front(&mut self, bundle_id: &str) {
        self.declined_until.retain(|id, _| id == bundle_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZOOM: &str = "us.zoom.xos";

    #[test]
    fn an_app_that_was_never_declined_may_be_offered() {
        assert!(OfferGate::new().may_offer(ZOOM, 1_000));
    }

    #[test]
    fn a_decline_survives_the_next_detection_tick() {
        // The bug this type exists to prevent: the state machine is back in Idle one second
        // later, the meeting app is still frontmost, and the offer returns as if nothing was
        // said. One second of "no" is not a "no" (FR-MT-02c).
        let mut gate = OfferGate::new();
        gate.decline(ZOOM, 1_000);

        gate.observe_front(ZOOM);
        assert!(!gate.may_offer(ZOOM, 2_000), "one second later it must still be declined");
    }

    #[test]
    fn a_decline_lapses_after_the_cooldown() {
        let mut gate = OfferGate::new();
        gate.decline(ZOOM, 1_000);

        assert!(!gate.may_offer(ZOOM, 1_000 + COOLDOWN_MS - 1));
        assert!(gate.may_offer(ZOOM, 1_000 + COOLDOWN_MS));
    }

    #[test]
    fn declining_one_app_does_not_silence_another() {
        let mut gate = OfferGate::new();
        gate.decline(ZOOM, 1_000);

        assert!(gate.may_offer("com.microsoft.teams", 2_000));
    }

    #[test]
    fn leaving_the_app_and_coming_back_is_a_new_meeting() {
        // Declining this morning's stand-up must not silence this afternoon's call in the same
        // app. Switching away is the signal that the meeting was left.
        let mut gate = OfferGate::new();
        gate.decline(ZOOM, 1_000);

        gate.observe_front("com.apple.Safari");

        assert!(gate.may_offer(ZOOM, 2_000));
    }

    #[test]
    fn staying_in_the_app_does_not_clear_the_decline() {
        let mut gate = OfferGate::new();
        gate.decline(ZOOM, 1_000);

        for tick in 1..=30 {
            gate.observe_front(ZOOM);
            assert!(!gate.may_offer(ZOOM, 1_000 + tick * 1_000), "tick {tick} re-offered");
        }
    }
}
