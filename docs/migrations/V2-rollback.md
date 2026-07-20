# Migration V2 — rollback procedure

Migration: `crates/shogun-memory/src/migrations/V2__embeddings.sql`.
Required by FR-MEM-30.

## What V2 creates

The sqlite-vec `vec0` virtual table `event_vec` (384-dim, e5-small) holding one
embedding per event, keyed by `rowid = event_log.id`. Additive over V1
(FR-MEM-31): no existing column is touched.

## Rollback

`event_vec` holds only derived data (embeddings recomputable from `event_log`),
so dropping it loses nothing durable:

```sql
DROP TABLE IF EXISTS event_vec;
DELETE FROM refinery_schema_history WHERE version = 2;
```

After rollback, search falls back to the FTS (lexical) half only — `search_hybrid`
with `query_embedding = None` and `search()` behave identically. Re-applying V2
recreates the empty table; the async embed job (FR-MEM-22) repopulates it from
`event_log` over time.

## Extension note

`vec0` requires the sqlite-vec extension to be registered before the connection
opens (`shogun_memory::vector::register_extension`, called by `open` /
`open_in_memory`). A database created at V2 can only be opened by a build that
registers the extension; this is intrinsic to the fixed stack (SQLite + sqlite-vec)
and is not a schema-compatibility break.
