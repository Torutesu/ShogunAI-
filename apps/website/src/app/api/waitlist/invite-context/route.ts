import { NextResponse } from 'next/server';

export const runtime = 'nodejs';

/**
 * GET /api/waitlist/invite-context?ref=<refCode>
 * Retired with the referral program. Keeping the route as a 404 prevents old
 * links from exposing even masked participant details.
 */
export function GET() {
  return NextResponse.json({ ok: false, error: 'not_found' }, { status: 404, headers: { 'Cache-Control': 'no-store' } });
}
