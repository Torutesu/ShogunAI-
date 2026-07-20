# On-device runbook — AX capture source (WP2.2)

This is the **macOS-only** half of the capture pipeline. Everything here is behind
`cfg(target_os = "macos")`, so it does not compile or run on Linux CI — it must be
built and verified on your notch MacBook Pro. Iterate by pasting build errors back.

## What was added (this branch)

Platform-independent (already Linux-tested, green):
- `shogun_core::capture::pipeline::capture_focus` — exclusion gate → bounded AX walk → text.
- `shogun_core::daemon::Db::ingest_capture` — near-dup collapse + first-stage extraction.

macOS adapter (this runbook — **unverified on Linux**):
- `axcache::focused_window(pid)` + `AxElement::title()` — exposed from the proven Phase 0 FFI.
- `apps/desktop/src-tauri/src/capture_source.rs` — `capture_once(db, policy, dwell)` and
  `spawn_capture_poller(db, policy, interval)`.
- `lib.rs::setup_macos` — opens the memory DB under the app-data dir and starts the poller.
- `Cargo.toml` — `shogun-core` gets `features = ["db"]` under the macOS target only.

## Build

```bash
# from repo root, on macOS/arm64
cargo build -p shogun-desktop-spike
# or run the app
cargo tauri dev   # if the Tauri CLI is set up, else `cargo run -p shogun-desktop-spike`
```

## Likely iteration points (fix on-device)

1. **`accessibility-sys` / `core-foundation` API names.** `focused_window`/`AxElement::title`
   reuse the exact calls already in `axcache.rs` that ran in Phase 0, so these should compile.
   If a symbol is missing, it's a version drift in `accessibility-sys 0.2` — check the actual
   const/fn name and adjust.
2. **`shogun-core` `db` feature pulls rusqlite (bundled SQLite).** First macOS build will
   compile the bundled SQLite C — slow once, then cached. If linking fails, confirm Xcode CLT.
3. **Thread affinity (most likely real issue).** The poller calls `display::frontmost_app()`
   (NSWorkspace) and the AX FFI from a **background thread**. AppKit's `NSWorkspace` prefers the
   main thread; AX reads are generally fine off-main with the messaging timeout set. If you see
   a crash/hang or empty frontmost on the poller thread:
   - Simplest fix: drive `capture_once` from a **main-thread timer** instead of the spawned
     thread (e.g. a Tauri `run_on_main_thread` scheduled tick, or an `NSTimer`), keeping the
     `Db` handle (it is `Send + Clone`).
   - The composition (`capture_once`) stays identical either way — only the driver changes.
4. **AXObserver push (later).** The poll (2 s) is the reliable fallback. A
   `didActivateApplicationNotification` observer can be layered on to cut latency; not required
   for first verification.

## Verify (acceptance)

1. Launch the app. Grant **Accessibility** permission when prompted (System Settings →
   Privacy & Security → Accessibility). Console should log
   `capture source started (poll 2000ms)` and `accessibility trusted: true`.
2. Focus a normal window (e.g. a text editor) and type a sentence with a promise/open-loop,
   e.g. *"I'll send the deck tomorrow. Waiting on legal to reply."* Wait ~4 s (two polls).
3. Confirm capture landed in memory — point the CLI/REST/MCP faces at the same DB path
   (`~/Library/Application Support/<bundle-id>/memory.db`), or open it with `sqlite3`:
   ```bash
   sqlite3 "~/Library/Application Support/<bundle-id>/memory.db" \
     "SELECT source, substr(content,1,60) FROM event_log ORDER BY id DESC LIMIT 3;"
   sqlite3 "…/memory.db" "SELECT direction, description, confidence FROM commitments;"
   ```
   Expect the captured text as a `capture` event and a low-confidence (≤0.4) commitment +
   open loop from first-stage extraction.
4. **Exclusion check (invariant / FR-CAP-05/06):** focus a password manager (1Password) or a
   private-browsing window. Confirm **no** `capture` event is written for it (the exclusion gate
   short-circuits before any AX walk).
5. **Near-dup collapse (FR-CAP-03):** keep typing in the same window; confirm the event count
   does **not** grow per keystroke — repeated near-identical bodies dedup-touch one row
   (`last_seen_at` advances, `dwell_ms` accumulates) rather than appending.

Paste the console log + the `sqlite3` output back and I'll iterate on anything that's off.

---

## Context actions in the notch panel (§6.1 — product core)

The notch panel now pulls **real context actions** from memory on expand: `Db::context_actions`
maps the current state (commitments / open loops / people / projects) into ranked, confidence-gated
candidates for the focused screen, and the React panel renders them as buttons (with an L1/L2/L3
level badge; low-confidence state is never shown, FR-ST-20).

### Verify
1. Build + run (`cargo run -p shogun-desktop-spike`). Let the capture source populate some state
   (type a few promise/open-loop sentences in a normal window, as above), so `commitments` /
   `open_loops` are non-empty.
2. Expand the notch panel (hover then click through to Expanded).
3. The action buttons should now show **real labels** derived from your captured state, e.g.
   *"Draft reply"* (from a reply-needed loop), *"Remind: …"* (from a commitment), *"Search memory: …"*
   (from a person/project), each with an `L1`/`L2` badge — instead of the placeholder labels.
   Hovering a button shows its rationale.
4. If the panel still shows placeholders, the command returned nothing (no gated state yet) or the
   DB wasn't managed — check the console for `capture source started` and that state rows exist.

### Not yet wired (next increment)
Clicking a button currently just reports the interaction — it does **not execute** the action yet.
Execution (L1 auto-run / L2 one-tap confirm / L3 approval-queue) is the next step; it needs the
agents `ExecutionEngine` wired into the app. Say the word and I'll add it.
