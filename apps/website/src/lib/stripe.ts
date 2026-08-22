/**
 * The Stripe client and its configuration gate (issue #8).
 *
 * Nothing here reads configuration at module scope: a missing key must produce a clean 503 from
 * the route, not a crash at import time that takes the whole marketing site down.
 */

import Stripe from 'stripe';

import { INTERVALS, PLAN_IDS, priceIdFor } from './pricing';
import { siteConfig } from './site';

let cached: { key: string; client: Stripe } | null = null;

export function stripeSecretKey(): string | null {
  return process.env.STRIPE_SECRET_KEY?.trim() || null;
}

export function webhookSecret(): string | null {
  return process.env.STRIPE_WEBHOOK_SECRET?.trim() || null;
}

/**
 * Is the Checkout path fully configured — the key, and a price for **every** plan × interval?
 *
 * Every combination, not just the annual pair: the app offers all four as buttons
 * (`PLAN_CHOICES` in the desktop settings panel), so a half-configured environment means a buyer
 * picks monthly and gets a 503 while annual quietly works. An ops mistake should take billing
 * down loudly rather than leave one purchase path broken and unnoticed.
 *
 * Derived from the catalog rather than named env vars, so adding a plan or an interval extends
 * the gate instead of silently escaping it.
 */
export function billingReady(): boolean {
  if (!stripeSecretKey()) return false;
  return PLAN_IDS.every((plan) => INTERVALS.every((interval) => priceIdFor(plan, interval) !== null));
}

/**
 * Whether to let Stripe Tax compute VAT / sales tax on Checkout (`STRIPE_AUTOMATIC_TAX=1`).
 *
 * Off by default and deliberately a flag: `automatic_tax` makes session creation fail outright
 * unless Stripe Tax is enabled and an origin address is set in the dashboard, so turning it on in
 * code before the account is configured would break every purchase. Flip it once the dashboard
 * side exists.
 *
 * Whether we are additionally *registered* to remit that tax in a given country is a separate,
 * non-engineering question — collecting the number first is what makes it answerable.
 */
export function automaticTaxEnabled(): boolean {
  return process.env.STRIPE_AUTOMATIC_TAX?.trim() === '1';
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

/**
 * Absolute origin for Checkout and Customer Portal return URLs.
 *
 * Read at **runtime** from `APP_ORIGIN`, not from `NEXT_PUBLIC_APP_ORIGIN`: Next inlines
 * `NEXT_PUBLIC_*` at build time, and the deploy workflow builds without it set, so the old
 * localhost fallback was being compiled into the Worker — a real buyer would have been redirected
 * to `http://localhost:3000` after paying. `NEXT_PUBLIC_APP_ORIGIN` is still honoured for local
 * `next dev`, and the last resort is the production domain rather than localhost, so a missing
 * variable degrades to "right for production" instead of "broken in production".
 */
export function appOrigin(): string {
  return (
    process.env.APP_ORIGIN?.trim() ||
    process.env.NEXT_PUBLIC_APP_ORIGIN?.trim() ||
    siteConfig.url
  );
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
