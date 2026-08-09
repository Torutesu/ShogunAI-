-- LessonDistillation watermark (Plan D-4). The nightly Dream job consumes feedback_events
-- strictly above `last_processed_feedback_id` and advances it after a successful pass, so a
-- crash-resume re-run re-reads the same unprocessed window (safe: lesson upserts dedupe
-- evidence) and a completed pass never re-consumes old feedback (which would wrongly refresh
-- lesson decay with stale evidence).
--
-- Single-row table (id CHECKed to 1), seeded at 0 = "nothing processed yet".
-- Rollback: DROP TABLE lesson_distill_meta;
CREATE TABLE lesson_distill_meta (
    id                         INTEGER PRIMARY KEY CHECK (id = 1),
    last_processed_feedback_id INTEGER NOT NULL DEFAULT 0
) STRICT;

INSERT INTO lesson_distill_meta (id, last_processed_feedback_id) VALUES (1, 0);
