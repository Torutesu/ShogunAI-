# V19__feedback_offer_context.sql — ロールバック手順

対象: `feedback_events` に提案時の文脈4列（`surface` / `rank` / `context_app` / `latency_ms`）＋
索引1本を追加（FR-PAT-01 / FR-CF-03 の供給）。

## 影響範囲

additive のみ。既存列・既存行は無変更で、追加列はすべて NULL 許容・デフォルトなし。
V18 以前のコードは 4列を知らないまま `feedback_events` を読み書きできる（INSERT は
列名を明示しているため、追加列は NULL のまま入る）。

`surface` には CHECK が付く（SQLite は NULL を許す限り ADD COLUMN での CHECK を認める）。

## ロールバック

SQLite 3.35 以降は `DROP COLUMN` を持つが、**CHECK 制約を参照する列は DROP できない**ため
`surface` は落とせない。テーブル再作成で戻す。

```sql
BEGIN;

CREATE TABLE feedback_events_v18 (
    id          INTEGER PRIMARY KEY,
    ts_ms       INTEGER NOT NULL,
    kind        TEXT    NOT NULL CHECK (kind IN ('edit_before_approve', 'reject', 'approve_unchanged', 'state_resolve', 'undo')),
    action_kind TEXT,
    scope       TEXT    NOT NULL CHECK (scope IN ('global', 'app', 'person', 'project')),
    scope_ref   TEXT,
    before_text TEXT,
    after_text  TEXT
) STRICT;

-- id を保存すること。lesson_provenance.feedback_event_id が参照している。
INSERT INTO feedback_events_v18
    (id, ts_ms, kind, action_kind, scope, scope_ref, before_text, after_text)
SELECT id, ts_ms, kind, action_kind, scope, scope_ref, before_text, after_text
FROM feedback_events;

DROP INDEX IF EXISTS idx_feedback_events_kind_ts;
DROP INDEX IF EXISTS idx_feedback_events_scope;
DROP INDEX IF EXISTS idx_feedback_events_ts;
DROP TABLE feedback_events;
ALTER TABLE feedback_events_v18 RENAME TO feedback_events;

-- V16 の索引を張り直す（DROP TABLE で付随索引も落ちている）
CREATE INDEX idx_feedback_events_ts ON feedback_events (ts_ms);
CREATE INDEX idx_feedback_events_scope ON feedback_events (scope, scope_ref);

DELETE FROM refinery_schema_history WHERE version = 19;

COMMIT;
```

**`PRAGMA foreign_keys` を OFF にしてから実行すること。** `lesson_provenance` が
`feedback_events(id)` を参照しており、`DROP TABLE` の時点で参照が切れる。`id` をそのまま
コピーしているので RENAME 後に整合は戻るが、FK が有効なままだと途中で落ちる。
実行後に `PRAGMA foreign_key_check;` で確認する。

## データ損失

追加4列の値のみ。`feedback_events` の本体（学習信号そのもの）と `lessons` /
`lesson_provenance` は無傷。

失うのは「どの面で・何番目に提示され・何のアプリで・どれだけ考えて決めたか」であり、
これは**再導出できない**（提示時にしか観測できない）。採択率の分母・分子（`kind` と
`action_kind`）は V16 の列にあるので、`acceptance_by_kind` 相当の集計自体は V18 の
スキーマでも成り立つ。落ちるのはランキング補正の精度。

## 注意

- ロールバック後は `lessons::acceptance_by_kind` と `NewFeedback` の4フィールドを
  V18 以前へ戻すこと。INSERT が存在しない列を指すと全 feedback 記録が失敗し、
  **承認フローそのものが止まる**（記録は承認コマンドの中で走る）。
- `crates/shogun-memory/src/lib.rs` の `LATEST_SCHEMA_VERSION` を 18 に戻す。
