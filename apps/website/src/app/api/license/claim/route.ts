import { NextResponse } from 'next/server';

import { redeemClaimNonce } from '@/db/billing-queries';
import { HttpError, fail, readJsonObject } from '@/lib/http';
import { isValidClaimNonce, licenseKeyFingerprint } from '@/lib/license';
import { rateLimit } from '@/lib/rate-limit';
import { withTimeout } from '@/lib/timeout';
import { clientIp } from '@/lib/waitlist-auth';

export const runtime = 'nodejs';

/**
 * Deadline for the database reads this route waits on. A stalled connection is not a rejected
 * promise, so without this the Workers runtime kills the request and the route's own error
 * handling never runs — the caller gets a dead request instead of the 500 it is written to
 * return. A 500 matters here: the buying Mac polls this route, and a dead request
 * spends one of its attempts learning nothing.
 */
const DB_TIMEOUT_MS = 3_000;


/**
 * POST /api/license/claim — the Mac collects the licence it just paid for.
 *
 * Body: `{ nonce }`, the one-shot capability the app minted before opening Checkout and passed
 * through Stripe metadata. Answers `{ ok: true, license_key }` once, then never again.
 *
 * This exists so a buyer never handles their own licence key. The alternative — print the key on
 * the success page and have the human retype it into Settings — routes a Keychain-grade secret
 * through a browser window, a clipboard and a receipt email (CLAUDE.md 不変条件7), and asks for a
 * transcription at the least forgiving moment in the funnel.
 *
 * **Every miss looks the same.** Unknown nonce, already redeemed, expired, revoked licence, and
 * "the webhook has not landed yet" all answer `{ ok: true, pending: true }`. The client polls
 * until its own deadline and then offers manual entry, so it needs nothing more; telling those
 * apart out loud would turn this into an oracle for probing nonces. Guessing one is infeasible
 * regardless (the Mac mints 256 bits), which is what makes the capability safe to carry in
 * Stripe metadata at all.
 */
export async function POST(req: Request) {
  // Generous, because the legitimate client polls: a 15-minute wait at 5s intervals is ~180
  // requests. Per-IP rather than per-nonce — rate limiting the nonce would let an attacker buy
  // fresh budget with every fresh guess.
  const rl = await rateLimit('license_claim', clientIp(req), { limit: 120, windowSec: 60 });
  if (!rl.allowed) return fail('rate_limited');

  let body: Record<string, unknown>;
  try {
    body = await readJsonObject(req);
  } catch (e) {
    return fail(e instanceof HttpError ? e.code : 'bad_request');
  }

  const nonce = body.nonce;
  if (!isValidClaimNonce(nonce)) return fail('bad_request');

  const pending = () =>
    NextResponse.json({ ok: true, pending: true }, { headers: { 'Cache-Control': 'no-store' } });

  try {
    const licenseKey = await withTimeout(
      redeemClaimNonce(nonce, new Date()),
      DB_TIMEOUT_MS,
      'claim redemption',
    );
    if (!licenseKey) return pending();

    // Fingerprint only — the key itself never reaches a log line.
    console.info('license claimed', { key_fp: licenseKeyFingerprint(licenseKey) });
    return NextResponse.json(
      { ok: true, license_key: licenseKey },
      { headers: { 'Cache-Control': 'no-store' } },
    );
  } catch (e) {
    console.error('claim error:', e);
    return fail('server_error');
  }
}
