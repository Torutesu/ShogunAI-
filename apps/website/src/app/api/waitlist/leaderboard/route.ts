import { leaderboard } from '@/db/queries';
import { maskEmail } from '@/lib/referral';
import { NextResponse } from 'next/server';

export const runtime = 'nodejs';

const MAX_LIMIT = 50;

/**
 * GET /api/waitlist/leaderboard?limit=N
 * Public. Emails are masked in app code; raw email/tokens never leave the DB.
 */
export async function GET(req: Request) {
  const raw = Number(new URL(req.url).searchParams.get('limit') ?? '10');
  const limit = Number.isFinite(raw) ? Math.min(Math.max(1, Math.trunc(raw)), MAX_LIMIT) : 10;

  const rows = await leaderboard(limit);
  const board = rows.map((r, i) => ({
    rank: i + 1,
    maskedEmail: maskEmail(r.email),
    count: r.qualified,
  }));

  return NextResponse.json(
    { ok: true, board },
    { headers: { 'Cache-Control': 'public, max-age=30, stale-while-revalidate=60' } },
  );
}
