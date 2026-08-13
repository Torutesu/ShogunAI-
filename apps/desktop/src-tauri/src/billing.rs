//! The device side of Stripe billing (issue #8) — activation, verification, and the two links
//! into Stripe's hosted surfaces.
//!
//! Split of responsibilities, and why:
//!
//! - **Licence key** — the bearer for the licence API. Keychain only, never a file, never a log
//!   (NFR-SEC-01 / CLAUDE.md invariant 7).
//! - **Licence token** — the signed, device-bound, expiring plan assertion. Cached in
//!   `billing.json` so the CLI / MCP / REST faces can see a paid plan too (the reasoning is in
//!   `shogun_mcp::plan_source`), and so an offline Mac keeps working for 14 days (FR-BIL-09).
//! - **Card data** — never touched here. Checkout and the Customer Portal are Stripe-hosted and
//!   open in the system browser (FR-BIL-07: "アプリ内にカード情報を扱うUIを作らず").
//!
//! Verification runs at launch and every 24h (FR-BIL-08). A failure that is not "this licence
//! does not exist" leaves the cached token in place: a flaky network must never take a paying
//! user's app away from them.

#![allow(dead_code)]

#[cfg(target_os = "macos")]
pub mod mac {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use shogun_agents::entitlement::BillingState;
    use shogun_core::daemon::Db;
    use shogun_core::license_client;
    use shogun_integrations::keychain_store;
    use shogun_license::{public_key, verify, Freshness};
    use shogun_mcp::plan_source::{
        billing_state_of, parse_billing_snapshot, serialize_billing_snapshot, BillingSnapshot,
    };
    use tauri::{AppHandle, Manager};

    /// Keychain account holding the licence key. The key is the ONLY billing secret on the device.
    const LICENSE_KEY_ACCOUNT: &str = "license-key";

    /// How often the app re-verifies (FR-BIL-08: 起動時 + 24時間ごと).
    pub const VERIFY_INTERVAL_SECS: u64 = 24 * 60 * 60;

    /// Guards the read-modify-write of `billing.json` so a manual refresh racing the 24h timer
    /// cannot interleave two writes.
    static WRITE_LOCK: Mutex<()> = Mutex::new(());

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }

    fn now_secs() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// The licence API origin: `SHOGUN_LICENSE_API` (staging / dev, debug builds only) else
    /// production. A release build that honoured the env could be pointed at an attacker's
    /// "licence API".
    fn api_origin() -> String {
        #[cfg(debug_assertions)]
        if let Some(o) = std::env::var("SHOGUN_LICENSE_API")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return o;
        }
        license_client::DEFAULT_LICENSE_API.to_string()
    }

    fn billing_path(app: &AppHandle) -> Option<PathBuf> {
        // The env override keeps dev/QA on a scratch file, and matches what the standalone
        // binaries read (`shogun_mcp::plan_source::billing_json_path`). Debug builds only —
        // release must read exactly one location.
        #[cfg(debug_assertions)]
        if let Ok(p) = std::env::var("SHOGUN_BILLING_JSON") {
            if !p.trim().is_empty() {
                return Some(PathBuf::from(p));
            }
        }
        app.path().app_data_dir().ok().map(|d| d.join("billing.json"))
    }

    fn read_snapshot(app: &AppHandle) -> Option<BillingSnapshot> {
        let text = std::fs::read_to_string(billing_path(app)?).ok()?;
        parse_billing_snapshot(&text)
    }

    fn write_snapshot(app: &AppHandle, snap: &BillingSnapshot) -> Result<(), String> {
        let path = billing_path(app).ok_or_else(|| "no app data dir".to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let _guard = WRITE_LOCK.lock();
        std::fs::write(&path, serialize_billing_snapshot(snap))
            .map_err(|e| format!("billing state write failed: {e}"))
    }

    fn clear_snapshot(app: &AppHandle) {
        if let Some(path) = billing_path(app) {
            let _guard = WRITE_LOCK.lock();
            let _ = std::fs::remove_file(path);
        }
    }

    /// This device's anonymous id. Reused from the cached snapshot when there is one, so a Mac
    /// keeps a stable identity across re-verifications; minted fresh otherwise. It is a random
    /// UUID with no relationship to hardware, the user, or the analytics distinct id — a billing
    /// call must not be joinable with a product-analytics identity.
    fn device_id(app: &AppHandle) -> Result<String, String> {
        if let Some(snap) = read_snapshot(app) {
            return Ok(snap.device_id);
        }
        crate::analytics::new_distinct_id()
    }

    fn stored_license_key() -> Option<String> {
        let bytes = keychain_store::get_generic_secret(LICENSE_KEY_ACCOUNT).ok()?;
        let s = String::from_utf8(bytes).ok()?;
        let trimmed = s.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    }

    /// The billing state in force right now, from the cached token alone (no network). This is
    /// what `entitlement::mac::current` feeds into `resolve_plan`.
    pub fn state(app: &AppHandle) -> BillingState {
        match read_snapshot(app) {
            Some(snap) => billing_state_of(&snap, now_ms()),
            None => BillingState::Unknown,
        }
    }

    /// Display-only billing view for the settings panel. Plan gating is decided in the Rust core
    /// (CLAUDE.md: プラン判定はRustコア側で行う); this only describes it.
    #[derive(serde::Serialize, Default)]
    pub struct BillingView {
        /// A licence key is stored on this Mac.
        pub activated: bool,
        /// "standard" | "pro" | null — from the signed token, never from the server's display copy.
        pub plan: Option<String>,
        /// Stripe subscription status at the last verification ("active", "past_due", …).
        pub status: Option<String>,
        /// The token is valid (fresh or inside the offline grace window).
        pub valid: bool,
        /// Working on a cached token because the licence API could not be reached.
        pub offline_grace: bool,
        /// Days offline inside the grace window, and whether that has reached the amber threshold.
        pub days_offline: u32,
        pub amber: bool,
        /// Unix seconds: next billing date, and the last successful verification.
        pub current_period_end: Option<i64>,
        pub verified_at: Option<i64>,
        /// The subscription ends at `current_period_end` (cancelled in the portal).
        pub cancel_at_period_end: bool,
        /// Last error, for the settings panel. Never contains the licence key.
        pub error: Option<String>,
    }

    fn view(app: &AppHandle, error: Option<String>) -> BillingView {
        let activated = stored_license_key().is_some();
        let Some(snap) = read_snapshot(app) else {
            return BillingView { activated, error, ..Default::default() };
        };
        let Some(key) = public_key() else {
            return BillingView {
                activated,
                error: error.or_else(|| Some("this build has no licence key configured".into())),
                verified_at: snap.verified_at,
                ..Default::default()
            };
        };
        match verify(&snap.token, &key, &snap.device_id) {
            Ok(token) => {
                let freshness = token.freshness(now_ms());
                let days_offline = match freshness {
                    Freshness::Grace { days_offline } => u32::try_from(days_offline).unwrap_or(u32::MAX),
                    _ => 0,
                };
                BillingView {
                    activated,
                    plan: Some(
                        match token.plan {
                            shogun_agents::entitlement::PaidPlan::Pro => "pro",
                            shogun_agents::entitlement::PaidPlan::Standard => "standard",
                        }
                        .to_string(),
                    ),
                    status: Some(token.status.clone()),
                    valid: freshness.is_valid(),
                    offline_grace: matches!(freshness, Freshness::Grace { .. }),
                    days_offline,
                    amber: freshness.is_amber(),
                    current_period_end: token.period_end,
                    verified_at: snap.verified_at,
                    cancel_at_period_end: token.cancel_at_period_end,
                    error,
                }
            }
            // A cached token we cannot verify is worth nothing, but it is also not the user's
            // fault — say so plainly instead of silently showing "no plan".
            Err(e) => BillingView {
                activated,
                verified_at: snap.verified_at,
                error: error.or_else(|| Some(format!("licence token rejected ({})", e.as_str()))),
                ..Default::default()
            },
        }
    }

    /// The result of one call to the licence API. Deliberately a value rather than a set of side
    /// effects: activation and re-verification want the *same* question answered and very
    /// different things done about it, and folding the writes in here is how a mistyped key ends
    /// up deleting a licence that was working a second ago.
    enum Outcome {
        /// Entitled, and the returned token verified against this device.
        Entitled(BillingSnapshot),
        /// The server answered authoritatively that this licence is not entitled (cancelled,
        /// unpaid). Carries the status for the message.
        NotEntitled(String),
        /// The server does not know this licence, or it was revoked. Terminal.
        Gone(String),
        /// We could not ask (offline, 5xx, rate limited) — say nothing about entitlement.
        Transient(String),
    }

    impl Outcome {
        fn message(&self) -> String {
            match self {
                Self::Entitled(_) => String::new(),
                Self::NotEntitled(status) => format!("subscription is {status}"),
                Self::Gone(m) | Self::Transient(m) => m.clone(),
            }
        }
    }

    /// Record one licence-API call in the egress ledger (invariant 3: the traceability screen
    /// claims to show every outbound connection, and billing traffic was the gap).
    ///
    /// Content-free by construction — the digest is over the endpoint name, because the request
    /// carries no capture or memory content at all (FR-BIL-08). Best effort on purpose: this
    /// runs at launch and on a 24h timer, sometimes before the DB is open, and a missing ledger
    /// must not be able to delicense a paying user. That is the opposite of the ASR rule, where
    /// the payload IS user content and an unrecorded send is the thing to prevent.
    fn record_billing_egress(app: &AppHandle, endpoint: &str) {
        use shogun_core::llm::traceability::{Route, TraceRecord, TraceabilitySink};
        let Some(db) = app.try_state::<Db>() else { return };
        let origin = api_origin();
        let host = origin
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .to_string();
        // Empty chunk: the request genuinely carries no capture or memory content, so there is
        // nothing to digest — the row exists to show that the connection happened, and to whom.
        db.traceability_sink().record(TraceRecord::for_chunk(
            Route::Billing,
            endpoint,
            host,
            "",
            false,
        ));
    }

    /// Ask the licence API about `license_key`, and verify any token it returns before believing
    /// it — a token for a different device, or one this build cannot check, is not an answer.
    fn check(app: &AppHandle, license_key: &str) -> Result<Outcome, String> {
        let device = device_id(app)?;
        let version = app.package_info().version.to_string();

        record_billing_egress(app, "license_verify");
        match license_client::verify(&api_origin(), license_key, &device, &version) {
            Ok(resp) => match resp.token {
                Some(token) => {
                    // No key = no verification = no entitlement. A build without the public key
                    // must fail closed, not store whatever the server (or whoever the request
                    // was pointed at) returned as an activated licence.
                    let Some(key) = public_key() else {
                        return Ok(Outcome::Transient(
                            "no licence public key in this build (cannot verify the token)"
                                .to_string(),
                        ));
                    };
                    if let Err(e) = verify(&token, &key, &device) {
                        return Ok(Outcome::Transient(format!(
                            "licence token rejected ({})",
                            e.as_str()
                        )));
                    }
                    Ok(Outcome::Entitled(BillingSnapshot {
                        device_id: device,
                        token,
                        verified_at: Some(now_secs()),
                    }))
                }
                None => Ok(Outcome::NotEntitled(resp.status)),
            },
            Err(e) if e.is_terminal() => Ok(Outcome::Gone(e.message())),
            Err(e) => Ok(Outcome::Transient(e.message())),
        }
    }

    /// Re-verify the licence already on this Mac and apply the result.
    ///
    /// Failure policy (FR-BIL-09): a transient failure changes **nothing** — the cached token
    /// keeps working through its 14-day grace window, because an outage is not a cancellation.
    /// Only an authoritative answer (not entitled / licence gone) clears the cache, and only
    /// "gone" also removes the key.
    fn reverify(app: &AppHandle, license_key: &str) -> Result<(), String> {
        match check(app, license_key)? {
            Outcome::Entitled(snap) => write_snapshot(app, &snap),
            // Authoritative lapse: lock now rather than at the end of the grace window.
            Outcome::NotEntitled(status) => {
                clear_snapshot(app);
                Err(format!("subscription is {status}"))
            }
            Outcome::Gone(msg) => {
                clear_snapshot(app);
                let _ = keychain_store::delete_generic_secret(LICENSE_KEY_ACCOUNT);
                Err(msg)
            }
            Outcome::Transient(msg) => Err(msg),
        }
    }

    /// Current billing state for the settings panel. Never hits the network.
    #[tauri::command]
    pub fn billing_status(app: AppHandle) -> BillingView {
        view(&app, None)
    }

    /// Activate this Mac with a licence key from the checkout success page.
    ///
    /// Verification happens **before** anything is written: a typo, or a key for a cancelled
    /// subscription, must leave whatever was already on this Mac exactly as it was. The key
    /// reaches the Keychain only after the server has accepted it.
    #[tauri::command]
    pub fn billing_activate(app: AppHandle, license_key: String) -> BillingView {
        let key = license_key.trim().to_string();
        if key.is_empty() {
            return view(&app, Some("enter your licence key".into()));
        }
        match check(&app, &key) {
            Ok(Outcome::Entitled(snap)) => {
                if let Err(e) = keychain_store::set_generic_secret(LICENSE_KEY_ACCOUNT, key.as_bytes())
                {
                    return view(&app, Some(format!("could not store the licence: {e}")));
                }
                match write_snapshot(&app, &snap) {
                    Ok(()) => view(&app, None),
                    Err(e) => view(&app, Some(e)),
                }
            }
            Ok(other) => view(&app, Some(other.message())),
            Err(e) => view(&app, Some(e)),
        }
    }

    /// Re-verify now (the Refresh button, and the 24h timer).
    #[tauri::command]
    pub fn billing_refresh(app: AppHandle) -> BillingView {
        let Some(key) = stored_license_key() else {
            return view(&app, Some("no licence on this Mac".into()));
        };
        match reverify(&app, &key) {
            Ok(()) => view(&app, None),
            Err(e) => view(&app, Some(e)),
        }
    }

    /// Remove the licence from this Mac (moving to another machine). Local memory is untouched —
    /// deactivating is not deleting (FR-BIL-05: ローカルデータは削除しない).
    #[tauri::command]
    pub fn billing_deactivate(app: AppHandle) -> BillingView {
        let _ = keychain_store::delete_generic_secret(LICENSE_KEY_ACCOUNT);
        clear_snapshot(&app);
        view(&app, None)
    }

    /// Open Stripe Checkout in the system browser for `plan` × `interval`.
    ///
    /// The app sends the plan *name*; the Price ID stays on the server (issue #8 セキュリティ), and
    /// the card form is Stripe's, in the browser — never in this window.
    #[tauri::command]
    pub fn billing_open_checkout(
        plan: String,
        interval: String,
        app: AppHandle,
    ) -> Result<String, String> {
        record_billing_egress(&app, "stripe_checkout");
        let url = license_client::checkout_url(&api_origin(), &plan, &interval)
            .map_err(|e| e.message())?;
        open_in_browser(&url)?;
        Ok(url)
    }

    /// Open the Stripe Customer Portal — cancellation, plan changes and card updates all live
    /// there (issue #8: 90%+ of billing ops off our plate).
    #[tauri::command]
    pub fn billing_open_portal(app: AppHandle) -> Result<String, String> {
        let key = stored_license_key().ok_or_else(|| "no licence on this Mac".to_string())?;
        record_billing_egress(&app, "stripe_portal");
        let url = license_client::portal_url(&api_origin(), &key).map_err(|e| e.message())?;
        open_in_browser(&url)?;
        Ok(url)
    }

    fn open_in_browser(url: &str) -> Result<(), String> {
        // Only ever an https URL built from our own origin — but check anyway, because `open`
        // will happily launch anything with a scheme.
        if !url.starts_with("https://") && !url.starts_with("http://localhost") {
            return Err("refusing to open a non-https billing URL".into());
        }
        std::process::Command::new("open")
            .arg(url)
            .status()
            .map_err(|e| format!("open failed: {e}"))?;
        Ok(())
    }

    /// Start the FR-BIL-08 verification loop: once at launch, then every 24h. Silent — a failure
    /// shows up in the settings panel and (when the grace window is running down) as the amber
    /// indicator, never as a modal that interrupts work (CLAUDE.md エラー時挙動).
    pub fn spawn_verification_loop(app: AppHandle) {
        std::thread::spawn(move || loop {
            if let Some(key) = stored_license_key() {
                if let Err(e) = reverify(&app, &key) {
                    // The message never contains the licence key (only status / HTTP codes).
                    eprintln!("[billing] verification failed: {e}");
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(VERIFY_INTERVAL_SECS));
        });
    }
}
