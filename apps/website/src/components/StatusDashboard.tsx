'use client';

import { ArrowRight, Check, Copy, Gift, Loader2, Mail, Star } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { Logo } from '@/components/Logo';
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
        <h1 className="mt-4 font-display text-[clamp(26px,6vw,44px)] font-semibold tracking-[-0.02em]">
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

/* ---------- Step 2: clean, share-first referral page ---------- */
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
  const referralCount = data.qualifiedReferrals;
  const shareText = "I'm getting early access to ShogunAI — the AI that remembers, then acts. Join me:";

  return (
    <div className="mx-auto w-full max-w-[600px]">
      <EnvelopeHero />

      <div className="mt-9 text-center">
        <h1 className="font-display text-[clamp(30px,7vw,46px)] font-semibold tracking-[-0.02em]">
          Invite builders, climb the line
        </h1>
        <p className="mx-auto mt-3.5 max-w-[46ch] text-[17px] leading-relaxed text-muted">
          Invite a builder you believe in. They get instant early access — and every friend who completes
          their profile is <span className="font-semibold text-ink">+100 points</span> toward months of
          ShogunAI free.
        </p>
      </div>

      {/* Share link */}
      <div className="mt-8 flex flex-wrap items-center gap-2.5 rounded-full border border-border bg-surface p-1.5 pl-5 shadow-[var(--shadow-card)]">
        <span className="min-w-[160px] flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[14px] text-ink">
          {data.shareUrl}
        </span>
        <Button onClick={copyShare} className="shrink-0">
          {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
          {copied ? 'Copied' : 'Copy link'}
        </Button>
      </div>

      <ShareRow url={data.shareUrl} text={shareText} />

      <Divider />
      <HowItWorks />

      <Divider />
      <StatsRow
        referrals={referralCount}
        points={points}
        rank={rank?.rank ?? null}
        reward={rank?.tier?.reward ?? null}
      />

      <Divider />

      {/* Rewards — progress toward the next tier */}
      <section>
        <SectionLabel>Rewards</SectionLabel>
        <div className="mb-1.5 mt-3.5 flex items-center justify-between text-sm">
          <span className="font-medium text-ink">
            {rank?.tier ? `Unlocked: ${rank.tier.reward}` : 'First reward at 300 pts'}
          </span>
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
        <div className="mt-4 grid gap-2.5">
          {PTS_TIERS.map((t) => {
            const done = points >= t.points;
            const isNext = !done && next?.points === t.points;
            return (
              <Rung
                key={t.points}
                done={done}
                next={isNext}
                badge={done ? '✓' : `${t.points / 100}`}
                label={t.reward}
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
      </section>

      <Divider />

      {/* Ways to climb */}
      <section>
        <SectionLabel>Ways to climb</SectionLabel>
        <div className="mt-3.5 grid gap-2.5">
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
      </section>

      <Divider />

      {/* Leaderboard */}
      <section>
        <SectionLabel>Leaderboard</SectionLabel>
        <h2 className="mb-3.5 mt-1.5 font-display text-xl font-semibold">Top referrers</h2>
        <LeaderboardInline meRefCode={data.refCode} />
      </section>
    </div>
  );
}

function Divider() {
  return <div className="my-9 h-px w-full bg-border" />;
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return <h2 className="font-display text-lg font-semibold text-ink">{children}</h2>;
}

/* Envelope illustration with a reward card peeking out (layered back < card < front). */
function EnvelopeHero() {
  return (
    <div className="relative mx-auto w-full max-w-[420px] pt-[54px]">
      {/* card */}
      <div className="absolute left-1/2 top-0 z-10 w-[64%] -translate-x-1/2 rounded-xl border border-border bg-white px-5 pb-16 pt-5 text-left shadow-[0_12px_30px_rgba(9,11,12,0.16)]">
        <div className="font-display text-[30px] font-semibold leading-none tracking-tight text-[#0b0e11]">
          Early access
        </div>
        <div className="mt-2 text-[13px] text-[#5f6b73]">for a builder you believe in</div>
      </div>
      {/* envelope (back + front flap) */}
      <div className="relative aspect-[420/250] w-full">
        <div className="absolute inset-0 z-0 rounded-[20px] bg-[#0f1317]" />
        <svg
          className="absolute inset-0 z-20 size-full"
          viewBox="0 0 420 250"
          preserveAspectRatio="none"
          aria-hidden="true"
        >
          <path d="M0 60 L210 156 L420 60 L420 230 Q420 250 400 250 L20 250 Q0 250 0 230 Z" fill="#0b0e11" />
          <path d="M0 60 L210 156 L420 60" fill="none" stroke="rgba(255,255,255,0.09)" strokeWidth="1.5" />
        </svg>
        <span className="absolute bottom-4 left-4 z-30 grid size-9 place-items-center rounded-full bg-white/10">
          <Logo size={17} />
        </span>
      </div>
    </div>
  );
}

const STEPS = [
  'Share your link with a builder you believe in.',
  'They get instant early access the moment they sign up.',
  'When they complete their profile, +100 points land in your balance.',
];

function HowItWorks() {
  return (
    <section>
      <SectionLabel>How it works</SectionLabel>
      <div className="mt-4 grid gap-4">
        {STEPS.map((s, i) => (
          <div key={s} className="flex items-start gap-3.5">
            <span className="mt-px grid size-[26px] shrink-0 place-items-center rounded-full bg-ink text-[13px] font-semibold tabular-nums text-on-ink">
              {i + 1}
            </span>
            <span className="text-[15px] leading-relaxed text-muted">{s}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

function StatsRow({
  referrals,
  points,
  rank,
  reward,
}: {
  referrals: number;
  points: number;
  rank: number | null;
  reward: string | null;
}) {
  const items = [
    { label: 'Referrals', value: referrals.toLocaleString() },
    { label: 'Points', value: points.toLocaleString() },
    { label: 'Rank', value: rank ? `#${rank.toLocaleString()}` : '—' },
    { label: 'Reward', value: reward ?? '—' },
  ];
  return (
    <section>
      <SectionLabel>Your referrals</SectionLabel>
      <div className="mt-5 grid grid-cols-2 gap-y-6 sm:grid-cols-4">
        {items.map((it) => (
          <div key={it.label}>
            <div className="font-display text-[26px] font-semibold leading-tight tracking-[-0.01em] tabular-nums text-ink">
              {it.value}
            </div>
            <div className="mt-1.5 text-[11px] font-semibold uppercase tracking-[0.07em] text-muted">
              {it.label}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

/* Social share targets — open the platform's share intent in a new tab. */
function ShareRow({ url, text }: { url: string; text: string }) {
  const e = encodeURIComponent;
  const targets: { label: string; href: string; icon: React.ReactNode }[] = [
    { label: 'X', href: `https://twitter.com/intent/tweet?text=${e(text)}&url=${e(url)}`, icon: <BrandX /> },
    {
      label: 'Email',
      href: `mailto:?subject=${e('Early access to ShogunAI')}&body=${e(`${text} ${url}`)}`,
      icon: <Mail className="size-[17px]" strokeWidth={1.9} />,
    },
    { label: 'LinkedIn', href: `https://www.linkedin.com/sharing/share-offsite/?url=${e(url)}`, icon: <BrandLinkedIn /> },
    { label: 'WhatsApp', href: `https://wa.me/?text=${e(`${text} ${url}`)}`, icon: <BrandWhatsApp /> },
    { label: 'Facebook', href: `https://www.facebook.com/sharer/sharer.php?u=${e(url)}`, icon: <BrandFacebook /> },
    { label: 'Telegram', href: `https://t.me/share/url?url=${e(url)}&text=${e(text)}`, icon: <BrandTelegram /> },
  ];
  return (
    <div className="mt-5 flex flex-wrap justify-center gap-3">
      {targets.map((t) => (
        <a
          key={t.label}
          href={t.href}
          target="_blank"
          rel="noopener noreferrer"
          aria-label={`Share on ${t.label}`}
          className="grid size-11 place-items-center rounded-full border border-border text-ink transition-colors hover:border-accent hover:bg-sky-soft hover:text-accent-strong"
        >
          {t.icon}
        </a>
      ))}
    </div>
  );
}

/* Monochrome brand marks (currentColor). */
function BrandX() {
  return (
    <svg viewBox="0 0 24 24" className="size-[15px]" fill="currentColor" aria-hidden="true">
      <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z" />
    </svg>
  );
}
function BrandLinkedIn() {
  return (
    <svg viewBox="0 0 24 24" className="size-[17px]" fill="currentColor" aria-hidden="true">
      <path d="M4.98 3.5C4.98 4.88 3.87 6 2.5 6S0 4.88 0 3.5 1.12 1 2.5 1 4.98 2.12 4.98 3.5zM.5 8h4V24h-4zM8 8h3.8v2.2h.05c.53-1 1.83-2.2 3.77-2.2 4.03 0 4.78 2.65 4.78 6.1V24h-4v-6.5c0-1.55-.03-3.55-2.16-3.55-2.17 0-2.5 1.69-2.5 3.44V24H8z" />
    </svg>
  );
}
function BrandWhatsApp() {
  return (
    <svg viewBox="0 0 24 24" className="size-[17px]" fill="currentColor" aria-hidden="true">
      <path d="M17.5 14.4c-.3-.15-1.77-.87-2.04-.97-.27-.1-.47-.15-.67.15-.2.3-.77.97-.94 1.17-.17.2-.35.22-.65.07-.3-.15-1.26-.46-2.4-1.48-.89-.79-1.49-1.77-1.66-2.07-.17-.3-.02-.46.13-.61.13-.13.3-.35.45-.52.15-.17.2-.3.3-.5.1-.2.05-.37-.02-.52-.08-.15-.67-1.62-.92-2.22-.24-.58-.49-.5-.67-.51h-.57c-.2 0-.52.07-.8.37-.27.3-1.04 1.02-1.04 2.49s1.07 2.89 1.22 3.09c.15.2 2.1 3.2 5.08 4.49.71.31 1.26.49 1.69.62.71.23 1.36.2 1.87.12.57-.08 1.77-.72 2.02-1.42.25-.7.25-1.29.17-1.42-.07-.13-.27-.2-.57-.35zM12.05 21.5h-.01a9.5 9.5 0 0 1-4.84-1.33l-.35-.2-3.6.94.96-3.5-.23-.36A9.46 9.46 0 0 1 2.5 12 9.53 9.53 0 0 1 18.8 5.28 9.45 9.45 0 0 1 21.6 12a9.54 9.54 0 0 1-9.55 9.5zM20.5 3.5A11.4 11.4 0 0 0 12.05.5 11.5 11.5 0 0 0 2.1 17.7L.5 23.5l5.95-1.56a11.5 11.5 0 0 0 5.6 1.42h.01A11.5 11.5 0 0 0 20.5 3.5z" />
    </svg>
  );
}
function BrandFacebook() {
  return (
    <svg viewBox="0 0 24 24" className="size-[17px]" fill="currentColor" aria-hidden="true">
      <path d="M24 12.07C24 5.4 18.63 0 12 0S0 5.4 0 12.07C0 18.1 4.39 23.1 10.13 24v-8.44H7.08v-3.49h3.05V9.41c0-3.02 1.79-4.69 4.53-4.69 1.31 0 2.68.24 2.68.24v2.97h-1.51c-1.49 0-1.95.93-1.95 1.88v2.26h3.32l-.53 3.49h-2.79V24C19.61 23.1 24 18.1 24 12.07z" />
    </svg>
  );
}
function BrandTelegram() {
  return (
    <svg viewBox="0 0 24 24" className="size-[17px]" fill="currentColor" aria-hidden="true">
      <path d="M23.07 3.36 19.6 20.3c-.26 1.16-.95 1.44-1.92.9l-5.3-3.9-2.56 2.46c-.28.28-.52.52-1.07.52l.38-5.42L18.9 6.4c.43-.38-.09-.6-.67-.22L6.9 13.3l-5.23-1.64c-1.14-.35-1.16-1.14.24-1.68L21.6 1.7c.95-.35 1.78.22 1.47 1.66z" />
    </svg>
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
