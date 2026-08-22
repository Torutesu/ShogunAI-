import type { SupportReport } from './support';

/**
 * Support-ticket email notification (CS窓口).
 *
 * A ticket that only lands in Postgres is one nobody sees until somebody remembers to poll the
 * admin endpoint. This turns each intake into a message in the operator's inbox.
 *
 * Sent over Resend's HTTP API rather than SMTP: this runs on Cloudflare Workers, which has no
 * raw socket path for mail, so an HTTP-based provider is the only shape that works here.
 *
 * The notification is **best effort and never load-bearing**. By the time it runs the ticket is
 * already committed, so a mail failure must not turn a saved report into an error for the user —
 * every failure path here logs and returns instead of throwing.
 */

/**
 * Where notifications go.
 *
 * A mailbox that actually receives, on purpose. `info@shogunaios.com` is the address the site
 * publishes, but that domain has no MX record — mail to it is a black hole, and a notification
 * nobody receives is worse than none at all because it looks like it works. Point this at the
 * operator's real inbox until `shogunaios.com` can accept mail, then move it back.
 */
export const DEFAULT_NOTIFY_TO = 'selectdev111@gmail.com';
/**
 * Envelope sender: Resend's shared sandbox address, which needs no DNS work at all.
 *
 * The trade-off it carries: the sandbox sender only delivers to the address the Resend account
 * was registered with, so it works *only* while [`DEFAULT_NOTIFY_TO`] is that same address.
 * That is the deal being taken here — day-one delivery with nothing to configure, instead of a
 * correct-looking setup that silently sends nowhere.
 *
 * Upgrade path, once `shogunaios.com` has SPF/DKIM verified in Resend: set both
 * `SUPPORT_NOTIFY_FROM` and `SUPPORT_NOTIFY_TO` to `info@shogunaios.com` and the restriction
 * lifts — no code change, the env vars already override both.
 */
export const DEFAULT_NOTIFY_FROM = 'onboarding@resend.dev';

const RESEND_ENDPOINT = 'https://api.resend.com/emails';

/** Subject prefix per category, so the inbox sorts itself without opening anything. */
const SUBJECT_TAG: Record<string, string> = {
  bug: '[bug]',
  feedback: '[feedback]',
  question: '[question]',
};

export type NotifyConfig = {
  apiKey: string;
  to: string;
  from: string;
};

/**
 * Resolve the notification config from the environment, or `null` when notifications are
 * switched off (no API key). `null` is a supported state, not an error: local development and
 * preview deploys run without mail and must still accept reports.
 */
export function resolveNotifyConfig(env: Record<string, string | undefined>): NotifyConfig | null {
  const apiKey = env.RESEND_API_KEY?.trim();
  if (!apiKey) return null;
  return {
    apiKey,
    to: env.SUPPORT_NOTIFY_TO?.trim() || DEFAULT_NOTIFY_TO,
    from: env.SUPPORT_NOTIFY_FROM?.trim() || DEFAULT_NOTIFY_FROM,
  };
}

/** Escape text for the HTML part. The body is reporter-authored, so it is never trusted markup. */
function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/**
 * Header-injection guard for anything interpolated into an address or subject field. A newline
 * in one of those is how an attacker adds their own headers, and `message` reaches the subject
 * line's neighbourhood by way of the ticket id only — but `email` is reporter-supplied and does
 * end up in Reply-To.
 */
function singleLine(value: string): string {
  return value.replace(/[\r\n]+/g, ' ').trim();
}

/** The Resend request body for one ticket. Pure, so the shape is testable without network. */
export function buildNotificationPayload(
  report: SupportReport,
  ticketId: string,
  config: NotifyConfig,
): Record<string, unknown> {
  const tag = SUBJECT_TAG[report.category] ?? '[support]';
  const preview = singleLine(report.message).slice(0, 60);
  const diagnostics = [
    report.appVersion ? `app ${report.appVersion}` : null,
    report.osVersion ? `macOS ${report.osVersion}` : null,
    report.plan ? `plan ${report.plan}` : null,
  ].filter(Boolean);

  const lines = [
    `Ticket: ${ticketId}`,
    `Category: ${report.category}`,
    `Reply-to: ${report.email ?? '(none given)'}`,
    `Diagnostics: ${diagnostics.length ? diagnostics.join(' · ') : '(not shared)'}`,
    '',
    report.message,
  ];

  const payload: Record<string, unknown> = {
    from: config.from,
    to: [config.to],
    subject: `${tag} ${preview}${preview.length < singleLine(report.message).length ? '…' : ''}`,
    text: lines.join('\n'),
    html:
      `<p><strong>Ticket</strong> ${escapeHtml(ticketId)}<br>` +
      `<strong>Category</strong> ${escapeHtml(report.category)}<br>` +
      `<strong>Reply-to</strong> ${escapeHtml(report.email ?? '(none given)')}<br>` +
      `<strong>Diagnostics</strong> ${escapeHtml(diagnostics.length ? diagnostics.join(' · ') : '(not shared)')}</p>` +
      `<pre style="white-space:pre-wrap;font-family:inherit">${escapeHtml(report.message)}</pre>`,
  };

  // Replying in the mail client should reach the reporter, not the shared inbox. Only set when
  // they actually gave an address — Resend rejects a malformed reply_to outright.
  if (report.email) payload.reply_to = singleLine(report.email);

  return payload;
}

/**
 * Send the notification. Resolves to `true` when the provider accepted it, `false` otherwise —
 * never throws, and never propagates a provider outage to the reporter.
 */
export async function sendSupportNotification(
  report: SupportReport,
  ticketId: string,
  config: NotifyConfig,
  timeoutMs = 5_000,
): Promise<boolean> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const res = await fetch(RESEND_ENDPOINT, {
      method: 'POST',
      headers: {
        authorization: `Bearer ${config.apiKey}`,
        'content-type': 'application/json',
      },
      body: JSON.stringify(buildNotificationPayload(report, ticketId, config)),
      signal: controller.signal,
    });
    if (!res.ok) {
      // Status only: the body can echo the address list, and this log is not the place for it.
      console.error(`support notification rejected: HTTP ${res.status}`);
      return false;
    }
    return true;
  } catch (e) {
    console.error('support notification failed:', e);
    return false;
  } finally {
    clearTimeout(timer);
  }
}
