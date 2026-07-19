import {
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
