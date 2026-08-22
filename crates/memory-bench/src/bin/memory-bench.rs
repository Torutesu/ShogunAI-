//! `memory-bench` — run a SHOGUN memory benchmark and print/save the report.
//!
//! Argument parsing is hand-rolled, matching `shogun-cli`: this workspace carries no `clap`, and a
//! benchmark is not the place to introduce a dependency to the whole tree for six flags.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::process::ExitCode;

use memory_bench::config::{
    BenchConfig, DEFAULT_EVENTS, DEFAULT_K, DEFAULT_QUERIES, DEFAULT_WARMUP, DEFAULT_WRITE_BATCH,
};
use memory_bench::{report, runner};

const USAGE: &str = "\
memory-bench — SHOGUN memory ingestion and retrieval benchmark

USAGE:
    memory-bench [OPTIONS]

OPTIONS:
    --workload <NAME>   Corpus to generate. Default: clean
                          clean      unique events, one answer per query. The reference point;
                                     write amplification here should be 1.0.
                          duplicate  ~30% repeated facts, half verbatim and half reworded, to
                                     measure what the content_hash dedup does and does not catch.
                          temporal   facts overwritten by later facts; questions are present-tense,
                                     so retrieving the old value is measurably wrong.
    --events <N>        Events to ingest. Default: 100000 (the scale M2 states the search SLO at)
    --queries <N>       Queries to run. Default: 500
    --k <N>             Results requested per query. Default: 10
    --warmup <N>        Unmeasured queries before measurement. Default: 20
    --seed <N>          Workload seed. Default: 42. Same seed = same corpus, forever.
    --db <PATH>         Private SQLite scratch directory to create. Default: in-memory.
                        PATH must NOT exist. The benchmark claims that directory atomically and
                        creates memory-bench.sqlite plus WAL/SHM inside it. It never opens PATH as
                        a database, reuses a directory, or resets caller-owned storage.
                        Quote latency numbers only from a file-backed run — in-memory writes
                        never touch a filesystem and skip fsync entirely.
    --out <DIR>         Write the JSON report into DIR. Default: print the summary only.
    --write-batch <N>   Events per bulk-load transaction, at least 1. Default: 1000. Use 1 to
                        measure the true per-capture write cost including commit.
    -h, --help          Show this help.

REPRODUCIBILITY:
    A result is defined by (workload, seed, events, queries) plus the commit and build profile,
    all of which are recorded in the report. Compare runs only across matching modes: a debug
    build, an in-memory database, or a lexical-only retrieval path each disqualify a run from
    certifying an SLO, and the report marks it.

EXAMPLES:
    memory-bench --workload clean --events 100000 --queries 500 --seed 42 --db /tmp/bench-run --out reports
    memory-bench --workload duplicate --events 10000 --queries 200
    memory-bench --workload temporal --events 50000 --queries 12
";

/// Parse `--flag value` pairs. Returns the usage string as an error for anything malformed, so a
/// typo prints help rather than silently running a different benchmark than the one intended.
fn parse(args: &[String]) -> Result<Option<BenchConfig>, String> {
    let mut cfg = BenchConfig {
        workload: "clean".to_string(),
        seed: 42,
        events: DEFAULT_EVENTS,
        queries: DEFAULT_QUERIES,
        k: DEFAULT_K,
        warmup: DEFAULT_WARMUP,
        db_path: None,
        out_dir: None,
        write_batch: DEFAULT_WRITE_BATCH,
    };

    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        if flag == "-h" || flag == "--help" {
            return Ok(None);
        }
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("{flag} needs a value"))?;
        let num = |what: &str| -> Result<usize, String> {
            value
                .parse::<usize>()
                .map_err(|_| format!("{what} must be a number, got {value:?}"))
        };
        match flag {
            "--workload" => cfg.workload = value.clone(),
            "--events" => cfg.events = num("--events")?,
            "--queries" => cfg.queries = num("--queries")?,
            "--k" => cfg.k = num("--k")?,
            "--warmup" => cfg.warmup = num("--warmup")?,
            "--write-batch" => cfg.write_batch = num("--write-batch")?,
            "--seed" => {
                cfg.seed = value
                    .parse::<u64>()
                    .map_err(|_| format!("--seed must be a number, got {value:?}"))?
            }
            "--db" => cfg.db_path = Some(value.clone()),
            "--out" => cfg.out_dir = Some(value.clone()),
            other => return Err(format!("unknown flag {other:?}")),
        }
        i += 2;
    }

    cfg.validate().map_err(|error| error.to_string())?;
    Ok(Some(cfg))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfg = match parse(&args) {
        Ok(Some(c)) => c,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}\n");
            print!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    let out_dir = cfg.out_dir.clone();
    let result = match runner::run(&cfg) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("benchmark failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    print!("{}", report::render_summary(&result));

    if let Some(dir) = out_dir {
        match runner::persist(&result, &dir) {
            Ok(path) => println!("\nReport:\n  {}\n", path.display()),
            Err(e) => {
                eprintln!("could not write report: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults_match_the_documented_scale() {
        let cfg = parse(&[]).expect("parse").expect("config");
        assert_eq!(cfg.events, 100_000);
        assert_eq!(cfg.seed, 42);
        assert_eq!(cfg.workload, "clean");
    }

    #[test]
    fn flags_are_applied() {
        let cfg = parse(&args(&[
            "--workload",
            "duplicate",
            "--events",
            "10",
            "--seed",
            "7",
        ]))
        .expect("parse")
        .expect("config");
        assert_eq!(cfg.workload, "duplicate");
        assert_eq!(cfg.events, 10);
        assert_eq!(cfg.seed, 7);
    }

    #[test]
    fn help_returns_no_config() {
        assert!(parse(&args(&["--help"])).expect("parse").is_none());
    }

    #[test]
    fn an_unknown_workload_is_rejected_rather_than_defaulted() {
        let err = parse(&args(&["--workload", "nonsense"])).expect_err("should reject");
        assert!(err.contains("unknown workload"), "{err}");
    }

    #[test]
    fn unknown_flags_and_missing_values_are_errors() {
        assert!(parse(&args(&["--nope", "1"])).is_err());
        assert!(parse(&args(&["--events"])).is_err());
        assert!(parse(&args(&["--events", "abc"])).is_err());
    }

    #[test]
    fn zero_events_is_rejected() {
        assert!(parse(&args(&["--events", "0"])).is_err());
    }

    #[test]
    fn out_of_bounds_numbers_are_rejected_with_the_limit_named() {
        // Issue #221: agent-generated values can be absurd; the error names the accepted range.
        let err = parse(&args(&["--events", "10000001"])).expect_err("too many events");
        assert!(err.contains("between 1 and 10000000"), "{err}");
        assert!(parse(&args(&["--queries", "0"])).is_err());
        assert!(parse(&args(&["--queries", "1000001"])).is_err());
        assert!(parse(&args(&["--k", "0"])).is_err());
        assert!(parse(&args(&["--k", "1001"])).is_err());
        assert!(parse(&args(&["--warmup", "100001"])).is_err());
    }

    #[test]
    fn write_batch_zero_is_rejected_not_silently_clamped() {
        // Issue #221: execution used to clamp 0 to 1 while the report recorded 0 — the label and
        // the experiment disagreed. Now the value that parses is the value that runs.
        let err = parse(&args(&["--write-batch", "0"])).expect_err("reject zero");
        assert!(err.contains("--write-batch"), "{err}");
        let cfg = parse(&args(&["--write-batch", "1"]))
            .expect("parse")
            .expect("config");
        assert_eq!(cfg.write_batch, 1);
    }
}
