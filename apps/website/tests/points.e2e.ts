/**
 * End-to-end drive of the points/gamification engine against real Postgres.
 * Covers: join_position, optional x_handle, form(+20) + referral settlement
 * (+100 keyed by referred id), idempotency, social awards from seeded
 * snapshots (follow/quote gates), and ranking order. Run: `npm run e2e:points`.
 */
import assert from 'node:assert/strict';
import { sql } from 'drizzle-orm';
import { client, db } from '../src/db/index.ts';
import { addParticipant, submitProfile } from '../src/lib/service.ts';
import {
  X_CONFIG,
  award,
  computeSocialAwards,
  pointsBreakdown,
  pointsLeaderboard,
  rankOf,
  totalPoints,
} from '../src/lib/points.ts';
import { ingestFollowers, ingestQuotes } from '../src/lib/xsource.ts';

let passed = 0;
function ok(label: string, cond: boolean) {
  assert.ok(cond, label);
  passed++;
  console.log(`  ✓ ${label}`);
}

async function complete(statusToken: string, nick: string) {
  return submitProfile(statusToken, { nickname: nick, a1: 'why', a2: 'co', a3: 'challenge' });
}

async function main() {
  await db.execute(sql`TRUNCATE participants, rate_limits, points_ledger, x_follower_snapshot, x_quote_snapshot`);

  // --- 1. Entry: join_position + optional x_handle ---
  const a = await addParticipant('a@ex.com', undefined, 'ip1', '@Alice');
  const b = await addParticipant('b@ex.com', a.row.refCode!, 'ip2', 'bob');
  ok('join_position is sequential', a.row.joinPosition === 1 && b.row.joinPosition === 2);
  ok('x_handle normalized (lowercase, no @)', a.row.xHandle === 'alice' && b.row.xHandle === 'bob');

  const dupHandle = await addParticipant('c@ex.com', undefined, 'ip3', 'alice'); // clashes with a
  ok('duplicate x_handle does not break entry (dropped)', !!dupHandle.row.refCode && dupHandle.row.xHandle == null);

  // --- 2. Form completion: +20 form, and referral settles to A (+100) ---
  await complete(b.row.statusToken!, 'bob');
  ok('B earns +20 form', (await totalPoints(b.row.id)) === 20);
  ok('A earns +100 when referred B completes the form', (await totalPoints(a.row.id)) === 100);

  // Idempotency: completing again must not double-award.
  await complete(b.row.statusToken!, 'bob');
  ok('form award is idempotent', (await totalPoints(b.row.id)) === 20);
  ok('referral settlement is idempotent', (await totalPoints(a.row.id)) === 100);

  // A second referred entry that completes → A gets another +100 (distinct source_ref).
  const d = await addParticipant('d@ex.com', a.row.refCode!, 'ip4', 'dave');
  await complete(d.row.statusToken!, 'dave');
  ok('a second settled referral stacks (+100 each, keyed by referee)', (await totalPoints(a.row.id)) === 200);

  // --- 3. Social awards from seeded snapshots ---
  // A follows both product + founder; posts a valid quote. B follows product only.
  await ingestFollowers(X_CONFIG.product, ['alice', 'bob', 'someone_else']);
  await ingestFollowers(X_CONFIG.founder, ['alice']);
  await ingestQuotes(X_CONFIG.launchTweetId || 'launch', [
    { authorHandle: 'alice', quoteTweetId: 'q1', text: 'love this, been waiting #ad' }, // valid → +30
    { authorHandle: 'bob', quoteTweetId: 'q2', text: 'nice #ad' }, // too short (<10) → no
    { authorHandle: 'dave', quoteTweetId: 'q3', text: 'this looks really promising' }, // no disclosure → no
  ]);
  // launchTweetId is required for quote awards to be looked at:
  if (!X_CONFIG.launchTweetId) process.env.X_LAUNCH_TWEET_ID = 'launch';

  const written = await computeSocialAwards();
  ok('computeSocialAwards wrote new awards', written > 0);

  const aBreak = await pointsBreakdown(a.row.id);
  ok('A: follow_product +10', aBreak.follow_product === 10);
  ok('A: follow_founder +10', aBreak.follow_founder === 10);
  ok('A: valid quote +30', aBreak.quote === 30);
  ok('A total = 200 referral + 10 + 10 + 30 = 250', (await totalPoints(a.row.id)) === 250);

  const bBreak = await pointsBreakdown(b.row.id);
  ok('B: follow_product +10', bBreak.follow_product === 10);
  ok('B: short quote is NOT awarded', bBreak.quote === undefined);
  ok('B: not a founder follower → no follow_founder', bBreak.follow_founder === undefined);

  ok('D: quote without #ad/#pr is NOT awarded', (await pointsBreakdown(d.row.id)).quote === undefined);

  // Re-running the social worker is idempotent.
  const written2 = await computeSocialAwards();
  ok('social worker is idempotent on re-run', written2 === 0);

  // --- 4. Ranking: SUM(points) desc, join_position asc ---
  const rankA = await rankOf(a.row.id); // 250 → #1
  ok('A is rank #1', rankA?.rank === 1);
  ok('totalWaiting counts all entries', rankA?.total === 4);

  const board = await pointsLeaderboard(3);
  ok('leaderboard top row is A (250pts), by refCode', board[0]?.ref_code === a.row.refCode && board[0]?.points === 250);
  ok('leaderboard is points-descending', board[0]!.points >= board[1]!.points && board[1]!.points >= board[2]!.points);
  ok('leaderboard never exposes email', (board[0] as { email?: string }).email === undefined);

  // Manual award path is also idempotent.
  const first = await award(d.row.id, 'form');
  ok('award() returns false when the row already exists', first === false);

  console.log(`\n✅ points e2e: ${passed} assertions passed`);
  await client.end();
}

main().catch(async (err) => {
  console.error('\n❌ points e2e failed:', err);
  await client.end();
  process.exit(1);
});
