/**
 * Licence keys and signed licence tokens (issue #8 / FR-BIL-08).
 *
 * Two different credentials, on purpose:
 *
 * - **Licence key** (`shogun-XXXX-XXXX-XXXX-XXXX`): the bearer the desktop app presents to
 *   `/api/license/verify`. Secret. Lives in the macOS Keychain on the device (NFR-SEC-01) and in
 *   our DB. Shown to the human exactly once, on the checkout success page.
 * - **Licence token**: an Ed25519-signed, short-lived, device-bound assertion of "this device is
 *   on plan X until Y". NOT a secret — it grants nothing on another machine (the device id is
 *   inside the signed payload) and it expires. The app caches it so a Mac that is offline keeps
 *   working for 14 days (FR-BIL-09).
 *
 * Signing happens here with `node:crypto` (Ed25519 is native — no dependency). Verification
 * happens in the Rust core (`crates/shogun-agents/src/license.rs`) against an embedded public
 * key, so a tampered token cannot unlock the app.
 *
 * The token carries no capture content, no memory content and no email — only what the plan
 * gate needs (NFR-PRV-04 / FR-BIL-08: "検証リクエストにキャプチャ内容・メモリ内容を一切含めない").
 */

import { createHash, createPrivateKey, randomBytes, sign, timingSafeEqual } from 'node:crypto';

import type { PlanId } from './pricing';
import type { SubscriptionStatus } from './billing';

/** How long a freshly issued token is considered current. The app re-verifies every 24h. */
export const TOKEN_TTL_SECONDS = 26 * 3600;

/** Offline grace measured from issuance (FR-BIL-09). The Rust core enforces the same number. */
export const OFFLINE_GRACE_DAYS = 14;

const KEY_GROUPS = 4;
const KEY_GROUP_LEN = 4;
/** Crockford-ish alphabet: no I/L/O/U, so a key read off a screen cannot be mistyped into another. */
const KEY_ALPHABET = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';

/** Mint a licence key: `shogun-XXXX-XXXX-XXXX-XXXX`, 80 bits of CSPRNG entropy. */
export function generateLicenseKey(): string {
  const bytes = randomBytes(KEY_GROUPS * KEY_GROUP_LEN);
  const chars = Array.from(bytes, (b) => KEY_ALPHABET[b % KEY_ALPHABET.length]);
  const groups: string[] = [];
  for (let i = 0; i < KEY_GROUPS; i += 1) {
    groups.push(chars.slice(i * KEY_GROUP_LEN, (i + 1) * KEY_GROUP_LEN).join(''));
  }
  return `shogun-${groups.join('-')}`;
}

const KEY_RE = new RegExp(`^shogun-([${KEY_ALPHABET}]{${KEY_GROUP_LEN}}-){${KEY_GROUPS - 1}}[${KEY_ALPHABET}]{${KEY_GROUP_LEN}}$`);

/**
 * Canonical storage/lookup form: `shogun-` + upper-case body. Tolerates what humans actually
 * paste — surrounding space, lower case, and the unicode dashes a mail client substitutes for
 * the hyphens.
 */
export function canonicalLicenseKey(raw: string): string {
  const body = raw
    .trim()
    .replace(/[‐-―−]/g, '-')
    .replace(/\s+/g, '')
    .replace(/^shogun-/i, '');
  return `shogun-${body.toUpperCase()}`;
}

export function isValidLicenseKey(raw: unknown): raw is string {
  return typeof raw === 'string' && raw.length <= 128 && KEY_RE.test(canonicalLicenseKey(raw));
}

/** A device id is an opaque anonymous identifier minted on the Mac. Cap it and keep it opaque. */
export function isValidDeviceId(raw: unknown): raw is string {
  return typeof raw === 'string' && /^[A-Za-z0-9_-]{8,64}$/.test(raw);
}

/** Constant-time compare for secrets that reached us as strings. */
export function safeEqual(a: string, b: string): boolean {
  const ab = Buffer.from(a, 'utf8');
  const bb = Buffer.from(b, 'utf8');
  if (ab.length !== bb.length) return false;
  return timingSafeEqual(ab, bb);
}

/** Short, non-reversible fingerprint of a key — safe to put in logs and analytics. */
export function licenseKeyFingerprint(key: string): string {
  return createHash('sha256').update(canonicalLicenseKey(key)).digest('hex').slice(0, 12);
}

/** The signed payload. Field names are short because this string is stored on the device. */
export interface LicenseTokenPayload {
  /** Format version. Bump only for breaking changes; the Rust verifier rejects unknown versions. */
  v: 1;
  /** Licence id (our DB id) — NOT the licence key. The key never leaves the device again. */
  lic: string;
  plan: PlanId;
  status: SubscriptionStatus;
  /** The device this token is bound to. A copy on another Mac fails verification. */
  device: string;
  /** Issued at / expires at, unix seconds. */
  iat: number;
  exp: number;
  /** Subscription period end, unix seconds — what the app shows as "next billing date". */
  period_end: number | null;
  /** Whether the subscription is set to stop at the period end (portal cancellation). */
  cancel_at_period_end: boolean;
  /** Offline grace budget in days, measured from `iat`. */
  grace_days: number;
}

function b64url(buf: Buffer): string {
  return buf.toString('base64url');
}

/**
 * The private signing key, as a base64-encoded PKCS#8 PEM in `LICENSE_SIGNING_KEY`
 * (base64 so it survives one-line environment variables). Generate a pair with
 * `node scripts/gen-license-keypair.mjs`.
 */
export function signingKeyConfigured(): boolean {
  return !!process.env.LICENSE_SIGNING_KEY?.trim();
}

function privateKey() {
  const raw = process.env.LICENSE_SIGNING_KEY?.trim();
  if (!raw) throw new Error('LICENSE_SIGNING_KEY is not set');
  const pem = raw.includes('BEGIN') ? raw : Buffer.from(raw, 'base64').toString('utf8');
  const key = createPrivateKey(pem);
  if (key.asymmetricKeyType !== 'ed25519') {
    throw new Error(`LICENSE_SIGNING_KEY must be an ed25519 key, got ${key.asymmetricKeyType}`);
  }
  return key;
}

/**
 * Serialise + sign: `v1.<base64url(payload)>.<base64url(signature)>`.
 * The signature covers the exact payload bytes that are transmitted, so the verifier never has
 * to re-serialise JSON (canonicalisation bugs are how signature schemes die).
 */
export function signLicenseToken(payload: LicenseTokenPayload): string {
  const body = Buffer.from(JSON.stringify(payload), 'utf8');
  const sig = sign(null, body, privateKey());
  return `v1.${b64url(body)}.${b64url(sig)}`;
}

/** Build the payload for a device from the current subscription view. */
export function buildTokenPayload(args: {
  licenseId: string;
  plan: PlanId;
  status: SubscriptionStatus;
  deviceId: string;
  periodEnd: number | null;
  cancelAtPeriodEnd: boolean;
  nowMs: number;
}): LicenseTokenPayload {
  const iat = Math.floor(args.nowMs / 1000);
  return {
    v: 1,
    lic: args.licenseId,
    plan: args.plan,
    status: args.status,
    device: args.deviceId,
    iat,
    exp: iat + TOKEN_TTL_SECONDS,
    period_end: args.periodEnd,
    cancel_at_period_end: args.cancelAtPeriodEnd,
    grace_days: OFFLINE_GRACE_DAYS,
  };
}
