#!/usr/bin/env python3
"""FR-TR-03 dependency guard: enforce the single HTTP egress.

CLAUDE.md invariant 3 + FR-TR-03: every external send must go through the one common client layer
(shogun-core's `llm::transport`), which forces a traceability record. If any *other* crate pulls in
a raw HTTP client, it could send off-device without a trace. This check fails CI when a workspace
crate outside the allowlist declares a banned HTTP-client dependency.

It reads real dependency names from `cargo metadata` (not text grep), so it can't be fooled by
comments or formatting. Run from the repo root:  python3 scripts/check-http-egress.py
Self-test the detector logic:                     python3 scripts/check-http-egress.py --self-test
"""

import json
import subprocess
import sys

# The only crates permitted to depend on an HTTP client — the single egress (ADR / FR-TR-03).
ALLOWLIST = {"shogun-core"}

# Raw HTTP client crates. A component reaching for any of these bypasses the traced egress.
BANNED_HTTP_CLIENTS = {
    "reqwest",
    "hyper",
    "ureq",
    "curl",
    "isahc",
    "attohttpc",
    "surf",
    "attohttp",
    "http-client",
    "hyper-util",
}


def violations(packages):
    """Yield (crate, banned_dep) for every non-allowlisted crate that declares a banned client."""
    out = []
    for pkg in packages:
        name = pkg["name"]
        if name in ALLOWLIST:
            continue
        for dep in pkg.get("dependencies", []):
            if dep["name"] in BANNED_HTTP_CLIENTS:
                out.append((name, dep["name"]))
    return out


def self_test():
    """Prove the detector trips on a violation and passes a clean tree — no cargo needed."""
    clean = [
        {"name": "shogun-core", "dependencies": [{"name": "reqwest"}]},
        {"name": "shogun-mcp", "dependencies": [{"name": "shogun-agents"}]},
    ]
    dirty = [
        {"name": "shogun-core", "dependencies": [{"name": "reqwest"}]},
        {"name": "shogun-mcp", "dependencies": [{"name": "reqwest"}]},  # violation
    ]
    assert violations(clean) == [], "clean tree must have no violations"
    assert violations(dirty) == [("shogun-mcp", "reqwest")], "must catch the raw client in shogun-mcp"
    print("self-test OK: detector passes a clean tree and catches a raw HTTP client.")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0

    meta = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        capture_output=True,
        text=True,
        check=True,
    )
    packages = json.loads(meta.stdout)["packages"]
    found = violations(packages)
    if found:
        print("FR-TR-03 violation: raw HTTP client used outside the traced egress (shogun-core).")
        for crate, dep in found:
            print(f"  - {crate} depends on `{dep}` — route external sends through shogun-core::llm::transport instead.")
        print("\nIf a new egress is genuinely needed, add the crate to ALLOWLIST here *and* record the decision.")
        return 1
    print(f"HTTP egress OK: only {sorted(ALLOWLIST)} depend on an HTTP client.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
