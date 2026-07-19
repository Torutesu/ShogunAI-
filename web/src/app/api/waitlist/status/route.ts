import { findByStatusToken } from '@/db/queries';
import { fail } from '@/lib/http';
import {
  TOP_REFERRER_COUNT,
  currentTier,
  isValidStatusToken,
  nextTier,
  shareUrl,
} from '@/lib/referral';
import { answeredCount, countQualifiedReferrals } from '@/lib/service';
import { leaderboardRank, queuePosition } from '@/db/queries';
import { NextResponse } from 'next/server';

export const runtime = 'nodejs';

const APP_ORIGIN = process.env.NEXT_PUBLIC_APP_ORIGIN ?? 'http://localhost:3000';

/**
 * GET /api/waitlist/status?code=<statusToken>
 * Private dashboard payload. Auth: the PRIVATE status token.
 * Response minimization (§6.7): NO email / IP / UA. noindex header.
 */
export async function GET(req: Request) {
  const code = new URL(req.url).searchParams.get('code') ?? '';
  if (!isValidStatusToken(code)) return fail('bad_request');

  const row = await findByStatusToken(code);
  if (!row || !row.refCode) return fail('not_found');

  const count = await countQualifiedReferrals(row.refCode);
  const pos = await queuePosition(row.refCode);
  const rank = await leaderboardRank(row.refCode);
  const tier = currentTier(count);
  const next = nextTier(count);

  const body = {
    ok: true as const,
    status: row.status,
    refCode: row.refCode, // public — needed to build the share link
    shareUrl: shareUrl(APP_ORIGIN, row.refCode),
    qualifiedReferrals: count,
    position: pos?.position ?? null,
    totalWaiting: pos?.total ?? null,
    answered: answeredCount(row),
    profileComplete: !!row.qualifiedAt,
    tier: tier ? { reward: tier.reward, label: tier.label, threshold: tier.threshold } : null,
    nextTier: next
      ? { reward: next.reward, label: next.label, threshold: next.threshold, remaining: next.threshold - count }
      : null,
    leaderboardRank: rank,
    isTopReferrer: rank !== null && rank <= TOP_REFERRER_COUNT,
  };

  return NextResponse.json(body, {
    headers: {
      'X-Robots-Tag': 'noindex',
      'Cache-Control': 'no-store',
    },
  });
}
