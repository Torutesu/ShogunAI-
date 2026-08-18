//! Pure safeguards for optional cloud dictation cleanup.
//!
//! The model may only polish punctuation and capitalization. Any untrusted output that changes
//! spoken words or protected tokens falls back to the original ASR transcript.

use std::collections::HashMap;

pub const MAX_TRANSCRIPT_BYTES: usize = 16 * 1024;
const MAX_EDITED_BYTES: usize = 24 * 1024;

/// Bounded, clearly delimited untrusted user content for the OpenAI-compatible request.
pub fn edit_user_message(transcript: &str) -> Option<String> {
    input_is_eligible(transcript).then(|| format!("<transcript>\n{transcript}\n</transcript>"))
}

/// Reject absent, blank, or oversized source text before it leaves the device.
pub fn input_is_eligible(transcript: &str) -> bool {
    !transcript.trim().is_empty() && transcript.len() <= MAX_TRANSCRIPT_BYTES
}

/// Return `true` only for safe punctuation/capitalization-only changes.
pub fn output_is_valid(raw: &str, edited: &str) -> bool {
    output_is_valid_with_protected(raw, edited, &[])
}

/// Validate model output and preserve every dictionary term exactly.
pub fn output_is_valid_with_protected(raw: &str, edited: &str, protected_terms: &[String]) -> bool {
    let edited = edited.trim();
    !edited.is_empty()
        && edited.len() <= edited_limit(raw)
        && !has_response_artifact(edited)
        && normalized_words(raw) == normalized_words(edited)
        && protected_token_counts(raw) == protected_token_counts(edited)
        && protected_terms
            .iter()
            .all(|term| raw.match_indices(term).count() == edited.match_indices(term).count())
}

fn edited_limit(raw: &str) -> usize {
    MAX_EDITED_BYTES.min(raw.len().saturating_mul(2).saturating_add(512))
}

fn has_response_artifact(edited: &str) -> bool {
    let lower = edited.to_ascii_lowercase();
    edited.contains("```")
        || ["edited:", "cleaned:", "transcript:", "output:"]
            .iter()
            .any(|label| lower.starts_with(label))
}

/// Ordered multiset, not merely a bag: reordering spoken words is also unsafe.
fn normalized_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|word| {
            let word: String = word
                .chars()
                .filter(|ch| ch.is_alphanumeric() || *ch == '\'')
                .flat_map(char::to_lowercase)
                .collect();
            (!word.is_empty()).then_some(word)
        })
        .collect()
}

fn protected_token_counts(text: &str) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for token in text
        .split_whitespace()
        .filter(|token| is_protected_token(token))
    {
        *counts.entry((*token).to_string()).or_insert(0) += 1;
    }
    counts
}

fn is_protected_token(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    let email = token.contains('@')
        && token
            .split_once('@')
            .is_some_and(|(_, host)| host.contains('.'));
    let url =
        lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("www.");
    let has_digit = token.chars().any(|ch| ch.is_ascii_digit());
    let code_like = token.contains("::")
        || token.contains('/')
        || token.contains('\\')
        || token.contains('_')
        || token.contains('=')
        || token.contains("->")
        || token.contains('`')
        || token.contains('{')
        || token.contains('}')
        || token.contains('[')
        || token.contains(']');
    email || url || has_digit || code_like
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_user_message_rejects_empty_input() {
        assert!(edit_user_message("   ").is_none());
    }

    #[test]
    fn edit_user_message_rejects_oversized_input() {
        assert!(edit_user_message(&"x".repeat(MAX_TRANSCRIPT_BYTES + 1)).is_none());
    }

    #[test]
    fn output_validation_accepts_punctuation_only_change() {
        assert!(output_is_valid("deploy API now", "Deploy API now."));
    }

    #[test]
    fn output_validation_rejects_dropped_word() {
        assert!(!output_is_valid("deploy the API now", "Deploy API now."));
    }

    #[test]
    fn output_validation_rejects_reordered_words() {
        assert!(!output_is_valid("deploy API now", "API deploy now."));
    }

    #[test]
    fn output_validation_rejects_changed_url() {
        assert!(!output_is_valid(
            "open https://example.com/path now",
            "Open https://example.org/path now."
        ));
    }

    #[test]
    fn output_validation_rejects_changed_email_number_and_code() {
        assert!(!output_is_valid(
            "email dev@example.com at 555-0100 run cargo_test",
            "Email dev@example.com at 555-0101 run cargo_test."
        ));
    }

    #[test]
    fn output_validation_rejects_model_labels_and_fences() {
        assert!(!output_is_valid("deploy now", "Edited: deploy now"));
        assert!(!output_is_valid("deploy now", "```\ndeploy now\n```"));
    }

    #[test]
    fn output_validation_rejects_changed_dictionary_term() {
        assert!(!output_is_valid_with_protected(
            "Open ShogunAI now",
            "Open Shogun now.",
            &["ShogunAI".into()]
        ));
    }

    #[test]
    fn output_validation_rejects_expansion_beyond_limit() {
        let raw = "word";
        assert!(!output_is_valid(raw, &"word ".repeat(200)));
    }
}
