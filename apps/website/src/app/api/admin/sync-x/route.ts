import { isAdmin } from '@/lib/admin-auth';
import { fail } from '@/lib/http';
import { httpXSource, runSnapshotSync } from '@/lib/xsource';
import { NextResponse } from 'next/server';

export const runtime = 'nodejs';

/**
 * POST /api/admin/sync-x?key=<ADMIN_TOKEN>
 * Pulls the follower/quote snapshots via the twscrape bridge and awards
 * social points idempotently (spec §3.6 step 3). Safe to call on a schedule
 * (external cron / GitHub Action hitting this endpoint).
 */
export async function POST(req: Request) {
  if (!isAdmin(req)) return fail('forbidden');
  try {
    const summary = await runSnapshotSync(httpXSource());
    return NextResponse.json({ ok: true as const, ...summary }, { headers: { 'Cache-Control': 'no-store' } });
  } catch (e) {
    return NextResponse.json(
      { ok: false as const, error: e instanceof Error ? e.message : 'sync_failed' },
      { status: 500 },
    );
  }
}
