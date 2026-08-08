-- Add `asr` to traceability_log.route CHECK (meeting Deepgram STT, 2026-08-05).
-- SQLite cannot ALTER a CHECK in place — rebuild the table.

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
