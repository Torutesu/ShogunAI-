//! What we save.
//!
//! One JSON object per run, carrying the result *and* everything needed to reproduce or discount
//! it: the config, the git commit, the platform, and the build profile. Six weeks from now the
//! only useful comparison is `baseline@a83f91d` against `controller@c92a120`, and that comparison
//! is only possible if both runs recorded which commit they were.

use serde::Serialize;
use spike_harness::slo::{self, Verdict};

use crate::config::BenchConfig;
use crate::metrics::{LatencySummary, QualitySummary, WriteStats};
use crate::resources::ResourceSummary;

/// Where the run happened. Latency is meaningless without it.
#[derive(Debug, Clone, Serialize)]
pub struct Environment {
    pub os: String,
    pub arch: String,
    /// `git rev-parse HEAD`, or `null` outside a checkout.
    pub git_commit: Option<String>,
    /// `true` when the working tree had uncommitted changes — the result then belongs to no commit
    /// at all, and treating it as a baseline for `git_commit` would be wrong.
    pub git_dirty: Option<bool>,
    /// `"debug"` or `"release"`. A debug-build latency figure is roughly an order of magnitude off
    /// and must never be quoted against an SLO; `docs/phase1-implementation-plan.md` requires SLO
    /// measurement on release builds.
    pub profile: &'static str,
}

impl Environment {
    pub fn detect() -> Self {
        let git = |args: &[&str]| -> Option<String> {
            // The launch directory may be /tmp, another checkout, or a GUI process directory. The
            // source tree that Cargo compiled is the only repository whose SHA describes this bin.
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(env!("CARGO_MANIFEST_DIR"))
                .args(args)
                .output()
                .ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        };
        let git_commit = git(&["rev-parse", "HEAD"]).filter(|s| !s.is_empty());
        let git_dirty = git(&["status", "--porcelain"]).map(|s| !s.trim().is_empty());
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            git_commit,
            git_dirty,
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        }
    }
}

/// How the run was configured beyond the knobs the user set — the facts that decide what the
/// numbers can be compared to.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RunMode {
    /// `false` means the semantic half of hybrid search contributed nothing (no embedder), so
    /// retrieval was lexical-only. Lexical and hybrid numbers are not comparable.
    pub semantic: bool,
    /// `true` means the database never touched a filesystem, so write latency excludes fsync.
    pub in_memory: bool,
}

/// Storage growth over the run.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct StorageReport {
    pub initial_logical_bytes: u64,
    pub final_logical_bytes: u64,
    pub final_file_bytes: Option<u64>,
    /// Bytes of database per row held. The unit that scales.
    pub bytes_per_row: Option<f64>,
}

/// The SLO this run can speak to, and whether it met it.
///
/// Only one applies: [`slo::LOCAL_SEARCH_MS`], whose in-tree comment reads "Not exercised in
/// Phase 0; kept for Phase 1." This is the bench that exercises it. The verdict is advisory —
/// `docs/phase1-implementation-plan.md` is explicit that a real SLO pass is confirmed with
/// on-device macOS numbers on a release build, so a green verdict from Linux CI is a regression
/// signal, not a certification.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SloCheck {
    pub name: &'static str,
    pub threshold_ms: f64,
    pub measured_p95_ms: f64,
    pub verdict: Verdict,
    /// `false` whenever the run's mode disqualifies it from certifying the SLO.
    pub authoritative: bool,
}

/// Machine-readable reasons this run must not be used as evidence of an improvement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Disqualification {
    WrongMerges,
    WriteFailures,
    QueryFailures,
    UnknownGitCommit,
    UnknownGitState,
    DirtyWorkingTree,
    InsufficientQuerySamples,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MeasurementValidity {
    pub valid: bool,
    pub disqualifications: Vec<Disqualification>,
}

impl MeasurementValidity {
    pub fn assess(
        writes: &WriteStats,
        quality: &QualitySummary,
        environment: &Environment,
        workload: &str,
    ) -> Self {
        let mut disqualifications = Vec::new();
        if writes.wrong_merges > 0 {
            disqualifications.push(Disqualification::WrongMerges);
        }
        if writes.failed > 0 {
            disqualifications.push(Disqualification::WriteFailures);
        }
        if quality.failed > 0 {
            disqualifications.push(Disqualification::QueryFailures);
        }
        if environment.git_commit.is_none() {
            disqualifications.push(Disqualification::UnknownGitCommit);
        }
        match environment.git_dirty {
            None => disqualifications.push(Disqualification::UnknownGitState),
            Some(true) => disqualifications.push(Disqualification::DirtyWorkingTree),
            Some(false) => {}
        }
        let minimum_queries = if workload == "temporal" {
            crate::workloads::temporal_project_count()
        } else {
            crate::config::MIN_PERCENTILE_SAMPLES
        };
        if quality.queries < minimum_queries {
            disqualifications.push(Disqualification::InsufficientQuerySamples);
        }
        Self {
            valid: disqualifications.is_empty(),
            disqualifications,
        }
    }
}

/// The complete artifact.
#[derive(Debug, Clone, Serialize)]
pub struct BenchReport {
    pub benchmark: &'static str,
    pub version: &'static str,
    pub config: BenchConfig,
    /// Events the generator actually produced (padded/clamped, so normally == `config.events`).
    pub generated_events: usize,
    /// Queries the generator actually produced. `clean` plants at most one answer per event, so
    /// this can be smaller than `config.queries`; the console warns when they differ (issue #221).
    pub generated_queries: usize,
    pub environment: Environment,
    pub mode: RunMode,
    pub backend: &'static str,
    /// Explicit validity gate for automated comparison. Consumers must require `valid == true`.
    pub validity: MeasurementValidity,
    pub writes: WriteStats,
    /// `null` when no writes were measured.
    pub write_latency: Option<LatencySummary>,
    pub query_latency: Option<LatencySummary>,
    pub quality: QualitySummary,
    pub resources: Option<ResourceSummary>,
    pub storage: StorageReport,
    pub slo: Option<SloCheck>,
    /// Derived, so a reader does not have to divide two fields to get the headline number.
    pub write_amplification: Option<f64>,
    pub duplicate_collapse_rate: Option<f64>,
    pub wall_seconds: f64,
}

/// Bench identity, recorded so a report file names its own producer.
pub const BENCHMARK_NAME: &str = "shogun-memory-bench";
/// Report schema version. Bump when a field changes meaning, so old reports are not silently
/// re-read under new semantics.
pub const REPORT_VERSION: &str = "0.4";

impl BenchReport {
    /// Build the SLO check from the query-latency summary, if there is one.
    pub fn slo_for(
        query_latency: Option<LatencySummary>,
        mode: RunMode,
        profile: &str,
        measurement_valid: bool,
    ) -> Option<SloCheck> {
        let q = query_latency?;
        if q.n < crate::config::MIN_PERCENTILE_SAMPLES {
            return None;
        }
        Some(SloCheck {
            name: "NFR-SLO-04 local search p95",
            threshold_ms: slo::LOCAL_SEARCH_MS,
            measured_p95_ms: q.p95_ms,
            verdict: Verdict::le(q.p95_ms, slo::LOCAL_SEARCH_MS),
            // A debug build, an in-memory database, or a lexical-only run each individually make
            // this number the wrong one to certify against.
            authoritative: measurement_valid
                && profile == "release"
                && !mode.in_memory
                && mode.semantic,
        })
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn opt_pct(v: Option<f64>) -> String {
    v.map_or_else(|| "n/a".to_string(), |x| format!("{:.1}%", x * 100.0))
}

fn opt_num(v: Option<f64>, places: usize) -> String {
    v.map_or_else(|| "n/a".to_string(), |x| format!("{x:.places$}"))
}

/// The human summary. Deliberately prints `n/a` rather than `0` wherever a metric had no
/// denominator — a benchmark that reports a confident zero for something it did not measure is
/// worse than one that reports nothing.
pub fn render_summary(r: &BenchReport) -> String {
    let mut s = String::new();
    s.push_str("\nSHOGUN MEMORY BENCHMARK\n");
    s.push_str("────────────────────────────────\n\n");

    s.push_str(&format!(
        "Workload\n  Name:             {}\n  Seed:             {}\n  Events:           {}\n  Queries:          {}\n",
        r.config.workload, r.config.seed, r.config.events, r.config.queries
    ));
    s.push_str(&format!(
        "  Retrieval:        {}\n  Storage:          {}\n  Profile:          {}\n\n",
        if r.mode.semantic {
            "hybrid (FTS + vector)"
        } else {
            "lexical only (no embedder)"
        },
        if r.mode.in_memory {
            "in-memory (no fsync)"
        } else {
            "on-disk (WAL)"
        },
        r.environment.profile
    ));

    s.push_str(&format!(
        "Writes\n  Submitted:        {}\n  Deduplicated:     {}\n  Failed:           {}\n  Rows held:        {}\n",
        r.writes.submitted, r.writes.deduplicated, r.writes.failed, r.writes.rows_after
    ));
    match r.write_latency {
        Some(w) => s.push_str(&format!(
            "  P50:              {:.2} ms\n  P95:              {:.2} ms\n  P99:              {:.2} ms\n\n",
            w.p50_ms, w.p95_ms, w.p99_ms
        )),
        None => s.push_str("  Latency:          n/a (no writes measured)\n\n"),
    }

    s.push_str(&format!(
        "Retrieval\n  Queries:          {}\n",
        r.quality.queries
    ));
    if r.generated_queries != r.config.queries {
        s.push_str(&format!(
            "  NOTE:             {} queries requested, workload produced {} — percentiles and \
recall rest on the smaller number\n",
            r.config.queries, r.generated_queries
        ));
    }
    match r.query_latency {
        Some(q) => s.push_str(&format!(
            "  P50:              {:.2} ms\n  P95:              {:.2} ms\n  P99:              {:.2} ms\n  Max:              {:.2} ms\n\n",
            q.p50_ms, q.p95_ms, q.p99_ms, q.max_ms
        )),
        None => s.push_str("  Latency:          n/a (no queries measured)\n\n"),
    }

    s.push_str(&format!(
        "Quality\n  Recall@1:         {}\n  Recall@5:         {}\n  Recall@10:        {}\n  MRR:              {}\n",
        opt_num(r.quality.recall_at_1, 3),
        opt_num(r.quality.recall_at_5, 3),
        opt_num(r.quality.recall_at_10, 3),
        opt_num(r.quality.mrr, 3)
    ));
    s.push_str(&format!(
        "  Write amp:        {}\n  Dup collapse:     {}\n",
        opt_num(r.write_amplification, 3),
        opt_pct(r.duplicate_collapse_rate)
    ));
    if r.writes.wrong_merges > 0 {
        s.push_str(&format!(
            "  WRONG MERGES:     {} — the backend combined events carrying different facts; \
all write-improvement scores are invalid\n",
            r.writes.wrong_merges
        ));
    }
    s.push_str(&format!(
        "  Stale returned:   {}\n  Stale outranking: {}\n\n",
        opt_pct(r.quality.stale_rate),
        opt_pct(r.quality.stale_outranked_rate)
    ));

    s.push_str("Resources\n");
    match r.resources {
        Some(res) => {
            s.push_str(&format!(
                "  Peak RSS:         {:.1} MB (sampled, n={})\n  Mean CPU:         {}\n",
                mb(res.peak_rss_bytes),
                res.samples,
                res.mean_cpu_pct
                    .map_or_else(|| "n/a".to_string(), |c| format!("{c:.1}% over the run")),
            ));
        }
        None => s.push_str("  n/a (no reader on this platform)\n"),
    }
    s.push_str(&format!(
        "  DB size:          {:.2} MB\n  Bytes/row:        {}\n\n",
        mb(r.storage.final_logical_bytes),
        opt_num(r.storage.bytes_per_row, 1)
    ));

    if let Some(slo_check) = r.slo {
        s.push_str(&format!(
            "SLO\n  {}: {:.2} ms vs {:.0} ms → {:?}{}\n\n",
            slo_check.name,
            slo_check.measured_p95_ms,
            slo_check.threshold_ms,
            slo_check.verdict,
            if slo_check.authoritative {
                ""
            } else {
                "  (advisory — not a certifying run)"
            }
        ));
    } else if let Some(query_latency) = r.query_latency {
        if query_latency.n < crate::config::MIN_PERCENTILE_SAMPLES {
            s.push_str(&format!(
                "SLO\n  n/a:              {} query samples; at least {} required for p95\n\n",
                query_latency.n,
                crate::config::MIN_PERCENTILE_SAMPLES
            ));
        }
    }

    let validity = if r.validity.valid {
        "valid".to_string()
    } else {
        format!("INVALID ({:?})", r.validity.disqualifications)
    };
    s.push_str(&format!(
        "Run\n  Validity:         {validity}\n  Commit:           {}{}\n  Platform:         {}/{}\n  Wall:             {:.1} s\n",
        r.environment.git_commit.as_deref().unwrap_or("unknown"),
        match r.environment.git_dirty {
            Some(true) => " (dirty)",
            _ => "",
        },
        r.environment.os,
        r.environment.arch,
        r.wall_seconds
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean_environment() -> Environment {
        Environment {
            os: "test".to_string(),
            arch: "test".to_string(),
            git_commit: Some("abc123".to_string()),
            git_dirty: Some(false),
            profile: "release",
        }
    }

    fn quality_with_queries(queries: usize) -> QualitySummary {
        QualitySummary {
            queries,
            failed: 0,
            recall_at_1: None,
            recall_at_5: None,
            recall_at_10: None,
            mrr: None,
            temporal_queries: 0,
            stale_returned: 0,
            stale_outranked_current: 0,
            stale_rate: None,
            stale_outranked_rate: None,
        }
    }

    #[test]
    fn opt_helpers_say_na_not_zero() {
        assert_eq!(opt_pct(None), "n/a");
        assert_eq!(opt_num(None, 3), "n/a");
        assert_eq!(opt_pct(Some(0.5)), "50.0%");
        assert_eq!(opt_num(Some(1.5), 2), "1.50");
    }

    #[test]
    fn slo_is_not_authoritative_from_a_debug_in_memory_lexical_run() {
        let q = LatencySummary {
            n: crate::config::MIN_PERCENTILE_SAMPLES,
            min_ms: 1.0,
            mean_ms: 1.0,
            p50_ms: 1.0,
            p95_ms: 1.0,
            p99_ms: 1.0,
            max_ms: 1.0,
        };
        let mode = RunMode {
            semantic: false,
            in_memory: true,
        };
        let check = BenchReport::slo_for(Some(q), mode, "debug", true).expect("check");
        assert!(check.verdict.is_pass(), "1ms is under the 500ms threshold");
        assert!(
            !check.authoritative,
            "a fast number from a disqualified run must not certify"
        );
    }

    #[test]
    fn slo_is_authoritative_only_when_every_condition_holds() {
        let q = LatencySummary {
            n: crate::config::MIN_PERCENTILE_SAMPLES,
            min_ms: 1.0,
            mean_ms: 1.0,
            p50_ms: 1.0,
            p95_ms: 1.0,
            p99_ms: 1.0,
            max_ms: 1.0,
        };
        let mode = RunMode {
            semantic: true,
            in_memory: false,
        };
        let check = BenchReport::slo_for(Some(q), mode, "release", true).expect("check");
        assert!(check.authoritative);
    }

    #[test]
    fn a_slow_run_fails_the_threshold() {
        let q = LatencySummary {
            n: crate::config::MIN_PERCENTILE_SAMPLES,
            min_ms: 600.0,
            mean_ms: 600.0,
            p50_ms: 600.0,
            p95_ms: 600.0,
            p99_ms: 600.0,
            max_ms: 600.0,
        };
        let mode = RunMode {
            semantic: true,
            in_memory: false,
        };
        let check = BenchReport::slo_for(Some(q), mode, "release", true).expect("check");
        assert!(!check.verdict.is_pass());
    }

    #[test]
    fn no_queries_means_no_slo_claim() {
        let mode = RunMode {
            semantic: true,
            in_memory: false,
        };
        assert!(BenchReport::slo_for(None, mode, "release", true).is_none());
    }

    #[test]
    fn one_query_cannot_produce_an_slo_verdict() {
        let q = LatencySummary {
            n: 1,
            min_ms: 1.0,
            mean_ms: 1.0,
            p50_ms: 1.0,
            p95_ms: 1.0,
            p99_ms: 1.0,
            max_ms: 1.0,
        };
        let mode = RunMode {
            semantic: true,
            in_memory: false,
        };

        assert!(BenchReport::slo_for(Some(q), mode, "release", true).is_none());
    }

    #[test]
    fn invalid_measurement_can_never_certify_an_slo() {
        let q = LatencySummary {
            n: crate::config::MIN_PERCENTILE_SAMPLES,
            min_ms: 1.0,
            mean_ms: 1.0,
            p50_ms: 1.0,
            p95_ms: 1.0,
            p99_ms: 1.0,
            max_ms: 1.0,
        };
        let mode = RunMode {
            semantic: true,
            in_memory: false,
        };
        let check = BenchReport::slo_for(Some(q), mode, "release", false).expect("check");
        assert!(!check.authoritative);
    }

    #[test]
    fn validity_names_every_backend_integrity_failure() {
        let writes = WriteStats {
            wrong_merges: 1,
            failed: 2,
            ..Default::default()
        };
        let quality = QualitySummary {
            failed: 3,
            ..quality_with_queries(crate::config::MIN_PERCENTILE_SAMPLES)
        };
        let validity =
            MeasurementValidity::assess(&writes, &quality, &clean_environment(), "clean");
        assert_eq!(
            validity.disqualifications,
            vec![
                Disqualification::WrongMerges,
                Disqualification::WriteFailures,
                Disqualification::QueryFailures,
            ]
        );
    }

    #[test]
    fn unknown_commit_disqualifies_measurement() {
        let environment = Environment {
            git_commit: None,
            ..clean_environment()
        };
        let validity = MeasurementValidity::assess(
            &WriteStats::default(),
            &quality_with_queries(crate::config::MIN_PERCENTILE_SAMPLES),
            &environment,
            "clean",
        );

        assert!(validity
            .disqualifications
            .contains(&Disqualification::UnknownGitCommit));
    }

    #[test]
    fn dirty_tree_disqualifies_measurement() {
        let environment = Environment {
            git_dirty: Some(true),
            ..clean_environment()
        };
        let validity = MeasurementValidity::assess(
            &WriteStats::default(),
            &quality_with_queries(crate::config::MIN_PERCENTILE_SAMPLES),
            &environment,
            "clean",
        );

        assert!(validity
            .disqualifications
            .contains(&Disqualification::DirtyWorkingTree));
    }

    #[test]
    fn one_query_disqualifies_measurement() {
        let validity = MeasurementValidity::assess(
            &WriteStats::default(),
            &quality_with_queries(1),
            &clean_environment(),
            "clean",
        );

        assert!(validity
            .disqualifications
            .contains(&Disqualification::InsufficientQuerySamples));
    }
}
