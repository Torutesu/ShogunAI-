import assert from 'node:assert/strict';
import { test } from 'node:test';

import { client, connectionMode, db, resetConnectionModeForTests } from '../src/db/index.ts';

/**
 * How the database layer scopes its connections, and what it says about them.
 *
 * Production spent a launch morning failing 20 of every 40 licence reads. The split was not
 * random: every request that opened its own connection answered 404 (a completed read), and every
 * request that inherited one from an earlier request in the same isolate hung to its deadline and
 * answered 500. Cloudflare's TCP socket documentation says why — "TCP sockets cannot be created
 * in global scope and shared across requests" — and these tests hold the module to that rule.
 */

/** The global slot OpenNext puts the per-request context in. */
const CONTEXT = Symbol.for('__cloudflare-context__');

let current: unknown;
Object.defineProperty(globalThis, CONTEXT, { get: () => current, configurable: true });

/** Stand in for one request. OpenNext passes a fresh object per request; so do we. */
function request(env: Record<string, unknown> = {}) {
  return { env, ctx: {}, cf: {} };
}

/** The identity of the underlying postgres client, seen through the lazy proxy. */
function clientIdentity() {
  return (client as unknown as { options: object }).options;
}

const MODES = ['hyperdrive', 'direct-binding-missing', 'direct-no-context', 'unresolved'];

test('one request gets one connection, however many times it asks', () => {
  resetConnectionModeForTests();
  current = request();
  assert.equal(clientIdentity(), clientIdentity());
});

test('a second request never inherits the first request connection', () => {
  resetConnectionModeForTests();
  current = request();
  const first = clientIdentity();
  current = request();
  assert.notEqual(clientIdentity(), first);
});

test('two requests alive at once keep their own connections', () => {
  resetConnectionModeForTests();
  const a = request();
  const b = request();
  current = a;
  const fromA = clientIdentity();
  current = b;
  const fromB = clientIdentity();
  // Back to A. A single cache slot would have been evicted by B; a per-request map has not.
  current = a;
  assert.equal(clientIdentity(), fromA);
  assert.notEqual(fromA, fromB);
});

test('the Hyperdrive binding is preferred and reported when it is bound', () => {
  resetConnectionModeForTests();
  current = request({ HYPERDRIVE: { connectionString: 'postgres://u:p@localhost:5432/hd' } });
  void clientIdentity();
  assert.equal(connectionMode(), 'hyperdrive');
});

test('a deployment without the binding says so rather than pretending', () => {
  resetConnectionModeForTests();
  current = request();
  void clientIdentity();
  assert.equal(connectionMode(), 'direct-binding-missing');
});

test('no transport is claimed before a query is attempted', () => {
  resetConnectionModeForTests();
  assert.equal(connectionMode(), 'unresolved');
});

test('outside a request the fallback is recorded, not silent', async () => {
  resetConnectionModeForTests();
  current = undefined;
  await db.execute('select 1').catch(() => {});
  assert.equal(connectionMode(), 'direct-no-context');
});

test('the transport is a fixed token, never an error message', () => {
  resetConnectionModeForTests();
  current = undefined;
  void clientIdentity();
  // The token travels in a public error response. An error string interpolated into it could
  // carry the host or the connection string; the closed set is what stops that.
  assert.ok(MODES.includes(connectionMode()), `unexpected token: ${connectionMode()}`);
});
