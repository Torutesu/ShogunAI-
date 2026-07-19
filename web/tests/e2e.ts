/**
 * End-to-end drive of the referral engine against real Postgres.
 * Exercises signup → attribution → qualifying action → count/rank/leaderboard,
 * plus the security-relevant edge cases (self-referral, invalid ref, two-token
 * split, IP-fraud signal). Run: `npm run e2e` (needs DATABASE_URL + migrate).
 */
import assert from 'node:assert/strict';
import { sql } from 'drizzle-orm';
import { client, db } from '../src/db/index.ts';
import {
  countQualifiedReferrals,
  distinctQualifiedIpHashes,
  leaderboard,
  leaderboardRank,
  queuePosition,
} from '../src/db/queries.ts';
import { addParticipant, submitProfile } from '../src/lib/service.ts';
import { hashIp } from '../src/lib/waitlist-auth.ts';
import { isValidStatusToken } from '../src/lib/referral.ts';

let passed = 0;
function ok(label: string, cond: boolean) {
  assert.ok(cond, label);
  passed++;
  console.log(`  ✓ ${label}`);
}

async function main() {
  // Fresh slate for deterministic assertions.
  await db.execute(sql`TRUNCATE participants, rate_limits`);

  // --- 1. Signup with no referral ---
  const a = await addParticipant('alice@example.com', undefined, hashIp('1.1.1.1'));
  ok('signup creates a row with both tokens', !!a.row.refCode && !!a.row.statusToken);
  ok('status token has private-bearer shape', isValidStatusToken(a.row.statusToken!));
  ok('public ref code is NOT a valid status token (two-token split)', !isValidStatusToken(a.row.refCode!));

  // --- 2. Duplicate signup is idempotent, no self-referral ---
  const dup = await addParticipant('alice@example.com', a.row.refCode!, hashIp('1.1.1.1'));
  ok('duplicate signup returns duplicate=true', dup.duplicate === true);
  ok('self-referral is dropped', dup.row.referredBy === null);

  // --- 3. Referred signups ---
  const bob = await addParticipant('bob@example.com', a.row.refCode!, hashIp('2.2.2.2'));
  ok('valid ref is attributed', bob.row.referredBy === a.row.refCode);

  const carol = await addParticipant('carol@example.com', 'not-a-real-code', hashIp('3.3.3.3'));
  ok('invalid ref is dropped silently at signup', carol.row.referredBy === null);

  // --- 4. Referral counts only AFTER the qualifying action ---
  ok('no qualified referrals before profile completion', (await countQualifiedReferrals(a.row.refCode!)) === 0);

  const partial = await submitProfile(bob.row.statusToken!, { a1: 'Founder', a2: '', a3: '' });
  ok('partial profile does not qualify', partial!.justQualified === false);
  ok('still zero qualified after partial', (await countQualifiedReferrals(a.row.refCode!)) === 0);

  const full = await submitProfile(bob.row.statusToken!, { a1: 'Founder', a2: 'code', a3: 'email' });
  ok('completing the profile fires justQualified once', full!.justQualified === true);
  ok('alice now has 1 qualified referral', (await countQualifiedReferrals(a.row.refCode!)) === 1);

  const again = await submitProfile(bob.row.statusToken!, { a1: 'Builder' });
  ok('re-submitting an already-qualified profile does not re-fire', again!.justQualified === false);
  ok('count stays at 1 (no double-count)', (await countQualifiedReferrals(a.row.refCode!)) === 1);

  // --- 5. Bearer must be the private token, not the public code ---
  const wrongBearer = await submitProfile(a.row.refCode!, { a1: 'x', a2: 'y', a3: 'z' });
  ok('public ref code cannot be used as a profile bearer', wrongBearer === null);

  // --- 6. Position, rank, leaderboard ---
  const posA = await queuePosition(a.row.refCode!);
  ok('alice has a queue position', !!posA && posA.position >= 1 && posA.total >= 3);

  const rankA = await leaderboardRank(a.row.refCode!);
  ok('alice is ranked on the board', rankA === 1);

  const board = await leaderboard(10);
  ok('leaderboard returns alice with count 1', board.some((r) => r.refCode === a.row.refCode && r.qualified === 1));
  ok('leaderboard never leaks raw un-referred rows', board.every((r) => r.qualified > 0));

  // --- 7. IP-fraud signal: quals from distinct IPs vs farmed ones ---
  const dave = await addParticipant('dave@example.com', a.row.refCode!, hashIp('2.2.2.2')); // same IP as bob
  await submitProfile(dave.row.statusToken!, { a1: 'Op', a2: 'x', a3: 'y' });
  ok('alice now has 2 qualified referrals', (await countQualifiedReferrals(a.row.refCode!)) === 2);
  const distinct = await distinctQualifiedIpHashes(a.row.refCode!);
  ok('distinct-IP-hash signal flags farming (2 quals, 1 distinct IP)', distinct === 1);

  console.log(`\nAll ${passed} e2e assertions passed against real Postgres.`);
  await client.end();
}

main().catch(async (err) => {
  console.error('\nE2E FAILED:', err);
  try {
    await client.end();
  } catch {}
  process.exit(1);
});
