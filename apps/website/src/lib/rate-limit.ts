import { sql } from 'drizzle-orm';
import { db } from '@/db';
import { withTimeout } from './timeout';

/**
 * DB-backed fixed-window rate limiter (REFERRAL_ENGINE.md §6.6).
 * Backed by the DB so limits hold across serverless instances. Fails OPEN
 * on any DB error — availability over strictness for a signup form.
 *
 * The upsert is atomic: the window either advances (count resets to 1) or
 * the counter increments, decided inside a single statement.
 */
export type RateLimitResult = { allowed: boolean; count: number; limit: number };

/** Deadline for the limiter's own query. See the fail-open note on the catch below. */
export const RATE_LIMIT_TIMEOUT_MS = 2_000;

export async function rateLimit(
  bucket: string,
  identifier: string,
  opts: { limit: number; windowSec: number },
): Promise<RateLimitResult> {
  const key = `${bucket}:${identifier}`;
  try {
    const rows = await withTimeout(
      db.execute<{ count: number }>(sql`
      INSERT INTO rate_limits (key, window_start, count)
      VALUES (${key}, now(), 1)
      ON CONFLICT (key) DO UPDATE SET
        count = CASE
          WHEN rate_limits.window_start < now() - make_interval(secs => ${opts.windowSec})
          THEN 1 ELSE rate_limits.count + 1 END,
        window_start = CASE
          WHEN rate_limits.window_start < now() - make_interval(secs => ${opts.windowSec})
          THEN now() ELSE rate_limits.window_start END
      RETURNING count
    `),
      // A limiter that stalls has already failed at its job; waiting longer only spends the
      // request's budget. Short, because this runs before everything else on every route.
      RATE_LIMIT_TIMEOUT_MS,
      'rate-limit query',
    );
    const count = Number(rows[0]?.count ?? 1);
    return { allowed: count <= opts.limit, count, limit: opts.limit };
  } catch (err) {
    // Fail open: never let a limiter outage take down signup. The `withTimeout` above is what
    // makes that promise keepable — a hung database is not a rejected promise, so without a
    // deadline this catch never runs and the runtime kills the request instead.
    console.error('rate-limit error (failing open):', err);
    return { allowed: true, count: 0, limit: opts.limit };
  }
}
