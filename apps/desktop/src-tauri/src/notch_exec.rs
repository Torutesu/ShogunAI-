//! Notch action execution (§6.6.2, product core). Clicking a context-action button runs it through
//! the L1/L2/L3-gated [`ExecutionEngine`](shogun_agents::engine): L1 auto-runs, L2 waits for a
//! one-tap confirm, L3 is refused (v1 has no send path, invariant 4 — context actions never carry an
//! L3 anyway). The gating logic is the Linux-tested engine; this module supplies the macOS effector
//! (the real local effects) and the reporting observer, and the Tauri commands the panel calls.
#![allow(dead_code)]

#[cfg(target_os = "macos")]
pub use mac::NotchEngine;

/// L2 confirm window (how long a queued one-tap action waits for the tap) — §6.6.2.
pub const CONFIRM_TIMEOUT_MS: u64 = 8_000;

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::Mutex;

    use shogun_agents::engine::{
        ActionId, Disposition, ExecutionEngine, ExecutionObserver, LocalEffector, Outcome, RejectReason,
    };
    use shogun_core::daemon::Db;
    use shogun_fusion::assemble::ScreenContext;
    use shogun_fusion::{Action, LocalAction};

    use super::CONFIRM_TIMEOUT_MS;
    use crate::axcache::focused_window;
    use crate::display::frontmost_app;

    /// The macOS local-effect seam. DB-effectful actions run against the shared handle; OS effects
    /// (open app, notification, clipboard) are best-effort and logged. Never runs a send (the engine
    /// rejects L3 before it reaches here).
    pub struct NotchEffector {
        db: Db,
    }

    impl NotchEffector {
        pub fn new(db: Db) -> Self {
            Self { db }
        }
    }

    impl LocalEffector for NotchEffector {
        fn run(&self, action: &Action) -> Result<(), String> {
            match action {
                Action::Local(LocalAction::LocalSearch { query }) => {
                    let hits = self.db.search(query, 5).len();
                    eprintln!("[exec] local search {query:?} → {hits} hit(s)");
                    Ok(())
                }
                Action::Local(LocalAction::SaveDraft { target }) => {
                    self.db.append_note(&format!("draft ({target})"));
                    eprintln!("[exec] saved local draft for {target}");
                    Ok(())
                }
                Action::Local(LocalAction::ShowNotification { .. }) => {
                    // Byte-length only would be odd for a notification; log the kind, not the text.
                    eprintln!("[exec] show notification");
                    Ok(())
                }
                Action::Local(other) => {
                    eprintln!("[exec] local action: {other:?}");
                    Ok(())
                }
                // Unreachable — the engine rejects L3 at submit (invariant 4).
                Action::Send(_) => Err("external send is not available in v1".into()),
            }
        }
    }

    /// Reporting seam → console (the daemon event-log wiring can replace this later). Logs the id and
    /// disposition only; never the action's captured-derived text.
    pub struct NotchObserver;

    impl ExecutionObserver for NotchObserver {
        fn on_executed(&self, id: ActionId, _action: &Action) {
            eprintln!("[exec] action {} executed", id.0);
        }
        fn on_rejected(&self, id: ActionId, _action: &Action, reason: &RejectReason) {
            eprintln!("[exec] action {} rejected: {reason:?}", id.0);
        }
        fn on_cancelled(&self, id: ActionId, _action: &Action) {
            eprintln!("[exec] action {} cancelled", id.0);
        }
        fn on_expired(&self, id: ActionId, _action: &Action) {
            eprintln!("[exec] action {} confirm expired", id.0);
        }
        fn on_failed(&self, id: ActionId, _action: &Action, error: &str) {
            eprintln!("[exec] action {} failed: {error}", id.0);
        }
    }

    /// The engine, behind a `Mutex` so the two commands (submit / confirm) share the pending queue.
    pub type NotchEngine = Mutex<ExecutionEngine<NotchEffector, NotchObserver>>;

    /// Build the engine for Tauri state (called once in setup).
    pub fn new_engine(db: Db) -> NotchEngine {
        Mutex::new(ExecutionEngine::new(NotchEffector::new(db), NotchObserver, CONFIRM_TIMEOUT_MS))
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// The current focused screen (frontmost app + window title), for re-assembling the actions.
    fn current_screen() -> ScreenContext {
        match frontmost_app() {
            Some(front) => {
                let title = focused_window(front.pid).and_then(|w| w.title()).unwrap_or_default();
                ScreenContext { app_bundle_id: front.bundle_id, window_title: title, salient: Vec::new() }
            }
            None => ScreenContext::default(),
        }
    }

    /// Tauri command: run the context action at `index` (as shown by `notch_actions`). Re-assembles
    /// the candidates for the current screen, submits the Nth to the engine, and returns a status:
    /// `"executed"` (L1 auto-ran), `"confirm:<id>"` (L2 — call `confirm_notch_action`), `"rejected"`,
    /// or `"unavailable"` / `"no-action"`.
    #[tauri::command]
    pub fn run_notch_action(index: usize, db: tauri::State<'_, Db>, engine: tauri::State<'_, NotchEngine>) -> String {
        let cache = db.context_actions(current_screen(), None);
        let Some(cand) = cache.actions.get(index) else {
            return "no-action".to_string();
        };
        let Ok(mut eng) = engine.lock() else {
            return "unavailable".to_string();
        };
        let submitted = eng.submit(cand.action.clone(), now_ms());
        match submitted.disposition {
            Disposition::AutoRan => "executed".to_string(),
            Disposition::AwaitingConfirm => format!("confirm:{}", submitted.id.0),
            Disposition::Rejected(_) => "rejected".to_string(),
        }
    }

    /// Tauri command: confirm a pending L2 action by id (from `run_notch_action`'s `"confirm:<id>"`).
    /// Returns `"executed"`, `"expired"`, `"failed"`, `"cancelled"`, or `"unknown"`.
    #[tauri::command]
    pub fn confirm_notch_action(id: u64, engine: tauri::State<'_, NotchEngine>) -> String {
        let Ok(mut eng) = engine.lock() else {
            return "unavailable".to_string();
        };
        match eng.confirm(ActionId(id), now_ms()) {
            Outcome::Executed => "executed",
            Outcome::Expired => "expired",
            Outcome::Failed(_) => "failed",
            Outcome::Cancelled => "cancelled",
            Outcome::Unknown => "unknown",
        }
        .to_string()
    }
}
