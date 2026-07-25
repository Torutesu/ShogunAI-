//! Dream Cycle health / Batch-API failure escalation (FR-DC-05). Pure counter → indicator.
//!
//! Batch-API failure (including no result within 24h) turns the indicator amber and carries the
//! unprocessed work to the next night; three consecutive failed days turn it red. Crucially, this
//! state governs **only** the Dream Cycle indicator — local capture, search, and Fusion
//! presentation are unaffected by Batch-API failure (FR-DC-05), which is why this module holds no
//! reference to them: they cannot be gated by it.

/// The Dream Cycle indicator colour (§6.6 error behaviour / FR-DC-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indicator {
    /// Last cycle succeeded.
    Normal,
    /// 1–2 consecutive failed days: carrying work over.
    Amber,
    /// 3+ consecutive failed days: needs attention (Full UI detail).
    Red,
}

/// Consecutive failed days at which the indicator goes red (FR-DC-05).
pub const RED_THRESHOLD_DAYS: u32 = 3;

/// The indicator for a given consecutive-failed-days count.
pub fn indicator(consecutive_failed_days: u32) -> Indicator {
    match consecutive_failed_days {
        0 => Indicator::Normal,
        d if d >= RED_THRESHOLD_DAYS => Indicator::Red,
        _ => Indicator::Amber,
    }
}

/// Fold a night's outcome into the running counter: success resets to zero, failure increments
/// (saturating). The daemon persists the returned count for the next night.
pub fn record_outcome(consecutive_failed_days: u32, batch_succeeded: bool) -> u32 {
    if batch_succeeded {
        0
    } else {
        consecutive_failed_days.saturating_add(1)
    }
}

/// The consecutive-failure count implied by a run of nightly outcomes, newest first.
///
/// The counter in [`record_outcome`] is the incremental form, for a daemon that folds one night at
/// a time. This is the same number recomputed from the ledger, which is what the app actually has
/// after a restart: nothing persists the running count, but every night's outcome is in `job_runs`.
/// Counting only the unbroken run of failures at the front is what makes a single good night reset
/// the indicator, exactly as `record_outcome(_, true)` does.
pub fn consecutive_failures(newest_first: &[bool]) -> u32 {
    newest_first.iter().take_while(|ok| !**ok).count() as u32
}

/// FR-DC-05 guarantee, made explicit: local features are never blocked by Dream Cycle health.
/// Always false — there is no failure count that disables local capture/search/Fusion.
pub fn local_features_blocked(_consecutive_failed_days: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_failures_is_normal() {
        assert_eq!(indicator(0), Indicator::Normal);
    }

    #[test]
    fn one_or_two_failed_days_is_amber() {
        assert_eq!(indicator(1), Indicator::Amber);
        assert_eq!(indicator(2), Indicator::Amber);
    }

    #[test]
    fn three_consecutive_failed_days_is_red() {
        assert_eq!(indicator(3), Indicator::Red);
        assert_eq!(indicator(10), Indicator::Red);
    }

    #[test]
    fn success_resets_the_counter() {
        assert_eq!(record_outcome(2, true), 0);
        assert_eq!(indicator(record_outcome(2, true)), Indicator::Normal);
    }

    #[test]
    fn failure_increments_and_escalates_over_three_nights() {
        let mut days = 0;
        for _ in 0..3 {
            days = record_outcome(days, false);
        }
        assert_eq!(days, 3);
        assert_eq!(indicator(days), Indicator::Red);
    }

    #[test]
    fn counter_saturates_without_overflow() {
        assert_eq!(record_outcome(u32::MAX, false), u32::MAX);
    }

    #[test]
    fn consecutive_failures_matches_the_incremental_counter() {
        // Two bad nights then a good one: the good night at the front resets, exactly as
        // record_outcome does when folded in the same order.
        assert_eq!(consecutive_failures(&[]), 0);
        assert_eq!(consecutive_failures(&[true, false, false]), 0);
        assert_eq!(consecutive_failures(&[false, true, false]), 1);
        assert_eq!(consecutive_failures(&[false, false, false]), 3);
        assert_eq!(indicator(consecutive_failures(&[false, false, false])), Indicator::Red);
        assert_eq!(indicator(consecutive_failures(&[false, true])), Indicator::Amber);
    }

    /// The recomputed count and the folded count must agree, or a restart would change the colour.
    #[test]
    fn recomputing_from_the_ledger_agrees_with_folding_night_by_night() {
        for nights in [
            vec![true, true, false],
            vec![false, false, true],
            vec![false, false, false],
            vec![true],
        ] {
            let mut folded = 0;
            for ok in &nights {
                folded = record_outcome(folded, *ok);
            }
            let newest_first: Vec<bool> = nights.iter().rev().copied().collect();
            assert_eq!(consecutive_failures(&newest_first), folded, "{nights:?}");
        }
    }

    #[test]
    fn local_features_never_blocked_regardless_of_failures() {
        // FR-DC-05: Batch-API failure must not affect local capture/search/Fusion.
        for d in [0, 1, 3, 100, u32::MAX] {
            assert!(!local_features_blocked(d));
        }
    }
}
