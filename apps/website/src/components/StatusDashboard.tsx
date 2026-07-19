'use client';

import { Check, Copy, Loader2, Star } from 'lucide-react';
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

const TIERS = [
  { threshold: 3, label: '1 month free' },
  { threshold: 10, label: '3 months free' },
  { threshold: 30, label: '6 months free' },
];

export function StatusDashboard({ code }: { code: string }) {
  const [data, setData] = useState<Status | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const load = useCallback(async () => {
    try {
      const res = await fetch(`/api/waitlist/status?code=${encodeURIComponent(code)}`);
      const d = await res.json();
      if (!res.ok || !d.ok) {
        setError(d?.error === 'not_found' ? 'not_found' : 'bad');
        return;
      }
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

  if (error === 'not_found') {
    return (
      <Card className="mx-auto grid max-w-md gap-4 text-center">
        <h1 className="font-display text-2xl font-semibold">We couldn’t find that page</h1>
        <p className="text-muted">This status link is invalid or expired. Check the URL from your welcome email.</p>
        <Button asChild className="justify-self-center">
          <a href="/">Back to home</a>
        </Button>
      </Card>
    );
  }
  if (error)
    return (
      <Card className="mx-auto max-w-md text-center text-muted">Something went wrong. Please refresh.</Card>
    );
  if (!data)
    return (
      <div className="grid place-items-center py-16">
        <Loader2 className="size-5 animate-spin text-accent" />
      </div>
    );

  const count = data.qualifiedReferrals;

  return (
    <div className="mx-auto grid max-w-3xl gap-6">
      <div className="text-center">
        <Badge dot>{data.profileComplete ? 'Your spot is secured' : 'You’re on the list'}</Badge>
        <h1 className="mt-4 font-display text-[clamp(30px,4vw,44px)] font-semibold tracking-[-0.015em]">
          Welcome to ShogunAI
        </h1>
        <p className="mt-2 text-[17px] text-muted">Refer friends and answer a few questions to move up the line.</p>
      </div>

      <div className="grid gap-4 sm:grid-cols-3">
        <Stat v={data.position ?? '—'} k={`your position${data.totalWaiting ? ` of ${data.totalWaiting}` : ''}`} />
        <Stat v={count} k="qualified referrals" />
        <Stat v={data.leaderboardRank ? `#${data.leaderboardRank}` : '—'} k="leaderboard rank" />
      </div>

      <Card>
        <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Your invite link</p>
        <h2 className="mb-1 mt-2.5 font-display text-2xl font-semibold">Skip the line — bring people in</h2>
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

      <Card>
        <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Rewards</p>
        <h2 className="mb-3.5 mt-2.5 font-display text-2xl font-semibold">
          {data.tier ? `You’ve unlocked ${data.tier.label}` : 'Refer 3 to unlock your first reward'}
        </h2>
        <div className="grid gap-2.5">
          {TIERS.map((tItem) => {
            const done = count >= tItem.threshold;
            const isNext = !done && data.nextTier?.threshold === tItem.threshold;
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

      {!data.profileComplete && <ProfileForm code={code} onDone={load} answered={data.answered} />}

      <Card>
        <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Leaderboard</p>
        <h2 className="mb-3.5 mt-2.5 font-display text-2xl font-semibold">Top referrers</h2>
        <LeaderboardInline />
      </Card>
    </div>
  );
}

function Stat({ v, k }: { v: string | number; k: string }) {
  return (
    <Card className="text-center">
      <div className="font-display text-[40px] font-semibold tabular-nums tracking-[-0.02em]">{v}</div>
      <div className="mt-1 text-[13px] text-muted">{k}</div>
    </Card>
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

function ProfileForm({ code, onDone, answered }: { code: string; onDone: () => void; answered: number }) {
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
        body: JSON.stringify({ code, a1: f.get('a1'), a2: f.get('a2'), a3: f.get('a3') }),
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

  const field = 'h-11 rounded-lg border border-border bg-surface px-4 text-[15px] text-ink focus:border-accent focus:outline-none focus:ring-4 focus:ring-accent/15';

  return (
    <Card>
      <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">Secure your spot</p>
      <h2 className="mb-1 mt-2.5 font-display text-2xl font-semibold">Tell us who you are</h2>
      <p className="text-sm text-muted">Answering all three completes your profile and moves you up the line. ({answered}/3 done)</p>
      <form onSubmit={onSubmit} className="mt-2 grid gap-4">
        <label className="grid gap-1.5 text-sm font-medium text-ink">
          What do you do?
          <select name="a1" defaultValue="" required className={field}>
            <option value="" disabled>Choose one…</option>
            <option>Founder</option>
            <option>Builder / Engineer</option>
            <option>Researcher</option>
            <option>Operator</option>
            <option>Creator</option>
            <option>Other</option>
          </select>
        </label>
        <label className="grid gap-1.5 text-sm font-medium text-ink">
          Where do you spend most of your day?
          <input name="a2" maxLength={200} placeholder="e.g. code, meetings, writing, research" required className={field} />
        </label>
        <label className="grid gap-1.5 text-sm font-medium text-ink">
          What would you hand off to an AI first?
          <input name="a3" maxLength={200} placeholder="e.g. follow-ups, note-taking, scheduling" required className={field} />
        </label>
        <Button type="submit" disabled={state === 'loading'}>
          {state === 'loading' ? <Loader2 className="size-4 animate-spin" /> : 'Complete profile'}
        </Button>
        {msg && <p className="text-sm text-danger">{msg}</p>}
      </form>
    </Card>
  );
}

function LeaderboardInline() {
  const [board, setBoard] = useState<{ rank: number; maskedEmail: string; count: number }[] | null>(null);
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
    <div className="grid">
      {board.map((e) => (
        <div key={e.rank} className="grid grid-cols-[40px_1fr_auto] items-center gap-3 border-b border-border px-2 py-3">
          <span className="font-mono text-[13px] tabular-nums text-muted">#{e.rank}</span>
          <span className="text-sm text-ink">{e.maskedEmail}</span>
          <span className="font-semibold tabular-nums text-accent">{e.count}</span>
        </div>
      ))}
    </div>
  );
}
