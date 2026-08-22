//! Source trust prior and query-time [`effective_confidence`] (issue #35).
//!
//! Confidence on search hits used to be a constant `1.0`, so a calendar title and an AX
//! snippet looked equally certain to [`crate::score::score_block`]. Toru's rule is:
//!
//! ```text
//! effective_confidence = source_trust_prior × extraction_confidence × freshness_decay
//! ```
//!
//! This module is the formula. It does **not** persist a new column on state rows (that stays
//! a follow-up). Quoted search hits use extraction `1.0` because they are not running the
//! speech-act extractor; the prior is what stops AX / mail / calendar notes looking like
//! High-band facts. Structured calendar metadata bypasses the extractor (prior ≈ 0.9).
//!
//! Freshness *curves* (calendar until next sync, AX in seconds) are not wired yet: this slice
//! uses decay `1.0` so recency stays [`crate::block::ScoreInputs::freshness`] and is not
//! multiplied twice.

use crate::block::SourceKind;

/// Same ceiling as `shogun_memory::extract::LOCAL_RULE_MAX_CONFIDENCE`. Fusion cannot
/// depend on memory, so the number is duplicated here.
pub const EVIDENCE_TRUST_PRIOR: f64 = 0.4;

/// Structured API metadata (calendar title / time / id). Not a speech-act extract.
pub const STRUCTURED_TRUST_PRIOR: f64 = 0.9;

/// State-table rows that already passed FR-ST-20. Same order of magnitude as structured.
pub const STATE_FACT_TRUST_PRIOR: f64 = 0.9;

/// Stored session / thread summaries: useful, still a summary.
pub const SUMMARY_TRUST_PRIOR: f64 = 0.7;

/// Learned lessons shape content only; never L1/L2/L3.
pub const LESSON_TRUST_PRIOR: f64 = 0.5;

/// Trust prior for a fusion block's origin. Independent of whether the bytes arrived via MCP.
pub fn source_trust_prior(kind: SourceKind) -> f64 {
    match kind {
        SourceKind::Structured => STRUCTURED_TRUST_PRIOR,
        SourceKind::StateFact => STATE_FACT_TRUST_PRIOR,
        SourceKind::SessionSummary | SourceKind::ThreadSummary => SUMMARY_TRUST_PRIOR,
        SourceKind::Evidence => EVIDENCE_TRUST_PRIOR,
        SourceKind::Lesson => LESSON_TRUST_PRIOR,
    }
}

/// `source_trust_prior × extraction_confidence × freshness_decay`, each factor clamped to `[0, 1]`.
pub fn effective_confidence(
    source_trust_prior: f64,
    extraction_confidence: f64,
    freshness_decay: f64,
) -> f64 {
    let factor = |x: f64| x.clamp(0.0, 1.0);
    (factor(source_trust_prior) * factor(extraction_confidence) * factor(freshness_decay))
        .clamp(0.0, 1.0)
}

/// Query-time confidence for a quoted (or extractor-bypassed) block.
///
/// Extraction is `1.0`: calendar metadata skipped the local-rule extractor; AX / mail /
/// calendar descriptions are shown as quotes, not as extracted commitments. Decay is `1.0`
/// this slice (no last-sync / age on [`crate::block::ContextBlock`]).
pub fn query_time_confidence(kind: SourceKind) -> f64 {
    effective_confidence(source_trust_prior(kind), 1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_prior_is_high_and_evidence_matches_local_extract_cap() {
        assert!((source_trust_prior(SourceKind::Structured) - 0.9).abs() < 1e-9);
        assert!((source_trust_prior(SourceKind::Evidence) - 0.4).abs() < 1e-9);
        assert!(
            source_trust_prior(SourceKind::Structured) > source_trust_prior(SourceKind::Evidence)
        );
    }

    #[test]
    fn quoted_calendar_metadata_is_high_band_input_notes_are_not() {
        let cal = query_time_confidence(SourceKind::Structured);
        let notes = query_time_confidence(SourceKind::Evidence);
        assert!((cal - 0.9).abs() < 1e-9);
        assert!((notes - 0.4).abs() < 1e-9);
        assert!(
            cal >= 0.8,
            "structured metadata may be treated as fact in score_block"
        );
        assert!(
            notes < 0.5,
            "quoted notes must not look like High/Medium facts"
        );
    }

    #[test]
    fn local_extract_on_ax_stays_low_when_the_extractor_cap_is_applied() {
        let c = effective_confidence(EVIDENCE_TRUST_PRIOR, 0.4, 1.0);
        assert!((c - 0.16).abs() < 1e-9);
        assert!(c < 0.5);
    }

    #[test]
    fn factors_are_clamped() {
        assert_eq!(effective_confidence(2.0, 1.0, 1.0), 1.0);
        assert_eq!(effective_confidence(-1.0, 1.0, 1.0), 0.0);
        assert_eq!(effective_confidence(0.9, 2.0, 0.5), 0.45);
    }
}
