# コンテキスト圧縮の忠実性完成（#63 続き・全Rust）

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** 有効化した圧縮パスの残ギャップを閉じる。①圧縮時に citation の source/title/ts を復元、②`sessions.summary` の populate/consume を threads と対称に、③fact ブロックの provenance を実 state row id に。

**Architecture:** すべて `crates/shogun-core`（daemon / dreamcycle）と `crates/shogun-memory`（session ヘルパ）内で完結。fusion の型（`BlockRef::Session`/`SourceKind::SessionSummary`/`StateTable`）は既存。`inline_memory` の公開 API は不変（ref 版へ委譲）。

**検証:** `cargo test -p shogun-core --features db` / `-p shogun-memory` / `-p shogun-fusion`、clippy 3種、`cargo check -p shogun-desktop-spike`、`cargo clippy -p shogun-desktop-spike --all-targets`（CI と同じ・テストコードも lint）、ガード `scripts/*.py`。前提 worktree: `/Users/torutano/ShogunAI--issue63c`（branch `feat/issue-63-fidelity`）。

---

## Task 1: citation の source/title/ts 復元

**Files:** Modify `crates/shogun-core/src/daemon.rs`（`assemble_context_compressed`, 現 ~854–920）

- [ ] Step 1（失敗テスト）: `mod tests` に、圧縮後も evidence の source/title が保持されることを検証:
```rust
#[test]
fn compressed_evidence_preserves_source_and_title() {
    let db = Db::open_in_memory(clock(10_000)).unwrap();
    for i in 0..3 {
        db.capture(&ev("renewal report pricing detail", &format!("h{i}"), 100 + i)).unwrap();
    }
    let raw = db.assemble_context("renewal", 6, 600);
    assert!(raw.evidence.iter().any(|e| !e.source.is_empty()), "raw has source");
    let cfg = CompressionConfig { enabled: true, budget_tokens: 100_000, ..Default::default() };
    let (pack, _stats, _fell) = db.assemble_context_compressed("renewal", 6, 600, &[], &cfg);
    // 予算十分＝全採用。各 evidence の source/title/ts が raw と一致（0/空でない）。
    for e in &pack.evidence {
        let orig = raw.evidence.iter().find(|o| o.event_id == e.event_id).unwrap();
        assert_eq!(e.source, orig.source);
        assert_eq!(e.title, orig.title);
        assert_eq!(e.ts, orig.ts);
    }
}
```
- [ ] Step 2: 再構成ループの直前に `event_id→&Evidence` マップを作る:
```rust
let ev_by_id: std::collections::HashMap<i64, &Evidence> =
    pack.evidence.iter().map(|e| (e.event_id, e)).collect();
```
- [ ] Step 3: `BlockRef::Event(id)` アームを、マップから ts/source/title を復元する形に置換:
```rust
shogun_fusion::block::BlockRef::Event(id) => {
    let (ts, source, title) = ev_by_id
        .get(&id)
        .map(|e| (e.ts, e.source.clone(), e.title.clone()))
        .unwrap_or((0, String::new(), None));
    evidence.push(Evidence { event_id: id, ts, source, title, excerpt: b.text.clone() });
}
```
- [ ] Step 4: `cargo test -p shogun-core --features db` green（新テスト含む）。clippy クリーン。
- [ ] Step 5: commit `fix(core): 圧縮時に citation の source/title/ts を復元 (#63)`

## Task 2: fact ブロックの実 state id

**Files:** Modify `crates/shogun-core/src/daemon.rs`

> `commitments_due(now)` が `CommitmentRow`（`.id`/`.description`/`.confidence`）を、`open_loops()` が `OpenLoopRow`（同）を返すことを確認（state.rs）。返り型が id を持たない縮約型なら、id を持つ元関数（`list_commitments`/`list_open_loops`）を使う。

- [ ] Step 1（失敗テスト）: fact ブロックが実 id/正しい table を持つことを、圧縮 refs 経由で検証。まず `inline_memory_with_refs` に対する直接テスト:
```rust
#[test]
fn inline_memory_with_refs_carries_real_ids_and_tables() {
    use shogun_fusion::block::StateTable;
    let db = Db::open_in_memory(clock(10_000)).unwrap();
    // 高 confidence の commitment/open_loop を1件ずつ入れる（既存テストの seed ヘルパに倣う）。
    seed_one_commitment_and_one_loop(&db); // ← 既存 seed が無ければ state 挿入ヘルパで用意
    let refs = db.inline_memory_with_refs(8);
    assert!(refs.iter().any(|(_, t, id)| *t == StateTable::Commitments && *id > 0));
    assert!(refs.iter().any(|(_, t, id)| *t == StateTable::OpenLoops && *id > 0));
    // inline_memory(文字列版) と本文が一致（委譲による不変）。
    let strs = db.inline_memory(8);
    assert_eq!(strs, refs.iter().map(|(s, _, _)| s.clone()).collect::<Vec<_>>());
}
```
> seed ヘルパは既存テスト（`test_inline_memory_gates_low_confidence_out` 付近）の state 挿入パターンを流用。無ければ `shogun_memory::state::insert_commitment/insert_open_loop` を provenance 付きで呼ぶ。
- [ ] Step 2: `inline_memory_with_refs` を追加し、`inline_memory` をそれに委譲:
```rust
/// inline_memory の provenance 付き版: (confidence ゲート済み fact, 由来テーブル, row id)。
/// `inline_memory`（文字列版・公開 API）はこれに委譲する（DRY・API 不変）。
fn inline_memory_with_refs(&self, limit: usize) -> Vec<(String, shogun_fusion::block::StateTable, i64)> {
    use shogun_fusion::block::StateTable;
    use shogun_fusion::confidence::{treat_fact, Treatment};
    let mut raw: Vec<(String, f64, StateTable, i64)> = Vec::new();
    for c in self.commitments_due(self.now_ms()) {
        raw.push((format!("you committed: {}", c.description), c.confidence, StateTable::Commitments, c.id));
    }
    for l in self.open_loops() {
        raw.push((format!("open loop: {}", l.description), l.confidence, StateTable::OpenLoops, l.id));
    }
    let mut out = Vec::new();
    for (desc, conf, table, id) in raw {
        match treat_fact(&desc, conf) {
            Treatment::Fact(s) | Treatment::Possible(s) => out.push((s, table, id)),
            Treatment::Excluded => {}
        }
    }
    out.truncate(limit);
    out
}
```
そして既存 `inline_memory` を:
```rust
pub fn inline_memory(&self, limit: usize) -> Vec<String> {
    self.inline_memory_with_refs(limit).into_iter().map(|(s, _, _)| s).collect()
}
```
> 挙動不変の担保: 元実装は commitments→open_loops の順に pairs を作り `assemble_facts`（＝内部で `treat_fact` フィルタ）→`truncate(limit)`。上記も同順・同フィルタ・同 truncate。`treat_fact`/`Treatment` が pub であることを確認（`shogun_fusion::confidence`）。
- [ ] Step 3: `facts_to_blocks` を ref 版に変更:
```rust
fn facts_to_blocks(
    facts: &[(String, shogun_fusion::block::StateTable, i64)],
    est: &dyn TokenEstimator,
) -> Vec<ContextBlock> {
    facts
        .iter()
        .map(|(f, table, id)| {
            ContextBlock::new(
                BlockRef::State { table: *table, id: *id },
                SourceKind::StateFact,
                f.clone(),
                ScoreInputs { relevance: 0.6, freshness: 0.6, task_link: 0.6, confidence: 0.9 },
                est,
            )
        })
        .collect()
}
```
- [ ] Step 4: `assemble_context_compressed` 内の `facts_to_blocks(&pack.facts, &est)` を `facts_to_blocks(&self.inline_memory_with_refs(FACT_LIMIT), &est)` に置換。`FACT_LIMIT` は `assemble_context` が facts に使う limit と一致させる（現状 8。定数化されていなければ `const FACT_LIMIT: usize = 8;` を近傍に置き、`assemble_context` 側の `inline_memory(8)` もこの定数に差し替えて一元化）。
- [ ] Step 5: 既存テスト/呼び出しの更新（`facts_to_blocks(` の呼び出しは圧縮パス内のみ）。`cargo test -p shogun-core --features db` green。clippy クリーン。
- [ ] Step 6: commit `feat(core): fact ブロックに実 state id/table を付与 (#63)`

## Task 3: session ヘルパ（shogun-memory）

**Files:** Modify `crates/shogun-memory/src/session.rs`（thread.rs のヘルパに倣う）

- [ ] Step 1（失敗テスト）: `session.rs` の `mod tests` に、open→attach_event→set_summary(redact)→get_summary→active_between→event_texts→session_ids_for_events の往復を検証するテストを追加（thread.rs の該当テストに倣う）。redaction テスト: `set_summary` に `sk-ant-xxxxxxxxxxxx` を渡し、`get_summary` で秘密が残らないこと。
- [ ] Step 2: 以下を追加（`thread::active_between/event_texts/set_summary/get_summary` と同じ書式・エラー型 `rusqlite::Result`）:
```rust
/// 指定ウィンドウで started_at を持つ session id 群（要約対象）。
pub fn active_between(conn: &Connection, from_ts: i64, to_ts: i64) -> rusqlite::Result<Vec<i64>>;
/// その session に属する event 本文（event_log.session_id = ?、ts,id 昇順）。
pub fn event_texts(conn: &Connection, session_id: i64) -> rusqlite::Result<Vec<crate::event_log::EventText>>;
/// 要約を書き込む（redact 後、updated_at=now）。thread::set_summary と同じく生成テキストは redact する。
pub fn set_summary(conn: &Connection, session_id: i64, summary: &str, now_ms: i64) -> rusqlite::Result<()>;
/// 保存済み要約（無ければ None）。
pub fn get_summary(conn: &Connection, session_id: i64) -> rusqlite::Result<Option<String>>;
/// 与えた event id 群が属する DISTINCT session id（クエリ時の consume 用）。
pub fn session_ids_for_events(conn: &Connection, event_ids: &[i64]) -> rusqlite::Result<Vec<i64>>;
```
> `set_summary` は `crate::redact::redact(summary)` を通してから `UPDATE sessions SET summary=?, updated_at=? WHERE id=?`。`session_ids_for_events` は空入力なら空 Vec を返す（IN 節の空対策）。実装は `SELECT DISTINCT session_id FROM event_log WHERE id IN (…) AND session_id IS NOT NULL`。
- [ ] Step 3: `cargo test -p shogun-memory session::` green。clippy クリーン。
- [ ] Step 4: commit `feat(memory): session の active_between/event_texts/set_summary/get_summary/session_ids_for_events (#63)`

## Task 4: session の populate（Dream Cycle）＋ consume（クエリ時）＋ daemon ラッパ

**Files:** Modify `crates/shogun-core/src/daemon.rs`（ラッパ）、`crates/shogun-core/src/dreamcycle/jobs.rs`（run_compression）

- [ ] Step 1: daemon に session ラッパを追加（既存 thread ラッパ `active_threads_between` 等に倣い、private conn を lock して delegate）:
```rust
pub fn active_sessions_between(&self, from_ts: i64, to_ts: i64) -> Vec<i64>;
pub fn session_event_texts(&self, session_id: i64) -> Vec<shogun_memory::event_log::EventText>;
pub fn set_session_summary(&self, session_id: i64, summary: &str); // now_ms を渡す。best-effort
pub fn session_summary(&self, session_id: i64) -> Option<String>;
pub fn session_ids_for_events(&self, event_ids: &[i64]) -> Vec<i64>;
```
- [ ] Step 2（populate）: `run_compression`（jobs.rs）の threads ループの後に sessions ループを追加:
```rust
for sid in self.db.active_sessions_between(from_ts, to_ts) {
    let events = self.db.session_event_texts(sid);
    if let Some(summary) = summarizer.summarize(&events) {
        self.db.set_session_summary(sid, &summary);
    }
}
```
- [ ] Step 3（consume・失敗テスト）: `assemble_context_compressed` が、retrieved evidence の属する session の要約を候補に含めることを検証するテストを daemon `mod tests` に追加（session を open、event を attach、`set_session_summary` で短い要約、予算逼迫で要約が残り raw が落ちる／十分予算で SessionSummary 由来テキストが pack に出る）。
- [ ] Step 4（consume 実装）: `assemble_context_compressed` の blocks 構築後（thread summary 投入があればその後）に:
```rust
let evidence_event_ids: Vec<i64> = pack.evidence.iter().map(|e| e.event_id).collect();
for sid in self.session_ids_for_events(&evidence_event_ids) {
    if let Some(s) = self.session_summary(sid) {
        blocks.push(shogun_fusion::block::ContextBlock::new(
            shogun_fusion::block::BlockRef::Session(sid),
            shogun_fusion::block::SourceKind::SessionSummary,
            s,
            shogun_fusion::block::ScoreInputs { relevance: 0.85, freshness: 0.7, task_link: 0.5, confidence: 1.0 },
            &est,
        ));
    }
}
```
再構成では `BlockRef::Session(_)` は既存の `_ => facts.push(...)` に落ちる（確認）。
- [ ] Step 5: `cargo test -p shogun-core --features db` green（新テスト含む・既存 dreamcycle テスト不変）。`cargo test -p shogun-core --features db --test context_slo` green。clippy クリーン。
- [ ] Step 6: commit `feat(core): session.summary の populate と クエリ時 consume (#63)`

## Task 5: 統合確認 ＋ docs

- [ ] `cargo test -p shogun-fusion` / `-p shogun-memory` / `-p shogun-core --features db` すべて green。
- [ ] clippy: fusion / core(db) / core(no-db) / **`shogun-desktop-spike --all-targets`** すべて `-D warnings` クリーン（CI と同一。テストコードの lint も含む）。
- [ ] `cargo check -p shogun-desktop-spike` 通過（共有 API を触るため必須）。
- [ ] ガード `scripts/check-http-egress.py` / `check-migrations.py` / `check-secret-exposure.py` すべて green。
- [ ] 不変条件: session/ thread summary は書込前 redact / テレメトリはハッシュのみ / confidence ゲート前置（fact は treat_fact 経由）/ マイグレーション変更なし（新スキーマ不要・sessions.summary 既存）。
- [ ] docs: 設計書の「休眠」節から citation 復元・sessions.summary・fact 実 id を除去し「配線済み」へ。残（設定 UI・reply 経路・Batch 抽象要約器・AB ダッシュボード）を次周に。

## 既知の割り切り
- session の query-time relevance は 0.85 固定（thread 同様、クエリ由来スコアは次周）。
- session consume は「retrieved evidence が属する session」に限定（会議跨ぎの広域探索はしない）。
