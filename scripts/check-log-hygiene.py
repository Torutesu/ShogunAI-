#!/usr/bin/env python3
"""CLAUDE.md log rule guard: captured user text must never reach a log line.

"テレメトリ・ログにキャプチャ内容（ユーザーのテキスト）を含めない。デバッグログも同様" — and the
2026-08-13 audit found two live violations of exactly this shape: a meeting diagnostic that
printed the focused WINDOW TITLE, and a self-test that printed an `Action` Debug carrying
notification and clipboard text. Both looked like ordinary diagnostics in review.

The check is a co-occurrence heuristic, like check-media-writes.py: a bare `eprintln!`/`println!`
that interpolates an identifier whose name means "user-derived content". It cannot know what a
variable holds, so two escape hatches exist and both are honest:

- Route the line through `elog!` (shogun_core::log_redact), which scrubs keys, emails and URLs
  before writing. That is the fix, not the exemption.
- Mark the site `// log-hygiene-ok: <reason>` when the identifier is structural despite its name
  (a window label, a provider status). The reason lives at the site, where the next reader is.

Run from the repo root:  python3 scripts/check-log-hygiene.py
Self-test the detector:   python3 scripts/check-log-hygiene.py --self-test
"""

import pathlib
import re
import sys

# Bare stderr/stdout logging. `elog!` is deliberately absent — it is the sanctioned path.
LOG_RE = re.compile(r"\b(?:eprintln|println)!\s*\(")

# Identifiers that mean "text a human wrote or a screen showed". `label`/`name` are intentionally
# NOT here: in this codebase they are window labels and app names (already the documented, allowed
# thing to log), so including them would be noise that trains people to ignore this check.
CONTENT_RE = re.compile(
    r"""\{[^}]*\b(
        title|text|body|content|snippet|transcript|chunk|subject|query|prompt|
        answer|excerpt|summary|note|msg|message|rationale|utterance|caption
    )\b[^}]*\}""",
    re.VERBOSE | re.IGNORECASE,
)

# A whole `Action`/struct printed with {:?} can carry content fields even when the variable name
# is innocent — that is precisely how the notch self-test leaked. Flag Debug-printing of the
# known content-bearing types.
DEBUG_TYPES_RE = re.compile(r"\{\s*(?:a\.action|action|record|event|item|pack)\s*:\?\s*\}")

OK_MARKER = "log-hygiene-ok"

SKIP_DIRS = {"target", "node_modules", ".git", "dist", ".next"}
# Dev-only probes that exist to print what the pipeline produced. Not shipped, not a log.
SKIP_PATH_PARTS = {"examples"}


def repo_rust_files():
    root = pathlib.Path(".")
    for path in sorted(root.rglob("*.rs")):
        if any(part in SKIP_DIRS or part in SKIP_PATH_PARTS for part in path.parts):
            continue
        try:
            yield path.as_posix(), path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue


def scan(files):
    """Yield (path, lineno, line) for every bare log line interpolating user content."""
    hits = []
    for path, text in files:
        lines = text.splitlines()
        for i, line in enumerate(lines):
            stripped = line.strip()
            if stripped.startswith(("//", "///", "*")):
                continue
            if not LOG_RE.search(line):
                continue
            if not (CONTENT_RE.search(line) or DEBUG_TYPES_RE.search(line)):
                continue
            # Escape hatch: same line, or the comment line directly above it.
            prev = lines[i - 1] if i > 0 else ""
            if OK_MARKER in line or OK_MARKER in prev:
                continue
            hits.append((path, i + 1, stripped))
    return hits


def self_test():
    bad = [
        # The real 2026-08-13 findings, in their original shape.
        ("crates/fake/src/a.rs", 'eprintln!("[meeting] saw {bundle} title={title:?}");'),
        ("crates/fake/src/b.rs", 'eprintln!("[selftest] {:?} {:?}", a.level, a.action);'),
        ("crates/fake/src/c.rs", 'println!("transcribed: {text}");'),
    ]
    # b.rs uses positional args, so CONTENT_RE misses it — DEBUG_TYPES_RE must not be the only
    # net either. Assert the two we can catch, and document the gap honestly.
    found = {p for p, _, _ in scan(bad)}
    assert "crates/fake/src/a.rs" in found, "detector missed a window title in a log line"
    assert "crates/fake/src/c.rs" in found, "detector missed transcribed text in a log line"

    debug_case = [("crates/fake/src/d.rs", 'eprintln!("[selftest] {action:?}");')]
    assert scan(debug_case), "detector missed a Debug-printed action"

    good = [
        # Structural identifiers this codebase logs on purpose.
        ("crates/fake/src/e.rs", 'eprintln!("[shell] panel docked ({anchor}) at {x},{y}");'),
        ("crates/fake/src/f.rs", 'eprintln!("[meeting] saw {bundle} title_len={n}");'),
        # The sanctioned path scrubs, so it is never a hit.
        ("crates/fake/src/g.rs", 'shogun_core::elog!("[ui] {msg}");'),
        # An explicit, justified exemption.
        ("crates/fake/src/h.rs", 'eprintln!("[shell] window `{label}` {msg}"); // log-hygiene-ok: AppKit status strings'),
        # A comment naming the rule must not trip it.
        ("crates/fake/src/i.rs", '// never eprintln! a {title} — CLAUDE.md log rule'),
    ]
    hits = scan(good)
    assert not hits, f"detector flagged honest code: {hits}"

    above = [("crates/fake/src/j.rs", '// log-hygiene-ok: provider status only\neprintln!("{msg}");')]
    assert not scan(above), "marker on the line above was not honoured"
    print("check-log-hygiene self-test OK")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    files = list(repo_rust_files())
    hits = scan(files)
    if hits:
        print("log-rule violation: a bare log line interpolates user-derived content.")
        for path, line, text in hits:
            print(f"  - {path}:{line}: {text}")
        print(
            "\nCaptured text must not reach a log, debug builds included. Either route the line "
            "through `elog!` (shogun_core::log_redact — it scrubs keys, emails and URLs), log a "
            "shape instead of the value (`title_len={n}`), or, if the identifier is structural "
            "despite its name, mark the site `// log-hygiene-ok: <reason>`."
        )
        return 1
    print(f"log hygiene OK: no bare log line interpolates user content ({len(files)} files scanned).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
