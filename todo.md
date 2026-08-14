# TODO

## Unrelated / app bugs

- [ ] **Offline crash** — app crashes when no internet. Repro/fix TBD (startup network call? model fetch? updater?). Tracked 2026-08-11.
- [ ] **Settings page polish** — fix Meeting Notes clutter, hierarchy, and general polish. Deferred until after meeting UI fix is committed; remind then.

## Desktop app — Priority fixes

- [ ] **[P1] Managed voice transcript editor** — add a dedicated fast cloud edit lane after Deepgram for filler/false-start cleanup and destination-aware formatting, with strict preservation validation, processing lock, consent/traceability, and silent raw fallback. Full implementation spec: [`docs/voice-edit-model-implementation.md`](docs/voice-edit-model-implementation.md).
- [ ] **[P0] Meeting overlay performance + live summaries** — remove 60 fps React rerenders, memoize transcript transforms, make 22 s live-summary scheduling survive incoming speech, and rAF-throttle resize IPC.
- [ ] **[P0] Real frontend quality gates** — add Vitest, React Testing Library, ESLint/React Hooks checks, desktop `test`/`lint` scripts, and CI enforcement; cover meeting scheduling, transcript transforms, timers, visual recall, and resize batching without requiring live Tauri.
- [ ] **[P1] Bound frontend async work** — rAF-batch visual-recall scrubbing, debounce/cancel stale JPEG loads, avoid unchanged 12 s preview reloads, own and cancel inline/toast timers, and cap voice watchdog lifetime.
- [ ] **[P1] Surface memory DB failures** — stop converting lock/query failures into successful empty-memory results; use explicit errors, content-free logging, and a recoverable Notch degraded-state indicator.
- [ ] **[P1] Harden desktop trust boundaries** — disable global Tauri API, add restrictive CSP without breaking visual-recall previews, separate LLM system instructions from untrusted draft context, and preserve L3 approval/full-body preview.

## Memory MCP — Immediate

stdio MCP only for now (limit to agents, prevent misuse).

- [ ] **`actions.execute` remaining parity** — wire OS-effect locals (`open_app` etc.) through the desktop effector and run an end-to-end smoke with the live desktop.
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

- [ ] **Cursor wire** — point Cursor at local stdio `shogun-mcp` once pushed/ready
