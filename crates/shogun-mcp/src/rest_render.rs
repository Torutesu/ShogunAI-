use shogun_agents::approval::{ApprovalId, ApprovalQueue, ApprovalStatus};
use shogun_agents::entitlement::Entitlements;
use shogun_agents::permission::Level;

use crate::backend::{MemoryBackend, ReadParams};
use crate::memory_api::{read_inclusion, ReadInclusion, TokenRegistry, Tool};
use crate::visual_recall_api::{is_structured_read, render_structured};
use crate::voice_dictionary_api::{
    parse_term, render as render_voice_dictionary, VoiceDictionaryOperation,
};

use super::{route, RestRequest, Routed};

pub fn status_code(routed: &Routed) -> u16 {
    match routed {
        Routed::Unauthorized => 401,
        Routed::PlanLocked => 403,
        Routed::NotFound => 404,
        Routed::MethodNotAllowed => 405,
        Routed::Read { .. } | Routed::ApprovalStatus { .. } | Routed::Status | Routed::Metrics => {
            200
        }
        // A write is accepted (L2 still confirms in the Notch); an action may be pending.
        Routed::Write { .. } | Routed::Action => 202,
    }
}

/// Expire stale pending work then expose only id and status; previews never leave this path.
pub fn poll_approval(id: u64, approvals: &mut ApprovalQueue, now_ms: i64) -> String {
    approvals.expire_due(u64::try_from(now_ms).unwrap_or(0));
    let status = match approvals.status(ApprovalId(id)) {
        Some(ApprovalStatus::Pending) => "pending",
        Some(ApprovalStatus::Rejected) => "rejected",
        Some(ApprovalStatus::TimedOut) => "timed_out",
        Some(ApprovalStatus::Sent) => "sent",
        Some(ApprovalStatus::SendFailed) => "send_failed",
        Some(ApprovalStatus::DraftSaved) => "draft_saved",
        None => "unknown",
    };
    format!(r#"{{"approval_id":{id},"status":"{status}"}}"#)
}

/// The stable wire name of a tool (delegates to the shared name).
pub(super) fn tool_name(tool: Tool) -> &'static str {
    tool.wire_name()
}

/// Render backend read items to the API's confidence-gated JSON result (FR-API-06). Shared by the
/// REST and MCP faces so their read output is identical. Low-confidence items are dropped unless
/// `include_low`; medium ones are flagged `possibly`.
pub fn render_reads(tool: Tool, items: &[crate::backend::ReadItem], include_low: bool) -> String {
    let rendered: Vec<String> = items
        .iter()
        .filter_map(|item| match read_inclusion(item.confidence, include_low) {
            ReadInclusion::Included { possibly } => Some(format!(
                r#"{{"text":"{}","confidence":{},"possibly":{}}}"#,
                escape(&item.label),
                item.confidence,
                possibly
            )),
            ReadInclusion::Excluded => None,
        })
        .collect();
    format!(
        r#"{{"tool":"{}","results":[{}]}}"#,
        tool_name(tool),
        rendered.join(",")
    )
}

/// Public JSON string escape (quotes, backslash, control chars) — used by the render helpers.
pub fn escape(s: &str) -> String {
    json_escape(s)
}

pub(crate) fn level_label(level: Level) -> &'static str {
    match level {
        Level::L1 => "L1",
        Level::L2 => "L2",
        Level::L3 => "L3",
    }
}

/// The JSON body for a routing decision. Tool responses stub the data (`results: []`) until the
/// server's backend is wired; the auth/routing envelope is real. Hand-built JSON (no serde dep).
pub fn body_for(routed: &Routed) -> String {
    match routed {
        Routed::Unauthorized => r#"{"error":"unauthorized"}"#.to_string(),
        Routed::PlanLocked => r#"{"error":"plan_required"}"#.to_string(),
        Routed::NotFound => r#"{"error":"not_found"}"#.to_string(),
        Routed::MethodNotAllowed => r#"{"error":"method_not_allowed"}"#.to_string(),
        Routed::Status => r#"{"status":"ok","service":"shogun-memory-api"}"#.to_string(),
        // The server overrides this with live metrics; the placeholder keeps the layer pure.
        Routed::Metrics => r#"{"metrics":[]}"#.to_string(),
        Routed::Read { tool, .. } => format!(r#"{{"tool":"{}","results":[]}}"#, tool_name(*tool)),
        Routed::Write { tool, level } => {
            format!(
                r#"{{"tool":"{}","level":"{}","accepted":true}}"#,
                tool_name(*tool),
                level_label(*level)
            )
        }
        Routed::Action => r#"{"tool":"actions.execute","status":"routed"}"#.to_string(),
        Routed::ApprovalStatus { id } => format!(r#"{{"approval_id":{id},"status":"routed"}}"#),
    }
}

/// Route + render with a stub body (no backend). The server uses [`respond_with`]; this stays for
/// callers/tests that don't need real data.
pub fn respond(req: &RestRequest, tokens: &TokenRegistry, ent: &Entitlements) -> (u16, String) {
    let routed = route(req, tokens, ent);
    (status_code(&routed), body_for(&routed))
}

/// A parsed `actions.execute` request: either an on-device action or an external send (with the
pub(crate) fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Route + render **with real data** from `backend`. For a read tool, the backend supplies rows and
/// this applies the confidence gate (FR-API-06: Low excluded unless `?include_low`, Medium flagged
/// `possibly`); other decisions render as [`body_for`]. This is what the server calls.
fn structured_read_status(json: &str) -> u16 {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(v) => {
            let err = v.get("error").and_then(|e| e.as_str());
            match err {
                Some("not_found") | Some("missing_frame_id") => 404,
                Some(_) => 400,
                None => 200,
            }
        }
        Err(_) => 500,
    }
}

pub fn respond_with<B: MemoryBackend + ?Sized>(
    req: &RestRequest,
    tokens: &TokenRegistry,
    ent: &Entitlements,
    backend: &B,
) -> (u16, String) {
    match route(req, tokens, ent) {
        Routed::Read {
            tool: Tool::VoiceDictionaryList,
            ..
        } => match backend.manage_voice_dictionary(VoiceDictionaryOperation::List) {
            Ok(value) => (200, render_voice_dictionary(value)),
            Err(_) => (
                500,
                r#"{"error":"voice_dictionary_unavailable"}"#.to_string(),
            ),
        },
        Routed::Read { tool, id } => {
            let params = ReadParams {
                id,
                query: req.query.clone(),
                from_ms: req.from_ms,
                to_ms: req.to_ms,
                for_generation: req.for_generation,
                app_bundle_id: req.app_bundle_id.clone(),
                person_id: req.person_id.clone(),
                project_id: req.project_id.clone(),
            };
            if is_structured_read(tool) {
                let json = backend
                    .read_structured(tool, &params)
                    .unwrap_or_else(|| r#"{"error":"unavailable"}"#.to_string());
                let status = structured_read_status(&json);
                (status, render_structured(tool, &json))
            } else {
                let items = backend.read(tool, &params);
                (200, render_reads(tool, &items, req.include_low))
            }
        }
        Routed::Write { tool, level } => {
            let voice_operation = match tool {
                Tool::VoiceDictionaryCreate => parse_term(req.body.as_deref().unwrap_or(""))
                    .map(VoiceDictionaryOperation::Create),
                Tool::VoiceDictionaryUpdate => {
                    let id = req
                        .path
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .and_then(|value| value.parse().ok());
                    match (id, parse_term(req.body.as_deref().unwrap_or(""))) {
                        (Some(id), Ok(term)) => Ok(VoiceDictionaryOperation::Update { id, term }),
                        _ => Err("invalid voice dictionary term".to_string()),
                    }
                }
                Tool::VoiceDictionaryDelete => {
                    let id = req
                        .path
                        .trim_end_matches('/')
                        .split('/')
                        .rev()
                        .nth(1)
                        .and_then(|value| value.parse().ok());
                    id.map(|id| VoiceDictionaryOperation::Delete { id })
                        .ok_or_else(|| "invalid voice dictionary term".to_string())
                }
                _ => {
                    return match backend.write(tool, req.body.as_deref().unwrap_or("")) {
                        Ok(Some(id)) => (
                            202,
                            format!(
                                r#"{{"tool":"{}","level":"{}","id":{},"accepted":true}}"#,
                                tool_name(tool),
                                level_label(level),
                                id
                            ),
                        ),
                        Ok(None) => (
                            202,
                            format!(
                                r#"{{"tool":"{}","level":"{}","accepted":true}}"#,
                                tool_name(tool),
                                level_label(level)
                            ),
                        ),
                        Err(_) => (500, r#"{"error":"write_failed"}"#.to_string()),
                    }
                }
            };
            match voice_operation.and_then(|operation| backend.manage_voice_dictionary(operation)) {
                Ok(value) => (202, render_voice_dictionary(value)),
                Err(_) => (
                    400,
                    r#"{"error":"voice_dictionary_request_failed"}"#.to_string(),
                ),
            }
        }
        other => (status_code(&other), body_for(&other)),
    }
}
