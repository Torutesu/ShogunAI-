import { NextResponse } from 'next/server';
import type Stripe from 'stripe';

import {
  claimStripeEvent,
  ensureLicense,
  linkCustomer,
  releaseStripeEvent,
  upsertSubscription,
} from '@/db/billing-queries';
import { toSubscriptionRecord, type StripeSubscriptionLike } from '@/lib/billing';
import { generateLicenseKey, licenseKeyFingerprint } from '@/lib/license';
import { stripe, stripeSecretKey, webhookSecret } from '@/lib/stripe';

export const runtime = 'nodejs';
/** The signature covers the raw bytes — no body parsing, no caching, no edge runtime. */
export const dynamic = 'force-dynamic';

/**
 * POST /api/stripe/webhook — issue #8, step 5: Stripe is the source of truth, this keeps our
 * mirror current so "誰がいつまで使えるのか" is answerable from one SELECT.
 *
 * Rules that make this safe:
 *  - **Signature verification is mandatory** (issue #8 セキュリティ). An unsigned or badly signed
 *    request is a 400 before anything is read; a missing `STRIPE_WEBHOOK_SECRET` is a 500, never
 *    a bypass.
 *  - **Idempotent**: every event id is claimed once (`stripe_events`). Stripe retries, and a
 *    replayed `checkout.session.completed` must not mint a second licence key.
 *  - **Order-independent**: each handler upserts the full subscription state it just read from
 *    Stripe, so a late-arriving older event cannot resurrect a cancelled subscription — it
 *    re-reads current state rather than applying a delta.
 *  - **Never 5xx on a handled-but-uninteresting event**: a non-2xx makes Stripe retry forever.
 */
const HANDLED = new Set([
  'checkout.session.completed',
  'customer.subscription.created',
  'customer.subscription.updated',
  'customer.subscription.deleted',
  'invoice.payment_succeeded',
  'invoice.payment_failed',
]);

/** Mirror one subscription (by id, freshly read) into our tables. */
async function syncSubscription(subId: string): Promise<void> {
  const sub = await stripe().subscriptions.retrieve(subId);
  await upsertSubscription(toSubscriptionRecord(sub as unknown as StripeSubscriptionLike));
}

async function onCheckoutCompleted(session: Stripe.Checkout.Session): Promise<void> {
  const subId = typeof session.subscription === 'string' ? session.subscription : session.subscription?.id;
  const customerId = typeof session.customer === 'string' ? session.customer : session.customer?.id;
  if (!subId || !customerId) return; // one-off payment — nothing to license

  const email =
    session.customer_details?.email?.trim().toLowerCase() ??
    session.customer_email?.trim().toLowerCase() ??
    null;

  await syncSubscription(subId);
  if (email) await linkCustomer(email, customerId);

  const license = await ensureLicense({
    licenseKey: generateLicenseKey(),
    stripeCustomerId: customerId,
    stripeSubscriptionId: subId,
    email,
  });

  // Stash the licence id on the Stripe objects so support can go from a Stripe dashboard row to
  // a licence without a DB query. The key itself is NEVER written to Stripe metadata.
  await stripe()
    .subscriptions.update(subId, { metadata: { shogun_license_id: license.id } })
    .catch(() => undefined);

  // Logs carry a fingerprint, never the key (CLAUDE.md: secrets never reach logs).
  console.info('license issued', {
    subscription: subId,
    license: license.id,
    key_fp: licenseKeyFingerprint(license.licenseKey),
  });
}

export async function POST(req: Request) {
  const secret = webhookSecret();
  if (!secret || !stripeSecretKey()) {
    console.error('webhook: billing not configured');
    return NextResponse.json({ ok: false, error: 'server_error' }, { status: 500 });
  }

  const signature = req.headers.get('stripe-signature');
  if (!signature) return NextResponse.json({ ok: false, error: 'bad_request' }, { status: 400 });

  const raw = await req.text();
  let event: Stripe.Event;
  try {
    event = await stripe().webhooks.constructEventAsync(raw, signature, secret);
  } catch (e) {
    console.error('webhook: signature verification failed:', e instanceof Error ? e.message : e);
    return NextResponse.json({ ok: false, error: 'bad_request' }, { status: 400 });
  }

  if (!HANDLED.has(event.type)) return NextResponse.json({ ok: true, ignored: event.type });

  try {
    if (!(await claimStripeEvent(event.id, event.type))) {
      return NextResponse.json({ ok: true, duplicate: true });
    }

    switch (event.type) {
      case 'checkout.session.completed':
        await onCheckoutCompleted(event.data.object);
        break;

      case 'customer.subscription.created':
      case 'customer.subscription.updated':
      case 'customer.subscription.deleted':
        // The event payload is already the full subscription; use it directly (one less API call)
        // — `deleted` in particular carries the terminal status we must persist.
        await upsertSubscription(
          toSubscriptionRecord(event.data.object as unknown as StripeSubscriptionLike),
        );
        break;

      case 'invoice.payment_succeeded':
      case 'invoice.payment_failed': {
        // Payment outcomes move `status` on the subscription; re-read it so past_due / active
        // lands in our mirror even if the subscription event is delayed.
        const invoice = event.data.object as Stripe.Invoice & {
          subscription?: string | { id: string } | null;
          parent?: { subscription_details?: { subscription?: string | { id: string } | null } | null } | null;
        };
        const ref = invoice.subscription ?? invoice.parent?.subscription_details?.subscription ?? null;
        const subId = typeof ref === 'string' ? ref : (ref?.id ?? null);
        if (subId) await syncSubscription(subId);
        break;
      }
    }

    return NextResponse.json({ ok: true });
  } catch (e) {
    // 500 → Stripe retries with backoff, which is what we want for a transient DB failure.
    // Release the claim first, or the retry would be swallowed as a duplicate.
    await releaseStripeEvent(event.id).catch(() => undefined);
    console.error('webhook handler error:', event.type, e);
    return NextResponse.json({ ok: false, error: 'server_error' }, { status: 500 });
  }
}
