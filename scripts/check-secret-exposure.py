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
    "crates/shogun-core/src/llm/anthropic.rs",     # builds the x-api-key header (the traced egress)
    "crates/shogun-core/src/llm/openai_compat.rs",  # builds the Authorization: Bearer header (OpenAI/OpenRouter egress)
    "crates/shogun-core/src/llm/relay.rs",          # builds Authorization: Bearer <license token> for the batch relay (traced egress; decision: docs/batch-relay-design.md §4.1)
    "crates/shogun-core/src/llm/mod.rs",           # defines expose() + its unit tests
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


# --- Issue #110 guard: never lift another application's stored credential -------------------
#
# Subscription delegation is legitimate precisely because SHOGUN delegates to a vendor CLI the user
# already signed into, and never touches the token that CLI stored. Reading `~/.claude/.credentials.json`
# (or a peer app's Keychain item) would turn the feature into credential theft, break the vendors'
# terms, and get user accounts banned. The distinction is invisible in a diff — one `read_to_string`
# looks like any other — so it is enforced here instead of remembered.
# Matches the credential FILES the vendor CLIs keep their tokens in. Deliberately not matching the
# Keychain APIs in general: SHOGUN legitimately uses the Keychain for its own items (invariant 7),
# so flagging `security find-generic-password` would be noise — and the file paths are where a
# credential lift would actually have to go.
CREDENTIAL_LIFT_RE = re.compile(
    r"""\.claude/\.credentials|\.codex/auth\.json|\.gemini/oauth_creds|\.credentials\.json""",
    re.IGNORECASE,
)

# The module that documents and tests the non-goal names these paths in its own assertions.
CREDENTIAL_LIFT_ALLOWLIST = {
    "crates/shogun-core/src/llm/subscription.rs",  # the FORBIDDEN table its own test checks against
}


def scan_credential_lift(files):
    """Yield (path, lineno, line) for every reach toward another app's credential store."""
    hits = []
    for path, text in files:
        if path in CREDENTIAL_LIFT_ALLOWLIST:
            continue
        for i, line in enumerate(text.splitlines(), 1):
            if CREDENTIAL_LIFT_RE.search(line):
                hits.append((path, i, line.strip()))
    return hits


# --- Batch-relay guard (E-38 / docs/batch-relay-design.md §7): the direct-Anthropic Batch path
# must not ship ------------------------------------------------------------------------------
#
# The shipping Batch lane goes license-token → relay; only a developer's debug build may hit
# Anthropic directly with a raw key. shogun-core enforces half of this in the type system
# (`BatchRoute::DirectAnthropic` exists only under `cfg(debug_assertions)`), but the desktop
# crate could still construct `AnthropicBatchClient` unconditionally — one `::new(` looks like
# any other — so the other half is enforced here: in `apps/desktop`, every reference to the
# direct client (or the debug-only route variant) must sit inside a `#[cfg(debug_assertions)]`-
# gated item. A hit outside such a region is the §2.1 rejected design headed for a release
# binary.
DIRECT_BATCH_RE = re.compile(r"AnthropicBatchClient|BatchRoute::DirectAnthropic")

# Only the desktop crate is held to this: shogun-core legitimately defines/tests the direct
# client, and CI's release guarantee there is the missing enum variant, not this scan.
DIRECT_BATCH_PREFIX = "apps/desktop/"

CFG_DEBUG_RE = re.compile(r"#\[cfg\(debug_assertions\)\]")


def _debug_gated_lines(text):
    """Return the set of 1-based line numbers covered by a #[cfg(debug_assertions)]-gated item.

    Brace-counting heuristic, deliberately simple (this is a guard, not a parser): the attribute
    covers every line up to and including the close of the first brace block that opens after it
    (a gated fn/mod/const body or match arm), or up to the first `;` for a braceless item.
    """
    gated = set()
    pending = False   # saw the attribute, waiting for the item's block to open
    depth = 0         # current brace depth
    close_at = None   # brace depth at which the gated block ends (None = not inside one)
    for i, line in enumerate(text.splitlines(), 1):
        if CFG_DEBUG_RE.search(line):
            pending = True
        if pending or close_at is not None:
            gated.add(i)
        opens, closes = line.count("{"), line.count("}")
        if pending:
            if opens > 0:
                close_at = depth  # the gated block closes when depth returns here
                pending = False
            elif ";" in line:
                pending = False   # braceless gated item (const/use) ends at the semicolon
        depth += opens - closes
        if close_at is not None and depth <= close_at:
            close_at = None
    return gated


def scan_direct_batch(files):
    """Yield (path, lineno, line) for direct-Batch references outside a debug-gated region."""
    hits = []
    for path, text in files:
        if not path.startswith(DIRECT_BATCH_PREFIX):
            continue
        gated = _debug_gated_lines(text)
        for i, line in enumerate(text.splitlines(), 1):
            if DIRECT_BATCH_RE.search(line) and i not in gated:
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

    lift_clean = [
        ("crates/shogun-core/src/llm/subscription.rs", 'const FORBIDDEN: &[&str] = &[".credentials"];'),
        ("crates/shogun-core/src/llm/mod.rs", "let cfg = read_to_string(settings_path)?;"),
        # SHOGUN's own Keychain item, and an unrelated path under a vendor's config dir. Neither is
        # a credential lift, and flagging them would train people to ignore this check.
        ("crates/shogun-core/examples/recap_probe.rs", "security find-generic-password -s SHOGUN -a select-kk-batch"),
        ("crates/shogun-memory/src/ai_session.rs", "SHOGUN_AI_SESSION_LOG=~/.claude/projects/x.jsonl"),
    ]
    lift_dirty = [
        ("crates/shogun-core/src/llm/thief.rs", 'read_to_string("~/.claude/.credentials.json")'),
        ("apps/desktop/src-tauri/src/oops.rs", 'let tok = fs::read(home.join(".codex/auth.json"))?;'),
    ]
    assert scan_credential_lift(lift_clean) == [], "the documented non-goal and ordinary reads must pass"
    assert len(scan_credential_lift(lift_dirty)) == 2, "must catch a reach into another app's credentials"

    gated_dream = (
        "apps/desktop/src-tauri/src/dream.rs",
        "\n".join([
            "fn run_via_batch() {",
            "    let outcome = match batch_route(env) {",
            "        BatchRoute::Relay => {",
            "            let client = RelayBatchClient::new(t, sink, credential, cfg);",
            "            run(client)",
            "        }",
            "        #[cfg(debug_assertions)]",
            "        BatchRoute::DirectAnthropic => {",
            "            let client = shogun_core::llm::anthropic::AnthropicBatchClient::new(t, sink, credential, cfg);",
            "            run(client)",
            "        }",
            "    };",
            "}",
        ]),
    )
    ungated_dream = (
        "apps/desktop/src-tauri/src/dream.rs",
        "\n".join([
            "fn run_via_batch() {",
            "    // the rejected §2.1 design: a raw operator key in a shipping code path",
            "    let client = AnthropicBatchClient::new(t, sink, credential, cfg);",
            "}",
        ]),
    )
    core_definition = (
        # shogun-core defines and tests the direct client; its release guarantee is the missing
        # enum variant, so this scan must not fire outside apps/desktop.
        "crates/shogun-core/src/llm/anthropic.rs",
        "pub struct AnthropicBatchClient<T, S> { transport: T, sink: S }",
    )
    assert scan_direct_batch([gated_dream, core_definition]) == [], "debug-gated / core code must pass"
    found = scan_direct_batch([ungated_dream])
    assert len(found) == 1 and found[0][1] == 3, "must catch an ungated direct-Batch construction"

    print(
        "self-test OK: detector allows the allowlist and catches a stray expose() / credential "
        "lift / ungated direct-Batch path."
    )


def repo_rust_files():
    root = pathlib.Path(".")
    # apps/desktop is scanned too: the Tauri layer is where a "just read the token" shortcut would
    # be most tempting, since it already holds the Keychain handle.
    for pattern in ("crates/**/*.rs", "apps/desktop/src-tauri/src/**/*.rs"):
        for p in sorted(root.glob(pattern)):
            yield p.as_posix(), p.read_text(encoding="utf-8", errors="replace")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    files = list(repo_rust_files())

    lifts = scan_credential_lift(files)
    if lifts:
        print("Issue #110 violation: code reaches into another application's credential store.")
        for path, line, text in lifts:
            print(f"  - {path}:{line}: {text}")
        print(
            "\nSubscription delegation works by launching a CLI the user already signed into — never by "
            "reading the token it stored. Delegate instead; if a site is genuinely unrelated, add it to "
            "CREDENTIAL_LIFT_ALLOWLIST here with a decision record."
        )
        return 1

    direct = scan_direct_batch(files)
    if direct:
        print("batch-relay violation (E-38): the direct-Anthropic Batch path escapes its cfg(debug_assertions) gate.")
        for path, line, text in direct:
            print(f"  - {path}:{line}: {text}")
        print(
            "\nA shipping binary must reach the Batch API only through the relay (license token; "
            "docs/batch-relay-design.md). Construct the direct client only inside a "
            "#[cfg(debug_assertions)]-gated item, behind shogun_core::llm::batch_route."
        )
        return 1

    hits = scan(files)
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
