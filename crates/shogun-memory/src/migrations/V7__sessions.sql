-- Sessions: the interval the event log cannot express (FR-MT-05, §6.16.2).
--
-- `event_log` is a log of points. A meeting is an interval, so "what was decided in that half
-- hour" has no container: the capture, the chat line and the mail sent during a meeting are
-- unrelated rows. Threads (V5) group by *conversation identity*, which is not the same thing —
-- one meeting touches several threads, and a thread spans many meetings.
--
-- Detection is inference (FR-MT-04): a session carries `confidence` + `provenance` and must not be
-- stated as fact below the gate, exactly like the state tables. `provenance` records *which*
-- signals fired, so a wrong detection can be explained rather than merely observed.
--
-- `kind` is wider than 'meeting' on purpose. 'focus' gives "what was I doing for the last thirty
-- minutes" the same container, which keeps meetings an application of the interval rather than a
-- special case welded into the schema.
--
-- Additive (FR-MEM-31): a new table plus a nullable column. Existing rows keep NULL and behave
-- exactly as before; nothing reads `session_id` without handling NULL.

CREATE TABLE sessions (
    id                     INTEGER PRIMARY KEY,
    -- Constrained rather than free text: an unknown kind is a bug, and SQLite will say so at the
    -- write instead of letting a typo quietly create a fourth category.
    kind                   TEXT    NOT NULL CHECK (kind IN ('meeting', 'call', 'focus')),
    started_at             INTEGER NOT NULL,
    -- NULL means *still open*. The half-open span is [started_at, ended_at) once closed.
    ended_at               INTEGER,
    -- Calendar title when tied to an occurrence, else the window title that triggered detection.
    -- NULL when neither is known — never guessed (FR-MT-15's rule applied to titles).
    title                  TEXT,
    app_bundle_id          TEXT,
    -- Set when detection signal (1) agrees with (2)/(3) (FR-MT-04). NULL for a drop-in meeting.
    -- No FK yet: `calendar_occurrences` arrives with the calendar lane (FR-MT-06); adding the
    -- constraint later is additive, inventing the table early to satisfy a REFERENCES is not.
    calendar_occurrence_id INTEGER,
    -- JSON array of people.id, filled once attendees have a supplier (FR-MT-06).
    participants           TEXT,
    thread_key             TEXT,
    -- Written by Recap (FR-MT-16). NULL until the session is wrapped.
    summary                TEXT,
    decisions              TEXT,
    -- Same contract as every inferred row (FR-ST-02 / FR-MT-04).
    confidence             REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    -- JSON: which detection signals fired. The evidence behind `confidence`.
    provenance             TEXT    NOT NULL,
    created_at             INTEGER NOT NULL,
    updated_at             INTEGER NOT NULL
) STRICT;

-- "Is a session open right now?" runs on every detection tick, so it must not scan. A partial
-- index over the open rows keeps that lookup on an index of (at most) one entry.
CREATE INDEX idx_sessions_open ON sessions (started_at) WHERE ended_at IS NULL;

-- Timeline reads: "the meetings of the last N days", newest first.
CREATE INDEX idx_sessions_started ON sessions (started_at);

-- Events that happened inside an interval. Nullable: most events belong to no session.
ALTER TABLE event_log ADD COLUMN session_id INTEGER REFERENCES sessions (id);

-- Recap reads one interval's events in time order.
CREATE INDEX idx_event_log_session ON event_log (session_id, ts);
