import type { Dictionary } from '@/i18n/dictionaries';

/**
 * Authority / credibility lockups shown directly below the hero form.
 * YC-backed · Product Hunt (Coming soon) · Hackathon win. Localized via dict.
 */
const TONE: Record<string, string> = {
  yc: '#f5610f',
  ph: '#da552f',
  award: '#0089cf',
};

export function Badges({ t }: { t: Dictionary }) {
  return (
    <div className="mt-9 flex flex-wrap items-center justify-center gap-3">
      {t.authority.items.map((b) => (
        <div
          key={b.brand + b.top}
          className="group/lk lift flex items-center gap-2.5 rounded-xl border border-border bg-surface/80 px-3.5 py-2 shadow-[var(--shadow-card)] backdrop-blur hover:border-accent/40"
        >
          <span
            className="flex size-8 items-center justify-center rounded-md text-[15px] font-bold text-white transition-transform duration-300 group-hover/lk:scale-110"
            style={{ background: TONE[b.tone] ?? '#0089cf' }}
          >
            {b.mark}
          </span>
          <span className="text-left leading-tight">
            <span className="block text-[10px] font-medium uppercase tracking-wide text-muted">{b.top}</span>
            <span className="block text-sm font-semibold text-ink">{b.brand}</span>
          </span>
        </div>
      ))}
    </div>
  );
}
