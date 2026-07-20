//! Confidence bands and prompt treatment (FR-ST-20 / §6.4.6).
//!
//! The single implementation of "how may a state record appear in a generation, given its
//! confidence". A CLAUDE.md invariant rides on this: **low-confidence state must never be mixed
//! into an output as fact**. The band boundaries and the rendered treatment are defined once
//! here; Context Fusion's prompt assembly and every agent go through [`assemble_facts`].
//!
//! | confidence   | band   | treatment                                             |
//! |--------------|--------|-------------------------------------------------------|
//! | 0.8 ..= 1.0  | High   | usable as fact                                        |
//! | 0.5 ..< 0.8  | Medium | passed weakly, prefixed `possibly:` — never assertive |
//! | 0.0 ..< 0.5  | Low    | excluded from generations/action decisions entirely   |

/// The confidence band a record falls in (FR-ST-20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    High,
    Medium,
    Low,
}

/// Classify a confidence value into its band. Values are clamped to [0, 1] first, so a
/// malformed confidence can never land a record in a stronger band than High.
pub fn band(confidence: f64) -> Band {
    let c = confidence.clamp(0.0, 1.0);
    if c >= 0.8 {
        Band::High
    } else if c >= 0.5 {
        Band::Medium
    } else {
        Band::Low
    }
}

/// How a fact may be rendered into a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Treatment {
    /// Include verbatim as a fact (High).
    Fact(String),
    /// Include weakly, `possibly:`-prefixed, never as an assertion (Medium).
    Possible(String),
    /// Do not include in any generation or action decision (Low).
    Excluded,
}

/// The mandatory `possibly:` marker for medium-confidence facts (FR-ST-20).
pub const POSSIBLY_PREFIX: &str = "possibly: ";

/// Render one fact according to its confidence (FR-ST-20). Low-confidence facts return
/// [`Treatment::Excluded`] and carry no text — the caller cannot accidentally surface the text.
pub fn treat_fact(fact: &str, confidence: f64) -> Treatment {
    match band(confidence) {
        Band::High => Treatment::Fact(fact.to_string()),
        Band::Medium => Treatment::Possible(format!("{POSSIBLY_PREFIX}{fact}")),
        Band::Low => Treatment::Excluded,
    }
}

/// Assemble `(fact, confidence)` pairs into prompt-ready lines. Low-confidence facts are
/// dropped entirely; medium ones are `possibly:`-prefixed; high ones pass through. This is the
/// choke point that guarantees no low-confidence state reaches a generation as fact.
pub fn assemble_facts(facts: &[(&str, f64)]) -> Vec<String> {
    facts
        .iter()
        .filter_map(|(fact, c)| match treat_fact(fact, *c) {
            Treatment::Fact(s) | Treatment::Possible(s) => Some(s),
            Treatment::Excluded => None,
        })
        .collect()
}

/// True if a confidence is high enough to drive an action decision (High or Medium; Low never
/// drives actions or generations, FR-ST-20). Medium may propose but must be surfaced weakly.
pub fn may_inform_action(confidence: f64) -> bool {
    !matches!(band(confidence), Band::Low)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_boundaries_match_spec() {
        assert_eq!(band(1.0), Band::High);
        assert_eq!(band(0.8), Band::High); // 0.8 is High (inclusive)
        assert_eq!(band(0.79), Band::Medium);
        assert_eq!(band(0.5), Band::Medium); // 0.5 is Medium (inclusive)
        assert_eq!(band(0.49), Band::Low);
        assert_eq!(band(0.0), Band::Low);
    }

    #[test]
    fn out_of_range_confidence_is_clamped() {
        assert_eq!(band(2.0), Band::High);
        assert_eq!(band(-1.0), Band::Low);
    }

    #[test]
    fn high_confidence_is_a_bare_fact() {
        assert_eq!(treat_fact("Alice owns the roadmap", 0.9), Treatment::Fact("Alice owns the roadmap".into()));
    }

    #[test]
    fn medium_confidence_is_possibly_prefixed() {
        assert_eq!(
            treat_fact("Bob is waiting on a reply", 0.6),
            Treatment::Possible("possibly: Bob is waiting on a reply".into())
        );
    }

    #[test]
    fn low_confidence_is_excluded_and_carries_no_text() {
        // The secret guarantee: a low-confidence fact returns Excluded with no text attached, so
        // it cannot be surfaced by accident.
        assert_eq!(treat_fact("shaky guess about a deadline", 0.3), Treatment::Excluded);
    }

    #[test]
    fn assemble_drops_low_and_marks_medium() {
        let facts = [
            ("high fact", 0.95),
            ("medium fact", 0.65),
            ("low fact", 0.2),
        ];
        let lines = assemble_facts(&facts);
        assert_eq!(lines, vec!["high fact".to_string(), "possibly: medium fact".to_string()]);
        // The low fact's text appears nowhere.
        assert!(!lines.iter().any(|l| l.contains("low fact")));
    }

    #[test]
    fn low_confidence_never_informs_actions() {
        assert!(may_inform_action(0.8));
        assert!(may_inform_action(0.5));
        assert!(!may_inform_action(0.49));
        assert!(!may_inform_action(0.0));
    }
}
