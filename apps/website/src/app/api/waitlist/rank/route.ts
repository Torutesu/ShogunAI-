import { findByStatusToken } from '@/db/queries';
import { fail } from '@/lib/http';
import { isValidStatusToken } from '@/lib/referral';
import {
  TOP_REFERRER_COUNT,
  currentTierFor,
  nextTierFor,
  pointsBreakdown,
  rankOf,
  totalPoints,
} from '@/lib/points';
import { NextResponse } from 'next/server';

export const runtime = 'nodejs';

/**
 * GET /api/waitlist/rank?code=<statusToken>
 * Private points/rank payload (spec §3.6 step 4). Auth: PRIVATE status token.
 * No email / IP / handle leaves here. noindex.
 */
export async function GET(req: Request) {
  const code = new URL(req.url).searchParams.get('code') ?? '';
  if (!isValidStatusToken(code)) return fail('bad_request');

  const row = await findByStatusToken(code);
  if (!row) return fail('not_found');

  const [points, rank, breakdown] = await Promise.all([
    totalPoints(row.id),
    rankOf(row.id),
    pointsBreakdown(row.id),
  ]);
  const tier = currentTierFor(points);
  const next = nextTierFor(points);

  return NextResponse.json(
    {
      ok: true as const,
      points,
      rank: rank?.rank ?? null,
      totalWaiting: rank?.total ?? null,
      joinPosition: row.joinPosition,
      breakdown, // { referral, quote, follow_product, follow_founder, form }
      tier: tier ? { points: tier.points, reward: tier.reward } : null,
      nextTier: next
        ? { points: next.points, reward: next.reward, remaining: next.points - points }
        : null,
      isTopReferrer: rank !== null && rank.rank <= TOP_REFERRER_COUNT,
    },
    { headers: { 'X-Robots-Tag': 'noindex', 'Cache-Control': 'no-store' } },
  );
}
