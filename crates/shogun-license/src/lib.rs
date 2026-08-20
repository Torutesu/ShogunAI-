//! Licence tokens — the device-side half of Stripe billing (issue #8, FR-BIL-08 / FR-BIL-09).
//!
//! The licence API (`apps/website/src/app/api/license/verify/route.ts`) answers a verification
//! with a compact, Ed25519-signed assertion:
//!
//! ```text
//! v1.<base64url(payload JSON)>.<base64url(signature)>
//! ```
//!
//! ```json
//! { "v":1, "lic":"<uuid>", "plan":"standard|pro", "status":"active",
//!   "device":"<anonymous device id>", "iat":1754800000, "exp":1754893600,
//!   "period_end":1786336000, "cancel_at_period_end":false, "grace_days":14 }
//! ```
//!
//! Why a signed token instead of "ask the server each time":
//!
//! - **Offline (FR-BIL-09)**: a Mac with no network keeps full access for 14 days from the last
//!   successful verification, amber from day 7. That is only safe if the cached answer cannot be
//!   forged, which is what the signature buys.
//! - **Device-bound**: the device id is inside the signed bytes, so a token copied to a second
//!   Mac verifies against a different device id and is rejected. One licence, one Mac at a time.
//! - **No secret on the device**: verification needs only the public key, embedded in the binary.
//!   The licence *key* (the bearer used to call the API) is the only secret, and it lives in the
//!   Keychain (NFR-SEC-01).
//!
//! This crate is **pure**: `now_ms` is always a parameter, nothing here reads the clock, the
//! filesystem or the network. The effectful side lives in the desktop app
//! (`apps/desktop/src-tauri/src/billing.rs`) and in `shogun-core::license_client`.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};
use shogun_agents::entitlement::{BillingState, PaidPlan};

/// Offline grace budget: how long a cached token keeps working with no successful verification
/// (FR-BIL-09). Measured from the token's `iat`, not from the last launch.
pub const OFFLINE_GRACE_DAYS: u64 = 14;

/// When the status indicator turns amber during that grace window (FR-BIL-09: "7日目からアンバー").
pub const OFFLINE_AMBER_DAYS: u64 = 7;

/// Hard ceiling on the `grace_days` a token may claim. The server sets the real number; this only
/// stops a future (or tampered-but-correctly-signed-by-a-leaked-key) token asking for a decade.
pub const MAX_GRACE_DAYS: u64 = 30;

const MS_PER_DAY: u64 = 24 * 60 * 60 * 1000;

/// The Ed25519 public key of the licence API, base64 (raw 32 bytes), baked into release builds.
///
/// Empty until the production key is generated — `scripts/gen-license-keypair.mjs` prints the
/// value to paste here. While it is empty, [`public_key`] falls back to the
/// `SHOGUN_LICENSE_PUBKEY` environment variable, which is how dev and CI run.
///
/// **Rotation**: ship a build carrying the new key BEFORE the API starts signing with it, or
/// every installed Mac drops into its offline-grace window on the next verification.
pub const EMBEDDED_PUBLIC_KEY_B64: &str = "";

/// Why a token was refused. Deliberately coarse: the UI says "couldn't verify", and a caller
/// must never branch into "grant access" on any of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseError {
    /// Not three dot-separated parts, or a part that is not base64url.
    Malformed,
    /// A version prefix this build does not know. Treated as no licence, never as access.
    UnsupportedVersion,
    /// The signature does not verify against the public key — tampered, or signed by someone else.
    BadSignature,
    /// A valid token, but issued for a different device.
    DeviceMismatch,
    /// Signature verified but the payload is not a licence payload we understand.
    BadPayload,
    /// No public key is configured in this build (see [`EMBEDDED_PUBLIC_KEY_B64`]).
    NoPublicKey,
}

impl LicenseError {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::UnsupportedVersion => "unsupported_version",
            Self::BadSignature => "bad_signature",
            Self::DeviceMismatch => "device_mismatch",
            Self::BadPayload => "bad_payload",
            Self::NoPublicKey => "no_public_key",
        }
    }
}

/// How current a verified token is, at a given moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Verified recently enough that the token has not expired. Normal steady state.
    Fresh,
    /// Past `exp` but inside the offline grace window. Full access, `days_offline` for the
    /// indicator (amber from [`OFFLINE_AMBER_DAYS`]).
    Grace { days_offline: u64 },
    /// Past the grace window. No access from this token.
    Stale,
}

impl Freshness {
    /// Does this state still entitle the app?
    pub fn is_valid(self) -> bool {
        !matches!(self, Self::Stale)
    }

    /// Should the notch indicator go amber (degraded-but-working, CLAUDE.md エラー時挙動)?
    pub fn is_amber(self) -> bool {
        matches!(self, Self::Grace { days_offline } if days_offline >= OFFLINE_AMBER_DAYS)
    }
}

/// A signature-verified, device-matched licence token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicenseToken {
    /// Licence id (not the licence key — the key never leaves the Keychain).
    pub license_id: String,
    pub plan: PaidPlan,
    /// The Stripe subscription status at issuance, verbatim ("active", "trialing", "past_due").
    pub status: String,
    /// The device this token is bound to.
    pub device_id: String,
    /// Issued-at / expires-at, unix **seconds** (as the API sends them).
    pub issued_at: u64,
    pub expires_at: u64,
    /// Subscription period end, unix seconds — the "next billing date" the UI shows.
    pub period_end: Option<i64>,
    /// The subscription stops at `period_end` (cancelled from the portal, still paid until then).
    pub cancel_at_period_end: bool,
    /// Offline grace this token grants, already clamped to [`MAX_GRACE_DAYS`].
    pub grace_days: u64,
}

impl LicenseToken {
    /// The instant this token stops entitling anything, in unix ms.
    pub fn grace_deadline_ms(&self) -> u64 {
        self.issued_at
            .saturating_mul(1000)
            .saturating_add(self.grace_days.saturating_mul(MS_PER_DAY))
    }

    /// Classify the token at `now_ms`.
    ///
    /// A clock that has been moved *backwards* (before issuance) reads as `Fresh`: refusing there
    /// would lock out a paying user over a timezone or NTP hiccup, and moving the clock back can
    /// only ever shorten the window a cheater gets, never extend it past the deadline.
    pub fn freshness(&self, now_ms: u64) -> Freshness {
        let exp_ms = self.expires_at.saturating_mul(1000);
        if now_ms <= exp_ms {
            return Freshness::Fresh;
        }
        if now_ms < self.grace_deadline_ms() {
            // Anchored on `iat`, like the deadline: FR-BIL-09 measures both the 14-day window
            // and the day-7 amber from the last obtained valid token. Anchoring this on `exp`
            // made amber arrive one token-lifetime late (a ~24h token: green through day 8,
            // then only a 6-day warning before the cutoff).
            let offline_ms = now_ms.saturating_sub(self.issued_at.saturating_mul(1000));
            return Freshness::Grace { days_offline: offline_ms / MS_PER_DAY };
        }
        Freshness::Stale
    }

    /// The billing state this token asserts at `now_ms` — the value
    /// `shogun_agents::entitlement::resolve_plan` consumes.
    ///
    /// The API only issues a token while the subscription is entitled, so a token that is still
    /// inside its validity window means paid access. Past the window it becomes
    /// [`BillingState::Lapsed`], which falls back to the trial rules (i.e. locked for anyone whose
    /// 7 days are up) rather than to full access.
    pub fn billing_state(&self, now_ms: u64) -> BillingState {
        if self.freshness(now_ms).is_valid() {
            BillingState::Active(self.plan)
        } else {
            BillingState::Lapsed
        }
    }
}

/// The public key this build verifies against: `SHOGUN_LICENSE_PUBKEY` (base64, raw 32 bytes) if
/// set, else [`EMBEDDED_PUBLIC_KEY_B64`]. The env override is what dev, CI and a staging licence
/// API use; release builds carry the embedded constant.
pub fn public_key() -> Option<[u8; 32]> {
    // Debug builds only: honouring the env in release would let anyone swap in their own
    // keypair and mint themselves a plan (the whole gate rests on this key).
    #[cfg(debug_assertions)]
    if let Ok(v) = std::env::var("SHOGUN_LICENSE_PUBKEY") {
        if let Some(k) = decode_public_key(v.trim()) {
            return Some(k);
        }
    }
    decode_public_key(EMBEDDED_PUBLIC_KEY_B64)
}

/// Decode a base64 (standard or url-safe, padded or not) 32-byte Ed25519 public key.
pub fn decode_public_key(b64: &str) -> Option<[u8; 32]> {
    let trimmed = b64.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(trimmed))
        .or_else(|_| URL_SAFE_NO_PAD.decode(trimmed))
        .ok()?;
    <[u8; 32]>::try_from(bytes.as_slice()).ok()
}

/// Verify a token's signature and device binding. Does **not** consider time — call
/// [`LicenseToken::freshness`] for that, so the caller decides what an expired-but-in-grace token
/// means for its own surface.
pub fn verify(
    token: &str,
    public_key: &[u8; 32],
    device_id: &str,
) -> Result<LicenseToken, LicenseError> {
    let mut parts = token.trim().split('.');
    let (version, body_b64, sig_b64) = match (parts.next(), parts.next(), parts.next(), parts.next())
    {
        (Some(v), Some(b), Some(s), None) => (v, b, s),
        _ => return Err(LicenseError::Malformed),
    };
    if version != "v1" {
        return Err(LicenseError::UnsupportedVersion);
    }

    let body = URL_SAFE_NO_PAD.decode(body_b64).map_err(|_| LicenseError::Malformed)?;
    let sig_bytes = URL_SAFE_NO_PAD.decode(sig_b64).map_err(|_| LicenseError::Malformed)?;
    let sig_arr = <[u8; 64]>::try_from(sig_bytes.as_slice()).map_err(|_| LicenseError::Malformed)?;

    let key = VerifyingKey::from_bytes(public_key).map_err(|_| LicenseError::NoPublicKey)?;
    // `verify_strict` rejects small-order / non-canonical keys and signatures — the malleability
    // class of accepted-but-not-really-valid signatures.
    key.verify_strict(&body, &Signature::from_bytes(&sig_arr))
        .map_err(|_| LicenseError::BadSignature)?;

    // Only now — after the bytes are proven to be ours — is it safe to interpret them.
    let v: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| LicenseError::BadPayload)?;
    if v.get("v").and_then(serde_json::Value::as_u64) != Some(1) {
        return Err(LicenseError::UnsupportedVersion);
    }

    let device = v.get("device").and_then(serde_json::Value::as_str).unwrap_or_default();
    if device.is_empty() || device != device_id {
        return Err(LicenseError::DeviceMismatch);
    }

    let plan = match v.get("plan").and_then(serde_json::Value::as_str) {
        Some("pro") => PaidPlan::Pro,
        Some("standard") => PaidPlan::Standard,
        // An unknown plan name never becomes Pro by default — a future plan must ship with the
        // build that understands it.
        _ => return Err(LicenseError::BadPayload),
    };

    let issued_at = v.get("iat").and_then(serde_json::Value::as_u64).ok_or(LicenseError::BadPayload)?;
    let expires_at = v.get("exp").and_then(serde_json::Value::as_u64).ok_or(LicenseError::BadPayload)?;

    Ok(LicenseToken {
        license_id: v.get("lic").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
        plan,
        status: v
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("active")
            .to_string(),
        device_id: device.to_string(),
        issued_at,
        expires_at,
        period_end: v.get("period_end").and_then(serde_json::Value::as_i64),
        cancel_at_period_end: v
            .get("cancel_at_period_end")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        grace_days: v
            .get("grace_days")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(OFFLINE_GRACE_DAYS)
            .min(MAX_GRACE_DAYS),
    })
}

/// Verify and immediately reduce to a billing state — the one-liner the plan sources want.
/// Any failure is [`BillingState::Unknown`] (fall back to the trial rules), never access.
pub fn billing_state_from_token(
    token: &str,
    public_key: &[u8; 32],
    device_id: &str,
    now_ms: u64,
) -> BillingState {
    match verify(token, public_key, device_id) {
        Ok(t) => t.billing_state(now_ms),
        Err(_) => BillingState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const DEVICE: &str = "device-abc123";

    fn key() -> SigningKey {
        // A fixed seed: the tests must be deterministic, and this key signs nothing real.
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn mint(payload: serde_json::Value) -> String {
        let body = serde_json::to_vec(&payload).expect("serialize");
        let sig = key().sign(&body);
        format!(
            "v1.{}.{}",
            URL_SAFE_NO_PAD.encode(&body),
            URL_SAFE_NO_PAD.encode(sig.to_bytes())
        )
    }

    fn payload(plan: &str, iat: u64, exp: u64) -> serde_json::Value {
        serde_json::json!({
            "v": 1, "lic": "lic-1", "plan": plan, "status": "active", "device": DEVICE,
            "iat": iat, "exp": exp, "period_end": 1_800_000_000i64,
            "cancel_at_period_end": false, "grace_days": OFFLINE_GRACE_DAYS,
        })
    }

    fn pubkey() -> [u8; 32] {
        key().verifying_key().to_bytes()
    }

    #[test]
    fn verifies_a_well_formed_token() {
        let t = verify(&mint(payload("pro", 1000, 2000)), &pubkey(), DEVICE).expect("verify");
        assert_eq!(t.plan, PaidPlan::Pro);
        assert_eq!(t.license_id, "lic-1");
        assert_eq!(t.period_end, Some(1_800_000_000));
        assert_eq!(t.grace_days, OFFLINE_GRACE_DAYS);
    }

    #[test]
    fn rejects_a_tampered_payload() {
        let token = mint(payload("standard", 1000, 2000));
        // Re-encode the payload as Pro, keep the original signature.
        let mut parts = token.split('.');
        let (_v, _b, sig) = (
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default(),
        );
        let forged_body = serde_json::to_vec(&payload("pro", 1000, 2000)).expect("serialize");
        let forged = format!("v1.{}.{}", URL_SAFE_NO_PAD.encode(&forged_body), sig);
        assert_eq!(verify(&forged, &pubkey(), DEVICE), Err(LicenseError::BadSignature));
    }

    #[test]
    fn rejects_another_devices_token() {
        let token = mint(payload("pro", 1000, 2000));
        assert_eq!(verify(&token, &pubkey(), "someone-elses-mac"), Err(LicenseError::DeviceMismatch));
    }

    #[test]
    fn rejects_a_token_signed_by_a_different_key() {
        let token = mint(payload("pro", 1000, 2000));
        let other = SigningKey::from_bytes(&[9u8; 32]).verifying_key().to_bytes();
        assert_eq!(verify(&token, &other, DEVICE), Err(LicenseError::BadSignature));
    }

    #[test]
    fn rejects_malformed_and_unknown_versions() {
        let pk = pubkey();
        assert_eq!(verify("not-a-token", &pk, DEVICE), Err(LicenseError::Malformed));
        assert_eq!(verify("v1.a.b.c", &pk, DEVICE), Err(LicenseError::Malformed));
        let body = URL_SAFE_NO_PAD.encode(b"{}");
        assert_eq!(
            verify(&format!("v2.{body}.{body}"), &pk, DEVICE),
            Err(LicenseError::UnsupportedVersion)
        );
    }

    #[test]
    fn unknown_plan_is_never_silently_pro() {
        let token = mint(payload("enterprise", 1000, 2000));
        assert_eq!(verify(&token, &pubkey(), DEVICE), Err(LicenseError::BadPayload));
    }

    #[test]
    fn grace_window_boundaries() {
        // issued at t=0, expires after 1h, 14 days of grace from issuance
        let iat = 0;
        let exp = 3600;
        let t = verify(&mint(payload("pro", iat, exp)), &pubkey(), DEVICE).expect("verify");

        let exp_ms = exp * 1000;
        assert_eq!(t.freshness(exp_ms), Freshness::Fresh, "last ms before expiry is fresh");
        assert_eq!(t.freshness(exp_ms + 1), Freshness::Grace { days_offline: 0 });

        // day 7 offline → amber, still valid
        let day7 = exp_ms + 7 * MS_PER_DAY;
        assert!(t.freshness(day7).is_valid());
        assert!(t.freshness(day7).is_amber());

        // the deadline is measured from issuance: iat + 14d
        let deadline = t.grace_deadline_ms();
        assert_eq!(deadline, 14 * MS_PER_DAY);
        assert!(t.freshness(deadline - 1).is_valid());
        assert_eq!(t.freshness(deadline), Freshness::Stale);
        assert!(!t.freshness(deadline).is_valid());
    }

    #[test]
    fn amber_arrives_on_day_7_from_issuance_even_with_a_day_long_token() {
        // FR-BIL-09: amber from day 7, cutoff at day 14 — both measured from the token. With a
        // ~24h token, anchoring days_offline on exp instead of iat pushed amber to day 8 and
        // shrank the warning window to 6 days.
        let iat = 0;
        let exp = 24 * 3600; // a one-day token
        let t = verify(&mint(payload("pro", iat, exp)), &pubkey(), DEVICE).expect("verify");
        let day7 = 7 * MS_PER_DAY;
        assert!(t.freshness(day7).is_amber(), "amber exactly at iat + 7d");
        assert!(!t.freshness(day7 - 1).is_amber(), "still green just before day 7");
        assert!(t.freshness(day7 - 1).is_valid() && t.freshness(day7).is_valid());
    }

    #[test]
    fn a_backwards_clock_does_not_lock_a_paying_user_out() {
        let t = verify(&mint(payload("pro", 10_000, 20_000)), &pubkey(), DEVICE).expect("verify");
        assert_eq!(t.freshness(0), Freshness::Fresh);
    }

    #[test]
    fn billing_state_tracks_the_window() {
        let t = verify(&mint(payload("standard", 0, 3600)), &pubkey(), DEVICE).expect("verify");
        assert_eq!(t.billing_state(0), BillingState::Active(PaidPlan::Standard));
        assert_eq!(t.billing_state(t.grace_deadline_ms()), BillingState::Lapsed);
    }

    #[test]
    fn grace_days_are_clamped() {
        let mut p = payload("pro", 0, 3600);
        p["grace_days"] = serde_json::json!(3650);
        let t = verify(&mint(p), &pubkey(), DEVICE).expect("verify");
        assert_eq!(t.grace_days, MAX_GRACE_DAYS);
    }

    #[test]
    fn a_bad_token_is_unknown_not_access() {
        assert_eq!(
            billing_state_from_token("garbage", &pubkey(), DEVICE, 0),
            BillingState::Unknown
        );
    }

    #[test]
    fn public_key_decoding_accepts_both_alphabets() {
        let raw = pubkey();
        let std_b64 = base64::engine::general_purpose::STANDARD.encode(raw);
        let url_b64 = URL_SAFE_NO_PAD.encode(raw);
        assert_eq!(decode_public_key(&std_b64), Some(raw));
        assert_eq!(decode_public_key(&url_b64), Some(raw));
        assert_eq!(decode_public_key(""), None);
        assert_eq!(decode_public_key("short"), None);
    }
}
