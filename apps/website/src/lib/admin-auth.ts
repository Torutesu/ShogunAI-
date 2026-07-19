import { timingSafeEqual } from 'node:crypto';

/**
 * Admin gate for the internal dashboard + X-sync trigger. Set ADMIN_TOKEN in
 * the environment; requests present it as `?key=` or `x-admin-token`. Constant
 * -time compare, and a missing/empty server token denies everything.
 */
export function isAdmin(req: Request): boolean {
  const expected = process.env.ADMIN_TOKEN ?? '';
  if (expected.length < 16) return false; // refuse to run with a weak/undefined token

  const url = new URL(req.url);
  const provided = req.headers.get('x-admin-token') ?? url.searchParams.get('key') ?? '';
  if (provided.length !== expected.length) return false;
  try {
    return timingSafeEqual(Buffer.from(provided), Buffer.from(expected));
  } catch {
    return false;
  }
}
