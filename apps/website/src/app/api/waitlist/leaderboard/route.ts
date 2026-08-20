import { NextResponse } from 'next/server';

export const runtime = 'nodejs';

/**
 * Retired with the referral program. Public nicknames and referral activity
 * are participant data and must not remain exposed on an email-only LP.
 */
export function GET() {
  return NextResponse.json({ ok: false, error: 'not_found' }, { status: 404, headers: { 'Cache-Control': 'no-store' } });
}
