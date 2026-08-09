//! Always-on SLO metrics (NFR-SLO-00): a lightweight fixed-bucket histogram plus a registry
//! that tracks each of the six SLOs (NFR-SLO-01..06) against its p95 budget.
//!
//! Design constraints:
//! - **O(1) record, bounded memory.** Fixed bucket bounds, no per-sample storage — this runs
//!   on hot paths (every expand, every cache update) and in a resident process for days.
//! - **Pure and Linux-testable.** The macOS adapter wraps a registry in a lock and feeds it
//!   observed durations; the percentile / pass-fail logic lives here, unit-tested off-device.
//! - **Honest under overflow.** A percentile that lands above the largest bound is reported as
//!   that bound (a floor) and flagged, never silently interpolated to a fabricated value — the
//!   budgets are chosen to be one of the bounds, so an over-budget p95 still reads as FAIL.

/// A fixed-bucket histogram over `f64` samples (milliseconds, or percent for CPU).
///
/// `bounds` are ascending *inclusive upper* edges; there is one extra overflow bucket for
/// samples greater than the last bound. `record` is O(bounds.len()) (small, ≤~12).
#[derive(Clone, Debug)]
pub struct Histogram {
    /// Ascending upper edges. `counts[i]` holds samples with `value <= bounds[i]` and
    /// `value > bounds[i-1]`; `counts[bounds.len()]` is the overflow bucket.
    bounds: Vec<f64>,
    counts: Vec<u64>,
    total: u64,
    sum: f64,
    max: f64,
}

impl Histogram {
    /// Create a histogram with the given ascending upper-edge bounds. Non-ascending or empty
    /// bounds are rejected (returns `None`) — a misconfigured histogram must not silently
    /// mis-bucket.
    pub fn new(bounds: Vec<f64>) -> Option<Self> {
        if bounds.is_empty() || bounds.windows(2).any(|w| w[1] <= w[0]) {
            return None;
        }
        let n = bounds.len() + 1;
        Some(Self { bounds, counts: vec![0; n], total: 0, sum: 0.0, max: f64::MIN })
    }

    /// Record one sample. NaN is ignored (a bad clock read must not corrupt the tally).
    pub fn record(&mut self, value: f64) {
        if value.is_nan() {
            return;
        }
        let idx = match self.bounds.iter().position(|b| value <= *b) {
            Some(i) => i,
            None => self.bounds.len(), // overflow
        };
        self.counts[idx] += 1;
        self.total += 1;
        self.sum += value;
        if value > self.max {
            self.max = value;
        }
    }

    pub fn count(&self) -> u64 {
        self.total
    }

    pub fn max(&self) -> Option<f64> {
        (self.total > 0).then_some(self.max)
    }

    pub fn mean(&self) -> Option<f64> {
        (self.total > 0).then(|| self.sum / self.total as f64)
    }

    /// Fraction of samples `<= threshold` (e.g. the "95% of samples within 5%" CPU rule).
    /// Counts only whole buckets at or below `threshold`; a bucket straddling the threshold is
    /// excluded, so this is a conservative (lower-bound) fraction. `None` if no samples.
    pub fn fraction_within(&self, threshold: f64) -> Option<f64> {
        if self.total == 0 {
            return None;
        }
        let mut within = 0u64;
        for (i, c) in self.counts.iter().enumerate() {
            let upper = self.bounds.get(i).copied().unwrap_or(f64::INFINITY);
            if upper <= threshold {
                within += c;
            }
        }
        Some(within as f64 / self.total as f64)
    }

    /// Estimate the `q`-quantile (0.0..=1.0) by linear interpolation within the crossing
    /// bucket. Returns `(estimate, overflowed)`: when the quantile lands in the overflow
    /// bucket (above the largest bound), `estimate` is the largest bound (a floor) and
    /// `overflowed` is true. `None` if there are no samples.
    pub fn quantile(&self, q: f64) -> Option<(f64, bool)> {
        if self.total == 0 {
            return None;
        }
        let q = q.clamp(0.0, 1.0);
        // Fractional rank in [0, total].
        let target = q * self.total as f64;
        let mut cum = 0u64;
        for (i, c) in self.counts.iter().enumerate() {
            let next = cum + c;
            if (next as f64) >= target && *c > 0 {
                // Overflow bucket: no finite upper edge — floor at the last bound.
                let Some(&upper) = self.bounds.get(i) else {
                    return Some((*self.bounds.last().unwrap_or(&0.0), true));
                };
                let lower = if i == 0 { 0.0 } else { self.bounds[i - 1] };
                // How far into this bucket's count the target falls.
                let into = (target - cum as f64) / *c as f64;
                return Some((lower + (upper - lower) * into.clamp(0.0, 1.0), false));
            }
            cum = next;
        }
        // All mass at/below target — return the top finite bound.
        Some((*self.bounds.last().unwrap_or(&0.0), false))
    }

    /// Convenience: p50 / p95 / p99 (each with its overflow flag).
    pub fn p50(&self) -> Option<(f64, bool)> {
        self.quantile(0.50)
    }
    pub fn p95(&self) -> Option<(f64, bool)> {
        self.quantile(0.95)
    }
    pub fn p99(&self) -> Option<(f64, bool)> {
        self.quantile(0.99)
    }
}

/// The six SLOs (NFR-SLO-01..06). `budget_p95` is the acceptance ceiling; `unit` documents
/// the sample unit (ms for latencies, percent for idle CPU).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slo {
    /// Notch expand: event → Expanded final frame (≤100ms).
    Expand,
    /// Context actions presented: Expanded draw → 4 buttons drawn (≤150ms).
    ActionsPresented,
    /// Action execute → first streamed token (≤1000ms).
    FirstToken,
    /// Local search: query committed → results drawn (≤500ms).
    Search,
    /// Context cache update: focus.changed → cache swapped (≤300ms).
    CacheUpdate,
    /// Idle CPU 1-min average, percent (≤5%).
    IdleCpu,
}

impl Slo {
    pub const ALL: [Slo; 6] = [
        Slo::Expand,
        Slo::ActionsPresented,
        Slo::FirstToken,
        Slo::Search,
        Slo::CacheUpdate,
        Slo::IdleCpu,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Slo::Expand => "NFR-SLO-01",
            Slo::ActionsPresented => "NFR-SLO-02",
            Slo::FirstToken => "NFR-SLO-03",
            Slo::Search => "NFR-SLO-04",
            Slo::CacheUpdate => "NFR-SLO-05",
            Slo::IdleCpu => "NFR-SLO-06",
        }
    }

    /// Map a UI-reported metric name to its SLO. This is the contract for the desktop shell's
    /// `record_ui_slo(name, ms)` command (Plan B-1/B-6): the webview reports the durations only
    /// it can observe (buttons painted, results drawn) under these fixed names, and the mapping
    /// lives here so the shell shim stays a dumb pipe and the naming is Linux-tested. `IdleCpu`
    /// is deliberately unmappable — it is sampled out-of-process, never a UI duration.
    pub fn from_ui_name(name: &str) -> Option<Slo> {
        match name {
            "expand" => Some(Slo::Expand),
            "actions_present" => Some(Slo::ActionsPresented),
            "first_token" => Some(Slo::FirstToken),
            "local_search" => Some(Slo::Search),
            "cache_update" => Some(Slo::CacheUpdate),
            _ => None,
        }
    }

    /// The p95 acceptance ceiling (ms, or percent for `IdleCpu`).
    pub fn budget_p95(self) -> f64 {
        match self {
            Slo::Expand => 100.0,
            Slo::ActionsPresented => 150.0,
            Slo::FirstToken => 1000.0,
            Slo::Search => 500.0,
            Slo::CacheUpdate => 300.0,
            Slo::IdleCpu => 5.0,
        }
    }

    /// Default bucket bounds — chosen so the budget is one of the edges (an over-budget p95
    /// therefore reads as strictly greater than the budget bound, never rounded under it).
    fn default_bounds(self) -> Vec<f64> {
        match self {
            Slo::Expand => vec![5.0, 10.0, 20.0, 30.0, 50.0, 75.0, 100.0, 150.0, 250.0, 500.0, 1000.0],
            Slo::ActionsPresented => vec![20.0, 40.0, 60.0, 80.0, 100.0, 150.0, 250.0, 500.0, 1000.0],
            Slo::FirstToken => vec![100.0, 250.0, 500.0, 750.0, 1000.0, 1500.0, 3000.0, 5000.0],
            Slo::Search => vec![50.0, 100.0, 200.0, 300.0, 500.0, 750.0, 1000.0, 2000.0],
            Slo::CacheUpdate => vec![30.0, 60.0, 100.0, 150.0, 300.0, 500.0, 1000.0],
            Slo::IdleCpu => vec![1.0, 2.0, 3.0, 4.0, 5.0, 8.0, 15.0, 30.0, 60.0, 100.0],
        }
    }
}

/// One SLO's current standing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SloSnapshot {
    pub slo: Slo,
    pub count: u64,
    pub p50: f64,
    pub p95: f64,
    pub budget_p95: f64,
    /// True once at least one sample exists AND p95 ≤ budget. `false` while unmeasured — an
    /// SLO with no samples is UNMEASURED, never a pass (spec §4.5: silence ≠ success).
    pub pass: bool,
    pub measured: bool,
    /// True if p95 fell in the overflow bucket (reported value is a floor).
    pub p95_overflowed: bool,
}

/// The always-on registry: one histogram per SLO. The macOS adapter records into it on the
/// hot paths and reads snapshots for `shogun metrics` / the Advanced UI (NFR-SLO-00).
#[derive(Clone, Debug)]
pub struct SloRegistry {
    hists: Vec<(Slo, Histogram)>,
}

impl Default for SloRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SloRegistry {
    pub fn new() -> Self {
        let hists = Slo::ALL
            .iter()
            .map(|&s| {
                // default_bounds is always valid (ascending, non-empty); fall back to a
                // trivial single-bound histogram rather than unwrap, to honour the no-panic rule.
                let h = Histogram::new(s.default_bounds()).unwrap_or_else(|| {
                    Histogram::new(vec![s.budget_p95()]).unwrap_or_else(|| Histogram {
                        bounds: vec![1.0],
                        counts: vec![0, 0],
                        total: 0,
                        sum: 0.0,
                        max: f64::MIN,
                    })
                });
                (s, h)
            })
            .collect();
        Self { hists }
    }

    pub fn record(&mut self, slo: Slo, value: f64) {
        if let Some((_, h)) = self.hists.iter_mut().find(|(s, _)| *s == slo) {
            h.record(value);
        }
    }

    pub fn snapshot(&self, slo: Slo) -> SloSnapshot {
        let budget = slo.budget_p95();
        let Some((_, h)) = self.hists.iter().find(|(s, _)| *s == slo) else {
            return SloSnapshot { slo, count: 0, p50: 0.0, p95: 0.0, budget_p95: budget, pass: false, measured: false, p95_overflowed: false };
        };
        match (h.p50(), h.p95()) {
            (Some((p50, _)), Some((p95, over))) => SloSnapshot {
                slo,
                count: h.count(),
                p50,
                p95,
                budget_p95: budget,
                pass: p95 <= budget,
                measured: true,
                p95_overflowed: over,
            },
            _ => SloSnapshot { slo, count: 0, p50: 0.0, p95: 0.0, budget_p95: budget, pass: false, measured: false, p95_overflowed: false },
        }
    }

    pub fn snapshot_all(&self) -> Vec<SloSnapshot> {
        Slo::ALL.iter().map(|&s| self.snapshot(s)).collect()
    }
}

/// L5 lesson counters for the `shogun metrics` surface (Plan D-6): how many lessons are active
/// and how much feedback the last week recorded. Counts only — no instruction text, and never
/// any `feedback_events` content (CLAUDE.md: capture/user text stays out of metrics surfaces).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LessonCounters {
    pub active_lessons: i64,
    pub feedback_events_last_7d: i64,
}

/// Render SLO snapshots as the JSON both `shogun metrics` and the Advanced UI read (NFR-SLO-00).
/// Hand-rolled (no serde dep in this ungated module). An unmeasured SLO reports `measured:false`
/// and `pass:false` — silence is never success (spec §4.5).
pub fn render_snapshots_json(snapshots: &[SloSnapshot]) -> String {
    format!(r#"{{"metrics":[{}]}}"#, slo_items(snapshots).join(","))
}

/// [`render_snapshots_json`] plus the D-6 `lessons` block. `None` (counters not computable —
/// no DB behind this process, or a read failure) renders `"lessons":{"measured":false}` in the
/// crate's convention: an unmeasured value is flagged, never fabricated as zero.
pub fn render_snapshots_json_with_lessons(
    snapshots: &[SloSnapshot],
    lessons: Option<LessonCounters>,
) -> String {
    let lessons_json = match lessons {
        Some(c) => format!(
            r#"{{"active_lessons":{},"feedback_events_last_7d":{},"measured":true}}"#,
            c.active_lessons, c.feedback_events_last_7d
        ),
        None => r#"{"measured":false}"#.to_string(),
    };
    format!(r#"{{"metrics":[{}],"lessons":{}}}"#, slo_items(snapshots).join(","), lessons_json)
}

fn slo_items(snapshots: &[SloSnapshot]) -> Vec<String> {
    snapshots
        .iter()
        .map(|s| {
            format!(
                r#"{{"slo":"{}","count":{},"p50":{},"p95":{},"budget_p95":{},"pass":{},"measured":{},"p95_overflowed":{}}}"#,
                s.slo.id(),
                s.count,
                num(s.p50),
                num(s.p95),
                num(s.budget_p95),
                s.pass,
                s.measured,
                s.p95_overflowed,
            )
        })
        .collect()
}

/// Format an f64 as a finite JSON number (non-finite → 0 so the payload is always valid JSON).
fn num(v: f64) -> String {
    if v.is_finite() {
        format!("{v}")
    } else {
        "0".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_bounds() {
        assert!(Histogram::new(vec![]).is_none());
        assert!(Histogram::new(vec![10.0, 10.0]).is_none()); // not strictly ascending
        assert!(Histogram::new(vec![10.0, 5.0]).is_none()); // descending
        assert!(Histogram::new(vec![1.0, 2.0, 3.0]).is_some());
    }

    #[test]
    fn empty_histogram_reports_no_stats() {
        let h = Histogram::new(vec![10.0, 100.0]).unwrap();
        assert_eq!(h.count(), 0);
        assert!(h.max().is_none());
        assert!(h.mean().is_none());
        assert!(h.quantile(0.95).is_none());
        assert!(h.fraction_within(5.0).is_none());
    }

    #[test]
    fn records_count_max_mean() {
        let mut h = Histogram::new(vec![10.0, 50.0, 100.0]).unwrap();
        for v in [5.0, 15.0, 45.0, 200.0] {
            h.record(v);
        }
        assert_eq!(h.count(), 4);
        assert_eq!(h.max(), Some(200.0));
        assert_eq!(h.mean(), Some((5.0 + 15.0 + 45.0 + 200.0) / 4.0));
    }

    #[test]
    fn nan_is_ignored() {
        let mut h = Histogram::new(vec![10.0]).unwrap();
        h.record(f64::NAN);
        assert_eq!(h.count(), 0);
    }

    #[test]
    fn quantile_interpolates_within_bucket() {
        // 100 samples uniformly across (0,10] land in the first bucket [0,10].
        let mut h = Histogram::new(vec![10.0, 20.0, 100.0]).unwrap();
        for i in 1..=100 {
            h.record(i as f64 / 10.0); // 0.1 .. 10.0, all <= 10
        }
        let (p50, over) = h.p50().unwrap();
        assert!(!over);
        // Median should be ~ halfway into the [0,10] bucket.
        assert!((p50 - 5.0).abs() < 1.0, "p50={p50}");
    }

    #[test]
    fn quantile_flags_overflow() {
        let mut h = Histogram::new(vec![10.0, 100.0]).unwrap();
        // Most samples above 100 → p95 overflows.
        for _ in 0..99 {
            h.record(500.0);
        }
        h.record(5.0);
        let (p95, over) = h.p95().unwrap();
        assert!(over);
        assert_eq!(p95, 100.0); // floored at the top bound
    }

    #[test]
    fn fraction_within_is_conservative() {
        let mut h = Histogram::new(vec![1.0, 2.0, 5.0, 100.0]).unwrap();
        for _ in 0..95 {
            h.record(0.5); // bucket <=1
        }
        for _ in 0..5 {
            h.record(50.0); // bucket <=100
        }
        // 95 of 100 are <= 5 (whole buckets <=1, <=2, <=5).
        assert_eq!(h.fraction_within(5.0), Some(0.95));
    }

    #[test]
    fn slo_pass_when_under_budget() {
        let mut reg = SloRegistry::new();
        // Expand: 100 fast samples (~15ms) → p95 well under 100ms.
        for _ in 0..100 {
            reg.record(Slo::Expand, 15.0);
        }
        let s = reg.snapshot(Slo::Expand);
        assert!(s.measured);
        assert!(s.pass, "p95={} budget={}", s.p95, s.budget_p95);
        assert_eq!(s.budget_p95, 100.0);
    }

    #[test]
    fn slo_fail_when_over_budget() {
        let mut reg = SloRegistry::new();
        for _ in 0..100 {
            reg.record(Slo::CacheUpdate, 900.0); // way over 300ms
        }
        let s = reg.snapshot(Slo::CacheUpdate);
        assert!(s.measured);
        assert!(!s.pass);
    }

    #[test]
    fn unmeasured_slo_is_not_a_pass() {
        let reg = SloRegistry::new();
        for s in reg.snapshot_all() {
            assert!(!s.measured, "{} should be unmeasured", s.slo.id());
            assert!(!s.pass, "{} unmeasured must not pass", s.slo.id());
        }
    }

    #[test]
    fn all_slos_have_distinct_ids_and_budgets() {
        let ids: Vec<&str> = Slo::ALL.iter().map(|s| s.id()).collect();
        let mut uniq = ids.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(ids.len(), uniq.len());
        // Budgets match the CLAUDE.md / §7.1 table.
        assert_eq!(Slo::Expand.budget_p95(), 100.0);
        assert_eq!(Slo::IdleCpu.budget_p95(), 5.0);
    }

    #[test]
    fn ui_names_map_to_their_slos() {
        assert_eq!(Slo::from_ui_name("actions_present"), Some(Slo::ActionsPresented));
        assert_eq!(Slo::from_ui_name("local_search"), Some(Slo::Search));
        assert_eq!(Slo::from_ui_name("expand"), Some(Slo::Expand));
        assert_eq!(Slo::from_ui_name("first_token"), Some(Slo::FirstToken));
        assert_eq!(Slo::from_ui_name("cache_update"), Some(Slo::CacheUpdate));
        // Unknown names and the out-of-process CPU sample must not silently land in a histogram.
        assert_eq!(Slo::from_ui_name("idle_cpu"), None);
        assert_eq!(Slo::from_ui_name(""), None);
        assert_eq!(Slo::from_ui_name("Actions_Present"), None);
    }

    #[test]
    fn render_json_marks_measured_and_unmeasured() {
        let mut reg = SloRegistry::new();
        // one measured SLO (a fast expand), the rest unmeasured
        reg.record(Slo::Expand, 40.0);
        let json = render_snapshots_json(&reg.snapshot_all());
        assert!(json.starts_with(r#"{"metrics":["#));
        // the measured Expand SLO reports NFR-SLO-01, a sample, and passes (40 ≤ 100)
        assert!(json.contains(r#""slo":"NFR-SLO-01","count":1"#), "{json}");
        assert!(json.contains(r#""measured":true,"p95_overflowed":false"#));
        // an unmeasured SLO is measured:false and pass:false (silence ≠ success)
        assert!(json.contains(r#""slo":"NFR-SLO-04","count":0,"p50":0,"p95":0,"budget_p95":500,"pass":false,"measured":false"#), "{json}");
    }

    #[test]
    fn lesson_counters_render_measured_or_flagged_unmeasured() {
        let reg = SloRegistry::new();
        let snaps = reg.snapshot_all();
        // computable counters render with measured:true
        let json = render_snapshots_json_with_lessons(
            &snaps,
            Some(LessonCounters { active_lessons: 3, feedback_events_last_7d: 12 }),
        );
        assert!(json.contains(r#""lessons":{"active_lessons":3,"feedback_events_last_7d":12,"measured":true}"#), "{json}");
        assert!(json.starts_with(r#"{"metrics":["#));
        // not computable → measured:false, never a fabricated zero
        let json = render_snapshots_json_with_lessons(&snaps, None);
        assert!(json.contains(r#""lessons":{"measured":false}"#), "{json}");
        assert!(!json.contains("active_lessons"), "{json}");
    }
}
