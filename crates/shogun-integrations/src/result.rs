//! Normalize an MCP `tools/call` result into [`FetchedItem`]s for the ingest pipeline.
//!
//! Two layers, both pure:
//! 1. **Envelope** (MCP spec, stable): a `CallToolResult` is `{ content: [...], isError?, ...}`.
//!    An `isError: true` result becomes an `Err`. Records are taken from `structuredContent` when
//!    present, else parsed out of the text content blocks (Google's servers return JSON there).
//! 2. **Field mapping** (tolerant): each record is reduced to (external_id, title, body, ts_ms) by
//!    trying a list of candidate keys. The exact Google field names are confirmed against live
//!    responses; the tolerant match means a schema tweak (e.g. `snippet` vs `preview`) does not
//!    break ingest — a record only drops if it yields no body (same rule as [`shogun_mcp::sync`]).
//!
//! No item content is ever logged from here; errors carry only a short reason.

use serde_json::Value;
use shogun_mcp::sync::FetchedItem;

const ID_KEYS: &[&str] = &["external_id", "id", "threadId", "messageId", "eventId", "fileId", "name"];
const TITLE_KEYS: &[&str] = &["title", "subject", "summary", "name", "filename"];
const BODY_KEYS: &[&str] = &["body", "snippet", "description", "text", "content", "preview"];
const TS_KEYS: &[&str] =
    &["ts_ms", "internalDate", "updatedMs", "start_ms", "createdTimeMs", "modifiedTimeMs"];

/// Parse a `tools/call` result value into normalized items. `result` is the JSON-RPC `result`
/// object (already unwrapped from the JSON-RPC envelope by the transport).
pub fn parse_items(result: &Value) -> Result<Vec<FetchedItem>, String> {
    if result.get("isError").and_then(Value::as_bool).unwrap_or(false) {
        return Err(tool_error_reason(result));
    }
    let records = collect_records(result)?;
    Ok(records.iter().filter_map(record_to_item).collect())
}

/// Pull the record array out of a result: prefer `structuredContent`, else the JSON embedded in the
/// text content blocks.
fn collect_records(result: &Value) -> Result<Vec<Value>, String> {
    if let Some(sc) = result.get("structuredContent") {
        if let Some(items) = as_record_array(sc) {
            return Ok(items);
        }
    }
    let content = result.get("content").and_then(Value::as_array);
    let Some(blocks) = content else {
        // No structured content and no content array — nothing to ingest (not an error).
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        let Some(text) = block.get("text").and_then(Value::as_str) else {
            continue;
        };
        // Each text block is expected to hold JSON (an array, or an object wrapping an array).
        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            if let Some(items) = as_record_array(&parsed) {
                out.extend(items);
            }
        }
    }
    Ok(out)
}

/// Interpret a value as an array of record objects: either a bare array, or an object with a single
/// array field (e.g. `{ "threads": [...] }` / `{ "items": [...] }`).
fn as_record_array(v: &Value) -> Option<Vec<Value>> {
    if let Some(arr) = v.as_array() {
        return Some(arr.clone());
    }
    if let Some(obj) = v.as_object() {
        // A common shape is a wrapper object with one array of results.
        let mut arrays = obj.values().filter(|val| val.is_array());
        if let (Some(first), None) = (arrays.next(), arrays.next()) {
            if let Some(arr) = first.as_array() {
                return Some(arr.clone());
            }
        }
    }
    None
}

fn record_to_item(rec: &Value) -> Option<FetchedItem> {
    let body = first_str(rec, BODY_KEYS)?.trim().to_string();
    if body.is_empty() {
        return None;
    }
    Some(FetchedItem {
        external_id: first_str(rec, ID_KEYS).unwrap_or_default(),
        title: first_str(rec, TITLE_KEYS).unwrap_or_default(),
        body,
        ts_ms: first_ts(rec, TS_KEYS),
    })
}

/// The first present, string-valued key from `keys`.
fn first_str(rec: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| rec.get(*k).and_then(Value::as_str).map(str::to_string))
}

/// The first present epoch-ms timestamp from `keys`. Accepts a JSON number or a numeric string;
/// non-numeric (e.g. an RFC3339 string) yields 0 — the daemon may refine ordering on ingest. This
/// keeps the normalizer dependency-free (no datetime crate).
fn first_ts(rec: &Value, keys: &[&str]) -> i64 {
    for k in keys {
        match rec.get(*k) {
            Some(Value::Number(n)) => {
                if let Some(i) = n.as_i64() {
                    return i;
                }
            }
            Some(Value::String(s)) => {
                if let Ok(i) = s.parse::<i64>() {
                    return i;
                }
            }
            _ => {}
        }
    }
    0
}

/// A short, content-free reason from an `isError` result (the first text block, truncated).
fn tool_error_reason(result: &Value) -> String {
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| blocks.iter().find_map(|b| b.get("text").and_then(Value::as_str)))
        .unwrap_or("tool call returned isError");
    let mut reason: String = text.chars().take(120).collect();
    reason.insert_str(0, "mcp tool error: ");
    reason
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_structured_content_array() {
        let result = json!({
            "structuredContent": [
                { "id": "t1", "subject": "Roadmap", "snippet": "Ship Friday", "internalDate": 1000 },
                { "id": "t2", "subject": "Invoice", "snippet": "Due next week", "internalDate": "2000" },
            ],
            "isError": false
        });
        let items = parse_items(&result).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].external_id, "t1");
        assert_eq!(items[0].title, "Roadmap");
        assert_eq!(items[0].body, "Ship Friday");
        assert_eq!(items[0].ts_ms, 1000);
        // numeric string timestamp is parsed
        assert_eq!(items[1].ts_ms, 2000);
    }

    #[test]
    fn parses_json_from_text_content_block_with_wrapper_array() {
        let payload = json!({ "threads": [ { "threadId": "x", "summary": "Hi", "body": "hello" } ] });
        let result = json!({
            "content": [ { "type": "text", "text": payload.to_string() } ]
        });
        let items = parse_items(&result).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].external_id, "x");
        assert_eq!(items[0].body, "hello");
    }

    #[test]
    fn is_error_result_becomes_err_without_leaking_lots_of_text() {
        let result = json!({
            "isError": true,
            "content": [ { "type": "text", "text": "invalid_grant: token expired" } ]
        });
        let err = parse_items(&result).unwrap_err();
        assert!(err.starts_with("mcp tool error: "));
        assert!(err.contains("invalid_grant"));
    }

    #[test]
    fn records_with_no_body_are_dropped() {
        let result = json!({
            "structuredContent": [
                { "id": "a", "subject": "only a title" },
                { "id": "b", "body": "   " },
                { "id": "c", "body": "real" },
            ]
        });
        let items = parse_items(&result).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].external_id, "c");
    }

    #[test]
    fn empty_or_contentless_result_is_empty_not_error() {
        assert_eq!(parse_items(&json!({})).unwrap().len(), 0);
        assert_eq!(parse_items(&json!({ "content": [] })).unwrap().len(), 0);
    }
}
