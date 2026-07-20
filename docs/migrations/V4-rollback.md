# Migration V4 — rollback procedure

Migration: `crates/shogun-memory/src/migrations/V4__cold_embeddings.sql`.
Required by FR-MEM-30.

## What V4 creates

The `cold_embeddings` table (Cold-layer archive, FR-MEM-04): one row per demoted
event holding its int8-quantized embedding (`codes` BLOB + per-vector `scale`),
`dim`, and a coarse period `partition` bucket, keyed by `event_id` with
`ON DELETE CASCADE` to `event_log`, plus an index on `partition`. Additive over
V1–V3 (FR-MEM-31): no existing column or table is touched.

## Rollback

`cold_embeddings` holds only *derived* data — int8 copies of Warm f32 embeddings,
which are themselves recomputable from `event_log.content` by the embedding job
(FR-MEM-22). Dropping it loses no user memory: events, state tables, FTS, and the
traceability log are untouched. The only effect is that already-demoted events lose
their archived vector until re-embedded; text search (FTS) still finds them.

```sql
DROP INDEX IF EXISTS idx_cold_partition;
DROP TABLE IF EXISTS cold_embeddings;
DELETE FROM refinery_schema_history WHERE version = 4;
```

After rollback the schema is at V3; the app must run a V3-compatible build (whose
`cold` layer and the Dream Cycle ColdDemotion job are absent or disabled) or
re-apply V4. Note: a V3 build performs no Warm→Cold demotion, so the Warm vector
set (`event_vec`) will grow unbounded over time — re-apply V4 before long-horizon
use to keep the sqlite-vec scan bounded (FR-MEM-03).
