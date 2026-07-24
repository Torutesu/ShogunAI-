//! AX capture source (WP2.2, §6.2) — the macOS adapter that feeds the memory pipeline.
//!
//! The platform-independent composition (exclusion gate → bounded AX walk → text) lives in
//! `shogun_core::capture::pipeline` and is unit-tested on Linux. This module supplies the two
//! things only macOS can: the **focus signal** (which app/window is frontmost, via NSWorkspace) and
//! an **`AxNode` over `AXUIElement`** (reusing the proven `axcache` FFI). It then persists a capture
//! through `Db::ingest_capture` (near-dup collapse + first-stage extraction).
//!
//! AXObserver focus notifications are unreliable across some apps (Phase 0 risk item), so the
//! source is driven by a bounded **poll** (≥2 s) as the reliable fallback; an AXObserver push can be
//! layered on later to reduce latency without changing this composition.
//!
//! Invariant 2: AX text only — no screenshot, no image, ever.
//!
//! `capture_once` is part of the public surface (the main-thread-timer driver alternative in the
//! runbook uses it directly); it is `allow(unused_imports)` because the default driver only calls
//! `spawn_capture_poller`.
#![allow(dead_code, unused_imports)]

#[cfg(target_os = "macos")]
pub use mac::{capture_once, spawn_capture_poller};

/// Default poll interval for the capture source (FR-CAP; ≥2 s per the AXObserver-reliability
/// fallback). Dwell accumulates across polls of the same window body.
pub const DEFAULT_POLL_MS: u64 = 2_000;

/// AX walk time budget per capture (spec §3.10.2 / FR-CAP-02).
pub const WALK_BUDGET_MS: u64 = 250;

#[cfg(target_os = "macos")]
mod mac {
    use std::time::{Duration, Instant};

    use shogun_core::capture::exclusion::ExclusionPolicy;
    use shogun_core::capture::pipeline::{capture_focus, CaptureOutcome, Focus};
    use shogun_core::capture::walk_policy::Limits;
    use shogun_core::daemon::Db;

    use super::{DEFAULT_POLL_MS, WALK_BUDGET_MS};
    use crate::axcache::{ax_trusted, focused_window};
    use crate::display::frontmost_app;

    /// Capture the current focus into memory once. Reads the frontmost app, builds the focused
    /// window's `AxNode`, runs the exclusion→walk composition (250 ms budget), and — on a real
    /// capture — persists it via `Db::ingest_capture` (collapse + extract). `dwell_ms` is credited
    /// to the event (accumulates on a near-dup touch). Returns the outcome, or `None` if there is no
    /// frontmost app / focused window.
    pub fn capture_once(db: &Db, policy: &ExclusionPolicy, dwell_ms: i64) -> Option<CaptureOutcome> {
        let front = frontmost_app()?;
        let root = focused_window(front.pid)?;
        let title = root.title();
        let focus = Focus { bundle_id: &front.bundle_id, window_title: title.as_deref() };

        let start = Instant::now();
        let outcome = capture_focus(policy, &focus, &root, Limits::default(), || {
            start.elapsed().as_millis() as u64 > WALK_BUDGET_MS
        });

        // Observability for on-device verification (FR-CAP-05 exclusion / FR-CAP-03 collapse).
        // Never logs captured text (only its length) — telemetry must not contain user content.
        match &outcome {
            CaptureOutcome::Excluded(reason) => {
                eprintln!("[capture] excluded {} ({:?}) — no walk", front.bundle_id, reason);
            }
            CaptureOutcome::Empty => {}
            CaptureOutcome::Captured { text, .. } => {
                match db.ingest_capture(Some(&front.bundle_id), title.as_deref(), text, dwell_ms) {
                    // Only log the interesting cases (a new event, or candidates extracted) — the
                    // dominant "touched, +0" re-reads would otherwise bury the console.
                    Some((id, touched, cands)) if !touched || !cands.is_empty() => eprintln!(
                        "[capture] {} {} bytes → event {id} {} (+{} candidate(s))",
                        front.bundle_id,
                        text.len(),
                        if touched { "touched" } else { "new" },
                        cands.len(),
                    ),
                    Some(_) => {}
                    None => eprintln!("[capture] {} — DB write skipped", front.bundle_id),
                }
            }
        }
        Some(outcome)
    }

    /// Spawn the capture poller: every `interval` (default [`DEFAULT_POLL_MS`]), if the process is
    /// Accessibility-trusted, capture the current focus into memory. The `Db` handle is cloned into
    /// the thread (it is `Arc`-backed and `Send`); the policy is moved in. Returns the thread handle;
    /// dropping it detaches the poller (it runs for the process lifetime).
    ///
    /// AX elements never leave this thread, so the `!Send` `AxElement` is safe here.
    pub fn spawn_capture_poller(
        db: Db,
        policy: ExclusionPolicy,
        interval: Option<Duration>,
        reply_cache: Option<shogun_core::daemon::ReplyContextCache>,
    ) -> std::thread::JoinHandle<()> {
        let interval = interval.unwrap_or(Duration::from_millis(DEFAULT_POLL_MS));
        let dwell_ms = interval.as_millis() as i64;
        std::thread::spawn(move || {
            let mut warm_for: Option<String> = None;
            loop {
                if ax_trusted() {
                    // errors are swallowed inside ingest; a None just means no focus this tick
                    let _ = capture_once(&db, &policy, dwell_ms);

                    // Pre-assemble the reply context for whatever the user is now looking at, so
                    // pressing the draft button only starts generation (SLO: offer in 150ms —
                    // collecting on the press is what that budget forbids). Rebuilt only when the
                    // focused thread actually changes, so a steady poll costs nothing.
                    if let Some(cache) = reply_cache.as_ref() {
                        if let Some(key) = focused_thread_key() {
                            if warm_for.as_deref() != Some(key.as_str()) {
                                let ctx = db.build_reply_context(&key);
                                eprintln!(
                                    "[capture] reply context warmed for {} in {}ms ({} turn(s))",
                                    key,
                                    ctx.build_ms,
                                    ctx.turns.len()
                                );
                                cache.put(ctx);
                                warm_for = Some(key);
                            }
                        }
                    }
                }
                std::thread::sleep(interval);
            }
        })
    }

    /// The thread key of the currently focused window, matching what the capture writer derives
    /// for the same focus — so the warmed context is keyed exactly as the events are.
    pub fn focused_thread_key() -> Option<String> {
        let front = frontmost_app()?;
        let title = focused_window(front.pid)?.title();
        shogun_memory::thread::thread_key("capture", None, Some(&front.bundle_id), title.as_deref())
    }
}
