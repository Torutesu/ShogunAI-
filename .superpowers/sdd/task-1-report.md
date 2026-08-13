# Task 1 report — Approval store safety and action status

## Status

DONE_WITH_CONCERNS. Implementation committed after focused verification. Existing unrelated untracked files were left untouched. `generated-pets/` was created during work and removed.

## Implementation

- Added durable approval statuses: `pending`, `rejected`, `timed_out`, `sent`, `send_failed`, `draft_saved`.
- Added body-free terminal records with bounded retention (`256` rows). Full preview body remains only on pending rows.
- Added advisory sidecar lock covering load/mutate/save transactions.
- Kept atomic replacement, unique temporary names, and mode `0600` temporary files.
- Corrupt/unreadable approval store now returns an error; missing store remains empty.
- Desktop approval flow records rejection, timeout, send, send failure, and draft-save outcomes in shared ledger.
- Added `actions.poll` MCP tool and `/v1/actions/poll/<id>` REST route.
- Added `shogun actions poll <id>` CLI command.
- Preserved existing Gmail Composio-only and Calendar/Slack/GitHub first-layer routing. Dedicated confirm remains required; Enter path unchanged.

## RED evidence

Focused RED run after adding safety tests failed at compile time with missing `Result`/status APIs, including:

`no method named is_err found for struct ApprovalQueue`

`no method named mark_status found for mutable reference &mut ApprovalQueue`

This recorded the required pre-implementation failing state.

## GREEN evidence

Focused tests passed:

- `cargo test -p shogun-agents approval::tests --lib`: 12 passed.
- `cargo test -p shogun-mcp approval_store::tests --lib`: 5 passed.
- MCP persistence test: 1 passed.
- REST poll route test: 1 passed.
- Earlier focused MCP/CLI run: shogun-mcp 106 passed; shogun-agents 33 passed. CLI had 29/30 pass; its loopback socket test was blocked by sandbox `Operation not permitted`.
- `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml -q`: passed; existing warnings only.

The requested full relevant suite was started but user-interrupted during wrap-up. It is therefore not claimed as complete.

## Changed files

`crates/shogun-agents/src/approval.rs`

`crates/shogun-mcp/src/approval_store.rs`, `mcp.rs`, `rest.rs`, `server.rs`

`crates/shogun-cli/src/command.rs`, `parse.rs`, `wire.rs`

`apps/desktop/src-tauri/src/approvals.rs`, `fullui.rs`

## Self-review / concerns

- Store lock is a filesystem advisory lock with bounded retry; stale lock recovery is not implemented.
- Desktop and REST server integration compile, but desktop send execution was not run against live connectors.
- Full suite completion unavailable because user interrupted it; CLI loopback test needs an environment allowing localhost sockets.
- No routing changes made.

## Reviewer fix pass

### RED

Pre-fix focused run exposed the blocking behavior: terminal transition test failed because the pending body remained serialized (`assertion failed: !text.contains("SECRET BODY")`), and initial reviewer-safety changes required missing status/import APIs. These tests were then completed against the corrected implementation.

### Fixes

- REST `AppState` now accepts shared approval-store path; `shogun-api` resolves same `l3_approvals.json` path as MCP. REST enqueue and poll use locked `with_queue` transactions.
- Queue tracks in-flight confirmed IDs, allowing post-confirm execution failures to become `send_failed`; forged IDs are rejected by `mark_status`.
- ID allocation has checked terminal/in-flight import advancement and explicit exhaustion refusal; no wrap reuse.
- Store import validates action kind, destination, route/action agreement, origin, IDs, duplicates, terminal status, and rejects invalid rows instead of filtering them.
- Public `save_queue` acquires sidecar lock; transaction path uses private unlocked writer, preventing bypass/deadlock.
- `actions.poll` schema now marks `approval_id` required with minimum `1`.

### GREEN evidence

- `cargo test -p shogun-agents -p shogun-mcp`: pass — 36 + 109 unit tests, 4 invariant tests, doc tests pass.
- `cargo test -p shogun-mcp --features server --lib`: pass — 117 tests, including REST shared-file poll.
- `cargo test -p shogun-cli --lib`: pass — 30 tests. Initial sandbox run blocked loopback with `Operation not permitted`; rerun with approved external execution passed.
- `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml -q`: pass; pre-existing warnings only.

Fix commit: `a164e7c fix: close approval ledger safety gaps`.
