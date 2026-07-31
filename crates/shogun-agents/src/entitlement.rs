//! Plan entitlement (issue #97) — the Rust-core source of truth for what the current plan may do.
//!
//! CLAUDE.md プラン構成 (2026-07-30 boundary):
//! - No Free plan. 7-day full trial (Pro-equivalent) → Standard / Pro.
//! - **Standard**: capture + memory + search + Notch UI + first-layer reads (**including Gmail
//!   read via Composio**) + Dream Cycle + Morning Brief. Select KK key only.
//! - **Pro**: + agent execution (L1/L2/L3) + Memory API (MCP/CLI/REST) + the Composio second-layer
//!   gate, which means **send unlock only** (real sending with draft-stop OFF). Reads are never
//!   Pro-gated.
//! - Meeting notes are available on every plan (only the Memory-API view, FR-MT-22, rides the
//!   Memory API gate). The Composio 3-disclosure read consent is orthogonal to plans.
//!
//! This module is **pure**: `now_ms` is always a parameter (repo convention — pure logic never
//! reads the clock), so the day-7 boundary is deterministic under test. The desktop layer owns
//! the inputs (onboarding.json's `trial_started_at` + the future Stripe billing state, #8) and
//! feeds them in through [`resolve_plan`]; every enforcement point takes the resulting
//! [`Entitlements`] value. Plan gating lives here in the Rust core — webview gating is display
//! only (CLAUDE.md: プラン判定はRustコア側で行う).
//!
//! ## Expired-trial posture (proposal, 要オーナー確認)
//!
//! CLAUDE.md says trial後は全員課金 (no Free fallback), so an expired trial with no paid plan
//! locks Standard features too. The chosen posture is the least destructive honest one:
//! - **Keeps working**: local capture and the local read-only search/memory view. Memory is
//!   year-scale data; silently stopping capture would punch a hole in it that a later purchase
//!   cannot backfill, and it costs nothing cloud-side (local ONNX embeddings).
//! - **Locked**: agent execution, the Memory API, every send path, first-layer reads/sync,
//!   Dream Cycle / Morning Brief (they spend the Select KK key).
//! - The UI surfaces a "trial ended" state from [`Entitlements::status`].
//!
//! Recorded in docs/fixes/2026-07-31-entitlement-enforcement-design.md; the owner may still
//! decide to lock capture as well.

/// Length of the full trial: 7 days, in milliseconds.
pub const TRIAL_DURATION_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// The plan the device is currently on, as resolved from billing + the trial stamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// Trialing (Pro-equivalent). `started_at_ms` is the onboarding-completion stamp
    /// (`trial_started_at` in onboarding.json, converted to unix ms); `None` means onboarding has
    /// not completed yet — the clock starts at completion, so a pre-onboarding device is treated
    /// as an active trial (documented default: nothing known = trial-not-started).
    Trial { started_at_ms: Option<u64> },
    /// Paid Standard.
    Standard,
    /// Paid Pro.
    Pro,
}

/// A paid plan (what a billing record can grant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaidPlan {
    Standard,
    Pro,
}

/// What the billing system knows about this device's subscription. **Stub until Stripe (#8)**:
/// today every provider returns [`BillingState::Unknown`], so everyone resolves to the trial
/// rules. Stripe #8 implements a real source for this value; nothing else changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BillingState {
    /// No billing record known (the pre-Stripe stub, and any device that never purchased).
    #[default]
    Unknown,
    /// An active paid subscription.
    Active(PaidPlan),
    /// A subscription that existed but lapsed (cancelled / payment failed). Treated like
    /// `Unknown`: back to the trial rules, which for a stamped 7-day-old trial means expired.
    Lapsed,
}

/// Resolve the effective plan from the trial stamp and the billing state. Billing wins: an active
/// subscription overrides the trial clock entirely (a paying user is never "expired").
pub fn resolve_plan(trial_started_at_ms: Option<u64>, billing: BillingState) -> Plan {
    match billing {
        BillingState::Active(PaidPlan::Pro) => Plan::Pro,
        BillingState::Active(PaidPlan::Standard) => Plan::Standard,
        BillingState::Unknown | BillingState::Lapsed => {
            Plan::Trial { started_at_ms: trial_started_at_ms }
        }
    }
}

/// The user-facing plan status, for the "trial ended" surface and settings display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStatus {
    /// Trial running (or not yet started — full access either way).
    Trial,
    /// Trial over, no paid plan: the locked posture (see module docs).
    TrialExpired,
    Standard,
    Pro,
}

/// What the current plan may do. One value, computed once per decision by [`entitlements`], passed
/// into every gate — the gates never re-derive plan logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entitlements {
    pub status: PlanStatus,
    /// Local capture into the on-device memory. True on every status including expired (least
    /// destructive posture: never punch holes in year-scale memory — see module docs).
    pub local_capture: bool,
    /// The local search / memory read-only view. True on every status including expired.
    pub local_search: bool,
    /// First-layer service reads (Gmail read via Composio included — the 2026-07-30 decision) and
    /// the read-sync that feeds them. Standard and up; locked when expired.
    pub first_layer_reads: bool,
    /// Dream Cycle + Morning Brief (Select KK key lane). Standard and up; locked when expired.
    pub background_intelligence: bool,
    /// Agent execution (L1/L2/L3 engine + first-layer writes). Pro / Trial only.
    pub agent_execution: bool,
    /// The Memory API, all three faces (MCP / CLI / REST). Pro / Trial only.
    pub memory_api: bool,
    /// The Composio second-layer **send unlock** — the only thing the Pro "Composio第2層" gate
    /// means (real sending with draft-stop OFF). Pro / Trial only. Composes with (never replaces)
    /// the 3-disclosure consent and draft-stop gates, and the send itself is still L3.
    pub composio_send_unlock: bool,
}

impl Entitlements {
    /// The documented default when nothing is known yet (no onboarding completion, no billing):
    /// an active, not-yet-started trial — full access; the 7-day clock starts at onboarding
    /// completion.
    pub fn trial_not_started() -> Self {
        entitlements(Plan::Trial { started_at_ms: None }, 0)
    }

    fn full(status: PlanStatus) -> Self {
        Self {
            status,
            local_capture: true,
            local_search: true,
            first_layer_reads: true,
            background_intelligence: true,
            agent_execution: true,
            memory_api: true,
            composio_send_unlock: true,
        }
    }
}

/// Whether a stamped trial is expired at `now_ms`: the trial covers exactly
/// [`TRIAL_DURATION_MS`]; the first millisecond of day 8 (`started + 7d`) is expired.
pub fn trial_expired(started_at_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(started_at_ms) >= TRIAL_DURATION_MS
}

/// The pure entitlement function: plan + injected clock → what is allowed. Never reads the clock.
pub fn entitlements(plan: Plan, now_ms: u64) -> Entitlements {
    match plan {
        Plan::Pro => Entitlements::full(PlanStatus::Pro),
        Plan::Standard => Entitlements {
            agent_execution: false,
            memory_api: false,
            composio_send_unlock: false,
            ..Entitlements::full(PlanStatus::Standard)
        },
        Plan::Trial { started_at_ms } => match started_at_ms {
            // Trial running (or clock not started yet): Pro-equivalent.
            Some(start) if trial_expired(start, now_ms) => Entitlements {
                status: PlanStatus::TrialExpired,
                local_capture: true,
                local_search: true,
                first_layer_reads: false,
                background_intelligence: false,
                agent_execution: false,
                memory_api: false,
                composio_send_unlock: false,
            },
            _ => Entitlements::full(PlanStatus::Trial),
        },
    }
}

/// The provider seam the effectful layers implement: the desktop app reads onboarding.json + the
/// (stubbed) billing state; the standalone API/MCP binaries read the shared file. Gates never call
/// this directly — the caller resolves once per request/decision and passes the value in.
pub trait EntitlementSource: Send + Sync {
    /// The entitlements in force at `now_ms`.
    fn entitlements(&self, now_ms: u64) -> Entitlements;
}

/// A fixed-plan source (tests, and the pre-Stripe default of "no billing record").
#[derive(Debug, Clone, Copy)]
pub struct StaticPlan(pub Plan);

impl EntitlementSource for StaticPlan {
    fn entitlements(&self, now_ms: u64) -> Entitlements {
        entitlements(self.0, now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: u64 = 1_000_000;

    fn at(plan: Plan, now_ms: u64) -> Entitlements {
        entitlements(plan, now_ms)
    }

    #[test]
    fn trial_is_pro_equivalent_while_running() {
        let e = at(Plan::Trial { started_at_ms: Some(START) }, START + 1);
        assert_eq!(e.status, PlanStatus::Trial);
        assert_eq!(e, Entitlements::full(PlanStatus::Trial));
        assert!(e.agent_execution && e.memory_api && e.composio_send_unlock);
    }

    #[test]
    fn day_7_boundary_last_ms_in_first_ms_out() {
        let plan = Plan::Trial { started_at_ms: Some(START) };
        // one millisecond before the boundary: still full trial
        let last = at(plan, START + TRIAL_DURATION_MS - 1);
        assert_eq!(last.status, PlanStatus::Trial);
        assert!(last.agent_execution);
        // exactly started + 7d: expired
        let first_out = at(plan, START + TRIAL_DURATION_MS);
        assert_eq!(first_out.status, PlanStatus::TrialExpired);
        assert!(!first_out.agent_execution);
    }

    #[test]
    fn unstamped_trial_is_active_and_is_the_default() {
        // The stamp is written at onboarding completion; before that the device runs as trial.
        let e = at(Plan::Trial { started_at_ms: None }, u64::MAX);
        assert_eq!(e.status, PlanStatus::Trial);
        assert_eq!(e, Entitlements::trial_not_started());
        assert!(e.memory_api);
    }

    #[test]
    fn expired_posture_local_only() {
        let e = at(Plan::Trial { started_at_ms: Some(START) }, START + TRIAL_DURATION_MS + 5);
        assert_eq!(e.status, PlanStatus::TrialExpired);
        // keeps working: never punch holes in year-scale memory
        assert!(e.local_capture);
        assert!(e.local_search);
        // locked: everything that spends a key or leaves the device
        assert!(!e.first_layer_reads);
        assert!(!e.background_intelligence);
        assert!(!e.agent_execution);
        assert!(!e.memory_api);
        assert!(!e.composio_send_unlock);
    }

    #[test]
    fn allow_deny_matrix_per_plan() {
        // (plan, reads, background, agent_exec, memory_api, composio_send)
        let cases: &[(Plan, u64, [bool; 5])] = &[
            (Plan::Trial { started_at_ms: Some(START) }, START, [true; 5]),
            (Plan::Trial { started_at_ms: None }, START, [true; 5]),
            (Plan::Standard, START, [true, true, false, false, false]),
            (Plan::Pro, START, [true; 5]),
            (
                Plan::Trial { started_at_ms: Some(START) },
                START + TRIAL_DURATION_MS,
                [false; 5],
            ),
        ];
        for (plan, now, [reads, bg, exec, api, send]) in cases {
            let e = at(*plan, *now);
            assert_eq!(e.first_layer_reads, *reads, "{plan:?} reads");
            assert_eq!(e.background_intelligence, *bg, "{plan:?} background");
            assert_eq!(e.agent_execution, *exec, "{plan:?} agent execution");
            assert_eq!(e.memory_api, *api, "{plan:?} memory api");
            assert_eq!(e.composio_send_unlock, *send, "{plan:?} composio send");
            // local capture/search survive every status (least destructive posture)
            assert!(e.local_capture && e.local_search, "{plan:?} local");
        }
    }

    #[test]
    fn standard_never_gets_pro_features_but_keeps_reads() {
        let e = at(Plan::Standard, u64::MAX);
        assert_eq!(e.status, PlanStatus::Standard);
        // Gmail read via Composio is a first-layer read → Standard (2026-07-30 decision)
        assert!(e.first_layer_reads);
        assert!(!e.agent_execution && !e.memory_api && !e.composio_send_unlock);
    }

    #[test]
    fn billing_overrides_an_expired_trial() {
        let stamp = Some(START);
        let expired_now = START + TRIAL_DURATION_MS + 1;
        // no billing → expired
        assert_eq!(
            at(resolve_plan(stamp, BillingState::Unknown), expired_now).status,
            PlanStatus::TrialExpired
        );
        // an active subscription wins over the trial clock
        assert_eq!(
            resolve_plan(stamp, BillingState::Active(PaidPlan::Pro)),
            Plan::Pro
        );
        assert_eq!(
            resolve_plan(stamp, BillingState::Active(PaidPlan::Standard)),
            Plan::Standard
        );
        // a lapsed subscription falls back to the trial rules (→ expired here)
        assert_eq!(
            at(resolve_plan(stamp, BillingState::Lapsed), expired_now).status,
            PlanStatus::TrialExpired
        );
    }

    #[test]
    fn static_source_is_deterministic() {
        let src = StaticPlan(Plan::Trial { started_at_ms: Some(START) });
        assert_eq!(src.entitlements(START).status, PlanStatus::Trial);
        assert_eq!(src.entitlements(START + TRIAL_DURATION_MS).status, PlanStatus::TrialExpired);
    }
}
