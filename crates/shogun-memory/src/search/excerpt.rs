/// A relevance-centred excerpt of `content`, at most `max_chars` characters.
///
/// A single captured window can be thousands of characters, so handing the model its first N
/// wastes the budget on whatever happened to be at the top of the window — usually chrome, not
/// the part that matched. This centres the window on the earliest occurrence of a query token
/// instead, and falls back to the head when nothing matches (an FTS trigram hit can land on a
/// substring no whole token covers).
///
/// Char-boundary safe: it slices by `char`, never by byte, so multi-byte text is never cut in
/// half. Matching is ASCII-case-insensitive — that covers the English path (the accuracy
/// priority) and is a no-op for scripts without case, so no text is mishandled either way.
pub fn excerpt(content: &str, query: &str, max_chars: usize) -> String {
    let chars: Vec<char> = content.chars().collect();
    if max_chars == 0 {
        return String::new();
    }
    if chars.len() <= max_chars {
        return content.trim().to_string();
    }
    // Earliest position among the query's tokens; short tokens are skipped as too noisy.
    let hit = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 2)
        .filter_map(|t| find_ci(&chars, t))
        .min();

    // Keep a little of the run-up so the match reads in context rather than starting mid-thought.
    let lead = max_chars / 3;
    let mut start = hit.unwrap_or(0).saturating_sub(lead);
    let mut end = start + max_chars;
    if end > chars.len() {
        end = chars.len();
        start = end.saturating_sub(max_chars);
    }
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(chars[start..end].iter().collect::<String>().trim());
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// Char-index of `needle` in `hay`, ASCII-case-insensitively. Works in char space so the index
/// it returns can be used to slice `hay` directly.
fn find_ci(hay: &[char], needle: &str) -> Option<usize> {
    let n: Vec<char> = needle.chars().map(|c| c.to_ascii_lowercase()).collect();
    if n.is_empty() || n.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - n.len()).find(|&i| {
        hay[i..i + n.len()]
            .iter()
            .map(|c| c.to_ascii_lowercase())
            .eq(n.iter().copied())
    })
}
