-- The transcript of a meeting, as text only (FR-MT-13, §6.16.2). Additive: one new table.
--
-- Invariant 2: audio is processed on-device and never stored — the waveform lives only in a RAM
-- ring buffer and is discarded after ASR. What persists is this text plus its provenance, nothing
-- that can reconstruct the sound.
--
-- `speaker` is 'me' (microphone) or 'other' (system tap); NULL means unknown and is never guessed.
-- `origin` is 'asr' here; 'caption' is reserved for a future path that reads the meeting UI's own
-- captions, which carries a different consent story (§5), so the column exists from the start.
-- `confidence` is the model's own certainty, normalised to [0,1] — a low-confidence line must not
-- be presented downstream as fact (data-model principle).

CREATE TABLE transcript_segments (
    id         INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES sessions (id),
    ts         INTEGER NOT NULL,
    speaker    TEXT,
    text       TEXT    NOT NULL,
    origin     TEXT    NOT NULL,
    confidence REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    created_at INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_transcript_session ON transcript_segments (session_id, ts);
