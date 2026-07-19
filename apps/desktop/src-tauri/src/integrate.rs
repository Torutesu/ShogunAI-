//! Integration glue (spec §3.3/§3.4/§3.11): runs the pure `NotchEngine` on a thread,
//! drives real one-shot timers with generation-based cancellation, and applies
//! `EngineOutput`s — push the `state` event to the webview, toggle the panel's
//! `ignoresMouseEvents` on the main thread, and record markers to the harness.
//!
//! The decision logic is all in `spike_core::engine` (unit-tested); this file is the
//! macOS-only wiring. Behaviour (real timing, panel visuals) is validated on-device; here
//! it is compile-verified. Mouse samples currently carry buttons=0 (the tap masks only
//! MouseMoved); on-device, add button events + `EngineInput::ButtonDown/Up`.
#![allow(dead_code, unused_imports)]

#[cfg(target_os = "macos")]
pub use mac::start;

#[cfg(target_os = "macos")]
mod mac {
    use crate::geometry::Regions;
    use crate::hover::MouseSample;
    use spike_core::engine::{EngineInput, EngineOutput, NotchEngine};
    use spike_core::hover::HoverParams;
    use spike_core::statemachine::{Params, Timer};
    use spike_harness::record::{Body, StateTransition};
    use spike_harness::recorder::Recorder;
    use spike_harness::MonoClock;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::{channel, Sender};
    use std::sync::Arc;
    use std::time::Duration;
    use tauri::{AppHandle, Emitter, Manager};
    use tauri_nspanel::ManagerExt;

    #[derive(Clone, serde::Serialize)]
    struct StatePayload {
        state: &'static str,
        t0_mono_ns: u64,
    }

    /// Events into the engine loop.
    enum Ev {
        Mouse(MouseSample, u64),
        Timer(Timer),
    }

    /// One-shot timers with generation-based cancellation (drops stale/rescheduled fires).
    struct TimerService {
        tx: Sender<Ev>,
        gens: [Arc<AtomicU64>; 3],
    }

    impl TimerService {
        fn new(tx: Sender<Ev>) -> Self {
            Self { tx, gens: [Arc::default(), Arc::default(), Arc::default()] }
        }
        fn idx(t: Timer) -> usize {
            match t {
                Timer::Dwell => 0,
                Timer::Grace => 1,
                Timer::CollapseAnim => 2,
            }
        }
        fn schedule(&self, t: Timer, ms: u64) {
            let slot = self.gens[Self::idx(t)].clone();
            let g = slot.fetch_add(1, Ordering::SeqCst) + 1;
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(ms));
                if slot.load(Ordering::SeqCst) == g {
                    let _ = tx.send(Ev::Timer(t));
                }
            });
        }
        fn cancel(&self, t: Timer) {
            self.gens[Self::idx(t)].fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Start the integration loop. Consumes `mouse_rx` (from the hover tap) and drives the
    /// engine; `regions`/`menubar_min_y`/`primary_height` seed geometry (spec §3.2/§3.4.7).
    pub fn start(
        app: AppHandle,
        regions: Regions,
        menubar_min_y: f64,
        primary_height: f64,
        mouse_rx: std::sync::mpsc::Receiver<MouseSample>,
    ) {
        let recorder = Recorder::new(8192);
        recorder.spawn_file_flusher(metrics_path(), Duration::from_secs(1));

        let (ev_tx, ev_rx) = channel::<Ev>();

        // Bridge raw mouse samples → timestamped engine events.
        {
            let ev_tx = ev_tx.clone();
            std::thread::spawn(move || {
                let clock = MonoClock::new();
                while let Ok(m) = mouse_rx.recv() {
                    let t_ms = clock.elapsed_ns() / 1_000_000;
                    if ev_tx.send(Ev::Mouse(m, t_ms)).is_err() {
                        break;
                    }
                }
            });
        }

        // Engine loop.
        std::thread::spawn(move || {
            let mut engine = NotchEngine::new(
                regions,
                menubar_min_y,
                primary_height,
                HoverParams::default(),
                Params::default(),
            );
            let timers = TimerService::new(ev_tx);
            let clock = MonoClock::new();
            while let Ok(ev) = ev_rx.recv() {
                let input = match ev {
                    Ev::Mouse(m, t_ms) => EngineInput::MouseCg { x: m.x, y: m.y, t_ms, buttons: 0 },
                    Ev::Timer(t) => EngineInput::TimerFired(t),
                };
                for out in engine.on_input(input) {
                    apply(&app, &recorder, &timers, &clock, out);
                }
            }
        });
    }

    fn apply(app: &AppHandle, recorder: &Recorder, timers: &TimerService, clock: &MonoClock, out: EngineOutput) {
        match out {
            EngineOutput::WebviewState(s) => {
                let _ = app.emit("state", StatePayload { state: s.tag(), t0_mono_ns: clock.elapsed_ns() });
                recorder.record(Body::StateTransition(StateTransition {
                    from: String::new(),
                    to: s.tag().to_string(),
                    trigger: "engine".to_string(),
                }));
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
                // Q2 t0 marker. Full latency (t1−t0) is computed on-device from the webview
                // `painted` round-trip + clock offset; here we log the commit instant.
                recorder.record(Body::StateTransition(StateTransition {
                    from: "hoverintent".to_string(),
                    to: "expanded".to_string(),
                    trigger: "expand_commit".to_string(),
                }));
            }
            EngineOutput::TopBandEntry => {
                recorder.record(Body::TopBandEntry { count: 1 });
            }
        }
    }

    fn metrics_path() -> PathBuf {
        // ~/Library/Application Support/dev.shogun.spike/metrics/spike.jsonl (spec §4.4).
        let base = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let dir = PathBuf::from(base).join("Library/Application Support/dev.shogun.spike/metrics");
        let _ = std::fs::create_dir_all(&dir);
        dir.join("spike.jsonl")
    }
}
