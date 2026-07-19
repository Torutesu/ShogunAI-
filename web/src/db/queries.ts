import { and, desc, eq, isNotNull, sql } from 'drizzle-orm';
import { db } from './index';
import { type NewParticipant, type Participant, participants } from './schema';

/**
 * Data layer for the referral engine. The SQL here is the real logic;
 * the ORM is cosmetic (REFERRAL_ENGINE.md §9). All values are bound
 * parameters — no string concatenation into queries.
 */

export async function findByEmail(email: string): Promise<Participant | undefined> {
  const [row] = await db.select().from(participants).where(eq(participants.email, email)).limit(1);
  return row;
}

export async function findByRefCode(refCode: string): Promise<Participant | undefined> {
  const [row] = await db.select().from(participants).where(eq(participants.refCode, refCode)).limit(1);
  return row;
}

export async function findByStatusToken(token: string): Promise<Participant | undefined> {
  const [row] = await db
    .select()
    .from(participants)
    .where(eq(participants.statusToken, token))
    .limit(1);
  return row;
}

export async function insertParticipant(values: NewParticipant): Promise<Participant> {
  const [row] = await db.insert(participants).values(values).returning();
  return row;
}

export async function updateParticipant(
  id: string,
  values: Partial<NewParticipant>,
): Promise<void> {
  await db.update(participants).set(values).where(eq(participants.id, id));
}

/** Backfill tokens for a legacy row that predates the referral engine. */
export async function ensureTokens(
  id: string,
  refCode: string,
  statusToken: string,
): Promise<void> {
  await db
    .update(participants)
    .set({ refCode, statusToken })
    .where(and(eq(participants.id, id), sql`${participants.refCode} IS NULL`));
}

/** Qualified referral count for one public code (REFERRAL_ENGINE.md §4.3). */
export async function countQualifiedReferrals(refCode: string): Promise<number> {
  const [row] = await db
    .select({ n: sql<number>`count(*)::int` })
    .from(participants)
    .where(and(eq(participants.referredBy, refCode), isNotNull(participants.qualifiedAt)));
  return row?.n ?? 0;
}

/**
 * Queue position: rank by qualified refs, then answered-questions, then
 * signup time. Both referring and answering move you up.
 */
export async function queuePosition(
  refCode: string,
): Promise<{ position: number; total: number } | null> {
  const rows = await db.execute<{ pos: number; total: number }>(sql`
    WITH scored AS (
      SELECT p.ref_code, p.created_at,
        COALESCE(r.qualified, 0) AS qualified,
        ((p.answer_1 IS NOT NULL)::int + (p.answer_2 IS NOT NULL)::int
         + (p.answer_3 IS NOT NULL)::int) AS answers
      FROM participants p
      LEFT JOIN (
        SELECT referred_by, count(*)::int AS qualified
        FROM participants
        WHERE referred_by IS NOT NULL AND qualified_at IS NOT NULL
        GROUP BY referred_by
      ) r ON r.referred_by = p.ref_code
      WHERE p.status = 'pending'
    ),
    ranked AS (
      SELECT ref_code,
        row_number() OVER (ORDER BY qualified DESC, answers DESC, created_at ASC) AS pos,
        count(*) OVER () AS total
      FROM scored
    )
    SELECT pos::int AS pos, total::int AS total FROM ranked WHERE ref_code = ${refCode}
  `);
  const row = rows[0];
  return row ? { position: Number(row.pos), total: Number(row.total) } : null;
}

/** Top-N leaderboard. Clamp `limit` to a sane max in the caller. */
export async function leaderboard(
  limit: number,
): Promise<Array<{ email: string; refCode: string; qualified: number }>> {
  const rows = await db.execute<{ email: string; ref_code: string; qualified: number }>(sql`
    SELECT p.email, p.ref_code, r.qualified
    FROM participants p
    JOIN (
      SELECT referred_by, count(*)::int AS qualified
      FROM participants
      WHERE referred_by IS NOT NULL AND qualified_at IS NOT NULL
      GROUP BY referred_by
    ) r ON r.referred_by = p.ref_code
    ORDER BY r.qualified DESC, p.created_at ASC
    LIMIT ${limit}
  `);
  return rows.map((r) => ({ email: r.email, refCode: r.ref_code, qualified: Number(r.qualified) }));
}

/**
 * Distinct signup-IP hashes among a referrer's qualified invites. Used at
 * reward time to catch farming (many quals from one IP = one person).
 */
export async function distinctQualifiedIpHashes(refCode: string): Promise<number> {
  const [row] = await db
    .select({ n: sql<number>`count(DISTINCT ${participants.ipHash})::int` })
    .from(participants)
    .where(and(eq(participants.referredBy, refCode), isNotNull(participants.qualifiedAt)));
  return row?.n ?? 0;
}

/** Rank on the leaderboard (1-based) or null if not yet on the board. */
export async function leaderboardRank(refCode: string): Promise<number | null> {
  const rows = await db.execute<{ rank: number }>(sql`
    WITH board AS (
      SELECT referred_by AS ref_code, count(*)::int AS qualified
      FROM participants
      WHERE referred_by IS NOT NULL AND qualified_at IS NOT NULL
      GROUP BY referred_by
    ),
    ranked AS (
      SELECT ref_code, row_number() OVER (ORDER BY qualified DESC) AS rank
      FROM board
    )
    SELECT rank::int AS rank FROM ranked WHERE ref_code = ${refCode}
  `);
  return rows[0] ? Number(rows[0].rank) : null;
}

export { desc };
