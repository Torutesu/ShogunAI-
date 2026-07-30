-- The polished layer of a meeting note (FR-MTUX-03). Additive: one new table.
--
-- Notes taken during a meeting are fragments — half sentences, a name, an arrow. After the
-- meeting the model reads them alongside the transcript and writes them up. That write-up is
-- useful and it is also *not what the user wrote*, so it lives in its own table rather than
-- replacing `session_notes`.
--
-- Two layers, not one column: the user can always return to their own words, and a re-run of the
-- enhancement can never destroy the only copy of them. A single `body` that the Batch job
-- overwrote would be a silent, unrecoverable edit of the one artefact in this database that is
-- unambiguously the user's.
--
-- UNIQUE on session_id for the same reason as `session_notes`: this is a document per meeting,
-- regenerated in place, not an append log — and it gives the upsert its conflict target.

CREATE TABLE session_notes_enhanced (
    id           INTEGER PRIMARY KEY,
    session_id   INTEGER NOT NULL UNIQUE REFERENCES sessions (id),
    body         TEXT    NOT NULL,
    generated_at INTEGER NOT NULL
) STRICT;
