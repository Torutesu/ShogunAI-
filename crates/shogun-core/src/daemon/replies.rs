//! Reply-domain helpers.

/// Fraction of title words also present in the question, ignoring very short words.
pub(super) fn title_overlap(question_lower: &str, title_lower: &str) -> f64 {
    let words: Vec<&str> = title_lower
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| word.chars().count() > 2)
        .collect();
    if words.is_empty() {
        return 0.0;
    }
    let hits = words
        .iter()
        .filter(|word| question_lower.contains(**word))
        .count();
    hits as f64 / words.len() as f64
}
