//! The support-intake client (CS / bug-report 窓口), feature `net`.
//!
//! FR-TR-03 keeps the single raw HTTP client in shogun-core, so the desktop shell asks this
//! module rather than reaching for reqwest itself. Same origin as the licence API
//! ([`crate::license_client::DEFAULT_LICENSE_API`]).
//!
//! What goes out, and nothing else: the category, the text the user typed into the report box,
//! and — only when the user ticked the diagnostics box — the app version, the macOS version and
//! the plan name. No capture content, no memory content, no licence key, no email unless the
//! user typed one in. The caller records one traceability row (`Route::Support`) before the
//! send; the payload here is user-authored and user-initiated, which is exactly what the
//! traceability screen exists to show.

use std::time::Duration;

/// A report as the user assembled it in the Help & Support panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportReport {
    /// "bug" | "feedback" | "question" — the server rejects anything else.
    pub category: String,
    /// The user's own words.
    pub message: String,
    /// Optional reply address, only if the user typed one.
    pub email: Option<String>,
    /// Diagnostics tuple — all three `None` unless the user opted in.
    pub app_version: Option<String>,
    pub os_version: Option<String>,
    pub plan: Option<String>,
}

impl SupportReport {
    /// The JSON body the server sees. Optional fields are omitted, not sent as null, so the
    /// wire shape shows exactly what was shared.
    pub fn to_json(&self) -> serde_json::Value {
        let mut body = serde_json::json!({
            "category": self.category,
            "message": self.message,
        });
        if let Some(v) = &self.email {
            body["email"] = serde_json::Value::String(v.clone());
        }
        if let Some(v) = &self.app_version {
            body["app_version"] = serde_json::Value::String(v.clone());
        }
        if let Some(v) = &self.os_version {
            body["os_version"] = serde_json::Value::String(v.clone());
        }
        if let Some(v) = &self.plan {
            body["plan"] = serde_json::Value::String(v.clone());
        }
        body
    }
}

/// Why a submission failed. All transient except `Rejected` — a 400 means this exact report
/// will never be accepted, so retrying it unchanged is noise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitError {
    /// The server refused the report as malformed (400). Terminal for this payload.
    Rejected,
    /// Rate limited or a server-side failure. Try again later.
    Server(String),
    /// Could not reach the API at all.
    Network(String),
    /// A 2xx whose body carried no ticket id.
    BadResponse(String),
}

impl SubmitError {
    pub fn message(&self) -> String {
        match self {
            Self::Rejected => "the server rejected this report".to_string(),
            Self::Server(m) => format!("support intake error: {m}"),
            Self::Network(m) => format!("support intake unreachable: {m}"),
            Self::BadResponse(m) => format!("unexpected support intake response: {m}"),
        }
    }
}

/// The intake endpoint for a given origin.
pub fn report_url(origin: &str) -> String {
    format!("{}/api/support/report", origin.trim_end_matches('/'))
}

/// POST the report. Blocking with a short timeout — the caller runs this off the UI thread and
/// shows the outcome; nothing here may hold a lane open.
pub fn submit(origin: &str, report: &SupportReport) -> Result<String, SubmitError> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("shogun/1.0")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| SubmitError::Network(e.to_string()))?;

    let resp = client
        .post(report_url(origin))
        .json(&report.to_json())
        .send()
        .map_err(|e| SubmitError::Network(e.to_string()))?;

    let status = resp.status();
    if status == reqwest::StatusCode::BAD_REQUEST {
        return Err(SubmitError::Rejected);
    }
    if !status.is_success() {
        return Err(SubmitError::Server(status.as_u16().to_string()));
    }

    let body: serde_json::Value = resp
        .json()
        .map_err(|e| SubmitError::BadResponse(e.to_string()))?;
    parse_submit_response(&body)
}

/// Pull the ticket id out of `{ ok: true, ticket_id: "…" }`.
fn parse_submit_response(body: &serde_json::Value) -> Result<String, SubmitError> {
    match body.get("ticket_id").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => Ok(id.to_string()),
        _ => Err(SubmitError::BadResponse("no ticket_id in response".to_string())),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn report_url_normalises_trailing_slash() {
        assert_eq!(report_url("https://shogunaios.com/"), "https://shogunaios.com/api/support/report");
        assert_eq!(report_url("https://shogunaios.com"), "https://shogunaios.com/api/support/report");
    }

    #[test]
    fn json_omits_absent_optionals() {
        let r = SupportReport {
            category: "bug".into(),
            message: "it broke".into(),
            email: None,
            app_version: None,
            os_version: None,
            plan: None,
        };
        let v = r.to_json();
        assert_eq!(v["category"], "bug");
        assert_eq!(v["message"], "it broke");
        assert!(v.get("email").is_none());
        assert!(v.get("app_version").is_none());
        assert!(v.get("os_version").is_none());
        assert!(v.get("plan").is_none());
    }

    #[test]
    fn json_carries_diagnostics_when_present() {
        let r = SupportReport {
            category: "feedback".into(),
            message: "love it".into(),
            email: Some("a@b.co".into()),
            app_version: Some("1.2.3".into()),
            os_version: Some("14.5".into()),
            plan: Some("pro".into()),
        };
        let v = r.to_json();
        assert_eq!(v["email"], "a@b.co");
        assert_eq!(v["app_version"], "1.2.3");
        assert_eq!(v["os_version"], "14.5");
        assert_eq!(v["plan"], "pro");
    }

    #[test]
    fn submit_response_needs_a_ticket_id() {
        let ok = serde_json::json!({ "ok": true, "ticket_id": "abc" });
        assert_eq!(parse_submit_response(&ok).unwrap(), "abc");
        let missing = serde_json::json!({ "ok": true });
        assert!(matches!(parse_submit_response(&missing), Err(SubmitError::BadResponse(_))));
        let empty = serde_json::json!({ "ok": true, "ticket_id": "" });
        assert!(matches!(parse_submit_response(&empty), Err(SubmitError::BadResponse(_))));
    }
}
