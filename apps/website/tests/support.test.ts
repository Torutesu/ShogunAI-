import assert from 'node:assert/strict';
import { test } from 'node:test';

import { isValidEmail } from '../src/lib/referral.ts';
import {
  MAX_MESSAGE_CHARS,
  MIN_MESSAGE_CHARS,
  isSupportCategory,
  isSupportStatus,
  parseSupportReport,
} from '../src/lib/support.ts';

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
