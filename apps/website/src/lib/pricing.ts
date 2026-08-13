/**
 * The pricing catalog — the ONE source of truth for what a plan costs (issue #8).
 *
 * CLAUDE.md プラン構成: no Free plan, a 7-day full trial (Pro-equivalent), then Standard / Pro.
 * Amounts are current as of 2026-08-10 and match the LP copy in `src/i18n/dictionaries.ts`
 * (Standard $49/mo billed annually, $62 month-to-month; Pro $99/mo billed annually,
 * $124 month-to-month). The issue text's "$50–60/mo" predates the 2026-07-26 pricing decision —
 * it is NOT the live price. Change prices here, then in the dictionaries, then in Stripe.
 *
 * Price IDs are deliberately NOT in this file and NEVER reach the browser: the client asks for
 * `{ plan, interval }` and the server resolves the Stripe price from the environment
 * (issue #8 セキュリティ: "Price ID などはフロントにハードコードせず、基本はバックエンド側で保持").
 */

export const PLAN_IDS = ['standard', 'pro'] as const;
export type PlanId = (typeof PLAN_IDS)[number];

export const INTERVALS = ['monthly', 'annual'] as const;
export type Interval = (typeof INTERVALS)[number];

/** Currency for v1. OPEN-05 (price localisation / JPY) stays open — see requirements §9. */
export const CURRENCY = 'usd' as const;

export interface PlanPrice {
  /** What Stripe charges per billing cycle, in the smallest currency unit (cents). */
  readonly amountCents: number;
  /** The per-month figure we advertise, in cents (annual is billed once at 12×). */
  readonly perMonthCents: number;
  /** Environment variable holding the Stripe Price ID for this plan × interval. */
  readonly priceEnv: string;
}

export interface Plan {
  readonly id: PlanId;
  readonly name: string;
  /** Stripe Product name — kept aligned with the dashboard object. */
  readonly productName: string;
  readonly prices: Readonly<Record<Interval, PlanPrice>>;
}

export const PLANS: Readonly<Record<PlanId, Plan>> = {
  standard: {
    id: 'standard',
    name: 'Standard',
    productName: 'ShogunAI Standard',
    prices: {
      annual: { amountCents: 58_800, perMonthCents: 4_900, priceEnv: 'STRIPE_PRICE_STANDARD_ANNUAL' },
      monthly: { amountCents: 6_200, perMonthCents: 6_200, priceEnv: 'STRIPE_PRICE_STANDARD_MONTHLY' },
    },
  },
  pro: {
    id: 'pro',
    name: 'Pro',
    productName: 'ShogunAI Pro',
    prices: {
      annual: { amountCents: 118_800, perMonthCents: 9_900, priceEnv: 'STRIPE_PRICE_PRO_ANNUAL' },
      monthly: { amountCents: 12_400, perMonthCents: 12_400, priceEnv: 'STRIPE_PRICE_PRO_MONTHLY' },
    },
  },
};

/** Length of the full trial, in days. Mirrors `TRIAL_DURATION_MS` in the Rust core. */
export const TRIAL_DAYS = 7;

export function isPlanId(v: unknown): v is PlanId {
  return typeof v === 'string' && (PLAN_IDS as readonly string[]).includes(v);
}

export function isInterval(v: unknown): v is Interval {
  return typeof v === 'string' && (INTERVALS as readonly string[]).includes(v);
}

/**
 * The Stripe Price ID for a plan × interval, read from the environment at call time
 * (never cached at module scope — the routes must see a value set after a redeploy).
 * `null` when unconfigured, which the routes turn into a 503 rather than a silent wrong charge.
 */
export function priceIdFor(plan: PlanId, interval: Interval): string | null {
  const raw = process.env[PLANS[plan].prices[interval].priceEnv];
  const id = raw?.trim();
  return id ? id : null;
}

/**
 * Reverse lookup: which plan × interval does a Stripe Price ID denote? Used by the webhook to
 * turn `subscription.items[0].price.id` into the plan the Rust core enforces. An unknown price
 * (a Stripe-dashboard price nobody told us about) returns null — the caller must not guess a
 * plan, because guessing "pro" would hand out entitlements nobody paid for.
 */
export function planForPriceId(priceId: string): { plan: PlanId; interval: Interval } | null {
  for (const plan of PLAN_IDS) {
    for (const interval of INTERVALS) {
      if (priceIdFor(plan, interval) === priceId) return { plan, interval };
    }
  }
  return null;
}

/** "$49" / "$62" — display helper for the success page and server-rendered copy. */
export function formatUsd(cents: number): string {
  const dollars = cents / 100;
  return Number.isInteger(dollars)
    ? `$${dollars.toLocaleString('en-US')}`
    : `$${dollars.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
}

/** One line of pricing copy, e.g. "$49/mo, billed annually ($588/yr)". */
export function priceLine(plan: PlanId, interval: Interval): string {
  const p = PLANS[plan].prices[interval];
  return interval === 'annual'
    ? `${formatUsd(p.perMonthCents)}/mo, billed annually (${formatUsd(p.amountCents)}/yr)`
    : `${formatUsd(p.amountCents)}/mo`;
}
