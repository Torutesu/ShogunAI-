import { createSupportTicket } from '@/db/support-queries';
import { HttpError, fail, ok, readJsonObject } from '@/lib/http';
import { rateLimit } from '@/lib/rate-limit';
import { isValidEmail } from '@/lib/referral';
import { parseSupportReport } from '@/lib/support';
import { resolveNotifyConfig, sendSupportNotification } from '@/lib/support-notify';
import { withTimeout } from '@/lib/timeout';
import { clientIp, hashIp } from '@/lib/waitlist-auth';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

/** Deadline for the insert — same reasoning as license/verify: a stalled connection is not a
 *  rejected promise, and the desktop app deserves a real error it can show. */
const DB_TIMEOUT_MS = 3_000;

/**
 * POST /api/support/report — the CS / bug-report intake (support窓口).
 *
 * Request:  { category, message, email?, app_version?, os_version?, plan? }
 * Response: { ok, ticket_id }
 *
 * Called by the desktop app's Help & Support panel. The body carries only what the reporter
 * typed plus the opt-in diagnostics tuple — no capture content, no memory content, no licence
 * key (the app enforces that; this endpoint enforces shape and size). No origin allowlist:
 * like /api/license/verify, the caller is a native app, not a browser. Abuse control is the
 * IP rate limit plus the 8 KB body cap.
 */
export async function POST(req: Request) {
  const ip = clientIp(req);
  const rl = await rateLimit('support-report', ip, { limit: 5, windowSec: 3600 });
  if (!rl.allowed) return fail('rate_limited');

  let body: Record<string, unknown>;
  try {
    body = await readJsonObject(req);
  } catch (e) {
    if (e instanceof HttpError) return fail(e.code);
    return fail('bad_request');
  }

  const parsed = parseSupportReport(body, isValidEmail);
  if ('error' in parsed) return fail('bad_request', { field: parsed.error });

  try {
    const ticketId = await withTimeout(
      createSupportTicket(parsed.report, 'desktop', hashIp(ip)),
      DB_TIMEOUT_MS,
      'support ticket insert',
    );

    // Notify the operator's inbox. Deliberately after the insert and deliberately non-fatal:
    // the report is already saved, so a mail outage must not answer the reporter with an error
    // that invites them to send it again. `null` config = notifications switched off (no key),
    // which is the normal state in local development and preview deploys.
    const notify = resolveNotifyConfig(process.env);
    if (notify) await sendSupportNotification(parsed.report, ticketId, notify);

    return ok({ ticket_id: ticketId });
  } catch (e) {
    console.error('support report insert error:', e);
    return fail('server_error');
  }
}
