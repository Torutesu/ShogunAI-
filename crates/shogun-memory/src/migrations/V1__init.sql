-- SHOGUN v1 initial schema (docs/requirements-v1.0.md §6.3–§6.4, FR-MEM/FR-ST).
--
-- Design rules encoded here:
--  * event_log is append-only (FR-MEM-10); the only UPDATEs allowed by the repository layer
--    are the dedup touch (last_seen_at/dwell_ms, FR-CAP-03).
--  * spatial-ready columns exist from V1 and must never be added later (FR-MEM-12):
--    display_id / window_bounds / window_pose / gaze_target.
--  * state tables are physically separate from event_log (FR-ST-01) and every state row
--    carries confidence + timestamps (FR-ST-02); provenance is a separate many-to-many table.
--  * traceability_log stores only a digest of any externally-sent chunk, never its text
--    (CLAUDE.md privacy rule / G8).
--
-- Deferred to later (additive, FR-MEM-31) migrations, when they are first populated:
--  * sqlite-vec Warm-layer embedding table + int8 Cold-layer partitions (with WP2.5 embeddings).
--  * Cold-layer month partitions (with the Dream Cycle layer move, M3).

-- ---------------------------------------------------------------- event log
CREATE TABLE event_log (
    id            INTEGER PRIMARY KEY,
    ts            INTEGER NOT NULL,                 -- occurrence time, unix ms
    source        TEXT    NOT NULL,                 -- capture/gmail/gcal/slack/notion/github/linear/agent/user
    kind          TEXT    NOT NULL,                 -- text/focus/message/event/action_executed/...
    app_bundle_id TEXT,
    window_title  TEXT,
    content       TEXT    NOT NULL,
    content_hash  TEXT    NOT NULL,                 -- dedup key (FR-CAP-03)
    last_seen_at  INTEGER NOT NULL,                 -- dedup touch
    dwell_ms      INTEGER NOT NULL DEFAULT 0,       -- spatial-ready dwell
    display_id    INTEGER,                          -- spatial-ready
    window_bounds TEXT,                             -- spatial-ready (JSON)
    window_pose   TEXT,                             -- spatial-ready (JSON, NULL in v1)
    gaze_target   TEXT                              -- spatial-ready (NULL in v1)
) STRICT;

CREATE INDEX idx_event_log_ts ON event_log (ts);
CREATE INDEX idx_event_log_source ON event_log (source);
CREATE INDEX idx_event_log_content_hash ON event_log (content_hash);

-- FTS5 trigram over content + title (FR-MEM-20). External-content table keyed on event_log.id,
-- kept in sync by triggers so full-text search covers all history (FR-MEM-03a).
CREATE VIRTUAL TABLE event_fts USING fts5(
    content,
    window_title,
    content='event_log',
    content_rowid='id',
    tokenize='trigram'
);

CREATE TRIGGER event_log_ai AFTER INSERT ON event_log BEGIN
    INSERT INTO event_fts (rowid, content, window_title)
    VALUES (new.id, new.content, new.window_title);
END;

CREATE TRIGGER event_log_ad AFTER DELETE ON event_log BEGIN
    INSERT INTO event_fts (event_fts, rowid, content, window_title)
    VALUES ('delete', old.id, old.content, old.window_title);
END;

CREATE TRIGGER event_log_au AFTER UPDATE ON event_log BEGIN
    INSERT INTO event_fts (event_fts, rowid, content, window_title)
    VALUES ('delete', old.id, old.content, old.window_title);
    INSERT INTO event_fts (rowid, content, window_title)
    VALUES (new.id, new.content, new.window_title);
END;

-- ---------------------------------------------------------------- state tables
-- Common columns on every state table (FR-ST-02): confidence + timestamps. provenance lives
-- in state_provenance. `confidence` is 0.0..1.0; a CHECK keeps it in range.

CREATE TABLE people (
    id                  INTEGER PRIMARY KEY,
    display_name        TEXT    NOT NULL,
    aliases             TEXT,                        -- JSON array (name matching)
    emails              TEXT,                        -- JSON array
    handles             TEXT,                        -- JSON array (slack/github/...)
    relationship_summary TEXT,
    last_interaction_at INTEGER,
    interaction_channel TEXT,
    pending_from_me     TEXT,                        -- JSON (open_loops cache)
    pending_from_them   TEXT,                        -- JSON
    confidence          REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    last_evidence_at    INTEGER NOT NULL
) STRICT;

CREATE TABLE projects (
    id               INTEGER PRIMARY KEY,
    name             TEXT    NOT NULL,
    status           TEXT    NOT NULL CHECK (status IN ('active', 'waiting', 'paused', 'done')),
    summary          TEXT,
    participants     TEXT,                           -- JSON array of people.id
    sources          TEXT,                           -- JSON array of external identifiers
    last_activity_at INTEGER,
    confidence       REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    last_evidence_at INTEGER NOT NULL
) STRICT;

CREATE TABLE commitments (
    id             INTEGER PRIMARY KEY,
    direction      TEXT    NOT NULL CHECK (direction IN ('mine', 'theirs')),
    counterparty_id INTEGER REFERENCES people (id),
    description    TEXT    NOT NULL,
    due_at         INTEGER,
    status         TEXT    NOT NULL CHECK (status IN ('open', 'done', 'overdue', 'cancelled')),
    project_id     INTEGER REFERENCES projects (id),
    confidence     REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    last_evidence_at INTEGER NOT NULL
) STRICT;

CREATE TABLE open_loops (
    id             INTEGER PRIMARY KEY,
    kind           TEXT    NOT NULL CHECK (kind IN ('reply_needed', 'waiting_on_them', 'review_pending', 'decision_pending', 'follow_up', 'other')),
    description    TEXT    NOT NULL,
    counterparty_id INTEGER REFERENCES people (id),
    project_id     INTEGER REFERENCES projects (id),
    opened_at      INTEGER NOT NULL,
    staleness_days INTEGER NOT NULL DEFAULT 0,
    status         TEXT    NOT NULL CHECK (status IN ('open', 'closed')),
    confidence     REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    last_evidence_at INTEGER NOT NULL
) STRICT;

-- provenance: many-to-many state ↔ event (FR-ST-02). state_table names the owning table.
CREATE TABLE state_provenance (
    id          INTEGER PRIMARY KEY,
    state_table TEXT    NOT NULL CHECK (state_table IN ('people', 'projects', 'commitments', 'open_loops')),
    state_id    INTEGER NOT NULL,
    event_id    INTEGER NOT NULL REFERENCES event_log (id),
    weight      REAL    NOT NULL DEFAULT 1.0
) STRICT;

CREATE INDEX idx_state_provenance ON state_provenance (state_table, state_id);
CREATE INDEX idx_state_provenance_event ON state_provenance (event_id);

-- ---------------------------------------------------------------- traceability
-- One row per external send (AR-11 / §6.14). Stores the chunk's byte length + digest only —
-- never the sent text (privacy rule / G8). `third_party` marks Composio-routed sends.
CREATE TABLE traceability_log (
    id          INTEGER PRIMARY KEY,
    ts          INTEGER NOT NULL,
    route       TEXT    NOT NULL CHECK (route IN ('batch_api', 'messages_api', 'mcp', 'composio', 'billing')),
    purpose     TEXT    NOT NULL,
    destination TEXT    NOT NULL,
    chunk_bytes INTEGER NOT NULL,
    chunk_xxh64 TEXT    NOT NULL,
    third_party INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE INDEX idx_traceability_ts ON traceability_log (ts);
