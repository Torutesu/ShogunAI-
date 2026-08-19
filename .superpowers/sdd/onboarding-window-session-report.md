# Task 3 report: cursor-display onboarding window session

Date: 2026-08-19

## Files

- `apps/desktop/src-tauri/src/onboarding_windows.rs`
- `apps/desktop/src-tauri/src/onboarding.rs`
- `apps/desktop/src-tauri/src/geometry.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/desktop/src/onboarding/ipc.ts`

## Behavior

- Added one generation-owned native session for cursor-selected main cinematic, genuine
  nonactivating ambient `NSPanel` surfaces, and normal-level interactive onboarding.
- Uses paired AppKit/CoreGraphics geometry with half-open cursor selection and native AppKit frames
  for negative, offset, above/below, and mixed-scale display layouts.
- Owns exact five-second monotonic intro deadline, atomic hidden-first surface reveal, CAS-backed
  `intro_complete` persistence, display-change generation replacement, and stale callback rejection.
- Ambient panels ignore mouse events, explicitly reject key/main status, host only generation-tagged
  ambient routes, and return their WKWebViews to hidden Tauri hosts during deterministic teardown.
- Interactive window is 1120x720, minimum 900x620, current-Space, normal level, activated and made
  key without `OVERLAY_LEVEL` or always-on-top behavior.
- Cleanup is idempotent, synchronously marshalled to AppKit main thread, stops permission polling,
  removes the owned display observer, closes every owned surface, and invalidates delayed work.
- All three explicit permission actions pass through one synchronous barrier that destroys intro
  surfaces and lowers interactive onboarding before any prompt or System Settings action.
- Added typed generation-tagged surface IPC. Routes carry generation bootstrap data and stale
  generation lookups fail closed.

## Tests

- `cargo test -p shogun-desktop-spike onboarding_windows::tests --lib --offline`: 18 passed.
- `cargo test -p shogun-desktop-spike onboarding --lib --offline`: 49 passed.
- `cargo test -p shogun-desktop-spike permissions::tests --lib --offline`: 4 passed.
- `cargo test -p shogun-desktop-spike voice_session --lib --offline`: 19 passed.
- `cargo check -p shogun-desktop-spike --lib --offline`: passed.
- `apps/desktop/node_modules/.bin/tsc --noEmit -p apps/desktop/tsconfig.json`: passed.
- `apps/desktop/node_modules/.bin/vitest run src/onboarding/Onboarding.test.tsx`: 6 passed.
- `rustfmt --check` on touched Rust files and `git diff --check`: passed.

## Device-only risks

- Signed packaged-app qualification still required for WindowServer behavior across real mixed-DPI
  displays, Space/full-screen transitions, hot-plug during intro, and NSPanel WKWebView reparenting.
- System Settings and native consent-sheet z-order must be confirmed on macOS 14 and current macOS.
- Frontend cinematic/ambient visuals are intentionally deferred; native routes and lifecycle exist.

## Commit

`feat(onboarding): own cursor display window session`
