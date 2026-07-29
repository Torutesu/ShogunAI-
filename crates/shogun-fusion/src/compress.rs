//! 圧縮の統括（Issue #63 設計 §3.4）。収集済み候補ブロックを score → 予算充填し、
//! 圧縮済みコンテキストを返す。**LLM を呼ばない**（クエリ時ローカル処理）。
//!
//! thread が予算を圧迫する場合の「raw turns → thread summary 差し替え」は、候補生成の時点で
//! daemon が両方（raw evidence と thread summary ブロック）を候補に入れておき、スコアと予算で
//! 自然に summary が選ばれる形にする。summary は evidence より短くトークン効率が高いので、
//! 同等スコアなら予算内に収まりやすい。

use crate::block::{BlockRef, ContextBlock};
use crate::budget::fit_to_budget;
use crate::score::{score_block, ScoreWeights};

/// 圧縮モード。v1 は Balanced のみ出荷（enum は将来拡張の余地）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionMode {
    Balanced,
}

/// 圧縮の設定。フラグ・予算・モード・重み。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressionConfig {
    /// フィーチャーフラグ。false のとき daemon は raw パスを使う（この関数は呼ばれない）。
    pub enabled: bool,
    pub budget_tokens: usize,
    pub mode: CompressionMode,
    pub weights: ScoreWeights,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: false, // 既定 off。ヘビーユーザー/AB でのみ有効化。
            budget_tokens: 2000,
            mode: CompressionMode::Balanced,
            weights: ScoreWeights::default(),
        }
    }
}

/// 圧縮の計測値。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompressionStats {
    pub pre_tokens: usize,
    pub post_tokens: usize,
    pub dropped: usize,
}

/// 圧縮済みコンテキスト。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompressedContext {
    /// 採用ブロック（スコア降順）。
    pub blocks: Vec<ContextBlock>,
    /// 採用ブロックの provenance 参照。
    pub refs: Vec<BlockRef>,
    pub stats: CompressionStats,
}

/// 収集済み候補。daemon が memory 行から正規化して渡す。
#[derive(Debug, Clone, Default)]
pub struct Candidates {
    pub blocks: Vec<ContextBlock>,
}

/// 候補をスコア付けし予算に収める。純粋・I/O なし。
pub fn compress(candidates: Candidates, config: &CompressionConfig) -> CompressedContext {
    let scored: Vec<(ContextBlock, f64)> = candidates
        .blocks
        .into_iter()
        .map(|b| {
            let s = score_block(&b.score_inputs, &config.weights);
            (b, s)
        })
        .collect();

    let fit = fit_to_budget(scored, config.budget_tokens);
    let refs = fit.kept.iter().map(|b| b.id_ref.clone()).collect();
    CompressedContext {
        stats: CompressionStats {
            pre_tokens: fit.pre_tokens,
            post_tokens: fit.post_tokens,
            dropped: fit.dropped.len(),
        },
        blocks: fit.kept,
        refs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{ScoreInputs, SourceKind};
    use crate::budget::HeuristicEstimator;

    fn blk(id: i64, chars: usize, rel: f64) -> ContextBlock {
        let est = HeuristicEstimator::default();
        ContextBlock::new(
            BlockRef::Event(id),
            SourceKind::Evidence,
            "a".repeat(chars),
            ScoreInputs { relevance: rel, freshness: 0.5, task_link: 0.0, confidence: 1.0 },
            &est,
        )
    }

    #[test]
    fn compress_respects_budget_and_prefers_relevant() {
        let cands = Candidates { blocks: vec![blk(1, 400, 0.1), blk(2, 400, 0.9)] };
        // 400 ラテン ≈ 100 tokens。予算 120 なら 1 ブロックだけ入る。
        let cfg = CompressionConfig { enabled: true, budget_tokens: 120, ..Default::default() };
        let out = compress(cands, &cfg);
        assert_eq!(out.blocks.len(), 1);
        assert_eq!(out.blocks[0].id_ref, BlockRef::Event(2)); // 関連度が高い方
        assert!(out.stats.post_tokens <= 120);
        assert_eq!(out.stats.dropped, 1);
    }

    #[test]
    fn refs_match_kept_blocks() {
        let cands = Candidates { blocks: vec![blk(1, 40, 0.5)] };
        let cfg = CompressionConfig { enabled: true, budget_tokens: 1000, ..Default::default() };
        let out = compress(cands, &cfg);
        assert_eq!(out.refs, vec![BlockRef::Event(1)]);
    }
}
