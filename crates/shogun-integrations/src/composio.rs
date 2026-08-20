//! The Composio tool-execution seam (second layer, §6.10). Pure — the reqwest client lives in
//! shogun-core (`composio_send`), the single allowlisted HTTP egress (FR-TR-03). Here: the API
//! seam, the `GMAIL_SEND_EMAIL` argument builder, and response parsing.
//!
//! v1's only second-layer op is Gmail send (FR-C2-01). The consent + draft-stop gate is
//! [`shogun_mcp::composio`]; by the time execution reaches this seam the send has already passed
//! that gate and the L3 approval queue.

use serde_json::{json, Value};

/// Composio's tool slug for sending a Gmail message.
pub const GMAIL_SEND_EMAIL: &str = "GMAIL_SEND_EMAIL";

/// Executes one Composio tool for a user. The real impl is a `reqwest` POST to
/// `POST /api/v3/tools/execute/{tool_slug}` with an `x-api-key` header
/// (`shogun_core::composio_send::HttpComposioApi`); tests inject a fake.
pub trait ComposioApi {
    /// Execute `tool` for `user_id` with `arguments`, returning the raw JSON response (or a
    /// content-free error string).
    fn execute(&self, tool: &str, user_id: &str, arguments: Value) -> Result<Value, String>;
}

/// Build the `GMAIL_SEND_EMAIL` arguments (Composio field names: `recipient_email`, `subject`,
/// `body`).
pub fn gmail_send_arguments(recipient_email: &str, subject: &str, body: &str) -> Value {
    json!({
        "recipient_email": recipient_email,
        "subject": subject,
        "body": body,
    })
}

/// Interpret a Composio execute response: success unless `successful`/`success` is explicitly
/// `false` (or an `error` is present). Returns a short, content-free reason on failure — the
/// response can echo the message body, so only a code/flag is surfaced, never the payload.
pub fn parse_execute_response(resp: &Value) -> Result<(), String> {
    fn flag_of(v: &Value) -> Option<bool> {
        v.get("successful").or_else(|| v.get("success")).and_then(Value::as_bool)
    }
    fn has_error(v: &Value) -> bool {
        v.get("error").map(|e| !e.is_null()).unwrap_or(false)
    }
    // Composio marks tool success with a boolean (`successful` in v3; some responses use
    // `success`). Several tools surface per-tool failure UNDER `data` with HTTP 200 at the top,
    // so the nested envelope must be checked too — reporting an unflagged failure as Ok() here
    // marks an email delivered (and writes a traceability row) for a send that never happened.
    let data = resp.get("data");
    match flag_of(resp).or_else(|| data.and_then(flag_of)) {
        Some(true) => Ok(()),
        Some(false) => Err("composio tool reported failure".to_string()),
        // No explicit flag: treat an `error` at either level as failure, else assume success.
        None => {
            if has_error(resp) || data.is_some_and(has_error) {
                Err("composio tool returned an error".to_string())
            } else {
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmail_args_use_composio_field_names() {
        let a = gmail_send_arguments("b@e.com", "Ship date", "Friday.");
        assert_eq!(a["recipient_email"], "b@e.com");
        assert_eq!(a["subject"], "Ship date");
        assert_eq!(a["body"], "Friday.");
    }

    #[test]
    fn response_success_and_failure_flags() {
        assert!(parse_execute_response(&json!({ "successful": true, "data": {} })).is_ok());
        assert!(parse_execute_response(&json!({ "success": true })).is_ok());
        assert!(parse_execute_response(&json!({ "successful": false })).is_err());
        assert!(parse_execute_response(&json!({ "error": "invalid_grant" })).is_err());
        // No flag, no error → assume success (some tools return only data).
        assert!(parse_execute_response(&json!({ "data": { "id": "m1" } })).is_ok());
    }

    #[test]
    fn a_failure_nested_under_data_is_not_a_success() {
        // HTTP 200 with the per-tool failure inside `data` — the Gmail-send shape that used to
        // parse as Ok() and mark an unsent email as delivered.
        assert!(parse_execute_response(
            &json!({ "data": { "successful": false, "error": "invalid_grant" } })
        )
        .is_err());
        assert!(parse_execute_response(&json!({ "data": { "error": "quota_exceeded" } })).is_err());
        // An explicit top-level success still wins over incidental nested fields.
        assert!(parse_execute_response(
            &json!({ "successful": true, "data": { "error": null } })
        )
        .is_ok());
    }
}
