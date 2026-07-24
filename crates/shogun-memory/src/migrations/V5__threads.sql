-- Threads: the unit a question like "what's the status of that thing?" resolves to
-- (docs/context-layer-audit-and-plan.md §5-3).
--
-- The event log is flat: an email, its reply and the window the user read it in are unrelated
-- rows. Referent resolution needs a grouping to rank and to answer from, so events carry a
-- `thread_key` and threads get their own table with the salience used for ranking.
--
-- Additive only (FR-MEM-31): a nullable column plus a new table. Existing rows keep NULL and
-- behave exactly as before; nothing reads thread_key without handling NULL.

ALTER TABLE event_log ADD COLUMN thread_key TEXT;

-- Lookup is always "this thread, newest first".
CREATE INDEX idx_event_log_thread ON event_log (thread_key, ts);

CREATE TABLE threads (
    id               INTEGER PRIMARY KEY,
    -- Stable per-source identifier: gmail=threadId, slack=channel+thread_ts, github=issue url,
    -- ai_session=session id, capture=normalised app+window title.
    thread_key       TEXT    NOT NULL UNIQUE,
    source           TEXT    NOT NULL,
    title            TEXT,
    -- Filled by the Dream Cycle; NULL until then.
    summary          TEXT,
    participants     TEXT,                            -- JSON array of people.id
    project_id       INTEGER REFERENCES projects (id),
    first_activity_at INTEGER NOT NULL,
    last_activity_at INTEGER NOT NULL,
    event_count      INTEGER NOT NULL DEFAULT 0,
    -- Ranking score for referent resolution ("that thing" → which thread). Recomputed, not
    -- authoritative: recency + open-loop pressure + current-screen agreement.
    salience         REAL    NOT NULL DEFAULT 0.0,
    -- Same contract as every other state row (FR-ST-02): a thread is inferred, so it carries a
    -- confidence and must not be stated as fact below the gate.
    confidence       REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_threads_last_activity ON threads (last_activity_at);
CREATE INDEX idx_threads_salience ON threads (salience);
