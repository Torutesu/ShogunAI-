import { HttpError, fail, ok, readCappedBody, readJsonObject } from '@/lib/http';
import { isValidEmail } from '@/lib/referral';
import { addParticipant } from '@/lib/service';
import { clientIp, hashIp, isAuthorizedOrigin, isHoneypotTripped } from '@/lib/waitlist-auth';
import { consumeSignupAttempt, incrementParticipantCount, saveWaitlistEmail } from '@/lib/waitlist-metrics';

export const runtime = 'nodejs';

/** Parse either supported signup encoding under the shared public-body cap. */
export async function readSignupBody(req: Request): Promise<Record<string, unknown>> {
  const contentType = req.headers.get('content-type') ?? '';
  if (contentType.includes('application/x-www-form-urlencoded') || contentType.includes('multipart/form-data')) {
    // `req.formData()` buffers the whole body with no ceiling of its own, so the no-JS form
    // fallback used to sidestep the 8 KB cap the JSON path enforces: a multi-MB multipart POST
    // cleared the origin and rate-limit checks and then materialised in full inside the isolate.
    // Read through the shared capped reader first, then re-present those (≤ 8 KB) bytes to the
    // platform parser — same cap, same 413, and multipart still parses because the original
    // Content-Type (boundary included) rides along.
    const bytes = await readCappedBody(req);
    // Copy into a plain Uint8Array: BodyInit rejects Node's Buffer<ArrayBufferLike>
    // under TS 5.7's generic typed arrays, and the body is capped at 8 KB anyway.
    const form = await new Response(new Uint8Array(bytes), { headers: { 'content-type': contentType } }).formData();
    return {
      email: form.get('email'),
      company_url: form.get('company_url'),
    };
  }
  return readJsonObject(req);
}

/**
 * POST /api/waitlist/signup
 * Create a participant row for the email-only waitlist.
 * Auth: origin allowlist, rate limit, and honeypot.
 */
export async function POST(req: Request) {
  if (!isAuthorizedOrigin(req)) return fail('forbidden');

  const ip = clientIp(req);
  const ipHash = hashIp(ip);
  try {
    if (await consumeSignupAttempt(ipHash) > 5) return fail('rate_limited');
  } catch (error) {
    // Fail closed: never expose the signup endpoint without its abuse guard.
    console.error('signup rate-limit error:', error);
    return fail('server_error');
  }

  let body: Record<string, unknown>;
  try {
    body = await readSignupBody(req);
  } catch (e) {
    if (e instanceof HttpError) return fail(e.code);
    return fail('bad_request');
  }

  // Honeypot: a hidden field only bots fill. Silently accept-and-drop so we
  // don't reveal the trap, but never create a row.
  if (isHoneypotTripped(body)) return ok({ refCode: null, statusUrl: null });

  if (!isValidEmail(body.email)) return fail('bad_request');
  const email = String(body.email).trim().toLowerCase();
  try {
    // Cloudflare Workers can occasionally hang while opening the external
    // Postgres connection. Give it a short window, then capture the email in
    // private D1 so the public waitlist remains available.
    const result = await Promise.race([
      addParticipant(email, undefined, ipHash),
      new Promise<never>((_, reject) => setTimeout(() => reject(new Error('postgres signup timeout')), 4000)),
    ]);
    // Metrics must never make a successful email signup fail. D1 stores only
    // the public counter, while the email itself remains in Supabase.
    if (!result.duplicate) await incrementParticipantCount().catch((error) => console.error('count increment error:', error));
    // Return the same successful shape for new and duplicate emails. This
    // avoids turning the endpoint into an email-enumeration oracle.
    return ok({});
  } catch (e) {
    console.error('signup error; using D1 capture fallback:', e);
    try {
      const inserted = await saveWaitlistEmail(email);
      if (inserted) await incrementParticipantCount().catch((error) => console.error('count increment error:', error));
      return ok({});
    } catch (fallbackError) {
      console.error('D1 signup fallback error:', fallbackError);
      return fail('server_error');
    }
  }
}
