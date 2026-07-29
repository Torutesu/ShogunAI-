//! ローカルなトークン推定と予算充填（Issue #63）。
//!
//! `shogun-fusion` はクラウド/ONNX に依存しない純粋クレートなので、トークン数は言語別の
//! char→token 比のヒューリスティックで見積もる（±10% で予算管理には十分）。正確な
//! トークナイザが必要になったら [`TokenEstimator`] を差し替える。

/// テキストのトークン数を見積もる seam。
pub trait TokenEstimator {
    fn count(&self, text: &str) -> usize;
}

/// 言語別 char→token 比のローカル推定器。CJK は文字あたりのトークンが多く、ラテンは
/// 単語あたり複数文字なので、CJK 文字比率で 2 つの比率を線形補間する。
#[derive(Debug, Clone, Copy)]
pub struct HeuristicEstimator {
    /// ラテン系: 1 トークンあたりの文字数（概ね 4）。
    latin_chars_per_token: f64,
    /// CJK 系: 1 トークンあたりの文字数（概ね 1.5）。
    cjk_chars_per_token: f64,
}

impl Default for HeuristicEstimator {
    fn default() -> Self {
        Self { latin_chars_per_token: 4.0, cjk_chars_per_token: 1.5 }
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF |   // ひらがな・カタカナ
        0x3400..=0x4DBF |   // CJK 拡張A
        0x4E00..=0x9FFF |   // CJK 統合漢字
        0xFF00..=0xFFEF)    // 全角
}

impl TokenEstimator for HeuristicEstimator {
    fn count(&self, text: &str) -> usize {
        let total = text.chars().count();
        if total == 0 {
            return 0;
        }
        let cjk = text.chars().filter(|c| is_cjk(*c)).count();
        let cjk_ratio = cjk as f64 / total as f64;
        let chars_per_token =
            self.cjk_chars_per_token * cjk_ratio + self.latin_chars_per_token * (1.0 - cjk_ratio);
        (total as f64 / chars_per_token).ceil() as usize
    }
}

use crate::block::{BlockRef, ContextBlock};

/// 予算充填の結果。
#[derive(Debug, Clone, PartialEq)]
pub struct FitResult {
    /// 予算内に採用したブロック（入力のスコア順を保つ）。
    pub kept: Vec<ContextBlock>,
    /// 落としたブロックの参照（計測・再展開用）。
    pub dropped: Vec<BlockRef>,
    pub pre_tokens: usize,
    pub post_tokens: usize,
}

/// `scored`（ブロック, スコア）を**スコア降順**に並べ、累積トークンが `budget_tokens` を
/// 超えない範囲で採用する。超えたブロックは採用せず `dropped` に回す（後続の低スコアで
/// 予算に収まるものは採用する = best-effort な充填）。
///
/// 不変: 返る `post_tokens <= budget_tokens`。高スコアほど優先採用。provenance は保持。
pub fn fit_to_budget(mut scored: Vec<(ContextBlock, f64)>, budget_tokens: usize) -> FitResult {
    let pre_tokens: usize = scored.iter().map(|(b, _)| b.tokens).sum();
    // スコア降順（同点は入力順を保つよう安定ソート）。
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    let mut used = 0usize;
    for (block, _score) in scored {
        if used + block.tokens <= budget_tokens {
            used += block.tokens;
            kept.push(block);
        } else {
            dropped.push(block.id_ref);
        }
    }
    FitResult { kept, dropped, pre_tokens, post_tokens: used }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_zero_tokens() {
        assert_eq!(HeuristicEstimator::default().count(""), 0);
    }

    #[test]
    fn latin_uses_about_four_chars_per_token() {
        // 40 文字のラテン → 約 10 トークン。
        let s = "a".repeat(40);
        let n = HeuristicEstimator::default().count(&s);
        assert!((9..=11).contains(&n), "got {n}");
    }

    #[test]
    fn cjk_costs_more_tokens_than_latin_for_same_length() {
        let est = HeuristicEstimator::default();
        let latin = est.count(&"a".repeat(30));
        let cjk = est.count(&"あ".repeat(30));
        assert!(cjk > latin, "cjk={cjk} latin={latin}");
    }

    use crate::block::{BlockRef, ScoreInputs, SourceKind};

    fn blk(id: i64, tokens_text: usize) -> ContextBlock {
        // tokens は text から算出されるので、ラテン文字を tokens_text*4 個並べて概算調整。
        let est = HeuristicEstimator::default();
        ContextBlock::new(
            BlockRef::Event(id),
            SourceKind::Evidence,
            "a".repeat(tokens_text * 4),
            ScoreInputs { relevance: 0.5, freshness: 0.5, task_link: 0.0, confidence: 1.0 },
            &est,
        )
    }

    #[test]
    fn never_exceeds_budget() {
        let scored = vec![(blk(1, 10), 0.9), (blk(2, 10), 0.8), (blk(3, 10), 0.7)];
        let r = fit_to_budget(scored, 15);
        assert!(r.post_tokens <= 15, "post={}", r.post_tokens);
    }

    #[test]
    fn keeps_higher_score_first() {
        let scored = vec![(blk(1, 10), 0.2), (blk(2, 10), 0.9)];
        let r = fit_to_budget(scored, 10);
        assert_eq!(r.kept.len(), 1);
        assert_eq!(r.kept[0].id_ref, BlockRef::Event(2)); // 高スコアが残る
        assert_eq!(r.dropped, vec![BlockRef::Event(1)]);
    }

    #[test]
    fn all_fit_when_budget_large() {
        let scored = vec![(blk(1, 5), 0.5), (blk(2, 5), 0.5)];
        let r = fit_to_budget(scored, 1000);
        assert_eq!(r.kept.len(), 2);
        assert!(r.dropped.is_empty());
    }
}
