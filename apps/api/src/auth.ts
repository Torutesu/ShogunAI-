/**
 * License-token verification (docs/batch-relay-design.md §4.1).
 *
 * `Authorization: Bearer <license token>` — the FR-BIL-08 signed licence token, in the SAME
 * format the licence API issues and the device caches (`apps/website/src/lib/license.ts` /
 * `crates/shogun-license`):
 *
 *     v1.<base64url(payload JSON)>.<base64url(Ed25519 signature over the payload bytes)>
 *
 * One token format across the whole billing surface, one signing key, no second auth system.
 * Verified locally against the licence API's Ed25519 public key (the raw-32-bytes base64 that
 * `scripts/gen-license-keypair.mjs` prints); no round trip. Revocation rides on the token's
 * short (~24h) `exp`.
 */
import { createPublicKey, timingSafeEqual, verify as edVerify, type KeyObject } from "node:crypto";

/** Plans whose licence may use the Batch lane. The Batch lane ships in Standard (§6.12.1), so
 * every paid plan qualifies; anything else is 402. (The trial is a plan + `trialing` status,
 * not a plan of its own — see BATCH_STATUSES.) */
export const BATCH_PLANS = ["standard", "pro"] as const;

/** Subscription statuses the lane accepts: live, in-trial, or in the payment-retry window.
 * Cancelled / unpaid tokens are refused even while their `exp` has not passed. */
export const BATCH_STATUSES = ["active", "trialing", "past_due"] as const;

export interface License {
  /** The licence id (`lic` claim) — the metering key. An opaque id, never user content. */
  licenseId: string;
  plan: string;
}

export class AuthError extends Error {
  constructor(
    message: string,
    public readonly status: 401 | 402,
  ) {
    super(message);
    this.name = "AuthError";
  }
}

/** SPKI DER header for a raw Ed25519 public key (RFC 8410). */
const ED25519_SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");

/** Import the licence public key from the `SHOGUN_LICENSE_PUBKEY`-style value: base64 of the raw
 * 32-byte Ed25519 key (what `gen-license-keypair.mjs` prints). Throws on anything else — a relay
 * that cannot verify tokens must not boot. */
export function licensePublicKeyFromB64(b64: string): KeyObject {
  const raw = Buffer.from(b64.trim(), "base64");
  if (raw.length !== 32) {
    throw new Error("license public key must be base64 of 32 raw Ed25519 bytes");
  }
  return createPublicKey({
    key: Buffer.concat([ED25519_SPKI_PREFIX, raw]),
    format: "der",
    type: "spki",
  });
}

function b64urlDecode(part: string): Buffer | undefined {
  try {
    const buf = Buffer.from(part, "base64url");
    // Node decodes leniently; round-trip to reject garbage that silently truncated.
    if (buf.length === 0) return undefined;
    return buf;
  } catch {
    return undefined;
  }
}

/** Verify the Authorization header and return the licence, or throw an AuthError (401 for a
 * missing/malformed/forged/expired token, 402 for a plan or status the Batch lane does not
 * cover). `nowSecs` is injectable for tests; only `exp` is time-checked (no `iat` window —
 * clock skew between the licence API and the relay must not lock paying users out). */
export function verifyLicense(
  authorization: string | undefined,
  publicKey: KeyObject,
  nowSecs: number = Math.floor(Date.now() / 1000),
): License {
  if (!authorization || !authorization.startsWith("Bearer ")) {
    throw new AuthError("missing bearer license token", 401);
  }
  const token = authorization.slice("Bearer ".length).trim();
  // Deliberately unspecific error strings below: the device re-verifies its licence either way,
  // and the reason must not require logging token material to diagnose here.
  const invalid = new AuthError("invalid or expired license token", 401);
  const [version, bodyB64, sigB64, ...rest] = token.split(".");
  if (version !== "v1" || !bodyB64 || !sigB64 || rest.length > 0) throw invalid;
  const body = b64urlDecode(bodyB64);
  const sig = b64urlDecode(sigB64);
  if (!body || !sig || sig.length !== 64) throw invalid;
  // Reject any re-encoding mismatch (padding/whitespace smuggling) before the crypto.
  if (!timingSafeEqual(Buffer.from(body.toString("base64url")), Buffer.from(bodyB64))) throw invalid;
  if (!edVerify(null, body, publicKey, sig)) throw invalid;

  let payload: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(body.toString("utf8"));
    if (typeof parsed !== "object" || parsed === null) throw invalid;
    payload = parsed as Record<string, unknown>;
  } catch {
    throw invalid;
  }
  if (payload.v !== 1) throw invalid;
  const exp = payload.exp;
  if (typeof exp !== "number" || exp <= nowSecs) throw invalid;
  const licenseId = payload.lic;
  if (typeof licenseId !== "string" || licenseId.length === 0) {
    throw new AuthError("license token missing lic", 401);
  }
  const plan = payload.plan;
  if (typeof plan !== "string" || !(BATCH_PLANS as readonly string[]).includes(plan)) {
    throw new AuthError("plan does not include the Batch lane", 402);
  }
  const status = payload.status;
  if (typeof status !== "string" || !(BATCH_STATUSES as readonly string[]).includes(status)) {
    throw new AuthError("subscription status does not include the Batch lane", 402);
  }
  return { licenseId, plan };
}
