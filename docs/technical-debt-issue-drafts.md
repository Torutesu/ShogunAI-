# Priority technical-debt issue drafts

Local drafts only. Nothing in this file has been published to GitHub.

The seven issues below are intentionally grouped by subsystem and failure mode. They cover the work that can affect security, correctness, user-visible performance, or failure visibility. Cosmetic cleanup and file-size-only refactors are excluded.

---

## 1. `security(website): patch vulnerable Next.js and sharp runtimes`

**Suggested labels:** `security`, `website`, `dependencies`, `P0`

### Problem

The website currently resolves `next@16.2.10` and `sharp@0.34.5`. Both versions fall inside published advisory ranges:

- Next.js `16.2.10` is below the patched `16.2.11` release.
- `sharp@0.34.5` is affected by the libvips advisory fixed in `sharp@0.35.0`.

Current source does not appear to satisfy the prerequisites for the high-severity Next.js paths: there are no Server Actions, middleware/proxy rules, dynamic external rewrites, or i18n configuration. The site also has no remote image patterns or user-controlled image ingestion. This lowers present exploitability but does not justify retaining vulnerable runtime versions.

### Scope

- Upgrade Next.js to a patched compatible release, at minimum `16.2.11`.
- Ensure the resolved `sharp` version is at least `0.35.0`; a Next.js upgrade alone may still resolve `0.34.5`.
- Regenerate `pnpm-lock.yaml` using pnpm.
- Add dependency-audit checks for production npm dependencies and Rust dependencies (`cargo audit`) to CI.
- Document any accepted advisory that remains, including why its vulnerable path is unreachable.

### Acceptance criteria

- `pnpm why next sharp` shows patched resolved versions.
- Website typecheck and production build pass.
- Image generation and any existing image optimization paths receive a smoke test.
- CI fails on newly introduced unaccepted high-severity production advisories.
- No deployment or publishing is part of this issue.

### Out of scope

- Adding remote image sources or user-uploaded images.
- General package upgrades unrelated to security advisories.

---

## 2. `security(desktop): harden untrusted-content boundaries`

**Suggested labels:** `security`, `desktop`, `tauri`, `P1`

### Problem

The desktop webview has no CSP and exposes the global Tauri object. No reachable script-injection sink was found, so this is a hardening gap rather than a demonstrated exploit. Impact would be high if an injection path is introduced later because the webview renders OCR, transcript, email, and LLM-controlled text near the IPC bridge.

Reply drafting also concatenates untrusted context into one flat LLM prompt. Malicious email or captured screen text can steer the draft body. L3 approval prevents silent sending, but the generated draft can still mislead a rushed user.

### Scope

- Set `withGlobalTauri` to `false` after confirming no frontend code depends on `window.__TAURI__`.
- Add the narrowest working Tauri CSP for bundled assets, IPC, inline styles, and visual-recall data images.
- Keep capability permissions restricted to the named application windows.
- Extend the agent interface to support separate system and user messages.
- Put drafting instructions in the system message and untrusted context in a clearly delimited user message.
- Preserve the existing L3 approval requirement and full-body preview.

### Acceptance criteria

- Desktop production build and all four window surfaces load under the new CSP.
- Visual-recall JPEG previews still render.
- No `window.__TAURI__` dependency remains.
- Tests prove captured text cannot replace the system instruction boundary in the serialized LLM request.
- Tests prove every generated send action still enters the L3 approval queue.

### Out of scope

- HTML rendering of LLM output.
- Expanding Tauri capabilities.
- Attempting to “sanitize” natural-language context with a blacklist.

---

## 3. `security(website): fail closed on production privacy configuration and add headers`

**Suggested labels:** `security`, `privacy`, `website`, `P1`

### Problem

`WAITLIST_IP_SALT` silently falls back to the known value `dev-salt`. If production is misconfigured, stored IP hashes lose their intended privacy property without any startup failure.

The website also defines no application-level security headers. This is defense in depth, but the site collects emails and returns waitlist status tokens, so production defaults should be explicit.

### Scope

- Permit `dev-salt` only outside production.
- Fail startup or request initialization in production when `WAITLIST_IP_SALT` is missing.
- Add CSP, HSTS, `X-Content-Type-Options`, `Referrer-Policy`, and frame protection.
- Roll CSP out in report-only mode first if required to inventory MDX and analytics sources, then enforce it.
- Add tests for production and development environment behavior.

### Acceptance criteria

- Production execution without `WAITLIST_IP_SALT` fails with a clear non-secret error.
- Development retains an explicit local fallback.
- Response-header tests cover every public route family and waitlist API route.
- CSP contains no undocumented wildcard source.
- No raw IP address or salt appears in logs.

### Related

- Broad privacy tracking issue: #19.

---

## 4. `perf(meetings): stop full-overlay frame renders and restore live summaries`

**Suggested labels:** `bug`, `performance`, `meetings`, `P0`

### Problem

`MeetingOverlay` drives React state at animation-frame frequency for an idle waveform. This re-renders the largest frontend component in every meeting window. Each render rebuilds transcript groups and timeline data without memoization, making long meetings progressively more expensive.

The live-summary effect resets its 22-second timer whenever transcript lines change. During normal speech, the timer never reaches its callback, so live summaries can remain permanently waiting.

Resize handles also send native resize IPC on every raw pointer event.

### Scope

- Replace the synthetic JavaScript idle-wave state loop with the existing CSS animation, or isolate it from React rendering.
- Do not run host-only animation work in caption, canvas, or chat windows.
- Memoize transcript grouping and timeline construction.
- Make live-summary scheduling stable while transcript lines continue arriving; retain fingerprint and in-flight deduplication.
- rAF-batch resize callbacks inside `usePointerResize` so every consumer gets bounded IPC.
- Reduce the 1 Hz four-window status poll to pushed events plus a slow host-only reconnect backstop.

### Acceptance criteria

- No component-level `setState` loop runs at 60 fps while the meeting surface is idle.
- Transcript grouping runs only when transcript inputs change.
- A test with speech arriving more often than every 22 seconds still triggers a live summary.
- Resize IPC occurs at most once per animation frame.
- Hidden and non-host meeting windows do not poll once per second.
- Record render count and CPU behavior for a synthetic one-hour transcript before and after the fix.

### Related

- Broad machine-load issue: #30.

---

## 5. `fix(frontend): bound high-rate events and cancel stale async work`

**Suggested labels:** `bug`, `performance`, `frontend`, `P1`

### Problem

Visual-recall scrubbing can issue a frame-image IPC request and JPEG decode for every crossed index. Stale requests continue doing work even when their results are discarded. A periodic frame-list refresh also reloads the currently displayed image because the effect depends on array identity.

Several UI timers are unowned. An old inline-status timeout can clear a newer drafting state, and voice toast timers can dismiss a newer error message early.

### Scope

- rAF-batch visual-recall scrub index updates.
- Debounce frame-image loading and prevent stale work from accumulating.
- Key image loading on selected frame ID, not the frames-array identity.
- Store inline-status and voice-toast timeout handles in refs.
- Cancel the previous handle before setting newer state and during unmount cleanup.
- Add an absolute lifetime to the voice release watchdog.

### Acceptance criteria

- One continuous scrub cannot create an unbounded queue of image requests or decodes.
- Refreshing an unchanged frame list does not reload the selected image.
- A stale timeout cannot clear a newer `drafting` state.
- A short toast cannot dismiss a newer long-lived error toast.
- All intervals and timeouts created by these paths are cleared on unmount.
- Fake-timer and mocked-IPC tests cover each race.

---

## 6. `fix(memory): distinguish empty results from database failures`

**Suggested labels:** `bug`, `rust`, `memory`, `reliability`, `P1`

### Problem

Many `Db` methods collapse lock poisoning and query failures into empty vectors, defaults, or `None` through repeated `.lock().ok()` and `unwrap_or*` chains. The UI cannot distinguish “no memories exist” from “the memory database failed.” For SHOGUN, silent loss of remembered state is worse than a visible degraded mode.

### Scope

- Introduce a consistent connection-access helper or typed error path.
- Return explicit errors for operations where an empty result and failure have different meanings.
- Log only error class and operation name; never captured text, OCR, transcript, or secrets.
- Surface degraded memory state through the existing non-blocking Notch status channel.
- Keep user work uninterrupted and preserve WAL/transaction behavior.

### Acceptance criteria

- Tests distinguish empty query results, query failure, and poisoned-lock failure.
- No production DB failure is silently converted to a successful empty result on user-facing memory paths.
- Logs contain no captured content.
- UI receives a non-sensitive degraded-state signal and can recover after a successful DB operation or restart.
- Existing memory and migration tests pass.

### Out of scope

- Splitting `daemon.rs` solely to reduce file size.
- Changing the SQLite ownership model.

---

## 7. `test(desktop): make frontend test and lint tasks real CI gates`

**Suggested labels:** `testing`, `frontend`, `ci`, `P0`

### Problem

The desktop frontend has no test runner or ESLint configuration. Workspace `test` and `lint` tasks can therefore pass without checking desktop code. Timer races, event floods, and expensive rerenders currently have no regression protection beyond typechecking and a successful build.

### Scope

- Add Vitest and React Testing Library to `apps/desktop`.
- Add an ESLint configuration with React Hooks checks.
- Define real `test` and `lint` scripts in the desktop package.
- Wire both tasks into CI so missing or skipped desktop tasks cannot silently pass.
- Add initial tests for the high-risk pure helpers and hooks used by issues 4 and 5.

### Acceptance criteria

- `pnpm test` runs desktop frontend tests and fails on a deliberately failing test.
- `pnpm lint` analyzes desktop TS/TSX and fails on a deliberately invalid hooks dependency.
- CI runs both commands.
- Initial coverage includes transcript grouping/timeline helpers, live-summary scheduling, timer cancellation, visual-recall selection, and pointer-resize batching.
- Test setup does not require a live Tauri process; IPC is mocked at the adapter boundary.

### Dependency order

Land this issue before, or in the first slice of, issues 4 and 5 so their behavioral changes receive regression tests.
