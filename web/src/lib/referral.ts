import { randomBytes } from 'node:crypto';

/**
 * Core referral engine — pure, no DB dependency. Ports verbatim to any stack.
 * See REFERRAL_ENGINE.md §4.
 */

// --- Reward ladder. Rewards REPLACE each other; they never stack. ---
export const REFERRAL_TIERS = [
  { threshold: 3, reward: 1, label: '1 month free' },
  { threshold: 10, reward: 3, label: '3 months free' },
  { threshold: 30, reward: 6, label: '6 months free' },
] as const;
export type ReferralTier = (typeof REFERRAL_TIERS)[number];

export const TOP_REFERRER_COUNT = 10;
export const TOP_REFERRER_REWARD = { reward: 12, label: '1 year free' } as const;

// --- Tokens. Public code is short & pretty; private token is long. ---
export function generateRefCode(): string {
  return randomBytes(8).toString('base64url').slice(0, 10); // ~60 bits
}
export function generateStatusToken(): string {
  return randomBytes(24).toString('base64url'); // 192 bits
}
export function isValidRefCode(code: string): boolean {
  return /^[A-Za-z0-9_-]{6,16}$/.test(code);
}
export function isValidStatusToken(token: string): boolean {
  return /^[A-Za-z0-9_-]{20,64}$/.test(token);
}

// --- Ladder math. `count` = qualified referrals. ---
export function currentTier(count: number): ReferralTier | null {
  let tier: ReferralTier | null = null;
  for (const t of REFERRAL_TIERS) if (count >= t.threshold) tier = t;
  return tier; // null below the first rung
}
export function nextTier(count: number): ReferralTier | null {
  for (const t of REFERRAL_TIERS) if (count < t.threshold) return t;
  return null; // null at the top
}

/**
 * Email masking for the public leaderboard. Output is INERT even if a
 * frontend interpolates it unescaped (alphanumeric-only visible chars).
 */
export function maskEmail(email: string): string {
  const [local, domain] = email.split('@');
  const visible = local.slice(0, 2).replace(/[^A-Za-z0-9]/g, '*');
  const tldRaw = domain?.includes('.') ? domain.slice(domain.lastIndexOf('.') + 1) : '';
  const tld = tldRaw.replace(/[^A-Za-z0-9]/g, '');
  return `${visible}***@***${tld ? '.' + tld : ''}`;
}

// --- Free-text guard: trims, rejects empties/non-strings, caps length. ---
export const MAX_ANSWER_LENGTH = 1000;
export function sanitizeAnswer(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  const trimmed = value.trim();
  return trimmed ? trimmed.slice(0, MAX_ANSWER_LENGTH) : null;
}

// --- Strict email validation (spec §6.3). Rejects markup/formula/header chars. ---
const EMAIL_RE = /^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$/;
export const MAX_EMAIL_LENGTH = 254;
export function isValidEmail(value: unknown): value is string {
  return typeof value === 'string' && value.length <= MAX_EMAIL_LENGTH && EMAIL_RE.test(value);
}

/**
 * CSV formula-injection guard (spec §6.4). Prefix cells that begin with a
 * dangerous lead char with a single quote so spreadsheets treat them as text.
 */
export function csvSafeCell(value: string): string {
  return /^[=+\-@\t\r]/.test(value) ? `'${value}` : value;
}

/**
 * Compute the reward a referrer is entitled to from a qualified count.
 * Returns the top-referrer reward when `isTopReferrer`, else the ladder tier.
 */
export function rewardFor(count: number, isTopReferrer = false) {
  if (isTopReferrer) return TOP_REFERRER_REWARD;
  const tier = currentTier(count);
  return tier ? { reward: tier.reward, label: tier.label } : null;
}

/** Build a public share link from a ref code. */
export function shareUrl(origin: string, refCode: string): string {
  return `${origin.replace(/\/$/, '')}/?ref=${encodeURIComponent(refCode)}`;
}

/** Build the private status-page URL from a status token. */
export function statusUrl(origin: string, statusToken: string): string {
  return `${origin.replace(/\/$/, '')}/status?code=${encodeURIComponent(statusToken)}`;
}
