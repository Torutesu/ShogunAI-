# Migration V3 — rollback procedure

Migration: `crates/shogun-memory/src/migrations/V3__job_runs.sql`.
Required by FR-MEM-30.

## What V3 creates

The `job_runs` table (Dream Cycle idempotency + crash-resume, FR-DC-04): one row
per `(cycle_id, kind)` recording the job's state and the event-time input range it
consumed, with a `UNIQUE(cycle_id, kind)` key and an index on `cycle_id`. Additive
over V1/V2 (FR-MEM-31): no existing column or table is touched.

## Rollback

`job_runs` holds only Dream Cycle bookkeeping — no user memory. Dropping it loses
nothing durable: the next Dream Cycle simply starts the current cycle from the first
job (a full re-run, which is upsert-idempotent against state, FR-DC-04). It does not
affect `event_log`, state tables, or `traceability_log`.

```sql
DROP INDEX IF EXISTS idx_job_runs_cycle;
DROP TABLE IF EXISTS job_runs;
DELETE FROM refinery_schema_history WHERE version = 3;
```

After rollback the schema is at V2; the app must run a V2-compatible build (a build
whose `dreamcycle` layer does not read `job_runs`) or re-apply V3.
