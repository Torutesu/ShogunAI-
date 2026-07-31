import { createHash } from 'node:crypto';

/**
 * Public-POST protections for the waitlist (REFERRAL_ENGINE.md §6).
 * Origin allowlist, honeypot, trustworthy client-IP extraction, and a
 * salted IP hash for referral-fraud detection. No PII stored in the clear.
 */

function allowedOrigins(): string[] {
  return (process.env.WAITLIST_ALLOWED_ORIGINS ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
}

/**
 * Accept the request if it carries a valid server webhook secret, OR its
 * Origin is on the allowlist. Browser POSTs always send Origin; server
 * callers use the shared secret instead.
 *
 * FAIL CLOSED: a missing Origin header is denied, and an empty/unset
 * allowlist falls back to SAME-ORIGIN only (Origin host === request host) —
 * never to allow-all. `next dev` posts same-origin from localhost, so local
 * dev works with no env; production must set WAITLIST_ALLOWED_ORIGINS.
 */
export function isAuthorizedOrigin(req: Request): boolean {
  const secret = process.env.WAITLIST_WEBHOOK_SECRET;
  if (secret && req.headers.get('x-webhook-secret') === secret) return true;

  const origin = req.headers.get('origin');
  if (!origin) return false; // no Origin and no secret → deny

  const allow = allowedOrigins();
  if (allow.length > 0) return allow.includes(origin);

  // Not configured → same-origin only. Host-level compare (not scheme) so a
  // TLS-terminating proxy doesn't break it; explicit allowlist still wins.
  try {
    return new URL(origin).host === new URL(req.url).host;
  } catch {
    return false;
  }
}

/**
 * Client IP. Behind Cloudflare read CF-Connecting-IP (unspoofable at the
 * edge); never trust the first X-Forwarded-For hop, which the client sets.
 * Falls back to the last XFF hop (closest proxy) then to a sentinel.
 */
export function clientIp(req: Request): string {
  const cf = req.headers.get('cf-connecting-ip');
  if (cf) return cf.trim();
  const xff = req.headers.get('x-forwarded-for');
  if (xff) {
    const hops = xff.split(',').map((s) => s.trim()).filter(Boolean);
    if (hops.length) return hops[hops.length - 1];
  }
  return req.headers.get('x-real-ip')?.trim() || '0.0.0.0';
}

/** Salted, one-way hash of a signup IP. Stored instead of the raw address. */
export function hashIp(ip: string): string {
  const salt = process.env.WAITLIST_IP_SALT ?? 'dev-salt';
  return createHash('sha256').update(`${salt}:${ip}`).digest('base64url').slice(0, 22);
}

/**
 * Honeypot: a hidden form field real users never fill. A non-empty value
 * means a bot — reject. The field name should look plausible (e.g. company).
 */
export function isHoneypotTripped(body: Record<string, unknown>, field = 'company_url'): boolean {
  const v = body[field];
  return typeof v === 'string' && v.trim().length > 0;
}
