/**
 * The pure layer of the CS / bug-report intake (support窓口): vocabulary and validation,
 * no DB, no network. The route handlers stay thin so this part stays provable in unit
 * tests the way billing's pure layer is.
 */

/** The closed category set. UI pickers and the API enforce the same list. */
export const SUPPORT_CATEGORIES = ['bug', 'feedback', 'question'] as const;
export type SupportCategory = (typeof SUPPORT_CATEGORIES)[number];

/** Triage lifecycle (docs/support-runbook.md). Terminal state is 'resolved'. */
export const SUPPORT_STATUSES = ['open', 'triaged', 'resolved'] as const;
export type SupportStatus = (typeof SUPPORT_STATUSES)[number];

/** Where a report can enter from. 'web' is reserved for a future site form. */
export const SUPPORT_SOURCES = ['desktop', 'web'] as const;
export type SupportSource = (typeof SUPPORT_SOURCES)[number];

/**
 * Free-text ceiling. The transport already caps the whole body at 8 KB
 * (MAX_BODY_BYTES), so this is about keeping single rows reviewable, not safety.
 */
export const MAX_MESSAGE_CHARS = 4000;
/** Floor that filters empty and single-character noise without blocking terse reports. */
export const MIN_MESSAGE_CHARS = 5;
/** Version strings and plan names are short identifiers; anything longer is garbage. */
export const MAX_META_CHARS = 64;

export function isSupportCategory(value: unknown): value is SupportCategory {
  return typeof value === 'string' && (SUPPORT_CATEGORIES as readonly string[]).includes(value);
}

export function isSupportStatus(value: unknown): value is SupportStatus {
  return typeof value === 'string' && (SUPPORT_STATUSES as readonly string[]).includes(value);
}

/** A validated report, ready to insert. */
export type SupportReport = {
  category: SupportCategory;
  message: string;
  email: string | null;
  appVersion: string | null;
  osVersion: string | null;
  plan: string | null;
};

/**
 * A short optional identifier field: absent/empty → null, non-string or oversized → undefined
 * (rejected). Distinguishing "not sent" from "sent but wrong" keeps the endpoint strict
 * without punishing an app build that simply has nothing to report for a field.
 */
function optionalMeta(value: unknown): string | null | undefined {
  if (value === undefined || value === null) return null;
  if (typeof value !== 'string') return undefined;
  const trimmed = value.trim();
  if (!trimmed) return null;
  if (trimmed.length > MAX_META_CHARS) return undefined;
  // Identifiers only — a control character here is a smuggling attempt, not a version string.
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f]/.test(trimmed)) return undefined;
  return trimmed;
}

/**
 * Validate a raw intake body into a SupportReport, or return the field that failed.
 * `emailValid` is injected so this module stays dependency-free of the referral lib.
 */
export function parseSupportReport(
  body: Record<string, unknown>,
  emailValid: (value: unknown) => boolean,
): { report: SupportReport } | { error: string } {
  if (!isSupportCategory(body.category)) return { error: 'category' };

  if (typeof body.message !== 'string') return { error: 'message' };
  const message = body.message.trim();
  if (message.length < MIN_MESSAGE_CHARS || message.length > MAX_MESSAGE_CHARS) {
    return { error: 'message' };
  }

  let email: string | null = null;
  if (body.email !== undefined && body.email !== null && body.email !== '') {
    if (!emailValid(body.email)) return { error: 'email' };
    email = String(body.email).trim().toLowerCase();
  }

  const appVersion = optionalMeta(body.app_version);
  const osVersion = optionalMeta(body.os_version);
  const plan = optionalMeta(body.plan);
  if (appVersion === undefined) return { error: 'app_version' };
  if (osVersion === undefined) return { error: 'os_version' };
  if (plan === undefined) return { error: 'plan' };

  return {
    report: { category: body.category, message, email, appVersion, osVersion, plan },
  };
}
