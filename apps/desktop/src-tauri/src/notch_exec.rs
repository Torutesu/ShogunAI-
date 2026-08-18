//! Notch action execution (§6.6.2, product core). Clicking a context-action button runs it through
//! the L1/L2/L3-gated [`ExecutionEngine`](shogun_agents::engine): L1 auto-runs, L2 waits for a
//! one-tap confirm. The one L3 candidate — DraftReply (B-5) — never touches the engine (which
//! structurally rejects sends, invariant 4): it is dispatched to the Reply Drafter, whose draft
//! lands on the shared L3 approval queue for an explicit human confirm. The gating logic is the
//! Linux-tested engine; this module supplies the macOS effector (the real local effects) and the
//! reporting observer, and the Tauri commands the panel calls.
#![allow(dead_code, unused_imports)]

#[cfg(target_os = "macos")]
pub use mac::NotchEngine;

/// L2 confirm window (how long a queued one-tap action waits for the tap) — §6.6.2.
pub const CONFIRM_TIMEOUT_MS: u64 = 8_000;

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::Mutex;

    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;
    use shogun_agents::engine::{
        ActionId, Disposition, ExecutionEngine, ExecutionObserver, LocalEffector, Outcome,
        RejectReason,
    };
    use shogun_core::daemon::Db;
    use shogun_fusion::assemble::ScreenContext;
    use shogun_fusion::{Action, LocalAction, SendAction};

    use super::CONFIRM_TIMEOUT_MS;
    use crate::axcache::focused_window;
    use crate::display::frontmost_app;

    // ---- real local effects (B-2). All macOS-only — this whole module is cfg(macos); Linux CI
    // ---- covers the engine gating, the device covers these effects. ---------------------------

    /// Deliver a user notification. macOS-only. Uses `NSUserNotificationCenter` — deprecated but
    /// dependency-free (the UNUserNotifications framework is not among this app's objc2 crates,
    /// and adding one is out of scope), and it matches the raw `msg_send!` style `lib.rs` uses for
    /// AppKit. The text is a state summary (already confidence-gated), shown to the user only —
    /// it is never written to a log (コード規約: capture-derived text stays out of logs).
    fn deliver_notification(text: &str) -> Result<(), String> {
        // SAFETY: plain AppKit/Foundation messaging. `new` returns +1 (released below);
        // `defaultUserNotificationCenter` is get-rule (not released). NSString refs stay alive
        // across the calls that borrow them.
        unsafe {
            let center: *mut AnyObject = msg_send![
                class!(NSUserNotificationCenter),
                defaultUserNotificationCenter
            ];
            if center.is_null() {
                return Err("no notification center (unbundled process?)".into());
            }
            let note: *mut AnyObject = msg_send![class!(NSUserNotification), new];
            if note.is_null() {
                return Err("could not create the notification".into());
            }
            let title = NSString::from_str("SHOGUN");
            let _: () = msg_send![note, setTitle: &*title];
            let body = NSString::from_str(text);
            let _: () = msg_send![note, setInformativeText: &*body];
            let _: () = msg_send![center, deliverNotification: note];
            let _: () = msg_send![note, release];
        }
        Ok(())
    }

    /// Put text on the general pasteboard (device-local, invariant 4). macOS-only. Same
    /// pasteboard pattern as `inline_source::paste_at_cursor`, minus the save/restore: copying IS
    /// the user's intent here, so overwriting the clipboard is the effect, not a side effect.
    fn copy_to_clipboard(text: &str) -> Result<(), String> {
        // SAFETY: generalPasteboard is get-rule; the NSStrings are owned locally and outlive the
        // calls that borrow them.
        unsafe {
            let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
            if pb.is_null() {
                return Err("no pasteboard".into());
            }
            let utf8 = NSString::from_str("public.utf8-plain-text");
            let ours = NSString::from_str(text);
            let _: isize = msg_send![pb, clearContents];
            let ok: bool = msg_send![pb, setString: &*ours, forType: &*utf8];
            if !ok {
                return Err("could not write the pasteboard".into());
            }
        }
        Ok(())
    }

    /// Bring an app to the foreground by bundle id via `NSWorkspace` (macOS-only; same raw
    /// `msg_send!` style as the workspace watchers in `lib.rs`). `pub(crate)`: the daily-summary
    /// source chips (issue #10) re-open a captured event's app through the same seam.
    pub(crate) fn open_app(bundle_id: &str) -> Result<(), String> {
        // SAFETY: sharedWorkspace and both returned URLs are get-rule/autoreleased — nothing to
        // release; the NSString outlives the call that borrows it.
        unsafe {
            let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            if ws.is_null() {
                return Err("no workspace".into());
            }
            let bid = NSString::from_str(bundle_id);
            let url: *mut AnyObject = msg_send![ws, URLForApplicationWithBundleIdentifier: &*bid];
            if url.is_null() {
                return Err(format!("no app for bundle id {bundle_id}"));
            }
            let ok: bool = msg_send![ws, openURL: url];
            if ok {
                Ok(())
            } else {
                Err(format!("could not open {bundle_id}"))
            }
        }
    }

    /// Reveal a file in Finder via `NSWorkspace` (macOS-only).
    fn reveal_file(path: &str) -> Result<(), String> {
        // SAFETY: all returned objects are get-rule/autoreleased class-method results; the
        // NSString outlives the calls that borrow it.
        unsafe {
            let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            if ws.is_null() {
                return Err("no workspace".into());
            }
            let ns_path = NSString::from_str(path);
            let url: *mut AnyObject = msg_send![class!(NSURL), fileURLWithPath: &*ns_path];
            if url.is_null() {
                return Err("could not build the file URL".into());
            }
            let urls: *mut AnyObject = msg_send![class!(NSArray), arrayWithObject: url];
            let _: () = msg_send![ws, activateFileViewerSelectingURLs: urls];
        }
        Ok(())
    }

    /// The macOS local-effect seam. DB-effectful actions run against the shared handle; OS effects
    /// (open app, notification, clipboard, reveal) are the real AppKit calls above. Never runs a
    /// send (the engine rejects L3 before it reaches here). The match is exhaustive on purpose —
    /// a future `LocalAction` variant must fail loudly here at compile time, not fall through a
    /// catch-all stub.
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
                    eprintln!("[exec] local search → {hits} hit(s)");
                    Ok(())
                }
                Action::Local(LocalAction::SaveDraft { target }) => {
                    self.db.append_note(&format!("draft ({target})"));
                    eprintln!("[exec] saved local draft for {target}");
                    Ok(())
                }
                Action::Local(LocalAction::ShowNotification { text }) => {
                    // Log the kind only, never the text (state summaries stay out of logs).
                    eprintln!("[exec] show notification");
                    deliver_notification(text)
                }
                Action::Local(LocalAction::CopyToClipboard { text }) => {
                    eprintln!("[exec] copy to clipboard ({} chars)", text.chars().count());
                    copy_to_clipboard(text)
                }
                Action::Local(LocalAction::OpenApp { bundle_id }) => {
                    eprintln!("[exec] open app {bundle_id}");
                    open_app(bundle_id)
                }
                Action::Local(LocalAction::RevealFile { path }) => {
                    eprintln!("[exec] reveal file");
                    reveal_file(path)
                }
                Action::Local(LocalAction::UpdateState { table, state_id }) => {
                    // The one L2 local action: resolve the state row (the same mutation the panel's
                    // click-to-resolve performs). Unknown tables are an error, not a silent no-op.
                    let done = match *table {
                        "commitments" => self.db.resolve_commitment(*state_id),
                        "open_loops" => self.db.resolve_open_loop(*state_id),
                        other => return Err(format!("update_state: unsupported table {other}")),
                    };
                    eprintln!("[exec] update {table} #{state_id} → resolved: {done}");
                    if done {
                        Ok(())
                    } else {
                        Err(format!("update_state: no such {table} row"))
                    }
                }
                // Unreachable — the engine rejects L3 at submit (invariant 4); the DraftReply
                // candidate is dispatched to the approval queue before it can reach the engine.
                Action::Send(_) => Err("external sends go through the L3 approval queue".into()),
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
        Mutex::new(ExecutionEngine::new(
            NotchEffector::new(db),
            NotchObserver,
            CONFIRM_TIMEOUT_MS,
        ))
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
                let title = focused_window(front.pid)
                    .and_then(|w| w.title())
                    .unwrap_or_default();
                ScreenContext {
                    app_bundle_id: front.bundle_id,
                    window_title: title,
                    salient: Vec::new(),
                }
            }
            None => ScreenContext::default(),
        }
    }

    /// Tauri command: run the context action at `index` (as shown by `notch_actions`). Re-assembles
    /// the candidates for the current screen and dispatches the Nth. A DraftReply (external
    /// send, L3 by construction) never reaches the L1/L2 engine: it drafts off-thread and lands
    /// on the ONE shared approval queue, returning `"drafting"`. Everything else goes to the
    /// engine and returns a status: `"executed"` (L1 auto-ran), `"confirm:<id>"` (L2 — call
    /// `confirm_notch_action`), `"rejected"`, or `"failed"` / `"unavailable"` / `"no-action"`.
    #[tauri::command]
    pub fn run_notch_action(
        index: usize,
        db: tauri::State<'_, Db>,
        engine: tauri::State<'_, NotchEngine>,
        analytics: tauri::State<'_, crate::analytics::Analytics>,
        app: tauri::AppHandle,
    ) -> String {
        let cache = db.context_actions(current_screen(), None);
        let Some(cand) = cache.actions.get(index) else {
            return "no-action".to_string();
        };
        let level = format!("{:?}", cand.level);
        // Plan gate (issue #97): resolved core-side per click; the engine rejects when the plan
        // has no agent execution (Standard / expired trial).
        let ent = crate::entitlement::mac::current(&app);

        // B-5: a DraftReply candidate is an external send — L3 by construction — so it never goes
        // through the L1/L2 engine (which structurally rejects sends). Dispatch it to the Reply
        // Drafter instead: draft on the Agent lane off-thread, then enqueue on the ONE shared
        // approval queue (origin: Ui) for the L3 confirm UI to list/edit/confirm (FR-AG-03).
        if let Action::Send(SendAction::SendEmail { to }) = &cand.action {
            let outcome = if !ent.agent_execution {
                "rejected"
            } else if spawn_draft_reply(
                to.clone(),
                cand.rationale.clone(),
                db.inner().clone(),
                &app,
            ) {
                "drafting"
            } else {
                "failed"
            };
            let mut p = shogun_core::analytics::Props::new();
            p.insert("query_type".into(), serde_json::Value::from("notch_action"));
            p.insert("permission_level".into(), serde_json::Value::from(level));
            p.insert("outcome".into(), serde_json::Value::from(outcome));
            analytics.capture("shogun_query_executed", p);
            return outcome.to_string();
        }

        let Ok(mut eng) = engine.lock() else {
            return "unavailable".to_string();
        };
        let submitted = eng.submit(cand.action.clone(), now_ms(), &ent);

        // shogun_query_executed（#61）: submit した時のみ発火。
        let outcome = match &submitted.disposition {
            Disposition::AutoRan => "ok",
            Disposition::Failed => "failed",
            Disposition::AwaitingConfirm => "awaiting_confirm",
            Disposition::Rejected(_) => "rejected",
        };
        let mut p = shogun_core::analytics::Props::new();
        p.insert("query_type".into(), serde_json::Value::from("notch_action"));
        p.insert("permission_level".into(), serde_json::Value::from(level));
        p.insert("outcome".into(), serde_json::Value::from(outcome));
        analytics.capture("shogun_query_executed", p);

        match submitted.disposition {
            Disposition::AutoRan => "executed".to_string(),
            Disposition::Failed => "failed".to_string(),
            Disposition::AwaitingConfirm => format!("confirm:{}", submitted.id.0),
            Disposition::Rejected(_) => "rejected".to_string(),
        }
    }

    /// Start the Reply Drafter for a notch DraftReply click (B-5): a dedicated thread (the Agent
    /// call blocks for seconds — same pattern as `inline_source::run_inline_at_cursor`) drafts the
    /// body and enqueues it on the shared [`ApprovalQueueState`](crate::approvals::mac) with
    /// origin `Ui`. Returns whether the thread was started (false when the queue state is absent).
    /// The error log carries provider/queue reasons only — never the draft or captured text.
    fn spawn_draft_reply(to: String, rationale: String, db: Db, app: &tauri::AppHandle) -> bool {
        use tauri::Manager;
        let Some(queue) = app.try_state::<crate::approvals::mac::ApprovalQueueState>() else {
            eprintln!("[exec] draft reply: no approval queue in state");
            return false;
        };
        let queue = queue.inner().clone();
        let directives = app
            .try_state::<crate::user_config_watch::UserConfigState>()
            .map(|s| s.directives())
            .unwrap_or_default();
        let screen = current_screen();
        std::thread::spawn(move || {
            // Ground the draft in the open loop plus what is on screen (same inputs the candidate
            // was scored from). The subject seed doubles as the draft's subject line.
            let context = format!(
                "Unanswered message (open loop): {rationale}\nOn screen: {} — {}",
                screen.app_bundle_id, screen.window_title
            );
            match crate::approvals::mac::draft_and_enqueue(
                crate::approvals::mac::Draft {
                    kind: "email",
                    destination: &to,
                    subject: &rationale,
                    context: &context,
                },
                &queue,
                &db,
                &directives,
                // The notch dispatched this on the user's tap — the human face, same as the
                // panel's own draft command.
                shogun_agents::approval::ApprovalOrigin::Ui,
            ) {
                Ok(id) => eprintln!("[exec] reply draft queued for L3 approval (id {id})"),
                Err(e) => eprintln!("[exec] reply draft failed: {e}"),
            }
        });
        true
    }

    /// On-device self-test of the product core, independent of the (fragile) panel rendering:
    /// assemble the current context actions, log them, and submit the top one to the engine. Proves
    /// capture → memory → fusion → action → execution end-to-end from a keypress.
    pub fn self_test(app: &tauri::AppHandle) {
        use tauri::Manager;
        let Some(db) = app.try_state::<Db>() else {
            eprintln!("[selftest] no Db in state");
            return;
        };
        let cache = db.context_actions(current_screen(), None);
        eprintln!(
            "[selftest] {} context action(s) for the current screen:",
            cache.actions.len()
        );
        for (i, a) in cache.actions.iter().enumerate() {
            // Level + discriminant only: the Action Debug carries captured text
            // (ShowNotification/CopyToClipboard payloads), and rationale derives from it.
            eprintln!(
                "[selftest]   [{i}] {:?} {}",
                a.level,
                match &a.action {
                    Action::Local(_) => "local",
                    Action::Send(_) => "send",
                }
            );
        }
        match (cache.actions.first(), app.try_state::<NotchEngine>()) {
            (Some(first), Some(engine)) => {
                if let Ok(mut eng) = engine.lock() {
                    let ent = crate::entitlement::mac::current(app);
                    let sub = eng.submit(first.action.clone(), now_ms(), &ent);
                    eprintln!("[selftest] submitted top action → {:?}", sub.disposition);
                }
            }
            (None, _) => {
                eprintln!("[selftest] no gated actions yet (capture more promise/loop text)")
            }
            _ => {}
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
