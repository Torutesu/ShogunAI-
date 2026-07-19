'use client';

import { useEffect, useState } from 'react';

type Entry = { rank: number; maskedEmail: string; count: number };

/** Public masked leaderboard. Highlights the viewer's own row when given. */
export function Leaderboard({ meRefCode, limit = 10 }: { meRefCode?: string; limit?: number }) {
  const [board, setBoard] = useState<Entry[] | null>(null);

  useEffect(() => {
    let alive = true;
    fetch(`/api/waitlist/leaderboard?limit=${limit}`)
      .then((r) => r.json())
      .then((d) => {
        if (alive && d.ok) setBoard(d.board);
      })
      .catch(() => alive && setBoard([]));
    return () => {
      alive = false;
    };
  }, [limit]);

  if (board === null) return <div className="center muted"><span className="spinner" /></div>;
  if (board.length === 0) return <p className="center muted t-body">No qualified referrals yet. Be the first.</p>;

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
