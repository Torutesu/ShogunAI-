//! Single source of truth for SHOGUN SLO thresholds.
//!
//! These values mirror the CLAUDE.md SLO table and `docs/notch-ui-prototype-spec.md`
//! §1 / §6. Report generation and Go/No-Go verdicts read from here — do not hardcode
//! SLO numbers anywhere else (spec §4.1). When the harness is moved into the real
//! implementation, this module becomes the SLO's single truth.

/// Idle → Expanded, p95 upper bound (ms). Q2.
pub const EXPAND_MS: f64 = 100.0;

/// Context-action button presentation, p95 upper bound (ms). Phase 1 SLO;
/// measured as a reference value in Phase 0 (spec §4.2.5).
pub const ACTION_PRESENT_MS: f64 = 150.0;

/// Context cache update on focus switch, p95 upper bound (ms). Q3-A.
pub const CACHE_UPDATE_MS: f64 = 300.0;

/// SHOGUN's own idle CPU, 1-minute average upper bound (percent, 1 core = 100%). Q3-B.
pub const IDLE_CPU_PCT: f64 = 5.0;

/// Local search, p95 upper bound (ms). Not exercised in Phase 0; kept for Phase 1.
pub const LOCAL_SEARCH_MS: f64 = 500.0;

// --- Phase 0 acceptance thresholds derived from the four questions (spec §6) ---

/// Reference target for perceived expand latency including dwell (spec §4.2.1). Not an SLO.
pub const EXPAND_PERCEIVED_MS: f64 = 250.0;

/// Q3-A: fraction of cache updates allowed to be partial before deepening budget cuts.
pub const CACHE_PARTIAL_RATE_MAX: f64 = 0.30;

/// Q3-B: idle CPU samples must be within `IDLE_CPU_PCT` for at least this fraction (spec §6.3).
pub const IDLE_CPU_WITHIN_FRACTION: f64 = 0.95;

/// Q3-B: hard ceiling on any idle CPU 1-min sample (spec §6.3).
pub const IDLE_CPU_MAX_PCT: f64 = 8.0;

/// Q4: free-work false positives allowed over the 8h soak window (spec §6.4).
pub const FALSE_POSITIVE_MAX_FREEWORK: u32 = 5;

/// Q4: false-positive rate ceiling (false positives / top-band entries) (spec §6.4).
pub const FALSE_POSITIVE_RATE_MAX: f64 = 0.02;

/// Q1: self-heals tolerated over a 24h soak before failing (spec §6.1).
pub const SELF_HEAL_MAX_24H: u32 = 2;

/// A pass/fail verdict against a threshold, carried into the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Verdict {
    Pass,
    Fail,
}

impl Verdict {
    /// `measured` (e.g. a p95) passes when it is at or below `threshold`.
    pub fn le(measured: f64, threshold: f64) -> Self {
        if measured <= threshold {
            Verdict::Pass
        } else {
            Verdict::Fail
        }
    }

    pub fn is_pass(self) -> bool {
        matches!(self, Verdict::Pass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_match_claude_md_table() {
        // Guardrail: if any of these change, CLAUDE.md and the spec must change too.
        assert_eq!(EXPAND_MS, 100.0);
        assert_eq!(ACTION_PRESENT_MS, 150.0);
        assert_eq!(CACHE_UPDATE_MS, 300.0);
        assert_eq!(IDLE_CPU_PCT, 5.0);
        assert_eq!(LOCAL_SEARCH_MS, 500.0);
    }

    #[test]
    fn verdict_boundary_is_inclusive() {
        assert!(Verdict::le(100.0, EXPAND_MS).is_pass());
        assert!(Verdict::le(99.9, EXPAND_MS).is_pass());
        assert!(!Verdict::le(100.1, EXPAND_MS).is_pass());
    }
}
