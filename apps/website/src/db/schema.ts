import { boolean, index, integer, pgTable, text, timestamp, uuid } from 'drizzle-orm/pg-core';

/**
 * Waitlist participants. Each row carries the two-token invariant:
 *   - refCode      PUBLIC  — broadcast in share links, grants attribution only
 *   - statusToken  PRIVATE — bearer for reading own status / writing answers
 * See REFERRAL_ENGINE.md §1. Never let the public code read/write status.
 */
export const participants = pgTable(
  'participants',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    email: text('email').notNull().unique(),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
    status: text('status').notNull().default('pending'), // pending | invited | converted

    // referral engine
    refCode: text('ref_code').unique(), // PUBLIC share code
    statusToken: text('status_token').unique(), // PRIVATE bearer
    referredBy: text('referred_by'), // the ref_code that referred this row
    qualifiedAt: timestamp('qualified_at', { withTimezone: true }), // qualifying action ts

    // salted hash of the signup IP — used only for referral-fraud checks at reward time
    ipHash: text('ip_hash'),

    // public handle used on the leaderboard (never the email)
    nickname: text('nickname'),

    // qualifying action = a short profile: answer1=why, answer2=company, answer3=challenge
    answer1: text('answer_1'),
    answer2: text('answer_2'),
    answer3: text('answer_3'),
  },
  (t) => [
    index('participants_ref_code_idx').on(t.refCode),
    index('participants_status_token_idx').on(t.statusToken),
    index('participants_referred_by_idx').on(t.referredBy),
  ],
);

/**
 * DB-backed fixed-window rate limiter store. Keyed by "bucket:identifier"
 * (e.g. "signup:1.2.3.4"). Holds across serverless instances.
 */
export const rateLimits = pgTable('rate_limits', {
  key: text('key').primaryKey(),
  windowStart: timestamp('window_start', { withTimezone: true }).notNull(),
  count: integer('count').notNull().default(0),
});

/**
 * Billing (issue #8). Three tables, one job each:
 *
 *   billing_customers  — the 1:1 map between a human (email) and a Stripe customer.
 *   subscriptions      — a mirror of the Stripe subscription, kept current by the webhook. This
 *                        is what "誰がいつまで使えるのか" is answered from, without calling Stripe.
 *   licenses           — the credential the desktop app presents. One per subscription.
 *
 * NFR-PRV-04: the server holds account / billing / licence state only. No capture content, no
 * memory content, ever.
 */
export const billingCustomers = pgTable(
  'billing_customers',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    email: text('email').notNull().unique(),
    stripeCustomerId: text('stripe_customer_id').notNull().unique(),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [index('billing_customers_stripe_idx').on(t.stripeCustomerId)],
);

export const subscriptions = pgTable(
  'subscriptions',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    stripeSubscriptionId: text('stripe_subscription_id').notNull().unique(),
    stripeCustomerId: text('stripe_customer_id').notNull(),
    stripePriceId: text('stripe_price_id'),
    /** 'standard' | 'pro' — null when the price is one we do not recognise (never entitles). */
    plan: text('plan'),
    /** 'monthly' | 'annual' */
    interval: text('interval'),
    /** trialing | active | past_due | canceled | unpaid | incomplete | incomplete_expired | paused */
    status: text('status').notNull(),
    currentPeriodStart: timestamp('current_period_start', { withTimezone: true }),
    currentPeriodEnd: timestamp('current_period_end', { withTimezone: true }),
    cancelAt: timestamp('cancel_at', { withTimezone: true }),
    canceledAt: timestamp('canceled_at', { withTimezone: true }),
    cancelAtPeriodEnd: boolean('cancel_at_period_end').notNull().default(false),
    trialEnd: timestamp('trial_end', { withTimezone: true }),
    updatedAt: timestamp('updated_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    index('subscriptions_customer_idx').on(t.stripeCustomerId),
    index('subscriptions_status_idx').on(t.status),
  ],
);

export const licenses = pgTable(
  'licenses',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    /** The bearer the app presents. Canonical form (`shogun-XXXX-…`); shown to the human once. */
    licenseKey: text('license_key').notNull().unique(),
    stripeCustomerId: text('stripe_customer_id').notNull(),
    stripeSubscriptionId: text('stripe_subscription_id').notNull().unique(),
    email: text('email'),
    /** Anonymous device id of the last Mac that verified (FR-BIL-08 sends nothing else). */
    lastDeviceId: text('last_device_id'),
    lastAppVersion: text('last_app_version'),
    lastVerifiedAt: timestamp('last_verified_at', { withTimezone: true }),
    /** Distinct devices seen — the seat-abuse signal, without storing a device list. */
    deviceCount: integer('device_count').notNull().default(0),
    revokedAt: timestamp('revoked_at', { withTimezone: true }),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    index('licenses_subscription_idx').on(t.stripeSubscriptionId),
    index('licenses_customer_idx').on(t.stripeCustomerId),
  ],
);

/**
 * Webhook idempotency. Stripe retries, and it can deliver the same event twice in normal
 * operation; a second `checkout.session.completed` must not mint a second licence key.
 */
export const stripeEvents = pgTable('stripe_events', {
  id: text('id').primaryKey(), // Stripe's evt_… id
  type: text('type').notNull(),
  receivedAt: timestamp('received_at', { withTimezone: true }).notNull().defaultNow(),
});

export type Participant = typeof participants.$inferSelect;
export type NewParticipant = typeof participants.$inferInsert;
export type BillingCustomer = typeof billingCustomers.$inferSelect;
export type Subscription = typeof subscriptions.$inferSelect;
export type License = typeof licenses.$inferSelect;
