//! LLM 送信単位の正規化ブロック（Issue #63 設計 §3.1）。
//!
//! `shogun-fusion` は shogun-memory に依存しない純粋クレートなので、ここではプリミティブしか
//! 扱わない。SearchHit/ThreadRow/state facts → [`ContextBlock`] の変換は daemon 側で行う。

use crate::budget::TokenEstimator;

/// 生ログへの参照（再展開・provenance 用）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BlockRef {
    /// `event_log.id`
    Event(i64),
    /// `threads.thread_key`
    Thread(String),
    /// `sessions.id`
    Session(i64),
    /// state テーブルの行
    State { table: StateTable, id: i64 },
    /// `lessons.id`（L5 注入、Plan D-5）
    Lesson(i64),
}

/// provenance が指す state テーブル。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StateTable {
    People,
    Projects,
    Commitments,
    OpenLoops,
}

/// ブロックの由来。予算充填時の差し替え判断に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    StateFact,
    Evidence,
    ThreadSummary,
    SessionSummary,
    Structured,
    /// L5 の学習済み lesson（instruction 1文、Plan D-5）。
    Lesson,
}

/// スコアリングの生入力（各 0.0..=1.0）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreInputs {
    /// クエリ関連（search score / salience.lexical 由来）。
    pub relevance: f64,
    /// 鮮度（recency 半減）。
    pub freshness: f64,
    /// タスク紐づき圧（open_loops/project 由来、salience.pressure 相当）。
    pub task_link: f64,
    /// 確信度。[`crate::trust::effective_confidence`] on search hits; High-band 0.9 on gated
    /// state facts. Quoted evidence is no longer a fake `1.0`.
    pub confidence: f64,
}

/// LLM に送る正規化ブロック。
#[derive(Debug, Clone, PartialEq)]
pub struct ContextBlock {
    pub id_ref: BlockRef,
    pub source_kind: SourceKind,
    pub text: String,
    pub score_inputs: ScoreInputs,
    /// 推定トークン数（[`ContextBlock::new`] で計上済み）。
    pub tokens: usize,
}

impl ContextBlock {
    /// テキストのトークン数を推定器で計上してブロックを作る。
    pub fn new(
        id_ref: BlockRef,
        source_kind: SourceKind,
        text: impl Into<String>,
        score_inputs: ScoreInputs,
        est: &dyn TokenEstimator,
    ) -> Self {
        let text = text.into();
        let tokens = est.count(&text);
        Self { id_ref, source_kind, text, score_inputs, tokens }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::HeuristicEstimator;

    #[test]
    fn new_counts_tokens_from_text() {
        let est = HeuristicEstimator::default();
        let b = ContextBlock::new(
            BlockRef::Event(42),
            SourceKind::Evidence,
            "a".repeat(40),
            ScoreInputs { relevance: 0.5, freshness: 0.5, task_link: 0.0, confidence: 1.0 },
            &est,
        );
        assert_eq!(b.tokens, 10); // 40 latin chars / 4 = 10
        assert_eq!(b.id_ref, BlockRef::Event(42));
    }
}
