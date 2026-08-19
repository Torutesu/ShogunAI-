# Onboarding motion implementation report

## Contract delivered

- Native display geometry derives a normalized CSS-coordinate vector from each ambient display center toward the cursor-selected launch display center. Tests cover all eight directions plus negative, above/below, and mixed-scale layouts.
- `OnboardingSurface.motion_vector` carries that direction through the existing generation-validated native surface lookup. Main and interactive surfaces receive the neutral vector.
- `AmbientSurface` maps each component to `-1 | 0 | 1` data attributes. CSS maps those attributes to bounded GPU transforms; React runs no animation frame loop.
- Full cinematic motion is finite at five seconds and keyframes animate only `transform` and `opacity`. Native deadline/session generation remains the teardown owner; stale deadline callbacks stay inert.
- Reduced Motion preserves the current artwork and substitutes one 200ms opacity-only animation. It contains no transform animation and no `display: none` removal.
- A replacement onboarding generation now releases the outgoing music player before managed state lookup/snapshot can fail. A focused ordering regression covers missing state and snapshot failure.

## TDD evidence

Failing-first frontend run: three expected failures for missing native direction attributes, missing bounded surface mapping, and the old reduced-motion `display: none` contract.

Failing-first Rust run: missing `OnboardingMotionVector` and `motion_vector_toward` symbols in the new eight-direction and mixed-layout tests.

Final verification:

- `cargo test -p shogun-desktop-spike onboarding_windows::tests --lib --offline` — 23 passed.
- `cargo check -p shogun-desktop-spike --lib --offline` — passed.
- `./node_modules/.bin/vitest run src/onboarding/Onboarding.test.tsx` — 40 passed.
- `./node_modules/.bin/tsc --noEmit -p tsconfig.json` — passed.
- `rustfmt --edition 2021 apps/desktop/src-tauri/src/onboarding_windows.rs` — passed.
- `git diff --check` — passed.

## Remaining device risk

- P3: automated geometry and CSS contracts cannot visually prove perceived current direction across every physical macOS display arrangement. One device pass with a secondary display above, left, and right remains prudent.
- P3: Reduce Motion is source/test verified; a macOS accessibility setting pass should confirm the 200ms opacity fade feels acceptable on the actual transparent windows.
