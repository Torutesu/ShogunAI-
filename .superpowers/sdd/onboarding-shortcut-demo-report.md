# Task 6 report: native Right Option and Scribe onboarding proof

Date: 2026-08-19

## Result

- Extracted the production solo-modifier tap observer from `lib.rs` into one native module.
- Global production events and pass-through local onboarding events now feed the same clean-tap
  state machine: right Option key code 61, solo/unpoisoned input, 500 ms hold ceiling, 300 ms
  double-tap cancellation, and delayed single dispatch remain unchanged.
- Added prepare/ready/disarm commands scoped by onboarding revision, semantic step, native window
  generation, monotonic demo generation, and random nonce. Local observation stays dormant until
  the interactive onboarding surface focuses/selects its field and calls ready.
- Added content-free `onboarding-shortcut` events with generation, nonce, stage, optional Scribe
  session id, and typed outcome. Only `single_tap` can complete `right_option`; only a matching
  `scribe_inserted` can complete `scribe_demo`. `no_key`, `failed`, `cancelled`, and `stale` remain
  retry outcomes.
- Double-tap opens normal Scribe. Native proof requires SHOGUN's PID, the exact Rust-provided seed,
  a full UTF-16 selection, the matching Scribe session, normal focus restoration/AX commit fences,
  and post-insert AX value readback. No DOM event or direct DOM mutation proves success.
- Live draft binding remains Rust-owned. Arm returns the current binding and whether it supports
  the production double-tap Scribe gesture. No shortcut setting is overwritten.

## Files

- `apps/desktop/src-tauri/src/right_option_shortcut.rs`
- `apps/desktop/src-tauri/src/scribe.rs`
- `apps/desktop/src-tauri/src/lib.rs`

## Tests

- `cargo test -p shogun-desktop-spike right_option_shortcut::tests --lib --offline`: 11 passed.
- `cargo test -p shogun-desktop-spike scribe::mac::tests --lib --offline`: 13 passed.
- `cargo test -p shogun-desktop-spike onboarding --lib --offline`: 53 passed.
- `cargo check -p shogun-desktop-spike --lib --offline`: passed.
- `rustfmt` on Task 6 Rust files and `git diff --check`: passed.

## Honest limits

- No deterministic onboarding editor was added. Fresh installs without a configured Agent lane
  receive `no_key` and stay retry; generation failure, cancellation, stale scope, wrong field,
  wrong session, or failed AX readback never advance.
- Signed packaged-device qualification remains required for local/global NSEvent delivery,
  WebKit AX textarea selection/readback, real focus restoration, custom bindings, left Option,
  poisoned holds, and double-tap timing.
- Frontend listener/persistence wiring is owned by the parallel onboarding UI task and is not part
  of this native commit.
