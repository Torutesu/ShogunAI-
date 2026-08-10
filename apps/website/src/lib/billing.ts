/**
 * Pure billing logic (issue #8): Stripe objects → the two rows we keep, and the entitlement
 * question "may this subscription unlock the app right now?".
 *
 * Everything here is a pure function of its arguments — no Stripe SDK, no DB, no clock reads
 * (`nowMs` is always a parameter, mirroring the Rust core's convention in
 * `crates/shogun-agents/src/entitlement.rs`). That keeps the state machine testable without a
 * Stripe account and keeps the webhook route a thin shell around it.
 */

import { type Interval, type PlanId, planForPriceId } from './pricing';

/** The Stripe subscription statuses we store verbatim. */
export const SUBSCRIPTION_STATUSES = [
  'trialing',
  'active',
  'past_due',
  'canceled',
  'unpaid',
  'incomplete',
  'incomplete_expired',
  'paused',
] as const;
export type SubscriptionStatus = (typeof SUBSCRIPTION_STATUSES)[number];

export function isSubscriptionStatus(v: unknown): v is SubscriptionStatus {
  return typeof v === 'string' && (SUBSCRIPTION_STATUSES as readonly string[]).includes(v);
}

/**
 * FR-BIL-09: after a failed payment Stripe runs its own retry (dunning) schedule; we keep the
 * user working through it and for 7 more days after the period lapses. Past that, access stops.
 */
export const PAST_DUE_GRACE_DAYS = 7;

/** Just enough of a Stripe Subscription for the mapper — structural, so tests need no SDK. */
export interface StripeSubscriptionLike {
  id: string;
  status: string;
  customer: string | { id: string };
  cancel_at?: number | null;
  canceled_at?: number | null;
  cancel_at_period_end?: boolean | null;
  trial_end?: number | null;
  /** Present on older API versions; newer ones carry the period on the item (handled below). */
  current_period_start?: number | null;
  current_period_end?: number | null;
  items?: {
    data?: Array<{
      price?: { id?: string | null } | null;
      current_period_start?: number | null;
      current_period_end?: number | null;
    }>;
  };
}

/** What we persist per subscription. Unix **seconds** throughout, as Stripe sends them. */
export interface SubscriptionRecord {
  stripeSubscriptionId: string;
  stripeCustomerId: string;
  stripePriceId: string | null;
  plan: PlanId | null;
  interval: Interval | null;
  status: SubscriptionStatus;
  currentPeriodStart: number | null;
  currentPeriodEnd: number | null;
  cancelAt: number | null;
  canceledAt: number | null;
  cancelAtPeriodEnd: boolean;
  trialEnd: number | null;
}

export function customerIdOf(sub: StripeSubscriptionLike): string {
  return typeof sub.customer === 'string' ? sub.customer : sub.customer.id;
}

/**
 * Read the billing period off a subscription. Stripe moved `current_period_*` from the
 * subscription onto its items in the 2025-03 API versions, so read both and prefer whichever is
 * present — a null period would otherwise silently become "no access" for a paying customer.
 */
function periodOf(sub: StripeSubscriptionLike): { start: number | null; end: number | null } {
  const item = sub.items?.data?.[0];
  return {
    start: sub.current_period_start ?? item?.current_period_start ?? null,
    end: sub.current_period_end ?? item?.current_period_end ?? null,
  };
}

/** Stripe Subscription → the row we upsert. Unknown statuses fall back to `incomplete` (locked). */
export function toSubscriptionRecord(sub: StripeSubscriptionLike): SubscriptionRecord {
  const priceId = sub.items?.data?.[0]?.price?.id ?? null;
  const mapped = priceId ? planForPriceId(priceId) : null;
  const period = periodOf(sub);
  return {
    stripeSubscriptionId: sub.id,
    stripeCustomerId: customerIdOf(sub),
    stripePriceId: priceId,
    plan: mapped?.plan ?? null,
    interval: mapped?.interval ?? null,
    status: isSubscriptionStatus(sub.status) ? sub.status : 'incomplete',
    currentPeriodStart: period.start,
    currentPeriodEnd: period.end,
    cancelAt: sub.cancel_at ?? null,
    canceledAt: sub.canceled_at ?? null,
    cancelAtPeriodEnd: !!sub.cancel_at_period_end,
    trialEnd: sub.trial_end ?? null,
  };
}

/**
 * Does this subscription entitle the app right now?
 *
 * - `active` / `trialing` → yes. The period end is only a backstop here: a renewal moves it
 *   forward before it passes, so a period end in the past means a webhook we have not processed
 *   yet, and the same `PAST_DUE_GRACE_DAYS` window absorbs that lag.
 * - `past_due` → yes, through Stripe's retry window and `PAST_DUE_GRACE_DAYS` past the period
 *   end (FR-BIL-09). Cutting a paying customer off on the first failed card charge is the
 *   single most expensive false negative in this whole flow.
 * - anything else (`canceled` / `unpaid` / `incomplete*` / `paused`) → no.
 *
 * A subscription whose price we do not recognise (`plan === null`) never entitles: the Rust core
 * needs a concrete Standard/Pro, and guessing would hand out what nobody bought.
 */
export function isEntitled(rec: SubscriptionRecord, nowMs: number): boolean {
  if (!rec.plan) return false;
  const nowSec = Math.floor(nowMs / 1000);
  const end = rec.currentPeriodEnd;
  switch (rec.status) {
    case 'active':
    case 'trialing':
    case 'past_due':
      // A missing period end (rare, mid-creation) is treated as entitled: the status is the
      // stronger signal, and the next webhook fills the period in.
      return end === null || nowSec <= end + PAST_DUE_GRACE_DAYS * 86_400;
    default:
      return false;
  }
}

/**
 * The subset of a subscription the desktop app is allowed to learn. Deliberately tiny —
 * NFR-PRV-04: the licence server holds account/billing state only, and the app needs exactly
 * "which plan, until when, and is it healthy".
 */
export interface LicenseView {
  plan: PlanId | null;
  status: SubscriptionStatus;
  entitled: boolean;
  currentPeriodEnd: number | null;
  cancelAtPeriodEnd: boolean;
}

export function toLicenseView(rec: SubscriptionRecord, nowMs: number): LicenseView {
  return {
    plan: rec.plan,
    status: rec.status,
    entitled: isEntitled(rec, nowMs),
    currentPeriodEnd: rec.currentPeriodEnd,
    cancelAtPeriodEnd: rec.cancelAtPeriodEnd,
  };
}
