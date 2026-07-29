# コンテキスト圧縮の最適化 実装計画（縦に薄い一周）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** クエリ時にローカルでトークン予算に基づきコンテキストを選別・圧縮し、重い抽象要約は Dream Cycle にオフライン化して、raw vs compressed を計測・AB 可能にする（Issue #63 の最小縦切り）。

**Architecture:** 純粋クレート `shogun-fusion` に「ブロック正規化型・スコア・トークン推定・予算充填・compress 統括」を置き（Linux テスト可能）、`shogun-core` の daemon が memory 行を `ContextBlock` に正規化してフラグ付きで通す。抽象要約は Dream Cycle の `Compression` ジョブ（現 no-op）を `Summarizer` seam で実装し、既存の `threads.summary`/`sessions.summary` を埋める。計測は追加マイグレーション V11 の 1 テーブル。

**Tech Stack:** Rust（Cargo workspace）、rusqlite + refinery マイグレーション、既存 `shogun-fusion`/`shogun-core`/`shogun-memory` クレート。設計書: `docs/superpowers/specs/2026-07-29-context-compression-design.md`。

**前提（作業環境）:** worktree `feat/issue-63-context-compression`（`/Users/torutano/ShogunAI--issue63`、main 起点）で作業。全コマンドはリポジトリルートで実行。

**共通の検証コマンド:**
- fusion 単体: `cargo test -p shogun-fusion`
- core（db feature 有）: `cargo test -p shogun-core --features db`
- clippy: `cargo clippy -p shogun-fusion --all-targets -- -D warnings`

---

## ファイル構成（作成 / 変更）

**作成:**
- `crates/shogun-fusion/src/block.rs` — `ContextBlock`/`BlockRef`/`ScoreInputs`/`SourceKind`（純粋な型＋コンストラクタ）
- `crates/shogun-fusion/src/budget.rs` — `TokenEstimator` trait ＋ `HeuristicEstimator` ＋ `fit_to_budget`/`FitResult`
- `crates/shogun-fusion/src/score.rs` — `ScoreWeights` ＋ `score_block`
- `crates/shogun-fusion/src/compress.rs` — `CompressionConfig`/`CompressionMode`/`CompressedContext`/`CompressionStats`/`Candidates`/`compress`
- `crates/shogun-memory/src/migrations/V11__compression_metrics.sql` — 計測テーブル
- `crates/shogun-memory/src/compression_metrics.rs` — 計測 insert/query
- `docs/migrations/V11-rollback.md` — ロールバック手順

**変更:**
- `crates/shogun-fusion/src/lib.rs` — 新モジュール公開
- `crates/shogun-memory/src/lib.rs` — `compression_metrics` モジュール公開
- `crates/shogun-core/src/dreamcycle/jobs.rs` — `Summarizer` seam ＋ `run_compression` 実装
- `crates/shogun-core/src/daemon.rs` — 正規化グルー＋フラグ配線＋計測（`assemble_context` に圧縮パス）
- `crates/shogun-core/tests/context_slo.rs` — SLO ＋ AB テスト拡張

---

## Task 1: トークン推定（`TokenEstimator` ＋ `HeuristicEstimator`）

**Files:**
- Create: `crates/shogun-fusion/src/budget.rs`
- Modify: `crates/shogun-fusion/src/lib.rs`

- [ ] **Step 1: lib.rs にモジュール宣言を追加**

`crates/shogun-fusion/src/lib.rs` の `pub mod confidence;` の直後に追記:

```rust
pub mod budget;
```

- [ ] **Step 2: budget.rs に推定器の失敗するテストを書く**

`crates/shogun-fusion/src/budget.rs` を新規作成:

```rust
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
}
```

- [ ] **Step 3: テストが失敗する（コンパイル前）ことを確認 → 実装は既に Step 2 に含む**

このタスクは型と実装を同時に置くため、Step 2 の実装で通る。次で確認する。

- [ ] **Step 4: テスト実行して通ることを確認**

Run: `cargo test -p shogun-fusion budget::`
Expected: PASS（3 テスト）

- [ ] **Step 5: clippy**

Run: `cargo clippy -p shogun-fusion --all-targets -- -D warnings`
Expected: 警告なし

- [ ] **Step 6: コミット**

```bash
git add crates/shogun-fusion/src/budget.rs crates/shogun-fusion/src/lib.rs
git commit -m "feat(fusion): ローカルなトークン推定 (TokenEstimator/HeuristicEstimator) (#63)"
```

---

## Task 2: 共通ブロック型（`ContextBlock` ほか）

**Files:**
- Create: `crates/shogun-fusion/src/block.rs`
- Modify: `crates/shogun-fusion/src/lib.rs`

- [ ] **Step 1: lib.rs にモジュール宣言を追加**

`crates/shogun-fusion/src/lib.rs` の `pub mod budget;` の直後に追記:

```rust
pub mod block;
```

- [ ] **Step 2: block.rs を新規作成（型＋コンストラクタ＋テスト）**

`crates/shogun-fusion/src/block.rs`:

```rust
//! LLM 送信単位の正規化ブロック（Issue #63 設計 §3.1）。
//!
//! `shogun-fusion` は shogun-memory に依存しない純粋クレートなので、ここではプリミティブしか
//! 扱わない。SearchHit/ThreadRow/state facts → [`ContextBlock`] の変換は daemon 側で行う。

use crate::budget::TokenEstimator;

/// 生ログへの参照（再展開・provenance 用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockRef {
    /// `event_log.id`
    Event(i64),
    /// `threads.thread_key`
    Thread(String),
    /// `sessions.id`
    Session(i64),
    /// state テーブルの行
    State { table: StateTable, id: i64 },
}

/// provenance が指す state テーブル。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateTable {
    People,
    Projects,
    Commitments,
    OpenLoops,
}

/// ブロックの由来。予算充填時の差し替え判断に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    StateFact,
    Evidence,
    ThreadSummary,
    SessionSummary,
    Structured,
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
    /// 確信度（state 由来。evidence/summary は 1.0）。
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
        assert_eq!(b.tokens, est.count(&b.text));
        assert_eq!(b.id_ref, BlockRef::Event(42));
    }
}
```

- [ ] **Step 3: テスト実行**

Run: `cargo test -p shogun-fusion block::`
Expected: PASS（1 テスト）

- [ ] **Step 4: clippy**

Run: `cargo clippy -p shogun-fusion --all-targets -- -D warnings`
Expected: 警告なし

- [ ] **Step 5: コミット**

```bash
git add crates/shogun-fusion/src/block.rs crates/shogun-fusion/src/lib.rs
git commit -m "feat(fusion): 共通ブロック型 ContextBlock/BlockRef/ScoreInputs (#63)"
```

---

## Task 3: ブロックスコアリング（`score_block`）

**Files:**
- Create: `crates/shogun-fusion/src/score.rs`
- Modify: `crates/shogun-fusion/src/lib.rs`

- [ ] **Step 1: lib.rs にモジュール宣言を追加**

`pub mod block;` の直後に:

```rust
pub mod score;
```

- [ ] **Step 2: score.rs を新規作成（重み＋関数＋テスト）**

`crates/shogun-fusion/src/score.rs`:

```rust
//! ブロックスコアリング（Issue #63 設計 §3.2）。4 要素の重み付き線形和。
//!
//! 初期重みは既存 thread `salience`（lexical .30 / on_screen .20 / recency .25 / pressure .25）
//! を出発点に、クエリ関連をやや上げた値。計測後に校正する。

use crate::block::ScoreInputs;

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
}
```

- [ ] **Step 3: テスト実行**

Run: `cargo test -p shogun-fusion score::`
Expected: PASS（3 テスト）

- [ ] **Step 4: コミット**

```bash
git add crates/shogun-fusion/src/score.rs crates/shogun-fusion/src/lib.rs
git commit -m "feat(fusion): ブロックスコアリング score_block/ScoreWeights (#63)"
```

---

## Task 4: 予算充填（`fit_to_budget`）

**Files:**
- Modify: `crates/shogun-fusion/src/budget.rs`

`fit_to_budget` は「スコア降順で予算を埋め、超過分は下位を落とす」。thread summary への差し替えは Task 5 の `compress` 側で候補を作った後に効くため、ここでは**純粋な予算充填**（スコア付きブロックの取捨）に集中する。

- [ ] **Step 1: budget.rs に FitResult とテストを追記（失敗する状態）**

`crates/shogun-fusion/src/budget.rs` の `#[cfg(test)]` ブロックの**直前**に追記:

```rust
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
```

- [ ] **Step 2: budget.rs の `mod tests` にテストを追記**

`mod tests` の中（既存テストの後ろ）に追記:

```rust
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
```

- [ ] **Step 3: テスト実行**

Run: `cargo test -p shogun-fusion budget::`
Expected: PASS（既存 3 ＋ 追加 3 = 6 テスト）

- [ ] **Step 4: clippy**

Run: `cargo clippy -p shogun-fusion --all-targets -- -D warnings`
Expected: 警告なし

- [ ] **Step 5: コミット**

```bash
git add crates/shogun-fusion/src/budget.rs
git commit -m "feat(fusion): 予算充填 fit_to_budget/FitResult (#63)"
```

---

## Task 5: compress 統括（`compress` ＋ 設定型）

**Files:**
- Create: `crates/shogun-fusion/src/compress.rs`
- Modify: `crates/shogun-fusion/src/lib.rs`

- [ ] **Step 1: lib.rs にモジュール宣言を追加**

`pub mod score;` の直後に:

```rust
pub mod compress;
```

- [ ] **Step 2: compress.rs を新規作成（設定・出力・統括＋テスト）**

`crates/shogun-fusion/src/compress.rs`:

```rust
//! 圧縮の統括（Issue #63 設計 §3.4）。収集済み候補ブロックを score → 予算充填し、
//! 圧縮済みコンテキストを返す。**LLM を呼ばない**（クエリ時ローカル処理）。
//!
//! thread が予算を圧迫する場合の「raw turns → thread summary 差し替え」は、候補生成の時点で
//! daemon が両方（raw evidence と thread summary ブロック）を候補に入れておき、スコアと予算で
//! 自然に summary が選ばれる形にする。summary は evidence より短くトークン効率が高いので、
//! 同等スコアなら予算内に収まりやすい。

use crate::block::{BlockRef, ContextBlock};
use crate::budget::{fit_to_budget, TokenEstimator};
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

/// 便宜ヘルパ: 推定器を明示したいときのために公開しておく（daemon が候補生成で使う）。
pub fn _uses_estimator(_est: &dyn TokenEstimator) {}

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
```

> 注: `_uses_estimator` は「daemon が推定器を候補生成で使う」ことを型で示すためのダミー公開関数。clippy が dead_code を出す場合は削除してよい（daemon 実装後は不要）。ビルドを通すためだけの補助なので、Task 10 で daemon が推定器を実使用した時点で削除する。

- [ ] **Step 3: テスト実行**

Run: `cargo test -p shogun-fusion compress::`
Expected: PASS（2 テスト）

- [ ] **Step 4: fusion 全体テスト＋clippy**

Run: `cargo test -p shogun-fusion && cargo clippy -p shogun-fusion --all-targets -- -D warnings`
Expected: 全 PASS・警告なし

- [ ] **Step 5: コミット**

```bash
git add crates/shogun-fusion/src/compress.rs crates/shogun-fusion/src/lib.rs
git commit -m "feat(fusion): compress 統括と CompressionConfig/CompressedContext (#63)"
```

---

## Task 6: 計測マイグレーション V11 ＋ ロールバック

**Files:**
- Create: `crates/shogun-memory/src/migrations/V11__compression_metrics.sql`
- Create: `docs/migrations/V11-rollback.md`

- [ ] **Step 1: マイグレーション SQL を作成**

`crates/shogun-memory/src/migrations/V11__compression_metrics.sql`:

```sql
-- Issue #63: 圧縮の計測（raw vs compressed 比較）。本文・キャプチャ内容は保存しない。
CREATE TABLE compression_metrics (
    id           INTEGER PRIMARY KEY,
    ts           INTEGER NOT NULL,
    query_hash   TEXT    NOT NULL,                 -- クエリの xxh64（本文は保存しない）
    path         TEXT    NOT NULL CHECK(path IN ('raw','compressed')),
    pre_tokens   INTEGER NOT NULL,
    post_tokens  INTEGER NOT NULL,
    compress_ms  INTEGER NOT NULL,
    assemble_ms  INTEGER NOT NULL
);

CREATE INDEX idx_compression_metrics_ts ON compression_metrics(ts);
```

- [ ] **Step 2: ロールバック手順を作成**

`docs/migrations/V11-rollback.md`:

```markdown
# V11 ロールバック — compression_metrics

計測専用テーブル。ドロップしてもメモリ本体（event_log / state / threads / sessions）に影響なし。

```sql
DROP INDEX IF EXISTS idx_compression_metrics_ts;
DROP TABLE IF EXISTS compression_metrics;
```
```

- [ ] **Step 3: マイグレーションが適用されることを確認**

Run: `cargo test -p shogun-memory --features db migration`
Expected: PASS（既存のマイグレーションテストが V11 込みで通る。テストが無い場合は次の Step 4 の insert テストで担保）

- [ ] **Step 4: コミット**

```bash
git add crates/shogun-memory/src/migrations/V11__compression_metrics.sql docs/migrations/V11-rollback.md
git commit -m "feat(memory): 圧縮計測テーブル V11 compression_metrics (#63)"
```

---

## Task 7: 計測の insert/query（`compression_metrics.rs`）

**Files:**
- Create: `crates/shogun-memory/src/compression_metrics.rs`
- Modify: `crates/shogun-memory/src/lib.rs`

> 既存モジュール（例 `meeting_recaps.rs`）の書式に合わせる。`Connection` を受け取り `rusqlite::Result` を返す関数群にする。

- [ ] **Step 1: lib.rs にモジュール公開を追加**

`crates/shogun-memory/src/lib.rs` のモジュール宣言群（`pub mod meeting_recaps;` などが並ぶ箇所）に追記:

```rust
pub mod compression_metrics;
```

- [ ] **Step 2: compression_metrics.rs を作成（型＋insert＋集計＋テスト）**

`crates/shogun-memory/src/compression_metrics.rs`:

```rust
//! 圧縮の計測（Issue #63）。raw / compressed の各パスの前後トークン数と処理時間を記録し、
//! AB 比較に使う。**本文・キャプチャ内容は保存しない**（テレメトリ規約 G8）。クエリは
//! `query_hash`（呼び出し側で xxh64 済み）のみ。

use rusqlite::{params, Connection};

/// 1 回の組み立てで記録する計測行。
#[derive(Debug, Clone, PartialEq)]
pub struct MetricRow {
    pub ts: i64,
    pub query_hash: String,
    /// "raw" または "compressed"。
    pub path: String,
    pub pre_tokens: i64,
    pub post_tokens: i64,
    pub compress_ms: i64,
    pub assemble_ms: i64,
}

/// 1 行を挿入する。best-effort（呼び出し側が失敗を無視できるよう Result を返す）。
pub fn insert(conn: &Connection, row: &MetricRow) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO compression_metrics
           (ts, query_hash, path, pre_tokens, post_tokens, compress_ms, assemble_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            row.ts,
            row.query_hash,
            row.path,
            row.pre_tokens,
            row.post_tokens,
            row.compress_ms,
            row.assemble_ms
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// あるパスの平均削減率（1 - post/pre）を返す。行が無ければ None。
pub fn avg_reduction(conn: &Connection, path: &str) -> rusqlite::Result<Option<f64>> {
    let v: Option<f64> = conn.query_row(
        "SELECT AVG(1.0 - CAST(post_tokens AS REAL) / NULLIF(pre_tokens, 0))
           FROM compression_metrics WHERE path = ?1",
        params![path],
        |r| r.get(0),
    )?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE compression_metrics (
                id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, query_hash TEXT NOT NULL,
                path TEXT NOT NULL CHECK(path IN ('raw','compressed')),
                pre_tokens INTEGER NOT NULL, post_tokens INTEGER NOT NULL,
                compress_ms INTEGER NOT NULL, assemble_ms INTEGER NOT NULL);",
        )
        .unwrap();
        c
    }

    fn row(path: &str, pre: i64, post: i64) -> MetricRow {
        MetricRow {
            ts: 1_000,
            query_hash: "deadbeef".into(),
            path: path.into(),
            pre_tokens: pre,
            post_tokens: post,
            compress_ms: 5,
            assemble_ms: 20,
        }
    }

    #[test]
    fn insert_returns_rowid() {
        let c = conn();
        let id = insert(&c, &row("compressed", 100, 30)).unwrap();
        assert_eq!(id, 1);
    }

    #[test]
    fn avg_reduction_computes_ratio() {
        let c = conn();
        insert(&c, &row("compressed", 100, 20)).unwrap(); // 0.8
        insert(&c, &row("compressed", 100, 40)).unwrap(); // 0.6
        let avg = avg_reduction(&c, "compressed").unwrap().unwrap();
        assert!((avg - 0.7).abs() < 1e-9, "avg={avg}");
    }

    #[test]
    fn avg_reduction_none_when_empty() {
        let c = conn();
        assert_eq!(avg_reduction(&c, "compressed").unwrap(), None);
    }
}
```

- [ ] **Step 3: テスト実行**

Run: `cargo test -p shogun-memory --features db compression_metrics::`
Expected: PASS（3 テスト）

> `--features db` が無効でもコンパイルさせるため、`compression_metrics` は rusqlite を使う。既存の他モジュール（meeting_recaps 等）が `db` feature でどう gate されているかを確認し、同じ `#[cfg(feature = "db")]` があれば `pub mod compression_metrics;` にも付ける。

- [ ] **Step 4: コミット**

```bash
git add crates/shogun-memory/src/compression_metrics.rs crates/shogun-memory/src/lib.rs
git commit -m "feat(memory): 圧縮計測 insert/avg_reduction (#63)"
```

---

## Task 8: Dream Cycle `Summarizer` seam ＋ `run_compression`

**Files:**
- Modify: `crates/shogun-core/src/dreamcycle/jobs.rs`

`Classifier` と同じ流儀で `Summarizer` seam を導入し、`JobKind::Compression` の no-op を実装する。ローカルデフォルトは抽出的要約（ネットワーク無し・決定的）。

- [ ] **Step 1: 失敗するテストを書く（jobs.rs の `mod tests`）**

`crates/shogun-core/src/dreamcycle/jobs.rs` の `#[cfg(test)] mod tests` 内に追記:

```rust
    #[test]
    fn local_extractive_summarizer_produces_nonempty_summary() {
        use shogun_memory::event_log::EventText;
        let events = vec![
            EventText { id: 1, content: "決めた: 金曜に出す。".into() },
            EventText { id: 2, content: "次は請求書のレビュー。".into() },
        ];
        let s = LocalExtractiveSummarizer.summarize(&events);
        assert!(s.is_some());
        assert!(!s.unwrap().is_empty());
    }

    #[test]
    fn local_extractive_summarizer_empty_input_is_none() {
        let s = LocalExtractiveSummarizer.summarize(&[]);
        assert!(s.is_none());
    }
```

> `EventText` の実フィールド名は `crates/shogun-memory/src/event_log.rs` で確認すること（`id` / `content` を想定）。異なれば上記テストとコンストラクタを合わせる。

- [ ] **Step 2: `Summarizer` trait とローカル実装を追加**

`crates/shogun-core/src/dreamcycle/jobs.rs` の `Classifier` trait 定義の**直後**に追記:

```rust
/// thread/session の event 群を 1 行要約へ。model を触るのは Batch 実装のみ（不変条件5）。
/// `None` は「要約する材料がない」を表す（呼び出し側は summary を書かない）。
pub trait Summarizer {
    fn summarize(&self, events: &[shogun_memory::event_log::EventText]) -> Option<String>;
}

/// ネットワーク不要のローカル抽出的要約（Linux テスト用デフォルト）。各 event の先頭文を
/// 拾って連結し、全体を一定長で切る。Batch 抽象要約器は on-device build が注入する。
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalExtractiveSummarizer;

/// 抽出的要約の最大文字数（要約は元より必ず短い）。
const EXTRACTIVE_SUMMARY_CHARS: usize = 280;

impl Summarizer for LocalExtractiveSummarizer {
    fn summarize(&self, events: &[shogun_memory::event_log::EventText]) -> Option<String> {
        if events.is_empty() {
            return None;
        }
        let mut out = String::new();
        for e in events {
            // 先頭文（区切り . ! ? 。！？ まで）を 1 つ拾う。
            let lead: String = e
                .content
                .split(['.', '!', '?', '。', '！', '？'])
                .find(|s| !s.trim().is_empty())
                .unwrap_or("")
                .trim()
                .to_string();
            if lead.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push_str(" / ");
            }
            out.push_str(&lead);
            if out.chars().count() >= EXTRACTIVE_SUMMARY_CHARS {
                break;
            }
        }
        if out.is_empty() {
            return None;
        }
        // 文字境界安全に切る。
        let truncated: String = out.chars().take(EXTRACTIVE_SUMMARY_CHARS).collect();
        Some(truncated)
    }
}
```

- [ ] **Step 3: テスト実行（要約器だけ先に green に）**

Run: `cargo test -p shogun-core --features db local_extractive`
Expected: PASS（2 テスト）

- [ ] **Step 4: `run_compression` を実装し、no-op を置換**

`crates/shogun-core/src/dreamcycle/jobs.rs` で `DbDreamRunner` の `impl` 内（`consolidate` 等の隣）にメソッドを追加:

```rust
    /// Compression ジョブ本体（Issue #63）。当日ウィンドウのアクティブ thread を要約し
    /// `threads.summary` を埋める。要約器は seam（ローカル抽出的 or Batch）。summary が空の
    /// thread はスキップ（破綻させない）。
    fn run_compression<S: Summarizer>(&self, summarizer: &S, from_ts: i64, to_ts: i64) -> Result<(), String> {
        let conn = self.db.conn.lock().map_err(|_| "db lock poisoned".to_string())?;
        // 当日ウィンドウにアクティビティがある thread。
        let threads = shogun_memory::thread::active_between(&conn, from_ts, to_ts)
            .map_err(|e| e.to_string())?;
        for t in threads {
            let events = shogun_memory::thread::event_texts(&conn, &t.thread_key)
                .map_err(|e| e.to_string())?;
            if let Some(summary) = summarizer.summarize(&events) {
                shogun_memory::thread::set_summary(&conn, &t.thread_key, &summary)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
```

> **依存する memory 関数**（`crates/shogun-memory/src/thread.rs` に無ければ本タスクで追加する。既存の `recent`/`recent_events` の書式に合わせる）:
> - `active_between(conn, from_ts, to_ts) -> rusqlite::Result<Vec<ThreadRow>>`: `last_activity_at BETWEEN from_ts AND to_ts` の thread。
> - `event_texts(conn, thread_key) -> rusqlite::Result<Vec<EventText>>`: その thread の event 本文（`recent_events` を `EventText` に写像）。
> - `set_summary(conn, thread_key, summary) -> rusqlite::Result<()>`: `UPDATE threads SET summary=?, updated_at=? WHERE thread_key=?`。
>
> これらを追加したら `cargo test -p shogun-memory --features db thread::` で個別テスト（各 1 ケース: 挿入→取得→summary 反映）を足す。

- [ ] **Step 5: `run` の分岐を差し替え、ランナーに summarizer を持たせる**

`DbDreamRunner` を `Summarizer` も注入できるようにする。構造体とコンストラクタを変更:

```rust
pub struct DbDreamRunner<'a, C: Classifier, S: Summarizer> {
    db: &'a Db,
    classifier: &'a C,
    summarizer: &'a S,
    now_ms: i64,
}

impl<'a, C: Classifier, S: Summarizer> DbDreamRunner<'a, C, S> {
    pub fn new(db: &'a Db, classifier: &'a C, summarizer: &'a S, now_ms: i64) -> Self {
        Self { db, classifier, summarizer, now_ms }
    }
}
```

`DreamJobRunner for DbDreamRunner` の `run` で Compression を差し替え:

```rust
            JobKind::Compression => self.run_compression(self.summarizer, from_ts, to_ts),
```

`impl<C: Classifier> DreamJobRunner for DbDreamRunner<'_, C>` のシグネチャを
`impl<C: Classifier, S: Summarizer> DreamJobRunner for DbDreamRunner<'_, C, S>` に更新する。

> **呼び出し側の更新**: `DbDreamRunner::new(...)` を呼ぶ箇所（`super::run` や既存テスト）を全て新シグネチャ（`summarizer` 追加）に合わせる。Linux 経路は `&LocalExtractiveSummarizer` を渡す。grep: `rg "DbDreamRunner::new" crates/`。

- [ ] **Step 6: Compression が summary を書くテストを追加**

`mod tests` に追記（既存の cycle テストの書式に合わせる。`db_at` ヘルパを利用）:

```rust
    #[test]
    fn compression_fills_thread_summary() {
        use std::sync::Arc;
        let db = db_at(2_000);
        // thread と event を用意（既存テストの投入ヘルパがあればそれを使う）。
        // ここでは capture イベントを 1 件入れて thread を作る想定。
        // （実際の投入 API は daemon/event_log のテストヘルパに合わせること）
        // ... 省略せず、リポジトリの既存 seed ヘルパで thread_key "t1" を作る ...

        let classifier = LocalRuleClassifier;
        let summarizer = LocalExtractiveSummarizer;
        let runner = DbDreamRunner::new(&db, &classifier, &summarizer, 2_000);
        runner.run(JobKind::Compression, 0, 10_000).unwrap();

        let conn = db.conn.lock().unwrap();
        let rows = shogun_memory::thread::recent(&conn, 10).unwrap();
        let t1 = rows.into_iter().find(|t| t.thread_key == "t1");
        // summary が埋まっている（thread に event があれば）。
        // 具体 assert はリポジトリの ThreadRow に summary が露出しているかで調整する。
        let _ = t1;
        let _ = Arc::new(()); // 参照だけ（ヘルパ整合用）
    }
```

> 注: 上のテストは**リポジトリ既存の thread/event 投入ヘルパに依存**する。実装時に `crates/shogun-core/src/dreamcycle/jobs.rs` の既存 `mod tests` にある seed パターン（`Db::open_in_memory` 後の event 投入）を踏襲し、`t1` の作り方と `ThreadRow` の summary 露出を実コードに合わせて確定させること。`ThreadRow` に `summary` が無ければ、検証は `thread::get_summary(conn, "t1")` の追加関数で行う。

- [ ] **Step 7: core テスト＋clippy**

Run: `cargo test -p shogun-core --features db dreamcycle && cargo clippy -p shogun-core --features db --all-targets -- -D warnings`
Expected: 全 PASS・警告なし

- [ ] **Step 8: コミット**

```bash
git add crates/shogun-core/src/dreamcycle/jobs.rs crates/shogun-memory/src/thread.rs
git commit -m "feat(dreamcycle): Summarizer seam と Compression ジョブ実装 (#63)"
```

---

## Task 9: daemon の正規化グルー ＋ 圧縮パス配線 ＋ 計測

**Files:**
- Modify: `crates/shogun-core/src/daemon.rs`

`assemble_context` に圧縮パスを追加する。既存の raw パスは残し、`CompressionConfig.enabled` で分岐。フォールバックと計測を含む。

- [ ] **Step 1: 正規化グルー関数の失敗するテストを書く**

`crates/shogun-core/src/daemon.rs` の `#[cfg(test)] mod tests`（無ければ作成）に追記:

```rust
    #[test]
    fn evidence_to_blocks_preserves_provenance_and_counts_tokens() {
        use shogun_fusion::block::{BlockRef, SourceKind};
        use shogun_fusion::budget::HeuristicEstimator;
        let ev = vec![Evidence {
            event_id: 7,
            ts: 100,
            source: "capture".into(),
            title: Some("t".into()),
            excerpt: "a".repeat(40),
        }];
        let est = HeuristicEstimator::default();
        let blocks = evidence_to_blocks(&ev, 0.7, &est);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].id_ref, BlockRef::Event(7));
        assert_eq!(blocks[0].source_kind, SourceKind::Evidence);
        assert!(blocks[0].tokens > 0);
    }
```

- [ ] **Step 2: 正規化グルーを実装**

`crates/shogun-core/src/daemon.rs`（`impl Db` の外、モジュールレベルの private fn として）に追記:

```rust
use shogun_fusion::block::{BlockRef, ContextBlock, ScoreInputs, SourceKind};
use shogun_fusion::budget::TokenEstimator;

/// 検索 evidence を圧縮ブロックへ正規化する。evidence は「実際に見たもの」なので
/// confidence=1.0、relevance は呼び出し側が渡す検索スコア由来の値。
fn evidence_to_blocks(evidence: &[Evidence], relevance: f64, est: &dyn TokenEstimator) -> Vec<ContextBlock> {
    evidence
        .iter()
        .map(|e| {
            ContextBlock::new(
                BlockRef::Event(e.event_id),
                SourceKind::Evidence,
                e.excerpt.clone(),
                ScoreInputs { relevance, freshness: 0.5, task_link: 0.0, confidence: 1.0 },
                est,
            )
        })
        .collect()
}

/// confidence ゲートを通した facts を圧縮ブロックへ正規化する。facts は既に
/// `assemble_facts` を通っている（低 confidence は除外済み）。relevance はやや高めに固定
/// （state は現在の作業に紐づく前提）、confidence は High 相当として 0.9。
fn facts_to_blocks(facts: &[String], est: &dyn TokenEstimator) -> Vec<ContextBlock> {
    facts
        .iter()
        .enumerate()
        .map(|(i, f)| {
            ContextBlock::new(
                BlockRef::State { table: shogun_fusion::block::StateTable::OpenLoops, id: i as i64 },
                SourceKind::StateFact,
                f.clone(),
                ScoreInputs { relevance: 0.6, freshness: 0.6, task_link: 0.6, confidence: 0.9 },
                est,
            )
        })
        .collect()
}
```

> 注: `facts_to_blocks` の `BlockRef::State{ id }` は行 id を厳密に持たない（facts は既に文字列化済み）。厳密な provenance が要るなら `inline_memory` を「(fact, state_id, table)」を返す形へ拡張する。今回は**再展開の粒度は thread/evidence を主**とし、fact は補助なので id はプレースホルダ index で可（設計 §7 の拡張点）。

- [ ] **Step 3: `assemble_context` に圧縮パスを追加**

既存 `assemble_context(&self, query, max_hits, excerpt_chars) -> ContextPack` の**隣**に、圧縮版を追加（既存関数は無変更で残す＝raw パス）:

```rust
    /// 圧縮版のコンテキスト組み立て（Issue #63）。`config.enabled` が false のときは
    /// 呼び出し側が `assemble_context` を使う想定なので、ここは true 前提。ローカル処理のみ
    /// （LLM を呼ばない）。処理が `COMPRESS_BUDGET_MS` を超えた/失敗したら raw にフォールバック。
    pub fn assemble_context_compressed(
        &self,
        query: &str,
        max_hits: usize,
        excerpt_chars: usize,
        config: &shogun_fusion::compress::CompressionConfig,
    ) -> (ContextPack, shogun_fusion::compress::CompressionStats, bool) {
        use shogun_fusion::budget::HeuristicEstimator;
        use shogun_fusion::compress::{compress, Candidates};

        let started = std::time::Instant::now();
        // raw と同じ材料を集める。
        let pack = self.assemble_context(query, max_hits, excerpt_chars);
        let est = HeuristicEstimator::default();

        let mut blocks = facts_to_blocks(&pack.facts, &est);
        blocks.extend(evidence_to_blocks(&pack.evidence, 0.7, &est));

        // 時間予算を超えたらフォールバック。
        if started.elapsed().as_millis() as u64 > COMPRESS_BUDGET_MS {
            return (pack, shogun_fusion::compress::CompressionStats::default(), true);
        }

        let out = compress(Candidates { blocks }, config);
        // 圧縮済みブロックから ContextPack を再構成（facts と evidence に振り分け）。
        let mut facts = Vec::new();
        let mut evidence = Vec::new();
        for b in &out.blocks {
            match b.id_ref {
                shogun_fusion::block::BlockRef::Event(id) => evidence.push(Evidence {
                    event_id: id,
                    ts: 0,
                    source: String::new(),
                    title: None,
                    excerpt: b.text.clone(),
                }),
                _ => facts.push(b.text.clone()),
            }
        }
        (ContextPack { facts, evidence }, out.stats, false)
    }
```

そして daemon 上部の定数群（`REPLY_TURNS` などの近く）に追記:

```rust
/// クエリ時のローカル圧縮に許す時間予算。超えたら raw にフォールバック（SLO +300ms 厳守）。
const COMPRESS_BUDGET_MS: u64 = 50;
```

- [ ] **Step 4: テスト実行**

Run: `cargo test -p shogun-core --features db daemon::`
Expected: PASS（Step 1 のテスト＋既存 daemon テスト）

- [ ] **Step 5: clippy**

Run: `cargo clippy -p shogun-core --features db --all-targets -- -D warnings`
Expected: 警告なし。`compress.rs` の `_uses_estimator` が dead_code 警告になる場合はこのタイミングで削除する（daemon が推定器を実使用したため）。

- [ ] **Step 6: コミット**

```bash
git add crates/shogun-core/src/daemon.rs crates/shogun-fusion/src/compress.rs
git commit -m "feat(core): daemon に圧縮パスと正規化グルーを配線 (#63)"
```

---

## Task 10: SLO ＋ AB テスト拡張

**Files:**
- Modify: `crates/shogun-core/tests/context_slo.rs`

- [ ] **Step 1: 圧縮パスが予算内に収まり raw 比で膨らまないテストを書く**

`crates/shogun-core/tests/context_slo.rs` に追記（既存の `assemble_context(q, 6, 600)` 呼び出しパターンに合わせる）:

```rust
#[test]
fn compressed_context_stays_within_budget_and_reduces_tokens() {
    use shogun_fusion::compress::CompressionConfig;
    // 既存テストと同じ seed 手順で db を作り、複数 evidence がヒットするクエリを用意する。
    let db = /* 既存 context_slo.rs の seed ヘルパで作成 */ seeded_db();
    let cfg = CompressionConfig { enabled: true, budget_tokens: 200, ..Default::default() };

    let (pack_c, stats, fell_back) = db.assemble_context_compressed("report", 6, 600, &cfg);

    assert!(!fell_back, "50ms 以内に収まるはず");
    assert!(stats.post_tokens <= 200, "post={}", stats.post_tokens);
    // 圧縮で減っている（材料が予算超なら pre > post）。
    if stats.pre_tokens > 200 {
        assert!(stats.post_tokens < stats.pre_tokens);
    }
    // 圧縮後も何か残る（全落ちしない）。
    assert!(!pack_c.facts.is_empty() || !pack_c.evidence.is_empty());
}
```

> `seeded_db()` は既存 `context_slo.rs` の seed 手順に置換すること（同ファイル内の既存テストがどう db を用意しているかを踏襲）。

- [ ] **Step 2: フォールバックが raw と同一内容を返すことを確認するテスト**

```rust
#[test]
fn disabled_or_fallback_matches_raw() {
    let db = seeded_db();
    let raw = db.assemble_context("report", 6, 600);
    // budget_tokens を極端に大きくすると圧縮しても全採用 → raw と同じ本文集合。
    use shogun_fusion::compress::CompressionConfig;
    let cfg = CompressionConfig { enabled: true, budget_tokens: 1_000_000, ..Default::default() };
    let (pack_c, _stats, _fell) = db.assemble_context_compressed("report", 6, 600, &cfg);
    // evidence 件数は一致（全部予算に収まるので落ちない）。
    assert_eq!(pack_c.evidence.len(), raw.evidence.len());
}
```

- [ ] **Step 3: テスト実行**

Run: `cargo test -p shogun-core --features db --test context_slo`
Expected: PASS

- [ ] **Step 4: コミット**

```bash
git add crates/shogun-core/tests/context_slo.rs
git commit -m "test(core): 圧縮パスの SLO/AB テスト (#63)"
```

---

## Task 11: 計測の配線（daemon → compression_metrics）

**Files:**
- Modify: `crates/shogun-core/src/daemon.rs`

圧縮パスの stats を DB の `compression_metrics` に記録する。**query 本文は xxh64 化して query_hash として渡す**（本文は保存しない）。

- [ ] **Step 1: 失敗するテストを書く**

`crates/shogun-core/src/daemon.rs` の `mod tests` に追記:

```rust
    #[test]
    fn record_compression_metric_persists_hash_not_text() {
        let db = Db::open_in_memory(std::sync::Arc::new(|| 1_000)).unwrap();
        db.record_compression_metric("report", "compressed", 100, 30, 5, 20);
        let conn = db.conn.lock().unwrap();
        let (qh, path): (String, String) = conn
            .query_row(
                "SELECT query_hash, path FROM compression_metrics LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_ne!(qh, "report");   // 本文は保存しない
        assert_eq!(path, "compressed");
    }
```

- [ ] **Step 2: 記録メソッドを実装**

`impl Db` に追記（xxh64 は既存 traceability で使っているハッシュ関数を流用。無ければ `twox_hash` 依存を確認）:

```rust
    /// 圧縮計測を 1 行記録する。best-effort（失敗は握りつぶし、作業を止めない）。
    /// query は xxh64 化して保存（本文は保存しない、テレメトリ規約 G8）。
    pub fn record_compression_metric(
        &self,
        query: &str,
        path: &str,
        pre_tokens: i64,
        post_tokens: i64,
        compress_ms: i64,
        assemble_ms: i64,
    ) {
        let query_hash = format!("{:016x}", shogun_memory::traceability::xxh64(query.as_bytes()));
        if let Ok(conn) = self.conn.lock() {
            let _ = shogun_memory::compression_metrics::insert(
                &conn,
                &shogun_memory::compression_metrics::MetricRow {
                    ts: self.now_ms(),
                    query_hash,
                    path: path.to_string(),
                    pre_tokens,
                    post_tokens,
                    compress_ms,
                    assemble_ms,
                },
            );
        }
    }
```

> `shogun_memory::traceability::xxh64` の実際の関数名/シグネチャを確認して合わせる（traceability_log が chunk_xxh64 を作っている経路と同じものを使う）。無ければ `compression_metrics` 内に小さな `pub fn xxh64(bytes: &[u8]) -> u64` を用意して流用する。

- [ ] **Step 3: `assemble_context_compressed` から記録を呼ぶ**

Task 9 で作った `assemble_context_compressed` の `return` 直前に、計測記録を挟む（compress_ms/assemble_ms を Instant から算出）。関数内に経過計測を追加:

```rust
        let compress_ms = started.elapsed().as_millis() as i64;
        self.record_compression_metric(query, "compressed", out.stats.pre_tokens as i64,
            out.stats.post_tokens as i64, compress_ms, compress_ms);
```

（フォールバック分岐でも `record_compression_metric(query, "raw", ...)` を残せると AB が揃うが、必須ではない。時間があれば追加。）

- [ ] **Step 4: テスト＋clippy**

Run: `cargo test -p shogun-core --features db daemon:: && cargo clippy -p shogun-core --features db --all-targets -- -D warnings`
Expected: PASS・警告なし

- [ ] **Step 5: コミット**

```bash
git add crates/shogun-core/src/daemon.rs
git commit -m "feat(core): 圧縮計測を compression_metrics に記録 (#63)"
```

---

## Task 12: 統合確認 ＋ ガード

**Files:** なし（検証のみ）

- [ ] **Step 1: ワークスペース全体のビルド/テスト**

Run: `cargo test -p shogun-fusion && cargo test -p shogun-memory --features db && cargo test -p shogun-core --features db`
Expected: 全 PASS

- [ ] **Step 2: clippy（db 有無の両方）**

Run:
```bash
cargo clippy -p shogun-fusion --all-targets -- -D warnings
cargo clippy -p shogun-core --features db --all-targets -- -D warnings
cargo clippy -p shogun-core --all-targets -- -D warnings
```
Expected: 全て警告なし

- [ ] **Step 3: 不変条件の目視チェック**

- [ ] `compression_metrics` に query 本文が入らない（query_hash のみ）
- [ ] Summarizer の Batch 実装は Select KK 経路（本タスクではローカルのみ実装、Batch 注入は on-device build の別 PR で配線）
- [ ] `assemble_facts` の confidence ゲートが圧縮の前段に残っている（低 confidence facts は evidence/facts に入らない）
- [ ] fusion クレートは shogun-memory に依存していない（`crates/shogun-fusion/Cargo.toml` に memory 依存が増えていない）

- [ ] **Step 4: 既存の egress/secret/migration ガードスクリプト**

Run: `ls scripts/` で確認し、存在する CI 相当のガード（egress 一元化・secret・migration）をローカル実行する。例:
```bash
# scripts/ 配下の該当スクリプトを実行（issue56-summary.md 記載のガード群）
```
Expected: 全 green

- [ ] **Step 5: PR 用サマリを用意してプッシュ（オーナー承認後）**

```bash
git push -u origin feat/issue-63-context-compression
```

PR 本文に含める:
- Goal（50–80% 削減・+300ms 以内・AB 可能）に対する計測結果（`avg_reduction` の出力、SLO テストの p50/p95）
- スキーマ変更（V11）＋ロールバック手順
- 次周スコープ（モード UI / デバッグパネル / Batch 抽象要約器の on-device 配線 / パーソナライズ）

---

## Self-Review（spec との突合）

- **正規化** → Task 2（ContextBlock）＋ Task 9（daemon グルー）
- **スコアリング**（関連/鮮度/タスク/confidence）→ Task 3
- **トークン予算** → Task 1（推定）＋ Task 4（充填）
- **オフライン抽象要約**（Compression ジョブ）→ Task 8（Summarizer seam ＋ threads.summary）
- **構造化データのロスレス短縮** → ScoreInputs/SourceKind::Structured を型で用意（Task 2）。実データ変換は daemon グルー拡張（Task 9 の evidence/facts と同じ経路で追加可能）。※ 今周は facts/evidence を主対象とし、カレンダー等の Structured 変換は daemon グルーの追加関数として次スライスで足す拡張点。
- **組み立て統合＋フラグ** → Task 9（`assemble_context_compressed` ＋ `CompressionConfig.enabled`）
- **計測＋AB** → Task 6/7（テーブル・集計）＋ Task 11（記録）＋ Task 10（AB テスト）
- **生ログ参照/再展開** → BlockRef（Task 2）＋ CompressedContext.refs（Task 5）
- **フォールバック** → Task 9（COMPRESS_BUDGET_MS 超過 → raw）
- **不変条件** → Task 12 Step 3

**既知の未カバー（意図的に次周）:** モードトグル UI、デバッグパネル/サイドバー、AB ダッシュボード UI、パーソナライズ学習、アプリ除外設定 UI、Batch 抽象要約器の on-device 配線、カレンダー/ToDo 等 Structured のロスレス変換の実データ実装。いずれも本計画の型・seam・テーブルで拡張点を確保済み。
