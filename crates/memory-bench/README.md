# memory-bench

Deterministic workloads and reproducible measurement for SHOGUN's memory ingestion and retrieval,
run against the real `shogun-memory` layer.

This is the benchmark `docs/phase1-implementation-plan.md` WP2.6 asks for — *"10万イベント合成データでの
p95 計測ベンチをCIに置く（Linuxで実行可能）"* — and it is the first thing in the tree to exercise
`spike_harness::slo::LOCAL_SEARCH_MS`, whose own comment still reads *"Not exercised in Phase 0;
kept for Phase 1."*

Measured baseline: [BASELINE.md](BASELINE.md) (100k events, release, on-disk). Machine-readable
reports in [baselines/](baselines/).

## Purpose

Before changing how memory behaves, establish what it currently does. Specifically:

- How fast are writes, and how fast is retrieval — as distributions, not averages?
- How does retrieval hold up as the log grows toward 100k events?
- How many rows does the store hold per distinct fact (write amplification)?
- Which repeated facts does the existing `content_hash` dedup collapse, and which survive?
- When a fact is overwritten by a later one, how often does retrieval still return the old value?
- What does it cost in CPU, RSS and disk?

## Running

`--db` only accepts a path that does not exist yet (stale `-wal`/`-shm` sidecars count too).
The benchmark migrates the schema and writes synthetic events into the file it is given, so it
requires disposable storage — it never reuses, resets, or deletes an existing database.

```bash
# Defaults: clean workload, 100k events, 500 queries, seed 42, in-memory.
cargo run -p memory-bench --release

# The measurement configuration — on disk, report saved.
cargo run -p memory-bench --release -- \
  --workload clean --events 100000 --queries 500 --seed 42 \
  --db /tmp/bench.db --out reports

cargo run -p memory-bench --release -- --workload duplicate --events 10000 --queries 200
cargo run -p memory-bench --release -- --workload temporal  --events 50000 --queries 12

cargo run -p memory-bench -- --help
```

Always `--release`. A debug build's latency is roughly an order of magnitude off; the report stamps
`environment.profile` so a debug number cannot quietly become a quoted one.

## Architecture

```text
workloads/   what we run       corpus + queries, generated from a seed
   ↓
backend      how we connect    the MemoryBackend seam over shogun-memory
   ↓
runner       how we execute    seed → generate → ingest → warm → measure
   ↓
metrics      how we measure    latency distributions, recall/MRR, amplification, staleness
   ↓
report       what we save      one JSON artifact per run, with config + commit
```

The load-bearing piece is the `MemoryBackend` trait. A later experiment — selective update,
consolidation, a retention policy — implements that trait a second time and is measured by this
same evaluator, unchanged. If an intervention required changes to the evaluator, the two runs would
no longer be measuring the same thing, and the comparison would prove nothing.

## Workloads

| Name | What it contains | What it measures |
|---|---|---|
| `clean` | Unique events, one answer per query | The reference point. Write amplification here should be 1.0 and nothing should collapse. |
| `duplicate` | ~30% repeated facts: half byte-identical, half reworded | What the `content_hash` contract catches, and what it misses. Exact repeats collapse via `insert_or_touch`; reworded ones hash differently and become second rows. |
| `temporal` | Facts overwritten by later facts; present-tense questions | How often retrieval hands back a value the user already overruled. |

The background is deliberately confusable with the answers — same projects, same people, same
vocabulary. A corpus of random strings would make retrieval look perfect and measure nothing,
because every needle would be the only text in the database containing its own words.

## Metrics

**Latency** — `p50`, `p95`, `p99`, `min`, `mean`, `max`, for writes and queries separately.
Percentiles are nearest-rank, computed by `spike_harness::stats::Percentiles` rather than
reimplemented here, so there is exactly one definition of p95 in the repository. Individual samples
are retained: a 2ms mean with a 400ms p99 is a product that feels broken, and the mean hides it.

**Write amplification** — `rows_held / distinct_facts`. Derived from what the database actually
contains, not from what we submitted.

**Duplicate collapse rate** — of the repeats the workload contained, the share the backend
recognised **correctly**. Every reported merge is checked against the fact the row's first writer
carried; a merge that combined two *different* facts destroyed information and is counted as a
`wrong_merge` instead — never into the collapse rate. This is what keeps a lossy semantic-dedup
intervention from scoring above the baseline by throwing memories away. `null` on a clean corpus:
there was no denominator, and reporting 0% would suggest a failure where there was nothing to
detect.

**Recall@1/5/10 and MRR** — computed the same way `shogun-memory/tests/retrieval_eval.rs` computes
them, deliberately. Two definitions of recall@5 in one repository would make the scale numbers and
the quality numbers incomparable.

**Staleness** — `stale_returned` (a superseded fact appeared at all) and `stale_outranked_current`
(a superseded fact outranked every correct one). The second is the sharper number: a stale row
sitting at rank 9 is untidy, one at rank 1 is a wrong answer.

**Resources** — sampled peak RSS and mean CPU% over the run. Mean CPU is total CPU time over
total wall time between the first and last reading — duration-weighted, so an unevenly sampled run
does not skew it. Peak RSS is a *sampled* peak; a spike between two samples is invisible to it.
Mean CPU covers this benchmark's measurement window and nothing else — it is not an idle-CPU
figure and must never be compared against `slo::IDLE_CPU_PCT`, which is defined over a 1-minute
idle window.

Every rate is nullable. A metric a workload cannot express reports `null`, never `0`.

## Reproducibility

A result is defined by `(workload, seed, events, queries)` plus the commit and the build profile.
All of it is written into the report:

```json
{
  "config":      { "workload": "clean", "seed": 42, "events": 100000, "queries": 500 },
  "environment": { "os": "linux", "arch": "x86_64", "git_commit": "a83f91d…",
                   "git_dirty": false, "profile": "release" },
  "mode":        { "semantic": false, "in_memory": false }
}
```

`git_dirty` matters as much as `git_commit`: a run from a modified tree belongs to no commit, and
recording it as a baseline for that SHA would be wrong. `db_path` and `out_dir` are recorded as
file names only — where the file lived on one contributor's disk is machine metadata, not part of
what defines the result, and committed baselines are public.

The seed drives a SplitMix64 defined in `rng.rs` rather than pulled from `rand`, whose generators
are explicitly allowed to change output between minor versions. A stored baseline has to stay
reproducible for longer than that guarantee holds.

Compare runs only across matching `mode` and `profile`. A debug build, an in-memory database, or a
lexical-only retrieval path each disqualify a run from certifying an SLO, and
`slo.authoritative` records it.

## CI

`cargo test --workspace` runs the smoke tests (small, seconds) on every PR, plus one CLI invocation
in `ci.yml`. The full 100k release run is `.github/workflows/memory-bench.yml`, triggered manually,
which uploads the JSON report as an artifact.

## Current limitations (v0.1)

This version establishes deterministic infrastructure and a system-level baseline. It is not a
complete research evaluation, and these gaps are recorded in the report rather than papered over:

- **Retrieval is lexical-only.** The ONNX embedder is behind an off-by-default feature and needs a
  model file on disk, so no query embedding is passed and RRF fuses a single list. `retrieval_eval`
  measured the gap as real (recall@5 0.93 lexical vs 1.00 hybrid), so lexical numbers here must
  never be compared against hybrid numbers there. Wiring the embedder in is the next step.
- **Duplicates are the workload's notion of a repeated fact**, not a semantic judgement. The bench
  knows two events carry the same fact because it generated them that way. It cannot yet detect a
  duplicate it did not plant.
- **Staleness is fact supersession, not contradiction detection.** The bench knows which statement
  came last because it planted the sequence. Nothing here reasons about whether two statements
  actually conflict.
- **Cold-tier and Hot-tier behaviour are not exercised.** Every run is Warm-only
  (`SearchDepth::WarmOnly`); `ColdScanStats` is available from the search path and not yet recorded.
- **CPU/RSS have no reader on Windows.** Linux reads `/proc`, macOS uses `spike_harness::cpu`;
  elsewhere the resource section is `null` rather than a fabricated zero.
- **`write.p95` is a lower bound** on per-capture write cost. The bulk load batches into
  transactions (`--write-batch`, default 1000) so a 100k run finishes; pass `--write-batch 1` to
  measure the true per-write cost including commit.
