import { createHash } from 'node:crypto';

/**
 * Public-POST protections for the waitlist (REFERRAL_ENGINE.md §6).
 * Origin allowlist, honeypot, trustworthy client-IP extraction, and a
 * salted IP hash for referral-fraud detection. No PII stored in the clear.
 */

// One-time production misconfiguration warnings (module scope = logged once
// per instance). Neither is fatal — the system degrades safely — but both
// weaken abuse protection, so make the gap visible in logs.
if (process.env.NODE_ENV === 'production') {
  if (!process.env.WAITLIST_IP_SALT) {
    console.warn('[waitlist] WAITLIST_IP_SALT is not set — IP hashes use the dev salt (crackable offline).');
  }
  if (!(process.env.WAITLIST_ALLOWED_ORIGINS ?? '').trim()) {
    console.warn('[waitlist] WAITLIST_ALLOWED_ORIGINS is not set — signup accepts any Origin.');
  }
}

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
 */
export function isAuthorizedOrigin(req: Request): boolean {
  const secret = process.env.WAITLIST_WEBHOOK_SECRET;
  if (secret && req.headers.get('x-webhook-secret') === secret) return true;

  const origin = req.headers.get('origin');
  const allow = allowedOrigins();
  if (allow.length === 0) return true; // not configured → don't hard-block (dev)
  return !!origin && allow.includes(origin);
}

/**
 * Client IP for rate limiting + fraud hashing. A client-settable value here
 * defeats BOTH controls, so we only read a header the edge is trusted to
 * overwrite:
 *   - On Cloudflare (our deploy target) `cf-connecting-ip` is set by the edge
 *     and any client-supplied copy is discarded — authoritative there.
 *   - On another platform, set WAITLIST_TRUSTED_IP_HEADER to the single header
 *     that platform guarantees (e.g. `x-real-ip`); we read exactly that.
 * We deliberately DO NOT parse X-Forwarded-For by default: without knowing the
 * proxy depth, no XFF hop is trustworthy, and picking one invites spoofing.
 */
export function clientIp(req: Request): string {
  const configured = process.env.WAITLIST_TRUSTED_IP_HEADER?.trim().toLowerCase();
  if (configured) {
    // Single-value trusted header only — take the first token defensively.
    const v = req.headers.get(configured);
    if (v) return v.split(',')[0].trim();
  }
  const cf = req.headers.get('cf-connecting-ip');
  if (cf) return cf.trim();
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
