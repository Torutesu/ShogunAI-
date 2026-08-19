# Task 8 report: native onboarding music

## Result

- Added one Rust-owned native `AVAudioPlayer` for bundled CC0 `yoiyami_core_theme.mp3`; no web audio path exists. Asset manifest now records native playback.
- Player starts at 0.50, loops, and settles monotonically at 0.40. Generation checks fence delayed fade/voice callbacks from replacement sessions.
- Mute is interactive in every onboarding phase, persists through existing atomic Store CAS/revision, and returns saved Rust state.
- Cleanup stops/releases player on completion, close, restart, generation replacement, and app exit. External System Settings mode leaves music running.
- Voice capture uses existing authoritative voice-session state to pause/resume. Reduced Motion does not change audio.
- Decode/playback failures are silent and cannot block onboarding.

## Tests

- `cargo check -p shogun-desktop-spike --lib --offline`: passed.
- `cargo test -p shogun-desktop-spike --lib onboarding_music --offline`: 6 passed.
- `cargo test -p shogun-desktop-spike --lib onboarding::mac::tests::music_mute_persists_with_revision_cas --offline`: 1 passed.
- `apps/desktop/node_modules/.bin/tsc --noEmit --pretty false -p apps/desktop/tsconfig.json`: passed.
- `apps/desktop/node_modules/.bin/vitest run src/onboarding/Onboarding.test.tsx`: 37 passed.
- File-scoped `rustfmt --check` and `git diff --check`: passed.

## Device qualification remaining

- Signed packaged-app check: initial 0.50 to 0.40 fade, persistent Mute across relaunch, Settings mode continuity, voice pause/resume, user close, completion, restart, and multi-display session replacement.
