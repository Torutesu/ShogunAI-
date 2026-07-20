import { adminStats } from '@/lib/admin';
import { isAdmin } from '@/lib/admin-auth';
import { fail } from '@/lib/http';
import { snapshotFreshness } from '@/lib/xsource';
import { NextResponse } from 'next/server';

export const runtime = 'nodejs';

/** GET /api/admin/stats — internal metrics. Auth: x-admin-token header. noindex. */
export async function GET(req: Request) {
  if (!isAdmin(req)) return fail('forbidden');
  const [stats, freshness] = await Promise.all([adminStats(), snapshotFreshness()]);
  return NextResponse.json(
    { ok: true as const, ...stats, snapshots: freshness },
    { headers: { 'X-Robots-Tag': 'noindex', 'Cache-Control': 'no-store' } },
  );
}
