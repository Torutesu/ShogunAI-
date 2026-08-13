# V5__threads.sql — ロールバック手順

対象: `threads` テーブルの新設 + `event_log.thread_key` 列の追加（FR-MEM-31、参照解決の単位）。

## 影響範囲

additive のみ。新規テーブル1つ＋索引3つ＋既存テーブルへの nullable 列1つ。既存行は `thread_key`
が NULL のまま動く。

## ロールバック

```sql
BEGIN;
DROP INDEX IF EXISTS idx_threads_salience;
DROP INDEX IF EXISTS idx_threads_last_activity;
DROP TABLE IF EXISTS threads;
DROP INDEX IF EXISTS idx_event_log_thread;
-- SQLite 3.35+ は DROP COLUMN を持つ。それより古い環境では列を残したままでよい
-- （NULL のまま読み捨てられる）。列を確実に消す必要がある場合だけ event_log を
-- create-copy-drop-rename で作り直すこと。
ALTER TABLE event_log DROP COLUMN thread_key;
DELETE FROM refinery_schema_history WHERE version = 5;
COMMIT;
```

## データ損失

`threads` はイベントログから **Dream Cycle が再導出できる派生インデックス**（タイトル・要約・
参加者・salience）。event_log 本体は無傷なので、失われるのは再生成可能なキャッシュだけ。

## 注意

`thread_key` を参照するコード（`crate::thread`）は V5 以降の前提。ロールバックするなら
アプリ側も V5 以前のリビジョンに戻すこと。
