//! How we measure.
//!
//! Percentiles come from [`spike_harness::stats::Percentiles`] rather than being recomputed here.
//! That crate is the designated carry-forward measurement asset and its nearest-rank
//! implementation is already unit-tested; a second percentile function in the tree would be a
//! second definition of p95, and the first time the two disagreed nobody would know which report
//! to believe.
//!
//! Individual samples are retained, not just their summary. A mean write latency of 2ms and a p99
//! of 400ms are the same mean, and only one of them is a product that feels broken.

use std::collections::HashMap;

use serde::Serialize;
use spike_harness::stats::Percentiles;

/// A latency sample set, kept in full so any percentile can be recomputed later.
#[derive(Debug, Default, Clone)]
pub struct LatencySeries {
    samples_ms: Vec<f64>,
}

impl LatencySeries {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_ns(&mut self, ns: u64) {
        self.samples_ms.push(ns as f64 / 1_000_000.0);
    }

    pub fn len(&self) -> usize {
        self.samples_ms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples_ms.is_empty()
    }

    pub fn samples_ms(&self) -> &[f64] {
        &self.samples_ms
    }

    /// `None` for an empty set — a percentile of nothing is not zero, and a report that printed
    /// `p95: 0.0ms` for a stage that never ran would read as a pass.
    pub fn percentiles(&self) -> Option<Percentiles> {
        Percentiles::of(&self.samples_ms)
    }

    pub fn mean_ms(&self) -> Option<f64> {
        if self.samples_ms.is_empty() {
            return None;
        }
        Some(self.samples_ms.iter().sum::<f64>() / self.samples_ms.len() as f64)
    }

    pub fn min_ms(&self) -> Option<f64> {
        self.samples_ms
            .iter()
            .copied()
            .fold(None, |acc: Option<f64>, x| {
                Some(match acc {
                    Some(a) => a.min(x),
                    None => x,
                })
            })
    }
}

/// Serializable latency summary. Mirrors [`Percentiles`] and adds the two fields it does not
/// carry, so a report never has to reach for the raw samples to say something ordinary.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct LatencySummary {
    pub n: usize,
    pub min_ms: f64,
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

impl LatencySummary {
    pub fn of(series: &LatencySeries) -> Option<Self> {
        let p = series.percentiles()?;
        Some(Self {
            n: p.n,
            min_ms: series.min_ms().unwrap_or(p.p50),
            mean_ms: series.mean_ms().unwrap_or(p.p50),
            p50_ms: p.p50,
            p95_ms: p.p95,
            p99_ms: p.p99,
            max_ms: p.max,
        })
    }
}

/// Ingest-side counters.
#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct WriteStats {
    /// Events handed to the backend.
    pub submitted: u64,
    /// Writes the backend absorbed into an existing row.
    pub deduplicated: u64,
    /// Merges that landed on a row carrying a *different* fact — information destroyed, not
    /// deduplicated (issue #221). The workload knows which fact each event carries, so the runner
    /// can check every reported merge against the row's owner. Always 0 for an honest backend;
    /// any non-zero value disqualifies the collapse rate as a score.
    pub wrong_merges: u64,
    /// Writes that returned an error.
    pub failed: u64,
    /// Rows in the log afterwards.
    pub rows_after: i64,
    /// Distinct facts the workload actually contained.
    pub unique_facts: u64,
    /// Repeat events the workload contained — what a perfect deduplicator would collapse.
    pub duplicate_events: u64,
}

impl WriteStats {
    /// Rows held per distinct fact. 1.0 is perfect; 2.0 means the store is twice the size the
    /// information in it requires.
    ///
    /// This is the number that motivates the whole research direction, which is why it is derived
    /// from `rows_after` (what the database really holds) rather than from the number of writes
    /// we submitted.
    pub fn write_amplification(&self) -> Option<f64> {
        if self.unique_facts == 0 {
            return None;
        }
        Some(self.rows_after as f64 / self.unique_facts as f64)
    }

    /// Of the repeats the workload contained, the share the backend recognised **correctly**.
    ///
    /// Only merges onto a row carrying the same fact count. A backend that combined two different
    /// memories would otherwise be rewarded for destroying information (issue #221): collapsing
    /// "Mom likes tea" into "Mom is allergic to tea" must never raise this number. Wrong merges
    /// are excluded from the numerator and reported separately as [`WriteStats::wrong_merges`].
    ///
    /// `None` when the workload contained no repeats — in a clean corpus this metric has no
    /// denominator, and reporting 0% would suggest a failure where there was nothing to detect.
    pub fn duplicate_collapse_rate(&self) -> Option<f64> {
        if self.duplicate_events == 0 {
            return None;
        }
        let correct = self.deduplicated.saturating_sub(self.wrong_merges);
        Some(correct as f64 / self.duplicate_events as f64)
    }
}

/// Retrieval quality over the query set.
///
/// Recall and MRR are computed the same way [`shogun-memory`'s `retrieval_eval`] test computes
/// them, deliberately: two different definitions of recall@5 in one repository would make the
/// scale numbers and the quality numbers incomparable.
#[derive(Debug, Default, Clone)]
pub struct QualityAccumulator {
    /// 1-indexed rank of the first correct answer per query; `None` when none was returned.
    first_correct_rank: Vec<Option<usize>>,
    /// Queries where a superseded fact appeared anywhere in the returned window.
    stale_returned: u64,
    /// Queries where a superseded fact outranked every correct one — an actively wrong answer,
    /// not merely a stale row sitting further down the list.
    stale_outranked_current: u64,
    /// Queries whose expectations included at least one superseded fact (the denominator for the
    /// two counters above).
    temporal_queries: u64,
    failed: u64,
}

impl QualityAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one query's outcome.
    ///
    /// `returned` is the ranked list of event ids; `fact_of` maps a row back to the fact it
    /// carries, which is how a duplicate row still counts as having found the answer.
    pub fn record(
        &mut self,
        returned: &[i64],
        fact_of: &HashMap<i64, String>,
        expected: &[String],
        superseded: &[String],
    ) {
        let rank_of = |set: &[String]| -> Option<usize> {
            returned.iter().enumerate().find_map(|(i, id)| {
                let fact = fact_of.get(id)?;
                set.iter().any(|e| e == fact).then_some(i + 1)
            })
        };

        let correct = rank_of(expected);
        self.first_correct_rank.push(correct);

        if !superseded.is_empty() {
            self.temporal_queries += 1;
            if let Some(stale_rank) = rank_of(superseded) {
                self.stale_returned += 1;
                if correct.map_or(true, |c| stale_rank < c) {
                    self.stale_outranked_current += 1;
                }
            }
        }
    }

    pub fn record_failure(&mut self) {
        self.failed += 1;
    }

    pub fn queries(&self) -> usize {
        self.first_correct_rank.len()
    }

    /// Share of queries whose answer appeared in the top `k`.
    pub fn recall_at(&self, k: usize) -> Option<f64> {
        if self.first_correct_rank.is_empty() {
            return None;
        }
        let hits = self
            .first_correct_rank
            .iter()
            .filter(|r| matches!(r, Some(rank) if *rank <= k))
            .count();
        Some(hits as f64 / self.first_correct_rank.len() as f64)
    }

    /// Mean reciprocal rank — sensitive to how far down the answer sits, which recall@k is not.
    pub fn mrr(&self) -> Option<f64> {
        if self.first_correct_rank.is_empty() {
            return None;
        }
        let total: f64 = self
            .first_correct_rank
            .iter()
            .map(|r| r.map_or(0.0, |rank| 1.0 / rank as f64))
            .sum();
        Some(total / self.first_correct_rank.len() as f64)
    }

    /// Summarise, given the `k` results each query actually requested.
    ///
    /// A recall@k the run never measured is `None`, not a relabeled smaller recall: with `--k 1`
    /// the returned lists are one row long, so "recall@5" would be recall@1 wearing the wrong
    /// name (issue #221).
    pub fn summary(&self, retrieval_k: usize) -> QualitySummary {
        let recall_if_measured =
            |k: usize| -> Option<f64> { (retrieval_k >= k).then(|| self.recall_at(k)).flatten() };
        QualitySummary {
            queries: self.queries(),
            failed: self.failed,
            recall_at_1: recall_if_measured(1),
            recall_at_5: recall_if_measured(5),
            recall_at_10: recall_if_measured(10),
            mrr: self.mrr(),
            temporal_queries: self.temporal_queries,
            stale_returned: self.stale_returned,
            stale_outranked_current: self.stale_outranked_current,
            stale_rate: (self.temporal_queries > 0)
                .then(|| self.stale_returned as f64 / self.temporal_queries as f64),
            stale_outranked_rate: (self.temporal_queries > 0)
                .then(|| self.stale_outranked_current as f64 / self.temporal_queries as f64),
        }
    }
}

/// Serializable form of [`QualityAccumulator`]. Every rate is `Option` so a workload that cannot
/// express a metric reports `null` rather than a number that looks like a measurement.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct QualitySummary {
    pub queries: usize,
    pub failed: u64,
    pub recall_at_1: Option<f64>,
    pub recall_at_5: Option<f64>,
    pub recall_at_10: Option<f64>,
    pub mrr: Option<f64>,
    pub temporal_queries: u64,
    pub stale_returned: u64,
    pub stale_outranked_current: u64,
    pub stale_rate: Option<f64>,
    pub stale_outranked_rate: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact_map(pairs: &[(i64, &str)]) -> HashMap<i64, String> {
        pairs.iter().map(|(id, f)| (*id, f.to_string())).collect()
    }

    #[test]
    fn empty_series_reports_nothing_rather_than_zero() {
        let s = LatencySeries::new();
        assert!(s.percentiles().is_none());
        assert!(LatencySummary::of(&s).is_none());
    }

    #[test]
    fn latency_summary_matches_known_percentiles() {
        let mut s = LatencySeries::new();
        for i in 1..=100 {
            s.record_ns(i * 1_000_000); // 1ms .. 100ms
        }
        let sum = LatencySummary::of(&s).expect("summary");
        assert_eq!(sum.n, 100);
        assert_eq!(sum.p50_ms, 50.0);
        assert_eq!(sum.p95_ms, 95.0);
        assert_eq!(sum.p99_ms, 99.0);
        assert_eq!(sum.max_ms, 100.0);
        assert_eq!(sum.min_ms, 1.0);
        assert!((sum.mean_ms - 50.5).abs() < 1e-9);
    }

    #[test]
    fn write_amplification_is_rows_over_facts() {
        let w = WriteStats {
            rows_after: 1500,
            unique_facts: 1000,
            ..Default::default()
        };
        assert_eq!(w.write_amplification(), Some(1.5));
    }

    #[test]
    fn amplification_and_collapse_are_none_without_a_denominator() {
        let w = WriteStats::default();
        assert!(w.write_amplification().is_none());
        assert!(w.duplicate_collapse_rate().is_none());
    }

    #[test]
    fn wrong_merges_never_raise_the_collapse_rate() {
        // Issue #221: a backend that combines two *different* memories destroys information and
        // must not be scored as if it deduplicated. 150 reported merges of which 100 were wrong →
        // only the 50 correct ones count.
        let w = WriteStats {
            deduplicated: 150,
            wrong_merges: 100,
            duplicate_events: 300,
            ..Default::default()
        };
        assert!((w.duplicate_collapse_rate().unwrap() - 50.0 / 300.0).abs() < 1e-12);
        // Pathological accounting (more wrong than reported) saturates at zero, never underflows.
        let w = WriteStats {
            deduplicated: 10,
            wrong_merges: 999,
            duplicate_events: 300,
            ..Default::default()
        };
        assert_eq!(w.duplicate_collapse_rate(), Some(0.0));
    }

    #[test]
    fn recall_at_k_never_measured_is_null_not_a_relabel() {
        // Issue #221: with --k 1 the returned lists are one row long, so "recall@5" would just be
        // recall@1 wearing the wrong name. It must come back None.
        let mut q = QualityAccumulator::new();
        let mut fact_of = HashMap::new();
        fact_of.insert(1_i64, "f-a".to_string());
        q.record(&[1], &fact_of, &["f-a".to_string()], &[]);
        let s = q.summary(1);
        assert_eq!(s.recall_at_1, Some(1.0));
        assert!(s.recall_at_5.is_none(), "k=1 never measured the top five");
        assert!(s.recall_at_10.is_none(), "k=1 never measured the top ten");
        let s = q.summary(5);
        assert_eq!(s.recall_at_5, Some(1.0));
        assert!(s.recall_at_10.is_none());
    }

    #[test]
    fn collapse_rate_counts_recognised_repeats() {
        let w = WriteStats {
            deduplicated: 150,
            duplicate_events: 300,
            ..Default::default()
        };
        assert_eq!(w.duplicate_collapse_rate(), Some(0.5));
    }

    #[test]
    fn recall_and_mrr_track_the_rank_of_the_answer() {
        let facts = fact_map(&[(1, "a"), (2, "b"), (3, "c")]);
        let mut q = QualityAccumulator::new();
        // Answer at rank 1.
        q.record(&[1, 2, 3], &facts, &["a".into()], &[]);
        // Answer at rank 3.
        q.record(&[1, 2, 3], &facts, &["c".into()], &[]);
        assert_eq!(q.recall_at(1), Some(0.5));
        assert_eq!(q.recall_at(5), Some(1.0));
        // (1/1 + 1/3) / 2
        let mrr = q.mrr().expect("mrr");
        assert!((mrr - (1.0 + 1.0 / 3.0) / 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_missed_answer_scores_zero_not_none() {
        let facts = fact_map(&[(1, "a")]);
        let mut q = QualityAccumulator::new();
        q.record(&[1], &facts, &["missing".into()], &[]);
        assert_eq!(q.recall_at(10), Some(0.0));
        assert_eq!(q.mrr(), Some(0.0));
    }

    #[test]
    fn stale_is_counted_separately_from_outranking() {
        let facts = fact_map(&[(1, "old"), (2, "new")]);
        let mut q = QualityAccumulator::new();
        // Current answer first, stale below it: returned, but not outranking.
        q.record(&[2, 1], &facts, &["new".into()], &["old".into()]);
        // Stale first: actively wrong.
        q.record(&[1, 2], &facts, &["new".into()], &["old".into()]);
        let s = q.summary(10);
        assert_eq!(s.temporal_queries, 2);
        assert_eq!(s.stale_returned, 2);
        assert_eq!(s.stale_outranked_current, 1);
        assert_eq!(s.stale_rate, Some(1.0));
        assert_eq!(s.stale_outranked_rate, Some(0.5));
    }

    #[test]
    fn stale_outranks_when_the_current_answer_is_absent_entirely() {
        let facts = fact_map(&[(1, "old")]);
        let mut q = QualityAccumulator::new();
        q.record(&[1], &facts, &["new".into()], &["old".into()]);
        let s = q.summary(10);
        assert_eq!(s.stale_outranked_current, 1);
    }

    #[test]
    fn a_clean_workload_reports_null_staleness_not_zero() {
        let facts = fact_map(&[(1, "a")]);
        let mut q = QualityAccumulator::new();
        q.record(&[1], &facts, &["a".into()], &[]);
        let s = q.summary(10);
        assert_eq!(s.temporal_queries, 0);
        assert!(s.stale_rate.is_none());
    }
}
