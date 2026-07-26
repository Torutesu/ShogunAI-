-- The user's own notes during a meeting (FR-MT-10). Additive: one new table.
--
-- During a meeting the expanded panel is a place to *type*, not a transcript to watch — live
-- transcription is deliberately not shown (§6.16.3). What the user writes is the one part of the
-- record that is unambiguously theirs, so it is stored whole and Recap builds around it rather
-- than over it.
--
-- One row per session, not an append log: typing is continuous, so the note is a document being
-- edited. UNIQUE on session_id makes that structural instead of a convention the writer has to
-- remember, and gives the upsert its conflict target.

CREATE TABLE session_notes (
    id         INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL UNIQUE REFERENCES sessions (id),
    body       TEXT    NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
