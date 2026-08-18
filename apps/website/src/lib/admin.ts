import { sql } from 'drizzle-orm';
import { db } from '@/db';
import { referralFarmingSuspects } from '@/db/queries';
import { TIERS, TOP_REFERRER_COUNT, pointsLeaderboard } from './points';

/**
 * Reward liability estimate (spec §5 — the $500K cap is a MAXIMUM, not a
 * promise). Values are Pro-annual-rate months, USD. FX / actual participation
 * make the real figure lower, so this is an internal upper-bound helper.
 */
const PRO_MONTH_USD = 99; // Pro billed annually ≈ $99/mo
const REWARD_MONTHS: Record<number, number> = { 300: 1, 1000: 3, 3000: 6 };
const TOP_REWARD_MONTHS = 12; // top-10 → 1 year
export const CAMPAIGN_CAP_USD = 500_000;

function tierMonths(points: number): number {
  let months = 0;
  for (const t of TIERS) if (points >= t.points) months = REWARD_MONTHS[t.points] ?? months;
  return months;
}

export type AdminStats = {
  totalEntries: number;
  formCompleted: number;
  withXHandle: number;
  pointsByAction: Record<string, number>;
  tierCounts: { none: number; t300: number; t1000: number; t3000: number };
  estLiabilityUsd: number;
  capUsd: number;
  top: Array<{ id: string; nickname: string | null; ref_code: string | null; points: number }>;
  farmingSuspects: Array<{ refCode: string; qualified: number; distinctIps: number }>;
};

export async function adminStats(): Promise<AdminStats> {
  // Per-entry points + flags (one pass; waitlist scale).
  const rows = (await db.execute(sql`
    SELECT p.id,
           (p.qualified_at IS NOT NULL) AS formed,
           (p.x_handle IS NOT NULL) AS has_handle,
           COALESCE(l.total, 0)::int AS points
    FROM participants p
    LEFT JOIN (SELECT entry_id, SUM(points) AS total FROM points_ledger GROUP BY entry_id) l
           ON l.entry_id = p.id
    ORDER BY points DESC
  `)) as unknown as Array<{ id: string; formed: boolean; has_handle: boolean; points: number }>;

  const byAction = (await db.execute(sql`
    SELECT action_type, SUM(points)::int AS total FROM points_ledger GROUP BY action_type
  `)) as unknown as Array<{ action_type: string; total: number }>;

  const tierCounts = { none: 0, t300: 0, t1000: 0, t3000: 0 };
  let liability = 0;
  rows.forEach((r, i) => {
    const p = Number(r.points);
    if (p >= 3000) tierCounts.t3000++;
    else if (p >= 1000) tierCounts.t1000++;
    else if (p >= 300) tierCounts.t300++;
    else tierCounts.none++;
    // top-N by points get the 1-year reward (rows are points-desc ordered)
    const months = i < TOP_REFERRER_COUNT && p > 0 ? Math.max(TOP_REWARD_MONTHS, tierMonths(p)) : tierMonths(p);
    liability += months * PRO_MONTH_USD;
  });

  return {
    totalEntries: rows.length,
    formCompleted: rows.filter((r) => r.formed).length,
    withXHandle: rows.filter((r) => r.has_handle).length,
    pointsByAction: Object.fromEntries(byAction.map((r) => [r.action_type, Number(r.total)])),
    tierCounts,
    estLiabilityUsd: Math.min(liability, CAMPAIGN_CAP_USD),
    capUsd: CAMPAIGN_CAP_USD,
    top: await pointsLeaderboard(20),
    farmingSuspects: await referralFarmingSuspects(),
  };
}
