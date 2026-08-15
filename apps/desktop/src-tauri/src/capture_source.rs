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
//! Invariant 2: AX text is the default path; optional visual recall (issue #107) may OCR the
//! focused window in RAM — text + provenance persisted; compressed JPEG frames use selected retention
//! when Visual recall is on (explicit exception, user decision 2026-08-02).
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
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::{Duration, Instant};

    use shogun_core::capture::exclusion::ExclusionPolicy;
    use shogun_core::capture::pipeline::{capture_focus, CaptureOutcome, Focus};
    use shogun_core::capture::visual_recall::Settings as VisualRecallSettings;
    use shogun_core::capture::walk_policy::Limits;
    use shogun_core::daemon::Db;

    use super::{DEFAULT_POLL_MS, WALK_BUDGET_MS};
    use crate::axcache::{ax_trusted_silent, focused_window};
    use crate::display::frontmost_app;
    #[cfg(feature = "visual-recall-ocr")]
    use crate::screen_ocr::{self, MIN_OCR_INTERVAL_MS};
    use crate::visual_recall::mac::SharedSettings;
    #[cfg(feature = "visual-recall-ocr")]
    use crate::visual_recall::pipeline::{self, RecallPipeline};

    #[cfg(not(feature = "visual-recall-ocr"))]
    const MIN_OCR_INTERVAL_MS: u64 = 5_000;

    #[cfg(feature = "visual-recall-ocr")]
    /// Poll cadence gate — separate from Screenpipe's pixel-signature OCR gate.
    struct OcrPollGate {
        last_focus_key: Option<String>,
        last_ocr_at: Option<Instant>,
    }

    #[cfg(feature = "visual-recall-ocr")]
    impl OcrPollGate {
        fn new() -> Self {
            Self {
                last_focus_key: None,
                last_ocr_at: None,
            }
        }

        fn should_run(
            &mut self,
            focus_key: &str,
            bundle_id: &str,
            window_title: Option<&str>,
            ax_empty: bool,
            ax_text_len: usize,
            meeting_active: bool,
            now: Instant,
        ) -> bool {
            let focus_changed = self.last_focus_key.as_deref() != Some(focus_key);
            if focus_changed {
                return true;
            }
            if !pipeline::wants_ocr(
                bundle_id,
                window_title,
                ax_empty,
                ax_text_len,
                meeting_active,
            ) {
                return false;
            }
            match self.last_ocr_at {
                Some(t) => now.duration_since(t).as_millis() as u64 >= MIN_OCR_INTERVAL_MS,
                None => true,
            }
        }

        fn mark(&mut self, focus_key: String, now: Instant) {
            self.last_focus_key = Some(focus_key);
            self.last_ocr_at = Some(now);
        }
    }

    fn focus_key(bundle_id: &str, title: Option<&str>) -> String {
        format!("{bundle_id}\0{}", title.unwrap_or(""))
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct CaptureFingerprint {
        focus_key: String,
        text_hash: u64,
    }

    fn capture_fingerprint(
        focus_key: &str,
        outcome: Option<&CaptureOutcome>,
    ) -> Option<CaptureFingerprint> {
        let text = outcome?.text()?;
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        Some(CaptureFingerprint {
            focus_key: focus_key.to_string(),
            text_hash: hasher.finish(),
        })
    }

    fn reply_context_needs_refresh(
        warm_for: Option<&str>,
        last_capture: Option<&CaptureFingerprint>,
        focus_key: &str,
        outcome: Option<&CaptureOutcome>,
    ) -> bool {
        if warm_for != Some(focus_key) {
            return true;
        }
        let Some(current_capture) = capture_fingerprint(focus_key, outcome) else {
            return false;
        };
        last_capture != Some(&current_capture)
    }

    fn capture_outcome_is_usable(outcome: Option<&CaptureOutcome>) -> bool {
        matches!(outcome, Some(CaptureOutcome::Captured { .. }))
    }

    fn invalidate_reply_context(
        cache: Option<&shogun_core::daemon::ReplyContextCache>,
        warm_for: &mut Option<String>,
        last_capture: &mut Option<CaptureFingerprint>,
    ) {
        if let Some(cache) = cache {
            cache.clear();
        }
        *warm_for = None;
        *last_capture = None;
    }

    /// Capture the current focus into memory once. Reads the frontmost app, builds the focused
    /// window's `AxNode`, runs the exclusion→walk composition (250 ms budget), and — on a real
    /// capture — persists it via `Db::ingest_capture` (collapse + extract). `dwell_ms` is credited
    /// to the event (accumulates on a near-dup touch). Returns the outcome, or `None` if there is no
    /// frontmost app / focused window.
    pub fn capture_once(
        db: &Db,
        policy: &ExclusionPolicy,
        dwell_ms: i64,
    ) -> Option<CaptureOutcome> {
        let front = frontmost_app()?;
        // Never capture / ingest our own process as focus — memory would store "reading itself".
        if crate::display::is_own_app(&front.bundle_id, &front.name) {
            return None;
        }
        let root = focused_window(front.pid)?;
        let title = root.title();
        let focus = Focus {
            bundle_id: &front.bundle_id,
            window_title: title.as_deref(),
        };

        let start = Instant::now();
        let outcome = capture_focus(policy, &focus, &root, Limits::default(), || {
            start.elapsed().as_millis() as u64 > WALK_BUDGET_MS
        });

        // Observability for on-device verification (FR-CAP-05 exclusion / FR-CAP-03 collapse).
        // Never logs captured text (only its length) — telemetry must not contain user content.
        match &outcome {
            CaptureOutcome::Excluded(reason) => {
                eprintln!(
                    "[capture] excluded {} ({:?}) — no walk",
                    front.bundle_id, reason
                );
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

    #[cfg(feature = "visual-recall-ocr")]
    const FRAME_PURGE_INTERVAL_MS: u64 = 30 * 60 * 1_000;

    #[cfg(feature = "visual-recall-ocr")]
    fn maybe_screen_ocr(
        db: &Db,
        visual: &VisualRecallSettings,
        front_pid: i32,
        bundle_id: &str,
        title: Option<&str>,
        dwell_ms: i64,
        ax_empty: bool,
        ax_text_len: usize,
        poll_gate: &mut OcrPollGate,
        recall: &mut RecallPipeline,
    ) {
        if !visual.enabled {
            return;
        }
        let now = Instant::now();
        let key = focus_key(bundle_id, title);
        let meeting_active = crate::meeting::mac::is_recording();
        if !poll_gate.should_run(
            &key,
            bundle_id,
            title,
            ax_empty,
            ax_text_len,
            meeting_active,
            now,
        ) {
            return;
        }
        let result = screen_ocr::ocr_focused_window_gated(
            recall,
            front_pid,
            &key,
            bundle_id,
            title,
            ax_empty,
            ax_text_len,
            meeting_active,
        );
        let text = match result.outcome {
            pipeline::OcrOutcome::Text(t) => t,
            pipeline::OcrOutcome::Skipped | pipeline::OcrOutcome::Empty => {
                poll_gate.mark(key, now);
                return;
            }
        };
        let digest = screen_ocr::text_digest(&text);
        let display_id = Some(crate::geometry::mac::primary_display_id());
        match db.ingest_screen_ocr(Some(bundle_id), title, &text, dwell_ms, display_id) {
            Some((id, touched, cands)) => {
                // Store JPEG only on a fresh OCR event (not dedup touch, not cache replay).
                if !touched {
                    if let Some(frame) = result.frame {
                        // Explicit invariant-2 exception: local encrypted BLOB, rolling purge.
                        if let Some(frame_id) = db.store_screen_frame(
                            id,
                            Some(bundle_id),
                            title,
                            display_id,
                            frame.width,
                            frame.height,
                            &frame.jpeg,
                        ) {
                            eprintln!(
                                "[screen_ocr] frame {frame_id} {}x{} {} bytes → event {id}",
                                frame.width,
                                frame.height,
                                frame.jpeg.len(),
                            );
                        }
                    }
                }
                eprintln!(
                    "[screen_ocr] {} {} chars digest={digest:#x} → event {id} {} (+{} candidate(s))",
                    bundle_id,
                    text.len(),
                    if touched { "touched" } else { "new" },
                    cands.len(),
                );
            }
            None => eprintln!(
                "[screen_ocr] {} — DB write skipped (digest={digest:#x})",
                bundle_id
            ),
        }
        poll_gate.mark(key, now);
    }

    /// Spawn the capture poller: every `interval` (default [`DEFAULT_POLL_MS`]), if the process is
    /// Accessibility-trusted, capture the current focus into memory. The `Db` handle is cloned into
    /// the thread (it is `Arc`-backed and `Send`); the policy is shared with the settings commands
    /// so a change applies on the next tick. Returns the thread handle; dropping it detaches the
    /// poller (it runs for the process lifetime).
    ///
    /// AX elements never leave this thread, so the `!Send` `AxElement` is safe here.
    pub fn spawn_capture_poller(
        db: Db,
        policy: std::sync::Arc<std::sync::Mutex<ExclusionPolicy>>,
        visual_recall: SharedSettings,
        interval: Option<Duration>,
        reply_cache: Option<shogun_core::daemon::ReplyContextCache>,
    ) -> std::thread::JoinHandle<()> {
        let interval = interval.unwrap_or(Duration::from_millis(DEFAULT_POLL_MS));
        let dwell_ms = interval.as_millis() as i64;
        std::thread::spawn(move || {
            let mut warm_for: Option<String> = None;
            let mut last_capture: Option<CaptureFingerprint> = None;
            #[cfg(feature = "visual-recall-ocr")]
            let mut ocr_poll_gate = OcrPollGate::new();
            #[cfg(feature = "visual-recall-ocr")]
            let mut recall_pipeline = RecallPipeline::new();
            #[cfg(feature = "visual-recall-ocr")]
            let mut last_frame_purge =
                Instant::now() - Duration::from_millis(FRAME_PURGE_INTERVAL_MS);
            #[cfg(feature = "visual-recall-ocr")]
            {
                let settings = crate::visual_recall::mac::refresh_settings(&visual_recall);
                match db.purge_screen_frames(settings.retention_ms()) {
                    Ok(removed) if removed > 0 => {
                        eprintln!("[screen_ocr] startup purge removed {removed} expired frame(s)");
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!("[screen_ocr] startup retention purge failed: {e}"),
                }
            }
            loop {
                // Retention is independent of capture permissions and policy locks. Revoking
                // Accessibility access must never leave JPEGs beyond selected retention.
                #[cfg(feature = "visual-recall-ocr")]
                if last_frame_purge.elapsed().as_millis() as u64 >= FRAME_PURGE_INTERVAL_MS {
                    let settings = crate::visual_recall::mac::refresh_settings(&visual_recall);
                    match db.purge_screen_frames(settings.retention_ms()) {
                        Ok(removed) if removed > 0 => {
                            eprintln!("[screen_ocr] purged {removed} expired frame(s)");
                        }
                        Ok(_) => {}
                        Err(e) => eprintln!("[screen_ocr] retention purge failed: {e}"),
                    }
                    last_frame_purge = Instant::now();
                }

                if !ax_trusted_silent() {
                    invalidate_reply_context(
                        reply_cache.as_ref(),
                        &mut warm_for,
                        &mut last_capture,
                    );
                    std::thread::sleep(interval);
                    continue;
                }
                // Re-read the policy each tick: excluding an app is usually a reaction to
                // what is on screen right now, so it must take effect now, not next launch.
                // A poisoned lock means capture stops rather than ignoring exclusions.
                let Ok(current) = policy.lock() else {
                    eprintln!("[capture] exclusion policy unreadable — pausing capture");
                    invalidate_reply_context(
                        reply_cache.as_ref(),
                        &mut warm_for,
                        &mut last_capture,
                    );
                    std::thread::sleep(interval);
                    continue;
                };
                let visual = crate::visual_recall::mac::refresh_settings(&visual_recall);
                let ax_outcome = capture_once(&db, &current, dwell_ms);
                if let Some(CaptureOutcome::Excluded(_)) = ax_outcome.as_ref() {
                    // Excluded windows are never OCR'd either.
                } else if visual.enabled {
                    if let Some(front) = frontmost_app() {
                        let title = focused_window(front.pid).and_then(|w| w.title());
                        if current
                            .is_excluded(&front.bundle_id, title.as_deref())
                            .is_none()
                        {
                            #[cfg(feature = "visual-recall-ocr")]
                            {
                                let ax_empty =
                                    matches!(ax_outcome, Some(CaptureOutcome::Empty) | None);
                                let ax_text_len = match ax_outcome.as_ref() {
                                    Some(CaptureOutcome::Captured { text, .. }) => text.len(),
                                    _ => 0,
                                };
                                maybe_screen_ocr(
                                    &db,
                                    &visual,
                                    front.pid,
                                    &front.bundle_id,
                                    title.as_deref(),
                                    dwell_ms,
                                    ax_empty,
                                    ax_text_len,
                                    &mut ocr_poll_gate,
                                    &mut recall_pipeline,
                                );
                            }
                        }
                    }
                }
                drop(current);

                // Pre-assemble the reply context for whatever the user is now looking at, so
                // pressing the draft button only starts generation (SLO: offer in 150ms —
                // collecting on the press is what that budget forbids). Rebuilt only when the
                // focused thread or captured AX text changes, so a steady poll costs nothing.
                if let Some(cache) = reply_cache.as_ref() {
                    if !capture_outcome_is_usable(ax_outcome.as_ref()) {
                        invalidate_reply_context(Some(cache), &mut warm_for, &mut last_capture);
                    } else if let Some((key, win_title)) = focused_thread_key_and_title() {
                        if reply_context_needs_refresh(
                            warm_for.as_deref(),
                            last_capture.as_ref(),
                            &key,
                            ax_outcome.as_ref(),
                        ) {
                            // Use the fusion path: if the on-screen window maps to a fetched
                            // Gmail thread, the context comes from the full email body
                            // (PayloadSource::Fetched); otherwise it falls back to the
                            // captured on-screen fragment (PayloadSource::OnScreenOnly).
                            let ctx = db.build_reply_context_for_screen(
                                &key,
                                win_title.as_deref().unwrap_or(""),
                            );
                            eprintln!(
                                "[capture] reply context warmed for {} in {}ms ({} turn(s))",
                                key,
                                ctx.build_ms,
                                ctx.turns.len()
                            );
                            cache.put(ctx);
                            warm_for = Some(key.clone());
                        }
                        if let Some(fingerprint) = capture_fingerprint(&key, ax_outcome.as_ref()) {
                            last_capture = Some(fingerprint);
                        } else if warm_for.as_deref() != Some(key.as_str()) {
                            last_capture = None;
                        }
                    } else {
                        invalidate_reply_context(Some(cache), &mut warm_for, &mut last_capture);
                    }
                }
                std::thread::sleep(interval);
            }
        })
    }

    /// Returns the thread key of the currently focused window, plus the raw window title. Used by
    /// the fusion path so `build_reply_context_for_screen` can match the on-screen title against
    /// Gmail threads.
    fn focused_thread_key_and_title() -> Option<(String, Option<String>)> {
        let front = frontmost_app()?;
        let title = focused_window(front.pid)?.title();
        let key = shogun_memory::thread::thread_key(
            "capture",
            None,
            Some(&front.bundle_id),
            title.as_deref(),
        )?;
        Some((key, title))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use shogun_core::capture::walk_policy::WalkResult;

        fn captured(text: &str) -> CaptureOutcome {
            CaptureOutcome::Captured {
                text: text.to_string(),
                walk: WalkResult {
                    text: text.to_string(),
                    text_bytes: text.len(),
                    elements_visited: 1,
                    depth_reached: 0,
                    truncated: false,
                    partial: false,
                },
            }
        }

        #[test]
        fn same_focus_key_changed_capture_refreshes_reply_context() {
            let first = captured("old message");
            let last = capture_fingerprint("same-window", Some(&first));
            let current = captured("new message");

            assert!(reply_context_needs_refresh(
                Some("same-window"),
                last.as_ref(),
                "same-window",
                Some(&current),
            ));
        }

        #[test]
        fn same_focus_key_same_capture_does_not_refresh_reply_context() {
            let current = captured("unchanged message");
            let last = capture_fingerprint("same-window", Some(&current));

            assert!(!reply_context_needs_refresh(
                Some("same-window"),
                last.as_ref(),
                "same-window",
                Some(&current),
            ));
        }

        #[test]
        fn excluded_empty_and_unavailable_capture_invalidate_warm_context() {
            let outcomes = [
                Some(CaptureOutcome::Excluded(
                    shogun_core::capture::exclusion::ExclusionReason::UserApp,
                )),
                Some(CaptureOutcome::Empty),
                None,
            ];

            for outcome in outcomes {
                let cache = shogun_core::daemon::ReplyContextCache::new();
                cache.put(shogun_core::daemon::ReplyContext {
                    thread_key: "sensitive".into(),
                    ..shogun_core::daemon::ReplyContext::default()
                });
                let mut warm_for = Some("sensitive".to_string());
                let mut last_capture = capture_fingerprint("sensitive", Some(&captured("secret")));

                assert!(!capture_outcome_is_usable(outcome.as_ref()));
                invalidate_reply_context(Some(&cache), &mut warm_for, &mut last_capture);

                assert!(cache.current().is_none());
                assert!(warm_for.is_none());
                assert!(last_capture.is_none());
            }
        }
    }
}
