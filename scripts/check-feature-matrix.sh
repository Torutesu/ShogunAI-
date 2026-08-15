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
# The macOS shell (shogun-desktop-spike) is excluded everywhere: it only builds on-device, and
# macos-14 CI remains its only verification.
#
# Keep in step with .github/workflows/ci.yml — the combinations below mirror its jobs.
set -euo pipefail

run() {
    echo "── $* "
    "$@"
}

run cargo clippy --workspace --exclude shogun-desktop-spike --all-targets
run cargo clippy -p shogun-core --features net --all-targets
run cargo clippy -p shogun-core --features db --all-targets
run cargo clippy -p shogun-core --features exec --all-targets
run cargo clippy -p shogun-core --features daemon-server --all-targets
run cargo clippy -p shogun-core --features db --bin shogun-mcp
run cargo clippy -p shogun-mcp --features server --all-targets

run cargo test --workspace --exclude shogun-desktop-spike
run cargo test -p shogun-core --features db
run cargo test -p shogun-mcp --features server
run cargo test -p shogun-core --features daemon-server --bin shogun-api

echo "feature matrix: OK"
