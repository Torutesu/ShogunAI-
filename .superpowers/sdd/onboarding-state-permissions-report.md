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

- Signed packaged-app/device qualification remains required for real TCC transitions,
  application activation, Screen Recording restart-required behavior, denial/repair, and revoke.

## Review-fix evidence (`fix(onboarding): close permission coordinator gaps`)

- I1: application activation now refreshes app-lifetime coordinator even after UI polling stops;
  stopped onboarding generation no longer blocks revoke/regrant detection.
- I2: Screen Recording distinguishes prompt-granted restart requirement from Settings-opened
  repair pending. Effective access clears repair state; later revoke returns honest not-granted.
- I3: onboarding window destruction explicitly stops its generation. Poll worker checks generation,
  never window labels. Frontend resolves event listener, then invokes listener-ready handshake;
  native side emits one bounded initial snapshot after readiness while bootstrap IPC remains truth.
- I4: `Store::load` atomically rewrites supported v1/legacy records as v2 immediately. Failed
  migration keeps original destination and migrated managed state.
- M1: parser accepts exactly versions 1 and 2. Future/malformed versioned files fail safe and are
  never rewritten.
- M2: frontend recognizes all current semantic Rust step tags. Future-stage records render inertly;
  navigation cannot overwrite them with legacy steps.
- M3: same-directory temp names now use 128-bit OS randomness and retry eight `create_new`
  collisions. Injected stale-name test proves retry without overwriting stale temp.

Review-fix validation:

- `cargo test -p shogun-desktop-spike onboarding --lib --offline`: 22 passed.
- `cargo test -p shogun-desktop-spike permissions::tests --lib --offline`: 4 passed.
- `cargo check -p shogun-desktop-spike --lib`: passed.
- `apps/desktop/node_modules/.bin/tsc --noEmit --pretty false`: passed.
- `apps/desktop/node_modules/.bin/vitest run src/onboarding/Onboarding.test.tsx`: 4 passed.
- `rustfmt --check ...` and `git diff --check`: passed.

## Restart completion evidence

- Added a packaged-app-only `restart_onboarding` command using Tauri's supported restart request.
  Loose development executables fail closed because they do not share the installed app's TCC
  identity.
- Restart validates the current revision and exact Screen Recording step, fences/cancels active
  Scribe and voice work, then atomically persists the bundle identity and resume step before the
  restart request.
- A matching relaunched bundle consumes the marker only after Screen Recording is effective. A
  wrong bundle, wrong step, missing grant, stale revision, cancellation failure, or persistence
  failure never requests restart and never clears the recovery marker.
- Added a typed frontend restart wrapper and registered the Tauri command. Visual button wiring is
  intentionally owned by the upcoming experience UI task.

Restart validation:

- `cargo test -p shogun-desktop-spike onboarding --lib --offline`: 26 passed.
- `cargo test -p shogun-desktop-spike scribe --lib --offline`: 11 passed.
- `cargo check -p shogun-desktop-spike --lib --offline`: passed.
- `apps/desktop/node_modules/.bin/tsc --noEmit -p apps/desktop/tsconfig.json`: passed.
- `rustfmt` on touched Rust files and `git diff --check`: passed.
