//! Notch context-action command (§6.1, Notch product core). Bridges the Rust core's
//! `Db::context_actions` (ranked, confidence-gated candidates) to the webview panel.
//!
//! The panel calls `notch_actions` on expand: the daemon assembles the candidates for the focused
//! screen and this maps them to a flat, serializable `ActionView` list the React panel renders as
//! buttons. Each view carries its permission `level` so the UI gates L1 (auto-eligible) vs L2/L3
//! (confirm) — invariant 4 surfaced in the UI. AX text is never serialized here; only the derived
//! action label + rationale (already confidence-gated) cross the boundary.
#![allow(dead_code)]

/// A flattened action candidate for the webview (serde-friendly; the domain types aren't
/// `Serialize`, so we project here).
#[derive(serde::Serialize, Clone)]
pub struct ActionView {
    /// Human label for the button (e.g. "Search memory: Alice").
    pub label: String,
    /// "L1" | "L2" | "L3" — the permission level the UI gates on.
    pub level: String,
    /// The supporting line (the candidate's rationale — already confidence-gated).
    pub rationale: String,
}

#[cfg(target_os = "macos")]
pub mod mac {
    use shogun_core::daemon::Db;
    use shogun_fusion::assemble::{ActionCandidate, ScreenContext};
    use shogun_fusion::{Action, Level, LocalAction, SendAction};

    use super::ActionView;
    use crate::axcache::focused_window;
    use crate::display::frontmost_app;

    fn level_str(level: Level) -> &'static str {
        match level {
            Level::L1 => "L1",
            Level::L2 => "L2",
            Level::L3 => "L3",
        }
    }

    /// A short human label for an action (no user capture text — these come from state summaries,
    /// which are already confidence-gated).
    fn label_of(action: &Action) -> String {
        match action {
            Action::Local(LocalAction::OpenApp { bundle_id }) => format!("Open {bundle_id}"),
            Action::Local(LocalAction::RevealFile { path }) => format!("Reveal {path}"),
            Action::Local(LocalAction::LocalSearch { query }) => format!("Search memory: {query}"),
            Action::Local(LocalAction::ShowNotification { text }) => format!("Remind: {}", clip(text)),
            Action::Local(LocalAction::CopyToClipboard { .. }) => "Copy draft".to_string(),
            Action::Local(LocalAction::UpdateState { table, .. }) => format!("Update {table}"),
            Action::Local(LocalAction::SaveDraft { target }) => format!("Draft {target}"),
            // B-5: the DraftReply candidate. Drafts only — the send still needs the L3 confirm.
            Action::Send(SendAction::SendEmail { .. }) => "Draft reply (confirm)".to_string(),
            Action::Send(_) => "Send (confirm)".to_string(),
        }
    }

    fn clip(s: &str) -> String {
        let s = s.trim();
        if s.chars().count() <= 40 {
            s.to_string()
        } else {
            let mut out: String = s.chars().take(40).collect();
            out.push('…');
            out
        }
    }

    fn view_of(c: &ActionCandidate) -> ActionView {
        ActionView { label: label_of(&c.action), level: level_str(c.level).to_string(), rationale: c.rationale.clone() }
    }

    /// Tauri command: assemble the context actions for the current focus and return the button
    /// views for the panel. Reads the frontmost app + focused-window title as the screen context.
    #[tauri::command]
    pub fn notch_actions(db: tauri::State<'_, Db>) -> Vec<ActionView> {
        let (bundle_id, title) = match frontmost_app() {
            Some(front) => {
                let title = focused_window(front.pid).and_then(|w| w.title()).unwrap_or_default();
                (front.bundle_id, title)
            }
            None => (String::new(), String::new()),
        };
        let screen = ScreenContext { app_bundle_id: bundle_id, window_title: title, salient: Vec::new() };
        db.context_actions(screen, None).actions.iter().map(view_of).collect()
    }
}
