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

# Strip these before any GitHub write. Keep the detector greedy and cheap.
SECRET_RES = [
    re.compile(r"ghp_[A-Za-z0-9]{20,}"),
    re.compile(r"gho_[A-Za-z0-9]{20,}"),
    re.compile(r"github_pat_[A-Za-z0-9_]{20,}"),
    re.compile(r"(?i)bearer\s+[A-Za-z0-9._\-]{12,}"),
    re.compile(r"sk-(?:ant-)?[A-Za-z0-9\-_]{16,}"),
    re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]+?-----END [A-Z ]*PRIVATE KEY-----"),
    re.compile(r"shogun-[A-Za-z0-9]{4,}-[A-Za-z0-9\-]+"),
]

CLIPPY_HINT = re.compile(r"\bclippy::|\berror: .+ clippy|cargo clippy", re.I)
FMT_HINT = re.compile(r"rustfmt|would reformat|cargo fmt", re.I)
TEST_FAIL = re.compile(r"\btest result: FAILED\b|\bFAILED\b.+\.rs|\btest .+\s\.\.\. FAILED\b")
INVARIANT = [
    ("check-http-egress.py", "invariant_guard"),
    ("check-secret-exposure.py", "invariant_guard"),
    ("check-media-writes.py", "invariant_guard"),
    ("check-log-hygiene.py", "invariant_guard"),
    ("check-migrations.py", "invariant_guard"),
    ("check-batch-source-filter.py", "invariant_guard"),
]
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


def classify(log: str) -> str:
    if "SECRET_REDACTED" in log or log.startswith("[possible secret"):
        return "leak"
    for needle, klass in INVARIANT:
        if needle in log:
            return klass
    if FMT_HINT.search(log) and not CLIPPY_HINT.search(log):
        return "rustfmt"
    if CLIPPY_HINT.search(log):
        return "clippy"
    if HARNESS_HINT.search(log):
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
    if FMT_HINT.search(log):
        return "rustfmt"
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

    fmt_log = "Diff in src/lib.rs exists\nWould reformat: crates/shogun-fusion/src/lib.rs\n"
    assert classify(redact(fmt_log)) == "rustfmt"
    assert apply_commands("rustfmt") == ["cargo fmt --all"]

    clippy_log = "error: this lint: clippy::unwrap_used\n --> crates/shogun-core/src/daemon.rs\n"
    assert classify(redact(clippy_log)) == "clippy"
    assert apply_commands("clippy")

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
