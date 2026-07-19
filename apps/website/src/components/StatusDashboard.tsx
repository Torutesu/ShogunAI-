'use client';

import { Check, Copy, Loader2, Share2, Star, Trophy } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';

type Status = {
  status: string;
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

/** Circular progress ring — visualizes queue percentile. */
function RankRing({ position, total }: { position: number | null; total: number | null }) {
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
        {topPct && <div className="mt-1 text-xs text-muted">Top {topPct}%</div>}
      </div>
    </div>
  );
}

export function StatusDashboard({ code }: { code: string }) {
  const [data, setData] = useState<Status | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

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

  async function copyShare() {
    if (!data) return;
    try {
      await navigator.clipboard.writeText(data.shareUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable */
    }
  }

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

  const count = data.qualifiedReferrals;
  const next = data.nextTier;
  const tierProgress = next ? Math.min(1, count / next.threshold) : 1;

  return (
    <div className="mx-auto grid max-w-3xl gap-6">
      {/* Intro */}
      <div className="text-center">
        <Badge dot>{data.profileComplete ? 'Your spot is secured' : 'You’re on the waitlist'}</Badge>
        <h1 className="mt-4 font-display text-[clamp(30px,4vw,44px)] font-semibold tracking-[-0.02em]">
          The ShogunAI referral program
        </h1>
        <p className="mx-auto mt-3 max-w-[48ch] text-[17px] leading-relaxed text-muted">
          Every friend who joins with your link and completes their profile moves you up the line. Refer more,
          climb faster, unlock free months — and top the leaderboard.
        </p>
      </div>

      {/* Rank hero — your position + referrals, visual */}
      <Card className="grid items-center gap-6 sm:grid-cols-[auto_1fr]">
        <RankRing position={data.position} total={data.totalWaiting} />
        <div className="grid gap-4">
          <div className="flex items-baseline justify-between">
            <div>
              <div className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Your position</div>
              <div className="text-sm text-muted">
                of {data.totalWaiting?.toLocaleString() ?? '—'} on the waitlist
              </div>
            </div>
            {data.leaderboardRank && (
              <span className="inline-flex items-center gap-1.5 rounded-full bg-sky-soft px-3 py-1 text-sm font-semibold text-accent-strong">
                <Trophy className="size-3.5" /> #{data.leaderboardRank}
              </span>
            )}
          </div>
          <div>
            <div className="mb-1.5 flex items-center justify-between text-sm">
              <span className="font-medium text-ink">
                <span className="font-display text-lg font-semibold tabular-nums">{count}</span> qualified referrals
              </span>
              {next && (
                <span className="text-muted">
                  {next.remaining} to {next.label}
                </span>
              )}
            </div>
            <div className="h-2.5 overflow-hidden rounded-full bg-cloud">
              <div
                className="h-full rounded-full bg-accent [transition:width_0.8s_var(--ease-out-soft)]"
                style={{ width: `${tierProgress * 100}%` }}
              />
            </div>
          </div>
        </div>
      </Card>

      {/* Share */}
      <Card>
        <div className="mb-1 flex items-center gap-2">
          <Share2 className="size-4 text-accent" />
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Your invite link</p>
        </div>
        <p className="text-sm text-muted">
          A referral counts once your invite completes their profile. Rewards replace each other; they don’t stack.
        </p>
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

      {/* Reward ladder */}
      <Card>
        <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Rewards</p>
        <h2 className="mb-3.5 mt-2.5 font-display text-2xl font-semibold">
          {data.tier ? `You’ve unlocked ${data.tier.label}` : 'Refer 3 to unlock your first reward'}
        </h2>
        <div className="grid gap-2.5">
          {TIERS.map((tItem) => {
            const done = count >= tItem.threshold;
            const isNext = !done && next?.threshold === tItem.threshold;
            return (
              <Rung
                key={tItem.threshold}
                done={done}
                next={isNext}
                badge={done ? '✓' : String(tItem.threshold)}
                label={tItem.label}
                meta={done ? 'unlocked' : `${tItem.threshold - count} more`}
              />
            );
          })}
          <Rung
            done={data.isTopReferrer}
            badge={<Star className="size-3.5" />}
            label="Top 10 referrers — 1 year free"
            meta={data.isTopReferrer ? 'you’re in' : 'compete on the board'}
          />
        </div>
      </Card>

      {/* Deep onboarding */}
      {!data.profileComplete && <ProfileForm code={code} onDone={load} />}

      {/* Leaderboard by nickname */}
      <Card>
        <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Leaderboard</p>
        <h2 className="mb-3.5 mt-2.5 font-display text-2xl font-semibold">Top referrers</h2>
        <LeaderboardInline meRefCode={data.refCode} />
      </Card>
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
      <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Secure your spot</p>
      <h2 className="mb-1 mt-2.5 font-display text-2xl font-semibold">Tell us about you</h2>
      <p className="text-sm text-muted">
        Completing this moves you up the line — and sets the nickname you’ll use on the leaderboard.
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
        <Button type="submit" disabled={state === 'loading'}>
          {state === 'loading' ? <Loader2 className="size-4 animate-spin" /> : 'Secure my spot'}
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
