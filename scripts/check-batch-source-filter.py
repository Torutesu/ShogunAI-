#!/usr/bin/env python3
"""A-2 guard (docs/meeting-text-on-the-search-spine.md): meeting text never reaches the Batch lane.

Meeting transcripts sit on the search spine (`event_log`, source='meeting') so search, Fusion and
local extraction work — but their consent story covers live transcription, not nightly cloud
classification. The exclusion is held by a type: `classify_via_batch`/`build_batch_items` demand
`BatchEventText`, and only `event_log::events_in_range_partitioned` (whose `cloud` half is
source-filtered) may construct it.

A type only guards what goes through it. This script pins the three ways the guarantee could be
quietly dismantled, each a small, reviewable-looking edit:

1. `BatchEventText { .. }` constructed outside `event_log.rs` (production code) — hands the batch
   pipeline a value that never went through the filter.
2. `BATCH_EXCLUDED_SOURCES` losing 'meeting', or the partition function no longer consulting it.
3. `build_batch_items` / `classify_via_batch` widening their signatures back to `EventText`.

Run from the repo root:  python3 scripts/check-batch-source-filter.py
Self-test the detector:   python3 scripts/check-batch-source-filter.py --self-test
"""

import pathlib
import re
import sys

# The one production file allowed to construct the proof type.
PRODUCER = "crates/shogun-memory/src/event_log.rs"

CONSTRUCT_RE = re.compile(r"BatchEventText\s*\{")

SKIP_DIRS = {"target", "node_modules", ".git", "dist", ".next"}


def repo_rust_files():
    root = pathlib.Path(".")
    for path in sorted(root.rglob("*.rs")):
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        rel = path.as_posix()
        try:
            yield rel, path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue


def test_boundary(text: str) -> int:
    """Line index where `mod tests` starts, or len(lines) if the file has no test module.
    Constructions past this line are test fixtures, which legitimately build the type by hand."""
    for i, line in enumerate(text.splitlines()):
        if re.match(r"\s*(pub\s+)?mod tests\b", line):
            return i
    return len(text.splitlines())


def scan_constructions(files):
    """(path, lineno, line) for every BatchEventText literal in production code outside PRODUCER."""
    hits = []
    for path, text in files:
        if path == PRODUCER:
            continue
        boundary = test_boundary(text)
        for i, line in enumerate(text.splitlines()):
            if i >= boundary:
                break
            stripped = line.strip()
            if stripped.startswith(("//", "///", "*", "//!")):
                continue
            if CONSTRUCT_RE.search(line):
                hits.append((path, i + 1, stripped))
    return hits


def scan_contract(files):
    """Structural assertions on the filter itself and the batch pipeline's signatures."""
    problems = []
    by_path = dict(files)

    producer = by_path.get(PRODUCER, "")
    if not re.search(r'BATCH_EXCLUDED_SOURCES[^=]*=\s*&\[[^\]]*"meeting"', producer):
        problems.append(f"{PRODUCER}: BATCH_EXCLUDED_SOURCES no longer lists 'meeting'")
    # The partition function must consult the constant — an inlined, drifting copy is the bug.
    m = re.search(r"fn events_in_range_partitioned[\s\S]{0,2000}?\n\}", producer)
    if not m:
        problems.append(f"{PRODUCER}: events_in_range_partitioned is gone — what filters the batch lane now?")
    elif "BATCH_EXCLUDED_SOURCES" not in m.group(0):
        problems.append(f"{PRODUCER}: events_in_range_partitioned no longer consults BATCH_EXCLUDED_SOURCES")

    jobs = by_path.get("crates/shogun-core/src/dreamcycle/jobs.rs", "")
    for fn in ("build_batch_items", "classify_via_batch"):
        m = re.search(rf"fn {fn}[\s\S]{{0,400}}?\)", jobs)
        if not m:
            continue  # moved files — the construction scan still holds the line
        sig = m.group(0)
        if "BatchEventText" not in sig:
            problems.append(
                f"crates/shogun-core/src/dreamcycle/jobs.rs: {fn} no longer demands BatchEventText "
                "— an unfiltered window can reach the relay again"
            )
    return problems


def self_test():
    contract_ok = [
        (PRODUCER,
         'pub const BATCH_EXCLUDED_SOURCES: &[&str] = &["meeting"];\n'
         "pub fn events_in_range_partitioned(conn: &Connection) -> Result<PartitionedEvents, E> {\n"
         "    if BATCH_EXCLUDED_SOURCES.contains(&source.as_str()) { }\n"
         "}\n"),
        ("crates/shogun-core/src/dreamcycle/jobs.rs",
         "pub fn build_batch_items(events: &[shogun_memory::event_log::BatchEventText])\n"
         "pub async fn classify_via_batch<B>(client: &B, events: &[shogun_memory::event_log::BatchEventText], max: u32)\n"),
    ]
    assert not scan_contract(contract_ok), f"honest contract flagged: {scan_contract(contract_ok)}"
    assert not scan_constructions(contract_ok), "the producer file itself was flagged"

    # 1. a rogue construction in production code
    rogue = contract_ok + [
        ("crates/shogun-core/src/sneaky.rs",
         "fn widen(events: Vec<EventText>) -> Vec<BatchEventText> {\n"
         "    events.into_iter().map(|e| BatchEventText { id: e.id, content: e.content }).collect()\n"
         "}\n"),
    ]
    assert scan_constructions(rogue), "missed a BatchEventText built outside the producer"

    # …but the same construction inside a test module is a fixture, not a bypass
    fixture = contract_ok + [
        ("crates/shogun-core/src/honest.rs",
         "pub fn run() {}\n"
         "#[cfg(test)]\n"
         "mod tests {\n"
         "    fn f() { let e = BatchEventText { id: 1, content: String::new() }; }\n"
         "}\n"),
    ]
    assert not scan_constructions(fixture), "a test fixture was flagged as a bypass"

    # …and a commented mention is documentation, not code
    doc = contract_ok + [
        ("crates/shogun-core/src/doc.rs", "// only event_log may build BatchEventText { .. }\n")
    ]
    assert not scan_constructions(doc), "a comment was flagged"

    # 2. the exclusion list losing 'meeting'
    dropped = [
        (PRODUCER,
         "pub const BATCH_EXCLUDED_SOURCES: &[&str] = &[];\n"
         "pub fn events_in_range_partitioned(conn: &Connection) -> Result<PartitionedEvents, E> {\n"
         "    if BATCH_EXCLUDED_SOURCES.contains(&source.as_str()) { }\n"
         "}\n"),
    ]
    assert scan_contract(dropped), "missed BATCH_EXCLUDED_SOURCES dropping 'meeting'"

    # 3. the pipeline widening back to EventText
    widened = [
        contract_ok[0],
        ("crates/shogun-core/src/dreamcycle/jobs.rs",
         "pub fn build_batch_items(events: &[shogun_memory::event_log::EventText])\n"
         "pub async fn classify_via_batch<B>(client: &B, events: &[shogun_memory::event_log::EventText], max: u32)\n"),
    ]
    assert scan_contract(widened), "missed the batch pipeline widening back to EventText"

    print("self-test OK")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return

    files = list(repo_rust_files())
    constructions = scan_constructions(files)
    problems = scan_contract(files)

    if constructions or problems:
        for path, lineno, line in constructions:
            print(f"{path}:{lineno}: BatchEventText constructed outside {PRODUCER}: {line}")
        for p in problems:
            print(p)
        print(
            "\nMeeting text must not reach the Batch lane (A-2, "
            "docs/meeting-text-on-the-search-spine.md). Route windows through "
            "event_log::events_in_range_partitioned and keep the proof type narrow."
        )
        sys.exit(1)

    print(
        f"batch source filter OK: BatchEventText built only in {PRODUCER}, "
        "'meeting' excluded, pipeline signatures intact."
    )


if __name__ == "__main__":
    main()
