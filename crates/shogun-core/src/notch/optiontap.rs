//! The ⌥ double-tap trigger (FR-NU-10): two quick taps of Option, and the top context action
//! runs.
//!
//! Option is not an ordinary hotkey — it is a modifier people press all day without meaning
//! anything by it: ⌥C, ⌥-drag to duplicate, ⌥-click, holding it for an accent, holding it to see
//! alternate menu items. A trigger built on it is therefore mostly a machine for *refusing* to
//! fire, and that is what this module is: a tap is only a tap when Option went down and came back
//! up quickly, alone, with nothing else happening in between.
//!
//! The refusals, each earning its place:
//!
//! | what the user did | why it must not fire |
//! |---|---|
//! | ⌥ held (accent palette, alternate menu items) | a hold is not a tap — [`TAP_MAX_MS`] |
//! | ⌥C, ⌥⇧4, any ⌥-chord | Option was a modifier; the other key says so |
//! | ⌥-drag / ⌥-click | pointer use during the gesture |
//! | two taps far apart | two separate deliberate-nothings — [`GAP_MAX_MS`] |
//!
//! Time is injected (every method takes `now`), so the whole gesture is testable without timers
//! or a keyboard — the same discipline as the rest of [`crate::notch`].

/// How long Option may be held and still count as a tap.
///
/// 250ms: comfortably longer than a deliberate tap, comfortably shorter than the hold people use
/// to bring up alternate characters — which is the gesture that would otherwise fire the action
/// while someone is typing "é".
pub const TAP_MAX_MS: i64 = 250;

/// The longest pause between the two taps (FR-NU-10 default; settings may adjust it).
///
/// 300ms is the usual double-click window. Longer starts catching "pressed Option twice while
/// thinking"; shorter starts failing for people who do not drum their fingers.
pub const GAP_MAX_MS: i64 = 300;

/// What the macOS adapter observed. Only the shapes that can invalidate a tap are modelled —
/// this type deliberately cannot carry a keystroke's identity, because the trigger has no
/// business knowing what was typed (NFR-PRV-03: never log keystrokes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// Modifier flags changed. `option_down` is whether ⌥ is held *now*; `other_modifiers` is
    /// whether any of ⌘/⌃/⇧/fn are held at the same moment.
    Flags { option_down: bool, other_modifiers: bool },
    /// A non-modifier key went down: Option was being used as a modifier, not tapped.
    Key,
    /// The pointer was used (click, drag). ⌥-drag duplicates a file; it must not also act.
    Pointer,
}

/// Where in the gesture we are. Private: callers see only [`OptionDoubleTap::observe`]'s verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Nothing in progress.
    Idle,
    /// ⌥ went down at this time and is still held.
    Down { at: i64 },
    /// A completed first tap, released at this time.
    Tapped { at: i64 },
    /// ⌥ went down again at this time, within the gap.
    SecondDown { at: i64 },
}

/// Recognises "⌥ tapped twice, quickly, alone".
#[derive(Debug, Clone, Copy)]
pub struct OptionDoubleTap {
    phase: Phase,
    tap_max_ms: i64,
    gap_max_ms: i64,
}

impl Default for OptionDoubleTap {
    fn default() -> Self {
        Self::new()
    }
}

impl OptionDoubleTap {
    pub fn new() -> Self {
        Self { phase: Phase::Idle, tap_max_ms: TAP_MAX_MS, gap_max_ms: GAP_MAX_MS }
    }

    /// With user-configured timings (FR-NU-10: the trigger is adjustable and can be turned off —
    /// "off" is the adapter simply not installing the monitor, not a variant here).
    pub fn with_timings(tap_max_ms: i64, gap_max_ms: i64) -> Self {
        Self { phase: Phase::Idle, tap_max_ms, gap_max_ms }
    }

    /// Feed one observation. Returns `true` exactly once, on the release that completes a valid
    /// double tap; the gesture then resets, so holding a third tap cannot fire twice.
    pub fn observe(&mut self, input: Input, now: i64) -> bool {
        match input {
            // Anything that means "Option was doing its normal job" abandons the gesture.
            Input::Key | Input::Pointer => {
                self.phase = Phase::Idle;
                false
            }
            Input::Flags { other_modifiers: true, .. } => {
                // A chord. Even if ⌥ is down, this is ⌥⇧-something, not a tap.
                self.phase = Phase::Idle;
                false
            }
            Input::Flags { option_down: true, .. } => {
                self.phase = match self.phase {
                    // Within the gap after a first tap → this is the second press.
                    Phase::Tapped { at } if elapsed(at, now) <= self.gap_max_ms => {
                        Phase::SecondDown { at: now }
                    }
                    // Too slow, or a fresh start.
                    Phase::Idle | Phase::Tapped { .. } => Phase::Down { at: now },
                    // Already held: macOS repeats flagsChanged; keep the original press time so a
                    // repeat cannot refresh the clock and turn a hold into a tap.
                    held @ (Phase::Down { .. } | Phase::SecondDown { .. }) => held,
                };
                false
            }
            Input::Flags { option_down: false, .. } => {
                let phase = self.phase;
                self.phase = Phase::Idle;
                match phase {
                    Phase::Down { at } if elapsed(at, now) <= self.tap_max_ms => {
                        // A clean first tap. Wait for the second.
                        self.phase = Phase::Tapped { at: now };
                        false
                    }
                    Phase::SecondDown { at } if elapsed(at, now) <= self.tap_max_ms => true,
                    // A hold, or a release with nothing pending.
                    _ => false,
                }
            }
        }
    }

    /// Abandon any gesture in progress (screen lock, focus loss, the monitor restarting).
    pub fn reset(&mut self) {
        self.phase = Phase::Idle;
    }
}

/// Elapsed time that never goes negative: a clock stepping backwards abandons the gesture rather
/// than reporting an impossibly fast tap.
fn elapsed(from: i64, now: i64) -> i64 {
    now.saturating_sub(from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(other: bool) -> Input {
        Input::Flags { option_down: true, other_modifiers: other }
    }
    fn up() -> Input {
        Input::Flags { option_down: false, other_modifiers: false }
    }

    /// The happy path, as a reusable sequence: two taps `gap` apart, each `hold` long.
    fn double_tap(d: &mut OptionDoubleTap, hold: i64, gap: i64) -> bool {
        d.observe(down(false), 0);
        d.observe(up(), hold);
        d.observe(down(false), hold + gap);
        d.observe(up(), hold + gap + hold)
    }

    #[test]
    fn two_quick_taps_fire() {
        let mut d = OptionDoubleTap::new();
        assert!(double_tap(&mut d, 60, 120));
    }

    #[test]
    fn one_tap_does_nothing() {
        let mut d = OptionDoubleTap::new();
        assert!(!d.observe(down(false), 0));
        assert!(!d.observe(up(), 60));
    }

    #[test]
    fn a_held_option_is_not_a_tap() {
        // Holding ⌥ shows alternate menu items and accent palettes. Someone reaching for "é"
        // must not have their top action run underneath them.
        let mut d = OptionDoubleTap::new();
        assert!(!double_tap(&mut d, TAP_MAX_MS + 1, 100));
    }

    #[test]
    fn taps_too_far_apart_do_not_fire() {
        let mut d = OptionDoubleTap::new();
        assert!(!double_tap(&mut d, 50, GAP_MAX_MS + 1));
    }

    #[test]
    fn a_chord_is_not_a_tap() {
        // ⌥C, ⌥⇧4, ⌘⌥ — Option doing its actual job.
        let mut d = OptionDoubleTap::new();
        d.observe(down(false), 0);
        d.observe(Input::Key, 20); // the other key
        d.observe(up(), 40);
        d.observe(down(false), 100);
        assert!(!d.observe(up(), 140), "a chord must not leave half a gesture behind");
    }

    #[test]
    fn another_modifier_joining_abandons_the_gesture() {
        let mut d = OptionDoubleTap::new();
        d.observe(down(false), 0);
        d.observe(down(true), 10); // ⇧ joins
        d.observe(up(), 40);
        d.observe(down(false), 100);
        assert!(!d.observe(up(), 140));
    }

    #[test]
    fn option_drag_does_not_fire() {
        // ⌥-drag duplicates a file. It must not also run an action.
        let mut d = OptionDoubleTap::new();
        d.observe(down(false), 0);
        d.observe(Input::Pointer, 30);
        d.observe(up(), 200);
        d.observe(down(false), 260);
        assert!(!d.observe(up(), 300));
    }

    #[test]
    fn a_repeated_flags_event_cannot_refresh_the_hold_clock() {
        // macOS repeats flagsChanged while a modifier is held. If a repeat reset the press time,
        // a long hold would become a tap at the moment the user let go.
        let mut d = OptionDoubleTap::new();
        d.observe(down(false), 0);
        d.observe(down(false), 200);
        d.observe(down(false), 400);
        assert!(!d.observe(up(), 500), "still a hold, not a tap");
    }

    #[test]
    fn a_third_tap_does_not_fire_again_on_its_own() {
        let mut d = OptionDoubleTap::new();
        assert!(double_tap(&mut d, 50, 100));
        // one more tap right after: the gesture has reset, so this is a first tap
        d.observe(down(false), 300);
        assert!(!d.observe(up(), 350));
    }

    #[test]
    fn a_fourth_tap_completes_a_second_gesture() {
        // Two double-taps in a row should each fire — the reset must not be sticky.
        let mut d = OptionDoubleTap::new();
        assert!(double_tap(&mut d, 50, 100));
        d.observe(down(false), 1_000);
        d.observe(up(), 1_050);
        d.observe(down(false), 1_150);
        assert!(d.observe(up(), 1_200));
    }

    #[test]
    fn a_slow_first_tap_can_still_start_a_new_gesture() {
        // Hold, release (not a tap), then two proper taps: the stale hold must not poison them.
        let mut d = OptionDoubleTap::new();
        d.observe(down(false), 0);
        d.observe(up(), TAP_MAX_MS + 100);
        d.observe(down(false), 1_000);
        d.observe(up(), 1_050);
        d.observe(down(false), 1_150);
        assert!(d.observe(up(), 1_200));
    }

    #[test]
    fn a_backwards_clock_abandons_the_gesture_instead_of_firing() {
        let mut d = OptionDoubleTap::new();
        d.observe(down(false), 10_000);
        // NTP steps the clock back mid-gesture.
        assert!(!d.observe(up(), 0), "negative elapsed must not read as instantaneous");
    }

    #[test]
    fn reset_abandons_a_gesture_in_progress() {
        let mut d = OptionDoubleTap::new();
        d.observe(down(false), 0);
        d.observe(up(), 50);
        d.reset(); // screen locked between taps
        d.observe(down(false), 100);
        assert!(!d.observe(up(), 150));
    }

    #[test]
    fn custom_timings_are_honoured() {
        let mut d = OptionDoubleTap::with_timings(500, 800);
        // Would fail the defaults, passes the configured window.
        assert!(double_tap(&mut d, 400, 700));
    }
}
