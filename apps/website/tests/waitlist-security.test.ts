import assert from 'node:assert/strict';
import { test } from 'node:test';
import { generateRefCode, generateStatusToken, signupPayload } from '../src/lib/referral.ts';
import { isAuthorizedOrigin } from '../src/lib/waitlist-auth.ts';

// --- signupPayload: duplicate signups must never leak the private token ---
// (docs/fixes/2026-07-30-waitlist-security-fix.md)

const ORIGIN = 'https://syogun.com';

test('duplicate signup payload never contains the existing statusToken', () => {
  const row = { refCode: generateRefCode(), statusToken: generateStatusToken() };
  const dup = signupPayload(row, true, ORIGIN);
  assert.deepEqual(dup, { refCode: null, statusUrl: null });
  assert.equal(JSON.stringify(dup).includes(row.statusToken), false, 'token must not appear');
  assert.equal(JSON.stringify(dup).includes(row.refCode), false, 'ref code must not appear');
});

test('duplicate payload is byte-identical to the honeypot response shape', () => {
  const row = { refCode: generateRefCode(), statusToken: generateStatusToken() };
  // The honeypot path returns { refCode: null, statusUrl: null } — duplicates
  // must be indistinguishable from it.
  assert.deepEqual(signupPayload(row, true, ORIGIN), { refCode: null, statusUrl: null });
});

test('fresh signup payload still returns the status link (happy path unchanged)', () => {
  const row = { refCode: generateRefCode(), statusToken: generateStatusToken() };
  const fresh = signupPayload(row, false, ORIGIN);
  assert.equal(fresh.refCode, row.refCode);
  assert.ok(fresh.statusUrl?.startsWith(`${ORIGIN}/status?code=`));
  assert.ok(fresh.statusUrl!.includes(row.statusToken), 'owner gets their own token once');
});

test('rows missing tokens degrade to the generic payload, never a broken URL', () => {
  assert.deepEqual(signupPayload({ refCode: null, statusToken: null }, false, ORIGIN), {
    refCode: null,
    statusUrl: null,
  });
});

// --- isAuthorizedOrigin: must fail CLOSED when unconfigured ---

function post(url: string, headers: Record<string, string>): Request {
  return new Request(url, { method: 'POST', headers });
}

/** Run `fn` with WAITLIST_* env pinned, restoring the previous values after. */
function withEnv(env: Record<string, string | undefined>, fn: () => void) {
  const keys = ['WAITLIST_ALLOWED_ORIGINS', 'WAITLIST_WEBHOOK_SECRET'] as const;
  const prev = Object.fromEntries(keys.map((k) => [k, process.env[k]]));
  try {
    for (const k of keys) {
      if (env[k] === undefined) delete process.env[k];
      else process.env[k] = env[k];
    }
    fn();
  } finally {
    for (const k of keys) {
      if (prev[k] === undefined) delete process.env[k];
      else process.env[k] = prev[k];
    }
  }
}

const SITE = 'https://syogun.com/api/waitlist/signup';

test('unset allowlist DENIES cross-origin requests (fail closed)', () => {
  withEnv({ WAITLIST_ALLOWED_ORIGINS: undefined, WAITLIST_WEBHOOK_SECRET: undefined }, () => {
    assert.equal(isAuthorizedOrigin(post(SITE, { origin: 'https://evil.example' })), false);
  });
});

test('unset allowlist DENIES requests with no Origin header (fail closed)', () => {
  withEnv({ WAITLIST_ALLOWED_ORIGINS: undefined, WAITLIST_WEBHOOK_SECRET: undefined }, () => {
    assert.equal(isAuthorizedOrigin(post(SITE, {})), false);
  });
});

test('empty-string allowlist behaves like unset (fail closed)', () => {
  withEnv({ WAITLIST_ALLOWED_ORIGINS: '', WAITLIST_WEBHOOK_SECRET: undefined }, () => {
    assert.equal(isAuthorizedOrigin(post(SITE, { origin: 'https://evil.example' })), false);
  });
});

test('unset allowlist still permits SAME-ORIGIN requests (local dev)', () => {
  withEnv({ WAITLIST_ALLOWED_ORIGINS: undefined, WAITLIST_WEBHOOK_SECRET: undefined }, () => {
    const dev = post('http://localhost:3000/api/waitlist/signup', { origin: 'http://localhost:3000' });
    assert.equal(isAuthorizedOrigin(dev), true);
    assert.equal(isAuthorizedOrigin(post(SITE, { origin: 'https://syogun.com' })), true);
  });
});

test('configured allowlist admits listed origins and rejects the rest', () => {
  withEnv({ WAITLIST_ALLOWED_ORIGINS: 'https://syogun.com', WAITLIST_WEBHOOK_SECRET: undefined }, () => {
    assert.equal(isAuthorizedOrigin(post(SITE, { origin: 'https://syogun.com' })), true);
    assert.equal(isAuthorizedOrigin(post(SITE, { origin: 'https://evil.example' })), false);
    // Explicit allowlist wins: same-origin fallback is not consulted.
    const other = post('http://localhost:3000/api/waitlist/signup', { origin: 'http://localhost:3000' });
    assert.equal(isAuthorizedOrigin(other), false);
  });
});

test('garbage Origin header is denied, not thrown on', () => {
  withEnv({ WAITLIST_ALLOWED_ORIGINS: undefined, WAITLIST_WEBHOOK_SECRET: undefined }, () => {
    assert.equal(isAuthorizedOrigin(post(SITE, { origin: 'null' })), false);
  });
});

test('webhook secret authorizes server callers; wrong/empty secret does not', () => {
  withEnv({ WAITLIST_ALLOWED_ORIGINS: undefined, WAITLIST_WEBHOOK_SECRET: 's3cret' }, () => {
    assert.equal(isAuthorizedOrigin(post(SITE, { 'x-webhook-secret': 's3cret' })), true);
    assert.equal(isAuthorizedOrigin(post(SITE, { 'x-webhook-secret': 'wrong' })), false);
  });
  // An EMPTY configured secret must never authorize an empty header.
  withEnv({ WAITLIST_ALLOWED_ORIGINS: undefined, WAITLIST_WEBHOOK_SECRET: '' }, () => {
    assert.equal(isAuthorizedOrigin(post(SITE, { 'x-webhook-secret': '' })), false);
  });
});
