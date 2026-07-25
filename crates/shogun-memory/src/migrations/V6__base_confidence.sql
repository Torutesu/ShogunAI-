-- Make confidence decay recomputable instead of accumulating (FR-ST-21). Additive migration.
--
-- `recompute::decay_confidence` multiplied each row's *stored* confidence by
-- 0.5^((now - last_evidence_at) / half_life) and wrote the product back. Run once a night that is
-- roughly the intended curve; run it hourly — which the on-device maintenance job does — and the
-- factors compound, so a row 30 days past its last evidence has been multiplied by a half-life
-- exponent of ~360 rather than 1. Every extracted state row collapses below the Medium band within
-- days and Context Fusion stops seeing it at all.
--
-- The fix is to keep the pre-decay value. `confidence` becomes a derived column —
-- base_confidence * 0.5^(elapsed / half_life) — which is idempotent by construction, and which also
-- lets a row recover when fresh evidence moves `last_evidence_at` forward.
--
-- Backfill note: existing rows can only be seeded from their current (possibly over-decayed)
-- confidence — the original is not recoverable. `recompute::corroborate` raises the base again for
-- any row with more than one piece of evidence behind it, so multi-evidence state repairs itself on
-- the next maintenance pass; single-sighting rows keep the value they have.
--
-- No CHECK on the new column: SQLite's ADD COLUMN cannot carry one, so the [0,1] bound is enforced
-- in Rust at every write (the derived `confidence` still has V1's CHECK behind it).

ALTER TABLE people      ADD COLUMN base_confidence REAL NOT NULL DEFAULT 0.0;
ALTER TABLE projects    ADD COLUMN base_confidence REAL NOT NULL DEFAULT 0.0;
ALTER TABLE commitments ADD COLUMN base_confidence REAL NOT NULL DEFAULT 0.0;
ALTER TABLE open_loops  ADD COLUMN base_confidence REAL NOT NULL DEFAULT 0.0;

UPDATE people      SET base_confidence = confidence;
UPDATE projects    SET base_confidence = confidence;
UPDATE commitments SET base_confidence = confidence;
UPDATE open_loops  SET base_confidence = confidence;
