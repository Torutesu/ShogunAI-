//! Desktop-side plan provider (issue #97) — the seam between the pure entitlement logic
//! (`shogun_agents::entitlement`) and this device's plan inputs:
//!
//! - **trial stamp**: `onboarding.json`'s `trial_started_at` (unix seconds, stamped once at
//!   onboarding completion — see `crate::onboarding`). No stamp = trial-not-started = full access
//!   (the documented default; the 7-day clock starts at completion).
//! - **billing**: this device's verified licence token (issue #8 — `crate::billing`). A valid
//!   token is `BillingState::Active(plan)`; no licence, an expired grace window or a token this
//!   build cannot verify is `Unknown`/`Lapsed`, which falls back to the trial rules.
//!
//! Every gate takes the resolved [`Entitlements`] value from here — plan decisions live in the
//! Rust core; the webview only *displays* the state (`entitlement_status` below). The onboarding
//! `plan` field is an intent only and grants nothing.
#![allow(dead_code)]

#[cfg(target_os = "macos")]
pub mod mac {
    use shogun_agents::entitlement::{entitlements, resolve_plan, Entitlements, PlanStatus};
    use tauri::AppHandle;

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }

    /// The trial stamp for this device in unix ms, from the Rust-owned onboarding state (managed
    /// copy when available, disk otherwise).
    fn trial_started_at_ms(app: &AppHandle) -> Option<u64> {
        let state = crate::onboarding::mac::onboarding_state(app.clone());
        let secs = u64::try_from(state.trial_started_at?).ok()?;
        secs.checked_mul(1000)
    }

    /// Resolve the entitlements in force right now. Called at each enforcement point (cheap: one
    /// in-memory state read + pure math), so trial expiry takes effect without a restart.
    pub fn current(app: &AppHandle) -> Entitlements {
        // The cached licence token — a local file read plus a signature check, no network (#8).
        let billing = crate::billing::mac::state(app);
        let plan = resolve_plan(trial_started_at_ms(app), billing);
        entitlements(plan, now_ms())
    }

    /// Display-only view for the webview (settings / "trial ended" surface). The webview renders
    /// this; it never re-derives or overrides it (CLAUDE.md: plan gating is core-side).
    #[derive(serde::Serialize)]
    pub struct EntitlementView {
        /// "trial" | "trial_expired" | "standard" | "pro"
        pub status: &'static str,
        pub agent_execution: bool,
        pub memory_api: bool,
        pub composio_send_unlock: bool,
        pub first_layer_reads: bool,
        /// Unix seconds of the trial stamp, if stamped (for the countdown display).
        pub trial_started_at: Option<i64>,
    }

    /// Current plan state for the UI.
    #[tauri::command]
    pub fn entitlement_status(app: AppHandle) -> EntitlementView {
        let e = current(&app);
        let status = match e.status {
            PlanStatus::Trial => "trial",
            PlanStatus::TrialExpired => "trial_expired",
            PlanStatus::Standard => "standard",
            PlanStatus::Pro => "pro",
        };
        EntitlementView {
            status,
            agent_execution: e.agent_execution,
            memory_api: e.memory_api,
            composio_send_unlock: e.composio_send_unlock,
            first_layer_reads: e.first_layer_reads,
            trial_started_at: crate::onboarding::mac::onboarding_state(app).trial_started_at,
        }
    }
}
