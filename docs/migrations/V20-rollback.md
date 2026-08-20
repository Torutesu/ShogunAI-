# V20__voice_dictionary.sql — rollback procedure

Target: user-managed voice dictionary tables `voice_terms` and `voice_term_aliases`.

## Impact

This migration is additive. Older builds ignore both tables. Voice dictation falls back to its
built-in local aliases when these tables are absent.

## Rollback

Do not launch V19 while V20 remains in refinery history. This permanently removes user-managed
voice terms and aliases. There is no export endpoint; make a manual backup of the encrypted
database before continuing.

1. Stop every SHOGUN desktop, API, and MCP process.
2. Copy the encrypted database file and keep that copy offline. This is the only recovery path for
   user-managed terms after rollback.
3. While V20-capable tooling is still installed, open the database and execute the transaction
   below.
4. Verify `refinery_schema_history` has no version 20 row and `PRAGMA foreign_key_check` returns
   no rows. Confirm both voice tables are absent.
5. Only then install and launch the V19 build.

```sql
BEGIN;

DROP TABLE voice_term_aliases;
DROP TABLE voice_terms;

DELETE FROM refinery_schema_history WHERE version = 20;

COMMIT;
```

## Data loss

All user-managed canonical terms, aliases, scopes, locales, priorities, and enabled state are
removed. Built-in dictionary aliases are compiled into the app and are unaffected.
