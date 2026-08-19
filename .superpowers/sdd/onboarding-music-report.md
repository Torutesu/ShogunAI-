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

## Review-fix evidence

- Cinematic `main` surface now has the same native Mute action as interactive onboarding; reduced-motion CSS hides only decorative motion, never the control.
- A reserved voice session synchronously pauses native music before any cue or microphone open. Music resumes only after the authoritative `voice_lane::stop` returns. Poisoned voice state is `None` and fails closed to paused.
- Fade work is exactly ten 90ms callbacks; no steady-state polling exists after 0.40. Voice state is event-driven.
- Display/session replacement stops and releases the old `AVAudioPlayer` before creating one replacement player; stale fade generations cannot affect it.
- Mute pauses before the Store CAS/persistence response; a failed CAS restores the prior native state. `Retained<AnyObject>` now has explicit typed main-thread ownership, not a hidden `usize`.

Additional checks: 7 `onboarding_music` Rust tests (poison fail-closed, generation replacement, one player, fixed fade completion), mute CAS test, TypeScript check, and 38 onboarding Vitest cases.
