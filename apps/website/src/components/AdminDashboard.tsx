'use client';

import { Loader2, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';

type Stats = {
  totalEntries: number;
  formCompleted: number;
  withXHandle: number;
  pointsByAction: Record<string, number>;
  tierCounts: { none: number; t300: number; t1000: number; t3000: number };
  estLiabilityUsd: number;
  capUsd: number;
  top: Array<{ id: string; nickname: string | null; ref_code: string | null; points: number }>;
  snapshots: Array<{ account: string; latest: string }>;
};

const usd = (n: number) => `$${n.toLocaleString()}`;

export function AdminDashboard() {
  const [key, setKey] = useState('');
  const [saved, setSaved] = useState(false);
  const [stats, setStats] = useState<Stats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);

  useEffect(() => {
    const k = sessionStorage.getItem('admin_key');
    if (k) {
      setKey(k);
      setSaved(true);
    }
  }, []);

  const load = useCallback(async (k: string) => {
    setError(null);
    try {
      const res = await fetch(`/api/admin/stats?key=${encodeURIComponent(k)}`);
      const d = await res.json();
      if (!res.ok || !d.ok) return setError(res.status === 403 ? 'Wrong key.' : 'Failed to load.');
      setStats(d);
    } catch {
      setError('Network error.');
    }
  }, []);

  useEffect(() => {
    if (saved && key) load(key);
  }, [saved, key, load]);

  async function sync() {
    setSyncing(true);
    try {
      await fetch(`/api/admin/sync-x?key=${encodeURIComponent(key)}`, { method: 'POST' });
      await load(key);
    } finally {
      setSyncing(false);
    }
  }

  if (!saved) {
    return (
      <Card className="mx-auto grid max-w-sm gap-3">
        <h1 className="font-display text-xl font-semibold">Admin</h1>
        <p className="text-sm text-muted">Enter the admin token.</p>
        <input
          type="password"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder="ADMIN_TOKEN"
          className="h-11 rounded-lg border border-border bg-surface px-4 text-[15px] focus:border-accent focus:outline-none focus:ring-4 focus:ring-accent/15"
        />
        <Button
          onClick={() => {
            sessionStorage.setItem('admin_key', key);
            setSaved(true);
          }}
          disabled={key.length < 16}
        >
          Enter
        </Button>
      </Card>
    );
  }

  return (
    <div className="mx-auto grid max-w-4xl gap-6">
      <div className="flex items-center justify-between">
        <h1 className="font-display text-2xl font-semibold">Waitlist admin</h1>
        <div className="flex gap-2">
          <Button variant="secondary" onClick={() => load(key)}>
            <RefreshCw className="size-4" /> Refresh
          </Button>
          <Button onClick={sync} disabled={syncing}>
            {syncing ? <Loader2 className="size-4 animate-spin" /> : <RefreshCw className="size-4" />} Sync X
          </Button>
        </div>
      </div>

      {error && <Card className="text-danger">{error}</Card>}
      {!stats && !error && (
        <div className="grid place-items-center py-16">
          <Loader2 className="size-5 animate-spin text-accent" />
        </div>
      )}

      {stats && (
        <>
          <div className="grid gap-4 sm:grid-cols-4">
            <Stat label="Entries" value={stats.totalEntries.toLocaleString()} />
            <Stat label="Form completed" value={stats.formCompleted.toLocaleString()} accent />
            <Stat label="With X handle" value={stats.withXHandle.toLocaleString()} />
            <Stat label="Est. liability" value={usd(stats.estLiabilityUsd)} sub={`cap ${usd(stats.capUsd)}`} />
          </div>

          <div className="grid gap-6 md:grid-cols-2">
            <Card>
              <h2 className="mb-3 font-display text-lg font-semibold">Points by action</h2>
              <div className="grid gap-2">
                {Object.entries(stats.pointsByAction).length === 0 && (
                  <p className="text-sm text-muted">No points awarded yet.</p>
                )}
                {Object.entries(stats.pointsByAction).map(([k, v]) => (
                  <Row key={k} label={k} value={`+${v.toLocaleString()}`} />
                ))}
              </div>
            </Card>

            <Card>
              <h2 className="mb-3 font-display text-lg font-semibold">Reward tiers reached</h2>
              <div className="grid gap-2">
                <Row label="6 months (3,000 pts)" value={String(stats.tierCounts.t3000)} />
                <Row label="3 months (1,000 pts)" value={String(stats.tierCounts.t1000)} />
                <Row label="1 month (300 pts)" value={String(stats.tierCounts.t300)} />
                <Row label="Below first tier" value={String(stats.tierCounts.none)} />
              </div>
              {stats.snapshots.length > 0 && (
                <p className="mt-4 text-xs text-muted">
                  Last X snapshot: {stats.snapshots.map((s) => `${s.account} @ ${new Date(s.latest).toLocaleString()}`).join(' · ')}
                </p>
              )}
            </Card>
          </div>

          <Card>
            <h2 className="mb-3 font-display text-lg font-semibold">Top 20 by points</h2>
            <div className="grid gap-1">
              {stats.top.map((r, i) => (
                <div key={r.id} className="grid grid-cols-[32px_1fr_auto] items-center gap-3 rounded-lg px-2.5 py-2">
                  <span className="flex size-7 items-center justify-center rounded-full bg-cloud text-[12px] font-semibold tabular-nums text-muted">
                    {i + 1}
                  </span>
                  <span className="truncate text-sm text-ink">
                    {r.nickname ?? <span className="text-muted">shogun-{r.ref_code?.slice(0, 4)}</span>}
                  </span>
                  <span className="font-semibold tabular-nums text-accent">{r.points.toLocaleString()}</span>
                </div>
              ))}
            </div>
          </Card>
        </>
      )}
    </div>
  );
}

function Stat({ label, value, sub, accent }: { label: string; value: string; sub?: string; accent?: boolean }) {
  return (
    <Card className={accent ? 'border-[#bfeeff] bg-sky-soft' : ''}>
      <div className="text-xs font-semibold uppercase tracking-[0.06em] text-muted">{label}</div>
      <div className="mt-1 font-display text-[26px] font-semibold tabular-nums">{value}</div>
      {sub && <div className="text-xs text-muted">{sub}</div>}
    </Card>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between border-b border-border/60 pb-1.5 text-sm last:border-0">
      <span className="text-muted">{label}</span>
      <span className="font-semibold tabular-nums text-ink">{value}</span>
    </div>
  );
}
