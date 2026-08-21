# SHOGUN memory baseline — v0.1

First reproducible measurement of SHOGUN's memory ingestion and retrieval, taken with
`crates/memory-bench` against the unmodified `shogun-memory` layer.

**These numbers describe one machine on one day.** They are a baseline to compare future runs
against, not a certification. The conditions that disqualify them from certifying an SLO are listed
under [Validity](#validity) and are recorded in every JSON report as `mode` and
`slo.authoritative: false`.

## Run conditions

| | |
|---|---|
| Commit | `79f8516` (`research/memory-harness`), clean tree — every report carries `git_dirty: false` |
| Date | 2026-08-21 |
| Platform | Windows 11 (10.0.26200), x86_64 |
| Hardware | 8 GB RAM, SSD (`D:`) |
| Build | `--release` (workspace profile: `lto = "thin"`, `codegen-units = 1`) |
| Toolchain | rustc 1.94.0, MSVC 14.44, SQLCipher via `bundled-sqlcipher-vendored-openssl` |
| Storage | On-disk, WAL. Not in-memory — write latency includes real filesystem work |
| Retrieval | **Lexical only.** No ONNX embedder, so RRF fused a single list |
| Seed | 42 for every run |

Reproduce any row with:

```bash
cargo run -p memory-bench --release -- \
  --workload <name> --events <n> --queries <q> --seed 42 --db <path> --out reports
```

## Tests

`cargo test --release -p memory-bench` — **42 passed, 0 failed**.

| Suite | Tests | Covers |
|---|---|---|
| Unit (lib) | 23 | percentiles, recall/MRR, amplification, staleness accounting, SLO gating, RNG determinism |
| CLI | 6 | argument parsing, rejection of unknown workloads/flags |
| Integration (`bench_smoke`) | 13 | end-to-end runs against real SQLite, determinism, report round-trip |

## Headline results — 100k events

| Workload | Write p50 | Write p95 | Write p99 | Query p50 | Query p95 | Query p99 | Recall@5 | MRR | Write amp | DB size |
|---|---|---|---|---|---|---|---|---|---|---|
| `clean` | 0.11 ms | 0.36 ms | 0.92 ms | 17.19 ms | **40.79 ms** | 52.51 ms | 1.000 | 1.000 | 1.000 | 60.59 MB |
| `duplicate` | 0.14 ms | 0.47 ms | 1.22 ms | 16.33 ms | **35.40 ms** | 40.93 ms | 1.000 | 1.000 | 1.192 | 58.26 MB |
| `temporal` | 0.11 ms | 0.40 ms | 0.98 ms | 18.96 ms | **33.24 ms** | 33.24 ms | 1.000 | 0.500 | 1.000 | 60.66 MB |

`clean` and `duplicate` ran 500 queries; `temporal` ran 12 (one per tracked project), so its
percentiles rest on 12 samples and p95 = p99 = max. Treat them as indicative only.

### NFR-SLO-04 (local search p95 ≤ 500 ms)

Every run passed with a wide margin — the worst p95 measured was **40.79 ms, about 8% of the
500 ms budget**. This is the first time `spike_harness::slo::LOCAL_SEARCH_MS` has been exercised;
its in-tree comment still reads *"Not exercised in Phase 0; kept for Phase 1."*

The margin is large enough to be worth stating plainly: **on this evidence the search SLO is not
where the risk is.** But see [Validity](#validity) — the semantic half of hybrid search never ran,
and it is the half that opens the archive and does vector work.

## Retrieval latency vs corpus size

`clean`, 300 queries, seed 42:

| Events | Query p50 | Query p95 | Query p99 | DB size | Bytes/row |
|---|---|---|---|---|---|
| 10,000 | 1.75 ms | 4.44 ms | 5.60 ms | 6.22 MB | 652.5 |
| 50,000 | 7.64 ms | 15.37 ms | 19.33 ms | 31.01 MB | 650.4 |
| 100,000 | 17.19 ms | 40.79 ms | 52.51 ms | 60.59 MB | 635.3 |

Query p50 tracks corpus size close to linearly (10k→100k is 10× the data and 9.8× the p50). The
tail grows faster than the median: p95 rises 9.2× and p99 rises 9.4× across the same range.
Storage is steady at **~640 bytes/row**, so a 1M-event log projects to roughly 610 MB.

Linear growth in the lexical half is the thing to watch. At this slope 500k events would put p95
around 200 ms, still inside budget but no longer comfortable — and that is before the vector half is
added.

## Write amplification and deduplication

| | `clean` | `duplicate` | `temporal` |
|---|---|---|---|
| Writes submitted | 100,000 | 100,000 | 100,000 |
| Rows held after | 100,000 | 83,465 | 99,988 |
| Deduplicated | 0 | 16,535 | 12 |
| Write amplification | 1.000 | **1.192** | 1.000 |
| Duplicate collapse rate | n/a | **55.1%** | 100% |

`clean` behaving at exactly 1.000 with zero collapses is the control: the layer invents no
duplicates when there are none.

The `duplicate` workload is the informative one. It submits ~30,000 repeated facts, split evenly
between byte-identical repeats and reworded ones carrying the same fact. **The layer collapsed
55.1% of them** — essentially the exact-repeat half — and the reworded half survived as separate
rows. That is exactly what `event_log::insert_or_touch` promises: it matches on `content_hash`,
so a fact restated in different words is a new row.

The resulting **1.19× write amplification** is the concrete cost, and it is the number a
selective-update or semantic-dedup intervention has to beat. It is also a floor rather than an
estimate: the workload's rewording is a mechanical prefix/suffix, and real paraphrase is harder.

## The temporal finding

This is the result worth Toru's attention.

| Metric | Value |
|---|---|
| Recall@1 | **0.000** |
| Recall@5 | 1.000 |
| Recall@10 | 1.000 |
| MRR | **0.500** |
| Stale answer returned | **100%** |
| Stale answer outranked the current one | **100%** |

Each tracked project states its database choice three times — PostgreSQL, then SQLite, then
PostgreSQL again — and the query asks in the present tense ("where does *project* store its
records"). The current answer was retrieved every time, and was **never ranked first**. MRR of
exactly 0.500 means it sat at rank 2 in all 12 queries, with the superseded value above it.

So the retrieval layer meets the contract `retrieval_eval.rs` states it owes — *"get the answer
into the handful of lines that reach the reading model"* — while the line the reading model sees
first is the one the user already overruled.

A likely mechanism, stated as a hypothesis rather than a finding: the three statements are
near-identical text and the query contains neither value, so bm25 cannot discriminate on the
values themselves and falls back on document length. The superseded sentence ("…in SQLite…") is
shorter than the current one ("…in PostgreSQL…"), and bm25 favours shorter documents. Confirming
that would need a run with length-matched values, which v0.1 does not do.

Two caveats keep this honest. It is **12 queries over one phrasing pattern** — enough to be worth
investigating, nowhere near enough to generalise. And it is **lexical-only**; an embedder might
rank recency or semantics differently, which is precisely the comparison the next commit should run.

### A defect this run caught, and what it cost

The first temporal run reported recall@10 = 0.000 and 100% staleness — a far more dramatic
result. It was wrong, and the tell was that the run recorded exactly 12 deduplications for exactly
12 tracked projects.

The workload keyed fact identity on the *revision index*, but the value sequence returns to its
starting point (A → B → A). Revision 2 was therefore byte-identical to revision 0, the memory
layer correctly collapsed it into that row, and the "current" fact mapped to no row at all. The
benchmark was measuring its own modelling error.

Fixed by keying fact identity on the **value** rather than the revision index: restating an
earlier value is the same fact asserted twice, not a new one. The corrected figures are the ones
above. Recorded here because a benchmark that quietly reports its own bugs as findings is worse
than no benchmark.

## Write latency

Write p95 sits between 0.34 ms and 0.47 ms across every run, and p99 between 0.87 ms and 1.22 ms.
Ingestion is not a bottleneck at this scale.

One caveat matters for interpreting these. The bulk load batches 1,000 events per transaction
(`--write-batch`), so per-write timings **exclude commit and fsync**. A run with `--write-batch 1`
at 20k events measures the unbatched path:

| | Batched (1000) | Unbatched (1) |
|---|---|---|
| Write p50 | 0.11 ms | 0.13 ms |
| Write p95 | 0.36 ms | 0.43 ms |
| Write p99 | 0.92 ms | 0.92 ms |

The gap is small enough to be near noise — WAL with `synchronous = NORMAL` does not fsync on every
commit, which is the crash-safety trade `NFR-REL-01` already chose. So the batched figures are a
reasonable proxy for per-capture cost, not a wild underestimate. (The unbatched run is 20k events,
the batched comparison row is the 100k `clean` run; both are seed 42.)

## Validity

What these numbers **cannot** support:

1. **Not an SLO certification.** `docs/phase1-implementation-plan.md` requires SLO confirmation
   from on-device macOS runs. This is Windows/x86_64. Every report carries
   `slo.authoritative: false`.
2. **Lexical only.** `mode.semantic: false` in every report. The ONNX embedder is behind an
   off-by-default feature and needs a model file, so no query embedding was passed and RRF fused a
   single list. `retrieval_eval.rs` measured the lexical/hybrid gap as real (recall@5 0.93 vs
   1.00), so these must never be compared against hybrid numbers.
3. **No CPU or RSS.** The resource section reads `n/a` — `memory-bench` has readers for Linux
   (`/proc`) and macOS (`spike_harness::cpu`), and this run was on Windows. Reported as `null`
   rather than a fabricated zero. **This is a gap against the CPU/RAM question that was
   promised**, and it closes as soon as the bench runs on Linux CI or on-device.
4. **Warm tier only.** Every query ran `SearchDepth::WarmOnly`; the Cold int8 archive was never
   opened and Hot-tier behaviour was not exercised.
5. **Synthetic corpora.** Vocabulary is generated. The background is deliberately confusable with
   the answers, but it is not real capture data.
6. **One machine, 8 GB RAM.** Memory pressure was real enough that parallel builds had to be
   constrained to one job. Absolute latencies on a developer's Mac will differ.

## Files

Machine-readable reports (the form future runs should be diffed against) are in
[`baselines/`](baselines/), one JSON per run, each carrying its full config, git commit, git
dirtiness, platform and profile.

## What this suggests next

1. **Wire in the embedder and re-run all three workloads.** The single largest gap. It would also
   test whether the semantic half re-ranks the temporal case, or repeats the same mistake.
2. **Run on Linux CI to capture CPU and RSS**, closing gap 3 above.
3. **Investigate the rank-2 result** with length-matched values, to confirm or kill the bm25
   length-normalisation hypothesis.
4. **Treat 1.19× write amplification and 55.1% collapse as the target** for a semantic-dedup or
   selective-update intervention, measured through the same `MemoryBackend` seam so the comparison
   is like-for-like.
