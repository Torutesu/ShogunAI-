import assert from 'node:assert/strict';
import { test } from 'node:test';

import { connectionMode, db, resetConnectionModeForTests } from '../src/db/index.ts';

/**
 * The transport the database layer reports about itself.
 *
 * This exists because production spent a launch morning failing roughly half its licence reads
 * while the Hyperdrive dashboard showed zero errors — two facts that are only compatible if some
 * isolates never take Hyperdrive at all. A log line answers that, but only for whoever can export
 * the logs; the token these tests pin is carried on the 500 itself, so the failing request says
 * which way it went out.
 */

const MODES = ['hyperdrive', 'direct-binding-missing', 'direct-no-context', 'unresolved'];

test('no transport is claimed before a query is attempted', () => {
  resetConnectionModeForTests();
  assert.equal(connectionMode(), 'unresolved');
});

test('falling back to a direct connection is recorded, not silent', async () => {
  resetConnectionModeForTests();
  // Outside a Worker request there is no Cloudflare context, so this is the fallback path — the
  // same one production takes when the binding is unreadable. The query itself has no database to
  // reach here; what is under test is that the attempt names its transport either way.
  await db.execute('select 1').catch(() => {});
  assert.equal(connectionMode(), 'direct-no-context');
});

test('the transport is a fixed token, never an error message', async () => {
  resetConnectionModeForTests();
  await db.execute('select 1').catch(() => {});
  // The token travels in a public error response. An error string interpolated into it could
  // carry the host or the connection string; the closed set is what stops that.
  assert.ok(MODES.includes(connectionMode()), `unexpected token: ${connectionMode()}`);
});
