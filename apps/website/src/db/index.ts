import { drizzle } from 'drizzle-orm/postgres-js';
import postgres from 'postgres';
import * as schema from './schema';

// `||` rather than `??`, and trimmed, to match src/db/migrate.ts: a whitespace-only secret falls
// back to the local default instead of being sent as a connection string.
const connectionString =
  process.env.DATABASE_URL?.trim() ||
  'postgres://postgres:postgres@localhost:5432/shogun_waitlist';

/**
 * Connections per client.
 *
 * One, not ten, on the merits: this module is evaluated once per Worker isolate and Cloudflare
 * runs many isolates at once, so `max` is a per-isolate figure multiplied by however many are
 * live rather than a cap on what the site opens — and a route handler here awaits its queries one
 * at a time, so the other nine bought nothing.
 *
 * It is **not** the cause of the intermittent failures, which is worth recording because it was
 * offered as one. Measured before and after: 6/10 requests served, then 8/20. Unchanged. The
 * cause is one layer down — `proxy request failed, cannot connect to the specified address`, the
 * `nodejs_compat` socket shim failing to reach the pooler. Hyperdrive is the supported path off
 * that shim; see DEPLOY.md.
 *
 * Locally `next dev` re-evaluates modules on every edit, hence the globalThis cache — without it
 * a long session leaks a pool per reload.
 */
const client =
  (globalThis as { _pg?: ReturnType<typeof postgres> })._pg ??
  postgres(connectionString, {
    max: 1,
    /**
     * Without these, a socket that never completes is not an error — it is a wait with no end.
     * The Workers runtime eventually kills the request itself ("detected that your Worker's code
     * had hung and would never..."), which arrives as a dead request rather than as the clean 500
     * every route here is written to return. A bounded connect turns that back into a rejected
     * promise the route can catch.
     */
    connect_timeout: 10,
    /** Release the pooler slot when the isolate goes quiet, rather than holding it until eviction. */
    idle_timeout: 20,
  });
if (process.env.NODE_ENV !== 'production') {
  (globalThis as { _pg?: ReturnType<typeof postgres> })._pg = client;
}

export const db = drizzle(client, { schema });
export { client };
export * from './schema';
