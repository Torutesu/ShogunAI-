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
- `vitest run src/onboarding/Onboarding.test.tsx`: 11 pass.
- `vitest run src/onboarding/Onboarding.test.tsx src/voice-ui.test.tsx`: 14 pass.
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
- Native shortcut/Scribe/dictation outcome contracts do not exist yet. Practice stages deliberately wait for those typed outcomes and cannot claim completion.
- Device qualification still needed for real TCC, Screen Recording restart, native drag, multi-display cinematic, and reduced-motion behavior.
