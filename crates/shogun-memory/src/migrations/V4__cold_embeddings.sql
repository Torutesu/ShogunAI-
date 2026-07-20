-- Cold-layer embeddings (FR-MEM-04, ADR-001). The full history at int8 precision, period-partitioned.
-- A Warm embedding (event_vec, f32, brute-force searched) is *demoted* here once its event ages past
-- the Warm window (30 days): the int8 codes + per-vector scale land in this table and the Warm row is
-- removed, keeping the sqlite-vec scan small (FR-MEM-03). The event_log row itself is untouched —
-- Cold is an archive of embeddings, not of events, and FTS still covers the text.
--
-- `partition` is a coarse period bucket (epoch-ms / 30d) so the archive can be pruned or loaded by
-- period without a date library. `ON DELETE CASCADE` keeps Cold consistent when an event is deleted
-- (FR-SET-07 wipe).
--
-- Additive over V1–V3 (FR-MEM-31): no existing table or column is modified.
CREATE TABLE cold_embeddings (
    event_id  INTEGER PRIMARY KEY REFERENCES event_log(id) ON DELETE CASCADE,
    partition INTEGER NOT NULL,
    scale     REAL    NOT NULL,
    dim       INTEGER NOT NULL,
    codes     BLOB    NOT NULL
) STRICT;

CREATE INDEX idx_cold_partition ON cold_embeddings(partition);
