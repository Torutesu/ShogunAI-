# SHOGUN killer onboarding implementation plan

Date: 2026-08-19
Branch: `codex-mikel/onboarding`
Base: fetched `origin/codex/full-integration` at `42516a3`

## Product decisions

1. First launch starts a five-second cinematic on the display containing the mouse cursor.
2. Other displays show noninteractive black veils with calm blue ink movement aimed toward the main display. These close before permission work begins.
3. Music starts at 50 percent, smoothly settles at 40 percent, continues through onboarding, and always exposes Mute.
4. Required permissions are Accessibility, Microphone, and Screen Recording. Notifications are excluded.
5. Each permission has its own stage and advances automatically after native status becomes effective.
6. Power demonstrations run in this order: single Right Option, double-tap Right Option, then hold Ctrl+Option+V for dictation.
7. Gate stays fixed in the right half. Completion plays the gate-opening video in place. A full-window opening remains an internal experiment only.
8. Restart persists exact progress and relaunches the same installed app bundle.
9. Existing privacy, plan, and connection setup remains available. Plan and connection skip semantics remain unchanged until separate product approval removes them.

## Experience

### Cinematic

The main screen darkens. Licensed blue wave SVGs move inward and resolve into the existing SHOGUN mark. Secondary monitors carry only quiet ambient motion. At five seconds all full-display windows close. One normal-level onboarding window opens on the launch display.

Reduced Motion replaces travel and morphing with short opacity fades. Audio still exposes Mute immediately.

### Onboarding shell

Window target: approximately 1120 by 720, responsive down to 900 by 620.

Left side owns copy, controls, and proof. Right side owns one fixed gate frame. Gate does not jump or resize between steps.

Visual system:

* warm white `#FAFAF8`
* ink `#101114`
* quiet text `#62676F`
* SHOGUN blue `#004CFC`
* hairline `#E7E8E5`
* Fraunces for restrained display headings
* macOS system face for controls and dense body copy
* translucent blue haze made from moving solid-alpha shapes, not strong gradients
* minimal containers, modest 8 to 10 pixel button radius, no excessive bold text

All new non-brand artwork comes from pinned external SVG sources with license records. No generated or hand-drawn replacement icons.

### Permission stages

Stages are sequential: Accessibility, Microphone, Screen Recording. Rust owns truth. Status reads never prompt. Explicit Allow actions may prompt or open the exact System Settings pane.

Foreground status refresh target is under 500 ms. Native activation and request callbacks refresh immediately. Duplicate poll and event delivery cannot advance twice. Screen Recording may become `restart_required`; Restart stays on that stage until the relaunched process proves effective access.

PermissionFlow behavior is native. Drag uses the real installed `.app`, a native drag threshold, compatible pasteboard types, and a helper panel that never covers System Settings.

### Power demonstrations

Demonstrations use real native shortcut paths and real onboarding text fields.

1. Single Right Option teaches the delayed inline draft action.
2. Double-tap Right Option opens Scribe against a selected messy sample email.
3. Hold Ctrl+Option+V records dictation, release completes, and successful insertion appears in the focused field.

Fresh-install demos must work without pretending success. Any deterministic onboarding result may replace model generation only for the exact seeded field and active nonce; capture, shortcut, AX validation, focus restoration, cancellation, and insertion remain production paths. Clipboard fallback never counts as demo success.

Custom bindings display honestly. The fixed default copy appears only when Rust confirms defaults.

### Gate

Static placeholder fills right frame until supplied gate assets arrive. Completion swaps the same media frame to gate-opening video and waits for its terminal event before closing onboarding. Missing video uses a restrained opacity reveal, never a broken player.

Internal experiment: `GateFrame` accepts a full-window presentation variant, but production routing never selects it.

## Native architecture

### State

Introduce versioned semantic state:

`Intro`, `Welcome`, `Accessibility`, `Microphone`, `ScreenRecording`, `RightOption`, `ScribeDemo`, `DictationDemo`, `Privacy`, `Plan`, `Connect`, `Gate`, `Ready`.

State includes revision, intro completion, music mute, restart reason, and legacy plan/trial fields. Writes use same-directory temp file, sync, atomic rename, and stale-revision rejection. Old six-step records migrate explicitly.

### Permission coordinator

One native coordinator owns latest typed permission states and revision. It polls only while onboarding or repair UI is open, refreshes on app activation and request completion, emits edges once, and rewinds to the first missing required permission after revocation.

### Window session

One generation-owned window session manages main cinematic, secondary ambient windows, interactive onboarding, display changes, external permission UI mode, and idempotent cleanup. Interactive onboarding uses normal window level. Ambient windows ignore input and exist only during the five-second intro.

### Music

One native AVAudioPlayer owns the bundled CC0 track. It starts once per active onboarding session, fades from 0.50 to 0.40, persists Mute in Rust state, and stops on completion, close, restart, or generation replacement. Existing hot-mic speaker safety remains authoritative.

## Delivery tasks

Live checklist, updated after reviewed commits:

- [x] Isolated `codex-mikel/onboarding` from fetched `codex/full-integration`
- [x] UI, native, and asset audits
- [x] Licensed waves, Fraunces, ambient music, checksums, and provenance
- [x] Atomic semantic state, migration, CAS revisions, and crash-safe persistence
- [x] One native permission coordinator with typed status and monotonic frontend delivery
- [x] Packaged restart with exact-step resume, runtime identity checks, and safe voice/Scribe fences
- [x] Cursor-display window session and exact five-second lifecycle
  - [x] Pure generation/session model and multi-display tests
  - [x] AppKit logical-coordinate placement for mixed-DPI/negative display layouts
  - [x] True nonactivating ambient panels and teardown
  - [x] Owned display observer and main-thread cleanup
  - [x] Intro/interactive close handling, external-UI reconfiguration, and stale-generation IPC
  - [x] Full validation, independent review pass, and commit
- [x] PermissionFlow drag parity and external Settings helper
- [x] White onboarding shell, fixed gate frame, cinematic, haze, and reduced motion
- [x] Native single/double Right Option demonstration path
- [x] Dictation session identity and verified insertion outcome demonstration
- [x] Native music, 50-to-40 percent fade, persistent Mute, and cleanup
- [x] Task-scoped integration tests and motion/design review
- [ ] Whole-branch integration tests and final review
- [ ] Gate-opening video and final gate assets — deferred until assets are supplied
- [ ] Signed packaged one/two/three-display TCC qualification — deferred for now

Testing note: run `pnpm desktop:onboarding:qa` while onboarding is under active QA. Each manual QA launch resets Accessibility, Microphone, and Screen Recording, then forces onboarding. An onboarding-triggered Restart bypasses the wrapper so a newly granted Screen Recording permission survives. Normal and release launch behavior remains unchanged.

Each task receives a fresh implementer, task-scoped review, and fix loop before the next task starts.

## Automated acceptance

* old onboarding state migrates without losing completion, plan, or trial timestamp
* stale writes and stale async completions cannot regress progress
* kill and Restart reopen exact unfinished stage
* cursor selects correct display across negative and mixed-scale coordinates
* ambient windows are noninteractive and gone before any permission action
* System Settings and consent sheets always appear above SHOGUN
* every permission auto-advances once and revoke returns to first missing stage
* Screen Recording restart-required never counts as granted early
* single and double Right Option use native shared state machine
* Scribe demo advances only after verified insertion
* dictation copied/failed/cancelled outcomes never advance
* Mute persists; one music player exists; volume settles from 0.50 to 0.40
* Reduced Motion disables travel, shape morph, and gate video motion
* external assets carry source, license, version, and checksum
* no `transition: all`, animated layout properties, frontend permission truth, fake DOM shortcuts, or direct demo text mutation

## Device qualification

Before merge, run signed packaged app from `/Applications` on one, two, and three displays. Cover clean grant, denial, repair, revoke, regrant, Screen Recording restart, app move/signature mismatch, System Settings drag, right/left Option behavior, focus drift, dictation clipboard fallback, reduced motion, mute, sleep/wake, display disconnect, and full-screen Spaces.
