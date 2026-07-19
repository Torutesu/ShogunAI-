//! Integration glue (spec §3.3/§3.4/§3.11): runs the pure `NotchEngine`, drives one-shot
//! timers from a single timer thread with generation checks **at receive time** (closing
//! the schedule/cancel TOCTOU), applies `EngineOutput`s, implements the webview→Rust
//! command half of the closed IPC contract (§3.11.2), and emits the measurement streams
//! (expand_latency via `painted`+clock offset, expand_session, cpu_sample, heartbeat,
//! tap_status). All timestamps share ONE `MonoClock` (the recorder's) so Q2 math has a
//! single timeline. Decision logic stays in `spike_core::engine` (unit-tested); behaviour
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
    use spike_core::engine::{EngineInput, EngineOutput, NotchEngine};
    use spike_core::hover::HoverParams;
    use spike_core::statemachine::{Params, State, Timer};
    use spike_harness::clock::{OffsetEstimator, SyncSample};
    use spike_harness::cpu::{read_process_usage, CpuMeter, CPU_METHOD};
    use spike_harness::record::{
        Body, CpuSample, ExpandLatency, ExpandSession, Heartbeat, Interactions, Mode,
        StateTransition,
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
    use tauri_nspanel::ManagerExt;

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

    /// Events into the engine loop.
    pub enum Ev {
        Tap(TapEvent),
        /// Timer fire carrying the generation it was armed with; accepted only if the
        /// generation is still current at receive time (TOCTOU-proof, review #6).
        Timer(Timer, u64),
        Input(EngineInput),
    }

    /// An open Expanded session (spec §4.2.4).
    struct SessionDraft {
        opened_at_ms: u64,
        opened_mono_ns: u64,
        clicks: u32,
        keys: u32,
        scrolls: u32,
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
        is_notch: bool,
        display_count: u32,
    }

    impl Shared {
        fn send(&self, ev: Ev) {
            if let Ok(tx) = self.ev_tx.lock() {
                let _ = tx.send(ev);
            }
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
    }

    // ---------------------------------------------------------------- timers

    const TIMERS: [Timer; 3] = [Timer::Dwell, Timer::Grace, Timer::CollapseAnim];

    fn tidx(t: Timer) -> usize {
        match t {
            Timer::Dwell => 0,
            Timer::Grace => 1,
            Timer::CollapseAnim => 2,
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
        gens: Arc<[AtomicU64; 3]>,
    }

    impl TimerSvc {
        fn spawn(ev_tx: Sender<Ev>) -> Self {
            let (cmd_tx, cmd_rx) = channel::<ArmCmd>();
            let gens: Arc<[AtomicU64; 3]> = Arc::new(Default::default());
            std::thread::spawn(move || {
                let mut slots: [Option<(u64, Instant, Timer)>; 3] = [None, None, None];
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
            let mut engine = NotchEngine::new(
                geo.regions,
                geo.menubar_min_y,
                geo.primary_height,
                HoverParams::default(),
                Params::default(),
            );
            let timers = TimerSvc::spawn(ev_tx);
            let mut prev_state = State::Idle;
            while let Ok(ev) = ev_rx.recv() {
                let input = match ev {
                    Ev::Tap(TapEvent::Status { active }) => {
                        shared.recorder.record(Body::TapStatus { active });
                        continue;
                    }
                    Ev::Tap(TapEvent::Moved { x, y, buttons }) => {
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
                        if t == Timer::Grace {
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
                    State::Expanded => {
                        // Open a session unless one is live (a T5 revive continues it).
                        if let Ok(mut sess) = shared.session.lock() {
                            if sess.is_none() {
                                *sess = Some(SessionDraft {
                                    opened_at_ms: now_epoch_ms(),
                                    opened_mono_ns: shared.clock.elapsed_ns(),
                                    clicks: 0,
                                    keys: 0,
                                    scrolls: 0,
                                    manual_false_positive: false,
                                });
                            }
                        }
                    }
                    State::Idle => close_session(shared),
                    _ => {}
                }
                *prev_state = s;
            }
            EngineOutput::SetIgnoresMouse(b) => {
                let app2 = app.clone();
                let _ = app.run_on_main_thread(move || {
                    if let Ok(panel) = app2.get_webview_panel("notch") {
                        panel.set_ignores_mouse_events(b);
                    }
                });
            }
            EngineOutput::ScheduleTimer { timer, ms } => timers.schedule(timer, ms),
            EngineOutput::CancelTimer(timer) => timers.cancel(timer),
            EngineOutput::ExpandCommit => {
                let t0 = shared.clock.elapsed_ns();
                shared.last_commit_ns.store(t0, Ordering::SeqCst);
                shared.recorder.record(Body::ExpandCommit { t0_mono_ns: t0 });
            }
            EngineOutput::TopBandEntry => {
                shared.recorder.record(Body::TopBandEntry { count: 1 });
            }
        }
    }

    /// Close the live session (Expanded → Idle) into an `event.expand_session` record.
    fn close_session(shared: &Shared) {
        let Some(d) = shared.session.lock().ok().and_then(|mut s| s.take()) else {
            return;
        };
        let duration_ms = (shared.clock.elapsed_ns().saturating_sub(d.opened_mono_ns)) / 1_000_000;
        let interactions = Interactions { clicks: d.clicks, keys: d.keys, scrolls: d.scrolls };
        let auto_fp = d.clicks + d.keys + d.scrolls == 0 && duration_ms < 1500;
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
                let visible = app2.get_webview_panel("notch").map(|p| p.is_visible()).unwrap_or(false);
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

    /// rAF×2 paint-completion: the Q2 `t1`. Latency = t1 − last expand commit, both on
    /// the shared clock (JS time converted via the min-RTT offset).
    #[tauri::command]
    pub fn painted(state: String, t1_perf_ms: f64, shared: tauri::State<'_, Arc<Shared>>) {
        eprintln!("[spike] cmd painted state={state} t1={t1_perf_ms:.1}");
        if state != "expanded" {
            return;
        }
        let t1_js_ns = (t1_perf_ms * 1e6) as u64;
        let Some(t1_ns) = shared.offset.lock().ok().and_then(|o| o.js_to_rust_ns(t1_js_ns)) else {
            return; // offset not calibrated yet — drop rather than record a biased value
        };
        // Consume the commit so each expand-commit pairs with exactly ONE paint. A T5
        // revive (Collapsing→Expanded, statemachine §3.3) re-emits `state=expanded` — and
        // therefore a `painted` — WITHOUT a fresh MarkExpandCommit; dev StrictMode can also
        // double-fire the paint. Without consuming, those extra paints reuse a stale t0 and
        // inject a multi-second outlier that dominates p95 (observed: a 298s sample from an
        // hours-old commit). swap→0 makes any second paint see t0==0 and drop.
        let t0 = shared.last_commit_ns.swap(0, Ordering::SeqCst);
        if t0 == 0 || t1_ns <= t0 {
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

    /// Interaction tally for the live Expanded session (Q4 auto-false-positive input).
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
            if let Ok(panel) = app2.get_webview_panel("notch") {
                panel.make_key_and_order_front();
            }
        });
    }
}
