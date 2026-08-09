//! Integration glue (spec §3.3/§3.4/§3.11): runs the pure `NotchEngine`, drives one-shot
//! timers from a single timer thread with generation checks **at receive time** (closing
//! the schedule/cancel TOCTOU), applies `EngineOutput`s, implements the webview→Rust
//! command half of the closed IPC contract (§3.11.2), and emits the measurement streams
//! (expand_latency via `painted`+clock offset, expand_session, cpu_sample, heartbeat,
//! tap_status). All timestamps share ONE `MonoClock` (the recorder's) so Q2 math has a
//! single timeline. Decision logic stays in `shogun_core::notch::engine` (unit-tested); behaviour
//! is validated on-device.
//!
//! CLOSED IPC message set (spec §3.11.2 — do not add messages without a spec change):
//! Rust→webview events: `state`, `geometry`, `context`, `fs_mode`, `clock_sync`.
//! webview→Rust commands: `painted`, `anim_done`, `interact`, `collapse_request`,
//! `focus_field`, `clock_sync_ack`. All emits and all #[tauri::command] fns live in THIS
//! file so the contract has a single enforcement point.
#![allow(dead_code, unused_imports)]

#[cfg(target_os = "macos")]
pub use mac::{start, Shared, StartGeometry};

#[cfg(target_os = "macos")]
pub mod mac {
    use crate::axcache;
    use crate::geometry::Regions;
    use crate::hover::TapEvent;
    use shogun_core::notch::engine::{EngineInput, EngineOutput, NotchEngine};
    use shogun_core::notch::hover::HoverParams;
    use shogun_core::notch::statemachine::{Params, State, Timer};
    use spike_harness::clock::{OffsetEstimator, SyncSample};
    use spike_harness::cpu::{read_process_usage, CpuMeter, CPU_METHOD};
    use spike_harness::record::{
        Body, CacheTrigger, CacheUpdate, CpuSample, ExpandLatency, ExpandSession, Heartbeat,
        Interactions, Mode, StateTransition,
    };
    use spike_harness::recorder::Recorder;
    use spike_harness::stats::MovingAverage;
    use spike_harness::{now_epoch_ms, MonoClock};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Emitter, Manager};

    #[derive(Clone, serde::Serialize)]
    struct StatePayload {
        state: &'static str,
        t0_mono_ns: u64,
    }

    #[derive(Clone, serde::Serialize)]
    struct ClockSyncPayload {
        seq: u32,
        rust_mono_ns: u64,
    }

    /// The `context` event payload (spec §3.11.2). `text` is the live captured context for
    /// on-screen display only — it never enters a record (the harness stores digests). The
    /// window title is not fetched; `title_masked` carries the non-sensitive app name.
    #[derive(Clone, serde::Serialize)]
    struct ContextPayload {
        bundle_id: String,
        title_masked: String,
        text: String,
        captured_at_ms: u64,
        partial: bool,
    }

    /// Events into the engine loop.
    pub enum Ev {
        Tap(TapEvent),
        /// Timer fire carrying the generation it was armed with; accepted only if the
        /// generation is still current at receive time (TOCTOU-proof, review #6).
        Timer(Timer, u64),
        Input(EngineInput),
        /// Live NSPanel size — rebuilds `r_exp` + CGEventTap band so leave-grace covers the panel.
        PanelSize { w: f64, h: f64 },
    }

    /// An open preview/expanded session (spec §4.2.4). Opened when the preview (Hover) first
    /// appears — that is the automatic open the Q4 false-positive rate is about. `promoted`
    /// records whether the user then deliberately opened the full panel (click / hotkey),
    /// which by definition rules the session out as a hover false positive.
    struct SessionDraft {
        opened_at_ms: u64,
        opened_mono_ns: u64,
        clicks: u32,
        keys: u32,
        scrolls: u32,
        promoted: bool,
        manual_false_positive: bool,
    }

    /// State shared between the engine loop and the Tauri commands.
    pub struct Shared {
        pub recorder: Recorder,
        clock: MonoClock,
        ev_tx: Mutex<Sender<Ev>>,
        offset: Mutex<OffsetEstimator>,
        sync_sent: Mutex<HashMap<u32, u64>>,
        last_commit_ns: AtomicU64,
        session: Mutex<Option<SessionDraft>>,
        collapse_reason: Mutex<&'static str>,
        cpu_1min: Mutex<Option<f64>>,
        engine_state: Mutex<&'static str>,
        /// Latest pre-assembled context (spec §3.10.2 step 6). Re-emitted to the webview on
        /// Expanded so a freshly-opened panel shows the current context immediately.
        last_context: Mutex<Option<ContextPayload>>,
        is_notch: bool,
        display_count: u32,
    }

    impl Shared {
        fn send(&self, ev: Ev) {
            if let Ok(tx) = self.ev_tx.lock() {
                let _ = tx.send(ev);
            }
        }
        /// Feed a Hotkey input to the engine (⌘⇧Space direct-expand, statemachine §3.3). Public so
        /// the global-shortcut handler can open the panel without depending on hover.
        pub fn trigger_hotkey(&self) {
            self.send(Ev::Input(EngineInput::Hotkey));
        }
        /// Update hover `r_exp` + CGEventTap top band to the live panel frame (open/resize).
        pub fn set_panel_hit_size(&self, w: f64, h: f64) {
            self.send(Ev::PanelSize { w, h });
        }
        fn set_reason(&self, r: &'static str) {
            if let Ok(mut g) = self.collapse_reason.lock() {
                *g = r;
            }
        }
    }

    /// Geometry snapshot handed to the engine at startup (from `geometry::read_primary`).
    pub struct StartGeometry {
        pub regions: Regions,
        pub menubar_min_y: f64,
        pub primary_height: f64,
        pub is_notch: bool,
        pub display_count: u32,
        pub screen: crate::geometry::Rect,
        pub idle: crate::geometry::Rect,
        /// One entry per attached display: screen rect, regions, menubar floor, idle rect.
        /// The engine hit-tests against whichever of these the pointer is inside, so the notch
        /// works on a second monitor instead of only where the panel happens to live.
        pub per_display: Vec<(crate::geometry::Rect, Regions, f64, crate::geometry::Rect)>,
    }

    // ---------------------------------------------------------------- timers

    const TIMERS: [Timer; 4] =
        [Timer::Dwell, Timer::HoverExit, Timer::ExpandedIdle, Timer::CollapseAnim];

    fn tidx(t: Timer) -> usize {
        match t {
            Timer::Dwell => 0,
            Timer::HoverExit => 1,
            Timer::ExpandedIdle => 2,
            Timer::CollapseAnim => 3,
        }
    }

    struct ArmCmd {
        timer: Timer,
        gen: u64,
        fire_at: Instant,
    }

    /// One timer thread for all three one-shot timers (no per-hover thread spawn,
    /// review efficiency). Cancellation = generation bump; stale fires are dropped by
    /// the engine loop's generation check.
    struct TimerSvc {
        cmd_tx: Sender<ArmCmd>,
        gens: Arc<[AtomicU64; 4]>,
    }

    impl TimerSvc {
        fn spawn(ev_tx: Sender<Ev>) -> Self {
            let (cmd_tx, cmd_rx) = channel::<ArmCmd>();
            let gens: Arc<[AtomicU64; 4]> = Arc::new(Default::default());
            std::thread::spawn(move || {
                let mut slots: [Option<(u64, Instant, Timer)>; 4] = [None, None, None, None];
                loop {
                    let now = Instant::now();
                    // Fire due slots.
                    for slot in slots.iter_mut() {
                        if let Some((gen, at, t)) = *slot {
                            if at <= now {
                                *slot = None;
                                if ev_tx.send(Ev::Timer(t, gen)).is_err() {
                                    return;
                                }
                            }
                        }
                    }
                    // Wait for the nearest deadline or the next command.
                    let next = slots.iter().flatten().map(|(_, at, _)| *at).min();
                    let wait = match next {
                        Some(at) => at.saturating_duration_since(Instant::now()),
                        None => Duration::from_secs(3600),
                    };
                    match cmd_rx.recv_timeout(wait) {
                        Ok(cmd) => slots[tidx(cmd.timer)] = Some((cmd.gen, cmd.fire_at, cmd.timer)),
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            });
            Self { cmd_tx, gens }
        }

        fn schedule(&self, t: Timer, ms: u64) {
            let gen = self.gens[tidx(t)].fetch_add(1, Ordering::SeqCst) + 1;
            let _ = self.cmd_tx.send(ArmCmd { timer: t, gen, fire_at: Instant::now() + Duration::from_millis(ms) });
        }

        fn cancel(&self, t: Timer) {
            self.gens[tidx(t)].fetch_add(1, Ordering::SeqCst);
        }

        /// True iff `gen` is still the current generation for `t` (receive-time check).
        fn is_current(&self, t: Timer, gen: u64) -> bool {
            self.gens[tidx(t)].load(Ordering::SeqCst) == gen
        }
    }

    // ---------------------------------------------------------------- start

    /// Start the integration: engine loop, timer thread, clock-sync pings, cpu sampler,
    /// heartbeat. Returns the `Shared` handle for `app.manage(...)` (commands need it).
    pub fn start(app: AppHandle, geo: StartGeometry, tap_rx: Receiver<TapEvent>) -> Arc<Shared> {
        let clock = MonoClock::new();
        let recorder = Recorder::with_clock(8192, clock);
        recorder.spawn_file_flusher(metrics_dir(), Duration::from_secs(1));

        let (ev_tx, ev_rx) = channel::<Ev>();
        let shared = Arc::new(Shared {
            recorder: recorder.clone(),
            clock,
            ev_tx: Mutex::new(ev_tx.clone()),
            offset: Mutex::new(OffsetEstimator::new()),
            sync_sent: Mutex::new(HashMap::new()),
            last_commit_ns: AtomicU64::new(0),
            session: Mutex::new(None),
            collapse_reason: Mutex::new("timeout"),
            cpu_1min: Mutex::new(None),
            engine_state: Mutex::new("idle"),
            last_context: Mutex::new(None),
            is_notch: geo.is_notch,
            display_count: geo.display_count,
        });

        // Tap → engine event forwarder.
        {
            let ev_tx = ev_tx.clone();
            std::thread::spawn(move || {
                while let Ok(t) = tap_rx.recv() {
                    if ev_tx.send(Ev::Tap(t)).is_err() {
                        break;
                    }
                }
            });
        }

        spawn_clock_sync(&app, &shared);
        spawn_cpu_sampler(&shared);
        spawn_heartbeat(&app, &shared);
        spawn_focus_watcher(&app, &shared);
        spawn_engine_loop(app, shared.clone(), ev_tx, ev_rx, geo);
        shared
    }

    fn spawn_engine_loop(
        app: AppHandle,
        shared: Arc<Shared>,
        ev_tx: Sender<Ev>,
        ev_rx: Receiver<Ev>,
        geo: StartGeometry,
    ) {
        std::thread::spawn(move || {
            let per_display = geo.per_display.clone();
            // Which entry the engine is currently configured for, so regions are only swapped on
            // an actual screen change rather than on every mouse move.
            let mut active_display: Option<usize> = None;
            let mut engine = NotchEngine::new(
                geo.regions,
                geo.menubar_min_y,
                geo.primary_height,
                HoverParams::default(),
                Params::default(),
                geo.screen,
                geo.idle,
            );
            let timers = TimerSvc::spawn(ev_tx);
            let mut prev_state = State::Idle;
            while let Ok(ev) = ev_rx.recv() {
                let input = match ev {
                    Ev::PanelSize { w, h } => {
                        engine.set_panel_hit_size(w, h);
                        // CGEventTap early-reject band must cover the open panel or moves into
                        // the body never reach HoverTracker (one edge sample → false leave).
                        crate::hover::set_hover_band_cg(h + 16.0);
                        continue;
                    }
                    Ev::Tap(TapEvent::Status { active }) => {
                        shared.recorder.record(Body::TapStatus { active });
                        continue;
                    }
                    Ev::Tap(TapEvent::Moved { x, y, buttons }) => {
                        // Point the engine at the display the pointer is actually on before it
                        // hit-tests. Cheap: a handful of rect comparisons, and only when the
                        // screen changes does anything get swapped.
                        if per_display.len() > 1 {
                            let ns_y = geo.primary_height - y;
                            if let Some((i, (screen, regs, menubar, idle))) = per_display
                                .iter()
                                .enumerate()
                                .find(|(_, (r, _, _, _))| {
                                    x >= r.x && x <= r.x + r.w && ns_y >= r.y && ns_y <= r.y + r.h
                                })
                            {
                                if active_display != Some(i) {
                                    active_display = Some(i);
                                    engine.set_regions(
                                        *regs,
                                        *menubar,
                                        geo.primary_height,
                                        *screen,
                                        *idle,
                                    );
                                }
                            }
                        }
                        EngineInput::MouseCg { x, y, t_ms: shared.clock.elapsed_ns() / 1_000_000, buttons }
                    }
                    Ev::Tap(TapEvent::Down { x, y }) => {
                        EngineInput::ButtonDownCg { x, y, t_ms: shared.clock.elapsed_ns() / 1_000_000 }
                    }
                    Ev::Tap(TapEvent::Up) => EngineInput::ButtonUp { t_ms: shared.clock.elapsed_ns() / 1_000_000 },
                    Ev::Timer(t, gen) => {
                        if !timers.is_current(t, gen) {
                            continue; // stale fire (cancelled/rescheduled since) — drop
                        }
                        if t == Timer::HoverExit || t == Timer::ExpandedIdle {
                            shared.set_reason("timeout");
                        }
                        if t == Timer::CollapseAnim {
                            // The webview failed to report anim_done within 400ms —
                            // hang suspicion, recorded per spec §3.3 T6.
                            shared.recorder.record(Body::AnimTimeout { state: "collapsing".into() });
                        }
                        EngineInput::TimerFired(t)
                    }
                    Ev::Input(i) => i,
                };
                for out in engine.on_input(input) {
                    apply(&app, &shared, &timers, &mut prev_state, out);
                }
            }
        });
    }

    fn apply(
        app: &AppHandle,
        shared: &Shared,
        timers: &TimerSvc,
        prev_state: &mut State,
        out: EngineOutput,
    ) {
        match out {
            EngineOutput::WebviewState(s) => {
                // Loud on purpose while D-06 is being wired: the hover path has never driven the
                // panel, so "did the tracker even see me?" is the first question every time.
                eprintln!("[spike] state → {}", s.tag());
                // Follow the pointer across displays. The panel is placed once at build time and
                // by ⌥J; without this the hover path opened it on whichever screen it was last
                // parked on, so the notch on a second monitor appeared dead even though the
                // tracker saw the hover perfectly well.
                if matches!(s, State::Hover | State::Expanded) {
                    crate::move_panel_to_cursor_screen(app);
                }
                let _ = app.emit("state", StatePayload { state: s.tag(), t0_mono_ns: shared.clock.elapsed_ns() });
                shared.recorder.record(Body::StateTransition(StateTransition {
                    from: prev_state.tag().to_string(),
                    to: s.tag().to_string(),
                    trigger: "sm".to_string(),
                }));
                if let Ok(mut g) = shared.engine_state.lock() {
                    *g = s.tag();
                }
                match s {
                    State::Hover => {
                        // The preview is the AUTOMATIC open (dwell-triggered) the Q4
                        // false-positive rate is about — open the session here unless one is
                        // live (a T5 revive continues it).
                        if let Ok(mut sess) = shared.session.lock() {
                            if sess.is_none() {
                                *sess = Some(SessionDraft {
                                    opened_at_ms: now_epoch_ms(),
                                    opened_mono_ns: shared.clock.elapsed_ns(),
                                    clicks: 0,
                                    keys: 0,
                                    scrolls: 0,
                                    promoted: false,
                                    manual_false_positive: false,
                                });
                            }
                        }
                        emit_cached_context(app, shared);
                    }
                    State::Expanded => {
                        // Deliberate promotion to the full panel — not a false positive.
                        if let Ok(mut sess) = shared.session.lock() {
                            if let Some(d) = sess.as_mut() {
                                d.promoted = true;
                            }
                        }
                        emit_cached_context(app, shared);
                    }
                    State::Idle => close_session(shared),
                    _ => {}
                }
                *prev_state = s;
            }
            EngineOutput::SetIgnoresMouse(_b) => {
                // No-op by design: the product UI is an always-visible, always-interactive panel
                // (not the hover-driven click-through idle from the Phase-0 spike). Letting the
                // engine toggle ignoresMouseEvents here would make the panel click-through the
                // moment hover returned to Idle, breaking chat/buttons. Interactivity is owned by
                // panel::install (set_ignores_mouse_events(false)).
            }
            EngineOutput::ScheduleTimer { timer, ms } => timers.schedule(timer, ms),
            EngineOutput::CancelTimer(timer) => timers.cancel(timer),
            EngineOutput::PreviewCommit => {
                // `t0` for the preview-open latency (Idle→Hover) — the Phase 0 Q2 metric
                // (the visible panel open). The webview reports its paint against this.
                let t0 = shared.clock.elapsed_ns();
                shared.last_commit_ns.store(t0, Ordering::SeqCst);
                shared.recorder.record(Body::ExpandCommit { t0_mono_ns: t0 });
            }
            EngineOutput::ExpandCommit => {
                // `t0` for the full-expand latency (→Expanded), i.e. NFR-SLO-01. The dedicated
                // SLO-01 histogram is WP1.4; for now record the marker for traceability.
                let t0 = shared.clock.elapsed_ns();
                shared.recorder.record(Body::ExpandCommit { t0_mono_ns: t0 });
            }
            EngineOutput::OpenFullUi => {
                // An ordinary window, not the overlay — see build_full_ui_window for why.
                crate::build_full_ui_window(app);
            }
            EngineOutput::TopBandEntry => {
                shared.recorder.record(Body::TopBandEntry { count: 1 });
            }
            EngineOutput::HoverBand(h) => {
                crate::hover::set_hover_band_cg(h);
            }
        }
    }

    /// Push the pre-assembled context to the webview. This is a READ of the existing cache —
    /// it never triggers a walk (spec §3.10.3 "no collect-on-press"); the walk only runs on
    /// focus change in the focus watcher.
    fn emit_cached_context(app: &AppHandle, shared: &Shared) {
        if let Ok(g) = shared.last_context.lock() {
            if let Some(ctx) = g.as_ref() {
                let _ = app.emit("context", ctx.clone());
            }
        }
    }

    /// Close the live session (preview/Expanded → Idle) into an `event.expand_session` record.
    fn close_session(shared: &Shared) {
        let Some(d) = shared.session.lock().ok().and_then(|mut s| s.take()) else {
            return;
        };
        let duration_ms = (shared.clock.elapsed_ns().saturating_sub(d.opened_mono_ns)) / 1_000_000;
        let interactions = Interactions { clicks: d.clicks, keys: d.keys, scrolls: d.scrolls };
        // A promoted session (the user clicked/hotkeyed to the full panel) is a deliberate
        // open, never a false positive — regardless of duration or later interaction.
        let auto_fp = !d.promoted && d.clicks + d.keys + d.scrolls == 0 && duration_ms < 1500;
        let reason = shared.collapse_reason.lock().map(|g| *g).unwrap_or("timeout");
        use spike_harness::record::CloseReason;
        let close_reason = match reason {
            "esc" => CloseReason::Esc,
            "outside_click" => CloseReason::OutsideClick,
            "forced" => CloseReason::Forced,
            _ => CloseReason::Timeout,
        };
        shared.recorder.record(Body::ExpandSession(ExpandSession {
            opened_at_ms: d.opened_at_ms,
            closed_at_ms: now_epoch_ms(),
            duration_ms,
            interactions,
            close_reason,
            auto_false_positive: auto_fp,
            manual_false_positive: d.manual_false_positive,
        }));
    }

    // ------------------------------------------------------- background streams

    /// 5 clock-sync pings (min-RTT offset estimation, spec §4.1).
    fn spawn_clock_sync(app: &AppHandle, shared: &Arc<Shared>) {
        let app = app.clone();
        let shared = shared.clone();
        std::thread::spawn(move || {
            // Three rounds so a slow webview (cold VM) still calibrates: listeners may not
            // be attached for the first round; the min-RTT estimator just keeps the best.
            let mut seq = 0u32;
            for round_delay_ms in [1500u64, 4500, 6000] {
                std::thread::sleep(Duration::from_millis(round_delay_ms));
                for _ in 0..5u32 {
                    let send_ns = shared.clock.elapsed_ns();
                    if let Ok(mut m) = shared.sync_sent.lock() {
                        m.insert(seq, send_ns);
                    }
                    let _ = app.emit("clock_sync", ClockSyncPayload { seq, rust_mono_ns: send_ns });
                    seq += 1;
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        });
    }

    /// 5s CPU/RSS sampling with a 1-minute moving average (spec §4.2.3).
    fn spawn_cpu_sampler(shared: &Arc<Shared>) {
        let shared = shared.clone();
        std::thread::spawn(move || {
            let mut meter = CpuMeter::new();
            let mut avg = MovingAverage::new(12);
            loop {
                std::thread::sleep(Duration::from_secs(5));
                let Ok(usage) = read_process_usage() else { continue };
                let wall_ns = shared.clock.elapsed_ns();
                let Some(pct) = meter.sample(usage.cpu_ns, wall_ns) else { continue };
                let one_min = avg.push(pct);
                if let Ok(mut g) = shared.cpu_1min.lock() {
                    *g = one_min;
                }
                shared.recorder.record(Body::CpuSample(CpuSample {
                    cpu_pct: pct,
                    cpu_1min_avg: one_min,
                    method: CPU_METHOD,
                    rss_mb: usage.rss_bytes as f64 / (1024.0 * 1024.0),
                }));
            }
        });
    }

    /// 60s soak heartbeat (spec §4.5). Panel visibility is read on the main thread.
    fn spawn_heartbeat(app: &AppHandle, shared: &Arc<Shared>) {
        let app = app.clone();
        let shared = shared.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(60));
            let state = shared.engine_state.lock().map(|g| *g).unwrap_or("?").to_string();
            let cpu = shared.cpu_1min.lock().ok().and_then(|g| *g).unwrap_or(0.0);
            let uptime_s = shared.clock.elapsed_ns() / 1_000_000_000;
            let ax_calls = crate::axcache::ax_call_count();
            let rss_mb = read_process_usage().map(|u| u.rss_bytes as f64 / (1024.0 * 1024.0)).unwrap_or(0.0);
            let recorder = shared.recorder.clone();
            let app2 = app.clone();
            let _ = app.run_on_main_thread(move || {
                let visible = crate::overlay_ptr(&app2)
                    .map(|ptr| {
                        use objc2::msg_send;
                        // SAFETY: main thread, live NSWindow/NSPanel, read-only getter.
                        unsafe {
                            let v: bool = msg_send![ptr, isVisible];
                            v
                        }
                    })
                    .unwrap_or(false);
                recorder.record(Body::Heartbeat(Heartbeat {
                    panel_visible: visible,
                    // Frame verification is the on-device health check (runbook D-06).
                    panel_frame_ok: visible,
                    state,
                    cpu_1min_avg: cpu,
                    rss_mb,
                    ax_calls_total: ax_calls,
                    uptime_s,
                }));
            });
        });
    }

    /// Focus-driven context cache (spec §3.10). Polls the frontmost app every 400ms (cheap —
    /// NSWorkspace only, no AX call) and runs the bounded AX walk on TWO triggers: (1) the
    /// pid changed (app switch, immediate) or (2) a ~2s content-refresh tick elapsed. (2) is
    /// required because switching a BROWSER TAB does not change the frontmost pid — a
    /// pid-only check goes stale the moment the user changes tabs without switching apps.
    /// This periodic re-walk is the spike's "always pre-assemble" cadence (CLAUDE.md: 押し
    /// てから収集禁止); event-driven AXFocusedWindowChanged/AXTitleChanged/
    /// AXFocusedUIElementChanged subscription is the on-device refinement (runbook D-03/D-05)
    /// that replaces this poll with instant, no-op-when-idle updates.
    ///
    /// Every walk is deduped by content digest (`walk_and_publish`) so an unchanged tab
    /// re-checked every 2s does not spam the `context` event or the `metric.cache_update`
    /// stream — only genuine changes publish. The walk runs ONLY here, on this schedule —
    /// never from the state machine or a press (spec §3.10.3); t0/t1 bracket only the walk
    /// itself, so poll cadence and dedup checks never inflate the measured Q3-A latency.
    fn spawn_focus_watcher(app: &AppHandle, shared: &Arc<Shared>) {
        let app = app.clone();
        let shared = shared.clone();
        std::thread::spawn(move || {
            const POLL: Duration = Duration::from_millis(400);
            const REFRESH_TICKS: u32 = 5; // ~2000ms content-refresh cadence
            let mut last_pid: Option<i32> = None;
            let mut last_digest: Option<String> = None;
            let mut ticks_since_refresh: u32 = 0;
            loop {
                std::thread::sleep(POLL);
                ticks_since_refresh += 1;
                let Some(front) = crate::display::frontmost_app() else { continue };
                let pid_changed = Some(front.pid) != last_pid;
                if !pid_changed && ticks_since_refresh < REFRESH_TICKS {
                    continue;
                }
                last_pid = Some(front.pid);
                ticks_since_refresh = 0;
                let trigger = if pid_changed { CacheTrigger::AppSwitch } else { CacheTrigger::WindowSwitch };

                let empty_ok_walk =
                    walk_and_publish(&app, &shared, front.pid, &front.bundle_id, &front.name, trigger, &mut last_digest)
                        .is_some_and(|r| r.text_bytes == 0 && !r.partial && !r.truncated);
                // Many browsers (Chrome/Safari/etc.) build their AX tree lazily on first
                // query, so a snapshot right after switching TO the app can land before it
                // exists — the walk succeeds but finds nothing (not partial/truncated, so
                // it isn't a budget issue). Retry once, same pid, after the tree has had
                // time to build. A genuinely textless app (e.g. a blank canvas) just
                // republishes empty again — one extra bounded walk, not a poll loop.
                if empty_ok_walk {
                    std::thread::sleep(Duration::from_millis(500));
                    if crate::display::frontmost_app().map(|f| f.pid) == Some(front.pid) {
                        walk_and_publish(&app, &shared, front.pid, &front.bundle_id, &front.name, trigger, &mut last_digest);
                    }
                }
            }
        });
    }

    /// One bounded AX walk of `pid`'s focused window. Publishes the `context` event and
    /// records `metric.cache_update` (spec §3.10/§4.2.2) ONLY when the captured text's digest
    /// differs from the last published one — an unchanged periodic re-walk (same tab, no
    /// content change) is silently absorbed rather than re-emitted. Returns the raw walk
    /// result regardless, so the caller's empty-retry check (see `spawn_focus_watcher`) still
    /// sees genuine emptiness even on a deduped call.
    fn walk_and_publish(
        app: &AppHandle,
        shared: &Arc<Shared>,
        pid: i32,
        bundle_id: &str,
        name: &str,
        trigger: CacheTrigger,
        last_digest: &mut Option<String>,
    ) -> Option<shogun_core::capture::walk_policy::WalkResult> {
        // The exclusion gate applies here too, not only on the DB write path (FR-CAP-05: decided
        // "before any event is generated"). This walker is a second reader of the same window —
        // it warms the context cache — so without this an app the user turned off still has its
        // text read into memory and pushed across to the webview. The text is unused there today,
        // which is the only reason this was invisible; the moment anything consumes it, an excluded
        // app is in a prompt.
        let title = crate::axcache::focused_window(pid).and_then(|w| w.title());
        if crate::exclusions::mac::is_excluded(bundle_id, title.as_deref()) {
            // Drop whatever the previous app left behind, so the panel never shows stale context
            // while the user is looking at something SHOGUN is not allowed to read.
            if let Ok(mut g) = shared.last_context.lock() {
                if g.is_some() {
                    eprintln!("[spike] cache cleared — {bundle_id} is excluded from reading");
                }
                *g = None;
            }
            *last_digest = None;
            return None;
        }

        // Bracket ONLY the walk (spec §4.2.2): poll cadence / retry wait is not measured.
        let t0 = shared.clock.elapsed_ns();
        let result = crate::axcache::snapshot(pid, 300)?;
        let t1 = shared.clock.elapsed_ns();
        let latency_ms = t1.saturating_sub(t0) as f64 / 1e6;
        let (_, digest) = spike_harness::digest::text_digest(&result.text);
        let unchanged = last_digest.as_deref() == Some(digest.as_str());
        // Diagnostics carry bundle + counts ONLY — never the captured text. Skip the log for an
        // unchanged periodic re-walk (same tab, no edit) so the console stays readable.
        if !unchanged {
            eprintln!(
                "[spike] cache_update bundle={bundle_id} bytes={} elems={} depth={} partial={} {latency_ms:.1}ms",
                result.text_bytes, result.elements_visited, result.depth_reached, result.partial
            );
        }
        if unchanged {
            return Some(result);
        }
        *last_digest = Some(digest);
        let payload = ContextPayload {
            bundle_id: bundle_id.to_string(),
            title_masked: name.to_string(),
            text: result.text.clone(),
            captured_at_ms: now_epoch_ms(),
            partial: result.partial,
        };
        if let Ok(mut g) = shared.last_context.lock() {
            *g = Some(payload.clone());
        }
        let _ = app.emit("context", payload);
        app.state::<crate::metrics::SloRegister>().record_cache_update_ms(latency_ms);
        shared.recorder.record(Body::CacheUpdate(CacheUpdate::from_text(
            latency_ms,
            trigger,
            bundle_id,
            &result.text,
            result.elements_visited,
            result.depth_reached,
            result.partial,
            result.truncated,
            false,
        )));
        Some(result)
    }

    fn metrics_dir() -> PathBuf {
        // ~/Library/Application Support/dev.shogun.spike/metrics/ (spec §4.4; the
        // recorder appends YYYYMMDD.jsonl with UTC daily rotation).
        let base = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let dir = PathBuf::from(base).join("Library/Application Support/dev.shogun.spike/metrics");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    // ------------------------------------------------------------- commands
    // The webview→Rust half of the closed IPC contract (spec §3.11.2). Registered in
    // lib.rs via generate_handler; payload keys arrive camelCase and map to snake_case.

    /// rAF×2 paint-completion: the Q2 `t1`. Latency = t1 − last preview commit, both on the
    /// shared clock (JS time converted via the min-RTT offset). Measures the preview open
    /// (Idle→Hover) — the visible panel appearance the Phase 0 p95 refers to. The dedicated
    /// full-expand (SLO-01) and preview vs expand split are WP1.4.
    #[tauri::command]
    pub fn painted(state: String, t1_perf_ms: f64, shared: tauri::State<'_, Arc<Shared>>, app: AppHandle) {
        eprintln!("[spike] cmd painted state={state} t1={t1_perf_ms:.1}");
        if state != "hover" {
            return;
        }
        // Consume the commit FIRST, unconditionally. Each expand-commit must pair with
        // exactly ONE paint: a T5 revive (Collapsing→Expanded, statemachine §3.3) re-emits
        // `state=expanded` — and thus a `painted` — WITHOUT a fresh MarkExpandCommit, and
        // dev StrictMode can double-fire. Consuming here (before the offset gate) also means
        // an early expand whose paint lands before the clock offset is calibrated clears its
        // t0 instead of leaving it armed to be mis-paired with a LATER paint — that
        // cross-pairing produced a ~298s outlier and, after the first fix, a ~1.66s warm-up
        // outlier. swap→0 makes any second/late paint see t0==0 and drop.
        let t0 = shared.last_commit_ns.swap(0, Ordering::SeqCst);
        if t0 == 0 {
            return;
        }
        let t1_js_ns = (t1_perf_ms * 1e6) as u64;
        let Some(t1_ns) = shared.offset.lock().ok().and_then(|o| o.js_to_rust_ns(t1_js_ns)) else {
            // Offset not calibrated yet — the commit is already consumed above, so this
            // early sample is simply dropped (an unbiased latency needs the offset) rather
            // than left to bias a later paint.
            return;
        };
        if t1_ns <= t0 {
            return;
        }
        let latency_ms = (t1_ns - t0) as f64 / 1e6;
        // A real expand is bounded by the dwell+paint budget; a seconds-range value is an
        // orphaned pair (stalled webview), not a latency sample — drop it so the SLO tail
        // stays honest. The ceiling is 50× the 100ms SLO, so a genuine regression is still
        // recorded as a FAIL rather than hidden.
        if latency_ms > 5000.0 {
            eprintln!("[spike] dropping implausible expand latency {latency_ms:.0}ms (orphaned pair)");
            return;
        }
        // Same sample, kept in memory for the Full UI's health pane. The recorder above drains to
        // JSONL for offline analysis; the register summarises while the app runs.
        tauri::Manager::state::<crate::metrics::SloRegister>(&app).record_expand_ms(latency_ms);
        shared.recorder.record(Body::ExpandLatency(ExpandLatency {
            latency_ms,
            // Perceived total and enter-offset need the R_enter entry timestamp —
            // wired on-device (runbook D-02); until then they mirror latency.
            total_perceived_ms: latency_ms,
            hover_enter_offset_ms: 0.0,
            mode: if shared.is_notch { Mode::Notch } else { Mode::Pseudo },
            fullscreen: false,
            display_count: shared.display_count,
        }));
    }

    /// Interaction tally for the live session (Q4 input) plus the Expanded idle-timer reset.
    /// `boot` is the webview-alive ping (no tally, no engine input).
    #[tauri::command]
    pub fn interact(kind: String, shared: tauri::State<'_, Arc<Shared>>) {
        eprintln!("[spike] cmd interact kind={kind}");
        if let Ok(mut sess) = shared.session.lock() {
            if let Some(d) = sess.as_mut() {
                match kind.as_str() {
                    "click" => d.clicks += 1,
                    "key" => d.keys += 1,
                    "scroll" => d.scrolls += 1,
                    _ => {}
                }
            }
        }
        // A real interaction inside Expanded resets the 20s idle timeout (state machine
        // ignores it in other states). `boot` must not count as interaction.
        if kind != "boot" {
            shared.send(Ev::Input(EngineInput::Interaction));
        }
    }

    /// Click on the preview → promote to the full Expanded panel (Hover→Expanded).
    #[tauri::command]
    pub fn promote(shared: tauri::State<'_, Arc<Shared>>) {
        eprintln!("[spike] cmd promote (preview → expanded)");
        shared.send(Ev::Input(EngineInput::Click));
    }

    /// Global-hotkey open (⌘⇧Space). Fed from the webview today; a Rust-side NSEvent global
    /// monitor for the true system-wide hotkey is a later adapter step.
    #[tauri::command]
    pub fn hotkey(shared: tauri::State<'_, Arc<Shared>>) {
        eprintln!("[spike] cmd hotkey");
        shared.send(Ev::Input(EngineInput::Hotkey));
    }

    /// "Open Full UI" chosen from the panel.
    ///
    /// The window is built here rather than waiting on the state machine's `OpenFullUi` effect.
    /// That effect only fires from `Expanded`, but the panel's open/closed state is driven by
    /// direct clicks in the webview (see App.tsx) — the Rust machine tracks the hover lifecycle
    /// and is usually still Collapsed when this arrives, so routing the window through it meant
    /// the command was silently swallowed. Opening a separate window is not part of the notch's
    /// hover/expand lifecycle, so it shouldn't be gated on it.
    ///
    /// The input is still sent: when the machine *is* Expanded it collapses the overlay, which is
    /// the right thing when you've just asked for the big window. `build_full_ui_window` is
    /// idempotent, so the effect handler running too is harmless (it focuses the existing one).
    #[tauri::command]
    pub fn open_full_ui(shared: tauri::State<'_, Arc<Shared>>, app: AppHandle) {
        eprintln!("[spike] cmd open_full_ui");
        crate::build_full_ui_window(&app);
        shared.send(Ev::Input(EngineInput::OpenFullUi));
    }

    /// transitionend from the webview (T6) — the normal collapse completion.
    #[tauri::command]
    pub fn anim_done(_state: String, shared: tauri::State<'_, Arc<Shared>>) {
        eprintln!("[spike] cmd anim_done");
        shared.send(Ev::Input(EngineInput::AnimDone));
    }

    /// Esc / transparent-margin click (T4b/T4c).
    #[tauri::command]
    pub fn collapse_request(reason: String, shared: tauri::State<'_, Arc<Shared>>) {
        match reason.as_str() {
            "esc" => {
                shared.set_reason("esc");
                shared.send(Ev::Input(EngineInput::Esc));
            }
            "outside_click" => {
                shared.set_reason("outside_click");
                shared.send(Ev::Input(EngineInput::OutsideClick));
            }
            _ => {}
        }
    }

    /// Clock-sync ack: feeds the min-RTT offset estimator (spec §4.1).
    #[tauri::command]
    pub fn clock_sync_ack(seq: u32, js_perf_ms: f64, shared: tauri::State<'_, Arc<Shared>>) {
        eprintln!("[spike] cmd clock_sync_ack seq={seq}");
        let recv_ns = shared.clock.elapsed_ns();
        let Some(send_ns) = shared.sync_sent.lock().ok().and_then(|mut m| m.remove(&seq)) else {
            return;
        };
        if let Ok(mut o) = shared.offset.lock() {
            o.observe(SyncSample {
                rust_send_ns: send_ns,
                rust_recv_ns: recv_ns,
                js_perf_ns: (js_perf_ms * 1e6) as u64,
            });
        }
    }

    /// Search-field focus: minimal key-window grab (the NotchPanel class is compiled with
    /// can_become_key_window=true). Key resignation + the level 25↔101 IME dance are
    /// on-device work (runbook D-04, tauri-nspanel #104).
    #[tauri::command]
    pub fn focus_field(focused: bool, app: AppHandle) {
        if !focused {
            return;
        }
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            // The visible surface is the NATIVE NSPanel (or the fallback window) — make IT key so
            // typing reaches the webview. A nonactivating panel becomes key without activating
            // the app, so the frontmost app keeps focus otherwise.
            if let Some(ptr) = crate::overlay_ptr(&app2) {
                use objc2::msg_send;
                use objc2::runtime::AnyObject;
                let nil: *mut AnyObject = std::ptr::null_mut();
                // SAFETY: main thread, live NSWindow/NSPanel.
                unsafe {
                    let _: () = msg_send![ptr, makeKeyAndOrderFront: nil];
                }
            }
        });
    }
}
