# Full Integration Checklist

Target branch: `codex/full-integration` from `origin/main` at `b21e7f1`  
Source branch: `mikel/meeting-recap-transcript` at `0083ec9`

## Global constraints

- [x] Preserve original checkout, stash, and uncommitted onboarding work.
- [x] Rebuild on current `main`; adapt the 43 source commits to the diverged current architecture.
- [ ] Make `mikel/meeting-recap-transcript` an ancestor of the integration branch so every source commit is retained in PR history.
- [x] Maintain an explicit disposition for every source commit; adapt conflicts to current `main`, never silently exclude them.
- [x] Keep current `main` security, privacy, entitlement, L3 approval, MCP, connector, and meeting behavior; change Visual Recall retention only per explicit user direction.
- [x] No audio files, transcript logs, raw captured text logs, plaintext secrets, or unconfirmed external sends.
- [x] Use focused tests after each batch; full gates before PR.
- [ ] Require physical macOS validation for Accessibility, shortcuts, insertion, microphone, Dock/tray, and signing behavior.

## 1. Baseline and scope audit

- [x] Create isolated worktree at `/private/tmp/shogun-full-integration`.
- [x] `cargo check -p shogun-desktop-spike` passes on current `main`.
- [x] Desktop typecheck passes.
- [x] Desktop tests pass: 29 tests.
- [x] Desktop lint exits successfully with 47 pre-existing warnings and 0 errors.
- [x] Audit source branch by subsystem.
- [x] Preserve the original checkout's uncommitted onboarding work outside this isolated integration worktree.

## 2. Low-risk assets, signing, and issue spec

- [x] Port runtime Dock/tray/icon assets from `15a0da8` and `c6438bb` selectively.
- [x] Account for every branch asset and generator in the source-commit disposition; restore source-only brand artifacts and generators byte-for-byte.
- [x] Port `scripts/codesign-desktop-dev.sh` improvement from `b32fa51` only.
- [x] Port `docs/multi-desktop-auto-switching-issue.md` from `0083ec9`.
- [ ] Validate asset file types, shell syntax/error path, Tauri config, debug bundle, Dock/tray appearance, and signature.

## 3. Inline rewrite foundation and Scribe

- [x] Port focused-target capture and UTF-16 range handling from `54f0821`, `ebb20cb`, `325b6ae`, and `b024670` onto current APIs.
- [x] Preserve current `main` prompt trust boundary, provider lane, user directives, paste fallback, secure-field refusal, and key status behavior.
- [x] Add Scribe overlay and lifecycle from `8642cb3`, `450c534`, `ac80149`, and `2b5b627` without replacing current shell features.
- [x] Capture source target before Scribe steals focus.
- [x] Guard stale writes with original AX value; reactivate source app, freshly select and verify range, then insert.
- [x] Never use unverified whole-field replacement.
- [x] Add focused Rust and frontend tests.

## 4. Right-Option trigger integration

- [x] Keep current rebindable shortcut API and daily-input tracking.
- [x] For configured `Tap+Alt`, accept right Option key code `61` only; left Option never triggers.
- [x] Single clean tap queues draft after 300 ms; second clean tap in window opens Scribe.
- [x] Poison hold on other key, modifier, mouse, scroll, gesture, or over-500 ms hold.
- [x] Preserve normal chord bindings and other gesture bindings.
- [x] Add deterministic state-machine tests.

## 5. Dictation reliability

- [x] Capture caret before mic opens; non-empty selection must not be replaced by dictation.
- [x] Keep backend processing lock through ASR, delivery, and terminal state.
- [x] Stale workers cannot emit, insert, clear state, or unlock a newer session.
- [x] Changed/unsafe target falls back to clipboard without rewriting existing text.
- [x] Preserve current VoiceStart, VoiceEnd, VoiceFailed, hot-mic, traceability, and no-audio-file behavior.
- [x] Keep useful Deepgram `speech_final` close behavior because current main lacked it.
- [x] Port Groq `openai/gpt-oss-120b` cleanup as explicit BYOK opt-in with Keychain-only secret, visible transcript-egress disclosure, traceability, strict validation, bounded timeout, cancellation fence, and raw fallback.
- [x] Port the branch static voice dictionary/default aliases and its conservative exact matcher; document built-in versus user-confirmed provenance accurately.
- [x] Add focused Rust tests.
- [ ] Complete physical microphone/AX checks.

## 6. Voice and notch UI

- [x] Port the 240px closed-notch recording visualizer from `789cafe`.
- [x] Show the closed-notch processing loader while transcription/editing runs.
- [x] Add the mic-frame heartbeat watchdog without replacing Rust lifecycle ownership.
- [x] Keep current toast feedback and make terminal voice errors visibly dismissible.
- [x] Restore Voice settings for the Groq cleanup key with explicit egress disclosure.
- [x] Restore the persisted Notch status Show/Hide settings control.
- [x] Add frontend tests for recording, processing, error, key, and visibility controls.

## 7. Other compatible source features

- [x] Use silent Accessibility trust checks in background polling (`30ffba3`).
- [x] Add bounded, deduplicated recent durable previews to `memory.get_context` (`2ab5f95`) without weakening current citations/privacy.
- [x] Refresh warmed inline context when focused text changes in the same window; preserve current split-role prompt boundary (`3a082c9`).
- [x] Restore app/field-aware tone classification where compatible (`ebb20cb`).
- [x] Restore Keychain-backed Google OAuth credential settings on top of current OAuth PKCE/runtime gating (`aa5b5fe`), not the obsolete connector implementation.
- [x] Design and port the durable cross-process L3 approval store so standalone MCP sends appear in the desktop approval surface; preserve current entitlement/consent/confirmation rules.

## 8. Meeting-platform recognition

- [x] Recognize native Zoom and active Zoom web meetings, including regional `*.zoom.us` join/web-client routes, without matching Zoom marketing pages.
- [x] Recognize active `meet.google.com` meetings without accepting lookalike hosts.
- [x] Recognize native Microsoft Teams and Teams web calls while keeping ordinary chat/home pages from opening an offer alone.
- [x] Preserve Webex and Slack Huddle detection and add a table-driven positive/negative platform matrix.
- [ ] Verify browser URL extraction and meeting lifecycle on Zoom app/web, Google Meet, Teams app/web, Webex, and Slack Huddles on macOS.

## 9. Remaining `b32fa51` reliability integration

- [x] Port per-display CG/AppKit coordinate mapping for negative/offset multi-monitor layouts, including `NSGraphics`, display selection, notch engine, and hover tests.
- [x] Restore the macOS `clang_rt.osx` build link required by the Whisper toolchain.
- [x] Preserve the full RAM audio ring during live streaming and add the 120 ms release tail for complete HTTP fallback.
- [x] Support non-F32 CPAL microphone sample formats with conversion/resampling coverage.
- [x] Retain/remove/reinstall voice shortcut monitors across Accessibility revoke/regrant; recover partial registrations and read combined-session CG modifier flags.
- [x] Reassert panels when compositor occlusion says they are not drawn; repair `hidesOnDeactivate`/`canHide`, marshal workspace callbacks to the main thread, and cover cold-launch health.
- [x] Reopen Accessibility repair after prior onboarding completion when trust is later lost, while preserving deliberate skip.
- [x] Probe microphone permission when Voice is enabled without weakening the current session state machine.

## 10. Conflict adaptations (no silent exclusions)

- [x] Adapt the old L3 queue/heartbeat to current approvals, entitlement, and consent APIs.
- [x] Carry Google connector/settings intent onto the current connector runtime and consent boundary.
- [x] Replace the former 72-hour Visual Recall cap with the user-approved encrypted retention selector: 1–7 days plus a bounded custom duration.
- [x] Defer forever retention by explicit user direction; every available mode must retain automatic age deletion.
- [x] Show current encrypted frame bytes and usage projections derived from recent capture rate.
- [x] Keep the 3-day legacy/default migration safe and preserve settings symmetry across desktop, MCP, REST, and CLI.
- [x] Bound and cite recent activity rather than exposing unbounded raw context.
- [x] Record source meeting hunks as integrated or superseded by current recap/live-summary/translation architecture; retain all commits in ancestry.
- [x] Retain source documentation, research, reports, and history rather than deleting artifacts silently.

## 11. Review and validation gates

- [x] Per-batch spec and code-quality review passes.
- [ ] `cargo fmt --all -- --check` (repository-wide pre-existing formatting drift; check-only command reports untouched files)
- [x] `cargo check -p shogun-desktop-spike`
- [x] `cargo test -p shogun-desktop-spike --lib` (126 passed)
- [x] `cargo test -p shogun-core --lib --features db` (658 passed)
- [x] `cargo test -p shogun-mcp --lib --no-default-features` (161 passed)
- [x] `cargo test -p shogun-integrations --lib` (64 passed)
- [x] Desktop TypeScript typecheck (direct local binary; pnpm wrapper hit sandbox fetch failure)
- [x] Desktop frontend tests (44 passed across 9 files)
- [x] Desktop lint exits with 0 errors and no new warnings (47 pre-existing warnings).
- [x] `git diff --check`
- [ ] Physical macOS checklist completed and recorded in PR.
- [x] Final whole-branch review has no open Critical or Important findings.

## 12. Delivery

- [x] Commit in coherent Conventional Commit batches.
- [ ] Push `codex/full-integration`.
- [ ] Open PR to `main` with scope, skipped superseded work, test evidence, Mac evidence, risk notes, and rollback guidance.
