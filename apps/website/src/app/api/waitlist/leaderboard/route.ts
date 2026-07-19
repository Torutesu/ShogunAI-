import { leaderboard } from '@/db/queries';
import { NextResponse } from 'next/server';

export const runtime = 'nodejs';

const MAX_LIMIT = 50;

/** Public handle: the chosen nickname, or an anonymous, stable fallback. */
function handleFor(nickname: string | null, refCode: string): string {
  if (nickname && nickname.trim()) return nickname.trim();
  return `shogun-${refCode.slice(0, 4).toLowerCase()}`;
}

/**
 * GET /api/waitlist/leaderboard?limit=N
 * Public. Ranks by nickname (never email). Raw email/tokens never leave the DB.
 */
export async function GET(req: Request) {
  const raw = Number(new URL(req.url).searchParams.get('limit') ?? '10');
  const limit = Number.isFinite(raw) ? Math.min(Math.max(1, Math.trunc(raw)), MAX_LIMIT) : 10;

  const rows = await leaderboard(limit);
  const board = rows.map((r, i) => ({
    rank: i + 1,
    name: handleFor(r.nickname, r.refCode),
    refCode: r.refCode,
    count: r.qualified,
  }));

  return NextResponse.json(
    { ok: true, board },
    { headers: { 'Cache-Control': 'public, max-age=30, stale-while-revalidate=60' } },
  );
}
