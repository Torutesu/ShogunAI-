import { NextResponse } from 'next/server';

import { findCustomerByEmail, linkCustomer } from '@/db/billing-queries';
import { HttpError, fail, readJsonObject } from '@/lib/http';
import { isInterval, isPlanId, priceIdFor } from '@/lib/pricing';
import { rateLimit } from '@/lib/rate-limit';
import { appOrigin, automaticTaxEnabled, billingReady, checkoutTrialDays, stripe } from '@/lib/stripe';
import { clientIp } from '@/lib/waitlist-auth';
import { isValidEmail } from '@/lib/referral';

export const runtime = 'nodejs';

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
  // Where the click came from ("lp" | "app"), for funnel analysis only. Never trusted for pricing.
  const source = typeof body.source === 'string' ? body.source.slice(0, 32) : 'lp';

  try {
    // Reuse the Stripe customer when we already know this email, so a returning buyer does not
    // end up with two customers and two portals.
    const known = email ? await findCustomerByEmail(email) : null;
    const trialDays = checkoutTrialDays();
    const origin = appOrigin();

    // Tax (STRIPE_AUTOMATIC_TAX). Stripe cannot compute a rate without a country, hence the
    // required address; `tax_id_collection` lets an EU/UK business supply a VAT number, which
    // moves that sale to reverse charge instead of us collecting. `customer_update` is only a
    // legal parameter when the session names an existing customer — passing it alongside
    // `customer_creation` is an API error, so it rides with `known`.
    const tax = automaticTaxEnabled()
      ? {
          automatic_tax: { enabled: true },
          billing_address_collection: 'required' as const,
          tax_id_collection: { enabled: true },
          ...(known ? { customer_update: { address: 'auto' as const, name: 'auto' as const } } : {}),
        }
      : {};

    const session = await stripe().checkout.sessions.create({
      mode: 'subscription',
      line_items: [{ price, quantity: 1 }],
      ...(known
        ? { customer: known.stripeCustomerId }
        : { ...(email ? { customer_email: email } : {}), customer_creation: 'always' as const }),
      ...tax,
      allow_promotion_codes: true,
      client_reference_id: email ?? undefined,
      // The webhook reads these back to attach the subscription to the right plan and buyer.
      metadata: { plan, interval, source },
      subscription_data: {
        metadata: { plan, interval, source },
        ...(trialDays > 0 ? { trial_period_days: trialDays } : {}),
      },
      success_url: `${origin}/billing/success?session_id={CHECKOUT_SESSION_ID}`,
      cancel_url: `${origin}/#pricing`,
    });

    if (email && known === null && typeof session.customer === 'string') {
      // Best-effort early link; the webhook writes the authoritative one.
      await linkCustomer(email, session.customer).catch(() => undefined);
    }

    if (!session.url) return fail('server_error', { reason: 'no_checkout_url' });
    return NextResponse.json({ ok: true, url: session.url }, { headers: { 'Cache-Control': 'no-store' } });
  } catch (e) {
    console.error('checkout error:', e);
    return fail('server_error');
  }
}
