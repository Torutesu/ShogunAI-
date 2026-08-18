# Memory MCP pending work plan

Date: 2026-08-13
Branch baseline: `mikel/meeting-recap-transcript`
Source handoff: `mcp_pending.md`
Scope: stdio `shogun-mcp`, existing REST/CLI symmetry, shared desktop approval and context paths. Streamable HTTP MCP remains parked.

## Goal

Finish immediate Memory MCP backlog without creating parallel policy or data paths:

- rate-limit runaway stdio clients;
- make MCP tool selection self-explanatory;
- expose fresh desktop context with safe DB-only fallback;
- persist and confirm `state.propose_update` as L2;
- expose FR-MT-22 meeting reads through MCP/REST/CLI;
- finish `actions.execute` concurrency, status polling, local OS effects, and live desktop smoke coverage.

Hard constraints remain unchanged:

- external sends always L3 and require dedicated UI confirmation;
- secrets remain in Keychain;
- DB, live context, proposal state, and policy stay in Rust;
- MCP, REST, CLI, and human UI remain behaviorally symmetric;
- no audio/waveform exposure or persistence;
- no HTTP MCP work in this plan.

## Architecture decisions

1. **One shared semantic catalog.** Tool names, descriptions, permission levels, argument schemas, and side-effect classes live beside `Tool` in `shogun-mcp`. MCP descriptors, REST routing tests, CLI help, and docs derive from or validate against it.
2. **One desktop local bridge.** Desktop owns a versioned Unix-domain socket under app support. Same-user peer validation, `0600` permissions, bounded messages, short timeouts. It serves explicit capabilities for cached live-context reads and acknowledged macOS local effects. It never exposes send execution or secrets.
3. **Separate L2 and L3 persistence.** L3 sends remain in locked `l3_approvals.json`. L2 state proposals use SQLite `state_proposals` because acceptance mutates state tables transactionally with provenance.
4. **Durable approval status.** Polling requires terminal status retention; dequeued approvals cannot become `unknown` immediately. Terminal records keep status and safe metadata, never full send body.
5. **Meeting API is read-only.** Four FR-MT-22 tools expose bounded session data, recap, and transcript. No audio fields. Session notes stay out of list responses and are response-redacted when explicitly fetched. Recap summary, decisions, and next actions are response-redacted.
6. **Soft Pro gate remains until Stripe.** Add shared plan-gate seam now; current Memory API enable toggle remains authoritative. Trial remains Pro-equivalent.

## Delivery order

### Task 0 — Baseline and contracts (WP0)

Purpose: remove known baseline noise and freeze wire contracts before parallel implementation.

Work:

- Fix stale `audio_probe` `TranscriptSink::emit` signature so full workspace examples compile.
- Record current tool catalog and response snapshots.
- Define stable error envelopes:
  - rate limited: JSON-RPC `-32029`, `retryAfterMs`;
  - desktop unavailable/timeout;
  - proposal invalid/not found/already decided;
  - meeting not found/Pro required;
  - approval unknown/terminal status.
- Confirm latest refinery migration number before adding `state_proposals`.

Acceptance:

- Relevant workspace tests compile apart from environment-only loopback restrictions.
- Wire-contract tests fail before implementation when expected behavior is absent.

### Task 1 — Approval store safety and action status (WP1)

Purpose: make existing L3 vertical slice safe before adding more clients or polling.

Files:

- `crates/shogun-agents/src/approval.rs`
- `crates/shogun-mcp/src/approval_store.rs`
- `crates/shogun-mcp/src/mcp.rs`
- `crates/shogun-mcp/src/rest.rs`
- `crates/shogun-cli/src/*`
- `apps/desktop/src-tauri/src/approvals.rs`

Work:

1. Add advisory sidecar lock held across full load/mutate/save transaction.
2. Keep atomic replace; use unique temp names and preserve restrictive permissions.
3. Treat corrupt store as error, not empty queue.
4. Add durable statuses: `pending`, `rejected`, `timed_out`, `sent`, `send_failed`, `draft_saved`.
5. Remove full body from terminal records; add bounded terminal-record retention.
6. Add `actions.poll` plus matching REST route and CLI command.
7. Make desktop confirm/reject/send outcome update same locked status ledger.

Acceptance:

- Concurrent enqueue/confirm/reject loses no rows and never reuses IDs.
- MCP enqueue returns ID; MCP/REST/CLI poll report same status after restart.
- No send occurs before dedicated Confirm; Enter alone remains inert.

### Task 2 — Rate limits and semantic tool catalog (WP2)

Purpose: protect stdio process and help agents choose correct tools.

Files:

- new pure limiter module in `crates/shogun-mcp/src/`
- `crates/shogun-mcp/src/memory_api.rs`
- `crates/shogun-mcp/src/mcp.rs`
- `crates/shogun-core/src/bin/shogun_mcp.rs`
- `docs/memory-api-mcp.md`
- `docs/mcp-setup.html`

Work:

1. Add deterministic token-bucket limiter with injected clock.
2. Exempt protocol housekeeping: initialize, initialized notification, ping, tools/list.
3. Separate budgets for reads, writes, local actions, and L3 enqueue. Initial defaults:
   - reads: 60/minute, burst 10;
   - writes: 20/minute, burst 5;
   - local actions: 10/minute, burst 2;
   - L3 enqueue: 5/minute, burst 1.
4. Reject before DB work, desktop IPC, or approval enqueue.
5. Use process-scoped identity for tokenless dev mode and non-reversible token identity when issued tokens exist. Never log or persist bearer values.
6. Centralize tool purpose, level, side effects, required arguments, and fallback behavior.
7. Generate strict schemas with required fields, field descriptions, and `additionalProperties: false` where client-compatible. Use action-specific `oneOf` only after Cursor/Claude compatibility smoke.

Acceptance:

- Deterministic refill/burst tests pass.
- Rate-rejected L3 calls leave approval store unchanged.
- Every tool has level, side-effect, required-field, and error/fallback descriptions.
- Wire names and permission levels remain identical across MCP/REST/CLI.

### Task 3 — Durable L2 state proposals (WP3)

Purpose: replace `state.propose_update` stub with explicit, provenance-safe confirmation.

Files:

- new next-numbered `crates/shogun-memory/src/migrations/V*__state_proposals.sql`
- `crates/shogun-memory/src/state.rs`
- `crates/shogun-core/src/daemon.rs`
- `crates/shogun-core/src/db_backend.rs`
- `crates/shogun-mcp/src/backend.rs`
- `crates/shogun-mcp/src/{memory_api,mcp,rest}.rs`
- `crates/shogun-cli/src/*`
- desktop Rust commands and Approvals/Notch UI

Contract:

```json
{
  "table": "commitments",
  "operation": "insert",
  "values": {
    "direction": "mine",
    "description": "Send report",
    "due_at": 1780000000000,
    "status": "open",
    "project_id": 4
  },
  "provenance_event_ids": [123],
  "proposed_confidence": 0.72
}
```

Work:

1. Add SQLite proposal queue with status, typed target/operation, validated payload, provenance IDs, origin, timestamps, and decision event ID.
2. Return `{pending:true, proposal_id, level:"L2"}` from all API faces.
3. Add desktop list/accept/reject surface. Do not reuse L3 send queue.
4. On Confirm, one SQLite transaction:
   - conditionally claim pending proposal;
   - append user `state_update` event;
   - apply typed state mutation;
   - link acceptance and original evidence provenance;
   - set accepted confidence to `1.0`;
   - mark proposal accepted.
5. Reject/expire without state mutation. Duplicate decisions remain idempotent.

Acceptance:

- Invalid table, fields, enums, confidence, or provenance fail before persistence.
- Acceptance atomically writes event, state row/update, provenance, and decision status.
- Failure rolls back all four; concurrent confirms have one winner.
- MCP/REST/CLI submission and desktop confirmation share same proposal ID.

### Task 4 — FR-MT-22 meeting reads (WP4)

Purpose: expose existing Rust meeting data through symmetric, bounded, private read APIs.

Tools:

- `meeting.sessions.list`
- `meeting.sessions.get`
- `meeting.recap.get`
- `meeting.transcript.get`

REST/CLI:

- `GET /v1/meetings/sessions` / `shogun meetings list`
- `GET /v1/meetings/sessions/:id` / `shogun meetings get <id>`
- `GET /v1/meetings/:id/recap` / `shogun meetings recap <id>`
- `GET /v1/meetings/:id/transcript` / `shogun meetings transcript <id>`

Files:

- `crates/shogun-memory/src/{session,session_notes,meeting_recaps,transcript_segments}.rs`
- `crates/shogun-core/src/{daemon,db_backend}.rs`
- `crates/shogun-mcp/src/{backend,memory_api,mcp,rest}.rs`
- `crates/shogun-cli/src/*`

Work:

1. Add bounded session list query with time range, limit, and explicit `include_open`.
2. Add typed core DTOs. Preserve transcript `ts`, nullable speaker, origin, and confidence; current helper drops origin/confidence.
3. Batch-hydrate session metadata to avoid N+1 reads.
4. Add four read-level tools and exact schemas.
5. Apply confidence filtering to transcript lines.
6. Return no audio/PCM/waveform/provider fields.
7. Response-redact explicit notes and all recap text fields. Never log content.
8. Add shared plan-gate seam; keep enable-toggle soft gate until Stripe WP5.1.

Acceptance:

- Same seeded DB yields equivalent MCP/REST/CLI JSON.
- List is ordered, bounded, closed-only by default.
- Missing recap/transcript uses stable empty/not-found contract.
- Low-confidence lines excluded by default; opt-in marks them `possibly`.
- No meeting read can reach action/send paths.

### Task 5 — Desktop bridge, live context, and local OS effects (WP5)

Purpose: provide data only running desktop owns while preserving standalone stdio architecture.

Files:

- new shared bridge protocol/store module in `crates/shogun-core/src/`
- `apps/desktop/src-tauri/src/{integrate,notch_exec,lib}.rs`
- `crates/shogun-core/src/db_backend.rs`
- `crates/shogun-mcp/src/dispatch.rs`

Work:

1. Define versioned bounded DTOs and explicit capabilities:
   - `context.get_cached`;
   - `local.open_app`;
   - `local.reveal_file`;
   - `local.copy_to_clipboard`;
   - `local.show_notification`.
2. Move live-context ownership from desktop-only `Shared.last_context` into Rust `LiveContextStore`.
3. Preserve preassembled capture: bridge reads cache only and never triggers AX collection.
4. Publish explicit `fresh`, `empty`, `excluded`, `unavailable`, and stale states. Excluded/empty must clear old text.
5. Start Unix socket under app support with `0600`, same-user peer validation, protocol version, payload limits, and 50–100ms client timeout.
6. Compose fresh live snapshot with DB context. Desktop absent, stale, permission-revoked, timeout, or mismatch returns DB-only result plus availability metadata.
7. Replace logging-only local action success with explicit macOS effectors and real acknowledgements. Validate bundle IDs and paths; never log payload text.
8. Keep local effects separate from L3 approvals. Sends are not bridge capabilities.

Acceptance:

- Focus change reaches stdio `memory.get_context` without new AX call.
- Excluded app never leaks previous context.
- Desktop exit returns DB-only within timeout.
- OS actions report actual success/failure; no fake `executed:local` response.
- Bridge cannot invoke any external send.

### Task 6 — Integration, live smoke, docs, backlog (WP6)

Work:

1. Add macOS test harness using isolated app-data, DB, Keychain test accounts, and fake send transports.
2. Exercise:
   - enable/token/fail-closed startup;
   - rate limit and recovery;
   - live context plus desktop-down fallback;
   - L2 proposal submit/confirm/reject;
   - meeting reads through all three faces;
   - L3 enqueue/UI confirm/poll terminal result;
   - concurrent approval writers;
   - real local-effect acknowledgements.
3. Run secret/content log scans and migration guard.
4. Update `docs/memory-api-mcp.md`, `docs/mcp-setup.html`, `mcp_pending.md`, and `todo.md` only when acceptance evidence exists.

Acceptance commands:

```bash
cargo test -p shogun-agents
cargo test -p shogun-memory
cargo test -p shogun-mcp
cargo test -p shogun-core --features db
cargo test -p shogun-cli
cargo clippy --workspace --exclude shogun-desktop-spike --all-targets -- -D warnings
pnpm --filter @shogun-ai/desktop typecheck
python3 scripts/check-migrations.py
python3 scripts/check-secret-exposure.py
python3 scripts/check-http-egress.py
```

macOS desktop checks run separately because Tauri/AppKit and Keychain paths are device-bound.

### Task 7 — Live connector execution parity

Purpose: make `actions.execute` truthful for released providers. Current pure routing is correct,
but desktop runtime is Gmail-only: `RemoteMcpTransport<ComposioReadRpc>` rejects Calendar calls,
and `connect_service` only marks Calendar connected without running OAuth.

Current provider contract:

- Gmail read/draft/send: Composio, opt-in required; send always L3 and draft-stop aware.
- Google Calendar read/create: official remote MCP with direct OAuth; create always L3.
- Slack: Wave 2, unavailable at Wave 1. Never claim execution while gated.
- GitHub: Wave 3, unavailable at Wave 1. Tool names remain provisional until live schema check.

Files:

- `apps/desktop/src-tauri/src/connectors.rs`
- `apps/desktop/src-tauri/src/approvals.rs`
- `crates/shogun-core/src/mcp_http.rs`
- `crates/shogun-integrations/src/{rpc,transport,runtime,oauth_flow,token,toolmap}.rs`
- connector settings commands/UI only as required for OAuth completion state

Work:

1. Add service-dispatching `McpRpc`: Gmail calls use `ComposioReadRpc`; released non-Gmail first-layer
   calls use `HttpMcpRpc<ManagedTokenProvider<KeychainTokenStore>>`.
2. Wire Google OAuth loopback for Calendar with least-privilege endpoint scopes and Keychain token
   storage. `connect_service` must complete OAuth before marking connected.
3. Refresh Calendar access token through existing `ManagedTokenProvider`; never expose tokens to
   shell, files, DB, logs, or MCP responses.
4. Keep Gmail send structurally unreachable from official first-layer MCP.
5. Make Calendar create return actual remote result/failure after L3 confirmation; never report
   success from route selection alone.
6. Return explicit `unreleased` for Slack/GitHub at Wave 1. Do not silently invoke provisional tools.
7. Add live `tools/list` schema probe for released first-layer providers and validate configured
   tool names/argument fields before enabling writes. Content-free diagnostics only.

Acceptance:

- Gmail send remains Composio L3 with consent + draft-stop gates and traceability.
- Calendar connect stores OAuth token set in Keychain; confirmed create reaches Calendar MCP.
- Disconnected/expired Calendar returns explicit failure and never claims sent.
- Slack/GitHub refuse execution at Wave 1.
- Fake-RPC integration tests cover provider dispatch and verify Gmail/Calendar cannot cross routes.
- macOS smoke uses test calendar/account or fake remote transport; no real external side effect in CI.

## Parallel ownership after contracts freeze

- Worker A: WP1 approval locking/status.
- Worker B: WP2 limiter/catalog.
- Worker C: WP3 state proposal persistence.
- Worker D: WP4 meeting reads.
- Worker E: WP5 bridge/live context/local effects.
- Coordinator: WP0 contract freeze, conflict resolution in shared `mcp.rs`/`memory_api.rs`, WP6 integration.

Workers B–E must avoid editing shared catalog/dispatcher files concurrently until coordinator lands interface stubs. Core/storage work can proceed in parallel behind those interfaces.

## Main risks

- `mcp.rs`, `memory_api.rs`, `db_backend.rs`, and CLI grammar are shared hotspots; interface-first commits prevent merge conflict and semantic drift.
- Tokenless stdio process-trust differs from strict FR-API wording. Preserve current documented dev behavior in this plan; resolve product policy separately.
- Same-user Unix sockets protect against other users, not a compromised same-user process. Keep capability surface narrow and exclude sends/secrets.
- State proposal payload must be finalized before migration/API/UI work.
- Meeting notes and recap structured fields may contain historical unredacted secrets; response-time redaction is mandatory even after write-path hardening.
- Live E2E must use fake transports to prevent accidental external sends.

## Definition of done

- Immediate backlog entries have passing unit, symmetry, and live smoke evidence.
- MCP remains stdio-only.
- No send can bypass L3 dedicated confirmation.
- No proposal can mutate state before L2 confirmation.
- No API or bridge stores secrets outside Keychain or logs user content.
- Desktop-down behavior is bounded and useful, not a crash or hang.
- `todo.md` and `mcp_pending.md` match shipped behavior.
