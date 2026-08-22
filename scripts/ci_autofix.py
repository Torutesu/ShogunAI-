#!/usr/bin/env python3
"""Issue #191: classify a failed CI log, redact secrets, emit a fix plan.

Mechanical apply is rustfmt / clippy --fix only. Everything else is comment-only.
No LLM. No auto-merge. Run from repo root:

    python3 scripts/ci_autofix.py --self-test
    python3 scripts/ci_autofix.py redact --log-file /tmp/ci.log
    python3 scripts/ci_autofix.py classify --log-file /tmp/ci.log --sha abc1234
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys

# Align with crates/shogun-redact issuer prefixes, plus PEM and the licence-key shape.
# Word-boundary prefixes so crate names / prose are not treated as credentials.
SECRET_RES = [
    re.compile(r"(?<![A-Za-z0-9])gh[pousr]_[A-Za-z0-9]{20,}"),
    re.compile(r"(?<![A-Za-z0-9])github_pat_[A-Za-z0-9_]{20,}"),
    re.compile(r"(?i)(?<![A-Za-z0-9])bearer\s+[A-Za-z0-9._\-+/=]{12,}"),
    re.compile(r"(?<![A-Za-z0-9])sk-(?:ant-)?[A-Za-z0-9\-_]{16,}"),
    re.compile(r"(?<![A-Za-z0-9])xox[bpas]-[A-Za-z0-9\-]{12,}"),
    re.compile(r"(?<![A-Za-z0-9])(?:AKIA|ASIA)[A-Z0-9]{12,}"),
    re.compile(r"(?<![A-Za-z0-9])AIza[A-Za-z0-9\-_]{20,}"),
    re.compile(r"(?<![A-Za-z0-9])ya29\.[A-Za-z0-9._\-+/=]{12,}"),
    re.compile(r"(?<![A-Za-z0-9])glpat-[A-Za-z0-9\-_]{16,}"),
    re.compile(r"(?<![A-Za-z0-9])sq0(?:atp|csp)-[A-Za-z0-9\-_]{12,}"),
    re.compile(r"(?<![A-Za-z0-9])shpat_[A-Za-z0-9]{20,}"),
    re.compile(r"(?<![A-Za-z0-9])SG\.[A-Za-z0-9\-_.]{16,}"),
    re.compile(r"(?<![A-Za-z0-9])GOCSPX-[A-Za-z0-9\-_]{16,}"),
    re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]+?-----END [A-Z ]*PRIVATE KEY-----"),
    # Licence key: shogun-XXXX-XXXX-XXXX-XXXX (Crockford; I/L/O/U excluded).
    # Must not match crate paths such as shogun-desktop-spike.
    re.compile(
        r"(?i)(?<![A-Za-z0-9])shogun-(?:[0-9A-HJ-NP-TV-Z]{4}-){3}[0-9A-HJ-NP-TV-Z]{4}"
        r"(?![A-Za-z0-9-])"
    ),
]

# Diagnostics, not the successful `cargo clippy` / `cargo fmt` command line.
CLIPPY_DIAG = re.compile(r"clippy::|^error: .+clippy", re.I | re.M)
FMT_DIAG = re.compile(
    r"Would reformat:|error: .+not formatted according to rustfmt|rustfmt --check",
    re.I,
)
TEST_FAIL = re.compile(r"\btest result: FAILED\b|\bFAILED\b.+\.rs|\btest .+\s\.\.\. FAILED\b")
INVARIANT = [
    ("check-http-egress.py", "invariant_guard"),
    ("check-secret-exposure.py", "invariant_guard"),
    ("check-media-writes.py", "invariant_guard"),
    ("check-log-hygiene.py", "invariant_guard"),
    ("check-migrations.py", "invariant_guard"),
    ("check-batch-source-filter.py", "invariant_guard"),
]
SCRIPT_FAIL = re.compile(
    r"violation|exit code [1-9]|Traceback|AssertionError|Process completed with exit code",
    re.I,
)
HARNESS_HINT = re.compile(
    r"spike-harness|slo-0[12]|notch.*(?:expand|p95)|context cache|idle cpu",
    re.I,
)
FRONTEND_HINT = re.compile(r"@shogun-ai/desktop|eslint|vitest|typecheck", re.I)
API_HINT = re.compile(r"@shogun-ai/api|batch relay", re.I)
MACOS_HINT = re.compile(r"shogun-desktop-spike|macos-14|macos shell", re.I)


def redact(text: str) -> str:
    out = text
    for rx in SECRET_RES:
        out = rx.sub("[SECRET_REDACTED]", out)
    if out != text:
        # Second pass so classify can see a leak happened without keeping the token.
        out = "[possible secret stripped from this log]\n" + out
    return out


def _failed_script(log: str, script: str) -> bool:
    """True only when the script name sits next to a failure signal.

    `gh run view --log` includes successful jobs. Matching the script name
    alone would classify every later frontend/API failure as invariant_guard.
    """
    lines = log.splitlines()
    for i, line in enumerate(lines):
        if script not in line:
            continue
        window = "\n".join(lines[i : i + 4])
        if SCRIPT_FAIL.search(window):
            return True
    return False


def classify(log: str) -> str:
    if "SECRET_REDACTED" in log or log.startswith("[possible secret"):
        return "leak"
    for needle, klass in INVARIANT:
        if _failed_script(log, needle):
            return klass
    # macOS-shell clippy cannot be fixed by the workspace apply (spike excluded).
    if MACOS_HINT.search(log) and CLIPPY_DIAG.search(log):
        return "macos"
    if FMT_DIAG.search(log) and not CLIPPY_DIAG.search(log):
        return "rustfmt"
    if CLIPPY_DIAG.search(log):
        return "clippy"
    if HARNESS_HINT.search(log) and (
        TEST_FAIL.search(log) or "error" in log.lower() or "FAILED" in log
    ):
        return "harness"
    if MACOS_HINT.search(log) and "error" in log.lower():
        return "macos"
    if API_HINT.search(log) and TEST_FAIL.search(log):
        return "api"
    if FRONTEND_HINT.search(log) and (
        TEST_FAIL.search(log) or "error TS" in log or "ELIFECYCLE" in log
    ):
        return "frontend"
    if TEST_FAIL.search(log) or "error: test failed" in log.lower():
        return "rust_test"
    return "unknown"


def apply_commands(klass: str) -> list[str]:
    if klass == "rustfmt":
        return ["cargo fmt --all"]
    if klass == "clippy":
        return [
            "cargo clippy --workspace --exclude shogun-desktop-spike --all-targets "
            "--fix --allow-dirty --allow-staged -- -D warnings"
        ]
    return []


def fingerprint(klass: str, sha: str) -> str:
    raw = f"{klass}:{sha}".encode("utf-8")
    return hashlib.sha256(raw).hexdigest()[:16]


def plan(log: str, sha: str) -> dict:
    redacted = redact(log)
    klass = classify(redacted)
    cmds = apply_commands(klass)
    excerpt_ok = klass not in {"leak"}
    excerpt = ""
    if excerpt_ok:
        lines = [ln for ln in redacted.splitlines() if ln.strip()][-40:]
        excerpt = "\n".join(lines)[:2500]
    title = f"DO NOT MERGE: ci autofix ({klass})"
    body = (
        f"<!-- ci-autofix fingerprint={fingerprint(klass, sha)} class={klass} sha={sha} -->\n\n"
        f"# DO NOT MERGE\n\n"
        f"Mechanical CI follow-up for `{sha}` (issue #191). "
        f"**Class:** `{klass}`.\n\n"
        f"- Auto-merge is forbidden (this is L3: a human lands it).\n"
        f"- No LLM patch. Apply list is deterministic.\n"
        f"- Capture text / secrets must not appear below. Logs ran through `redact`.\n"
    )
    if klass == "harness":
        body += "- Harness/SLO signal (#103): measure on a Mac; do not bolt on an OSS router from this PR.\n"
    if cmds:
        body += "\n## Apply\n\n```\n" + "\n".join(cmds) + "\n```\n"
    else:
        body += "\nNo mechanical apply for this class. Human fix required.\n"
    if excerpt:
        body += "\n## Redacted tail\n\n```\n" + excerpt + "\n```\n"
    return {
        "class": klass,
        "sha": sha,
        "fingerprint": fingerprint(klass, sha),
        "apply": cmds,
        "comment_only": not bool(cmds),
        "title": title,
        "body": body,
        "label": "ci-autofix",
    }


def self_test() -> None:
    leak_log = "token ghp_" + ("a" * 36) + " boom\nerror: test failed\n"
    assert "[SECRET_REDACTED]" in redact(leak_log)
    assert classify(redact(leak_log)) == "leak"
    assert apply_commands("leak") == []

    for sample in (
        "ghs_" + ("b" * 36),
        "ghu_" + ("c" * 36),
        "ghr_" + ("d" * 36),
        "xoxb-123456789012-abcdefghijkl",
        "AKIAIOSFODNN7EXAMPLE",
        "AIza" + ("E" * 32),
        "ya29." + ("f" * 20),
        "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
        "Authorization: Bearer abcdefghijkl+/==",
        "shogun-A2B3-C4D5-E6F7-G8H9",
        "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n-----END RSA PRIVATE KEY-----",
    ):
        assert "[SECRET_REDACTED]" in redact(sample), sample
    for prose in (
        "task-management-workflow",
        "ASIA-PACIFIC-2026-PLANNING",
    ):
        assert "SECRET_REDACTED" not in redact(prose), prose

    crate = "cargo clippy -p shogun-desktop-spike --all-targets\n"
    assert "SECRET_REDACTED" not in redact(crate)
    assert "shogun-desktop-spike" in redact(crate)
    assert classify(redact(crate + "error: clippy::unwrap_used\n")) == "macos"

    fmt_log = "Diff in src/lib.rs exists\nWould reformat: crates/shogun-fusion/src/lib.rs\n"
    assert classify(redact(fmt_log)) == "rustfmt"
    assert apply_commands("rustfmt") == ["cargo fmt --all"]
    # Command-name only: not a rustfmt failure.
    assert classify(redact("cargo fmt --all\nerror: test failed\n")) == "rust_test"

    clippy_log = "error: this lint: clippy::unwrap_used\n --> crates/shogun-core/src/daemon.rs\n"
    assert classify(redact(clippy_log)) == "clippy"
    assert apply_commands("clippy")

    # Successful rust job prints these command names on every run. A later test
    # failure must not become clippy / invariant_guard / leak.
    mixed = (
        "cargo clippy --workspace --exclude shogun-desktop-spike --all-targets\n"
        "python3 scripts/check-log-hygiene.py\n"
        "python3 scripts/check-migrations.py\n"
        "python3 scripts/ci_autofix.py --self-test\n"
        "test db_backend::tests::foo ... FAILED\n"
        "test result: FAILED. 1 failed\n"
    )
    assert "SECRET_REDACTED" not in redact(mixed)
    assert classify(redact(mixed)) == "rust_test"

    inv_log = "python3 scripts/check-log-hygiene.py\nlog-rule violation: a bare log line\n"
    assert classify(redact(inv_log)) == "invariant_guard"
    assert apply_commands("invariant_guard") == []

    harness_log = "test notch_expand_p95 ... FAILED\nspike-harness slo-01 exceeded\n"
    assert classify(redact(harness_log)) == "harness"

    test_log = "test db_backend::tests::foo ... FAILED\ntest result: FAILED. 1 failed\n"
    assert classify(redact(test_log)) == "rust_test"

    fe_log = "pnpm --filter @shogun-ai/desktop typecheck\nerror TS2322: Type 'x' is not assignable\nELIFECYCLE\n"
    assert classify(redact(fe_log)) == "frontend"

    p = plan(fmt_log, "abc1234deadbeef")
    assert p["title"].startswith("DO NOT MERGE:")
    assert p["fingerprint"] == fingerprint("rustfmt", "abc1234deadbeef")
    assert "ghp_" not in p["body"]
    print("ci_autofix self-test OK")


def main(argv: list[str]) -> int:
    if "--self-test" in argv and len(argv) == 2:
        self_test()
        return 0
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["redact", "classify"])
    parser.add_argument("--log-file", required=True)
    parser.add_argument("--sha", default="unknown")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv[1:])
    if args.self_test:
        self_test()
        return 0
    log = open(args.log_file, encoding="utf-8", errors="replace").read()
    if args.command == "redact":
        sys.stdout.write(redact(log))
        return 0
    json.dump(plan(log, args.sha), sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
