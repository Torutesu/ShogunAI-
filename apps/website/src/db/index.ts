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
 * One, not ten. This module is evaluated once per Worker isolate, and Cloudflare runs many
 * isolates concurrently across colos — so `max` is a per-isolate figure multiplied by however
 * many isolates are live, not a cap on what the site opens. Against Supabase's session pooler,
 * which admits a bounded number of clients, ten apiece oversubscribes it: the pooler starts
 * refusing new connections, and because postgres.js surfaces connection errors asynchronously the
 * refusal escapes the route's try/catch and takes the whole request out as an unhandled Worker
 * exception rather than a clean 500.
 *
 * A route handler here awaits its queries one at a time, so a second connection buys nothing.
 * Locally `next dev` re-evaluates modules on every edit, hence the globalThis cache — without it
 * a long session leaks a pool per reload.
 */
const client =
  (globalThis as { _pg?: ReturnType<typeof postgres> })._pg ?? postgres(connectionString, { max: 1 });
if (process.env.NODE_ENV !== 'production') {
  (globalThis as { _pg?: ReturnType<typeof postgres> })._pg = client;
}

export const db = drizzle(client, { schema });
export { client };
export * from './schema';
