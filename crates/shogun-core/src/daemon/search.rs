//! Retrieval-facing daemon value types.

/// One retrieved piece of evidence behind an answer ([`crate::daemon::Db::assemble_context`]).
/// Carries its `event_id` so a generated answer can cite what it was grounded in and its `source`
/// so mail is distinguishable from a captured window (FR-MEM-23).
#[derive(Debug, Clone, PartialEq)]
pub struct Evidence {
    pub event_id: i64,
    pub ts: i64,
    pub source: String,
    pub title: Option<String>,
    pub excerpt: String,
    /// Linked `screen_frames` row when a finite-retention encrypted JPEG is stored.
    pub frame_id: Option<i64>,
}

/// A stored screen capture available for visual recall (metadata only — bytes via
/// [`crate::daemon::Db::get_screen_frame`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenFrameRef {
    pub frame_id: i64,
    pub event_id: i64,
    pub ts: i64,
    pub app_bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub width: u32,
    pub height: u32,
    pub ocr_excerpt: String,
    /// Thin stored OCR — caller should re-scan the JPEG (Vision) before answering.
    pub needs_rescan: bool,
    /// Linked event source.
    pub source: String,
}

/// Grounded context for one question: confidence-gated state facts plus retrieved evidence.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ContextPack {
    pub facts: Vec<String>,
    pub evidence: Vec<Evidence>,
    /// Stored JPEG frames matching a visual-recall question (hook for future vision input).
    pub screen_frames: Vec<ScreenFrameRef>,
}
