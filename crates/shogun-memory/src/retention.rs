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
//! Two independent reasons to delete, applied in that order:
//!
//! 1. **Age** — past the 72-hour window (`screen_frames::RETENTION_MS`). Non-negotiable: this is
//!    what the user was told would happen.
//! 2. **Budget** — the cache has a byte ceiling. Age alone bounds nothing on a heavy day: 72 hours
//!    of a busy screen is not a fixed size, and NFR-RES-03's disk budget is shared with Warm,
//!    Cold and the indexes. Over the ceiling, the oldest go until it fits.
//!
//! Age first, because deleting expired frames may already bring the cache under budget, and a
//! frame that is *both* expired and oldest must not be counted twice.
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
    /// Frames evicted to get back under the byte ceiling (oldest first).
    pub over_budget: Vec<i64>,
}

impl Sweep {
    /// Everything to delete, in one list.
    pub fn all(&self) -> Vec<i64> {
        self.expired.iter().chain(self.over_budget.iter()).copied().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.expired.is_empty() && self.over_budget.is_empty()
    }
}

/// The frame cache's ceiling. A tenth of NFR-RES-03's 20 GB amber threshold: the cache is a
/// 72-hour convenience, and it should not be what pushes a user's memory DB into the warning.
pub const FRAME_MAX_BYTES: i64 = 2 * 1_024 * 1_024 * 1_024;

/// A retention policy: how long to keep frames, and how many bytes of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// Retention period in milliseconds. `0` means "delete on the next sweep" — the honest reading
    /// of a zero setting, and what "keep nothing" must mean if it is ever offered.
    pub retain_ms: i64,
    /// The cache's ceiling in bytes.
    pub max_bytes: i64,
}

impl Policy {
    /// The shipping policy: the 72-hour window the disclosure names, under [`FRAME_MAX_BYTES`].
    pub fn frames() -> Self {
        Self { retain_ms: crate::screen_frames::RETENTION_MS, max_bytes: FRAME_MAX_BYTES }
    }

    /// Whether a frame written at `created_at` has expired by `now`.
    ///
    /// The boundary is inclusive — a frame exactly at its retention age is gone, not kept for one
    /// more sweep. "Deleted after 72 hours" should not quietly mean "after 72 hours and a bit".
    pub fn is_expired(&self, created_at: i64, now: i64) -> bool {
        now.saturating_sub(created_at) >= self.retain_ms
    }

    /// Decide one sweep over `items` (any order).
    pub fn sweep(&self, items: &[Item], now: i64) -> Sweep {
        let mut expired = Vec::new();
        // Survivors, oldest first — the eviction order if the cache is still over budget.
        let mut kept: Vec<Item> = Vec::new();
        for item in items {
            if self.is_expired(item.created_at, now) {
                expired.push(item.id);
            } else {
                kept.push(*item);
            }
        }
        // Oldest first, `id` breaking ties so a sweep is deterministic when timestamps collide —
        // two frames from the same millisecond must not evict in whichever order SQLite happened
        // to return them.
        kept.sort_by_key(|i| (i.created_at, i.id));

        let mut total: i64 = kept.iter().map(|i| i.bytes).sum();
        let mut over_budget = Vec::new();
        for item in &kept {
            if total <= self.max_bytes {
                break;
            }
            over_budget.push(item.id);
            total -= item.bytes;
        }
        Sweep { expired, over_budget }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i64, created_at: i64, bytes: i64) -> Item {
        Item { id, created_at, bytes }
    }

    fn policy(retain_ms: i64, max_bytes: i64) -> Policy {
        Policy { retain_ms, max_bytes }
    }

    #[test]
    fn expiry_boundary_is_inclusive() {
        let p = policy(1_000, i64::MAX);
        assert!(!p.is_expired(100, 1_099), "one ms short is still kept");
        assert!(p.is_expired(100, 1_100), "exactly at the age is gone");
    }

    #[test]
    fn a_zero_period_deletes_everything_on_the_next_sweep() {
        let p = policy(0, i64::MAX);
        let s = p.sweep(&[item(1, 500, 10)], 500);
        assert_eq!(s.expired, vec![1], "keep nothing has to mean keep nothing");
    }

    #[test]
    fn eviction_takes_the_oldest_until_it_fits() {
        let p = policy(i64::MAX, 100); // never expires; budget only
        let s = p.sweep(&[item(3, 300, 60), item(1, 100, 60), item(2, 200, 60)], 1_000);
        assert!(s.expired.is_empty());
        // 180 bytes against a 100-byte ceiling: drop the two oldest, not just enough to fit under
        // by one — 3 alone is 60 ≤ 100.
        assert_eq!(s.over_budget, vec![1, 2]);
    }

    #[test]
    fn ties_on_timestamp_break_by_id_so_a_sweep_is_deterministic() {
        let p = policy(i64::MAX, 10);
        let s = p.sweep(&[item(9, 100, 10), item(2, 100, 10), item(5, 100, 10)], 1_000);
        assert_eq!(s.over_budget, vec![2, 5], "same ms → lowest id evicts first");
    }

    #[test]
    fn expired_frames_are_not_also_counted_against_the_budget() {
        // 3 frames of 60 bytes against a 100-byte ceiling, but two of them are already expired.
        // Age runs first, which brings the cache to 60 — nothing left to evict.
        let p = policy(1_000, 100);
        let s = p.sweep(&[item(1, 0, 60), item(2, 0, 60), item(3, 5_000, 60)], 5_000);
        assert_eq!(s.expired, vec![1, 2]);
        assert!(s.over_budget.is_empty(), "an expired frame must not be deleted twice");
        assert_eq!(s.all(), vec![1, 2]);
    }

    #[test]
    fn a_cache_inside_both_limits_sweeps_nothing() {
        let p = Policy::frames();
        let s = p.sweep(&[item(1, 1_000, 1_024)], 2_000);
        assert!(s.is_empty());
        assert!(s.all().is_empty());
    }

    #[test]
    fn one_frame_larger_than_the_whole_ceiling_is_still_evicted() {
        // Otherwise the loop would leave the cache permanently over budget with nothing to drop.
        let p = policy(i64::MAX, 100);
        let s = p.sweep(&[item(1, 0, 5_000)], 1_000);
        assert_eq!(s.over_budget, vec![1]);
    }
}
