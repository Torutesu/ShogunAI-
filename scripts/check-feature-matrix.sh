#!/usr/bin/env bash
# Every feature combination CI builds, in one command.
#
# Why this exists: most crates here have empty default features on purpose (the pure-logic build
# stays free of rusqlite, reqwest and axum), so `cargo test --workspace` compiles only a slice of
# the tree. A change to a shared type — an enum gaining a variant, a trait gaining a method — can
# be clean locally and fail CI on a match that only exists behind `--features db` or `exec`.
# That happened on issue #81 step 3 and cost a red CI round trip; running this before pushing
# turns that into a local failure instead.
#
# The macOS shell (shogun-desktop-spike) and the Windows/Linux shell (shogun-shell) are
# excluded everywhere: they need a platform webview, and their CI jobs compile them.
#
# Keep in step with .github/workflows/ci.yml — the combinations below mirror its jobs.
set -euo pipefail

run() {
    echo "── $* "
    "$@"
}

run cargo clippy --workspace --exclude shogun-desktop-spike --exclude shogun-shell --all-targets
run cargo clippy -p shogun-core --features net --all-targets
run cargo clippy -p shogun-core --features db --all-targets
run cargo clippy -p shogun-core --features exec --all-targets
run cargo clippy -p shogun-core --features daemon-server --all-targets
run cargo clippy -p shogun-core --features db --bin shogun-mcp
run cargo clippy -p shogun-mcp --features server --all-targets

run cargo test --workspace --exclude shogun-desktop-spike --exclude shogun-shell
run cargo test -p shogun-core --features db
run cargo test -p shogun-mcp --features server
run cargo test -p shogun-core --features daemon-server --bin shogun-api

echo "feature matrix: OK"

# The one surface no local command can reach. A shared enum gaining a variant breaks exhaustive
# matches in the macOS shell, and macos-14 CI is the only thing that compiles it — so when the
# diff touches one of these types, the sites below have to be read by eye before pushing.
SHARED_ENUMS='SendAction|LocalAction|Action::|ConnState|OpDecision|DenyReason|MemoryFault'
if git diff --name-only "${1:-origin/main}"...HEAD 2>/dev/null | grep -q '^crates/'; then
    if git diff -U0 "${1:-origin/main}"...HEAD -- crates/ 2>/dev/null |
        grep -qE "^\+.*(enum (SendAction|LocalAction|ConnState|OpDecision|DenyReason|MemoryFault)|^\+    [A-Z][A-Za-z]* \{)"; then
        echo
        echo "NOTE: this diff may add a variant to a shared enum. The macOS shell is not compiled"
        echo "      by anything above — check these sites by eye (each must be exhaustive or have"
        echo "      a catch-all):"
        grep -rnE "$SHARED_ENUMS" apps/desktop/src-tauri/src/ | grep -E "match |if let |matches!" || true
    fi
fi
