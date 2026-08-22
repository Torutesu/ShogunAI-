import { NextResponse } from 'next/server';

import { findCustomerByEmail, linkCustomer } from '@/db/billing-queries';
import { HttpError, fail, readJsonObject } from '@/lib/http';
import { isInterval, isPlanId, priceIdFor } from '@/lib/pricing';
import { rateLimit } from '@/lib/rate-limit';
import {
  appOrigin,
  automaticTaxEnabled,
  billingReady,
  buildCheckoutParams,
  checkoutTrialDays,
  stripe,
} from '@/lib/stripe';
import { withTimeout } from '@/lib/timeout';
import { clientIp } from '@/lib/waitlist-auth';
import { isValidClaimNonce } from '@/lib/license';
import { isValidEmail } from '@/lib/referral';

export const runtime = 'nodejs';

/** Deadline for the returning-buyer lookup. See the note at the call site. */
const CUSTOMER_LOOKUP_TIMEOUT_MS = 2_000;

/**
 * POST /api/stripe/checkout  — issue #8, step 2 of the flow.
 * Body: { plan: "standard"|"pro", interval: "monthly"|"annual", email?, source? }
 * → { ok: true, url } ; the caller redirects to `url`.
 *
 * The client sends a plan *name*, never a Price ID: the price is resolved server-side from the
 * environment so a tampered request cannot buy Pro at the Standard price (issue #8 セキュリティ).
 * An unconfigured environment answers 503 rather than starting a checkout that cannot complete.
 */
export async function POST(req: Request) {
  if (!billingReady()) return fail('server_error', { reason: 'billing_not_configured' });

  const rl = await rateLimit('checkout', clientIp(req), { limit: 10, windowSec: 60 });
  if (!rl.allowed) return fail('rate_limited');

  let body: Record<string, unknown>;
  try {
    body = await readJsonObject(req);
  } catch (e) {
    return fail(e instanceof HttpError ? e.code : 'bad_request');
  }

  const plan = body.plan;
  const interval = body.interval ?? 'annual';
  if (!isPlanId(plan) || !isInterval(interval)) return fail('bad_request');

  const price = priceIdFor(plan, interval);
  if (!price) return fail('server_error', { reason: 'price_not_configured' });

  const email = isValidEmail(body.email) ? String(body.email).trim().toLowerCase() : null;
  // One-shot capability minted by the buying Mac so it can pull its own licence key down after
  // payment. Opaque to us and never logged; a request without one (the LP) just gets the old
  // show-the-key-on-the-success-page path.
  const claimNonce = isValidClaimNonce(body.claim_nonce) ? body.claim_nonce : null;
  // Where the click came from ("lp" | "app"), for funnel analysis only. Never trusted for pricing.
  const source = typeof body.source === 'string' ? body.source.slice(0, 32) : 'lp';

  try {
    // Reuse the Stripe customer when we already know this email, so a returning buyer does not
    // end up with two customers and two portals.
    //
    // Bounded, and treated as "not known" when the deadline passes. A stalled lookup would
    // otherwise hang the request until the runtime kills it, which costs the sale outright; the
    // worst a timeout costs is a second Stripe customer for a returning buyer, and the webhook
    // writes the authoritative link either way. Losing the sale is the larger loss.
    const known = email
      ? await withTimeout(findCustomerByEmail(email), CUSTOMER_LOOKUP_TIMEOUT_MS, 'customer lookup')
          .catch((err) => {
            console.error('customer lookup failed; continuing as a new customer:', err);
            return null;
          })
      : null;

    const session = await stripe().checkout.sessions.create(
      buildCheckoutParams({
        price,
        plan,
        interval,
        source,
        email,
        claimNonce,
        customerId: known?.stripeCustomerId ?? null,
        trialDays: checkoutTrialDays(),
        automaticTax: automaticTaxEnabled(),
        origin: appOrigin(),
      }),
    );

    if (email && known === null && typeof session.customer === 'string') {
      // Best-effort early link; the webhook writes the authoritative one.
      await linkCustomer(email, session.customer).catch(() => undefined);
    }

    if (!session.url) return fail('server_error', { reason: 'no_checkout_url' });
    return NextResponse.json({ ok: true, url: session.url }, { headers: { 'Cache-Control': 'no-store' } });
  } catch (e) {
    // Log the identifying fields on one line rather than the whole error: a Stripe rejection is
    // an ops problem ("no such price", "unknown parameter"), and a log viewer that truncates the
    // stack must still show which one it was. The buyer gets a bare `server_error` — a Stripe
    // message can name internal parameters and price IDs.
    const err = e as { type?: unknown; code?: unknown; param?: unknown; message?: unknown };
    console.error(
      `checkout error: type=${String(err.type)} code=${String(err.code)} ` +
        `param=${String(err.param)} message=${String(err.message)}`,
    );
    // Stripe answers `resource_missing` on the price when the configured Price IDs and the secret
    // key belong to different modes — a live key cannot see a test price, and vice versa. Nothing
    // in a Price ID encodes its mode, so no startup check can catch this; naming it in the
    // response is what keeps the next occurrence from being a log-archaeology exercise. Safe to
    // expose: it describes our own misconfiguration, not the buyer or the key.
    if (err.code === 'resource_missing' && String(err.param).includes('price')) {
      return fail('server_error', { reason: 'price_mode_mismatch' });
    }
    return fail('server_error');
  }
}
