//! The Dream Cycle's on-device driver (§6.7, FR-DC-01/05/06, macOS side).
//!
//! Everything the nightly cycle decides — the gate, the resumable plan, the Batch classification,
//! the health escalation — already lives in `shogun_core::dreamcycle` and is tested without a Mac.
//! What was missing is the thing only this side can do: read the wall clock, the idle timer, the
//! lock state and the power source, and drive a tick. Without it the whole cycle is code that never
//! runs, and the state tables only ever hold what inline capture happened to notice.
//!
//! Two lanes, chosen by whether a Select KK key is present:
//! - **Batch/Select-KK** (invariant 5): the model classifies the night's events. Medium confidence.
//! - **Local rules**: the same heuristics inline capture uses, no network at all. Low confidence.
//!
//! The local lane is not a stub — it is the honest degradation. A device with no Batch key still
//! gets overdue/staleness recomputed, Warm→Cold demotion, and heuristic candidates; it just does not
//! get the model's judgement. FR-DC-05 in the same spirit: a Batch failure turns the indicator amber
//! and carries the work to the next night, and touches nothing local.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use shogun_core::daemon::Db;
    use shogun_core::dreamcycle::gate::RunConditions;
    use shogun_core::dreamcycle::health::Indicator;
    use shogun_core::dreamcycle::plan::{CycleKind, JobKind, JobState};
    use shogun_core::dreamcycle::run::GatedRun;
    use shogun_core::dreamcycle::schedule::{
        cycle_id, input_range, local_time, run_batch_cycle, window_position, DreamScheduler,
        DEFAULT_LOOKBACK_MS, DEFAULT_WINDOW_END_HOUR, DEFAULT_WINDOW_START_HOUR,
    };

    /// How often the driver re-evaluates the gate. The gate itself decides whether anything happens;
    /// ticking every 5 minutes just means a machine that goes idle at 02:07 does not wait for 03:00.
    const TICK: Duration = Duration::from_secs(5 * 60);

    /// Nights the indicator looks back over. It only counts the unbroken run of failures at the
    /// front, so a week is more than enough to reach the red threshold (3).
    const HEALTH_WINDOW_NIGHTS: usize = 7;

    /// Poll cadence and budget for a submitted batch. 60s × 120 ≈ 2 hours — inside the 4-hour
    /// nightly window, so a batch that stalls fails the cycle *tonight* and is retried next night
    /// (FR-DC-05) rather than pinning a thread for the API's full 24-hour ceiling.
    const BATCH_POLL_INTERVAL: Duration = Duration::from_secs(60);
    const BATCH_MAX_POLLS: u32 = 120;

    /// The Batch lane's classification model. Small and fast on purpose: consolidation is a
    /// per-event labelling job over a whole night of events, and Select KK pays for every one.
    ///
    /// Interim. Once the batch relay lands the device stops naming a model at all and sends an
    /// intent instead — a client that can pick the model is a client that can pick an expensive one
    /// (docs/batch-relay-design.md §4.4).
    const BATCH_MODEL: &str = "claude-haiku-4-5-20251001";

    /// Keychain coordinates of the Batch lane's credential (invariant 7 — never a file, a DB or a
    /// log). No UI writes here: it is not the user's key.
    ///
    /// **Interim, development only.** Today this slot holds a raw Anthropic key and the lane calls
    /// Anthropic directly, which is fine on a developer's own machine and must never ship — a
    /// shipped binary carrying the operator's key can be extracted, and spend caps become
    /// unenforceable. The shipping design puts a licence token here and a Select-operated relay in
    /// front of the Batch API: docs/batch-relay-design.md.
    const KEYCHAIN_SERVICE: &str = "SHOGUN";
    const SELECT_KK_ACCOUNT: &str = "select-kk-batch";

    /// Guards a manual run against the nightly one. Both would write the same ledger rows, and while
    /// that is idempotent it would double the Batch spend.
    static RUNNING: AtomicBool = AtomicBool::new(false);

    // ------------------------------------------------------------------------ platform reads
    // The facts the gate needs that only macOS can answer. Kept at the sys level: these are
    // Core Foundation pointer conventions, and being explicit about create/get rules is what makes
    // the ownership auditable.

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        /// Seconds since the last input event of any kind, from the HID system source.
        fn CGEventSourceSecondsSinceLastEventType(source_state: i32, event_type: u32) -> f64;
        /// The current session's properties (create rule), or NULL outside a login session.
        fn CGSessionCopyCurrentDictionary() -> core_foundation_sys::dictionary::CFDictionaryRef;
    }

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPSCopyPowerSourcesInfo() -> core_foundation_sys::base::CFTypeRef;
        fn IOPSCopyPowerSourcesList(
            blob: core_foundation_sys::base::CFTypeRef,
        ) -> core_foundation_sys::array::CFArrayRef;
        fn IOPSGetPowerSourceDescription(
            blob: core_foundation_sys::base::CFTypeRef,
            source: core_foundation_sys::base::CFTypeRef,
        ) -> core_foundation_sys::dictionary::CFDictionaryRef;
    }

    /// `kCGEventSourceStateHIDSystemState` — the real HID stream, not this process's event source.
    const HID_SYSTEM_STATE: i32 = 1;
    /// `kCGAnyInputEventType`.
    const ANY_INPUT_EVENT: u32 = u32::MAX;

    /// Milliseconds since the user last touched the machine.
    fn idle_ms() -> u64 {
        // SAFETY: a plain C call with scalar arguments; no pointers cross the boundary.
        let secs = unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT_EVENT) };
        if secs.is_finite() && secs > 0.0 {
            (secs * 1000.0) as u64
        } else {
            0
        }
    }

    /// Look a string key up in a CF dictionary, returning the raw value (get rule — borrowed from
    /// the dictionary, so the caller must keep the dictionary alive).
    ///
    /// # Safety
    /// `dict` must be a live `CFDictionaryRef`.
    unsafe fn cf_get(
        dict: core_foundation_sys::dictionary::CFDictionaryRef,
        key: &str,
    ) -> Option<core_foundation_sys::base::CFTypeRef> {
        use core_foundation::base::TCFType;
        use core_foundation::string::CFString;
        let key = CFString::new(key);
        let v = core_foundation_sys::dictionary::CFDictionaryGetValue(dict, key.as_CFTypeRef());
        if v.is_null() {
            None
        } else {
            Some(v)
        }
    }

    /// Whether the screen is locked. A locked screen counts as idle regardless of the timer
    /// (FR-DC-01) — someone who locked and walked away is the clearest possible "go ahead".
    fn screen_locked() -> bool {
        // SAFETY: the create-rule dictionary is released exactly once below; the key lookup borrows
        // from it while it is still alive. NULL (no login session) short-circuits before any use.
        unsafe {
            let session = CGSessionCopyCurrentDictionary();
            if session.is_null() {
                return false;
            }
            let locked = cf_get(session, "CGSSessionScreenIsLocked").is_some();
            core_foundation_sys::base::CFRelease(session.cast());
            locked
        }
    }

    /// `(on wall power, battery percent)`. A desktop Mac has no battery source at all, which reads
    /// as "on power, 100%" — correct, and the gate then never blocks on power there.
    fn power_state() -> (bool, u8) {
        use core_foundation::base::TCFType;
        use core_foundation::number::CFNumber;
        use core_foundation::string::CFString;
        use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex};
        use core_foundation_sys::base::{CFGetTypeID, CFRelease};
        use core_foundation_sys::number::CFNumberGetTypeID;
        use core_foundation_sys::string::CFStringGetTypeID;

        // SAFETY: `blob` and `list` are create-rule and released once at the end. Every description
        // dictionary and every value read from one is get-rule — borrowed from `blob`/`list`, which
        // stay alive for the whole loop. Each value's type is checked before it is wrapped.
        unsafe {
            let blob = IOPSCopyPowerSourcesInfo();
            if blob.is_null() {
                return (true, 100);
            }
            let list = IOPSCopyPowerSourcesList(blob);
            if list.is_null() {
                CFRelease(blob);
                return (true, 100);
            }

            let mut on_power = true;
            let mut pct: u8 = 100;
            let mut saw_battery = false;
            for i in 0..CFArrayGetCount(list) {
                let source = CFArrayGetValueAtIndex(list, i);
                if source.is_null() {
                    continue;
                }
                let desc = IOPSGetPowerSourceDescription(blob, source);
                if desc.is_null() {
                    continue;
                }
                saw_battery = true;
                if let Some(v) = cf_get(desc, "Power Source State") {
                    if CFGetTypeID(v) == CFStringGetTypeID() {
                        on_power = CFString::wrap_under_get_rule(v.cast()).to_string() == "AC Power";
                    }
                }
                let num = |name: &str| -> Option<i64> {
                    let v = cf_get(desc, name)?;
                    (CFGetTypeID(v) == CFNumberGetTypeID())
                        .then(|| CFNumber::wrap_under_get_rule(v.cast()).to_i64())
                        .flatten()
                };
                if let (Some(current), Some(max)) = (num("Current Capacity"), num("Max Capacity")) {
                    if max > 0 {
                        pct = ((current * 100) / max).clamp(0, 100) as u8;
                    }
                }
            }
            CFRelease(list.cast());
            CFRelease(blob);
            if saw_battery {
                (on_power, pct)
            } else {
                (true, 100)
            }
        }
    }

    /// Now, as `(unix seconds, seconds east of UTC)`. The offset comes from the OS, so DST and any
    /// mid-session zone change are already folded in — the window is local wall-clock, not UTC.
    fn now_local() -> (i64, i32) {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // SAFETY: `tm` is written by localtime_r before it is read, and `t` outlives the call.
        let off = unsafe {
            let mut tm: libc::tm = std::mem::zeroed();
            let t = secs as libc::time_t;
            if libc::localtime_r(&t, &mut tm).is_null() {
                0
            } else {
                tm.tm_gmtoff as i32
            }
        };
        (secs, off)
    }

    /// Read the current conditions for the gate (FR-DC-01).
    fn conditions(db: &Db, tonight: &str) -> RunConditions {
        let (secs, off) = now_local();
        let pos = window_position(
            local_time(secs, off).hour,
            DEFAULT_WINDOW_START_HOUR,
            DEFAULT_WINDOW_END_HOUR,
        );
        let (power_connected, battery_pct) = power_state();
        RunConditions {
            within_window: pos.within,
            window_elapsed: pos.elapsed,
            idle_ms: idle_ms(),
            screen_locked: screen_locked(),
            power_connected,
            battery_pct,
            full_run_done_today: db.dream_status(tonight, HEALTH_WINDOW_NIGHTS).full_run_done_today,
        }
    }

    /// The Select KK key, if this build has been provisioned with one. Absent is the normal case
    /// today: the cycle then runs the local lane rather than not running.
    fn select_kk_key() -> Option<String> {
        security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, SELECT_KK_ACCOUNT)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    // ------------------------------------------------------------------------ running a cycle

    /// Run the local-rule lane: the same heuristics inline capture uses, no network at all.
    fn run_local(db: &Db, cond: &RunConditions, tonight: &str, now_ms: i64) -> GatedRun {
        let classifier = shogun_core::dreamcycle::jobs::LocalRuleClassifier;
        DreamScheduler::new(db, &classifier).tick(cond, tonight, now_ms)
    }

    /// Run one evaluation. `Err` means the Batch lane failed and tonight's work carries over — the
    /// gate outcome is unknowable in that case, which is why it is not a `GatedRun`.
    fn tick_once(db: &Db) -> Result<GatedRun, String> {
        let (secs, off) = now_local();
        let tonight = cycle_id(secs, off, DEFAULT_WINDOW_START_HOUR, DEFAULT_WINDOW_END_HOUR);
        let cond = conditions(db, &tonight);
        let now_ms = secs * 1000;

        match select_kk_key() {
            Some(key) => run_via_batch(db, key, &cond, &tonight, now_ms),
            None => Ok(run_local(db, &cond, &tonight, now_ms)),
        }
    }

    /// The Batch/Select-KK lane (invariant 5). Falls back to the local lane only when the transport
    /// or runtime cannot be built at all — a *provider* failure is a failed cycle that carries to
    /// the next night (FR-DC-05), not a silent downgrade to weaker candidates.
    fn run_via_batch(
        db: &Db,
        key: String,
        cond: &RunConditions,
        tonight: &str,
        now_ms: i64,
    ) -> Result<GatedRun, String> {
        use shogun_core::llm::anthropic::{AnthropicBatchClient, AnthropicConfig};
        use shogun_core::llm::transport::ReqwestTransport;
        use shogun_core::llm::{Secret, SelectKkKey};

        let (Ok(transport), Ok(rt)) = (
            ReqwestTransport::new(),
            tokio::runtime::Builder::new_current_thread().enable_all().build(),
        ) else {
            eprintln!("[dream] transport/runtime unavailable — running the local lane tonight");
            return Ok(run_local(db, cond, tonight, now_ms));
        };

        let client = AnthropicBatchClient::new(
            transport,
            db.traceability_sink(),
            SelectKkKey::new(Secret::new(key)),
            AnthropicConfig::new(BATCH_MODEL),
        );
        match rt.block_on(run_batch_cycle(
            db,
            &client,
            cond,
            tonight,
            now_ms,
            BATCH_MAX_POLLS,
            || async { tokio::time::sleep(BATCH_POLL_INTERVAL).await },
        )) {
            Ok(gated) => Ok(gated),
            // A rejected credential is not a bad night, and treating it as one would retry it
            // every night forever while the indicator blamed the service. Fall back to the local
            // lane — which is what a device with no credential at all does — and say why. The
            // shipping design does the same for a 401 from the relay
            // (docs/batch-relay-design.md §4.5).
            Err(shogun_core::llm::LlmError::Unauthorized(status)) => {
                eprintln!(
                    "[dream] batch credential rejected (HTTP {status}) — running the local lane. \
                     Check the SHOGUN/select-kk-batch Keychain entry."
                );
                Ok(run_local(db, cond, tonight, now_ms))
            }
            Err(e) => {
                // The lane errors *before* the cycle records anything, so without this the ledger
                // would show no night at all and the indicator would stay green through a week of
                // failures. Recording the consolidation as failed is what makes it amber (FR-DC-05)
                // and what makes the window re-run next night (FR-DC-04).
                let (from_ts, to_ts) =
                    input_range(db.last_consolidated_to(), now_ms, DEFAULT_LOOKBACK_MS);
                db.record_job(tonight, JobKind::Consolidation, JobState::Failed, from_ts, to_ts);
                Err(format!("{e}"))
            }
        }
    }

    /// Spawn the nightly driver. Returns the thread handle; dropping it detaches the driver, which
    /// runs for the process lifetime like the capture poller.
    pub fn spawn_dream_driver(db: Db) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || loop {
            if !RUNNING.swap(true, Ordering::SeqCst) {
                let outcome = tick_once(&db);
                RUNNING.store(false, Ordering::SeqCst);
                match outcome {
                    Ok(GatedRun::Ran { cycle, report }) => eprintln!(
                        "[dream] {} cycle: {} job(s) done{}",
                        if cycle == CycleKind::Full { "full" } else { "degraded" },
                        report.completed.len(),
                        report
                            .failed
                            .as_ref()
                            .map(|(k, e)| format!(", stopped at {k:?}: {e}"))
                            .unwrap_or_default()
                    ),
                    Ok(GatedRun::Skipped(_)) => {}
                    Err(e) => eprintln!(
                        "[dream] batch lane failed ({e}) — carrying tonight's work to the next cycle"
                    ),
                }
            }
            std::thread::sleep(TICK);
        })
    }

    // ------------------------------------------------------------------------ commands

    /// The Dream Cycle status the Settings panel shows (FR-DC-06).
    #[derive(serde::Serialize)]
    pub struct DreamStatusView {
        /// `normal` / `amber` / `red` (FR-DC-05).
        pub indicator: &'static str,
        /// Whether the Batch lane is provisioned. When false the cycle still runs, locally.
        pub batch_lane: bool,
        /// `full` / `degraded`, or None if no cycle has ever run.
        pub last_kind: Option<&'static str>,
        pub last_cycle_id: Option<String>,
        pub last_succeeded: bool,
        pub last_ended_at: i64,
        pub jobs_done: usize,
        pub jobs_failed: usize,
        pub duration_ms: i64,
        pub events_processed: i64,
        pub state_changes: i64,
        pub chunks_sent: i64,
        /// True once tonight's full cycle has completed.
        pub done_tonight: bool,
    }

    #[tauri::command]
    pub fn dream_status(db: tauri::State<'_, Db>) -> DreamStatusView {
        let (secs, off) = now_local();
        let tonight = cycle_id(secs, off, DEFAULT_WINDOW_START_HOUR, DEFAULT_WINDOW_END_HOUR);
        let s = db.dream_status(&tonight, HEALTH_WINDOW_NIGHTS);
        DreamStatusView {
            indicator: match s.indicator {
                Indicator::Normal => "normal",
                Indicator::Amber => "amber",
                Indicator::Red => "red",
            },
            batch_lane: select_kk_key().is_some(),
            last_kind: s
                .last
                .as_ref()
                .map(|c| if c.kind == CycleKind::Full { "full" } else { "degraded" }),
            last_cycle_id: s.last.as_ref().map(|c| c.cycle_id.clone()),
            last_succeeded: s.last.as_ref().is_some_and(|c| c.succeeded),
            last_ended_at: s.last.as_ref().map(|c| c.ended_at).unwrap_or(0),
            jobs_done: s.last.as_ref().map(|c| c.jobs_done).unwrap_or(0),
            jobs_failed: s.last.as_ref().map(|c| c.jobs_failed).unwrap_or(0),
            duration_ms: s.last.as_ref().map(|c| c.duration_ms()).unwrap_or(0),
            events_processed: s.events_processed,
            state_changes: s.state_changes,
            chunks_sent: s.chunks_sent,
            done_tonight: s.full_run_done_today,
        }
    }

    /// Run the degraded cycle now, on request (FR-SET: "manual run (degraded)"). Deliberately the
    /// state-only sequence: a manual full run would spend Select KK's budget on a button press, and
    /// what a user wants from this button is their overdue/staleness brought up to date.
    #[tauri::command]
    pub fn run_dream_now(db: tauri::State<'_, Db>) -> Result<String, String> {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return Err("a cycle is already running".into());
        }
        let db = db.inner();
        let (secs, off) = now_local();
        let now_ms = secs * 1000;
        let classifier = shogun_core::dreamcycle::jobs::LocalRuleClassifier;
        let runner = shogun_core::dreamcycle::jobs::DbDreamRunner::new(db, &classifier, now_ms);
        let (from_ts, to_ts) = input_range(db.last_consolidated_to(), now_ms, DEFAULT_LOOKBACK_MS);
        // Its own ledger key, so a manual catch-up never counts as "tonight's full cycle ran" and
        // never marks the night's jobs done on the nightly driver's behalf.
        let manual_id = format!(
            "{}-manual",
            cycle_id(secs, off, DEFAULT_WINDOW_START_HOUR, DEFAULT_WINDOW_END_HOUR)
        );
        let report = shogun_core::dreamcycle::run::run_cycle(
            db,
            &runner,
            &manual_id,
            CycleKind::Degraded,
            from_ts,
            to_ts,
        );
        RUNNING.store(false, Ordering::SeqCst);
        match &report.failed {
            None => Ok(format!("{} job(s) done", report.completed.len())),
            Some((kind, e)) => Err(format!("stopped at {kind:?}: {e}")),
        }
    }
}
