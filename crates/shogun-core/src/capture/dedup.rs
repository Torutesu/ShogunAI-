//! Near-duplicate capture collapse (FR-CAP-03). Pure decision logic, no macOS, no DB.
//!
//! Accessibility capture re-reads a window's whole text on many focus/scroll/keystroke events, so
//! the same body arrives again and again with tiny differences (a few characters typed, whitespace
//! reflow). FR-CAP-03 says a near-duplicate (~98% similar) must **not** become a new event — it
//! should dedup-touch the existing one. The event log keys dedup on `content_hash`; this module is
//! the piece that decides *which* hash a freshly captured body gets: the prior body's hash when
//! they are near-duplicates, a fresh hash otherwise.
//!
//! Similarity is the Sørensen–Dice coefficient over adjacent-character bigrams of the normalized
//! text — dependency-free, O(n), and robust to edits anywhere in the body (unlike a prefix compare).
//! The stored event always keeps its *original* content; normalization only feeds the comparison.

use std::collections::HashMap;

/// The near-duplicate threshold (FR-CAP-03: "~98% similarity"). At or above this, two bodies are
/// the same event.
pub const NEAR_DUP_THRESHOLD: f64 = 0.98;

/// Normalize for comparison only: lowercase, collapse any whitespace run to one space, trim. The
/// original text is what gets stored — this is purely to make the similarity robust to cosmetic
/// reflow.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_ws = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            in_ws = true;
        } else {
            if in_ws && !out.is_empty() {
                out.push(' ');
            }
            in_ws = false;
            out.extend(ch.to_lowercase());
        }
    }
    out
}

/// Multiset of adjacent-character bigrams of `s` (as counts), over `char`s so multibyte text
/// (Japanese) is handled correctly.
fn bigrams(s: &str) -> HashMap<(char, char), u32> {
    let chars: Vec<char> = s.chars().collect();
    let mut m = HashMap::new();
    for w in chars.windows(2) {
        *m.entry((w[0], w[1])).or_insert(0) += 1;
    }
    m
}

/// Sørensen–Dice similarity over character bigrams of the normalized strings, in `[0, 1]`.
/// Two identical normalized strings score 1.0; strings too short to form a bigram fall back to an
/// exact-equality check (1.0 if equal, else 0.0).
pub fn similarity(a: &str, b: &str) -> f64 {
    let (na, nb) = (normalize(a), normalize(b));
    if na == nb {
        return 1.0;
    }
    let (ba, bb) = (bigrams(&na), bigrams(&nb));
    let total: u32 = ba.values().sum::<u32>() + bb.values().sum::<u32>();
    if total == 0 {
        // both had <2 chars and were not equal
        return 0.0;
    }
    let mut overlap = 0u32;
    for (bg, ca) in &ba {
        if let Some(cb) = bb.get(bg) {
            overlap += (*ca).min(*cb);
        }
    }
    (2 * overlap) as f64 / total as f64
}

/// A recent event offered as a dedup candidate: its stored hash and content.
#[derive(Debug, Clone, Copy)]
pub struct Recent<'a> {
    pub content_hash: &'a str,
    pub content: &'a str,
}

/// The outcome of a dedup decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HashDecision {
    /// `new_content` is a near-duplicate of an existing event — reuse its hash (dedup-touch).
    Duplicate(String),
    /// `new_content` is novel — use this freshly computed hash (a new event).
    Fresh(String),
}

impl HashDecision {
    /// The chosen content_hash, whichever branch.
    pub fn hash(&self) -> &str {
        match self {
            HashDecision::Duplicate(h) | HashDecision::Fresh(h) => h,
        }
    }

    /// Whether the decision was a near-duplicate.
    pub fn is_duplicate(&self) -> bool {
        matches!(self, HashDecision::Duplicate(_))
    }
}

/// Decide the content_hash for `new_content`. If the most-similar recent candidate is at or above
/// [`NEAR_DUP_THRESHOLD`], reuse that candidate's hash (the event log will dedup-touch, FR-CAP-03);
/// otherwise compute a fresh hash with `hash_fn`. Ties pick the highest similarity, then the first
/// candidate in order — callers pass `recents` newest-first (`ORDER BY id DESC`), so a tie
/// dedup-touches the *newest* matching event rather than reviving a stale one.
pub fn decide_hash(
    new_content: &str,
    recents: &[Recent<'_>],
    hash_fn: impl Fn(&str) -> String,
) -> HashDecision {
    // `max_by` would keep the *last* maximal element, i.e. the oldest row, so `last_seen_at` /
    // `dwell_ms` would pile onto a stale event whenever two candidates score the same (e.g. two
    // prior bodies that normalize equal, both scoring exactly 1.0). Reduce with a strict `>` so
    // the incumbent — the earlier, newer candidate — survives a tie. No `partial_cmp`/`unwrap`
    // needed: a NaN similarity never clears the `>= NEAR_DUP_THRESHOLD` filter, and a strict `>`
    // against one would keep the incumbent anyway.
    let best = recents
        .iter()
        .map(|r| (r, similarity(new_content, r.content)))
        .filter(|(_, s)| *s >= NEAR_DUP_THRESHOLD)
        .reduce(|best, cand| if cand.1 > best.1 { cand } else { best });
    match best {
        Some((r, _)) => HashDecision::Duplicate(r.content_hash.to_string()),
        None => HashDecision::Fresh(hash_fn(new_content)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_hash(s: &str) -> String {
        // a stand-in stable hash for tests (the daemon injects the real xxhash)
        format!("hash:{}", s.len())
    }

    #[test]
    fn normalize_collapses_whitespace_and_case() {
        assert_eq!(normalize("  The\tQuick\n  Brown  "), "the quick brown");
    }

    #[test]
    fn identical_text_is_fully_similar() {
        assert_eq!(similarity("hello world", "hello world"), 1.0);
        // differs only by whitespace/case → still identical after normalize
        assert_eq!(similarity("Hello   World", "hello world"), 1.0);
    }

    #[test]
    fn a_few_appended_chars_stay_above_threshold() {
        let base: String = "the quarterly roadmap review meeting notes ".repeat(12);
        let typed = format!("{base}x"); // one keystroke later
        let s = similarity(&base, &typed);
        assert!(s >= NEAR_DUP_THRESHOLD, "near-dup similarity too low: {s}");
    }

    #[test]
    fn different_text_is_well_below_threshold() {
        let s = similarity(
            "the quarterly roadmap review meeting",
            "lunch plans for saturday afternoon downtown",
        );
        assert!(s < 0.5, "unrelated text should be dissimilar: {s}");
    }

    #[test]
    fn decide_reuses_hash_for_near_duplicate() {
        let base: String = "sprint planning board with many tickets and notes ".repeat(10);
        let typed = format!("{base}!");
        let recents = [Recent { content_hash: "abc123", content: &base }];
        let decision = decide_hash(&typed, &recents, fresh_hash);
        assert!(decision.is_duplicate());
        assert_eq!(decision.hash(), "abc123", "near-dup must reuse the prior hash → dedup-touch");
    }

    #[test]
    fn decide_makes_fresh_hash_for_novel_content() {
        let recents = [Recent { content_hash: "abc123", content: "an old unrelated window body" }];
        let decision = decide_hash("a totally different fresh capture", &recents, fresh_hash);
        assert!(!decision.is_duplicate());
        assert_eq!(decision.hash(), fresh_hash("a totally different fresh capture"));
    }

    #[test]
    fn decide_picks_the_most_similar_candidate() {
        let target: String = "weekly one-on-one agenda: growth, blockers, feedback ".repeat(8);
        let near = format!("{target}."); // ~identical
        let far = "completely unrelated scratchpad".to_string();
        let recents = [
            Recent { content_hash: "far", content: &far },
            Recent { content_hash: "near", content: &target },
        ];
        let decision = decide_hash(&near, &recents, fresh_hash);
        assert_eq!(decision.hash(), "near");
    }

    #[test]
    fn tied_candidates_pick_the_first_newest_one() {
        // `recents` arrives newest-first (ORDER BY id DESC). Two prior bodies that differ only by
        // whitespace/case normalize equal, so both score exactly 1.0 against the new capture. The
        // tie must dedup-touch the newest event — otherwise last_seen_at/dwell_ms accumulate on a
        // stale row.
        let newest = "Sprint  Planning\tBoard";
        let oldest = "sprint planning board";
        let new_content = "SPRINT PLANNING BOARD";
        assert_eq!(similarity(new_content, newest), similarity(new_content, oldest));
        let recents = [
            Recent { content_hash: "newest", content: newest },
            Recent { content_hash: "oldest", content: oldest },
        ];
        let decision = decide_hash(new_content, &recents, fresh_hash);
        assert!(decision.is_duplicate());
        assert_eq!(decision.hash(), "newest", "a tie must collapse onto the newest candidate");
    }

    #[test]
    fn no_recents_is_always_fresh() {
        let decision = decide_hash("anything", &[], fresh_hash);
        assert!(!decision.is_duplicate());
    }

    #[test]
    fn japanese_text_bigrams_are_char_aware() {
        // near-identical Japanese bodies (one appended char) collapse; unrelated ones don't.
        let base: String = "四半期のロードマップ会議の議事録です。".repeat(6);
        let typed = format!("{base}あ");
        assert!(similarity(&base, &typed) >= NEAR_DUP_THRESHOLD);
        assert!(similarity(&base, "全く関係のない別の文章").abs() < 0.5);
    }
}
