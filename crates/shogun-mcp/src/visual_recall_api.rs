//! Visual recall Memory API helpers — structured JSON responses (no confidence gate).

use crate::memory_api::Tool;

/// Reads that return structured JSON, not [`crate::backend::ReadItem`] rows: the visual-recall
/// tools, and `lessons.list` (a lesson row is id + kind + scope + instruction + confidence +
/// evidence_count + active — richer than a label/confidence pair; the Low-band read gate does
/// not apply because the Learned list deliberately shows sleeping/weak rows too, exactly like
/// the human UI — invariant 6).
pub fn is_structured_read(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::VisualRecallStatus
            | Tool::VisualRecallSearchFrames
            | Tool::VisualRecallGetFrame
            | Tool::VisualRecallRescanFrame
            | Tool::LessonsList
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
