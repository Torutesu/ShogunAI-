import { HttpError, fail, ok, readJsonObject } from '@/lib/http';
import { rateLimit } from '@/lib/rate-limit';
import { isValidEmail, signupPayload } from '@/lib/referral';
import { addParticipant } from '@/lib/service';
import { clientIp, hashIp, isAuthorizedOrigin, isHoneypotTripped } from '@/lib/waitlist-auth';
import { getPostHogClient } from '@/lib/posthog-server';

export const runtime = 'nodejs';

const APP_ORIGIN = process.env.NEXT_PUBLIC_APP_ORIGIN ?? 'http://localhost:3000';

/**
 * POST /api/waitlist/signup
 * Create a participant row, accept an optional `ref` code.
 * Auth: origin allowlist (+ rate limit + honeypot) OR webhook secret.
 * Returns { ok, refCode, statusUrl } — both null for duplicate signups, which
 * must never echo the existing row's private statusToken (§6.8).
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
    const { row, duplicate } = await addParticipant(body.email, ref, hashIp(ip));

    // Analytics only for a FRESH signup: a duplicate is indistinguishable from the honeypot
    // path in the response, and must not emit an identify/capture for a row the caller does
    // not own (see docs/fixes/2026-07-30-waitlist-security-fix.md).
    const posthog = getPostHogClient();
    if (posthog && !duplicate && row.refCode) {
      posthog.identify({ distinctId: row.refCode });
      posthog.capture({
        distinctId: row.refCode,
        event: 'waitlist_signed_up',
        properties: { has_ref_code: !!ref },
      });
      await posthog.flush();
    }

    // Duplicates get the same generic success as the honeypot path: the
    // owner keeps their original status link (tokens are NOT rotated), and
    // a third party who knows the email learns nothing they can use.
    return ok(signupPayload(row, duplicate, APP_ORIGIN));
  } catch (e) {
    console.error('signup error:', e);
    return fail('server_error');
  }
}
