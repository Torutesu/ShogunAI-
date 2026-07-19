import { sql } from 'drizzle-orm';
import { db } from '@/db';
import { participants, pointsLedger, xFollowerSnapshot, xQuoteSnapshot } from '@/db/schema';

/**
 * Points engine for the SHOGUN waitlist (spec §1, §3).
 * Entry (email) is unconditional; points only *raise rank*. Every award is
 * idempotent via the (entry_id, action_type, source_ref) unique key, so the
 * social worker can re-run against fresh snapshots without double-counting.
 */

export type ActionType = 'referral' | 'quote' | 'follow_product' | 'follow_founder' | 'form';

export const POINTS: Record<ActionType, number> = {
  referral: 100,
  quote: 30,
  follow_product: 10,
  follow_founder: 10,
  form: 20,
};

/** Reward ladder — replacement (highest reached tier only), not additive. */
export const TIERS = [
  { points: 300, referralsEq: 3, reward: '1 month free' },
  { points: 1000, referralsEq: 10, reward: '3 months free' },
  { points: 3000, referralsEq: 30, reward: '6 months free' },
] as const;
export const TOP_REFERRER_COUNT = 10; // top-10 by points → 1 year free

/** Social award gates (spec §3.2 / §3.5). */
export const MIN_COMMENT_LEN = 10;
const AD_DISCLOSURE = /#ad|#pr/i;

/** Monitored X accounts / launch tweet. Configure via env in production. */
export const X_CONFIG = {
  product: (process.env.X_PRODUCT_ACCOUNT ?? 'shogun').toLowerCase(),
  founder: (process.env.X_FOUNDER_ACCOUNT ?? 'shogun_founder').toLowerCase(),
  launchTweetId: process.env.X_LAUNCH_TWEET_ID ?? '',
};

export function normalizeHandle(raw: unknown): string | null {
  if (typeof raw !== 'string') return null;
  const h = raw.trim().replace(/^@/, '').toLowerCase();
  return /^[a-z0-9_]{1,15}$/.test(h) ? h : null;
}

/** Idempotent award. Returns true if a NEW row was written. */
export async function award(
  entryId: string,
  action: ActionType,
  sourceRef = '',
): Promise<boolean> {
  const res = await db
    .insert(pointsLedger)
    .values({ entryId, actionType: action, points: POINTS[action], sourceRef })
    .onConflictDoNothing()
    .returning({ id: pointsLedger.id });
  return res.length > 0;
}

export async function totalPoints(entryId: string): Promise<number> {
  const [row] = await db
    .select({ n: sql<number>`COALESCE(SUM(${pointsLedger.points}), 0)::int` })
    .from(pointsLedger)
    .where(sql`${pointsLedger.entryId} = ${entryId}`);
  return row?.n ?? 0;
}

export async function pointsBreakdown(entryId: string): Promise<Record<string, number>> {
  const rows = await db
    .select({
      action: pointsLedger.actionType,
      n: sql<number>`SUM(${pointsLedger.points})::int`,
    })
    .from(pointsLedger)
    .where(sql`${pointsLedger.entryId} = ${entryId}`)
    .groupBy(pointsLedger.actionType);
  return Object.fromEntries(rows.map((r) => [r.action, r.n]));
}

/** Rank = SUM(points) desc, join_position asc. 1-based. null if no such entry. */
export async function rankOf(entryId: string): Promise<{ rank: number; total: number } | null> {
  const rows = await db.execute(sql`
    SELECT rank, total FROM (
      SELECT p.id,
             RANK() OVER (ORDER BY COALESCE(l.total, 0) DESC, p.join_position ASC NULLS LAST) AS rank,
             (SELECT COUNT(*) FROM participants) AS total
      FROM participants p
      LEFT JOIN (SELECT entry_id, SUM(points) AS total FROM points_ledger GROUP BY entry_id) l
             ON l.entry_id = p.id
    ) r WHERE r.id = ${entryId}
  `);
  const row = (rows as unknown as Array<{ rank: number; total: number }>)[0];
  return row ? { rank: Number(row.rank), total: Number(row.total) } : null;
}

export async function pointsLeaderboard(limit = 10) {
  const rows = await db.execute(sql`
    SELECT p.id,
           p.nickname,
           p.ref_code,
           COALESCE(l.total, 0)::int AS points
    FROM participants p
    LEFT JOIN (SELECT entry_id, SUM(points) AS total FROM points_ledger GROUP BY entry_id) l
           ON l.entry_id = p.id
    ORDER BY points DESC, p.join_position ASC NULLS LAST
    LIMIT ${limit}
  `);
  return rows as unknown as Array<{ id: string; nickname: string | null; ref_code: string | null; points: number }>;
}

/** Highest reached reward tier (replacement, not additive). */
export function currentTierFor(points: number) {
  let reached = null as (typeof TIERS)[number] | null;
  for (const t of TIERS) if (points >= t.points) reached = t;
  return reached;
}
export function nextTierFor(points: number) {
  return TIERS.find((t) => points < t.points) ?? null;
}

/**
 * Social awards from the latest snapshots (spec §3.1 batch-pull design).
 * Runs over every entry that submitted an x_handle. Idempotent.
 * Returns the number of new awards written.
 */
export async function computeSocialAwards(): Promise<number> {
  const entries = await db
    .select({ id: participants.id, handle: participants.xHandle })
    .from(participants)
    .where(sql`${participants.xHandle} IS NOT NULL`);
  if (entries.length === 0) return 0;

  // Latest snapshot per account → follower handle sets.
  const followerRows = await db.execute(sql`
    SELECT s.account, s.handle
    FROM x_follower_snapshot s
    JOIN (SELECT account, MAX(snapshot_at) AS mx FROM x_follower_snapshot GROUP BY account) m
      ON m.account = s.account AND m.mx = s.snapshot_at
  `);
  const followers = new Map<string, Set<string>>();
  for (const r of followerRows as unknown as Array<{ account: string; handle: string }>) {
    const acc = r.account.toLowerCase();
    if (!followers.has(acc)) followers.set(acc, new Set());
    followers.get(acc)!.add(r.handle.toLowerCase());
  }
  const productSet = followers.get(X_CONFIG.product) ?? new Set<string>();
  const founderSet = followers.get(X_CONFIG.founder) ?? new Set<string>();

  // Latest quote snapshot per author.
  const quoteRows = await db.execute(sql`
    SELECT DISTINCT ON (author_handle) author_handle, quote_tweet_id, text
    FROM x_quote_snapshot
    ORDER BY author_handle, snapshot_at DESC
  `);
  const quotes = new Map<string, { quoteTweetId: string; text: string }>();
  for (const r of quoteRows as unknown as Array<{ author_handle: string; quote_tweet_id: string; text: string }>) {
    quotes.set(r.author_handle.toLowerCase(), { quoteTweetId: r.quote_tweet_id, text: r.text ?? '' });
  }

  let written = 0;
  for (const e of entries) {
    const handle = (e.handle as string).toLowerCase();
    if (productSet.has(handle) && (await award(e.id, 'follow_product'))) written++;
    if (founderSet.has(handle) && (await award(e.id, 'follow_founder'))) written++;

    const q = quotes.get(handle);
    if (q && q.text.trim().length >= MIN_COMMENT_LEN && AD_DISCLOSURE.test(q.text)) {
      if (await award(e.id, 'quote', q.quoteTweetId)) written++;
    }
  }
  return written;
}
