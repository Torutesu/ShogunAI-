import { index, integer, pgTable, text, timestamp, uuid } from 'drizzle-orm/pg-core';

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

export type Participant = typeof participants.$inferSelect;
export type NewParticipant = typeof participants.$inferInsert;
