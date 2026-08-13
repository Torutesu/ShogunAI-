-- The persisted nightly Morning Brief (Plan C-1, §6.8 / FR-MB-01..06). Additive: one new table.
--
-- One row per local calendar day: the Dream Cycle's MorningBrief job assembles the brief at night
-- and UPSERTs it here, so the morning display is a read — immediate and offline-stable — instead
-- of a live degraded assembly. `payload` is the BriefPayload JSON (shogun_memory::briefs);
-- `generated` records whether model prose was attached (0 = extractive honest degradation,
-- FR-MB-04). `prev_digest` keeps the digest of the payload this row replaced, which is what makes
-- the FR-MB-06 "Updated" mark derivable (current payload digest != prev_digest).
CREATE TABLE briefs (
    date        TEXT    PRIMARY KEY,  -- 'YYYY-MM-DD', local calendar day
    payload     TEXT    NOT NULL,     -- BriefPayload JSON
    generated   INTEGER NOT NULL,     -- 1 when generated prose was attached (Batch lane)
    built_at    INTEGER NOT NULL,     -- unix ms of the write
    prev_digest TEXT                  -- digest of the replaced payload (FR-MB-06 updated mark)
) STRICT;
