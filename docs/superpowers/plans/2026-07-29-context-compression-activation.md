# コンテキスト圧縮の「起動」実装計画（#63 続き）

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** マージ済みの休眠スライスを実際に機能させる。①本番チャット経路を `CompressionConfig.enabled` で圧縮パスへ分岐、②クエリ時に解決済みスレッドの `threads.summary` を候補投入して差し替えレバーを有効化、③desktop からフラグ付きで config を daemon に供給。

**Architecture:** `Db` に `compression_config: Option<CompressionConfig>` を embedder と同じ builder パターンで持たせる。`assemble_context_compressed` に候補 `thread_keys` を渡し、その `thread_summary()` を `SourceKind::ThreadSummary` ブロックとして `Candidates` に加える（高 relevance なので予算逼迫時に raw evidence を押しのける）。desktop 初期化で env `SHOGUN_COMPRESSION` を読み、有効時のみ `enabled:true` の config を注入（既定 off = 挙動不変）。

**Tech Stack:** Rust / shogun-core daemon / shogun-fusion（既存 compress）/ Tauri desktop (`apps/desktop/src-tauri`)。前提 worktree: `/Users/torutano/ShogunAI--issue63b`（branch `feat/issue-63-activate`、merged main 起点）。

**検証:** `cargo test -p shogun-core --features db`、`cargo clippy -p shogun-core --features db --all-targets -- -D warnings`、desktop は `cargo check -p <desktop-crate>`。

---

## Task 1: `Db` に compression_config（field + builder + getter）

**Files:** Modify `crates/shogun-core/src/daemon.rs`

- [ ] Step 1: `Db` struct に `compression_config: Option<shogun_fusion::compress::CompressionConfig>` を追加（embedder の隣）。
- [ ] Step 2: 全ての `Db { ... }` 構造体リテラル生成箇所（`new` ほか。`rg "Db \{|Self \{" crates/shogun-core/src/daemon.rs` と他コンストラクタ）に `compression_config: None` を追加。
- [ ] Step 3: builder とゲッターを追加:
```rust
/// 圧縮設定を注入する（未設定＝raw 経路のまま）。embedder と同じ handoff パターン。
pub fn with_compression_config(mut self, config: shogun_fusion::compress::CompressionConfig) -> Self {
    self.compression_config = Some(config);
    self
}
/// 現在の圧縮設定（未設定なら None）。
pub fn compression_config(&self) -> Option<&shogun_fusion::compress::CompressionConfig> {
    self.compression_config.as_ref()
}
```
- [ ] Step 4: `cargo build -p shogun-core --features db` が通る（既存テスト不変）。
- [ ] Step 5: commit `feat(core): Db に compression_config を builder で持たせる (#63)`

## Task 2: 要約 consume（thread_summaries_to_blocks + シグネチャ拡張）

**Files:** Modify `crates/shogun-core/src/daemon.rs`

- [ ] Step 1（失敗テスト）: `mod tests` に、要約があるスレッドで予算逼迫時に要約が残り raw が落ちることを検証するテストを追加:
```rust
#[test]
fn thread_summary_substitutes_for_raw_turns_under_budget() {
    let db = Db::open_in_memory(clock(10_000)).unwrap();
    // 同一スレッドに長めのイベントを複数投入（thread_key はキャプチャで導出）。
    for i in 0..6 {
        db.capture(&ev("vendor renewal pricing discussion detail line", &format!("h{i}"), 100 + i)).unwrap();
    }
    // そのスレッドに短い要約を付与。
    let tk = db.active_threads_between(0, 10_000).first().map(|t| t.thread_key.clone()).unwrap();
    db.set_thread_summary(&tk, "Renewal priced at 12k; awaiting sign-off.");
    let cfg = CompressionConfig { enabled: true, budget_tokens: 12, ..Default::default() };
    let (pack, stats, fell_back) = db.assemble_context_compressed("renewal pricing", 6, 600, &[tk.clone()], &cfg);
    assert!(!fell_back);
    assert!(stats.post_tokens <= 12);
    // 要約テキストが採用され（facts 側 or evidence 側のどこかに要約の語が出る）、
    // 生ターンよりトークン効率が良いので全落ちしない。
    let joined = format!("{} {}", pack.facts.join(" "), pack.evidence.iter().map(|e| e.excerpt.clone()).collect::<Vec<_>>().join(" "));
    assert!(joined.contains("sign-off") || joined.contains("12k"), "summary should survive: {joined}");
}
```
> `ev`/`clock`/`capture` は既存 daemon テストのヘルパ。thread_key 導出は `active_threads_between` から取得（テスト内）。合わなければ既存テストの seed パターンに合わせて調整。
- [ ] Step 2: `thread_summaries_to_blocks` を追加（`&self` メソッド）:
```rust
/// 解決済みスレッドの保存済み要約を ThreadSummary ブロックにする。要約は raw ターンより
/// 短くトークン効率が高いので、高 relevance を与えると予算逼迫時に raw を押しのけて残る
/// （設計 §3.3/§3.4 の差し替えレバー）。summary 未設定のスレッドはスキップ。
fn thread_summaries_to_blocks(
    &self,
    thread_keys: &[String],
    est: &dyn shogun_fusion::budget::TokenEstimator,
) -> Vec<shogun_fusion::block::ContextBlock> {
    use shogun_fusion::block::{BlockRef, ContextBlock, ScoreInputs, SourceKind};
    thread_keys
        .iter()
        .filter_map(|tk| {
            self.thread_summary(tk).map(|s| {
                ContextBlock::new(
                    BlockRef::Thread(tk.clone()),
                    SourceKind::ThreadSummary,
                    s,
                    // 参照先として解決済み＝関連度は高い。confidence は要約＝1.0。
                    ScoreInputs { relevance: 0.9, freshness: 0.7, task_link: 0.5, confidence: 1.0 },
                    est,
                )
            })
        })
        .collect()
}
```
- [ ] Step 3: `assemble_context_compressed` のシグネチャに `thread_keys: &[String]` を追加（`config` の直前）。本文で facts/evidence ブロックの後に `blocks.extend(self.thread_summaries_to_blocks(thread_keys, &est));` を追加。ThreadSummary ブロックは再構成時 `BlockRef::Thread(_)` → fact 側に振り分け（既存の `_ => facts.push(...)` に該当）。
- [ ] Step 4: 既存の呼び出し側を更新（`rg "assemble_context_compressed\(" crates/ apps/`）。daemon.rs テスト・`crates/shogun-core/tests/context_slo.rs` の呼び出しに `&[]`（または該当 thread_keys）を追加。
- [ ] Step 5: `cargo test -p shogun-core --features db` green（新テスト含む）、`cargo test -p shogun-core --features db --test context_slo` green。
- [ ] Step 6: clippy クリーン。
- [ ] Step 7: commit `feat(core): クエリ時に thread.summary を候補投入し差し替えを有効化 (#63)`

## Task 3: 本番チャット経路の分岐配線

**Files:** Modify `apps/desktop/src-tauri/src/inline_source.rs`

- [ ] Step 1: `chat_blocking` で、解決済みスレッドキーを外に取り出す。`let mut resolved_threads: Vec<String> = Vec::new();` を宣言し、`Referent::Resolved` アームで `if let Some(c) = outcome.candidates.first() { resolved_threads.push(c.thread_key.clone()); }` を追加（既存の title fold はそのまま）。
- [ ] Step 2: `let ctx = db.assemble_context(&query, CHAT_EVIDENCE_HITS, CHAT_EVIDENCE_CHARS);` を分岐に置換:
```rust
let ctx = match db.compression_config() {
    Some(cfg) if cfg.enabled => {
        db.assemble_context_compressed(&query, CHAT_EVIDENCE_HITS, CHAT_EVIDENCE_CHARS, &resolved_threads, cfg).0
    }
    _ => db.assemble_context(&query, CHAT_EVIDENCE_HITS, CHAT_EVIDENCE_CHARS),
};
```
> `ThreadCandidate` の `thread_key` フィールド名を実型で確認（`crates/shogun-memory/src/thread.rs` / daemon の `ThreadCandidate`）。異なれば合わせる。
- [ ] Step 3: `cargo check -p <desktop crate>`（クレート名は `apps/desktop/src-tauri/Cargo.toml` の `[package] name` を確認）で通る。
- [ ] Step 4: commit `feat(desktop): チャット経路を compression_config で圧縮パスへ分岐 (#63)`

## Task 4: desktop から env フラグで config 供給

**Files:** Modify `apps/desktop/src-tauri/src/lib.rs`（Db 初期化箇所、`.with_embedder(...)` 付近 / 調査で示された ~1866 行付近）

- [ ] Step 1: Db を組み立てる箇所で、env を読んで有効時のみ config を注入:
```rust
// 圧縮は段階展開: 既定 off。ヘビーユーザー/AB は SHOGUN_COMPRESSION=1 で有効化（設定 UI は次周）。
let db = if std::env::var("SHOGUN_COMPRESSION").map(|v| v == "1" || v.eq_ignore_ascii_case("on")).unwrap_or(false) {
    let budget = std::env::var("SHOGUN_COMPRESSION_BUDGET").ok()
        .and_then(|v| v.parse::<usize>().ok()).unwrap_or(2000);
    db.with_compression_config(shogun_core::/*(実 re-export パス)*/CompressionConfig {
        enabled: true, budget_tokens: budget, ..Default::default()
    })
} else { db };
```
> `CompressionConfig` の desktop からの正しい参照パス（`shogun_core` 再エクスポート or `shogun_fusion::compress::CompressionConfig`）を確認して合わせる。`shogun_fusion` が desktop の依存にあるか（`Cargo.toml`）を確認し、無ければ `shogun_core` 経由の再エクスポートを追加するか、`shogun_fusion` を dev 依存でなく通常依存に追加。
- [ ] Step 2: `cargo check -p <desktop crate>` 通過。
- [ ] Step 3: commit `feat(desktop): SHOGUN_COMPRESSION フラグで圧縮を有効化 (#63)`

## Task 5: 統合確認

- [ ] `cargo test -p shogun-fusion` / `-p shogun-memory` / `-p shogun-core --features db` すべて green。
- [ ] clippy: fusion / core(--features db) / core(no-db) すべて `-D warnings` クリーン。
- [ ] `cargo check` desktop crate 通過。
- [ ] ガード（`scripts/` の egress/secret/migration）実行し green。
- [ ] 不変条件確認: 圧縮は既定 off（env 未設定で挙動不変）/ confidence ゲート前置 / テレメトリはハッシュのみ / キー分離不変。
- [ ] docs 更新: 設計・計画の「休眠」節から①②を「配線済み」に移し、残（sessions.summary / fact 実 id / 設定 UI / Batch 抽象要約器 / AB ダッシュボード）を次周として残す。

## 既知の割り切り（この増分）
- 圧縮時、再構成 Evidence は source/title を落とすため citation のラベルが空になる（event_id は保持＝クリック可）。ラベル復元は次周の小改善。
- 有効化は env フラグ（設定 UI トグルは次周）。
- reply ドラフト経路（`build_reply_context`）は今回未対応（チャット経路のみ）。
