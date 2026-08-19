# Onboarding UI implementation report

Date: 2026-08-19
Branch: `codex-mikel/onboarding`

## Result

- Replaced dark card flow with white, spacious onboarding shell.
- Added typed `main`, `ambient`, and `interactive` surface routing. Native generation mismatch renders inert black surface.
- Added five-second visual cinematic with licensed Lucide waves and existing SHOGUN mark. Native session remains timeline authority.
- Added fixed right `GateFrame`; local-media-free completion uses in-frame opacity fallback. `full-window` API exists but production never selects it.
- Added sequential Accessibility, Microphone, and Screen Recording stages. One current permission only. Native status drives auto-advance only after Rust save resolves. Restart failure stays put.
- Kept real native PermissionFlow drag surface for Accessibility and Screen Recording.
- Added real email/dictation practice fields and live binding display. Browser keys only reflect held appearance. They never claim shortcut, Scribe, dictation, copied, failed, or cancelled success.
- Kept privacy, plan, connections, analytics, and skip-equivalent deferred connection semantics available.
- Added bundled Fraunces, licensed waves, translucent blue haze, fixed gate, restrained button feedback, and reduced-motion path.

## Files

- `apps/desktop/src/onboarding/Onboarding.tsx`
- `apps/desktop/src/onboarding/ipc.ts`
- `apps/desktop/src/onboarding/onboarding.css`
- `apps/desktop/src/onboarding/experience/*`
- `apps/desktop/src/onboarding/Onboarding.test.tsx`
- `apps/desktop/src/strings.ts`

## Tests

- `tsc --noEmit --pretty false`: pass.
- `vitest run src/onboarding/Onboarding.test.tsx`: 16 pass.
- `vitest run src/onboarding/Onboarding.test.tsx src/voice-ui.test.tsx`: 21 pass.
- `vite build`: pass.
- Motion grep: no `transition: all`; no layout-property transitions.
- `git diff --check`: pass.

## Motion review

| Before | After | Why |
|---|---|---|
| Dark/glass card with continuous spinner | One five-second cinematic and short state fade | First-run ritual gets rare-event delight; routine setup stays quiet. |
| Layout/card transition paths | Transform/opacity only | Keep compositor-friendly visual work. |
| Motion under reduced preference | Opacity-only route | Remove travel, loops, and cinematic movement. |

Verdict: Approve. No keyboard-triggered animation, `transition: all`, layout animation, permanent `will-change`, or reduced-motion gap in onboarding CSS.

## Concerns

- Native audio/mute command does not exist. Mute stays visible but disabled rather than pretending to control playback.
- Shortcut/Scribe Rust contract is landing separately. Frontend now waits for its typed outcomes and cannot claim completion before they arrive.
- Device qualification still needed for real TCC, Screen Recording restart, native drag, multi-display cinematic, and reduced-motion behavior.

## Review-fix pass

- Restored live exclusion categories and all original privacy disclosures.
- Restored Pro BYOK Keychain entry, plan `aria-pressed`, safe plan skip, draft-stop fail-safe, connection skip, connection list, and analytics toggle.
- Added typed native-only practice seam: prepare, focus/select seeded Scribe text, ready, disarm, and generation/nonce/stage/session outcome matching. Only `single_tap` and verified `scribe_inserted` persist progression. Browser keys, copied, failed, cancelled, stale, and missing-key results never progress.
- Custom shortcut instructions now use loaded native binding instead of claiming Right Option after a rebind. Sample email lives in `strings.ts`.
- Haze starts only on visible interactive shell, pauses when hidden/offscreen, and has reduced-motion stop path. Ambient motion is neutral because native surface data exposes no directional vector.
- Added tests for live privacy/exclusions, Pro Keychain, plan/connect skip, draft-stop fail-safe, analytics, full-window API, native practice proof, custom binding copy, haze pause, and reduced motion.
