-- The generated meeting minutes (MT4, §6.16): summary + decisions + next actions, built by the
-- Select KK Batch lane from the transcript and the user's notes. Additive: one new table.
--
-- One row per session (UNIQUE session_id): a meeting has exactly one set of minutes, replaced if
-- regenerated — like session_notes, this is a document, not a log. decisions / next_actions are
-- JSON arrays of text (next_actions carry an optional owner). `model` is provenance: which model
-- wrote these minutes.
CREATE TABLE meeting_recaps (
    id           INTEGER PRIMARY KEY,
    session_id   INTEGER NOT NULL UNIQUE REFERENCES sessions (id),
    summary      TEXT    NOT NULL,
    decisions    TEXT    NOT NULL,
    next_actions TEXT    NOT NULL,
    model        TEXT    NOT NULL,
    created_at   INTEGER NOT NULL
) STRICT;
