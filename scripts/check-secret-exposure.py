#!/usr/bin/env python3
"""CLAUDE.md invariant 7 guard: keep raw secrets from leaking out of the Secret type.

`Secret` (shogun-core::llm) redacts under Debug/Display; the only way to obtain the raw string is
`Secret::expose()`. That call is the single leak vector — pass its result to a log, a DB write, or
telemetry and a token/BYOK key escapes. This check fails CI when `.expose(` appears anywhere outside
a small allowlist of files that legitimately need the raw value (the HTTP header builder, the type's
own definition/tests, and — later — the Keychain store).

Run from the repo root:  python3 scripts/check-secret-exposure.py
Self-test the detector:   python3 scripts/check-secret-exposure.py --self-test
"""

import pathlib
import re
import sys

# Files permitted to call Secret::expose(). Anything else must never touch the raw secret.
ALLOWLIST = {
    "crates/shogun-core/src/llm/anthropic.rs",  # builds the x-api-key header (the traced egress)
    "crates/shogun-core/src/llm/mod.rs",        # defines expose() + its unit tests
    # Future: the Keychain store module, when added, goes here with a decision record.
}

EXPOSE_RE = re.compile(r"\.expose\s*\(")


def scan(files):
    """Yield (path, lineno, line) for every .expose( call outside the allowlist."""
    hits = []
    for path, text in files:
        if path in ALLOWLIST:
            continue
        for i, line in enumerate(text.splitlines(), 1):
            if EXPOSE_RE.search(line):
                hits.append((path, i, line.strip()))
    return hits


def self_test():
    clean = [
        ("crates/shogun-core/src/llm/anthropic.rs", "key.expose()"),  # allowlisted
        ("crates/shogun-mcp/src/scope.rs", "let x = authorize(s, o);"),
    ]
    dirty = [
        ("crates/shogun-agents/src/engine.rs", 'tracing::info!("key={}", k.expose());'),  # leak
    ]
    assert scan(clean) == [], "allowlisted / unrelated code must pass"
    found = scan(dirty)
    assert len(found) == 1 and found[0][0].endswith("engine.rs"), "must catch expose() in a non-allowlisted file"
    print("self-test OK: detector allows the allowlist and catches a stray expose().")


def repo_rust_files():
    root = pathlib.Path(".")
    for p in sorted(root.glob("crates/**/*.rs")):
        yield p.as_posix(), p.read_text(encoding="utf-8", errors="replace")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    hits = scan(list(repo_rust_files()))
    if hits:
        print("invariant-7 violation: Secret::expose() used outside the allowlist — a raw secret may leak.")
        for path, line, text in hits:
            print(f"  - {path}:{line}: {text}")
        print("\nDo not log/store the exposed value. If a new site legitimately needs it, add the file to ALLOWLIST here with a decision record.")
        return 1
    print(f"secret exposure OK: .expose() only in {sorted(ALLOWLIST)}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
