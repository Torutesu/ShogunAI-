/// Human label for evidence citations (FR-MEM-23). Raw `source` tags stay in the DB.
pub fn evidence_source_label(source: &str) -> String {
    match source {
        "screen_ocr" => "screen text".to_string(),
        "capture" => "window".to_string(),
        "meeting" => "meeting".to_string(),
        "gmail" => "mail".to_string(),
        "gcal" => "calendar".to_string(),
        "slack" => "chat".to_string(),
        "notion" => "doc".to_string(),
        "github" => "code".to_string(),
        "linear" => "issue".to_string(),
        other => other.to_string(),
    }
}

/// True when the question is likely about on-screen content (visual recall path).
pub fn query_asks_about_screen(query: &str) -> bool {
    let q = query.to_ascii_lowercase();
    [
        "on my screen",
        "on screen",
        "what was on",
        "what did i see",
        "shown on",
        "displayed on",
        "looking at",
        "on my display",
    ]
    .iter()
    .any(|p| q.contains(p))
}

/// Exact local calendar boundaries supplied by the OS-facing caller.
///
/// Two boundaries are required because a local day can be 23 or 25 hours across DST changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDayBounds {
    pub yesterday_start_ms: i64,
    pub today_start_ms: i64,
}

fn query_has_word(query: &str, word: &str) -> bool {
    query
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|part| part == word)
}

/// Local day window implied by the question (`today`, `yesterday`, …). Returns `(from_ms, to_ms)`.
pub fn query_time_window(
    query: &str,
    now_ms: i64,
    local_days: LocalDayBounds,
) -> Option<(i64, i64)> {
    let q = query.to_ascii_lowercase();
    if q.contains("yesterday") {
        return Some((local_days.yesterday_start_ms, local_days.today_start_ms));
    }
    if q.contains("today") || q.contains("this morning") || q.contains("earlier today") {
        return Some((local_days.today_start_ms, now_ms));
    }
    None
}

/// True when the agent should pull stored screen frames (visual recall path).
pub fn query_wants_visual_recall(query: &str, now_ms: i64, local_days: LocalDayBounds) -> bool {
    if query_asks_about_screen(query) {
        return true;
    }
    query_time_window(query, now_ms, local_days).is_some_and(|_| {
        let q = query.to_ascii_lowercase();
        ["screen", "window", "see", "look", "show", "display", "app"]
            .iter()
            .any(|word| query_has_word(&q, word))
    })
}

/// Default time window for visual-recall frame search.
pub fn visual_recall_window(
    query: &str,
    now_ms: i64,
    local_days: LocalDayBounds,
    retention_ms: i64,
) -> (i64, i64) {
    let retention_start = now_ms.saturating_sub(retention_ms.max(0));
    let (from_ms, to_ms) =
        query_time_window(query, now_ms, local_days).unwrap_or((retention_start, now_ms));
    (from_ms.max(retention_start), to_ms.min(now_ms))
}
