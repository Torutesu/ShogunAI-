import { timingSafeEqual } from 'node:crypto';

const adminAttempts = new Map<string, { count: number; resetAt: number }>();

/** Lightweight edge-instance guard in addition to the token check. */
export function adminRateLimited(req: Request): boolean {
  const key = req.headers.get('cf-connecting-ip') ?? req.headers.get('x-real-ip') ?? 'unknown';
  const now = Date.now();
  const current = adminAttempts.get(key);
  if (!current || current.resetAt <= now) {
    adminAttempts.set(key, { count: 1, resetAt: now + 60_000 });
    return false;
  }
  current.count += 1;
  return current.count > 20;
}

/**
 * Admin gate for the internal dashboard + X-sync trigger. Set ADMIN_TOKEN in
 * the environment; requests present it in the `x-admin-token` header ONLY
 * (a query-string token leaks into access logs / browser history). Constant
 * -time compare, and a missing/empty server token denies everything.
 */
export function isAdmin(req: Request): boolean {
  const expected = process.env.ADMIN_TOKEN ?? '';
  if (expected.length < 16) return false; // refuse to run with a weak/undefined token

  const provided = req.headers.get('x-admin-token') ?? '';
  if (provided.length !== expected.length) return false;
  try {
    return timingSafeEqual(Buffer.from(provided), Buffer.from(expected));
  } catch {
    return false;
  }
}
