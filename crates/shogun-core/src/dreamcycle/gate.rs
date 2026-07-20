//! Dream Cycle run-condition gate (FR-DC-01). Pure: given the current conditions, decide whether
//! to run the full cycle, the degraded (state-only) cycle, or nothing — and why.
//!
//! The policy (FR-DC-01):
//! - Run only when the user is idle (no input ≥15 min) **or** the screen is locked, **and** the
//!   Mac is on power **or** the battery is ≥30%.
//! - Inside the nightly window a satisfied condition runs the **full** cycle.
//! - If the window elapsed without a full run, the next idle runs the **degraded** cycle
//!   (state maintenance only) and the full cycle carries to the next night.
//! - Before the window (and not carrying over) there is nothing to do.
//! - A full cycle already done today is never repeated.

/// Idle threshold: 15 minutes of no input (FR-DC-01).
pub const IDLE_THRESHOLD_MS: u64 = 15 * 60 * 1000;
/// Battery floor when off power (FR-DC-01).
pub const BATTERY_FLOOR_PCT: u8 = 30;

/// The observable conditions the gate decides from. The daemon fills these from NSWorkspace / IOKit
/// power / the idle timer; the gate itself reads no clock or device state (so it is testable).
#[derive(Debug, Clone, Copy)]
pub struct RunConditions {
    /// The current time is inside the configured nightly window (default 02:00–06:00 local).
    pub within_window: bool,
    /// The window has already passed today without a full run (carry-over case).
    pub window_elapsed: bool,
    /// Input-idle duration in ms.
    pub idle_ms: u64,
    /// Screen is locked (counts as idle regardless of `idle_ms`).
    pub screen_locked: bool,
    /// On wall power.
    pub power_connected: bool,
    /// Battery percentage (0..=100).
    pub battery_pct: u8,
    /// A full cycle has already completed today (do not repeat).
    pub full_run_done_today: bool,
}

/// Why the gate declined to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// A full cycle already ran today.
    AlreadyRanToday,
    /// The user is active (not idle, screen unlocked).
    NotIdle,
    /// Off power and battery below the floor.
    InsufficientPower,
    /// Before the window and nothing to carry over.
    OutsideWindow,
}

/// The gate's decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunDecision {
    /// Run the full six-job cycle.
    Full,
    /// Run the degraded, state-maintenance-only cycle (no Batch API).
    Degraded,
    /// Do not run; carries the reason.
    Skip(SkipReason),
}

impl RunConditions {
    fn user_idle(&self) -> bool {
        self.screen_locked || self.idle_ms >= IDLE_THRESHOLD_MS
    }

    fn power_ok(&self) -> bool {
        self.power_connected || self.battery_pct >= BATTERY_FLOOR_PCT
    }
}

/// Decide whether/how to run the Dream Cycle (FR-DC-01). Order of checks matters: an
/// already-complete day short-circuits before idle/power so a re-trigger never double-runs.
pub fn decide(cond: &RunConditions) -> RunDecision {
    if cond.full_run_done_today {
        return RunDecision::Skip(SkipReason::AlreadyRanToday);
    }
    if !cond.user_idle() {
        return RunDecision::Skip(SkipReason::NotIdle);
    }
    if !cond.power_ok() {
        return RunDecision::Skip(SkipReason::InsufficientPower);
    }
    if cond.within_window {
        RunDecision::Full
    } else if cond.window_elapsed {
        // Missed the window but idle+powered now → degraded catch-up; full carries to next night.
        RunDecision::Degraded
    } else {
        RunDecision::Skip(SkipReason::OutsideWindow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> RunConditions {
        RunConditions {
            within_window: true,
            window_elapsed: false,
            idle_ms: IDLE_THRESHOLD_MS,
            screen_locked: false,
            power_connected: true,
            battery_pct: 100,
            full_run_done_today: false,
        }
    }

    #[test]
    fn idle_and_powered_in_window_runs_full() {
        assert_eq!(decide(&base()), RunDecision::Full);
    }

    #[test]
    fn screen_locked_counts_as_idle_even_if_active_timer() {
        let c = RunConditions { idle_ms: 0, screen_locked: true, ..base() };
        assert_eq!(decide(&c), RunDecision::Full);
    }

    #[test]
    fn active_user_is_skipped() {
        let c = RunConditions { idle_ms: 60_000, screen_locked: false, ..base() };
        assert_eq!(decide(&c), RunDecision::Skip(SkipReason::NotIdle));
    }

    #[test]
    fn battery_at_floor_is_enough_off_power() {
        let c = RunConditions { power_connected: false, battery_pct: 30, ..base() };
        assert_eq!(decide(&c), RunDecision::Full);
    }

    #[test]
    fn low_battery_off_power_is_skipped() {
        let c = RunConditions { power_connected: false, battery_pct: 29, ..base() };
        assert_eq!(decide(&c), RunDecision::Skip(SkipReason::InsufficientPower));
    }

    #[test]
    fn missed_window_runs_degraded_catch_up() {
        let c = RunConditions { within_window: false, window_elapsed: true, ..base() };
        assert_eq!(decide(&c), RunDecision::Degraded);
    }

    #[test]
    fn before_window_with_no_carryover_skips() {
        let c = RunConditions { within_window: false, window_elapsed: false, ..base() };
        assert_eq!(decide(&c), RunDecision::Skip(SkipReason::OutsideWindow));
    }

    #[test]
    fn already_ran_today_never_repeats_even_if_idle_and_in_window() {
        let c = RunConditions { full_run_done_today: true, ..base() };
        assert_eq!(decide(&c), RunDecision::Skip(SkipReason::AlreadyRanToday));
    }
}
