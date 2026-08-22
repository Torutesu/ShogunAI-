import {
  boolean,
  index,
  integer,
  pgTable,
  primaryKey,
  text,
  timestamp,
  unique,
  uuid,
} from 'drizzle-orm/pg-core';

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

    // gamification (SHOGUN waitlist points spec)
    joinPosition: integer('join_position'), // 1-based signup order
    xHandle: text('x_handle'), // OPTIONAL. lowercased, no @. only holders get social points

    // which plan the user intended when they signed up (e.g. "Pro · annual"); analytics only
    plan: text('plan'),

    // qualifying action = a short profile: answer1=why, answer2=company, answer3=challenge
    answer1: text('answer_1'),
    answer2: text('answer_2'),
    answer3: text('answer_3'),
  },
  (t) => [
    index('participants_ref_code_idx').on(t.refCode),
    index('participants_status_token_idx').on(t.statusToken),
    index('participants_referred_by_idx').on(t.referredBy),
    unique('participants_x_handle_key').on(t.xHandle),
  ],
);

/**
 * Append-only, idempotent points ledger (SHOGUN waitlist spec §3.4).
 * Rank = SUM(points) desc, join_position asc. Every award is deduped by
 * (entry_id, action_type, source_ref); source_ref is '' for one-shot actions
 * (form / follows) and the referred entry id / quote tweet id otherwise.
 */
export const pointsLedger = pgTable(
  'points_ledger',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    entryId: uuid('entry_id')
      .notNull()
      .references(() => participants.id, { onDelete: 'cascade' }),
    actionType: text('action_type').notNull(), // referral | quote | follow_product | follow_founder | form
    points: integer('points').notNull(),
    sourceRef: text('source_ref').notNull().default(''),
    awardedAt: timestamp('awarded_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    unique('points_ledger_dedup').on(t.entryId, t.actionType, t.sourceRef),
    index('points_ledger_entry_idx').on(t.entryId),
  ],
);

/** Snapshot of a monitored account's followers (product / founder). */
export const xFollowerSnapshot = pgTable(
  'x_follower_snapshot',
  {
    account: text('account').notNull(), // which account this snapshot is OF
    handle: text('handle').notNull(), // a follower handle (lowercased)
    snapshotAt: timestamp('snapshot_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [primaryKey({ columns: [t.account, t.handle, t.snapshotAt] })],
);

/** Snapshot of quote-tweets on the launch post. */
export const xQuoteSnapshot = pgTable('x_quote_snapshot', {
  tweetId: text('tweet_id').notNull(), // the launch tweet being quoted
  authorHandle: text('author_handle').notNull(), // quoting author (lowercased)
  quoteTweetId: text('quote_tweet_id').notNull(), // the quote tweet id
  text: text('text').notNull().default(''),
  snapshotAt: timestamp('snapshot_at', { withTimezone: true }).notNull().defaultNow(),
});

/**
 * DB-backed fixed-window rate limiter store. Keyed by "bucket:identifier"
 * (e.g. "signup:1.2.3.4"). Holds across serverless instances.
 */
export const rateLimits = pgTable('rate_limits', {
  key: text('key').primaryKey(),
  windowStart: timestamp('window_start', { withTimezone: true }).notNull(),
  count: integer('count').notNull().default(0),
});

export type Participant = typeof participants.$inferSelect;
export type NewParticipant = typeof participants.$inferInsert;
export type PointsRow = typeof pointsLedger.$inferSelect;
export type NewPointsRow = typeof pointsLedger.$inferInsert;

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
    /**
     * SHA-256 of the one-shot claim nonce the buying Mac minted before Checkout, so it can pull
     * this key down itself instead of the human transcribing it. Cleared the moment it is used —
     * a NULL here means "nothing to claim", which is also the state of every licence bought
     * before this existed.
     */
    claimNonceHash: text('claim_nonce_hash'),
    claimExpiresAt: timestamp('claim_expires_at', { withTimezone: true }),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    index('licenses_subscription_idx').on(t.stripeSubscriptionId),
    index('licenses_customer_idx').on(t.stripeCustomerId),
    index('licenses_claim_idx').on(t.claimNonceHash),
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

export type BillingCustomer = typeof billingCustomers.$inferSelect;
export type Subscription = typeof subscriptions.$inferSelect;
export type License = typeof licenses.$inferSelect;
