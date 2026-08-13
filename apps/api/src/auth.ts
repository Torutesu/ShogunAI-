/**
 * License-token verification (docs/batch-relay-design.md §4.1).
 *
 * `Authorization: Bearer <license JWT>` — the FR-BIL-08 signed license token, verified locally
 * (ES256) against the license API's public key. No new auth system, no round trip; revocation
 * rides on the token's short (24h) expiry. jose enforces `exp` during `jwtVerify`.
 */
import { jwtVerify, type KeyLike } from "jose";

/** Plans whose license may use the Batch lane. The Batch lane ships in Standard (§6.12.1), so
 * every paid plan (and the Pro-equivalent trial) qualifies; anything else is 402. */
export const BATCH_PLANS = ["standard", "pro", "trial"] as const;
export type BatchPlan = (typeof BATCH_PLANS)[number];

export interface License {
  /** The license id (`sub` claim) — the metering key. An opaque id, never user content. */
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

/** Verify the Authorization header and return the license, or throw an AuthError (401 for a
 * missing/invalid/expired token, 402 for a plan the Batch lane does not cover). */
export async function verifyLicense(
  authorization: string | undefined,
  publicKey: KeyLike,
): Promise<License> {
  if (!authorization || !authorization.startsWith("Bearer ")) {
    throw new AuthError("missing bearer license token", 401);
  }
  const token = authorization.slice("Bearer ".length).trim();
  let payload: Record<string, unknown>;
  try {
    const verified = await jwtVerify(token, publicKey, { algorithms: ["ES256"] });
    payload = verified.payload;
  } catch {
    // Deliberately unspecific: the caller re-verifies its license either way, and the reason
    // (bad signature vs expired) must not require logging token material to diagnose here.
    throw new AuthError("invalid or expired license token", 401);
  }
  const licenseId = payload.sub;
  if (typeof licenseId !== "string" || licenseId.length === 0) {
    throw new AuthError("license token missing sub", 401);
  }
  const plan = payload.plan;
  if (typeof plan !== "string" || !(BATCH_PLANS as readonly string[]).includes(plan)) {
    throw new AuthError("plan does not include the Batch lane", 402);
  }
  return { licenseId, plan };
}
