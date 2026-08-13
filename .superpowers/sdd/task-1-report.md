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
