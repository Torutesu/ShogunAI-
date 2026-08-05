-- Issue #110: Agent lane をユーザーの既存サブスクで動かす（ローカル公式CLIへの委譲）。
--
-- 委譲経路も egress は egress なので AR-11 のトレーサビリティに記録する。ただし既存の
-- 'messages_api'（BYOK キーで SHOGUN 自身が叩く）とは開示内容が違う——委譲では SHOGUN が
-- 資格情報を持たず、ユーザーのサブスク枠で、ローカルの別プロセスが送信する。トレーサビリティ
-- 画面はこの違いを表示できなければならない（FR-TR-01/02）ので、route を1つ追加する。
--
-- SQLite は CHECK 制約を ALTER できないため、テーブル再作成 + コピー。traceability_log を
-- 参照する外部キーは存在しないので FK 無効化は不要。DROP TABLE は付随インデックスも落とすため
-- idx_traceability_ts を張り直す。
--
-- ロールバック手順:
--   1. 下と同じ手順で CHECK から 'local_agent' を除いたテーブルを作り直す
--   2. コピー前に  DELETE FROM traceability_log WHERE route = 'local_agent';
--      （トレース行は監査記録であり、消すのは CHECK に収まらない行だけに限ること）
--   3. refinery_schema_history から version = 12 の行を削除する
-- 破壊的な列削除・改名は行っていないため、V11 のコードは V12 のスキーマ上でも動作する。

CREATE TABLE traceability_log_v12 (
    id          INTEGER PRIMARY KEY,
    ts          INTEGER NOT NULL,
    route       TEXT    NOT NULL CHECK (route IN ('batch_api', 'messages_api', 'mcp', 'composio', 'billing', 'local_agent')),
    purpose     TEXT    NOT NULL,
    destination TEXT    NOT NULL,
    chunk_bytes INTEGER NOT NULL,
    chunk_xxh64 TEXT    NOT NULL,
    third_party INTEGER NOT NULL DEFAULT 0
) STRICT;

INSERT INTO traceability_log_v12 (id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party)
    SELECT id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party
    FROM traceability_log;

DROP TABLE traceability_log;

ALTER TABLE traceability_log_v12 RENAME TO traceability_log;

CREATE INDEX idx_traceability_ts ON traceability_log (ts);
