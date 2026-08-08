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
