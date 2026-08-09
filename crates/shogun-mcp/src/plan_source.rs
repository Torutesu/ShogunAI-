//! Plan resolution for the **standalone** Memory API faces (issue #97) — the `shogun-api` (REST)
//! and `shogun-mcp` (stdio) binaries, which run outside the desktop app and therefore cannot ask
//! the Tauri layer for the plan.
//!
//! The desktop app owns the plan inputs: `onboarding.json` (Rust-written, versioned — see
//! `apps/desktop/src-tauri/src/onboarding.rs`) carries `state.trial_started_at` (unix **seconds**,
//! stamped once at onboarding completion). This module re-reads that file on every resolution so
//! a trial expiring while a server runs locks the next request; billing is the pre-Stripe stub
//! ([`BillingState::Unknown`], #8 replaces it).
//!
//! Path resolution: `SHOGUN_ONBOARDING_JSON` env override first (dev / tests), else the macOS
//! app-data location of the desktop app. When no file is found the answer is the documented
//! default — trial-not-started (full access; the 7-day clock starts at onboarding completion).

use std::path::PathBuf;

use shogun_agents::entitlement::{
    entitlements, resolve_plan, BillingState, EntitlementSource, Entitlements,
};

/// The desktop app's bundle identifier — its app-data directory holds `onboarding.json`. Kept in
/// lockstep with `apps/desktop/src-tauri/tauri.conf.json`.
const DESKTOP_IDENTIFIER: &str = "com.syogun.shogunai";

/// Parse `onboarding.json` text into the trial stamp in unix **ms**, tolerating anything: only a
/// versioned (v1+) file with a numeric `state.trial_started_at` (unix seconds) yields a stamp.
/// Everything else — legacy #46 files, garbage, missing fields — is `None` (trial-not-started).
pub fn parse_trial_started_at_ms(text: &str) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if v.get("version")?.as_u64()? < 1 {
        return None;
    }
    let secs = v.get("state")?.get("trial_started_at")?.as_i64()?;
    u64::try_from(secs).ok()?.checked_mul(1000)
}

/// Where the desktop app's `onboarding.json` lives for this process: the `SHOGUN_ONBOARDING_JSON`
/// env override, else the macOS app-data path. `None` when neither resolves (e.g. Linux dev with
/// no override).
pub fn onboarding_json_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SHOGUN_ONBOARDING_JSON") {
        if !p.trim().is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        return Some(
            PathBuf::from(home)
                .join("Library/Application Support")
                .join(DESKTOP_IDENTIFIER)
                .join("onboarding.json"),
        );
    }
    None
}

/// A plan source backed by the shared `onboarding.json` file (+ the billing stub). Re-reads the
/// file on every call — the stamp is written once by the desktop app, and re-reading keeps a
/// long-running server honest about trial expiry.
#[derive(Debug, Clone)]
pub struct FilePlanSource {
    path: Option<PathBuf>,
}

impl FilePlanSource {
    /// Resolve the path from the environment (see [`onboarding_json_path`]).
    pub fn from_env() -> Self {
        Self { path: onboarding_json_path() }
    }

    /// A source pinned to a specific file (tests / explicit config).
    pub fn at(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn trial_started_at_ms(&self) -> Option<u64> {
        let path = self.path.as_ref()?;
        let text = std::fs::read_to_string(path).ok()?;
        parse_trial_started_at_ms(&text)
    }

    /// Resolve the entitlements in force at `now_ms` (inherent form of [`EntitlementSource`], so
    /// binaries don't need the trait in scope).
    pub fn resolve(&self, now_ms: u64) -> Entitlements {
        // Billing stub (#8): no record known. Stripe replaces this line with a real lookup.
        let plan = resolve_plan(self.trial_started_at_ms(), BillingState::Unknown);
        entitlements(plan, now_ms)
    }
}

impl EntitlementSource for FilePlanSource {
    fn entitlements(&self, now_ms: u64) -> Entitlements {
        self.resolve(now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shogun_agents::entitlement::{PlanStatus, TRIAL_DURATION_MS};

    #[test]
    fn parses_the_versioned_desktop_format_seconds_to_ms() {
        let json = r#"{"version":1,"state":{"completed":true,"step":"ready","trial_started_at":1753842000}}"#;
        assert_eq!(parse_trial_started_at_ms(json), Some(1_753_842_000_000));
    }

    #[test]
    fn missing_stamp_legacy_and_garbage_are_trial_not_started() {
        for text in [
            r#"{"version":1,"state":{"completed":false,"step":"welcome"}}"#, // no stamp yet
            r#"{"completed":true}"#,                                          // legacy #46 (version 0)
            "not json",
            "",
            r#"{"version":0,"state":{"trial_started_at":5}}"#, // pre-version files never stamped
        ] {
            assert_eq!(parse_trial_started_at_ms(text), None, "{text:?}");
        }
    }

    #[test]
    fn file_source_expires_the_trial_from_the_stamp() {
        let dir = std::env::temp_dir().join(format!("shogun-plan-src-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("onboarding.json");
        std::fs::write(&path, r#"{"version":1,"state":{"completed":true,"step":"ready","trial_started_at":100}}"#)
            .unwrap();
        let src = FilePlanSource::at(path);
        let start_ms = 100 * 1000;
        assert_eq!(src.entitlements(start_ms + 1).status, PlanStatus::Trial);
        assert_eq!(src.entitlements(start_ms + TRIAL_DURATION_MS).status, PlanStatus::TrialExpired);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn absent_file_is_the_documented_default_trial_not_started() {
        let src = FilePlanSource::at(PathBuf::from("/nonexistent/onboarding.json"));
        let e = src.entitlements(u64::MAX);
        assert_eq!(e.status, PlanStatus::Trial);
        assert!(e.memory_api);
    }
}
