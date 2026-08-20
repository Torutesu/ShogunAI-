# SHOGUN PC shell

Windowed ShogunAI for **Windows** and **Linux**. This is not the macOS Notch app (`apps/desktop`).

v1 in `docs/requirements-v1.0.md` §3.2 still lists Windows/Linux desktop as out of scope. This crate is a **platform base** on a separate branch so the Mac tree stays clean.

## What this slice is

- Tauri v2 + React, SHOGUN Full UI language (OLED black, sidebar, hairlines)
- Windows caption buttons on the **right** (min / max / close)
- System tray; close hides to tray; Quit from the tray exits
- Single instance; `AppUserModelID` `com.syogun.shogunai.pc`
- App data in `%LOCALAPPDATA%\ShogunAI` / `~/.local/share/shogunai`
- Secrets: Windows Credential Manager. Linux fails closed (no file keys)
- Honest empty panes — no invented health numbers
- **Not** wired: AX/UIA capture, sqlcipher, Notch, connectors, agent runs

## Run (Windows)

Need MSVC, WebView2 (current Windows 11 has it), and Node 20+.

```
pnpm install
pnpm --filter @shogun-ai/shell dev
```

Frontend only (no window chrome):

```
pnpm --filter @shogun-ai/shell dev:vite
```

## Linux

Same crate. Tray needs an app-indicator stack. Secrets are not stored until Secret Service is wired.

Linux CI does **not** compile this package (WebKitGTK). A Windows job clippy-checks it.

## Invariants

Same as `CLAUDE.md`: no screenshot/audio files, no L1 send, no secrets in files, no HTTP client in this crate (FR-TR-03).
