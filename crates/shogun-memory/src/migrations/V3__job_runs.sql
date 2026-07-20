-- Dream Cycle job-run ledger (FR-DC-04): idempotency + crash-resume. Additive migration.
--
-- One row per (cycle, job): the job kind, the event-time input range it consumed, and its state.
-- On restart the Dream Cycle skips jobs already `done` for the cycle and re-runs the rest; because
-- the state effects are upsert-idempotent, re-running a killed job cannot corrupt state. The
-- UNIQUE(cycle_id, kind) key makes a job's record a single upsert target across retries.

CREATE TABLE job_runs (
    id            INTEGER PRIMARY KEY,
    cycle_id      TEXT    NOT NULL,                 -- groups a night's jobs (e.g. '20260720')
    kind          TEXT    NOT NULL,                 -- consolidation/compression/state_update/...
    state         TEXT    NOT NULL CHECK (state IN ('pending', 'running', 'done', 'failed')),
    input_from_ts INTEGER NOT NULL,                 -- inclusive start of the consumed range (unix ms)
    input_to_ts   INTEGER NOT NULL,                 -- exclusive end
    updated_at    INTEGER NOT NULL,
    UNIQUE (cycle_id, kind)
) STRICT;

CREATE INDEX idx_job_runs_cycle ON job_runs (cycle_id);
