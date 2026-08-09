-- L5 lessons / patterns (docs/layer-completion-designs.md §5.1). Additive: three new tables.
--
-- Design rules encoded here:
--  * feedback_events is the raw learning signal — approval-time edits, rejections, unchanged
--    approvals. before/after text is LOCAL ONLY: no egress path touches these tables, and the
--    repository layer never logs their content (CLAUDE.md privacy rule).
--  * lessons follow the state-table discipline: confidence CHECKed to [0, 1], timestamps, and
--    provenance in a separate many-to-many table (same shape as state_provenance) — a lesson
--    with no provenance rows cannot be created through the repository layer.
--  * active is a user-visible switch (Learned UI) AND the lifecycle's sleep flag (contradiction,
--    confidence floor, active-cap eviction). Deactivated lessons keep their rows: evidence and
--    provenance are year-scale data, only injection stops.

-- Raw learning signals (local DB only; never exported, never logged).
CREATE TABLE feedback_events (
    id          INTEGER PRIMARY KEY,
    ts_ms       INTEGER NOT NULL,
    kind        TEXT    NOT NULL CHECK (kind IN ('edit_before_approve', 'reject', 'approve_unchanged', 'state_resolve', 'undo')),
    action_kind TEXT,                               -- e.g. 'draft_reply'
    scope       TEXT    NOT NULL CHECK (scope IN ('global', 'app', 'person', 'project')),
    scope_ref   TEXT,                               -- bundle id / person id / project id
    before_text TEXT,                               -- proposed text (local storage only; egress forbidden)
    after_text  TEXT                                -- approved text
) STRICT;

CREATE INDEX idx_feedback_events_ts ON feedback_events (ts_ms);
CREATE INDEX idx_feedback_events_scope ON feedback_events (scope, scope_ref);

-- Distilled lessons: one prompt-injectable English sentence each.
CREATE TABLE lessons (
    id               INTEGER PRIMARY KEY,
    kind             TEXT    NOT NULL CHECK (kind IN ('style', 'preference', 'correction', 'pattern')),
    scope            TEXT    NOT NULL CHECK (scope IN ('global', 'app', 'person', 'project')),
    scope_ref        TEXT,
    instruction      TEXT    NOT NULL,
    confidence       REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    evidence_count   INTEGER NOT NULL DEFAULT 1,
    active           INTEGER NOT NULL DEFAULT 1,    -- user can switch any lesson off individually
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    last_evidence_at INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_lessons_scope ON lessons (scope, scope_ref);
CREATE INDEX idx_lessons_active ON lessons (active, confidence);

-- Provenance: many-to-many lesson ↔ feedback event (same discipline as state_provenance).
CREATE TABLE lesson_provenance (
    lesson_id         INTEGER NOT NULL REFERENCES lessons (id),
    feedback_event_id INTEGER NOT NULL REFERENCES feedback_events (id),
    PRIMARY KEY (lesson_id, feedback_event_id)
) STRICT;
