'use client';

import { ArrowRight, Check, Copy, Gift, Loader2, Share2, Star, Trophy } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';

type Status = {
  status: string;
  nickname: string | null;
  refCode: string;
  shareUrl: string;
  qualifiedReferrals: number;
  position: number | null;
  totalWaiting: number | null;
  answered: number;
  profileComplete: boolean;
  tier: { reward: number; label: string; threshold: number } | null;
  nextTier: { reward: number; label: string; threshold: number; remaining: number } | null;
  leaderboardRank: number | null;
  isTopReferrer: boolean;
};

type LbRow = { rank: number; name: string; refCode: string; count: number };

const TIERS = [
  { threshold: 3, label: '1 month free' },
  { threshold: 10, label: '3 months free' },
  { threshold: 30, label: '6 months free' },
];

export function StatusDashboard({ code }: { code: string }) {
  const [data, setData] = useState<Status | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const res = await fetch(`/api/waitlist/status?code=${encodeURIComponent(code)}`);
      const d = await res.json();
      if (!res.ok || !d.ok) return setError(d?.error === 'not_found' ? 'not_found' : 'bad');
      setData(d);
    } catch {
      setError('network');
    }
  }, [code]);

  useEffect(() => {
    load();
  }, [load]);

  if (error === 'not_found')
    return (
      <Card className="mx-auto grid max-w-md gap-4 text-center">
        <h1 className="font-display text-2xl font-semibold">We couldn’t find that page</h1>
        <p className="text-muted">This link is invalid or expired. Check the URL from your welcome email.</p>
        <Button asChild className="justify-self-center">
          <a href="/">Back to home</a>
        </Button>
      </Card>
    );
  if (error) return <Card className="mx-auto max-w-md text-center text-muted">Something went wrong. Please refresh.</Card>;
  if (!data)
    return (
      <div className="grid place-items-center py-16">
        <Loader2 className="size-5 animate-spin text-accent" />
      </div>
    );

  return data.profileComplete ? (
    <Tracking data={data} code={code} />
  ) : (
    <Onboarding code={code} onDone={load} />
  );
}

type Rank = {
  points: number;
  rank: number | null;
  totalWaiting: number | null;
  joinPosition: number | null;
  breakdown: Record<string, number>;
  tier: { points: number; reward: string } | null;
  nextTier: { points: number; reward: string; remaining: number } | null;
  isTopReferrer: boolean;
};

const PTS_TIERS = [
  { points: 300, reward: '1 month free' },
  { points: 1000, reward: '3 months free' },
  { points: 3000, reward: '6 months free' },
];

// Ways to climb — point values mirror lib/points.ts POINTS.
const CLIMB: { key: string; label: string; points: number; hint: string }[] = [
  { key: 'referral', label: 'Invite a friend', points: 100, hint: 'counts once they finish their profile' },
  { key: 'quote', label: 'Quote-post the launch', points: 30, hint: 'must add a comment + #ad (paid-promotion disclosure)' },
  { key: 'follow_product', label: 'Follow ShogunAI on X', points: 10, hint: 'one-time' },
  { key: 'follow_founder', label: 'Follow the founder on X', points: 10, hint: 'one-time' },
];

// Legal fine print (spec §5): action-based (not a sweepstakes), capped value.
const LEGAL =
  'Rewards are earned by action — no purchase necessary and no prize draw. Amounts shown are the maximum, in Pro annual-rate terms; the actual value varies with participation and exchange rate. Up to $500,000 total across the campaign.';

function FinePrint() {
  return <p className="mt-3 text-[11px] leading-relaxed text-faint">{LEGAL}</p>;
}

/* ---------- Step 1: thank-you + program + rewards + form ---------- */
function Onboarding({ code, onDone }: { code: string; onDone: () => void }) {
  return (
    <div className="mx-auto grid max-w-2xl gap-6">
      <div className="text-center">
        <Badge dot>You’re on the waitlist</Badge>
        <h1 className="mt-4 font-display text-[clamp(30px,4vw,44px)] font-semibold tracking-[-0.02em]">
          Thanks for signing up
        </h1>
        <p className="mx-auto mt-3 max-w-[48ch] text-[17px] leading-relaxed text-muted">
          You’re on the list for ShogunAI. Want to skip ahead? We run a referral program — invite friends,
          climb the line, and earn months of ShogunAI free. Up to <span className="font-semibold text-ink">$500,000</span>{' '}
          in ShogunAI (max) is set aside for early believers — earned by action, not chance.
        </p>
      </div>

      {/* Program + rewards pitch */}
      <Card>
        <div className="mb-1 flex items-center gap-2">
          <Gift className="size-4 text-accent" />
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">How the referral program works</p>
        </div>
        <p className="text-sm text-muted">
          Every friend who joins with your link and completes their profile counts as one referral. The more
          you refer, the more you unlock — rewards replace each other, so you always hold your best one.
        </p>
        <div className="mt-4 grid gap-2.5">
          {TIERS.map((t) => (
            <Rung key={t.threshold} badge={String(t.threshold)} label={t.label} meta={`${t.threshold} referrals`} />
          ))}
          <Rung
            badge={<Star className="size-3.5" />}
            label="Top 10 referrers — 1 year free"
            meta="leaderboard"
          />
        </div>
        <FinePrint />
      </Card>

      {/* Interested? → form that issues the link */}
      <ProfileForm code={code} onDone={onDone} />
    </div>
  );
}

/* ---------- Step 2: your link + points + rank gamification ---------- */
function Tracking({ data, code }: { data: Status; code: string }) {
  const [copied, setCopied] = useState(false);
  const [rank, setRank] = useState<Rank | null>(null);

  useEffect(() => {
    let alive = true;
    fetch(`/api/waitlist/rank?code=${encodeURIComponent(code)}`)
      .then((r) => r.json())
      .then((d) => alive && d.ok && setRank(d))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [code]);

  async function copyShare() {
    try {
      await navigator.clipboard.writeText(data.shareUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable */
    }
  }

  const points = rank?.points ?? 0;
  const next = rank?.nextTier ?? null;
  const prevPts = rank?.tier?.points ?? 0;
  const segProgress = next ? Math.min(1, (points - prevPts) / (next.points - prevPts)) : 1;
  const referralCount = Math.round((rank?.breakdown.referral ?? 0) / 100);

  return (
    <div className="mx-auto grid max-w-3xl gap-6">
      <div className="text-center">
        <Badge dot>Your spot is secured</Badge>
        <h1 className="mt-4 font-display text-[clamp(30px,4vw,44px)] font-semibold tracking-[-0.02em]">
          You’re in{data.nickname ? `, ${data.nickname}` : ''}
        </h1>
        <p className="mx-auto mt-3 max-w-[48ch] text-[17px] leading-relaxed text-muted">
          You’re <span className="font-semibold text-ink">#{rank?.rank ?? '—'}</span>
          {rank?.totalWaiting ? ` of ${rank.totalWaiting.toLocaleString()}` : ''}. Earn points to move up — invite
          friends to jump, follow &amp; quote to nudge ahead.
        </p>
      </div>

      {/* Points + Rank — the headline gamification */}
      <Card className="grid items-center gap-6 sm:grid-cols-[auto_1fr]">
        <RankRing position={rank?.rank ?? null} total={rank?.totalWaiting ?? null} points={points} />
        <div className="grid gap-4">
          <div className="flex items-baseline justify-between">
            <div>
              <div className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Your points</div>
              <div className="font-display text-[34px] font-semibold leading-none tabular-nums">
                {points.toLocaleString()}
              </div>
            </div>
            {rank?.isTopReferrer && (
              <span className="inline-flex items-center gap-1.5 rounded-full bg-sky-soft px-3 py-1 text-sm font-semibold text-accent-strong">
                <Trophy className="size-3.5" /> Top 10
              </span>
            )}
          </div>
          {/* breakdown chips */}
          <div className="flex flex-wrap gap-2">
            {referralCount > 0 && <Chip>{referralCount} referrals · +{referralCount * 100}</Chip>}
            {rank?.breakdown.form ? <Chip>Profile · +{rank.breakdown.form}</Chip> : null}
            {rank?.breakdown.follow_product ? <Chip>Follow · +{rank.breakdown.follow_product}</Chip> : null}
            {rank?.breakdown.follow_founder ? <Chip>Follow founder · +{rank.breakdown.follow_founder}</Chip> : null}
            {rank?.breakdown.quote ? <Chip>Quote · +{rank.breakdown.quote}</Chip> : null}
          </div>
          <div>
            <div className="mb-1.5 flex items-center justify-between text-sm">
              <span className="font-medium text-ink">{rank?.tier ? `Unlocked: ${rank.tier.reward}` : 'Next reward'}</span>
              {next && (
                <span className="text-muted">
                  {next.remaining.toLocaleString()} pts to {next.reward}
                </span>
              )}
            </div>
            <div className="h-2.5 overflow-hidden rounded-full bg-cloud">
              <div
                className="h-full rounded-full bg-accent [transition:width_0.8s_var(--ease-out-soft)]"
                style={{ width: `${segProgress * 100}%` }}
              />
            </div>
          </div>
        </div>
      </Card>

      {/* Your referral link */}
      <Card>
        <div className="mb-1 flex items-center gap-2">
          <Share2 className="size-4 text-accent" />
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Your referral link · +100 each</p>
        </div>
        <p className="text-sm text-muted">A referral counts once your invite completes their profile.</p>
        <div className="mt-4 flex flex-wrap gap-2.5">
          <span className="flex min-w-[220px] flex-1 items-center overflow-hidden text-ellipsis whitespace-nowrap rounded-full border border-border bg-cloud px-4 font-mono text-[13px] leading-[44px] text-ink">
            {data.shareUrl}
          </span>
          <Button onClick={copyShare}>
            {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
            {copied ? 'Copied' : 'Copy link'}
          </Button>
        </div>
      </Card>

      {/* Ways to climb */}
      <Card>
        <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Ways to climb</p>
        <h2 className="mb-3.5 mt-2.5 font-display text-2xl font-semibold">Every point moves you up</h2>
        <div className="grid gap-2.5">
          {CLIMB.map((c) => {
            const earned = c.key === 'referral' ? referralCount > 0 : (rank?.breakdown[c.key] ?? 0) > 0;
            return (
              <div
                key={c.key}
                className={`flex items-center gap-3.5 rounded-lg border px-4 py-3 ${
                  earned ? 'border-[#bfeeff] bg-sky-soft' : 'border-border bg-surface'
                }`}
              >
                <span
                  className={`flex size-[34px] shrink-0 items-center justify-center rounded-full text-[13px] font-semibold ${
                    earned ? 'bg-accent text-white' : 'bg-cloud text-accent-strong'
                  }`}
                >
                  {earned ? <Check className="size-4" /> : `+${c.points}`}
                </span>
                <span className="min-w-0">
                  <span className="block font-medium text-ink">{c.label}</span>
                  <span className="block text-[13px] text-muted">{c.hint}</span>
                </span>
                <span className="ml-auto text-[13px] font-semibold tabular-nums text-accent">+{c.points}</span>
              </div>
            );
          })}
        </div>
      </Card>

      {/* Reward ladder — in points, replacement not additive */}
      <Card>
        <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Rewards</p>
        <h2 className="mb-3.5 mt-2.5 font-display text-2xl font-semibold">
          {rank?.tier ? `You’ve unlocked ${rank.tier.reward}` : 'Reach 300 pts for your first reward'}
        </h2>
        <div className="grid gap-2.5">
          {PTS_TIERS.map((t) => {
            const done = points >= t.points;
            const isNext = !done && next?.points === t.points;
            return (
              <Rung
                key={t.points}
                done={done}
                next={isNext}
                badge={done ? '✓' : `${t.points / 100}`}
                label={`${t.reward}`}
                meta={done ? 'unlocked' : `${(t.points - points).toLocaleString()} pts more`}
              />
            );
          })}
          <Rung
            done={!!rank?.isTopReferrer}
            badge={<Star className="size-3.5" />}
            label="Top 10 by points — 1 year free"
            meta={rank?.isTopReferrer ? 'you’re in' : 'compete on the board'}
          />
        </div>
        <FinePrint />
      </Card>

      {/* Leaderboard by nickname */}
      <Card>
        <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Leaderboard</p>
        <h2 className="mb-3.5 mt-2.5 font-display text-2xl font-semibold">Top referrers</h2>
        <LeaderboardInline meRefCode={data.refCode} />
      </Card>
    </div>
  );
}

function Chip({ children }: { children: React.ReactNode }) {
  return (
    <span className="rounded-full border border-border bg-cloud px-2.5 py-1 text-[12px] font-medium text-muted">
      {children}
    </span>
  );
}

/* ---------- shared bits ---------- */
function RankRing({
  position,
  total,
  points,
}: {
  position: number | null;
  total: number | null;
  points?: number;
}) {
  const pct = position && total && total > 0 ? Math.max(0, Math.min(1, 1 - position / total)) : 0;
  const topPct = position && total ? Math.max(1, Math.round((position / total) * 100)) : null;
  const r = 54;
  const c = 2 * Math.PI * r;
  return (
    <div className="relative grid size-[150px] place-items-center">
      <svg viewBox="0 0 130 130" className="size-[150px] -rotate-90">
        <circle cx="65" cy="65" r={r} fill="none" stroke="var(--color-border)" strokeWidth="10" />
        <circle
          cx="65"
          cy="65"
          r={r}
          fill="none"
          stroke="var(--color-accent)"
          strokeWidth="10"
          strokeLinecap="round"
          strokeDasharray={c}
          strokeDashoffset={c * (1 - pct)}
          className="[transition:stroke-dashoffset_1s_var(--ease-out-soft)]"
        />
      </svg>
      <div className="absolute text-center">
        <div className="font-display text-[32px] font-semibold leading-none tabular-nums">
          {position ? `#${position.toLocaleString()}` : '—'}
        </div>
        {points != null ? (
          <div className="mt-1 text-xs text-muted tabular-nums">{points.toLocaleString()} pts</div>
        ) : (
          topPct && <div className="mt-1 text-xs text-muted">Top {topPct}%</div>
        )}
      </div>
    </div>
  );
}

function Rung({
  done,
  next,
  badge,
  label,
  meta,
}: {
  done?: boolean;
  next?: boolean;
  badge: React.ReactNode;
  label: string;
  meta: string;
}) {
  return (
    <div
      className={`flex items-center gap-3.5 rounded-lg border px-4 py-3.5 ${
        done ? 'border-[#bfeeff] bg-sky-soft' : next ? 'border-accent ring-4 ring-accent/10' : 'border-border bg-surface'
      }`}
    >
      <span
        className={`flex size-[34px] shrink-0 items-center justify-center rounded-full text-[13px] font-semibold tabular-nums ${
          done ? 'bg-accent text-white' : 'bg-cloud text-muted'
        }`}
      >
        {badge}
      </span>
      <span className="font-medium text-ink">{label}</span>
      <span className="ml-auto text-[13px] text-muted">{meta}</span>
    </div>
  );
}

function ProfileForm({ code, onDone }: { code: string; onDone: () => void }) {
  const [state, setState] = useState<'idle' | 'loading'>('idle');
  const [msg, setMsg] = useState('');

  async function onSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setState('loading');
    setMsg('');
    const f = new FormData(e.currentTarget);
    try {
      const res = await fetch('/api/waitlist/profile', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          code,
          nickname: f.get('nickname'),
          a1: f.get('a1'),
          a2: f.get('a2'),
          a3: f.get('a3'),
          xHandle: f.get('xHandle'),
        }),
      });
      const d = await res.json();
      if (!res.ok || !d.ok) {
        setMsg('Could not save. Please try again.');
        setState('idle');
        return;
      }
      onDone();
    } catch {
      setMsg('Network hiccup. Try again.');
      setState('idle');
    }
  }

  const field =
    'h-11 rounded-lg border border-border bg-surface px-4 text-[15px] text-ink focus:border-accent focus:outline-none focus:ring-4 focus:ring-accent/15';

  return (
    <Card>
      <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Interested? Join the program</p>
      <h2 className="mb-1 mt-2.5 font-display text-2xl font-semibold">Answer 4 quick questions</h2>
      <p className="text-sm text-muted">
        We’ll generate your personal referral link and add you to the leaderboard under your nickname.
      </p>
      <form onSubmit={onSubmit} className="mt-4 grid gap-4">
        <label className="grid gap-1.5 text-sm font-medium text-ink">
          Leaderboard nickname
          <input name="nickname" maxLength={40} required placeholder="e.g. shogun_sam" className={field} />
        </label>
        <label className="grid gap-1.5 text-sm font-medium text-ink">
          Why did you sign up?
          <input name="a1" maxLength={200} required placeholder="What made ShogunAI click for you" className={field} />
        </label>
        <label className="grid gap-1.5 text-sm font-medium text-ink">
          Where do you work?
          <input name="a2" maxLength={120} required placeholder="Company / what you do" className={field} />
        </label>
        <label className="grid gap-1.5 text-sm font-medium text-ink">
          What’s the biggest problem you’d hand to ShogunAI?
          <input name="a3" maxLength={200} required placeholder="The workflow you’d love gone" className={field} />
        </label>
        <label className="grid gap-1.5 text-sm font-medium text-ink">
          X handle <span className="font-normal text-muted">— optional, unlocks follow &amp; quote points</span>
          <input name="xHandle" maxLength={16} placeholder="@yourhandle" className={field} />
        </label>
        <Button type="submit" disabled={state === 'loading'}>
          {state === 'loading' ? <Loader2 className="size-4 animate-spin" /> : 'Get my referral link'}
          {state !== 'loading' && <ArrowRight className="size-4" />}
        </Button>
        {msg && <p className="text-sm text-danger">{msg}</p>}
      </form>
    </Card>
  );
}

function LeaderboardInline({ meRefCode }: { meRefCode: string }) {
  const [board, setBoard] = useState<LbRow[] | null>(null);
  useEffect(() => {
    let alive = true;
    fetch('/api/waitlist/leaderboard?limit=10')
      .then((r) => r.json())
      .then((d) => alive && d.ok && setBoard(d.board))
      .catch(() => alive && setBoard([]));
    return () => {
      alive = false;
    };
  }, []);
  if (board === null)
    return (
      <div className="grid place-items-center py-4">
        <Loader2 className="size-4 animate-spin text-accent" />
      </div>
    );
  if (!board.length) return <p className="text-sm text-muted">No qualified referrals yet. Be the first.</p>;
  return (
    <div className="grid gap-1">
      {board.map((e) => {
        const me = e.refCode === meRefCode;
        return (
          <div
            key={e.rank}
            className={`grid grid-cols-[32px_1fr_auto] items-center gap-3 rounded-lg px-2.5 py-2.5 ${
              me ? 'bg-sky-soft' : ''
            }`}
          >
            <span
              className={`flex size-7 items-center justify-center rounded-full text-[12px] font-semibold tabular-nums ${
                e.rank <= 3 ? 'bg-accent text-white' : 'bg-cloud text-muted'
              }`}
            >
              {e.rank}
            </span>
            <span className="truncate text-sm font-medium text-ink">
              {e.name}
              {me && <span className="ml-2 text-xs font-normal text-accent">you</span>}
            </span>
            <span className="font-semibold tabular-nums text-accent">{e.count}</span>
          </div>
        );
      })}
    </div>
  );
}
