import { sql } from 'drizzle-orm';
import { db } from '@/db';
import { xFollowerSnapshot, xQuoteSnapshot } from '@/db/schema';
import { X_CONFIG, computeSocialAwards, normalizeHandle } from './points';

/**
 * X data seam (spec §3.1, §4). The award engine only ever reads the snapshot
 * tables, so the *source* of those snapshots is swappable: twscrape now, the
 * official X API later. Implement `XSource` and pass it to `runSnapshotSync`.
 */
export interface XSource {
  /** Followers of `account` (handles, any casing). Batch pull, not per-user. */
  fetchFollowers(account: string): Promise<string[]>;
  /** Quote-tweets of `tweetId`: the quoting author + the quote id + its text. */
  fetchQuotes(tweetId: string): Promise<Array<{ authorHandle: string; quoteTweetId: string; text: string }>>;
}

/** Persist one followers snapshot for an account. */
export async function ingestFollowers(account: string, handles: string[]): Promise<number> {
  const acc = account.toLowerCase();
  const rows = handles
    .map((h) => normalizeHandle(h))
    .filter((h): h is string => !!h)
    .map((handle) => ({ account: acc, handle }));
  if (rows.length === 0) return 0;
  await db.insert(xFollowerSnapshot).values(rows).onConflictDoNothing();
  return rows.length;
}

/** Persist one quote-tweets snapshot for a launch tweet. */
export async function ingestQuotes(
  tweetId: string,
  quotes: Array<{ authorHandle: string; quoteTweetId: string; text: string }>,
): Promise<number> {
  const rows = quotes
    .map((q) => {
      const handle = normalizeHandle(q.authorHandle);
      return handle ? { tweetId, authorHandle: handle, quoteTweetId: q.quoteTweetId, text: q.text ?? '' } : null;
    })
    .filter((r): r is NonNullable<typeof r> => !!r);
  if (rows.length === 0) return 0;
  await db.insert(xQuoteSnapshot).values(rows);
  return rows.length;
}

/**
 * Full snapshot cycle (spec §3.6 step 3): pull the three snapshots via the
 * given source, persist them, then award idempotently. Returns a summary.
 */
export async function runSnapshotSync(source: XSource) {
  const [product, founder] = await Promise.all([
    source.fetchFollowers(X_CONFIG.product),
    source.fetchFollowers(X_CONFIG.founder),
  ]);
  await ingestFollowers(X_CONFIG.product, product);
  await ingestFollowers(X_CONFIG.founder, founder);

  let quoteCount = 0;
  if (X_CONFIG.launchTweetId) {
    const quotes = await source.fetchQuotes(X_CONFIG.launchTweetId);
    quoteCount = await ingestQuotes(X_CONFIG.launchTweetId, quotes);
  }

  const awarded = await computeSocialAwards();
  return { product: product.length, founder: founder.length, quotes: quoteCount, awarded };
}

/**
 * twscrape-backed source. Kept as a documented stub: twscrape is a Python
 * tool, so wire this to a small bridge (a local HTTP endpoint or a shelled
 * process that returns JSON) reusing the LEADSHOGUN twscrape+Supabase setup.
 * Throws until configured so a half-set-up cron fails loudly instead of
 * silently zeroing out everyone's social points.
 */
export const twscrapeSource: XSource = {
  async fetchFollowers() {
    throw new Error('twscrapeSource not configured — wire it to the twscrape bridge (see xsource.ts).');
  },
  async fetchQuotes() {
    throw new Error('twscrapeSource not configured — wire it to the twscrape bridge (see xsource.ts).');
  },
};

/** Latest snapshot age per account — for the admin dashboard / freshness checks. */
export async function snapshotFreshness() {
  const rows = await db.execute(sql`
    SELECT account, MAX(snapshot_at) AS latest FROM x_follower_snapshot GROUP BY account
  `);
  return rows as unknown as Array<{ account: string; latest: string }>;
}
