import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  csvSafeCell,
  currentTier,
  generateRefCode,
  generateStatusToken,
  isValidEmail,
  isValidRefCode,
  isValidStatusToken,
  maskEmail,
  nextTier,
  rewardFor,
  sanitizeAnswer,
} from '../src/lib/referral.ts';

test('token shapes are distinct and non-interchangeable', () => {
  const ref = generateRefCode();
  const tok = generateStatusToken();
  assert.ok(isValidRefCode(ref), 'ref code valid');
  assert.ok(isValidStatusToken(tok), 'status token valid');
  // The two-token invariant: a public code can never pass as a status token.
  assert.equal(isValidStatusToken(ref), false, 'ref code must fail status-token regex');
  assert.equal(isValidRefCode(tok), false, 'status token is too long for ref-code regex');
});

test('ladder replaces, never stacks', () => {
  assert.equal(currentTier(0), null);
  assert.equal(currentTier(2), null);
  assert.equal(currentTier(3)?.reward, 1);
  assert.equal(currentTier(9)?.reward, 1);
  assert.equal(currentTier(10)?.reward, 3);
  assert.equal(currentTier(30)?.reward, 6);
  assert.equal(currentTier(999)?.reward, 6);
  // 3 + 10 !== 13: reaching 10 supersedes the 3-tier.
  assert.equal(rewardFor(13)?.reward, 3);
});

test('nextTier points at the following rung', () => {
  assert.equal(nextTier(0)?.threshold, 3);
  assert.equal(nextTier(3)?.threshold, 10);
  assert.equal(nextTier(30), null);
});

test('top-referrer reward overrides the ladder', () => {
  assert.equal(rewardFor(5, true).reward, 12);
});

test('maskEmail output is inert (alphanumeric visible chars only)', () => {
  assert.equal(maskEmail('alice@example.com'), 'al***@***.com');
  // Injection attempt in local part is stripped to asterisks.
  const masked = maskEmail('a<b@example.com');
  assert.ok(!masked.includes('<'), 'no markup survives masking');
});

test('email validation rejects markup/formula/header chars', () => {
  assert.ok(isValidEmail('good@example.com'));
  assert.equal(isValidEmail('bad<script>@x.com'), false);
  assert.equal(isValidEmail('a@b'), false);
  assert.equal(isValidEmail('no-at-sign'), false);
  assert.equal(isValidEmail(42), false);
});

test('sanitizeAnswer trims, caps, rejects empties/non-strings', () => {
  assert.equal(sanitizeAnswer('  hi  '), 'hi');
  assert.equal(sanitizeAnswer('   '), null);
  assert.equal(sanitizeAnswer(123), null);
  assert.equal(sanitizeAnswer('x'.repeat(5000))?.length, 1000);
});

test('csvSafeCell neutralizes formula-injection leads', () => {
  assert.equal(csvSafeCell('=1+1'), "'=1+1");
  assert.equal(csvSafeCell('+cmd'), "'+cmd");
  assert.equal(csvSafeCell('@x'), "'@x");
  assert.equal(csvSafeCell('normal'), 'normal');
});
