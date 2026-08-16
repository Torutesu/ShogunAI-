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

- `src-tauri/icons/icon.svg` → `icon.icns` + `icon.png` + `icon-{32,128,256,512}.png`
  (the app icon: Finder, the Dock, the DMG. `tauri.conf.json` lists every one of those rasters)
- `src-tauri/icons/tray-icon.svg` → `tray-icon.png` (22x22) + `tray-icon@2x.png` (44x44)
  (the menu-bar icon. `lib.rs` `include_bytes!`s the @2x one and installs it as a **template**
  image, so that SVG is black-on-transparent and macOS paints the shape itself)
- `src-tauri/dmg/background.svg` → `background.png` / `background@2x.png` (the DMG window)

**All four sources carry the same mark**, which is also `src/Logo.tsx` and the marketing site's
`Logo.tsx`. One brand, one mark: change it in one place and it must change in all five. The facet
paths live in a 957x614 space and only the left half is authored; the right half is that mirrored,
so the two sides cannot drift apart.

Regenerate **every** raster below after editing any SVG, and check the result by eye. A merge once
rewrote the icon rasters and the tray icon back to a retired mark while leaving `icon.svg` and both
`Logo.tsx` on the current one, and the app shipped showing two different brands at once — nothing
in CI can see that, because CI has no image toolchain and the rasters are committed output.

`sharp-cli`'s `-o` takes a **directory** and writes `<dir>/<input-basename>`, keeping the input's
extension — so each render lands as a `.svg`-named PNG that has to be moved into place:

```bash
cd apps/desktop
render() {  # render <source.svg> <size> <dest-filename>
  local tmp; tmp="$(mktemp -d)"
  npx --yes sharp-cli -i "$1" -o "$tmp/" resize "$2" "$2" >/dev/null
  mv "$tmp/$(basename "$1")" "src-tauri/icons/$3"; rmdir "$tmp"
}
for s in 32 128 256 512; do render src-tauri/icons/icon.svg "$s" "icon-$s.png"; done
render src-tauri/icons/icon.svg 1024 icon.png
render src-tauri/icons/tray-icon.svg 22 tray-icon.png
render src-tauri/icons/tray-icon.svg 44 'tray-icon@2x.png'

# The .icns, from the 1024 render. The CLI also emits Windows/iOS/Android sets, so it writes to a
# scratch directory and only icon.icns is taken — this app is macOS-only and the icons folder must
# not fill with dead art.
icns="$(mktemp -d)"; pnpm tauri icon src-tauri/icons/icon.png -o "$icns"
mv "$icns/icon.icns" src-tauri/icons/icon.icns; rm -rf "$icns"

npx --yes sharp-cli -i src-tauri/dmg/background.svg -o /tmp/dmgbg/ resize 660 400
npx --yes sharp-cli -i src-tauri/dmg/background.svg -o /tmp/dmgbg2x/ resize 1320 800
mv /tmp/dmgbg/background.svg src-tauri/dmg/background.png
mv /tmp/dmgbg2x/background.svg src-tauri/dmg/background@2x.png
```

The DMG icon coordinates live in the `appdmg` spec inside `.github/workflows/release.yml` and are
drawn into the background art. Move an icon in one place and it must move in the other.
