import assert from 'node:assert/strict';
import { test } from 'node:test';

import { isValidEmail } from '../src/lib/referral.ts';
import {
  MAX_MESSAGE_CHARS,
  MIN_MESSAGE_CHARS,
  isSupportCategory,
  isSupportStatus,
  parseSupportReport,
  type SupportReport,
} from '../src/lib/support.ts';
import {
  DEFAULT_NOTIFY_FROM,
  DEFAULT_NOTIFY_TO,
  buildNotificationPayload,
  resolveNotifyConfig,
} from '../src/lib/support-notify.ts';

/**
 * CS / bug-report intake, pure layer. Everything the endpoint decides about a body is
 * decided here, so this is where malformed input has to be proven out.
 */

const valid = {
  category: 'bug',
  message: 'The notch panel stops expanding after sleep.',
  email: 'user@example.com',
  app_version: '1.2.3',
  os_version: '14.5',
  plan: 'pro',
};

test('a full valid body parses with every field carried through', () => {
  const r = parseSupportReport(valid, isValidEmail);
  assert.ok('report' in r);
  assert.equal(r.report.category, 'bug');
  assert.equal(r.report.email, 'user@example.com');
  assert.equal(r.report.appVersion, '1.2.3');
  assert.equal(r.report.osVersion, '14.5');
  assert.equal(r.report.plan, 'pro');
});

test('a minimal body (category + message) parses with nulls elsewhere', () => {
  const r = parseSupportReport({ category: 'question', message: 'How do I export?' }, isValidEmail);
  assert.ok('report' in r);
  assert.equal(r.report.email, null);
  assert.equal(r.report.appVersion, null);
  assert.equal(r.report.osVersion, null);
  assert.equal(r.report.plan, null);
});

test('unknown category is rejected as category', () => {
  const r = parseSupportReport({ ...valid, category: 'praise' }, isValidEmail);
  assert.deepEqual(r, { error: 'category' });
});

test('message bounds are enforced on the trimmed text', () => {
  const short = parseSupportReport(
    { ...valid, message: ` ${'x'.repeat(MIN_MESSAGE_CHARS - 1)} ` },
    isValidEmail,
  );
  assert.deepEqual(short, { error: 'message' });
  const long = parseSupportReport(
    { ...valid, message: 'x'.repeat(MAX_MESSAGE_CHARS + 1) },
    isValidEmail,
  );
  assert.deepEqual(long, { error: 'message' });
  const exact = parseSupportReport(
    { ...valid, message: 'x'.repeat(MAX_MESSAGE_CHARS) },
    isValidEmail,
  );
  assert.ok('report' in exact);
});

test('a non-string message is rejected, not coerced', () => {
  const r = parseSupportReport({ ...valid, message: 42 }, isValidEmail);
  assert.deepEqual(r, { error: 'message' });
});

test('empty-string email means "none", an invalid one is rejected', () => {
  const none = parseSupportReport({ ...valid, email: '' }, isValidEmail);
  assert.ok('report' in none && none.report.email === null);
  const bad = parseSupportReport({ ...valid, email: 'not-an-email' }, isValidEmail);
  assert.deepEqual(bad, { error: 'email' });
});

test('email is normalised to trimmed lowercase', () => {
  const r = parseSupportReport({ ...valid, email: 'User@Example.COM' }, isValidEmail);
  assert.ok('report' in r);
  assert.equal(r.report.email, 'user@example.com');
});

test('diagnostics fields reject non-strings, oversize and control characters', () => {
  assert.deepEqual(parseSupportReport({ ...valid, app_version: 7 }, isValidEmail), {
    error: 'app_version',
  });
  assert.deepEqual(
    parseSupportReport({ ...valid, os_version: 'v'.repeat(65) }, isValidEmail),
    { error: 'os_version' },
  );
  assert.deepEqual(parseSupportReport({ ...valid, plan: 'pro\nadmin' }, isValidEmail), {
    error: 'plan',
  });
});

test('empty diagnostics strings collapse to null rather than storing ""', () => {
  const r = parseSupportReport({ ...valid, app_version: '  ' }, isValidEmail);
  assert.ok('report' in r);
  assert.equal(r.report.appVersion, null);
});

test('the status vocabulary is closed', () => {
  assert.ok(isSupportStatus('open') && isSupportStatus('triaged') && isSupportStatus('resolved'));
  assert.ok(!isSupportStatus('closed') && !isSupportStatus('') && !isSupportStatus(undefined));
});

test('the category vocabulary is closed', () => {
  assert.ok(isSupportCategory('bug') && isSupportCategory('feedback') && isSupportCategory('question'));
  assert.ok(!isSupportCategory('BUG') && !isSupportCategory(null));
});

/**
 * Email notification. The send itself is a network call, so what is proven here is the payload
 * and the on/off switch — the two things that decide whether a report reaches a human.
 */

const report: SupportReport = {
  category: 'bug',
  message: 'The notch panel stops expanding after sleep.',
  email: 'user@example.com',
  appVersion: '1.2.3',
  osVersion: '14.5',
  plan: 'pro',
};
const config = { apiKey: 'k', to: DEFAULT_NOTIFY_TO, from: DEFAULT_NOTIFY_FROM };

test('notifications are off without an API key and on with one', () => {
  assert.equal(resolveNotifyConfig({}), null);
  assert.equal(resolveNotifyConfig({ RESEND_API_KEY: '   ' }), null);
  const on = resolveNotifyConfig({ RESEND_API_KEY: 'secret' });
  assert.deepEqual(on, { apiKey: 'secret', to: DEFAULT_NOTIFY_TO, from: DEFAULT_NOTIFY_FROM });
});

test('the default destination is the shared inbox on the live domain', () => {
  assert.equal(DEFAULT_NOTIFY_TO, 'info@shogunaios.com');
  assert.equal(DEFAULT_NOTIFY_FROM, 'info@shogunaios.com');
});

test('env overrides the destination without touching the key', () => {
  const on = resolveNotifyConfig({
    RESEND_API_KEY: 'secret',
    SUPPORT_NOTIFY_TO: 'staging@example.com',
    SUPPORT_NOTIFY_FROM: 'noreply@example.com',
  });
  assert.equal(on?.to, 'staging@example.com');
  assert.equal(on?.from, 'noreply@example.com');
});

test('the payload carries ticket, category, diagnostics and the body', () => {
  const p = buildNotificationPayload(report, 'tkt-1', config);
  assert.deepEqual(p.to, [DEFAULT_NOTIFY_TO]);
  assert.equal(p.from, DEFAULT_NOTIFY_FROM);
  assert.match(String(p.subject), /^\[bug\] /);
  assert.match(String(p.text), /tkt-1/);
  assert.match(String(p.text), /app 1\.2\.3 · macOS 14\.5 · plan pro/);
  assert.match(String(p.text), /notch panel stops expanding/);
});

test('reply_to is the reporter when given, and absent otherwise', () => {
  assert.equal(buildNotificationPayload(report, 't', config).reply_to, 'user@example.com');
  const anon = buildNotificationPayload({ ...report, email: null }, 't', config);
  assert.equal('reply_to' in anon, false);
  assert.match(String(anon.text), /\(none given\)/);
});

test('undisclosed diagnostics read as not shared rather than as blanks', () => {
  const bare = buildNotificationPayload(
    { ...report, appVersion: null, osVersion: null, plan: null },
    't',
    config,
  );
  assert.match(String(bare.text), /Diagnostics: \(not shared\)/);
});

test('reporter text cannot inject markup into the HTML part', () => {
  const nasty = buildNotificationPayload(
    { ...report, message: '<script>alert(1)</script> & "quoted"' },
    't',
    config,
  );
  assert.equal(String(nasty.html).includes('<script>'), false);
  assert.match(String(nasty.html), /&lt;script&gt;/);
});

test('a newline in the reporter email cannot forge headers', () => {
  const forged = buildNotificationPayload(
    { ...report, email: 'a@b.co\nBcc: victim@x.com' },
    't',
    config,
  );
  assert.equal(String(forged.reply_to).includes('\n'), false);
});

test('the subject is a single line and stays short for long reports', () => {
  const long = buildNotificationPayload(
    { ...report, message: 'x'.repeat(500) },
    't',
    config,
  );
  const subject = String(long.subject);
  assert.equal(subject.includes('\n'), false);
  assert.ok(subject.length < 90, `subject too long: ${subject.length}`);
  assert.ok(subject.endsWith('…'));
});
