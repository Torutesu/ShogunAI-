import { NextResponse } from 'next/server';

import { findLicenseByKey } from '@/db/billing-queries';
import { HttpError, fail, readJsonObject } from '@/lib/http';
import { isValidLicenseKey } from '@/lib/license';
import { rateLimit } from '@/lib/rate-limit';
import { appOrigin, stripeSecretKey, stripe } from '@/lib/stripe';
import { clientIp } from '@/lib/waitlist-auth';

export const runtime = 'nodejs';

/**
 * POST /api/stripe/portal — issue #8, the "90% of billing ops go to the Customer Portal" goal.
 * Body: { license_key } → { ok: true, url }.
 *
 * The licence key is the only credential: cancellations, plan changes and card updates all
 * happen inside Stripe's portal, so ShogunAI never renders a card field (FR-BIL-07).
 * Rate-limited per IP because this endpoint is also a licence-key oracle otherwise.
 */
export async function POST(req: Request) {
  if (!stripeSecretKey()) return fail('server_error', { reason: 'billing_not_configured' });

  const rl = await rateLimit('portal', clientIp(req), { limit: 10, windowSec: 60 });
  if (!rl.allowed) return fail('rate_limited');

  let body: Record<string, unknown>;
  try {
    body = await readJsonObject(req);
  } catch (e) {
    return fail(e instanceof HttpError ? e.code : 'bad_request');
  }

  const key = body.license_key;
  if (!isValidLicenseKey(key)) return fail('bad_request');

  try {
    const license = await findLicenseByKey(key);
    // Same answer for "no such licence" and "revoked": a portal link is account access.
    if (!license || license.revokedAt) return fail('not_found');

    const session = await stripe().billingPortal.sessions.create({
      customer: license.stripeCustomerId,
      return_url: `${appOrigin()}/billing/success`,
    });
    return NextResponse.json(
      { ok: true, url: session.url },
      { headers: { 'Cache-Control': 'no-store', 'X-Robots-Tag': 'noindex' } },
    );
  } catch (e) {
    console.error('portal error:', e);
    return fail('server_error');
  }
}
