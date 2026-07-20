import { timingSafeEqual } from 'node:crypto';

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
