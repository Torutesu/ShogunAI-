# Windows / Linux windowed shell (platform base)

**Status:** spike. **DO NOT MERGE.** Out of v1 (`docs/requirements-v1.0.md` §3.2). Does not change that table.

**Owner intent:** Anand — own a PC product surface without forking `apps/desktop` (objc2 / Notch / Keychain).

## Decision

| Layer | macOS v1 | This shell |
|---|---|---|
| World model, Dream Cycle, MCP/CLI/REST, L1/L2/L3 | `crates/` | Unchanged; not linked here yet |
| Secrets | Keychain | Windows Credential Manager; Linux fail-closed |
| Capture | Accessibility text | Not in this slice (UIA / AT-SPI later) |
| Shell | Notch NSPanel + Full UI | Tray + real window, Full UI language |
| Stack | Tauri v2 + Rust + React | Same |

Do **not** compile `shogun-desktop-spike` on Windows. Do **not** put sqlcipher/openssl on the MSVC graph in this slice.

## Windows practices in this slice

- `#![windows_subsystem = "windows"]` in release
- Custom title bar, caption buttons **right**, Win11 close-hover red
- Close → tray; tray **Quit** exits; left-click tray restores
- Single-instance focus
- `AppUserModelID` = `com.syogun.shogunai.pc` (taskbar pin / grouping)
- Per-user NSIS (`installMode: currentUser`)
- `%LOCALAPPDATA%\ShogunAI` — not Roaming, not a file key store
- Launch at sign-in is **opt-in** (Settings)
- Per-monitor DPI via WebView2 defaults

## Honest empty states

Invariant 1: Rust assembles `ShellView`. The webview does not invent coverage/yield/SLO numbers. Empty copy says what would produce the pane.

## Follow-ups (not this slice)

- UI Automation text capture (no bitmaps on disk)
- Linux Secret Service
- Encrypted memory DB once MSVC/sqlcipher is a solved build
- Snap layouts with a frameless window (DWM / native decorations trade-off)
- UI ≡ MCP ≡ CLI for whatever this shell grows (invariant 6)
