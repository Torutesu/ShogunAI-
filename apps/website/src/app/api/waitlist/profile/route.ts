import { HttpError, fail, ok, readJsonObject } from '@/lib/http';
import { rateLimit } from '@/lib/rate-limit';
import { isValidStatusToken } from '@/lib/referral';
import { submitProfile } from '@/lib/service';
import { clientIp } from '@/lib/waitlist-auth';

export const runtime = 'nodejs';

/**
 * POST /api/waitlist/profile
 * Save profile answers; may set qualified_at (the qualifying action).
 * Auth: the PRIVATE status token in the body — never the public ref code.
 * Returns { ok, qualified: bool, justQualified: bool }.
 */
export async function POST(req: Request) {
  const rl = await rateLimit('profile', clientIp(req), { limit: 20, windowSec: 60 });
  if (!rl.allowed) return fail('rate_limited');

  let body: Record<string, unknown>;
  try {
    body = await readJsonObject(req);
  } catch (e) {
    if (e instanceof HttpError) return fail(e.code);
    return fail('bad_request');
  }

  const token = typeof body.code === 'string' ? body.code : '';
  // Shape-check before any DB hit: a short public code fails this regex, so
  // it can never be used as a bearer (two-token split, §6.1).
  if (!isValidStatusToken(token)) return fail('bad_request');

  const result = await submitProfile(token, {
    nickname: body.nickname,
    a1: body.a1,
    a2: body.a2,
    a3: body.a3,
  });
  if (!result) return fail('not_found');

  const qualified = result.justQualified || !!result.row.qualifiedAt;
  return ok({ qualified, justQualified: result.justQualified });
}
