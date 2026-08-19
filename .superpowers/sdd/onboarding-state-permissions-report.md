# Task 2 report: onboarding state and permission coordinator

Date: 2026-08-19

## Files

- `apps/desktop/src-tauri/src/onboarding.rs`
- `apps/desktop/src-tauri/src/permissions.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/desktop/src/onboarding/ipc.ts`
- `apps/desktop/src/onboarding/Onboarding.tsx`
- `apps/desktop/src/onboarding/Onboarding.test.tsx`

## Behavior

- Added version 2 typed onboarding state, stable semantic step tags, revision, intro/music fields,
  and typed restart marker schema. All six version 1 steps migrate explicitly while preserving
  completion, plan, trial timestamp, Accessibility skip, and repair state.
- Serialized state validation, compare-and-set mutation, atomic persistence, and managed-state
  replacement under one mutex owner. Saves use same-directory unique `create_new` temp files,
  `write_all`, `sync_all`, and atomic rename. Failed writes leave managed and destination state
  unchanged.
- Kept current six-step frontend and all-three effective-permission completion gate. Legacy
  Accessibility skip now suppresses only Accessibility repair, never missing Microphone or Screen
  Recording repair.
- Replaced Rust watcher plus React poll with one typed native coordinator: 500 ms generation-owned
  poll while onboarding exists, immediate request-completion refresh, app-activation refresh,
  monotonic snapshot revision, and edge-only events.
- Exposed compatibility booleans plus typed Accessibility, Microphone, and Screen Recording states.
  Screen Recording request success remains `restart_required` until current-process preflight is
  effective.
- Frontend now passes expected revision, consumes saved Rust records, and keeps current UI state
  after stale-write rejection. No visual, step-order, shortcut, music, window, or drag changes.

## Tests

- `cargo test -p shogun-desktop-spike onboarding --lib --offline`: 18 passed.
- `cargo test -p shogun-desktop-spike permissions::tests --lib --offline`: 3 passed.
- `cargo check -p shogun-desktop-spike --lib`: passed.
- `apps/desktop/node_modules/.bin/tsc --noEmit --pretty false`: passed.
- `apps/desktop/node_modules/.bin/vitest run src/onboarding/Onboarding.test.tsx`: 3 passed.
- `rustfmt --check ...` and `git diff --check`: passed.
- Full clippy remains blocked by pre-existing unrelated lints in
  `meeting_live_summary.rs` (`duplicated_attributes`) and `fullui.rs` (`unnecessary_cast`).
- `pnpm --dir apps/desktop typecheck` could not run because pnpm attempted a fetch and returned
  `[ERROR] fetch failed`; pinned local TypeScript/Vitest binaries passed.

## Commit

`fix(onboarding): persist permission progress safely`

## Concerns

- Native restart command, exact packaged relaunch, voice/Scribe cancellation, marker consumption,
  launcher-failure rollback, and relaunch tests are not implemented. Safety review rejected the
  process-spawn/exit patch. Typed persisted restart schema and round-trip coverage are present;
  no fake or partial restart command was added.
- Signed packaged-app/device qualification remains required for real TCC transitions,
  application activation, Screen Recording restart-required behavior, denial/repair, and revoke.
