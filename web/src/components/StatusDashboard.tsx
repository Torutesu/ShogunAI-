'use client';

import { useCallback, useEffect, useState } from 'react';

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

// Mirror of REFERRAL_TIERS for rendering the ladder.
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
      <div className="card center stack">
        <h1 className="t-h2">We couldn’t find that page</h1>
        <p className="muted">This status link is invalid or expired. Check the URL from your welcome email.</p>
        <a className="btn btn-primary" href="/" style={{ justifySelf: 'center' }}>Back to home</a>
      </div>
    );
  }
  if (error) return <div className="card center"><p className="muted">Something went wrong. Please refresh.</p></div>;
  if (!data) return <div className="center" style={{ padding: 60 }}><span className="spinner" /></div>;

  const count = data.qualifiedReferrals;

  return (
    <div className="dash stack">
      <div className="dash__head">
        <span className="chip"><span className="dot" />{data.profileComplete ? 'Your spot is secured' : 'You’re on the list'}</span>
        <h1 className="t-h1" style={{ margin: '16px 0 8px' }}>Welcome to ShogunAI</h1>
        <p className="muted t-body-lg">Refer friends and answer a few questions to move up the line.</p>
      </div>

      <div className="dash__pos">
        <div className="card dash__stat">
          <div className="dash__stat-v">{data.position ?? '—'}</div>
          <div className="dash__stat-k">your position{data.totalWaiting ? ` of ${data.totalWaiting}` : ''}</div>
        </div>
        <div className="card dash__stat">
          <div className="dash__stat-v">{count}</div>
          <div className="dash__stat-k">qualified referrals</div>
        </div>
        <div className="card dash__stat">
          <div className="dash__stat-v">{data.leaderboardRank ? `#${data.leaderboardRank}` : '—'}</div>
          <div className="dash__stat-k">leaderboard rank</div>
        </div>
      </div>

      {/* Share */}
      <div className="card">
        <span className="eyebrow">Your invite link</span>
        <h2 className="t-h3" style={{ margin: '10px 0 4px' }}>Skip the line — bring people in</h2>
        <p className="muted t-body">A referral counts once your invite completes their profile. Rewards replace each other; they don’t stack.</p>
        <div className="share">
          <span className="share__url" title={data.shareUrl}>{data.shareUrl}</span>
          <button className="btn btn-primary" onClick={copyShare}>{copied ? 'Copied ✓' : 'Copy link'}</button>
        </div>
      </div>

      {/* Reward ladder */}
      <div className="card">
        <span className="eyebrow">Rewards</span>
        <h2 className="t-h3" style={{ margin: '10px 0 14px' }}>
          {data.tier ? `You’ve unlocked ${data.tier.label}` : 'Refer 3 to unlock your first reward'}
        </h2>
        <div className="ladder">
          {TIERS.map((t) => {
            const done = count >= t.threshold;
            const isNext = !done && data.nextTier?.threshold === t.threshold;
            return (
              <div key={t.threshold} className={`rung ${done ? 'rung--done' : ''} ${isNext ? 'rung--next' : ''}`}>
                <span className="rung__badge">{done ? '✓' : t.threshold}</span>
                <span className="rung__label">{t.label}</span>
                <span className="rung__meta">
                  {done ? 'unlocked' : `${t.threshold - count} more`}
                </span>
              </div>
            );
          })}
          <div className={`rung ${data.isTopReferrer ? 'rung--done' : ''}`}>
            <span className="rung__badge">★</span>
            <span className="rung__label">Top 10 referrers — 1 year free</span>
            <span className="rung__meta">{data.isTopReferrer ? 'you’re in' : 'compete on the board'}</span>
          </div>
        </div>
      </div>

      {/* Qualifying profile */}
      {!data.profileComplete && <ProfileForm code={code} onDone={load} answered={data.answered} />}

      <div className="card">
        <span className="eyebrow">Leaderboard</span>
        <h2 className="t-h3" style={{ margin: '10px 0 14px' }}>Top referrers</h2>
        <LeaderboardInline meRefCode={data.refCode} />
      </div>
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

  return (
    <div className="card">
      <span className="eyebrow">Secure your spot</span>
      <h2 className="t-h3" style={{ margin: '10px 0 4px' }}>Tell us who you are</h2>
      <p className="muted t-body">Answering all three completes your profile and moves you up the line. ({answered}/3 done)</p>
      <form className="profile" onSubmit={onSubmit}>
        <div className="profile__q">
          <label htmlFor="a1">What do you do?</label>
          <select id="a1" name="a1" defaultValue="" required>
            <option value="" disabled>Choose one…</option>
            <option>Founder</option><option>Builder / Engineer</option><option>Researcher</option>
            <option>Operator</option><option>Creator</option><option>Other</option>
          </select>
        </div>
        <div className="profile__q">
          <label htmlFor="a2">Where do you spend most of your day?</label>
          <input id="a2" name="a2" maxLength={200} placeholder="e.g. code, meetings, writing, research" required />
        </div>
        <div className="profile__q">
          <label htmlFor="a3">What would you hand off to an AI first?</label>
          <input id="a3" name="a3" maxLength={200} placeholder="e.g. follow-ups, note-taking, scheduling" required />
        </div>
        <button className="btn btn-primary" type="submit" disabled={state === 'loading'}>
          {state === 'loading' ? <span className="spinner" /> : 'Complete profile'}
        </button>
        {msg && <div className="wl-msg wl-msg--err">{msg}</div>}
      </form>
    </div>
  );
}

// Inline leaderboard fetch (kept local so the dashboard is one client bundle).
function LeaderboardInline({ meRefCode }: { meRefCode?: string }) {
  const [board, setBoard] = useState<{ rank: number; maskedEmail: string; count: number }[] | null>(null);
  useEffect(() => {
    let alive = true;
    fetch('/api/waitlist/leaderboard?limit=10')
      .then((r) => r.json())
      .then((d) => alive && d.ok && setBoard(d.board))
      .catch(() => alive && setBoard([]));
    return () => { alive = false; };
  }, []);
  if (board === null) return <div className="center"><span className="spinner" /></div>;
  if (!board.length) return <p className="muted t-body">No qualified referrals yet. Be the first.</p>;
  return (
    <div className="lb">
      {board.map((e) => (
        <div key={e.rank} className="lb__row">
          <span className="lb__rank">#{e.rank}</span>
          <span className="lb__email">{e.maskedEmail}</span>
          <span className="lb__count">{e.count}</span>
        </div>
      ))}
    </div>
  );
}
