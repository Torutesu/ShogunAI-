-- Add `asr` to traceability_log.route CHECK (meeting Deepgram STT, 2026-08-05).
-- non-additive-ok: SQLite cannot ALTER a CHECK in place — data-preserving table rebuild
-- (create-copy-drop-rename). Every row is copied; no column is dropped, renamed or retyped,
-- so V12 readers still work against the V13 schema.
--
-- ロールバック手順:
--   1. 同じ手順で CHECK から 'asr' を除いたテーブルを作り直す
--   2. コピー前に  DELETE FROM traceability_log WHERE route = 'asr';
--      （トレース行は監査記録であり、消すのは CHECK に収まらない行だけに限ること）
--   3. refinery_schema_history から version = 13 の行を削除する

CREATE TABLE traceability_log_new (
    id          INTEGER PRIMARY KEY,
    ts          INTEGER NOT NULL,
    route       TEXT    NOT NULL CHECK (route IN ('batch_api', 'messages_api', 'mcp', 'composio', 'billing', 'asr')),
    purpose     TEXT    NOT NULL,
    destination TEXT    NOT NULL,
    chunk_bytes INTEGER NOT NULL,
    chunk_xxh64 TEXT    NOT NULL,
    third_party INTEGER NOT NULL DEFAULT 0
) STRICT;

INSERT INTO traceability_log_new
    (id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party)
SELECT id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party
FROM traceability_log;

DROP TABLE traceability_log;
ALTER TABLE traceability_log_new RENAME TO traceability_log;

CREATE INDEX idx_traceability_ts ON traceability_log (ts);
