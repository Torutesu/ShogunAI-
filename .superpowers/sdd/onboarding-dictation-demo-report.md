# Task 7 report: verified onboarding dictation practice

Date: 2026-08-19

## Result

- Added generation/nonce/session-scoped `dictation_demo` to native practice coordinator.
- Uses live production voice binding and existing local/global hold state machine. Custom valid chords render and work unchanged; unsupported bindings offer explicit default restoration.
- Ready captures exact SHOGUN-owned focused textarea through production AX target/value/caret path. Hold start captures again and requires identical AX element before opening mic.
- Only matching delivery with verified AX value readback emits content-free `dictation_inserted`. Copied, failed, cancelled, stale, ASR failure, target drift, and wrong session remain retry.
- Production processing ownership, cancellation/delivery fences, editor fallback validation, and VoiceEnd timing remain unchanged.
- Dictation enable uses production `set_voice_enabled` only after explicit click.
- Frontend listens before arm/ready, uses uncontrolled proof fields, disarms late async arms, and persists only matching success.
- Try Again performs real disarm plus new arm. Unsupported Scribe bindings expose `supports_scribe` and explicit Right Option restoration.
- Right Option global monitors now retain ownership and reconcile Accessibility revoke/regrant without restart. Existing key code 61, solo/poison, 500 ms hold, and 300 ms double-tap semantics remain covered.

## Validation

- `cargo test -p shogun-desktop-spike right_option_shortcut::tests --lib --offline`: 15 passed.
- `cargo test -p shogun-desktop-spike scribe::mac::tests --lib --offline`: 13 passed.
- `cargo test -p shogun-desktop-spike voice_session --lib --offline`: 19 passed.
- `cargo test -p shogun-desktop-spike voice_shortcut --lib --offline`: 7 passed.
- `cargo test -p shogun-desktop-spike onboarding --lib --offline`: 60 passed.
- `cargo check -p shogun-desktop-spike --lib --offline`: passed.
- `apps/desktop/node_modules/.bin/tsc --noEmit -p tsconfig.json`: passed.
- `apps/desktop/node_modules/.bin/vitest run src/onboarding/Onboarding.test.tsx src/voice-ui.test.tsx`: 42 passed.
- Scoped `rustfmt --check` and `git diff --check`: passed.

## Device-only qualification

- Signed packaged-app test remains required for live WebKit AX identity/readback, real microphone/ASR, local/global custom chord delivery, Accessibility revoke/regrant, and clipboard fallback.
