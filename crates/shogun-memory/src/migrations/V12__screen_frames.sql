-- Visual recall frame cache (issue #106, user decision 2026-08-02).
--
-- Explicit exception to invariant 2: when Visual recall is on, compressed JPEG frames from the
-- OCR capture path are retained locally for ≤72 hours, then purged. No audio, no cloud upload.
-- Text + provenance still live in event_log (`source = screen_ocr`); this table holds pixels only.

CREATE TABLE screen_frames (
    id              INTEGER PRIMARY KEY,
    created_at_ms   INTEGER NOT NULL,
    event_id        INTEGER REFERENCES event_log (id),
    app_bundle_id   TEXT,
    window_title    TEXT,
    display_id      INTEGER,
    width           INTEGER NOT NULL,
    height          INTEGER NOT NULL,
    mime            TEXT    NOT NULL DEFAULT 'image/jpeg',
    bytes           BLOB    NOT NULL
) STRICT;

CREATE INDEX idx_screen_frames_created ON screen_frames (created_at_ms);
