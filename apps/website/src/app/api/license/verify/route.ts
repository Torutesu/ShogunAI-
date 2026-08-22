import { NextResponse } from 'next/server';

import {
  findLicenseByKey,
  findSubscription,
  recordVerification,
} from '@/db/billing-queries';
import { isEntitled, isSubscriptionStatus, type SubscriptionRecord } from '@/lib/billing';
import { HttpError, fail, readJsonObject } from '@/lib/http';
import {
  buildTokenPayload,
  isValidDeviceId,
  isValidLicenseKey,
  signLicenseToken,
  signingKeyConfigured,
  OFFLINE_GRACE_DAYS,
  TOKEN_TTL_SECONDS,
} from '@/lib/license';
import { isPlanId } from '@/lib/pricing';
import { rateLimit } from '@/lib/rate-limit';
import { withTimeout } from '@/lib/timeout';
import { clientIp } from '@/lib/waitlist-auth';

export const runtime = 'nodejs';

/**
 * Deadline for the database reads this route waits on. A stalled connection is not a rejected
 * promise, so without this the Workers runtime kills the request and the route's own error
 * handling never runs — the caller gets a dead request instead of the 500 it is written to
 * return. A 500 matters here: the desktop app treats it as "could not check" and stays inside its
 * offline grace window, where a dead request just looks like the network is down.
 */
const DB_TIMEOUT_MS = 3_000;

export const dynamic = 'force-dynamic';

/**
 * POST /api/license/verify — FR-BIL-08. The desktop app calls this at launch and every 24h.
 *
 * Request:  { license_key, device_id, app_version? }
 * Response: { ok, entitled, plan, status, current_period_end, cancel_at_period_end, token, … }
 *
 * The request carries **nothing but** the licence id, an anonymous device id and the app version
 * — no capture content, no memory content, no email (FR-BIL-08 / NFR-PRV-04). The response's
 * `token` is Ed25519-signed and device-bound; the Rust core verifies it offline, which is what
 * makes the 14-day offline grace (FR-BIL-09) possible without trusting the local clock's honesty
 * about anything except elapsed time.
 *
 * A licence that exists but is not entitled still gets a 200 with `entitled: false` — the app
 * needs to tell "your subscription lapsed" apart from "this key is not real".
 */
export async function POST(req: Request) {
  if (!signingKeyConfigured()) return fail('server_error', { reason: 'signing_key_not_configured' });

  const rl = await rateLimit('license-verify', clientIp(req), { limit: 30, windowSec: 60 });
  if (!rl.allowed) return fail('rate_limited');

  let body: Record<string, unknown>;
  try {
    body = await readJsonObject(req);
  } catch (e) {
    return fail(e instanceof HttpError ? e.code : 'bad_request');
  }

  const key = body.license_key;
  const deviceId = body.device_id;
  if (!isValidLicenseKey(key) || !isValidDeviceId(deviceId)) return fail('bad_request');
  const appVersion = typeof body.app_version === 'string' ? body.app_version.slice(0, 32) : null;

  try {
    const license = await withTimeout(findLicenseByKey(key), DB_TIMEOUT_MS, 'licence lookup');
    if (!license || license.revokedAt) return fail('not_found');

    const sub = await withTimeout(
      findSubscription(license.stripeSubscriptionId),
      DB_TIMEOUT_MS,
      'subscription lookup',
    );
    if (!sub) return fail('not_found');

    const rec: SubscriptionRecord = {
      stripeSubscriptionId: sub.stripeSubscriptionId,
      stripeCustomerId: sub.stripeCustomerId,
      stripePriceId: sub.stripePriceId,
      plan: isPlanId(sub.plan) ? sub.plan : null,
      interval: sub.interval === 'monthly' || sub.interval === 'annual' ? sub.interval : null,
      status: isSubscriptionStatus(sub.status) ? sub.status : 'incomplete',
      currentPeriodStart: sub.currentPeriodStart ? Math.floor(sub.currentPeriodStart.getTime() / 1000) : null,
      currentPeriodEnd: sub.currentPeriodEnd ? Math.floor(sub.currentPeriodEnd.getTime() / 1000) : null,
      cancelAt: sub.cancelAt ? Math.floor(sub.cancelAt.getTime() / 1000) : null,
      canceledAt: sub.canceledAt ? Math.floor(sub.canceledAt.getTime() / 1000) : null,
      cancelAtPeriodEnd: sub.cancelAtPeriodEnd,
      trialEnd: sub.trialEnd ? Math.floor(sub.trialEnd.getTime() / 1000) : null,
    };

    const nowMs = Date.now();
    const entitled = isEntitled(rec, nowMs);

    await withTimeout(
      recordVerification(license.id, deviceId, appVersion),
      DB_TIMEOUT_MS,
      'verification record',
    ).catch((e) => {
      // Bookkeeping must never cost a paying customer their access.
      console.error('license verification bookkeeping failed:', e);
    });

    // A token is only issued while entitled. An unentitled device gets the status and no token,
    // so its cached token simply ages out through the grace window and then locks.
    const token =
      entitled && rec.plan
        ? signLicenseToken(
            buildTokenPayload({
              licenseId: license.id,
              plan: rec.plan,
              status: rec.status,
              deviceId,
              periodEnd: rec.currentPeriodEnd,
              cancelAtPeriodEnd: rec.cancelAtPeriodEnd,
              nowMs,
            }),
          )
        : null;

    return NextResponse.json(
      {
        ok: true,
        entitled,
        plan: rec.plan,
        status: rec.status,
        current_period_end: rec.currentPeriodEnd,
        cancel_at_period_end: rec.cancelAtPeriodEnd,
        token,
        token_ttl_seconds: TOKEN_TTL_SECONDS,
        grace_days: OFFLINE_GRACE_DAYS,
      },
      { headers: { 'Cache-Control': 'no-store', 'X-Robots-Tag': 'noindex' } },
    );
  } catch (e) {
    console.error('license verify error:', e);
    return fail('server_error');
  }
}
