//! Retention policy for the media SHOGUN now keeps on disk (CLAUDE.md invariant 2, as revised
//! 2026-07-30): keyframes (FR-VIS-03, default 7 days) and meeting audio (FR-MT-12, default 30
//! days).
//!
//! The revision traded "we never store this" for "we store it, on this device, and it expires".
//! That trade is only honest if the expiry actually happens, which makes this module the thing
//! standing behind the disclosure text — deciding *what to delete* is not a detail of a cleanup
//! job, it is the promise itself. So it is pure: no filesystem, no clock, no database. Callers
//! hand in what they have and get back the ids to remove, and the rules can be tested exhaustively
//! without a Mac.
//!
//! Two independent reasons to delete, applied in that order:
//!
//! 1. **Age** — past the retention period (FR-VIS-03 / FR-MT-12). Non-negotiable: this is what the
//!    user was told would happen.
//! 2. **Budget** — the media store has its own storage ceiling, separate from the memory
//!    database's (NFR-RES-03). Over it, the oldest go until it fits.
//!
//! Age first, because deleting expired items may already bring the store under budget, and an item
//! that is *both* expired and oldest must not be counted twice.

/// One stored item, as far as retention is concerned. Deliberately not the row itself: the policy
/// has no business knowing a file path, let alone its contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub id: i64,
    /// When it was written (epoch ms).
    pub created_at: i64,
    /// Its size on disk.
    pub bytes: i64,
}

/// What a policy decides for one sweep.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sweep {
    /// Items past their retention period.
    pub expired: Vec<i64>,
    /// Items evicted to get back under the storage ceiling (oldest first).
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

/// A retention policy: how long to keep media, and how much of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// Retention period in milliseconds. `0` means "delete on the next sweep" — the honest reading
    /// of a zero-day setting, and what "keep nothing" must mean if it is offered.
    pub retain_ms: i64,
    /// The store's ceiling in bytes (NFR-RES-03: keyframes and audio each get their own).
    pub max_bytes: i64,
}

/// FR-VIS-03: keyframes are kept for a week by default.
pub const KEYFRAME_RETAIN_MS: i64 = 7 * 24 * 60 * 60 * 1_000;
/// FR-MT-12: meeting audio is kept for a month by default (Warm-tier symmetry).
pub const AUDIO_RETAIN_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
/// NFR-RES-03: each media store's own ceiling.
pub const MEDIA_MAX_BYTES: i64 = 5 * 1_024 * 1_024 * 1_024;

impl Policy {
    pub fn keyframes() -> Self {
        Self { retain_ms: KEYFRAME_RETAIN_MS, max_bytes: MEDIA_MAX_BYTES }
    }

    pub fn audio() -> Self {
        Self { retain_ms: AUDIO_RETAIN_MS, max_bytes: MEDIA_MAX_BYTES }
    }

    /// Whether an item written at `created_at` has expired by `now`.
    ///
    /// The boundary is inclusive — an item exactly at its retention age is gone, not kept for one
    /// more sweep. "Deleted after 7 days" should not quietly mean "after 7 days and a bit".
    pub fn is_expired(&self, created_at: i64, now: i64) -> bool {
        now.saturating_sub(created_at) >= self.retain_ms
    }

    /// Decide one sweep over `items` (any order).
    pub fn sweep(&self, items: &[Item], now: i64) -> Sweep {
        let mut expired = Vec::new();
        // Survivors, oldest first — the eviction order if the store is still over budget.
        let mut kept: Vec<Item> = Vec::new();
        for item in items {
            if self.is_expired(item.created_at, now) {
                expired.push(item.id);
            } else {
                kept.push(*item);
            }
        }
        // Oldest first, `id` breaking ties so a sweep is deterministic when timestamps collide —
        // two keyframes from the same millisecond must not evict in whichever order SQLite
        // happened to return them.
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

        expired.sort_unstable();
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
    fn items_past_the_period_expire_and_the_rest_stay() {
        let p = policy(1_000, i64::MAX);
        let now = 10_000;
        let s = p.sweep(&[item(1, 8_000, 1), item(2, 9_500, 1)], now);
        assert_eq!(s.expired, vec![1]);
        assert!(s.over_budget.is_empty());
    }

    #[test]
    fn the_expiry_boundary_is_inclusive() {
        // "Deleted after 7 days" must not mean "after 7 days and one sweep".
        let p = policy(1_000, i64::MAX);
        assert!(p.is_expired(0, 1_000));
        assert!(!p.is_expired(0, 999));
    }

    #[test]
    fn a_zero_day_setting_deletes_on_the_next_sweep() {
        // If the UI offers "keep nothing", it has to mean it.
        let p = policy(0, i64::MAX);
        let s = p.sweep(&[item(1, 10_000, 1)], 10_000);
        assert_eq!(s.expired, vec![1]);
    }

    #[test]
    fn a_backwards_clock_does_not_delete_everything() {
        // An NTP step must not read as "written in the future, therefore ancient".
        let p = policy(1_000, i64::MAX);
        let s = p.sweep(&[item(1, 10_000, 1)], 0);
        assert!(s.is_empty(), "a clock jump is not a retention event");
    }

    #[test]
    fn going_over_budget_evicts_the_oldest_first() {
        let p = policy(i64::MAX, 10);
        let s = p.sweep(&[item(3, 300, 6), item(1, 100, 6), item(2, 200, 6)], 1_000);
        // 18 bytes over a 10-byte ceiling: drop 1 (12 left, still over), then 2 (6 left, fits).
        assert_eq!(s.over_budget, vec![1, 2]);
        assert!(s.expired.is_empty());
    }

    #[test]
    fn eviction_stops_as_soon_as_the_store_fits() {
        let p = policy(i64::MAX, 10);
        let s = p.sweep(&[item(1, 100, 5), item(2, 200, 5), item(3, 300, 5)], 1_000);
        assert_eq!(s.over_budget, vec![1], "one eviction is enough; do not keep going");
    }

    #[test]
    fn a_store_exactly_at_the_ceiling_is_left_alone() {
        let p = policy(i64::MAX, 10);
        let s = p.sweep(&[item(1, 100, 10)], 1_000);
        assert!(s.is_empty());
    }

    #[test]
    fn expiry_runs_before_the_budget_and_may_settle_it_alone() {
        // The order matters: an expired item is deleted anyway, so counting it against the budget
        // would evict a live item that was about to fit.
        let p = policy(1_000, 10);
        let now = 10_000;
        let s = p.sweep(
            &[item(1, 100, 9), item(2, 9_500, 9)], // 1 is expired, 2 is fresh
            now,
        );
        assert_eq!(s.expired, vec![1]);
        assert!(s.over_budget.is_empty(), "9 bytes fit once the expired one is gone");
    }

    #[test]
    fn an_item_is_never_deleted_twice() {
        let p = policy(1_000, 1);
        let now = 10_000;
        let s = p.sweep(&[item(1, 100, 50), item(2, 9_999, 50)], now);
        let all = s.all();
        let mut sorted = all.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(all.len(), sorted.len(), "the two reasons must not overlap");
    }

    #[test]
    fn ties_in_timestamp_evict_deterministically() {
        let p = policy(i64::MAX, 1);
        let a = p.sweep(&[item(2, 100, 1), item(1, 100, 1)], 1_000);
        let b = p.sweep(&[item(1, 100, 1), item(2, 100, 1)], 1_000);
        assert_eq!(a.over_budget, b.over_budget, "row order must not change what is deleted");
        assert_eq!(a.over_budget, vec![1]);
    }

    #[test]
    fn an_empty_store_sweeps_to_nothing() {
        assert!(Policy::keyframes().sweep(&[], 1_000).is_empty());
    }

    #[test]
    fn the_shipped_defaults_are_the_documented_ones() {
        // The numbers in the disclosure text (FR-VIS-03 / FR-MT-12) and the ones in the code have
        // to be the same numbers, or the disclosure is fiction.
        assert_eq!(Policy::keyframes().retain_ms, 7 * 24 * 60 * 60 * 1_000);
        assert_eq!(Policy::audio().retain_ms, 30 * 24 * 60 * 60 * 1_000);
        assert_eq!(Policy::keyframes().max_bytes, Policy::audio().max_bytes);
    }

    #[test]
    fn a_week_old_keyframe_goes_and_a_six_day_one_stays() {
        let p = Policy::keyframes();
        let now = 100 * KEYFRAME_RETAIN_MS;
        let s = p.sweep(
            &[
                item(1, now - KEYFRAME_RETAIN_MS, 1),
                item(2, now - KEYFRAME_RETAIN_MS + 24 * 60 * 60 * 1_000, 1),
            ],
            now,
        );
        assert_eq!(s.expired, vec![1]);
    }
}
