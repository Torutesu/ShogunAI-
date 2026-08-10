//! The licence API client (issue #8 / FR-BIL-08), feature `net`.
//!
//! FR-TR-03 keeps the single raw HTTP client in shogun-core, so the desktop shell asks this
//! module rather than reaching for reqwest itself.
//!
//! What goes out, and nothing else: the licence key, an anonymous device id and the app version
//! (FR-BIL-08 — "検証リクエストにキャプチャ内容・メモリ内容を一切含めない"). No capture text, no
//! memory content, no email. What comes back is a signed licence token that
//! `shogun_license::verify` checks offline.
//!
//! Traceability: billing traffic is out of scope for the egress ledger
//! (docs/requirements-v1.0.md §7.7 table — it carries no capture content), but it is still
//! plain HTTPS with certificate verification left on, like every other egress here.

use std::time::Duration;

/// What the licence API answers. Field names mirror the JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyResponse {
    /// Whether the subscription entitles the app right now.
    pub entitled: bool,
    /// "standard" | "pro" | none when the price is unknown to the server.
    pub plan: Option<String>,
    /// Stripe subscription status ("active" / "trialing" / "past_due" / "canceled" / …).
    pub status: String,
    /// Subscription period end, unix seconds — the "next billing date" the UI shows.
    pub current_period_end: Option<i64>,
    pub cancel_at_period_end: bool,
    /// The signed token. `None` when the subscription is not entitled — the device then simply
    /// ages out of its cached token's grace window.
    pub token: Option<String>,
}

/// Why a verification did not produce an answer. The caller keeps using its cached token for
/// anything that is not [`VerifyError::NotFound`] — a network outage must not lock a paying Mac
/// (FR-BIL-09).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// The server does not know this licence key (or it was revoked). Terminal: stop retrying.
    NotFound,
    /// Rate limited or a server-side failure. Transient: retry on the next cycle.
    Server(String),
    /// Could not reach the API at all. Transient.
    Network(String),
    /// A 2xx whose body we could not read as a verification answer.
    BadResponse(String),
}

impl VerifyError {
    /// Is this a "the licence is gone" answer rather than "we could not ask"?
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::NotFound)
    }

    pub fn message(&self) -> String {
        match self {
            Self::NotFound => "licence not found".to_string(),
            Self::Server(m) => format!("licence server error: {m}"),
            Self::Network(m) => format!("licence server unreachable: {m}"),
            Self::BadResponse(m) => format!("unexpected licence response: {m}"),
        }
    }
}

/// Default licence API origin. Overridable with `SHOGUN_LICENSE_API` for staging and dev.
pub const DEFAULT_LICENSE_API: &str = "https://syogun.com";

/// The verification endpoint for a given origin.
pub fn verify_url(origin: &str) -> String {
    format!("{}/api/license/verify", origin.trim_end_matches('/'))
}

/// POST the verification. Blocking, with a short timeout: this runs at launch and on a 24h timer,
/// and it must never be able to hold a startup path open.
pub fn verify(
    origin: &str,
    license_key: &str,
    device_id: &str,
    app_version: &str,
) -> Result<VerifyResponse, VerifyError> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("shogun/1.0")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| VerifyError::Network(e.to_string()))?;

    let resp = client
        .post(verify_url(origin))
        .json(&serde_json::json!({
            "license_key": license_key,
            "device_id": device_id,
            "app_version": app_version,
        }))
        .send()
        .map_err(|e| VerifyError::Network(e.to_string()))?;

    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(VerifyError::NotFound);
    }
    if !status.is_success() {
        return Err(VerifyError::Server(status.as_u16().to_string()));
    }

    let body: serde_json::Value = resp
        .json()
        .map_err(|e| VerifyError::BadResponse(e.to_string()))?;
    parse_verify_response(&body)
}

/// Ask the backend for a Stripe Checkout URL for `plan` × `interval`.
///
/// The app never holds a Stripe Price ID — it names the plan and the server picks the price
/// (issue #8 セキュリティ). The returned URL is Stripe-hosted and opens in the system browser, so
/// no card data ever touches the app (FR-BIL-07).
pub fn checkout_url(origin: &str, plan: &str, interval: &str) -> Result<String, VerifyError> {
    post_for_url(
        &format!("{}/api/stripe/checkout", origin.trim_end_matches('/')),
        &serde_json::json!({ "plan": plan, "interval": interval, "source": "app" }),
    )
}

/// Ask the backend for a Stripe Customer Portal URL for this licence.
pub fn portal_url(origin: &str, license_key: &str) -> Result<String, VerifyError> {
    post_for_url(
        &format!("{}/api/stripe/portal", origin.trim_end_matches('/')),
        &serde_json::json!({ "license_key": license_key }),
    )
}

/// Shared shape of the two "give me a hosted Stripe URL" calls.
fn post_for_url(url: &str, body: &serde_json::Value) -> Result<String, VerifyError> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("shogun/1.0")
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| VerifyError::Network(e.to_string()))?;
    let resp = client
        .post(url)
        .json(body)
        .send()
        .map_err(|e| VerifyError::Network(e.to_string()))?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(VerifyError::NotFound);
    }
    if !status.is_success() {
        return Err(VerifyError::Server(status.as_u16().to_string()));
    }
    let body: serde_json::Value = resp.json().map_err(|e| VerifyError::BadResponse(e.to_string()))?;
    parse_url_response(&body)
}

/// Pure parser for the `{ ok, url }` bodies.
pub fn parse_url_response(body: &serde_json::Value) -> Result<String, VerifyError> {
    if body.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        let err = body.get("error").and_then(serde_json::Value::as_str).unwrap_or("unknown");
        return Err(VerifyError::Server(err.to_string()));
    }
    body.get("url")
        .and_then(serde_json::Value::as_str)
        .filter(|u| !u.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| VerifyError::BadResponse("no url".to_string()))
}

/// Pure parser for the verification body — separated so it is testable without a server.
pub fn parse_verify_response(body: &serde_json::Value) -> Result<VerifyResponse, VerifyError> {
    if body.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        let err = body
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        return Err(VerifyError::Server(err.to_string()));
    }
    Ok(VerifyResponse {
        entitled: body
            .get("entitled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        plan: body
            .get("plan")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        status: body
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        current_period_end: body.get("current_period_end").and_then(serde_json::Value::as_i64),
        cancel_at_period_end: body
            .get("cancel_at_period_end")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        token: body
            .get("token")
            .and_then(serde_json::Value::as_str)
            .filter(|t| !t.trim().is_empty())
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_endpoint_without_doubling_slashes() {
        assert_eq!(verify_url("https://syogun.com/"), "https://syogun.com/api/license/verify");
        assert_eq!(verify_url("http://localhost:3000"), "http://localhost:3000/api/license/verify");
    }

    #[test]
    fn parses_an_entitled_answer() {
        let body = serde_json::json!({
            "ok": true, "entitled": true, "plan": "pro", "status": "active",
            "current_period_end": 1_800_000_000i64, "cancel_at_period_end": false,
            "token": "v1.aaa.bbb", "grace_days": 14,
        });
        let r = parse_verify_response(&body).expect("parse");
        assert!(r.entitled);
        assert_eq!(r.plan.as_deref(), Some("pro"));
        assert_eq!(r.token.as_deref(), Some("v1.aaa.bbb"));
    }

    #[test]
    fn a_lapsed_answer_carries_status_but_no_token() {
        let body = serde_json::json!({
            "ok": true, "entitled": false, "plan": "pro", "status": "canceled",
            "current_period_end": 1_700_000_000i64, "cancel_at_period_end": true, "token": null,
        });
        let r = parse_verify_response(&body).expect("parse");
        assert!(!r.entitled);
        assert_eq!(r.status, "canceled");
        assert_eq!(r.token, None);
    }

    #[test]
    fn an_error_body_is_an_error_not_a_default_answer() {
        let body = serde_json::json!({ "ok": false, "error": "rate_limited" });
        assert_eq!(
            parse_verify_response(&body),
            Err(VerifyError::Server("rate_limited".to_string()))
        );
    }

    #[test]
    fn parses_a_hosted_url_answer_and_rejects_an_empty_one() {
        assert_eq!(
            parse_url_response(&serde_json::json!({ "ok": true, "url": "https://checkout.stripe.com/x" })),
            Ok("https://checkout.stripe.com/x".to_string())
        );
        assert_eq!(
            parse_url_response(&serde_json::json!({ "ok": true, "url": "" })),
            Err(VerifyError::BadResponse("no url".to_string()))
        );
        assert_eq!(
            parse_url_response(&serde_json::json!({ "ok": false, "error": "billing_not_configured" })),
            Err(VerifyError::Server("billing_not_configured".to_string()))
        );
    }

    #[test]
    fn only_not_found_is_terminal() {
        assert!(VerifyError::NotFound.is_terminal());
        assert!(!VerifyError::Network("offline".into()).is_terminal());
        assert!(!VerifyError::Server("500".into()).is_terminal());
    }
}
