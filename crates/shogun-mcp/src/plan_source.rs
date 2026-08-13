//! Plan resolution for the **standalone** Memory API faces (issue #97) — the `shogun-api` (REST)
//! and `shogun-mcp` (stdio) binaries, which run outside the desktop app and therefore cannot ask
//! the Tauri layer for the plan.
//!
//! Two inputs, both files the desktop app owns and this module re-reads on every resolution:
//!
//! - **trial stamp** — `onboarding.json` (Rust-written, versioned; see
//!   `apps/desktop/src-tauri/src/onboarding.rs`) carries `state.trial_started_at` (unix
//!   **seconds**, stamped once at onboarding completion). Re-reading means a trial expiring while
//!   a server runs locks the next request.
//! - **billing** — `billing.json` (issue #8) carries this device's signed licence token. The
//!   token is verified here, on every read, against the licence API's public key.
//!
//! ## Why the signed token sits in a file and the licence key does not
//!
//! NFR-SEC-01 keeps *secrets* in the Keychain, and the licence **key** (the bearer that talks to
//! the licence API) obeys that without exception — it is never written to disk. The **token** is a
//! different kind of object: a public-key-signed, device-bound, expiring assertion. Copying it
//! elsewhere grants nothing (verification checks the device id), editing it invalidates the
//! signature, and it ages out on its own. Mirroring it into a file is what lets the CLI, the MCP
//! server and the REST face see a paid plan at all — they cannot read the app's Keychain items,
//! and a Pro subscriber whose `shogun` CLI locked itself out would be a worse outcome by far.
//! Recorded in docs/fixes/2026-08-10-stripe-billing-flow-design.md §5.
//!
//! Path resolution: `SHOGUN_ONBOARDING_JSON` / `SHOGUN_BILLING_JSON` env overrides first (dev /
//! tests), else the macOS app-data location of the desktop app. When no file is found the answer
//! is the documented default — trial-not-started (full access; the 7-day clock starts at
//! onboarding completion) with no billing record.

use std::path::PathBuf;

use shogun_agents::entitlement::{
    entitlements, resolve_plan, BillingState, EntitlementSource, Entitlements,
};
use shogun_license::{billing_state_from_token, public_key};

/// The desktop app's bundle identifier — its app-data directory holds `onboarding.json` and
/// `billing.json`. Kept in lockstep with `apps/desktop/src-tauri/tauri.conf.json`.
const DESKTOP_IDENTIFIER: &str = "com.syogun.shogunai";

/// File name of the billing snapshot the desktop app writes after each successful verification.
pub const BILLING_FILE: &str = "billing.json";

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

/// The device-side billing snapshot (`billing.json`). Written by the desktop app after a
/// successful licence verification; read by every plan source.
///
/// Only `device_id` and `token` matter — the token is signed and carries the plan. Nothing else
/// in this file is trusted for a gating decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillingSnapshot {
    pub device_id: String,
    pub token: String,
    /// Unix seconds of the last successful verification (display / diagnostics only).
    pub verified_at: Option<i64>,
}

/// Parse `billing.json`. Anything unparseable is `None` = no billing record = trial rules.
pub fn parse_billing_snapshot(text: &str) -> Option<BillingSnapshot> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if v.get("version")?.as_u64()? < 1 {
        return None;
    }
    let device_id = v.get("device_id")?.as_str()?.trim().to_string();
    let token = v.get("token")?.as_str()?.trim().to_string();
    if device_id.is_empty() || token.is_empty() {
        return None;
    }
    Some(BillingSnapshot {
        device_id,
        token,
        verified_at: v.get("verified_at").and_then(serde_json::Value::as_i64),
    })
}

/// Serialise a snapshot for the desktop writer. Kept here so the reader and the writer can never
/// drift apart on the field names.
pub fn serialize_billing_snapshot(snap: &BillingSnapshot) -> String {
    serde_json::json!({
        "version": 1,
        "device_id": snap.device_id,
        "token": snap.token,
        "verified_at": snap.verified_at,
    })
    .to_string()
}

/// The billing state a snapshot asserts at `now_ms`: signature + device binding + grace window.
/// Any failure — no public key in this build, a tampered token, another Mac's token — is
/// [`BillingState::Unknown`], which falls back to the trial rules rather than granting anything.
pub fn billing_state_of(snap: &BillingSnapshot, now_ms: u64) -> BillingState {
    match public_key() {
        Some(key) => billing_state_from_token(&snap.token, &key, &snap.device_id, now_ms),
        None => BillingState::Unknown,
    }
}

fn app_data_file(env_override: &str, file: &str) -> Option<PathBuf> {
    // Debug builds only: a release binary that lets an env var re-point its billing/onboarding
    // state is a plan-gate bypass primitive (pair it with a swapped public key and the gate is
    // gone). Dev and tests keep the override.
    #[cfg(debug_assertions)]
    if let Ok(p) = std::env::var(env_override) {
        if !p.trim().is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = env_override;
    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        return Some(
            PathBuf::from(home)
                .join("Library/Application Support")
                .join(DESKTOP_IDENTIFIER)
                .join(file),
        );
    }
    None
}

/// Where the desktop app's `onboarding.json` lives for this process: the `SHOGUN_ONBOARDING_JSON`
/// env override, else the macOS app-data path. `None` when neither resolves (e.g. Linux dev with
/// no override).
pub fn onboarding_json_path() -> Option<PathBuf> {
    app_data_file("SHOGUN_ONBOARDING_JSON", "onboarding.json")
}

/// Where `billing.json` lives: the `SHOGUN_BILLING_JSON` env override, else the macOS app-data
/// path next to `onboarding.json`.
pub fn billing_json_path() -> Option<PathBuf> {
    app_data_file("SHOGUN_BILLING_JSON", BILLING_FILE)
}

/// A plan source backed by the shared `onboarding.json` + `billing.json` files. Re-reads both on
/// every call — the stamp is written once by the desktop app, and re-reading keeps a long-running
/// server honest about trial expiry, a fresh purchase and a lapsed subscription alike.
#[derive(Debug, Clone)]
pub struct FilePlanSource {
    path: Option<PathBuf>,
    billing_path: Option<PathBuf>,
}

impl FilePlanSource {
    /// Resolve both paths from the environment (see [`onboarding_json_path`]).
    pub fn from_env() -> Self {
        Self { path: onboarding_json_path(), billing_path: billing_json_path() }
    }

    /// A source pinned to a specific onboarding file (tests / explicit config). Billing still
    /// resolves from the environment.
    pub fn at(path: PathBuf) -> Self {
        Self { path: Some(path), billing_path: billing_json_path() }
    }

    /// Pin the billing snapshot path too (tests / explicit config).
    pub fn with_billing(mut self, path: PathBuf) -> Self {
        self.billing_path = Some(path);
        self
    }

    fn trial_started_at_ms(&self) -> Option<u64> {
        let path = self.path.as_ref()?;
        let text = std::fs::read_to_string(path).ok()?;
        parse_trial_started_at_ms(&text)
    }

    fn billing(&self, now_ms: u64) -> BillingState {
        let Some(path) = self.billing_path.as_ref() else {
            return BillingState::Unknown;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return BillingState::Unknown;
        };
        match parse_billing_snapshot(&text) {
            Some(snap) => billing_state_of(&snap, now_ms),
            None => BillingState::Unknown,
        }
    }

    /// Resolve the entitlements in force at `now_ms` (inherent form of [`EntitlementSource`], so
    /// binaries don't need the trait in scope).
    pub fn resolve(&self, now_ms: u64) -> Entitlements {
        let plan = resolve_plan(self.trial_started_at_ms(), self.billing(now_ms));
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
    use shogun_agents::entitlement::{PaidPlan, PlanStatus, TRIAL_DURATION_MS};

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
        let src = FilePlanSource::at(path).with_billing(dir.join("no-such-billing.json"));
        let start_ms = 100 * 1000;
        assert_eq!(src.entitlements(start_ms + 1).status, PlanStatus::Trial);
        assert_eq!(src.entitlements(start_ms + TRIAL_DURATION_MS).status, PlanStatus::TrialExpired);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn absent_file_is_the_documented_default_trial_not_started() {
        let src = FilePlanSource::at(PathBuf::from("/nonexistent/onboarding.json"))
            .with_billing(PathBuf::from("/nonexistent/billing.json"));
        let e = src.entitlements(u64::MAX);
        assert_eq!(e.status, PlanStatus::Trial);
        assert!(e.memory_api);
    }

    #[test]
    fn billing_snapshot_round_trips() {
        let snap = BillingSnapshot {
            device_id: "dev-1".into(),
            token: "v1.aaa.bbb".into(),
            verified_at: Some(1_754_800_000),
        };
        let text = serialize_billing_snapshot(&snap);
        assert_eq!(parse_billing_snapshot(&text), Some(snap));
    }

    #[test]
    fn garbage_billing_files_are_no_billing_record() {
        for text in [
            "",
            "not json",
            r#"{"version":0,"device_id":"d","token":"t"}"#,
            r#"{"version":1,"device_id":"","token":"t"}"#,
            r#"{"version":1,"device_id":"d"}"#,
        ] {
            assert_eq!(parse_billing_snapshot(text), None, "{text:?}");
        }
    }

    #[test]
    fn an_unverifiable_token_never_grants_a_plan() {
        // No public key configured in a test build (and a garbage token anyway) → Unknown, which
        // falls back to the trial rules rather than to access.
        let snap = BillingSnapshot {
            device_id: "dev-1".into(),
            token: "v1.not-a-real-token.sig".into(),
            verified_at: None,
        };
        assert_eq!(billing_state_of(&snap, 0), BillingState::Unknown);
    }

    /// A stamped, long-expired trial + a valid Pro token = Pro. This is the whole point of #8:
    /// billing wins over the trial clock (`resolve_plan`), and the token is what carries it.
    #[test]
    fn a_valid_token_beats_an_expired_trial() {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine as _;
        use ed25519_dalek::{Signer, SigningKey};

        let signing = SigningKey::from_bytes(&[3u8; 32]);
        let body = serde_json::json!({
            "v": 1, "lic": "lic-1", "plan": "pro", "status": "active", "device": "dev-1",
            "iat": 0, "exp": 3600, "period_end": 1_800_000_000i64,
            "cancel_at_period_end": false, "grace_days": 14,
        })
        .to_string();
        let token = format!(
            "v1.{}.{}",
            URL_SAFE_NO_PAD.encode(body.as_bytes()),
            URL_SAFE_NO_PAD.encode(signing.sign(body.as_bytes()).to_bytes())
        );
        let snap = BillingSnapshot { device_id: "dev-1".into(), token, verified_at: None };

        let key = signing.verifying_key().to_bytes();
        let state = shogun_license::billing_state_from_token(&snap.token, &key, &snap.device_id, 0);
        assert_eq!(state, BillingState::Active(PaidPlan::Pro));

        // …and that state overrides a trial stamped 30 days ago.
        let stamp = Some(0);
        let now = 30 * 24 * 3600 * 1000;
        let plan = resolve_plan(stamp, state);
        assert_eq!(entitlements(plan, now).status, PlanStatus::Pro);
    }
}
