# V7__sessions.sql — ロールバック手順

対象: `sessions` テーブルの新設と `event_log.session_id` の追加（FR-MT-05）。

## 影響範囲

additive のみ。既存テーブルの列削除・型変更・制約変更は行っていない。
`event_log.session_id` は NULL 許容で、既存行はすべて NULL のまま従来どおり動作する。

## ロールバック

```sql
BEGIN;
DROP INDEX IF EXISTS idx_event_log_session;
DROP INDEX IF EXISTS idx_sessions_open;
DROP INDEX IF EXISTS idx_sessions_started;
DROP TABLE IF EXISTS sessions;
-- SQLite 3.35+ は DROP COLUMN を直接サポートする（macOS 14 同梱版は 3.43 以降）
ALTER TABLE event_log DROP COLUMN session_id;
DELETE FROM refinery_schema_history WHERE version = 7;
COMMIT;
```

## データ損失

- `sessions` の行（会議の区間・要約・決定事項）は**失われる**。
- `event_log` の行は失われない。区間への紐付け（`session_id`）のみ失われる。
- ロールバック前にユーザーデータを残す必要がある場合は `maintenance::export_json` を先に実行する。

## 注意

`ALTER TABLE ... DROP COLUMN` が使えない環境では、`event_log` をテーブル再作成
（CREATE TABLE new → INSERT SELECT → DROP → RENAME）で置き換える。その場合は
`event_fts` のトリガーと `idx_event_log_thread` を張り直すこと。
