import { findByRefCode } from '@/db/queries';
import { currentTier, isValidRefCode, maskEmail } from '@/lib/referral';
import { countQualifiedReferrals } from '@/lib/service';
import { NextResponse } from 'next/server';

export const runtime = 'nodejs';

/**
 * GET /api/waitlist/invite-context?ref=<refCode>
 * Public. Personalizes the landing page for an invited visitor.
 * Returns only masked identity + tier — never the inviter's email/token.
 */
export async function GET(req: Request) {
  const ref = new URL(req.url).searchParams.get('ref') ?? '';
  if (!isValidRefCode(ref)) {
    return NextResponse.json({ ok: true, valid: false });
  }

  const inviter = await findByRefCode(ref);
  if (!inviter) {
    return NextResponse.json({ ok: true, valid: false });
  }

  const count = await countQualifiedReferrals(ref);
  const tier = currentTier(count);

  return NextResponse.json(
    {
      ok: true,
      valid: true,
      inviter: maskEmail(inviter.email),
      tier: tier ? { reward: tier.reward, label: tier.label } : null,
    },
    { headers: { 'Cache-Control': 'public, max-age=60' } },
  );
}
