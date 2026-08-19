//! ブロックスコアリング（Issue #63 設計 §3.2）。4 要素の重み付き線形和。
//!
//! 初期重みは既存 thread `salience`（lexical .30 / on_screen .20 / recency .25 / pressure .25）
//! を出発点に、クエリ関連をやや上げた値。計測後に校正する。

use crate::block::{ScoreInputs, SourceKind};

/// スコア要素の重み。合計は 1.0 に正規化して使う。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreWeights {
    pub rel: f64,
    pub fresh: f64,
    pub task: f64,
    pub conf: f64,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self { rel: 0.35, fresh: 0.25, task: 0.20, conf: 0.20 }
    }
}

/// 4 要素の重み付き線形和。各入力は [0,1] にクランプされる。
pub fn score_block(inputs: &ScoreInputs, w: &ScoreWeights) -> f64 {
    let c = |x: f64| x.clamp(0.0, 1.0);
    w.rel * c(inputs.relevance)
        + w.fresh * c(inputs.freshness)
        + w.task * c(inputs.task_link)
        + w.conf * c(inputs.confidence)
}

/// Trust rank for a block's origin (issue #35). Higher wins ties when [`score_block`] is equal.
///
/// 1. MCP/API structured facts
/// 2. session / thread summary (meeting text with provenance)
/// 3. world-model [`SourceKind::StateFact`]
/// 4. AX / search [`SourceKind::Evidence`]
/// 5. everything else (lessons, …)
pub fn source_rank(kind: SourceKind) -> u8 {
    match kind {
        SourceKind::Structured => 4,
        SourceKind::SessionSummary | SourceKind::ThreadSummary => 3,
        SourceKind::StateFact => 2,
        SourceKind::Evidence => 1,
        SourceKind::Lesson => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(rel: f64, fresh: f64, task: f64, conf: f64) -> ScoreInputs {
        ScoreInputs { relevance: rel, freshness: fresh, task_link: task, confidence: conf }
    }

    #[test]
    fn higher_relevance_scores_higher() {
        let w = ScoreWeights::default();
        let lo = score_block(&inputs(0.1, 0.5, 0.5, 0.5), &w);
        let hi = score_block(&inputs(0.9, 0.5, 0.5, 0.5), &w);
        assert!(hi > lo);
    }

    #[test]
    fn all_max_scores_to_sum_of_weights() {
        let w = ScoreWeights::default();
        let s = score_block(&inputs(1.0, 1.0, 1.0, 1.0), &w);
        assert!((s - (w.rel + w.fresh + w.task + w.conf)).abs() < 1e-9);
    }

    #[test]
    fn out_of_range_inputs_are_clamped() {
        let w = ScoreWeights::default();
        let s = score_block(&inputs(5.0, -1.0, 0.5, 0.5), &w);
        let clamped = score_block(&inputs(1.0, 0.0, 0.5, 0.5), &w);
        assert!((s - clamped).abs() < 1e-9);
    }

    #[test]
    fn source_rank_orders_structured_above_ax_evidence() {
        assert!(source_rank(SourceKind::Structured) > source_rank(SourceKind::SessionSummary));
        assert_eq!(
            source_rank(SourceKind::SessionSummary),
            source_rank(SourceKind::ThreadSummary)
        );
        assert!(source_rank(SourceKind::SessionSummary) > source_rank(SourceKind::StateFact));
        assert!(source_rank(SourceKind::StateFact) > source_rank(SourceKind::Evidence));
        assert!(source_rank(SourceKind::Evidence) > source_rank(SourceKind::Lesson));
    }
}
