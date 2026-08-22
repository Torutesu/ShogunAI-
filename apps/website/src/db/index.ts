import { getCloudflareContext } from '@opennextjs/cloudflare';
import { drizzle } from 'drizzle-orm/postgres-js';
import postgres from 'postgres';
import * as schema from './schema';

const LOCAL_FALLBACK = 'postgres://postgres:postgres@localhost:5432/shogun_waitlist';

/**
 * Hyperdrive terminates the connection inside Cloudflare's network, so the Worker never opens a
 * raw socket to Supabase through the `nodejs_compat` shim. It is preferred wherever it is bound.
 *
 * `getCloudflareContext()` throws outside a Worker request, and `next dev`, the test suites and
 * `db:migrate` all reach this module that way. The throw is the signal to fall back, not an
 * error: it is the normal path everywhere except production.
 */
let announced = false;

/**
 * Which transport the last `resolve()` picked, as a fixed token safe to hand to a caller.
 *
 * Deliberately a closed set, not a free-form string: it names the transport and nothing about
 * the host, the credentials or the error text, so an error response can carry it without leaking
 * anything. `unresolved` means no query has been attempted in this isolate yet.
 */
export type ConnectionMode = 'hyperdrive' | 'direct-binding-missing' | 'direct-no-context' | 'unresolved';

let mode: ConnectionMode = 'unresolved';

/** The transport this isolate is using. See ConnectionMode. */
export function connectionMode(): ConnectionMode {
  return mode;
}

type CfContext = { env: { HYPERDRIVE?: { connectionString?: string } } };

/**
 * The current request's Cloudflare context, or null when there is no request.
 *
 * OpenNext runs each request inside `AsyncLocalStorage.run({ env, ctx, cf }, …)` with a fresh
 * object literal, so the value this returns is a per-request identity — which is what makes it
 * usable as the cache key below.
 */
function requestContext(): CfContext | null {
  try {
    return getCloudflareContext() as unknown as CfContext;
  } catch (e) {
    mode = 'direct-no-context';
    announce(`direct: the Cloudflare context was unreadable: ${(e as Error).message}`);
    return null;
  }
}

function hyperdriveUrl(cf: CfContext): string | null {
  const url = cf.env.HYPERDRIVE?.connectionString ?? null;
  mode = url ? 'hyperdrive' : 'direct-binding-missing';
  announce(url ? 'hyperdrive' : 'direct: the HYPERDRIVE binding is missing from this deployment');
  return url;
}

/**
 * Say which connection this isolate chose, once, in production.
 *
 * Both outcomes are logged, not just the fallback. Silence is not evidence — a log that only
 * appears when something is wrong cannot distinguish "the binding works" from "these requests
 * never got far enough to check".
 *
 * Firing once per isolate rather than once per request turned out to matter more than the text:
 * it made "this isolate is on its first request" visible in the log, and lining that column up
 * against the status column is what identified the reuse bug that `perRequest` below fixes.
 */
function announce(how: string): void {
  if (announced || process.env.NODE_ENV !== 'production') return;
  announced = true;
  console.error(`db: connecting via ${how}`);
}

/**
 * Reset the recorded transport and drop the off-request client. Tests only.
 */
export function resetConnectionModeForTests(): void {
  mode = 'unresolved';
  announced = false;
  shared = null;
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

type Entry = { url: string; client: Client; db: Db };

/**
 * Clients are scoped to the request that opened them.
 *
 * A Worker may not reuse a socket across requests — Cloudflare's TCP socket documentation states
 * it outright ("TCP sockets cannot be created in global scope and shared across requests. You
 * should always create TCP sockets within a handler"). Caching one client for the isolate broke
 * exactly that rule, and production showed the consequence with no exceptions in 40 requests:
 * every request that opened the connection itself answered 404 (a completed read), and every
 * request that inherited one from an earlier request hung until its 3s deadline and answered 500.
 *
 * The key is the request context object, which OpenNext creates fresh per request. A WeakMap
 * rather than a single slot so that two requests interleaving in one isolate keep their own
 * client instead of evicting each other's, and so both are collectable once the requests end.
 */
const perRequest = new WeakMap<object, Entry>();

/** The one client used where there is no request: tests, `next dev`, `db:migrate`. */
let shared: Entry | null = null;

function open(url: string, viaHyperdrive: boolean): Entry {
  const client = postgres(url, {
    /**
     * One per request. A route handler awaits its queries one at a time, so a second connection
     * buys nothing, and the client no longer outlives the request that opened it.
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
  return { url, client, db: drizzle(client, { schema }) };
}

/**
 * Resolve the client for whoever is asking.
 *
 * Deliberately lazy. The Hyperdrive binding exists only inside a request, and this module is
 * evaluated once per isolate at import time — so reading it eagerly would always miss, and every
 * production query would silently take the direct path instead.
 *
 * `client.end()` is never called here. postgres.js refuses queries once a client has ended, and
 * this runs on first property access rather than at the close of a handler we control, so an
 * eager end would break the very request that opened the connection. The runtime closes
 * request-scoped I/O when the request finishes, which is what the per-request scope buys.
 */
function resolve(): Entry {
  const cf = requestContext();
  if (!cf) {
    const url = directUrl();
    if (shared?.url !== url) shared = open(url, false);
    return shared;
  }

  const hit = perRequest.get(cf);
  if (hit) return hit;

  const viaHyperdrive = hyperdriveUrl(cf);
  const entry = open(viaHyperdrive ?? directUrl(), viaHyperdrive !== null);
  perRequest.set(cf, entry);
  return entry;
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
