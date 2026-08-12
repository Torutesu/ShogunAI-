# TODO

## Unrelated / app bugs

- [ ] **Offline crash** — app crashes when no internet. Repro/fix TBD (startup network call? model fetch? updater?). Tracked 2026-08-11.
- [ ] **Settings page polish** — fix Meeting Notes clutter, hierarchy, and general polish. Deferred until after meeting UI fix is committed; remind then.

## Memory MCP — Done

- [x] stdio `shogun-mcp` + enable gate (`memory_api.json`)
- [x] Settings → Memory API (toggle, profile, Issue/Revoke tokens → Keychain)
- [x] `memory.get_context` (DB facts/notes; not live AX)
- [x] `profile.whoami` + `profile.set` (L1) + CLI/REST
- [x] Docs + examples: `docs/memory-api-mcp.md`, `docs/examples/*-shogun.json`
- [x] Smoke end-to-end (stdio) — passed 2026-08-12 on `dev.shogun.spike` DB: whoami, get_context, append_note, search, fail-closed. Note: unsigned `shogun-mcp` needs `SHOGUN_DB_KEY` from Keychain for encrypted DB.
- [x] `memory.append_note` (pre-existing)

## Memory MCP — Immediate

stdio MCP only for now (limit to agents, prevent misuse).

- [ ] **`actions.execute`** — **Wired (stdio).** Shared `l3_approvals.json` queue: MCP enqueue → Settings → Approvals / Notch activity list → confirm/reject → existing send exec. Auth: same Memory API enable + token gate as other tools. Still TODO for full parity: OS-effect locals (`open_app` etc.) via desktop effector; file-lock under concurrent writers; poll-by-approval_id tool; e2e smoke with live desktop.
- [ ] **Rate limits** — stop runaway clients hammering the API
- [ ] **Better tool docs** — clearer MCP tool descriptions so agents pick the right tool
- [ ] **Live context from running app** — what’s on screen / focus now from desktop app, not cold DB-only snapshot
- [ ] **Persist `state.propose_update`** — accepting a proposal must stick in the DB (not stub)
- [ ] **Meeting tools (FR-MT-22)** — sessions / recap / transcript via Memory API

## Memory MCP — Later

- [ ] **Streamable HTTP MCP** on 127.0.0.1 — park; stick to stdio for now
- [ ] **Stripe WP5.1 hard Pro gate** — today Enable toggle is soft gate (trial OK)
- [ ] **whoami depth** — richer Unabyss-style prefs graph / auto-learning prefs if needed (v1 = Settings profile + work counts/names)

## Ops / ship

- [ ] **Push MCP commits** if not on origin yet (`5090806`…`30eb92b`)
- [ ] **Cursor wire** — point Cursor at local stdio `shogun-mcp` once pushed/ready
