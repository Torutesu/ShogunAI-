//! Phase 0 result report generator (spec §4.6).
//!
//! Reads one or more JSONL metric files and emits the Go/No-Go Markdown report:
//! per-question verdicts against the SLO constants, layered p50/p95/p99 tables,
//! false-positive tallies, and record-gap notes. Verdicts are computed from
//! [`spike_harness::slo`] — the report never hardcodes a threshold.
//!
//! Usage: `cargo run -p spike-harness --bin report -- <a.jsonl> <b.jsonl> -o out.md`
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use serde_json::Value;
use spike_harness::slo;
use spike_harness::stats::Percentiles;
use std::io::Write;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut inputs: Vec<String> = Vec::new();
    let mut out_path: Option<String> = None;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" | "--out" => out_path = it.next(),
            "-h" | "--help" => {
                eprintln!("usage: report <files.jsonl...> [-o out.md]");
                return Ok(());
            }
            _ => inputs.push(a),
        }
    }
    if inputs.is_empty() {
        eprintln!("error: no input files. usage: report <files.jsonl...> [-o out.md]");
        std::process::exit(2);
    }

    let mut records: Vec<Value> = Vec::new();
    for path in &inputs {
        let content = std::fs::read_to_string(path)?;
        for (i, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(line) {
                Ok(v) => records.push(v),
                Err(e) => eprintln!("warn: {path}:{}: skipping unparseable line: {e}", i + 1),
            }
        }
    }

    let md = render_report(&records, &inputs);
    match out_path {
        Some(p) => {
            let mut f = std::fs::File::create(&p)?;
            f.write_all(md.as_bytes())?;
            eprintln!("wrote {p} ({} records)", records.len());
        }
        None => print!("{md}"),
    }
    Ok(())
}

fn of_type<'a>(records: &'a [Value], ty: &str) -> Vec<&'a Value> {
    records.iter().filter(|r| r["type"] == ty).collect()
}

fn f64_at(v: &Value, path: &[&str]) -> Option<f64> {
    let mut cur = v;
    for k in path {
        cur = cur.get(k)?;
    }
    cur.as_f64()
}

/// p50/p95/p99 line for a set of latency values, or a "no data" note.
fn pct_line(label: &str, values: &[f64], threshold: f64) -> String {
    match Percentiles::of(values) {
        Some(p) => {
            let verdict = slo::Verdict::le(p.p95, threshold);
            let mark = if verdict.is_pass() { "PASS" } else { "FAIL" };
            format!(
                "| {label} | {} | {:.1} | {:.1} | {:.1} | {:.1} | {mark} (≤{threshold:.0}) |",
                p.n, p.p50, p.p95, p.p99, p.max
            )
        }
        None => format!("| {label} | 0 | — | — | — | — | NO DATA |"),
    }
}

/// Split expand-latency values into the four (mode × fullscreen) layers (spec §4.3).
fn layer_expand(records: &[Value]) -> Vec<(String, Vec<f64>)> {
    let rows = of_type(records, "metric.expand_latency");
    let mut layers: Vec<(String, Vec<f64>)> = vec![
        ("notch / windowed".into(), vec![]),
        ("notch / fullscreen".into(), vec![]),
        ("pseudo / windowed".into(), vec![]),
        ("pseudo / fullscreen".into(), vec![]),
    ];
    for r in rows {
        let Some(lat) = f64_at(r, &["payload", "latency_ms"]) else { continue };
        let mode = r["payload"]["mode"].as_str().unwrap_or("");
        let fs = r["payload"]["fullscreen"].as_bool().unwrap_or(false);
        let idx = match (mode, fs) {
            ("notch", false) => 0,
            ("notch", true) => 1,
            ("pseudo", false) => 2,
            ("pseudo", true) => 3,
            _ => continue,
        };
        layers[idx].1.push(lat);
    }
    layers
}

fn render_report(records: &[Value], inputs: &[String]) -> String {
    let mut s = String::new();
    s.push_str("# SHOGUN Phase 0 — Spike Report\n\n");
    s.push_str(&format!("- Inputs: {}\n", inputs.join(", ")));
    s.push_str(&format!("- Total records: {}\n", records.len()));
    s.push_str(
        "- Verdicts use `spike_harness::slo` (expand ≤100ms, cache ≤300ms, idle CPU ≤5%).\n",
    );
    s.push_str("- Clock-offset / rAF calibration error is annotated per spec §4.1; see harness notes.\n\n");

    // --- Q2 expand latency ---
    s.push_str("## Q2 — Expand latency (SLO p95 ≤ 100ms)\n\n");
    s.push_str("| layer | n | p50 | p95 | p99 | max | verdict |\n|---|---|---|---|---|---|---|\n");
    let all_expand: Vec<f64> = of_type(records, "metric.expand_latency")
        .iter()
        .filter_map(|r| f64_at(r, &["payload", "latency_ms"]))
        .collect();
    s.push_str(&pct_line("ALL", &all_expand, slo::EXPAND_MS));
    s.push('\n');
    for (label, vals) in layer_expand(records) {
        s.push_str(&pct_line(&label, &vals, slo::EXPAND_MS));
        s.push('\n');
    }
    s.push('\n');

    // --- Q3-A cache update ---
    s.push_str("## Q3-A — Context cache update (SLO p95 ≤ 300ms)\n\n");
    let cache_rows = of_type(records, "metric.cache_update");
    let cache_vals: Vec<f64> = cache_rows
        .iter()
        .filter(|r| !r["payload"]["cancelled"].as_bool().unwrap_or(false))
        .filter_map(|r| f64_at(r, &["payload", "latency_ms"]))
        .collect();
    let partial_n = cache_rows
        .iter()
        .filter(|r| r["payload"]["partial"].as_bool().unwrap_or(false))
        .count();
    let partial_rate = if cache_rows.is_empty() {
        0.0
    } else {
        partial_n as f64 / cache_rows.len() as f64
    };
    s.push_str("| set | n | p50 | p95 | p99 | max | verdict |\n|---|---|---|---|---|---|---|\n");
    s.push_str(&pct_line("cache_update (non-cancelled)", &cache_vals, slo::CACHE_UPDATE_MS));
    s.push('\n');
    let partial_mark = if partial_rate <= slo::CACHE_PARTIAL_RATE_MAX { "PASS" } else { "FAIL" };
    s.push_str(&format!(
        "\nPartial rate: {:.1}% ({partial_n}/{}) — {partial_mark} (≤{:.0}%)\n\n",
        partial_rate * 100.0,
        cache_rows.len(),
        slo::CACHE_PARTIAL_RATE_MAX * 100.0
    ));

    // --- Q3-B idle CPU ---
    s.push_str("## Q3-B — Idle CPU (1-min avg ≤ 5%, 95% of samples)\n\n");
    let cpu_vals: Vec<f64> = of_type(records, "metric.cpu_sample")
        .iter()
        .filter_map(|r| f64_at(r, &["payload", "cpu_1min_avg"]))
        .collect();
    if cpu_vals.is_empty() {
        s.push_str("NO DATA (no cpu_1min_avg samples)\n\n");
    } else {
        let within = cpu_vals.iter().filter(|v| **v <= slo::IDLE_CPU_PCT).count();
        let frac = within as f64 / cpu_vals.len() as f64;
        let max = cpu_vals.iter().cloned().fold(f64::MIN, f64::max);
        let mark = if frac >= slo::IDLE_CPU_WITHIN_FRACTION && max <= slo::IDLE_CPU_MAX_PCT {
            "PASS"
        } else {
            "FAIL"
        };
        s.push_str(&format!(
            "- samples: {}\n- within 5%: {:.1}% (need ≥95%)\n- max 1-min avg: {:.2}% (ceiling 8%)\n- verdict: {mark}\n\n",
            cpu_vals.len(),
            frac * 100.0,
            max
        ));
    }

    // --- Q4 false positives ---
    s.push_str("## Q4 — Hover false positives\n\n");
    let sessions = of_type(records, "event.expand_session");
    let fp = sessions
        .iter()
        .filter(|r| {
            r["payload"]["auto_false_positive"].as_bool().unwrap_or(false)
                || r["payload"]["manual_false_positive"].as_bool().unwrap_or(false)
        })
        .count();
    let top_band: u64 = of_type(records, "counter.top_band_entry")
        .iter()
        .filter_map(|r| r["payload"]["count"].as_u64())
        .sum();
    let fp_rate = if top_band == 0 { 0.0 } else { fp as f64 / top_band as f64 };
    let mark = if fp as u32 <= slo::FALSE_POSITIVE_MAX_FREEWORK
        && fp_rate <= slo::FALSE_POSITIVE_RATE_MAX
    {
        "PASS"
    } else {
        "FAIL"
    };
    s.push_str(&format!(
        "- Expanded sessions: {}\n- False positives: {fp} (≤{})\n- Top-band entries: {top_band}\n- FP rate: {:.2}% (≤{:.0}%)\n- verdict: {mark}\n\n",
        sessions.len(),
        slo::FALSE_POSITIVE_MAX_FREEWORK,
        fp_rate * 100.0,
        slo::FALSE_POSITIVE_RATE_MAX * 100.0
    ));

    // --- Q1 residency (heartbeat gaps) ---
    s.push_str("## Q1 — Residency (heartbeat + panel events)\n\n");
    let hb = of_type(records, "soak.heartbeat");
    let recovered = of_type(records, "event.panel_recovered").len();
    let anim_timeouts = of_type(records, "event.anim_timeout").len();
    s.push_str(&format!(
        "- Heartbeats: {}\n- Panel self-heals: {recovered} (≤{} over 24h)\n- Anim timeouts (webview hang suspicion): {anim_timeouts}\n",
        hb.len(),
        slo::SELF_HEAL_MAX_24H
    ));
    s.push_str(
        "- NOTE: heartbeat-gap detection (≥180s silence) and 2s-visibility-loss checks require the on-device soak; this generator reports counts, the soak script flags gaps.\n\n",
    );

    s.push_str("---\n\n_Go/No-Go is a human decision (docs/phase0-dev-instructions.md §1). This report is the primary evidence, not the verdict._\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn expand_layering_splits_by_mode_and_fullscreen() {
        let recs = vec![
            json!({"type":"metric.expand_latency","payload":{"latency_ms":50.0,"mode":"notch","fullscreen":false}}),
            json!({"type":"metric.expand_latency","payload":{"latency_ms":90.0,"mode":"pseudo","fullscreen":true}}),
        ];
        let layers = layer_expand(&recs);
        assert_eq!(layers[0].1, vec![50.0]); // notch/windowed
        assert_eq!(layers[3].1, vec![90.0]); // pseudo/fullscreen
        assert!(layers[1].1.is_empty());
    }

    #[test]
    fn report_flags_no_data_without_samples() {
        let md = render_report(&[], &["none".into()]);
        assert!(md.contains("NO DATA"));
        assert!(md.contains("Q2"));
        assert!(md.contains("Q4"));
    }

    #[test]
    fn report_marks_pass_when_under_threshold() {
        let recs: Vec<Value> = (0..20)
            .map(|_| json!({"type":"metric.expand_latency","payload":{"latency_ms":40.0,"mode":"notch","fullscreen":false}}))
            .collect();
        let md = render_report(&recs, &["t".into()]);
        assert!(md.contains("PASS"));
    }
}
