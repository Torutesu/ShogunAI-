import assert from 'node:assert/strict';
import { test } from 'node:test';
import { isAuthorizedOrigin } from '../src/lib/waitlist-auth.ts';

// The four signupPayload cases that used to live here are gone with the helper:
// /api/waitlist/signup now answers `ok({})` for new and duplicate emails alike,
// so there is no payload left to leak a status token or act as an enumeration
// oracle. See docs/fixes/2026-07-30-waitlist-security-fix.md for the original.

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

test('unset allowlist denies even SAME-ORIGIN requests outside development', () => {
  withEnv({ WAITLIST_ALLOWED_ORIGINS: undefined, WAITLIST_WEBHOOK_SECRET: undefined }, () => {
    const prev = process.env.NODE_ENV;
    try {
      delete process.env.NODE_ENV;
      assert.equal(isAuthorizedOrigin(post(SITE, { origin: 'https://syogun.com' })), false);
      process.env.NODE_ENV = 'development';
      const dev = post('http://localhost:3000/api/waitlist/signup', { origin: 'http://localhost:3000' });
      assert.equal(isAuthorizedOrigin(dev), true);
    } finally {
      if (prev === undefined) delete process.env.NODE_ENV;
      else process.env.NODE_ENV = prev;
    }
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
