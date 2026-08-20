# Memory MCP — handoff (continue here)

Self-contained brief so another session (e.g. Claude Desktop / Cursor) can pick up **Memory MCP** work with no prior chat. Scope today: **stdio `shogun-mcp` first** — not HTTP MCP.

Product context: SHOGUN Memory API is three faces (invariant 6): MCP / REST / CLI. This track is the stdio MCP face + shared L3 approvals with the desktop app.

---

## Branch / commits

- **Branch:** `mikel/meeting-recap-transcript`
- **Remote:** tracking `origin/mikel/meeting-recap-transcript`, **ahead by 5** (MCP commits not pushed)
- **Untracked leftovers (ignore unless relevant):** `TECHNICAL_DEBT.md`, `apps/desktop/src/usePointerMove.ts`, `.pnpm-store/` — do not casually edit `TECHNICAL_DEBT.md`

### Notable local commits (unpushed)

| Hash | One-liner |
|------|-----------|
| `5090806` | feat(mcp): Memory API enable gate and whoami/context tools |
| `0a8a803` | feat(mcp): Keychain tokens and Settings UI for Memory API |
| `30eb92b` | docs(mcp): stdio recipes for Cursor and Claude Desktop |
| `93868b3` | docs: reprioritize Memory MCP backlog after stdio ship |
| `1f90ba8` | feat(mcp): share L3 approval queue for actions.execute |

Also on this branch: meeting-recap/transcript work (unrelated to Memory MCP unless you touch FR-MT-22).

---

## Done (P0)

- [x] stdio `shogun-mcp` + enable gate (`memory_api.json`)
- [x] Settings → **Memory API** (toggle, profile, Issue/Revoke tokens → Keychain)
- [x] `profile.whoami` + `profile.set` (L1) + CLI/REST symmetry
- [x] `memory.get_context` (DB facts/notes; **not** live AX / Notch cache)
- [x] `memory.append_note` / search (pre-existing + smoke)
- [x] Docs + client examples: `docs/memory-api-mcp.md`, `docs/examples/*-shogun.json`
- [x] Setup page: `docs/mcp-setup.html` (landed with `1f90ba8`)
- [x] Smoke e2e (stdio) — **passed 2026-08-12** on `dev.shogun.spike` DB: whoami, get_context, append_note, search, fail-closed
- [x] `actions.execute` **vertical slice**: MCP enqueue → shared `l3_approvals.json` → Settings Approvals / Notch → confirm → existing send exec (same Memory API enable + token gate)

---

## Auth

Fail-closed by design.

1. **Enable:** Settings → Memory API → On. Writes `memory_api.json` next to the live DB. Missing file or `"enabled": false` → `shogun-mcp` / `shogun-api` exit nonzero with a clear stderr message.
2. **Tokens:** Issue in Settings → Memory API. Secret lives in **Keychain** (not the JSON settings file). Put the shown-once value in the client env as `SHOGUN_API_TOKEN`.
   - If **any** tokens exist → matching `SHOGUN_API_TOKEN` is **required**.
   - If **none** yet → process-trust allowed when enabled (dev DX).
3. **DB path (typical macOS app data):**
   ```text
   ~/Library/Application Support/com.selectkk.shogun/
     memory.db
     memory_api.json
     l3_approvals.json
   ```
4. **Env overrides (standalone bins):**
   - `SHOGUN_DB_PATH` (default `./shogun.db` for standalone)
   - `SHOGUN_MEMORY_API_SETTINGS` → else `<parent of DB>/memory_api.json`
   - `SHOGUN_L3_APPROVALS` → else `<parent of DB>/l3_approvals.json`
   - `SHOGUN_API_TOKEN`
5. **Encrypted DB + unsigned binary:** signed desktop can read the Keychain DB key; **unsigned `shogun-mcp` needs `SHOGUN_DB_KEY`** (hex from Keychain) or open fails. Soft Pro gate: Enable toggle is the product gate until Stripe WP5.1; trial counts as Pro-equivalent.

---

## `actions.execute` flow

Never auto-send. Sends are always L3.

```text
MCP tools/call actions.execute (send_*)
  → enqueue PendingRecord
  → persist l3_approvals.json (approval_store)
  → return { pending, approval_id }

Desktop (Settings → Approvals / Notch activity)
  → load same file
  → user Confirm (dedicated button; Enter alone never sends) or Reject
  → existing send execution path
```

- Locals like `local_search` can run immediately (hybrid search).
- File store is atomic temp-replace; concurrent writers are best-effort (last writer wins) — proper file lock still TODO.
- OS local actions (`open_app`, etc.) via desktop effector: **not** full parity yet.

---

## Immediate backlog

From `todo.md` + execute parity gaps:

1. **Rate limits** — stop runaway clients hammering the API
2. **Better tool docs** — clearer MCP tool descriptions so agents pick the right tool
3. **Live context from running desktop app** — focus / screen context, not cold DB-only `memory.get_context`
4. **Persist `state.propose_update`** — accepting a proposal must stick in DB (not stub)
5. **Meeting tools (FR-MT-22)** — sessions / recap / transcript via Memory API (Pro for API face)
6. **Execute parity**
   - OS locals via desktop effector (`open_app`, etc.)
   - File lock under concurrent writers (`l3_approvals.json`)
   - Poll-by-`approval_id` tool
   - Live e2e smoke with desktop running (MCP enqueue → Approvals UI → confirm)

stdio only for now (limit surface / misuse). See `todo.md` § Memory MCP — Immediate.

---

## Later / parked

- Streamable **HTTP MCP** on `127.0.0.1` — park; stick to stdio
- **Stripe WP5.1** hard Pro gate (Enable is soft today)
- Richer **whoami** (Unabyss-style prefs graph / auto-learning) — v1 = Settings profile + work counts/names

Unrelated app bugs in `todo.md` (offline crash, Settings polish) — out of scope unless blocking MCP.

---

## Key files

| Path | Role |
|------|------|
| `crates/shogun-mcp/` | MCP/REST shared crate |
| `crates/shogun-mcp/src/mcp.rs` | stdio tools, `actions.execute` |
| `crates/shogun-mcp/src/approval_store.rs` | `l3_approvals.json` load/save |
| `crates/shogun-mcp/src/memory_api_settings.rs` | enable gate + profile JSON |
| `crates/shogun-mcp/src/memory_api.rs` | tool enum / naming |
| `crates/shogun-mcp/src/dispatch.rs` | execute local vs L3 send |
| `crates/shogun-core/src/bin/shogun_mcp.rs` | stdio binary entry |
| `crates/shogun-core/src/db_backend.rs` | DB-backed Memory tools |
| `apps/desktop/src-tauri/src/memory_api_settings.rs` | Settings IPC + Keychain tokens |
| `apps/desktop/src-tauri/src/approvals.rs` | desktop shared approval queue |
| `apps/desktop/src/App.tsx` | Settings → Memory API UI |
| `docs/memory-api-mcp.md` | stdio recipes (Cursor / Claude) |
| `docs/mcp-setup.html` | human setup page |
| `docs/examples/cursor-mcp.shogun.json` | Cursor mcp.json template |
| `docs/examples/claude-desktop.shogun.json` | Claude Desktop template |
| `todo.md` | live backlog (keep in sync when shipping) |

---

## Smoke-test recipes

**Build**

```bash
cargo build -p shogun-core --features db --bin shogun-mcp --release
# → target/release/shogun-mcp
```

**Client env (minimal)**

```json
{
  "command": "/ABS/PATH/TO/target/release/shogun-mcp",
  "env": {
    "SHOGUN_DB_PATH": ".../com.selectkk.shogun/memory.db",
    "SHOGUN_MEMORY_API_SETTINGS": ".../com.selectkk.shogun/memory_api.json",
    "SHOGUN_API_TOKEN": "shogun_…",
    "SHOGUN_DB_KEY": "<hex if unsigned binary + encrypted DB>"
  }
}
```

**Happy path**

1. Desktop: Enable Memory API + Issue token.
2. Wire Cursor or Claude Desktop per `docs/memory-api-mcp.md`.
3. Call `profile.whoami` → `memory.get_context` → `memory.append_note`.
4. Optional: `actions.execute` with a send kind → expect `{pending, approval_id}` → confirm in Settings → Approvals (dedicated Confirm; Enter alone must not send).
5. Disable Memory API → restart MCP → process must refuse (fail-closed).

Full recipes: `docs/memory-api-mcp.md`. Visual setup: open `docs/mcp-setup.html`.

---

## Hard constraints

- **Do not `git push`, open/edit GitHub issues, or open/update PRs** without an explicit user yes (workspace rule).
- **L3 for all sends** — never auto-send; UI Confirm required (invariant 4: no external-send in L1).
- **UI ↔ API symmetry** — new Memory surfaces must work from MCP/CLI/REST as well as Settings (invariant 6).
- **No image/audio persistence** as product default (visual recall / meeting ASR are documented exceptions in `CLAUDE.md` — do not invent new ones).
- **Secrets only in Keychain** — tokens, OAuth, BYOK, DB key; never plain files/DB/logs (invariant 7).
- **Don’t casually edit `TECHNICAL_DEBT.md`** (untracked leftover; not part of this track).
- Data gravity stays in Rust core; webview is not the data layer.

---

## Ops

- [ ] **Push** the five MCP commits (`5090806` … `1f90ba8`) — **only after user asks**
- [ ] **Cursor wire** — point Cursor MCP config at local release `shogun-mcp` with DB path + settings + token (+ `SHOGUN_DB_KEY` if needed)
- [ ] Keep `todo.md` checkboxes honest when closing backlog items

When continuing: prefer extending the shared approval / Memory API paths above rather than a parallel queue. Start with rate limits or execute parity if unblocking clients; FR-MT-22 if meeting-recap branch work is already in flight.
