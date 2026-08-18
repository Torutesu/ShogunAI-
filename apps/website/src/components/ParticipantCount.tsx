'use client';

import { useEffect, useState } from 'react';

/**
 * Keep the landing page available even if the waitlist database is briefly
 * unavailable. The server-rendered count is a safe fallback; this component
 * refreshes only the number after the page is already visible.
 */
export function ParticipantCount({ initialCount, suffix }: { initialCount: number; suffix: string }) {
  const [count, setCount] = useState<number | null>(null);

  useEffect(() => {
    let active = true;

    // The query parameter avoids a browser or intermediary returning an old
    // response after a new person has joined the list.
    void fetch(`/api/waitlist/count?t=${Date.now()}`, { cache: 'no-store' })
      .then(async (response) => {
        if (!response.ok) return null;
        return (await response.json()) as { ok?: boolean; count?: number; fresh?: boolean };
      })
      .then((data) => {
        if (!active) return;
        if (data?.ok && Number.isFinite(data.count)) {
          setCount(data.count as number);
          return;
        }
        setCount(initialCount);
      })
      .catch(() => {
        // The fallback remains visible; a counter outage must not affect the LP.
        if (active) setCount(initialCount);
      });

    return () => {
      active = false;
    };
  }, []);

  return (
    <span className="hero-participant-count inline-flex items-center gap-2 rounded-full border border-white/70 bg-white/50 px-3.5 py-2 shadow-[0_8px_24px_rgba(37,124,151,0.08)] backdrop-blur-sm">
      <span className="size-2 rounded-full bg-[#2bb3ef] shadow-[0_0_0_4px_rgba(43,179,239,0.15)]" />
      {count === null ? (
        <span aria-label="Loading live waitlist count" className="inline-block h-4 w-36 animate-pulse rounded bg-[#2b6173]/15" />
      ) : (
        <>{count.toLocaleString()} {suffix}</>
      )}
    </span>
  );
}
