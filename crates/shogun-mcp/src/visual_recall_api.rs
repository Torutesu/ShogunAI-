//! Visual recall Memory API helpers — structured JSON responses (no confidence gate).

use crate::memory_api::Tool;

/// Visual-recall / profile reads return structured JSON, not [`crate::backend::ReadItem`] rows.
pub fn is_structured_read(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::VisualRecallStatus
            | Tool::VisualRecallSearchFrames
            | Tool::VisualRecallGetFrame
            | Tool::VisualRecallRescanFrame
            | Tool::ProfileWhoami
    )
}

/// Wrap a JSON object/array as the standard tool response envelope.
pub fn render_structured(tool: Tool, json: &str) -> String {
    let result = serde_json::from_str::<serde_json::Value>(json)
        .unwrap_or_else(|_| serde_json::json!({ "error": "invalid_backend_response" }));
    serde_json::json!({
        "tool": tool.wire_name(),
        "result": result,
    })
    .to_string()
}
