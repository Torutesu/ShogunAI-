/// Longest sequence of terms sent to FTS. A pathological paste should not turn into a thousand-
/// clause MATCH; the leading terms carry the question anyway.
pub(crate) const MAX_FTS_TERMS: usize = 24;

/// Build the FTS5 MATCH expression for a user's question.
///
/// Quoting matters twice over. Unquoted, a question containing `-`, `*`, `OR` or `NEAR` is parsed
/// as FTS operators and errors or silently means something else. But quoting the *whole* question
/// makes it a single phrase, which only matches text containing those words contiguously — and a
/// question almost never appears verbatim in the answer ("vendor renewal pricing" does not occur
/// in "the vendor renewal discussion continued; pricing was raised"). That returned nothing for
/// essentially every multi-word question.
///
/// So each term is quoted separately and combined with OR: any term can match, and bm25 ranks a
/// document that matches more — and rarer — terms higher, which is the behaviour a question wants.
///
/// The index uses a trigram tokenizer, so a term shorter than three characters cannot match
/// anything and is dropped. For CJK, which has no spaces to split on, a long run is expanded into
/// its overlapping trigrams — that is how a trigram index is queried for those scripts, and it is
/// the same OR-of-terms shape.
///
/// Extract searchable terms from a user's question (same rules as [`fts_query`]).
///
/// Shared by event FTS and meeting-table retrieval so both halves of hybrid search speak the
/// same vocabulary.
pub fn lexical_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for raw in query.split(|c: char| !c.is_alphanumeric()) {
        if terms.len() >= MAX_FTS_TERMS {
            break;
        }
        let chars: Vec<char> = raw.chars().collect();
        if chars.len() < 3 {
            continue;
        }
        if chars.iter().any(|c| is_cjk(*c)) {
            // No word boundaries to rely on: match on the run's trigrams.
            for w in chars.windows(3) {
                if terms.len() >= MAX_FTS_TERMS {
                    break;
                }
                terms.push(w.iter().collect::<String>());
            }
        } else if !is_stopword(raw) {
            terms.push(raw.to_string());
        }
    }
    terms
}

/// Returns `None` when nothing usable is left, which the caller treats as "no results" rather than
/// running an empty MATCH.
pub(crate) fn fts_query(query: &str) -> Option<String> {
    let terms = lexical_terms(query);
    if terms.is_empty() {
        // A question made only of function words has nothing to retrieve on. Returning None is
        // honest: matching every document via "the" would fill the result budget with noise and
        // push out anything real.
        return None;
    }
    Some(
        terms
            .into_iter()
            .map(|t| quote(&t))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

/// English function words, which match nearly every document and so only crowd out real hits.
///
/// bm25 already scores them near zero, but scoring is not the problem — the result *limit* is:
/// a handful of slots filled by documents that merely contain "the" are slots a real match
/// cannot have. Kept deliberately small and closed-class; anything with topical meaning stays.
fn is_stopword(term: &str) -> bool {
    const STOPWORDS: &[&str] = &[
        "the", "and", "for", "are", "was", "were", "you", "your", "our", "their", "his", "her",
        "its", "that", "this", "these", "those", "with", "from", "into", "about", "what", "when",
        "where", "which", "who", "whom", "how", "why", "did", "does", "done", "have", "has", "had",
        "can", "could", "would", "should", "will", "shall", "may", "might", "not", "but", "any",
        "all", "some", "there", "here", "then", "than", "them", "they", "she", "him", "get", "got",
    ];
    let lower = term.to_ascii_lowercase();
    STOPWORDS.contains(&lower.as_str())
}

fn quote(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x309F | 0x30A0..=0x30FF | 0x4E00..=0x9FFF | 0xFF66..=0xFF9F
    )
}
