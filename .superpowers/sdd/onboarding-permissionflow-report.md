# Task 4 report: PermissionFlow-native permission drag

Date: 2026-08-19

## Files

- `apps/desktop/src-tauri/src/permission_drag.rs`
- `apps/desktop/src-tauri/src/onboarding.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/desktop/src-tauri/Cargo.toml`
- `docs/third-party/PermissionFlow-LICENSE.txt`

## Behavior

- Replaced the process-wide webview drag monitor with a genuine AppKit `NSView` drag source. The
  native view owns mouse-down, the strict greater-than-four-point Euclidean threshold, mouse-up
  cancellation, and `NSDraggingSession` begin/end callbacks.
- Added the complete PermissionFlow pasteboard matrix: `public.file-url`, `public.url`, legacy
  `NSFilenamesPboardType`, promised file URL, and UTF-8 path text. Every type resolves from the same
  canonical running `.app` URL.
- Extracted Task 2's runtime identity into one shared validator. It requires the canonical Tauri
  current binary, exact `.app/Contents/MacOS` shape, current `NSBundle` identifier, matching bundle
  executable URL, and canonical bundle URL containing that executable. Loose and nested tools fail
  closed.
- Added one generation-owned, borderless, nonactivating `NSPanel` helper. It can never become key or
  main, follows the real System Settings layer-0 window by bundle identity, stays adjacent without
  overlapping when display space permits, and closes after Settings disappears or onboarding ends.
- Drag begin makes the helper mouse-transparent, fades it, and sends it behind Settings. Drag end
  restores opacity, input, and front ordering. Stale generations cannot restore a replacement.
- Captures the previous foreground application and restores it on cleanup. Cleanup is idempotent,
  cancels the tracker, closes the panel, and runs on AppKit main.
- All permission commands still cross Task 3's external-window barrier before helper preparation
  and the native permission request. Accessibility and Screen Recording receive drag sessions;
  Microphone explicitly remains prompt/Settings-only with no drag helper.
- Step transition, completion, restart, Settings close, onboarding-window disappearance, request
  failure, and explicit close all clean controller state. The former arm/disarm IPC remains a
  compatibility surface, while typed show/close commands expose the native helper lifecycle.
- Corrected the vendored PermissionFlow v2.11.2 MIT notice to the pinned upstream license identity.

## Tests

- `cargo test -p shogun-desktop-spike permission_drag::tests --lib --offline`: 15 passed.
- `cargo test -p shogun-desktop-spike onboarding --lib --offline`: 53 passed.
- `cargo check -p shogun-desktop-spike --lib --offline`: passed.
- `rustfmt --check` on touched Rust files and `git diff --check`: passed.

The focused drag suite covers threshold boundary and diagonal distance, mouse-up invalidation,
pasteboard type/value contracts, one-helper ownership, Microphone no-drag behavior, drag begin/end,
Settings cleanup, stale generation rejection, idempotent cleanup, barrier ordering, and packaged
bundle path shape.

## Device-only risks

- Final qualification requires a signed packaged build because TCC consumes the code-signature and
  bundle identity; development binaries intentionally cannot produce a drag payload.
- Real System Settings acceptance of all pasteboard representations, drop highlighting, z-order,
  multi-display placement, Space behavior, and foreground-app restoration require macOS device QA.
- System Settings' bundle identity is stable, but its window layout remains OS-owned; the tracker
  intentionally follows the largest available layer-0 Settings window rather than private views.

## Commit

`fix(onboarding): match PermissionFlow native drag`
