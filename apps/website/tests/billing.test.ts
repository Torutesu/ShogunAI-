import assert from 'node:assert/strict';
import { generateKeyPairSync, verify as cryptoVerify } from 'node:crypto';
import { test } from 'node:test';

import {
  PAST_DUE_GRACE_DAYS,
  isEntitled,
  isSubscriptionStatus,
  toSubscriptionRecord,
  type StripeSubscriptionLike,
} from '../src/lib/billing.ts';
import {
  buildTokenPayload,
  canonicalLicenseKey,
  generateLicenseKey,
  isValidDeviceId,
  isValidLicenseKey,
  licenseKeyFingerprint,
  signLicenseToken,
} from '../src/lib/license.ts';
import { PLANS, formatUsd, planForPriceId, priceIdFor, priceLine } from '../src/lib/pricing.ts';

/**
 * Billing unit tests (issue #8). Everything here is the pure layer — no Stripe account, no DB,
 * no network. The state machine that decides "may this person use the app" has to be provable
 * without either.
 */

const DAY = 86_400 * 1000;
const setPrices = () => {
  process.env.STRIPE_PRICE_STANDARD_ANNUAL = 'price_std_year';
  process.env.STRIPE_PRICE_STANDARD_MONTHLY = 'price_std_month';
  process.env.STRIPE_PRICE_PRO_ANNUAL = 'price_pro_year';
  process.env.STRIPE_PRICE_PRO_MONTHLY = 'price_pro_month';
};

// ── pricing catalog ───────────────────────────────────────────────────────────

test('the catalog carries the live 2026-08 amounts', () => {
  // These are the numbers on the LP (src/i18n/dictionaries.ts) — the issue's "$50–60/mo" is
  // pre-decision copy. If this test fails, the two have drifted and one of them is lying to a
  // customer.
  assert.equal(PLANS.standard.prices.annual.perMonthCents, 4_900);
  assert.equal(PLANS.standard.prices.annual.amountCents, 58_800);
  assert.equal(PLANS.standard.prices.monthly.amountCents, 6_200);
  assert.equal(PLANS.pro.prices.annual.perMonthCents, 9_900);
  assert.equal(PLANS.pro.prices.annual.amountCents, 118_800);
  assert.equal(PLANS.pro.prices.monthly.amountCents, 12_400);
  // Annual is exactly 12× the advertised monthly-equivalent — no hidden rounding.
  for (const plan of ['standard', 'pro'] as const) {
    const p = PLANS[plan].prices.annual;
    assert.equal(p.amountCents, p.perMonthCents * 12, `${plan} annual = 12× per-month`);
  }
});

test('price copy reads the way the LP does', () => {
  assert.equal(formatUsd(4_900), '$49');
  assert.equal(formatUsd(58_800), '$588');
  setPrices();
  assert.equal(priceLine('standard', 'annual'), '$49/mo, billed annually ($588/yr)');
  assert.equal(priceLine('pro', 'monthly'), '$124/mo');
});

test('price ids resolve both ways and an unknown price maps to no plan', () => {
  setPrices();
  assert.equal(priceIdFor('pro', 'annual'), 'price_pro_year');
  assert.deepEqual(planForPriceId('price_std_month'), { plan: 'standard', interval: 'monthly' });
  // An unrecognised price must never be guessed into a plan — that would hand out entitlements.
  assert.equal(planForPriceId('price_someone_made_in_the_dashboard'), null);
});

test('an unconfigured price is null, not a fallback', () => {
  delete process.env.STRIPE_PRICE_PRO_MONTHLY;
  assert.equal(priceIdFor('pro', 'monthly'), null);
  setPrices();
});

// ── subscription mapping ──────────────────────────────────────────────────────

const sub = (over: Partial<StripeSubscriptionLike> = {}): StripeSubscriptionLike => ({
  id: 'sub_1',
  status: 'active',
  customer: 'cus_1',
  items: { data: [{ price: { id: 'price_pro_year' }, current_period_end: 2_000_000 }] },
  ...over,
});

test('maps a subscription onto our row, including the period moved onto the item', () => {
  setPrices();
  const rec = toSubscriptionRecord(sub());
  assert.equal(rec.plan, 'pro');
  assert.equal(rec.interval, 'annual');
  assert.equal(rec.status, 'active');
  // Newer Stripe API versions moved current_period_* onto the item; both shapes must land.
  assert.equal(rec.currentPeriodEnd, 2_000_000);
  const legacy = toSubscriptionRecord(
    sub({ current_period_end: 111, items: { data: [{ price: { id: 'price_pro_year' } }] } }),
  );
  assert.equal(legacy.currentPeriodEnd, 111);
});

test('an unknown status is stored as incomplete, which never entitles', () => {
  setPrices();
  const rec = toSubscriptionRecord(sub({ status: 'something_new' }));
  assert.equal(rec.status, 'incomplete');
  assert.equal(isEntitled(rec, Date.now()), false);
  assert.equal(isSubscriptionStatus('something_new'), false);
});

test('entitlement follows status, with a grace window for past_due only', () => {
  setPrices();
  const periodEnd = 1_000_000; // unix seconds
  const atEnd = periodEnd * 1000;
  const mk = (status: string) =>
    toSubscriptionRecord(
      sub({
        status,
        items: { data: [{ price: { id: 'price_pro_year' }, current_period_end: periodEnd }] },
      }),
    );

  for (const status of ['active', 'trialing']) {
    assert.equal(isEntitled(mk(status), atEnd), true, `${status} inside period`);
  }
  // past_due keeps working through Stripe's dunning retries and 7 days past the period
  // (FR-BIL-09) — cutting a paying customer off on one failed charge is the expensive mistake.
  const pastDue = mk('past_due');
  assert.equal(isEntitled(pastDue, atEnd + (PAST_DUE_GRACE_DAYS - 1) * DAY), true);
  assert.equal(isEntitled(pastDue, atEnd + (PAST_DUE_GRACE_DAYS + 1) * DAY), false);

  for (const status of ['canceled', 'unpaid', 'incomplete', 'incomplete_expired', 'paused']) {
    assert.equal(isEntitled(mk(status), atEnd), false, `${status} must not entitle`);
  }
});

test('a subscription on an unrecognised price never entitles', () => {
  setPrices();
  const rec = toSubscriptionRecord(sub({ items: { data: [{ price: { id: 'price_mystery' } }] } }));
  assert.equal(rec.plan, null);
  assert.equal(isEntitled(rec, Date.now()), false);
});

// ── licence keys ──────────────────────────────────────────────────────────────

test('licence keys are well shaped, unique, and tolerant of how humans paste them', () => {
  const key = generateLicenseKey();
  assert.match(key, /^shogun-[0-9A-HJKMNP-TV-Z]{4}(-[0-9A-HJKMNP-TV-Z]{4}){3}$/);
  assert.ok(isValidLicenseKey(key));
  assert.notEqual(generateLicenseKey(), generateLicenseKey());

  // lower case, stray whitespace and a mail client's en-dash all canonicalise back to the key.
  assert.equal(canonicalLicenseKey(`  ${key.toLowerCase()} `), key);
  assert.equal(canonicalLicenseKey(key.replace(/-/g, '‐')), key);
  assert.ok(isValidLicenseKey(key.toLowerCase()));

  assert.equal(isValidLicenseKey('shogun-XXXX'), false);
  assert.equal(isValidLicenseKey(''), false);
  assert.equal(isValidLicenseKey(42), false);
});

test('the key alphabet has no lookalike characters', () => {
  // A key is retyped off a screen; I/L/O/U would generate support tickets, not activations.
  for (let i = 0; i < 50; i += 1) {
    assert.doesNotMatch(generateLicenseKey().slice('shogun-'.length), /[ILOU]/);
  }
});

test('the fingerprint is short, stable and not the key', () => {
  const key = generateLicenseKey();
  const fp = licenseKeyFingerprint(key);
  assert.equal(fp.length, 12);
  assert.equal(fp, licenseKeyFingerprint(key.toLowerCase()));
  assert.ok(!key.includes(fp));
});

test('device ids are opaque and bounded', () => {
  assert.ok(isValidDeviceId('7f3c1d20-9b2a-4e11-8a44-1f2c3d4e5f60'));
  assert.equal(isValidDeviceId('short'), false);
  assert.equal(isValidDeviceId('x'.repeat(65)), false);
  assert.equal(isValidDeviceId('has space'), false);
  assert.equal(isValidDeviceId(null), false);
});

// ── licence tokens ────────────────────────────────────────────────────────────

test('a signed token verifies against the public key and covers the exact bytes sent', () => {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  process.env.LICENSE_SIGNING_KEY = Buffer.from(
    privateKey.export({ type: 'pkcs8', format: 'pem' }) as string,
  ).toString('base64');

  const payload = buildTokenPayload({
    licenseId: 'lic-1',
    plan: 'pro',
    status: 'active',
    deviceId: 'device-abc12345',
    periodEnd: 1_800_000_000,
    cancelAtPeriodEnd: false,
    nowMs: 1_754_800_000_000,
  });
  const token = signLicenseToken(payload);

  const [version, body, sig] = token.split('.');
  assert.equal(version, 'v1');
  assert.ok(
    cryptoVerify(null, Buffer.from(body, 'base64url'), publicKey, Buffer.from(sig, 'base64url')),
    'signature verifies over the transmitted payload bytes',
  );
  // The payload the Rust verifier will read is the one we signed, byte for byte.
  assert.deepEqual(JSON.parse(Buffer.from(body, 'base64url').toString('utf8')), payload);

  // Flipping one byte of the payload breaks verification — no canonicalisation escape hatch.
  const tampered = Buffer.from(JSON.stringify({ ...payload, plan: 'standard' }), 'utf8');
  assert.equal(
    cryptoVerify(null, tampered, publicKey, Buffer.from(sig, 'base64url')),
    false,
    'a re-serialised payload must not verify under the original signature',
  );
});

test('the token carries the plan gate and nothing about the person', () => {
  const payload = buildTokenPayload({
    licenseId: 'lic-1',
    plan: 'standard',
    status: 'trialing',
    deviceId: 'device-abc12345',
    periodEnd: null,
    cancelAtPeriodEnd: true,
    nowMs: 0,
  });
  // FR-BIL-08 / NFR-PRV-04: no email, no name, no capture or memory content — ever.
  assert.deepEqual(Object.keys(payload).sort(), [
    'cancel_at_period_end',
    'device',
    'exp',
    'grace_days',
    'iat',
    'lic',
    'period_end',
    'plan',
    'status',
    'v',
  ]);
  assert.ok(payload.exp > payload.iat, 'a token must expire');
  assert.equal(payload.grace_days, 14);
});
