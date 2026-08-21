'use client';

import { useEffect, useState } from 'react';

/**
 * Keep the landing page available even if the waitlist database is briefly
 * unavailable. The server-rendered count is a safe fallback; this component
 * refreshes only the number after the page is already visible.
 */
export function ParticipantCount({
  initialCount,
  locale,
  suffix,
}: {
  initialCount: number;
  locale: string;
  suffix: string;
}) {
  const [count, setCount] = useState(initialCount);

  useEffect(() => {
    let active = true;

    const refresh = () => {
      // The query parameter avoids a browser or intermediary returning an old
      // response after a new person has joined the list.
      void fetch(`/api/waitlist/count?t=${Date.now()}`, { cache: 'no-store' })
        .then(async (response) => {
          if (!response.ok) return null;
          return (await response.json()) as { ok?: boolean; count?: number; fresh?: boolean };
        })
        .then((data) => {
          if (active && data?.ok && Number.isFinite(data.count)) {
            setCount(data.count as number);
          }
        })
        .catch(() => {
          // The fallback remains visible; a counter outage must not affect the LP.
        });
    };

    refresh();
    window.addEventListener('waitlist:updated', refresh);

    return () => {
      active = false;
      window.removeEventListener('waitlist:updated', refresh);
    };
  }, [initialCount]);

  return (
    <span className="hero-participant-count inline-flex items-center gap-2 rounded-full border border-white/70 bg-white/50 px-3.5 py-2 text-xs font-medium text-[#4b5d7d] shadow-[0_8px_24px_rgba(37,124,151,0.08)] backdrop-blur-sm" aria-live="polite">
      <span className="size-2 rounded-full bg-[#2bb3ef] shadow-[0_0_0_4px_rgba(43,179,239,0.15)]" />
      <>{count.toLocaleString(locale)} {suffix}</>
    </span>
  );
}
