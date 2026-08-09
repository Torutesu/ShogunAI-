# @shogun-ai/desktop — Phase 0 notch-UI spike shell

The ShogunAI macOS app. **Currently the Phase 0 spike** (throwaway; see
`docs/notch-ui-prototype-spec.md` and `docs/phase0-dev-instructions.md`), whose only goal
is to answer the four questions (residency, 100ms expand, 300ms cache + 5% CPU, hover
false-positives). Only `crates/spike-harness` is carried into the real implementation.

## Layout

```
apps/desktop/
  src/                 # React + TS webview (Idle/Expanded dummy UI). Class-swap +
                       #   paint-done (rAF×2) + input forwarding only — no state/timers here.
  src-tauri/           # Rust shell (macOS/arm64 only)
    src/lib.rs         # Tauri setup; wires the subsystems on-device (T-05)
    src/{panel,geometry,hover,statemachine,axcache,display,ipc}.rs
                       #   module boundaries per spec §3.11.1 — typed stubs until on-device
    tauri.conf.json    # transparent NSPanel window; macOSPrivateApi
```

## Status by environment

- **Linux CI / this repo now**: frontend typechecks and `vite build` passes;
  `spike-harness` builds + tests + clippy-clean. The Rust shell (`src-tauri`) is a workspace
  member that **only builds on macOS/arm64** — its macOS deps (tauri-nspanel, objc2) are
  commented in `Cargo.toml` until T-05.
- **On-device (Apple Silicon, macOS 14+)**: `pnpm --filter @shogun-ai/desktop dev` runs
  `tauri dev`. All measurement (S-11/S-12/S-13) and the four-question verdicts happen here.
  Requires Accessibility permission (single TCC category — see `docs/phase0-findings.md`).

## Release builds — analytics key (Issue #99)

The PostHog project write key is embedded **at build time** via
`option_env!("SHOGUN_POSTHOG_KEY")` in `src-tauri/src/analytics.rs`, with the runtime
env var of the same name overriding it for local development. Precedence
(`shogun_core::analytics::resolve_api_key`): runtime env → build-time embed → disabled
(no-op).

- **Release CI must export `SHOGUN_POSTHOG_KEY` (from a CI secret) in the environment of
  the `tauri build` step.** A build without it ships with analytics silently disabled —
  the original Issue #99 bug.
- The key is a PostHog *project API key* (`phc_…`): write-only and public-by-design, so
  embedding it in the binary is safe. It is still sourced from a CI secret and **never
  committed to the repo**.
- `src-tauri/build.rs` emits `cargo:rerun-if-env-changed=SHOGUN_POSTHOG_KEY` so
  incremental builds pick up env changes.
- Optional: `SHOGUN_POSTHOG_HOST` (runtime env) overrides the default
  `https://us.i.posthog.com`.

## Key decisions (Stage-A research → `docs/phase0-findings.md`)

- NSPanel via `tauri-nspanel` v2.1 (`object_setClass`); dynamic key-window for the search
  field needs a runtime toggle or a partial hand-rolled `define_class!`.
- Hover via a listen-only **CGEventTap** from the start (global NSEvent monitor drops
  mouseMoved over other apps' fullscreen / during menu tracking).
- `level 101` blocks IME (tauri-nspanel #104) — drop to `25` while the search field is key.

## App icon and disk-image art

Both are generated from SVG sources; the SVGs are the things to edit, the rasters are build
output that happens to be committed (CI has no image toolchain).

- `src-tauri/icons/icon.svg` → `icon.icns` (the app icon, what Finder and the Dock show)
- `src-tauri/dmg/background.svg` → `background.png` / `background@2x.png` (the DMG window)

The mark itself is the same "S" ribbon as `src/Logo.tsx` and the marketing site. One brand, one
mark: change it in one place and it must change in all three.

To regenerate after editing an SVG (needs `sharp`, and the Tauri CLI for the `.icns`):

```bash
npx --yes sharp-cli -i src-tauri/icons/icon.svg -o src-tauri/icons/icon.png resize 1024 1024
pnpm tauri icon src-tauri/icons/icon.png -o src-tauri/icons
# The CLI also emits Windows/iOS/Android sets. This app is macOS-only — keep icon.icns,
# icon.png and icon.svg, and delete the rest so the folder does not fill with dead art.

npx --yes sharp-cli -i src-tauri/dmg/background.svg -o src-tauri/dmg/background.png resize 660 400
npx --yes sharp-cli -i src-tauri/dmg/background.svg -o src-tauri/dmg/background@2x.png resize 1320 800
```

The DMG icon coordinates live in the `appdmg` spec inside `.github/workflows/release.yml` and are
drawn into the background art. Move an icon in one place and it must move in the other.
