/**
 * The Stripe client and its configuration gate (issue #8).
 *
 * Nothing here reads configuration at module scope: a missing key must produce a clean 503 from
 * the route, not a crash at import time that takes the whole marketing site down. `billingReady()`
 * is what the LP asks before it shows a Checkout button at all.
 */

import Stripe from 'stripe';

let cached: { key: string; client: Stripe } | null = null;

export function stripeSecretKey(): string | null {
  return process.env.STRIPE_SECRET_KEY?.trim() || null;
}

export function webhookSecret(): string | null {
  return process.env.STRIPE_WEBHOOK_SECRET?.trim() || null;
}

/** Is the Checkout path fully configured (key + at least one price)? */
export function billingReady(): boolean {
  return (
    !!stripeSecretKey() &&
    !!process.env.STRIPE_PRICE_STANDARD_ANNUAL?.trim() &&
    !!process.env.STRIPE_PRICE_PRO_ANNUAL?.trim()
  );
}

/**
 * The shared client. Re-created only when the key changes (so a rotated key takes effect without
 * a cold start). `apiVersion` is deliberately left at the SDK default: pinning it here and in the
 * dashboard separately is how period fields silently go missing.
 */
export function stripe(): Stripe {
  const key = stripeSecretKey();
  if (!key) throw new Error('STRIPE_SECRET_KEY is not set');
  if (cached?.key !== key) {
    cached = {
      key,
      client: new Stripe(key, {
        appInfo: { name: 'ShogunAI', url: 'https://syogun.com' },
        maxNetworkRetries: 2,
      }),
    };
  }
  return cached.client;
}

/** Absolute origin for Checkout return URLs. */
export function appOrigin(): string {
  return process.env.NEXT_PUBLIC_APP_ORIGIN?.trim() || 'http://localhost:3000';
}

/**
 * Extra trial days granted by Stripe Checkout, from `STRIPE_TRIAL_DAYS`.
 *
 * The product's 7-day full trial is enforced locally by the Rust core from the onboarding stamp,
 * so a buyer who already ran it does not need a second one. Set this to 7 for a
 * card-on-file-but-not-charged LP funnel, or leave it at 0 to charge immediately and let the
 * local trial be the only trial. FR-BIL-06 keeps the choice a flag, not a rewrite.
 */
export function checkoutTrialDays(): number {
  const raw = Number(process.env.STRIPE_TRIAL_DAYS ?? '0');
  return Number.isFinite(raw) && raw > 0 ? Math.min(Math.floor(raw), 30) : 0;
}
