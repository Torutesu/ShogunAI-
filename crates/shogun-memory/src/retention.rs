//! Retention policy for the one media store SHOGUN keeps: the visual-recall frame cache
//! (`screen_frames`, the explicit invariant-2 exception of 2026-08-02).
//!
//! The exception traded "we never store this" for "we store it, on this device, in the encrypted
//! memory DB, and it expires". That trade is only honest if the expiry actually happens, which
//! makes this module the thing standing behind the disclosure text — deciding *what to delete* is
//! not a detail of a cleanup job, it is the promise itself. So it is pure: no filesystem, no
//! clock, no database. Callers hand in what they have and get back the ids to remove, and the
//! rules can be tested exhaustively without a Mac.
//!
//! The single deletion reason is **age**: a frame is removed when it reaches the finite duration
//! the user selected. There is deliberately no hidden byte-budget eviction. A duration setting
//! must mean that duration. New capture pauses at [`FRAME_CAPTURE_MAX_BYTES`] instead, preserving
//! every already-captured frame until its selected expiry.
//!
//! Note what is NOT here: there is no audio policy. SHOGUN does not write waveforms to disk or to
//! a temp file at all (invariant 2), so there is nothing to expire — a retention rule for audio
//! would imply a store that must not exist.

/// One stored frame, as far as retention is concerned. Deliberately not the row itself: the policy
/// has no business knowing the JPEG, let alone its contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub id: i64,
    /// When it was written (epoch ms).
    pub created_at: i64,
    /// Its stored size in bytes.
    pub bytes: i64,
}

/// What a policy decides for one sweep.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sweep {
    /// Frames past the retention window.
    pub expired: Vec<i64>,
}

impl Sweep {
    /// Everything to delete, in one list.
    pub fn all(&self) -> Vec<i64> {
        self.expired.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.expired.is_empty()
    }
}

/// New automatic frame capture pauses before this encrypted-store ceiling. Existing frames are
/// never deleted to meet it; age expiry resumes room naturally.
pub const FRAME_CAPTURE_MAX_BYTES: i64 = 2 * 1_024 * 1_024 * 1_024;

/// A frame that crosses the ceiling is accepted; subsequent capture pauses. This makes the pause
/// state observable from stored totals and avoids an invisible permanently-rejected edge frame.
pub fn capture_allowed(current_bytes: i64) -> bool {
    current_bytes < FRAME_CAPTURE_MAX_BYTES
}

/// A finite age-based retention policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// Retention period in milliseconds. `0` means "delete on the next sweep" — the honest reading
    /// of a zero setting, and what "keep nothing" must mean if it is ever offered.
    pub retain_ms: i64,
}

impl Policy {
    pub fn frames(retain_ms: i64) -> Self {
        Self { retain_ms }
    }

    /// Whether a frame written at `created_at` has expired by `now`.
    ///
    /// The boundary is inclusive — a frame exactly at its retention age is gone, not kept for one
    /// more sweep. A selected duration must not quietly mean "that duration and a bit".
    pub fn is_expired(&self, created_at: i64, now: i64) -> bool {
        now.saturating_sub(created_at) >= self.retain_ms
    }

    /// Decide one sweep over `items` (any order).
    pub fn sweep(&self, items: &[Item], now: i64) -> Sweep {
        let mut expired: Vec<Item> = items
            .iter()
            .copied()
            .filter(|item| self.is_expired(item.created_at, now))
            .collect();
        expired.sort_by_key(|item| (item.created_at, item.id));
        Sweep { expired: expired.into_iter().map(|item| item.id).collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i64, created_at: i64, bytes: i64) -> Item {
        Item { id, created_at, bytes }
    }

    fn policy(retain_ms: i64) -> Policy {
        Policy { retain_ms }
    }

    #[test]
    fn expiry_boundary_is_inclusive() {
        let p = policy(1_000);
        assert!(!p.is_expired(100, 1_099), "one ms short is still kept");
        assert!(p.is_expired(100, 1_100), "exactly at the age is gone");
    }

    #[test]
    fn a_zero_period_deletes_everything_on_the_next_sweep() {
        let p = policy(0);
        let s = p.sweep(&[item(1, 500, 10)], 500);
        assert_eq!(s.expired, vec![1], "keep nothing has to mean keep nothing");
    }

    #[test]
    fn stored_bytes_never_shorten_the_selected_duration() {
        let p = policy(1_000);
        let s = p.sweep(&[item(1, 4_201, i64::MAX), item(2, 4_500, i64::MAX)], 5_200);
        assert!(s.is_empty(), "in-window frames survive regardless of byte size");
    }

    #[test]
    fn a_frame_inside_selected_age_sweeps_nothing() {
        let p = Policy::frames(1_000);
        let s = p.sweep(&[item(1, 1_000, 1_024)], 1_999);
        assert!(s.is_empty());
        assert!(s.all().is_empty());
    }

    #[test]
    fn expired_order_is_deterministic() {
        let p = policy(1_000);
        let s = p.sweep(&[item(9, 100, 10), item(2, 100, 10), item(5, 50, 10)], 2_000);
        assert_eq!(s.expired, vec![5, 2, 9]);
    }

    #[test]
    fn one_crossing_capture_is_allowed_then_future_capture_pauses() {
        assert!(capture_allowed(FRAME_CAPTURE_MAX_BYTES - 1));
        assert!(!capture_allowed(FRAME_CAPTURE_MAX_BYTES));
        assert!(!capture_allowed(FRAME_CAPTURE_MAX_BYTES + 1));
    }
}
