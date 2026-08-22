/// A search result row, hydrated with the fields the UI needs (FR-MEM-23).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub event_id: i64,
    pub ts: i64,
    pub source: String,
    pub app_bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub content: String,
    /// Fusion score (higher = more relevant).
    pub score: f64,
}
