import { HttpError, fail, ok, readJsonObject } from '@/lib/http';
import { rateLimit } from '@/lib/rate-limit';
import { isValidEmail, statusUrl } from '@/lib/referral';
import { addParticipant } from '@/lib/service';
import { clientIp, hashIp, isAuthorizedOrigin, isHoneypotTripped } from '@/lib/waitlist-auth';

export const runtime = 'nodejs';

const APP_ORIGIN = process.env.NEXT_PUBLIC_APP_ORIGIN ?? 'http://localhost:3000';

/**
 * POST /api/waitlist/signup
 * Create a participant row, accept an optional `ref` code.
 * Auth: origin allowlist (+ rate limit + honeypot) OR webhook secret.
 * Returns { ok, refCode, statusUrl }.
 */
export async function POST(req: Request) {
  if (!isAuthorizedOrigin(req)) return fail('forbidden');

  const ip = clientIp(req);
  const rl = await rateLimit('signup', ip, { limit: 5, windowSec: 60 });
  if (!rl.allowed) return fail('rate_limited');

  let body: Record<string, unknown>;
  try {
    body = await readJsonObject(req);
  } catch (e) {
    if (e instanceof HttpError) return fail(e.code);
    return fail('bad_request');
  }

  // Honeypot: a hidden field only bots fill. Silently accept-and-drop so we
  // don't reveal the trap, but never create a row.
  if (isHoneypotTripped(body)) return ok({ refCode: null, statusUrl: null });

  if (!isValidEmail(body.email)) return fail('bad_request');
  const ref = typeof body.ref === 'string' ? body.ref : undefined;

  try {
    const { row, duplicate } = await addParticipant(body.email, ref, hashIp(ip), body.xHandle, body.plan);
    // SECURITY: never hand out the private status token for an EXISTING row —
    // otherwise anyone who knows a victim's email could take over their entry
    // (rewrite answers/nickname/handle). Only the creator of a brand-new row
    // gets the URL; returning users must reuse their original link.
    if (duplicate) return ok({ refCode: null, statusUrl: null, existing: true });
    return ok({ refCode: row.refCode, statusUrl: statusUrl(APP_ORIGIN, row.statusToken!) });
  } catch (e) {
    console.error('signup error:', e);
    return fail('server_error');
  }
}
