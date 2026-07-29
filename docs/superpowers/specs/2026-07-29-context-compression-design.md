# コンテキスト圧縮の最適化 — 設計書（縦に薄い一周）

- Issue: [#63 コンテキスト圧縮の最適化](https://github.com/Torutesu/ShogunAI-/issues/63)
- 日付: 2026-07-29
- ステータス: 設計確定（実装計画へ）
- スコープ判断: 「縦に薄く一周」＋「重い要約はオフライン、クエリ時は軽量選別」（オーナー承認済み 2026-07-29）

---

## 0. この設計書の位置づけ

Issue #63 は「正規化層・スコアリング・4つの圧縮戦略・再展開・計測・ABテスト・モードUI・パーソナライズ」までを含む大きな塊。本設計書は、そのうち **end-to-end で価値が通る最小の縦切り（一周）** に絞る。目的は Issue の Goal に直撃すること：

- 圧縮後トークン **50〜80% 削減**、回答品質は同等以上
- 体感レイテンシ増を **+300ms 以内**（SLO 準拠）
- raw vs compressed を **AB 比較可能**にし、計測できる状態にする
- パイプラインを **モジュール化**し、モデル/ソース追加に再利用可能にする

モード UI・デバッグパネル・ダッシュボード・パーソナライズ学習は**次周**。ただし今回の設計はそれらを前提に拡張点を残す。

---

## 1. 現状（コードベースの事実）

調査で確定した「今あるもの」。本設計はこの上に積む。

### 1.1 組み立て（`crates/shogun-fusion`, `crates/shogun-core`）
- `shogun-fusion/src/assemble.rs::assemble()` — state 候補を `relevance × confidence_weight` でランク、dedup、`MAX_ACTIONS=4` で cap。`ContextCache`（RAM）を返す純関数。
- `shogun-fusion/src/confidence.rs::assemble_facts()` — **confidence ゲートの choke point**。Low(<0.5) は完全除外、Medium(0.5–0.8) は `possibly:` 接頭、High(≥0.8) は逐語。
- `shogun-core/src/daemon.rs`
  - `assemble_context(query, max_hits, excerpt_chars)` — 接地 QA コンテキスト。`inline_memory(8)` の facts ＋ `search()` の evidence（各 `excerpt_chars` で切詰）。
  - `build_reply_context(thread_key)` — 返信ドラフト用ウォームキャッシュ。直近ターン `REPLY_TURNS=12`（各 `REPLY_TURN_CHARS=800`）＋関連スレッド `REPLY_RELATED=4`（各 `REPLY_RELATED_CHARS=300`）＋`inline_memory(6)`。**フォーカスパスで事前構築**（SLO-05 300ms）、ボタン押下時は生成のみ。
  - `inline_memory(limit)` — commitments/open_loops を `assemble_facts()` 経由（confidence ゲート）で facts 化し `limit` 行で切詰。
- `crates/shogun-core/src/llm/anthropic.rs` — Agent lane（BYOK, streaming）/ Batch lane（Select KK）。**トークンカウントなし**、`max_tokens=1024` 固定。

### 1.2 メモリ（`crates/shogun-memory`）
- 3層メモリ: Hot(24h/RAM) / Warm(30日/SQLite+sqlite-vec) / Cold(全履歴/int8量子化)。
- ハイブリッド検索: FTS5(trigram) ＋ vector KNN を **RRF(k=60)** で融合（`search_hybrid`, `search_warm_first`）。
- state tables（people/projects/commitments/open_loops）: 全行に `confidence` ＋ `base_confidence` ＋ **provenance 必須**（`state_provenance`、空 provenance は挿入拒否）。
- **threads テーブルに `summary` カラムが既にある（Dream Cycle まで NULL）**。`salience = 0.30·lexical + 0.20·on_screen + 0.25·recency + 0.25·pressure`。
- **sessions テーブルにも `summary`（Recap 出力）がある**。
- event_log: append-only、安定 `id (PK)`、`content_hash` で dedup。全 state 行はここへ provenance 参照。

### 1.3 Dream Cycle（`crates/shogun-core/src/dreamcycle`）
- FULL_SEQUENCE: Consolidation → **Compression（現 no-op: `JobKind::Compression => Ok(())`）** → StateUpdate → ConfidenceRecalc → ColdDemotion → MorningBrief。
- `job_runs` テーブルで `UNIQUE(cycle_id, kind)`、`done` は再実行スキップ（冪等・クラッシュ再開）。
- Batch 分類は `Classifier` trait で注入（`PrecomputedClassifier` / `LocalRuleClassifier`）。`BATCH_CONFIDENCE=0.6`。

### 1.4 現状のギャップ（本 Issue が埋める）
1. **トークンカウント/予算が皆無**（文字数・行数 cap のみ）。
2. **抽象要約が未実装**（Compression ジョブは no-op、`threads.summary`/`sessions.summary` は空のまま）。
3. クエリ依存の関連度フィルタが検索止まり（送信直前の予算ベース選別がない）。
4. LLM 送信単位の**共通正規化フォーマットがない**。
5. **圧縮メトリクスがない**（前後トークン・圧縮時間・レイテンシ）。
6. raw vs compressed を切替・比較する **AB 手段がない**。

---

## 2. アーキテクチャ / データフロー

```
┌─ Dream Cycle（夜間・オフライン・Batch/Select KK キー）─────────────┐
│ run_compression()  ← 現 no-op を実装                              │
│   for 当日ウィンドウのアクティブ thread / session:               │
│     Batch API で要約生成 → threads.summary / sessions.summary へ  │
│     provenance: thread_key / session_id / 根拠 event_id 群        │
│   job_runs(cycle_id, 'compression') で冪等・クラッシュ再開        │
└──────────────────────────────────────────────────────────────────┘
                    ↓（保存された要約を後段が読むだけ）
┌─ クエリ時（ローカルのみ・LLM 呼ばない）── +300ms / cache300ms SLO ─┐
│ compress(candidates, config) :                                    │
│  1. 収集   state facts + search evidence + thread/session summary │
│  2. 正規化 → ContextBlock { id_ref, source_kind, text,           │
│                              score_inputs, tokens }               │
│  3. スコア score_block() = w_rel·rel + w_fresh·fresh              │
│                          + w_task·task + w_conf·confidence        │
│  4. 構造化 calendar/commitments/open_loops → コンパクト table/JSON │
│             （ロスレス短縮・トークン計上）                        │
│  5. 予算   fit_to_budget(): スコア降順に充填、予算超過の thread は │
│             raw turns → 保存済み summary へ差替、下位ブロック drop │
│  6. 出力   CompressedContext { blocks, refs(provenance), stats }  │
│      失敗 or ローカル 50ms 超 → raw フォールバック＋インジケータ  │
└──────────────────────────────────────────────────────────────────┘
                    ↓
        confidence ゲート（assemble_facts）は圧縮の前段に温存
                    ↓ 圧縮済みコンテキスト＋provenance
                  LLM（Agent lane / BYOK）
```

**設計の芯**
- **計算の重心を分離**: 重い抽象要約は Dream Cycle（Batch, Select KK）へ、クエリ時はローカルの選別・予算・整形のみ（LLM を呼ばない）。→ `+300ms` SLO と CLAUDE.md のキー分離（インデックス/要約=Select KK、推論=BYOK）の両方を守る。
- **既存の choke point を壊さない**: confidence ゲート（不変条件：低 confidence を事実として混ぜない）は圧縮の前段にそのまま残す。
- **provenance を貫通**: 全 ContextBlock が `id_ref`（event_id / thread_key / session_id / state 参照）を保持 → 「詳細文脈を再展開」を後周で UI 化できる土台。

---

## 3. コンポーネント設計（境界を小さく）

各ユニットは「何をするか／どう使うか／何に依存するか」が単独で言えることを条件にする。

> **クレート境界の補正**: `shogun-fusion` は意図的に `shogun-memory` に依存しない純粋クレート（Linux テスト可能）。よって `ContextBlock` 型・スコア・予算・compress は fusion に置き**プリミティブのみ扱う**。SearchHit/ThreadRow/facts → `ContextBlock` の**正規化グルーは daemon（shogun-core）側**に置く（既存の `StateCandidate` と同じ流儀）。

### 3.1 `shogun-fusion/src/block.rs` — 共通ブロック型（純粋）
```rust
/// LLM 送信単位の正規化ブロック。ソース非依存。
pub struct ContextBlock {
    pub id_ref: BlockRef,          // 生ログへの参照（再展開用）
    pub source_kind: SourceKind,   // StateFact | Evidence | ThreadSummary | SessionSummary | Structured
    pub text: String,              // 送信テキスト（confidence マーカー適用後）
    pub score_inputs: ScoreInputs, // relevance/freshness/task_link/confidence の生値
    pub tokens: usize,             // TokenEstimator で見積り済み
}

pub enum BlockRef {
    Event(i64),                    // event_log.id
    Thread(String),                // thread_key
    Session(i64),                  // sessions.id
    State { table: StateTable, id: i64 },
}

pub struct ScoreInputs {
    pub relevance: f64,   // 0..=1（クエリ関連。search score / salience.lexical 由来）
    pub freshness: f64,   // 0..=1（recency 半減）
    pub task_link: f64,   // 0..=1（open_loops/project 紐づき圧、salience.pressure 由来）
    pub confidence: f64,  // 0..=1（state 由来。evidence は 1.0 固定）
}
```
- **責務**: `ContextBlock` / `BlockRef` / `ScoreInputs` / `SourceKind` の**型定義とプリミティブ入力からのコンストラクタ**のみ。SearchHit 等 shogun-memory 型からの変換は daemon 側（§3.6）で行う。
- **依存**: なし（fusion 内のみ）。`TokenEstimator` はコンストラクタに渡す。
- **純関数**: I/O なし。

### 3.2 `shogun-fusion/src/score.rs` — ブロックスコアリング
```rust
pub struct ScoreWeights { pub rel: f64, pub fresh: f64, pub task: f64, pub conf: f64 }
impl Default for ScoreWeights { /* salience 由来の初期値: rel .35, fresh .25, task .20, conf .20 */ }

pub fn score_block(inputs: &ScoreInputs, w: &ScoreWeights) -> f64;
```
- **責務**: 4 要素の重み付き線形和。重みは `CompressionConfig` から供給（将来モード別に差替）。
- **初期重み**は既存 `salience()`（lexical .30 / on_screen .20 / recency .25 / pressure .25）を出発点に、クエリ関連を少し上げた値。校正は計測後に調整。
- **純関数**・単体テスト対象。

### 3.3 `shogun-fusion/src/budget.rs` — トークン推定と予算充填
```rust
/// トークン推定の seam。v1 はローカル・ヒューリスティック実装。
pub trait TokenEstimator {
    fn count(&self, text: &str) -> usize;
}

/// 言語別の char→token 比（モデル別係数を config 化）。クラウド/ONNX 不使用。
pub struct HeuristicEstimator { /* 係数テーブル */ }
impl TokenEstimator for HeuristicEstimator { /* CJK/ラテンで比率を変える */ }

/// スコア降順で予算を充填。thread 超過は summary へ差替、下位を drop。
/// 予算を決して超えない（プロパティ）。provenance は保持。
pub fn fit_to_budget(
    blocks: Vec<ContextBlock>,
    budget_tokens: usize,
    est: &dyn TokenEstimator,
) -> FitResult;

pub struct FitResult {
    pub kept: Vec<ContextBlock>,
    pub dropped: Vec<BlockRef>,   // 落とした参照（計測・再展開用）
    pub pre_tokens: usize,
    pub post_tokens: usize,
}
```
- **トークン推定方針（確定判断）**: v1 は **BPE トークナイザを同梱しない**。言語別 char→token 比のヒューリスティック（±10% 精度で予算管理には十分・依存を増やさない・ローカルファースト維持）。`TokenEstimator` trait 化しておき、将来正確な実装へ差替可能。
- **thread 差替ロジック**: ある thread の raw turns 群が予算を圧迫する場合、その thread に対応する `threads.summary`（あれば）1 ブロックへ丸ごと差替。無ければ raw を下位から drop。
- **純関数**・プロパティテスト対象（予算超過しない／高スコア保持／provenance 保持）。

### 3.4 `shogun-fusion/src/compress.rs` — 統括と設定
```rust
pub struct CompressionConfig {
    pub enabled: bool,           // フィーチャーフラグ
    pub budget_tokens: usize,    // 目標予算
    pub mode: CompressionMode,   // v1 は Balanced のみ出荷（enum は将来拡張）
    pub weights: ScoreWeights,
}
pub enum CompressionMode { Balanced /* , Aggressive, Detailed（次周）*/ }

pub struct CompressedContext {
    pub blocks: Vec<ContextBlock>,
    pub refs: Vec<BlockRef>,      // 採用ブロックの provenance
    pub stats: CompressionStats,  // pre/post tokens, dropped 数
}

pub fn compress(
    candidates: Candidates,       // facts / evidence / summaries / structured
    config: &CompressionConfig,
    est: &dyn TokenEstimator,
) -> CompressedContext;
```
- **責務**: 収集済み候補を block 化 → score → fit_to_budget → `CompressedContext`。**LLM を呼ばない**。
- **依存**: block/score/budget。I/O なし（候補は呼び出し側=daemon が DB から用意して渡す）。

### 3.5 `shogun-core/src/dreamcycle/jobs.rs::run_compression()` — オフライン要約（Summarizer seam）
- **現 no-op を実装**。既存の `Classifier` seam と**同じ流儀**で `Summarizer` seam を導入する：
  ```rust
  /// thread/session の event 群を1行要約へ。model を触るのは Batch 実装のみ（不変条件5）。
  pub trait Summarizer {
      fn summarize(&self, events: &[shogun_memory::event_log::EventText]) -> Option<String>;
  }
  /// ネットワーク不要のローカル抽出的要約（Linux テスト用デフォルト）。
  pub struct LocalExtractiveSummarizer;
  ```
  - デフォルト = `LocalExtractiveSummarizer`（先頭 salient 文の抽出的連結、決定的・ネットワークなし → ランナー全体が Linux テスト可能）。
  - on-device build は **Batch/Select KK** の抽象要約器を注入（`LocalRuleClassifier`↔Batch 分類器の対称）。
- 当日ウィンドウ `[from_ts, to_ts)` のアクティブ thread / open session を対象に summarize → `threads.summary` / `sessions.summary` を UPDATE。
- provenance: 要約対象の event_id 群（既存 thread↔event 紐づけ）を保持。
- **冪等/再開**: 既存 `job_runs(cycle_id, 'compression')` に乗る。`done` はスキップ。
- **キー分離/プライバシー**: Batch 実装は必ず Select KK、traceability_log に chunk 記録・本文は保存しない。

### 3.6 `shogun-core/src/daemon.rs` — 配線と計測
- `assemble_context()` / `build_reply_context()` に**フラグ分岐**を追加：
  - `config.enabled == false` → 現行 raw パス（無変更）。
  - `true` → 候補を集めて `shogun_fusion::compress()` を通す。
- **フォールバック**: `compress` が失敗 or ローカル処理が `COMPRESS_BUDGET_MS`(=50ms) を超過 → raw パスに落として軽量インジケータ用フラグを立てる。
- **計測**: `pre_tokens / post_tokens / compress_ms / assemble_ms / path` を metrics シンクへ（下記 4章）。**キャプチャ本文は記録しない**。
- **AB**: dev/ヘビーユーザーフラグ時、同一クエリで raw と compressed の両方を組み立て、両方のトークン数（と任意で両回答）を記録して人間評価に回せるようにする。

---

## 4. データモデル変更

- **要約の保存は新規テーブル不要**。既存の `threads.summary` / `sessions.summary`（現状 NULL）を埋める。
- 唯一のスキーマ変更 = `crates/shogun-memory/src/migrations/V11__compression_metrics.sql`（V9/V10 は使用済み）:
  ```sql
  CREATE TABLE compression_metrics (
      id INTEGER PRIMARY KEY,
      ts INTEGER NOT NULL,
      query_hash TEXT NOT NULL,     -- クエリの xxh64（本文は保存しない）
      path TEXT NOT NULL CHECK(path IN ('raw','compressed')),
      pre_tokens INTEGER NOT NULL,
      post_tokens INTEGER NOT NULL,
      compress_ms INTEGER NOT NULL,
      assemble_ms INTEGER NOT NULL
  );
  CREATE INDEX idx_compression_metrics_ts ON compression_metrics(ts);
  ```
  - **本文・キャプチャ内容は一切入れない**（テレメトリ規約 G8）。クエリは `query_hash` のみ。
  - rollback: `docs/migrations/V11-rollback.md`（`DROP TABLE compression_metrics;`）を添付。
- Issue の Non-Goal（永続ストレージ/ベクトル DB 設計）には抵触しない小テーブル（計測専用）。

---

## 5. 計測 / テスト

### 5.1 計測（Goal の受入基準）
- `pre_tokens`, `post_tokens`（→ 削減率 50–80% を検証）
- `compress_ms`, `assemble_ms`（→ raw 比 +300ms 以内を検証）
- `path`（raw / compressed）で AB 比較
- 人間評価: N=5〜10 のヘビーユーザーに raw vs compressed の回答対を提示（ダッシュボード UI は次周、今周は記録まで）

### 5.2 テスト
- **純関数**: `score_block`（単調性）、`HeuristicEstimator::count`（言語別校正の許容誤差）、`fit_to_budget`（プロパティ: 予算超過しない・高スコア保持・provenance 保持・thread 差替が起きる）、block 正規化。
- **Compression ジョブ**: 注入した Batch 結果で `threads.summary`/`sessions.summary` が埋まる、job_runs 冪等・クラッシュ再開。
- **SLO**（`crates/shogun-core/tests/context_slo.rs` 拡張）: compressed パスが raw 比 +300ms 以内、フォーカス切替 cache 更新 300ms 維持。
- **不変条件**: 低 confidence 事実が compressed でも除外される／`compression_metrics`・`traceability_log` に本文が入らない／Compression ジョブが Select KK のみ使用。
- **フォールバック**: 50ms 超過・compress エラー時に raw へ落ちる。

---

## 6. エラー処理 / フォールバック

| 事象 | 挙動 |
|---|---|
| ローカル圧縮が `COMPRESS_BUDGET_MS`(50ms) 超過 | raw 組み立てへフォールバック＋軽量インジケータ |
| `compress()` 内部エラー | 同上。ユーザー作業を止めない |
| Dream Cycle の要約が未生成（summary=NULL） | クエリ時は raw turns を使う（圧縮効果が出ないだけ、破綻しない） |
| Compression Batch ジョブ失敗 | `job_runs` で failed 記録、翌サイクル再試行（既存セマンティクス） |

---

## 7. 拡張点（次周のために残すもの）

- `CompressionMode` enum は `Balanced` のみ出荷。`Aggressive`/`Detailed` は `ScoreWeights`/`budget_tokens` の別プリセットとして後付け。
- 設定 UI（Off/Balanced/Aggressive トグル）、デバッグパネル（採用ブロック一覧＋トークン数＋ソース種別）、AB ダッシュボードは `compression_metrics` と `CompressedContext.refs` を読むだけで作れる。
- パーソナライズ（要約粒度・残す情報タイプの学習）は `ScoreWeights` の per-user 化で接続。
- アプリ/ウィンドウ除外は候補収集段（daemon）でフィルタする前提の設計互換のみ確保（UI は別 Issue）。
- 時系列クラスタリング/トピックグルーピングは、v1 では thread/session 単位の要約で近似。より細かいクラスタリングは要約対象の切り方を差し替えるだけで拡張可能。

> **実装状態（配線済み vs 休眠）** — 更新: 2026-07-29「起動」増分後。
> - **配線済み（第1周）**: fusion 純粋パイプライン（block/score/budget/compress）、daemon の正規化グルー＋`assemble_context_compressed`＋計測、Dream Cycle `Compression` ジョブ（Full サイクルで `threads.summary` を `LocalExtractiveSummarizer` で populate）、V11 計測テーブル＋AB（raw/compressed 両記録）。
> - **配線済み（起動増分 2026-07-29）**: ①本番チャット経路（`inline_source.rs::chat_blocking`）を `Db::compression_config()` で圧縮パスへ分岐。②クエリ時の `threads.summary` **consume**（解決済みスレッドの要約を `SourceKind::ThreadSummary` 高 relevance 候補として投入 → 予算逼迫時に raw ターンを押しのける＝§3.3/§3.4 の差し替えレバー本体。テストで substitution を実証）。有効化は env `SHOGUN_COMPRESSION=1`（＋`SHOGUN_COMPRESSION_BUDGET`）で desktop から config 注入、既定 off。
> - **配線済み（忠実性増分 2026-07-29）**: ①圧縮時 citation の source/title/ts 復元（`pack.evidence` から event_id で復元）。②`sessions.summary` の populate（Dream Cycle）＋ consume（retrieved evidence が属する session の要約を `SourceKind::SessionSummary` 高 relevance 候補に）。③fact ブロックの実 state id/table 付与（`inline_memory_with_refs`。公開 `inline_memory` は byte 一致で不変）。
> - **休眠（次周）**: 設定 UI トグル（現状 env フラグ）、reply ドラフト経路（`build_reply_context`）の圧縮、Batch 抽象要約器の on-device 配線、AB ダッシュボード UI、パーソナライズ、session/thread 候補のクエリ由来 relevance（現状固定）、圧縮パスの facts 二重ロード解消（軽微）。
> - マージ安全性: `SHOGUN_COMPRESSION` 未設定なら `compression_config()` は None、チャットは raw 経路のまま＝既存挙動は不変。
> - **付随修正**: 起動増分でマージ済み main の macOS desktop ビルド破損（`dream.rs` の summarizer 引数取りこぼし）を修復。忠実性増分で desktop-clippy(`--all-targets`) の既存 bool_assert 破損（`approvals.rs` テスト）を修復。desktop の `cargo check` ＋ `clippy --all-targets` を検証フローに追加。

---

## 8. 不変条件チェック（CLAUDE.md 準拠）

- ✅ データの重心は Rust コア（圧縮ロジックは fusion/core、webview に置かない）
- ✅ 画像/音声を保存しない（テキストのみ）
- ✅ 生データをデバイス外に出さない（要約は Batch chunk のみ、traceability 記録）
- ✅ キー分離: 要約=Select KK（Batch）、推論=BYOK。逆転させない
- ✅ 低 confidence を事実として混ぜない（`assemble_facts` ゲート温存）
- ✅ テレメトリ/ログにキャプチャ本文を含めない（`query_hash` のみ）
- ✅ 後方互換を破らない（V11 は追加のみ、rollback 添付）
- ✅ SLO 計測コードを同梱（context_slo.rs 拡張）
