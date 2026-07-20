# Migration V1 — rollback procedure

Migration: `crates/shogun-memory/src/migrations/V1__init.sql` (initial schema).
Required by FR-MEM-30 (every migration ships a documented rollback).

## What V1 creates

Tables: `event_log`, `people`, `projects`, `commitments`, `open_loops`,
`state_provenance`, `traceability_log`; the FTS5 virtual table `event_fts` and its
sync triggers (`event_log_ai/ad/au`); plus refinery's own `refinery_schema_history`.

## Rollback

V1 is the initial schema, so "rolling back" means returning to an empty database.
There is no user data to preserve at V1 (it precedes any capture), so the safe and
supported rollback is to **discard the database file** and let the app recreate it:

1. Quit SHOGUN (the resident process must not hold the DB open).
2. Move the database aside (do not delete outright, in case of misdiagnosis):
   ```
   mv "~/Library/Application Support/com.selectkk.shogun/shogun.db" \
      "~/Library/Application Support/com.selectkk.shogun/shogun.db.rollback-bak"
   # also move the -wal and -shm sidecars if present
   ```
3. Relaunch. `shogun_memory::open` recreates the file and re-applies migrations from
   scratch.

## Manual, data-preserving teardown (if ever needed at a later version)

If a future situation requires undoing V1's objects while keeping the file, drop in
reverse dependency order (children/triggers first):

```sql
DROP TRIGGER IF EXISTS event_log_au;
DROP TRIGGER IF EXISTS event_log_ad;
DROP TRIGGER IF EXISTS event_log_ai;
DROP TABLE IF EXISTS event_fts;
DROP TABLE IF EXISTS traceability_log;
DROP TABLE IF EXISTS state_provenance;
DROP TABLE IF EXISTS open_loops;
DROP TABLE IF EXISTS commitments;
DROP TABLE IF EXISTS projects;
DROP TABLE IF EXISTS people;
DROP TABLE IF EXISTS event_log;
DELETE FROM refinery_schema_history WHERE version = 1;
```

Run inside a transaction; `PRAGMA foreign_keys=OFF` first if FK enforcement blocks the
order, then re-enable.

## Compatibility note (FR-MEM-31)

Future migrations must be additive (new tables/columns) — never drop or retype a V1
column, and never make a change that stops an older app version from opening the file,
without a major version bump and explicit notice. The spatial-ready columns
(`display_id` / `window_bounds` / `window_pose` / `gaze_target`) exist in V1 precisely
so they never need a backward-incompatible add later (FR-MEM-12).
