import type { Participant } from '@/db';
import {
  countQualifiedReferrals,
  ensureTokens,
  findByEmail,
  findByRefCode,
  findByStatusToken,
  insertParticipant,
  nextJoinPosition,
  updateParticipant,
} from '@/db/queries';
import {
  generateRefCode,
  generateStatusToken,
  isValidRefCode,
  sanitizeAnswer,
} from './referral';
import { award, normalizeHandle } from './points';

/**
 * Signup with referral attribution (REFERRAL_ENGINE.md §4.1).
 * Invalid or self-referring codes are DROPPED silently — a bad ?ref must
 * never make the signup itself fail.
 */
export async function addParticipant(
  email: string,
  ref?: string,
  ipHash?: string,
  xHandle?: unknown,
): Promise<{ row: Participant; duplicate: boolean }> {
  const normalized = email.trim().toLowerCase();

  const existing = await findByEmail(normalized);
  if (existing) {
    if (!existing.refCode || !existing.statusToken) {
      await ensureTokens(existing.id, generateRefCode(), generateStatusToken());
      const refreshed = await findByEmail(normalized);
      return { row: refreshed ?? existing, duplicate: true };
    }
    return { row: existing, duplicate: true }; // returning users still get a statusUrl
  }

  let referredBy: string | null = null;
  if (ref && isValidRefCode(ref)) {
    const referrer = await findByRefCode(ref);
    if (referrer && referrer.email !== normalized) referredBy = ref; // no self-referral
  }

  // X handle is optional; only holders are eligible for social points. A
  // duplicate handle (unique) must not break the entry, so drop it on clash.
  const handle = normalizeHandle(xHandle);
  const position = await nextJoinPosition();

  let row: Participant;
  try {
    row = await insertParticipant({
      email: normalized,
      refCode: generateRefCode(),
      statusToken: generateStatusToken(),
      referredBy,
      ipHash: ipHash ?? null,
      joinPosition: position,
      xHandle: handle,
    });
  } catch {
    // Most likely the x_handle unique clashed — retry without it.
    row = await insertParticipant({
      email: normalized,
      refCode: generateRefCode(),
      statusToken: generateStatusToken(),
      referredBy,
      ipHash: ipHash ?? null,
      joinPosition: position,
    });
  }
  return { row, duplicate: false };
}

/**
 * The qualifying action (REFERRAL_ENGINE.md §4.2). Bearer is the PRIVATE
 * status token — never the public ref code. `justQualified` fires exactly
 * once, on the transition to a complete profile.
 */
export async function submitProfile(
  statusToken: string,
  answers: { nickname?: unknown; a1?: unknown; a2?: unknown; a3?: unknown; xHandle?: unknown },
): Promise<{ row: Participant; justQualified: boolean } | null> {
  const row = await findByStatusToken(statusToken);
  if (!row) return null;

  // Optional X handle (only holders earn social points). Set once; ignore a
  // unique clash so a taken handle never fails the profile save.
  const handle = normalizeHandle(answers.xHandle);
  if (handle && handle !== row.xHandle) {
    try {
      await updateParticipant(row.id, { xHandle: handle });
    } catch {
      /* handle already taken — leave it unset */
    }
  }

  const nickname = sanitizeAnswer(answers.nickname)?.slice(0, 40) ?? null;
  const a1 = sanitizeAnswer(answers.a1);
  const a2 = sanitizeAnswer(answers.a2);
  const a3 = sanitizeAnswer(answers.a3);

  const merged = {
    a1: a1 ?? row.answer1,
    a2: a2 ?? row.answer2,
    a3: a3 ?? row.answer3,
  };
  const complete = !!(merged.a1 && merged.a2 && merged.a3);
  const justQualified = complete && !row.qualifiedAt; // fires exactly once

  await updateParticipant(row.id, {
    ...(nickname && { nickname }),
    ...(a1 && { answer1: a1 }),
    ...(a2 && { answer2: a2 }),
    ...(a3 && { answer3: a3 }),
    ...(justQualified && { qualifiedAt: new Date() }),
  });

  // On the one-shot transition to a complete profile:
  //   • the entry earns the +20 form award, and
  //   • any referrer's invite becomes "settled" → +100 (spec §3.3). Keyed by
  //     this entry's id, so a burner can't be re-counted for the same inviter.
  if (justQualified) {
    await award(row.id, 'form');
    if (row.referredBy) {
      const referrer = await findByRefCode(row.referredBy);
      if (referrer && referrer.id !== row.id) {
        await award(referrer.id, 'referral', row.id);
      }
    }
  }

  return {
    row: { ...row, ...merged, answer1: merged.a1, answer2: merged.a2, answer3: merged.a3 },
    justQualified,
  };
}

/** Count of answered profile questions (0–3) for the status payload. */
export function answeredCount(row: Participant): number {
  return [row.answer1, row.answer2, row.answer3].filter(Boolean).length;
}

export { countQualifiedReferrals };
