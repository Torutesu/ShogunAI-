import { ok } from '@/lib/http';
import { getParticipantCount } from '@/lib/waitlist-metrics';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

/** Public count backed by D1, independent from the Supabase signup database. */
export async function GET() {
  try {
    const count = await getParticipantCount();
    return ok(
      { count, fresh: true },
      { headers: { 'Cache-Control': 'no-store, max-age=0' } },
    );
  } catch (error) {
    console.error('waitlist count error:', error);
    return ok(
      { count: Math.max(0, Number(process.env.WAITLIST_IMPORTED_COUNT ?? 485)), fresh: false },
      { headers: { 'Cache-Control': 'no-store, max-age=0' } },
    );
  }
}
