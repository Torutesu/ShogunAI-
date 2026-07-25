# @shogun-ai/desktop — Phase 0 notch-UI spike shell

The ShogunAI macOS app. **Currently the Phase 0 spike** (throwaway; see
`docs/notch-ui-prototype-spec.md` and `docs/phase0-dev-instructions.md`), whose only goal
is to answer the four questions (residency, 100ms expand, 300ms cache + 5% CPU, hover
false-positives). Only `crates/spike-harness` is carried into the real implementation.

## Browser preview (design work without a Mac)

The panel's look — spacing, glass, type, the states Settings can be in — is judged in a
browser, not on device. `preview.html` mounts the **real** `src/App.tsx` inside a stand-in
Mac (wallpaper, menu bar, notch, a window underneath so the glass has something to blur)
and fakes the Tauri IPC, so every state is one click away instead of a device round-trip.

```bash
pnpm --filter @shogun-ai/desktop preview:dev      # http://localhost:1420/preview.html
pnpm --filter @shogun-ai/desktop preview:build    # one self-contained HTML file to pass around
```

- The desk is laid out in true macOS points (1512×944, 14" MBP), so the panel is shown at
  its real size — the footer prints the panel's current geometry.
- The rail on the right is developer tooling, not product UI: it switches the foreground
  app, key state (missing / rejected), tracked items, the approval queue, per-service
  connection states, the nightly-review indicator, and simulated IPC latency.
- Geometry comes only from the App's own `set_panel_size` calls, so collapse, Settings and
  the corner grip move real numbers. Below 100% zoom the corner grip drags in scaled pixels.
- `src/preview/` is the ONLY place with mock data, and it is unreachable from `index.html`
  (the entry the Tauri window loads) — nothing here ships in the app bundle.

## Layout

```
apps/desktop/
  preview.html         # browser preview entry (design harness; never loaded by Tauri)
  src/                 # React + TS webview (Idle/Expanded dummy UI). Class-swap +
                       #   paint-done (rAF×2) + input forwarding only — no state/timers here.
  src/preview/         # mock Tauri IPC + the simulated Mac around the panel
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

## Key decisions (Stage-A research → `docs/phase0-findings.md`)

- NSPanel via `tauri-nspanel` v2.1 (`object_setClass`); dynamic key-window for the search
  field needs a runtime toggle or a partial hand-rolled `define_class!`.
- Hover via a listen-only **CGEventTap** from the start (global NSEvent monitor drops
  mouseMoved over other apps' fullscreen / during menu tracking).
- `level 101` blocks IME (tauri-nspanel #104) — drop to `25` while the search field is key.
