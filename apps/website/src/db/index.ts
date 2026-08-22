import { getCloudflareContext } from '@opennextjs/cloudflare';
import { drizzle } from 'drizzle-orm/postgres-js';
import postgres from 'postgres';
import * as schema from './schema';

const LOCAL_FALLBACK = 'postgres://postgres:postgres@localhost:5432/shogun_waitlist';

/**
 * The Hyperdrive binding's connection string, when one is bound and we are inside a request.
 *
 * Hyperdrive terminates the connection inside Cloudflare's network, so the Worker never opens a
 * raw socket through the `nodejs_compat` shim — the layer that fails in production with `proxy
 * request failed, cannot connect to the specified address`, taking roughly half of all database
 * calls with it.
 *
 * `getCloudflareContext()` throws outside a Worker request, and `next dev`, the test suites and
 * `db:migrate` all reach this module that way. The throw is the signal to fall back, not an
 * error: returning null here is the normal path everywhere except production.
 */
function hyperdriveUrl(): string | null {
  try {
    const env = getCloudflareContext().env as { HYPERDRIVE?: { connectionString?: string } };
    return env.HYPERDRIVE?.connectionString ?? null;
  } catch {
    return null;
  }
}

/**
 * `||` rather than `??`, and trimmed, to match src/db/migrate.ts: a whitespace-only secret falls
 * back to the local default instead of being sent as a connection string.
 */
function directUrl(): string {
  return process.env.DATABASE_URL?.trim() || LOCAL_FALLBACK;
}

type Client = ReturnType<typeof postgres>;
type Db = ReturnType<typeof drizzle<typeof schema>>;

let cached: { url: string; client: Client; db: Db } | null = null;

/**
 * Resolve the client, re-creating it only when the connection string changes.
 *
 * Deliberately lazy. The Hyperdrive binding exists only inside a request, and this module is
 * evaluated once per isolate at import time — so reading it eagerly would always miss, and every
 * production query would silently take the broken direct path instead.
 */
function resolve(): { url: string; client: Client; db: Db } {
  const viaHyperdrive = hyperdriveUrl();
  const url = viaHyperdrive ?? directUrl();
  if (cached?.url === url) return cached;

  const client = postgres(url, {
    /**
     * One per isolate. Cloudflare runs many isolates at once, so `max` is multiplied by however
     * many are live rather than being a cap on what the site opens — and a route handler here
     * awaits its queries one at a time, so a second connection buys nothing.
     */
    max: 1,
    /**
     * Without these, a socket that never completes is not an error but a wait with no end, and
     * the Workers runtime kills the request rather than letting the route return its own 500.
     */
    connect_timeout: 10,
    idle_timeout: 20,
    /**
     * Skip the type-introspection round trip when Hyperdrive is in front. This follows
     * Cloudflare's documented postgres.js example rather than a measurement of our own, and is
     * left on for direct connections, where the query is local and cheap.
     */
    ...(viaHyperdrive ? { fetch_types: false } : {}),
  });

  cached = { url, client, db: drizzle(client, { schema }) };
  return cached;
}

/**
 * `db` and `client` stay value exports so no call site changes, but each defers to `resolve()` on
 * first use — which is inside a request, where the binding is readable.
 *
 * Methods are bound to the real instance: handing back an unbound function would call it with the
 * proxy as `this`, and postgres.js and drizzle both rely on their own internals.
 */
function lazy<T extends object>(pick: () => T): T {
  return new Proxy({} as T, {
    get(_target, prop) {
      const real = pick() as Record<string | symbol, unknown>;
      const value = real[prop];
      return typeof value === 'function' ? value.bind(real) : value;
    },
    has: (_target, prop) => prop in (pick() as object),
  });
}

export const db = lazy<Db>(() => resolve().db);
export const client = lazy<Client>(() => resolve().client);
export * from './schema';
