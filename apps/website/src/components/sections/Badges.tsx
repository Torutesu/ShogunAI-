import type { Dictionary } from '@/i18n/dictionaries';

/**
 * Authority credentials shown below the hero form — two equally-sized compact
 * lockups on one wrapping row: the real "Winner of YC RFS Hackathon 2026 ·
 * Presented by Transpose" award, and Product Hunt "coming soon". Theme-aware.
 */
function TransposeMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="none" aria-hidden="true">
      <rect
        x="5.4"
        y="5.4"
        width="13.2"
        height="13.2"
        rx="3.6"
        transform="rotate(45 12 12)"
        stroke="currentColor"
        strokeWidth="2"
      />
      <circle cx="12" cy="12" r="2.3" fill="currentColor" />
    </svg>
  );
}

const lockup =
  'group/lk lift flex items-center gap-2.5 rounded-xl border border-border bg-surface/80 px-3.5 py-2 shadow-[var(--shadow-card)] backdrop-blur hover:border-accent/40';
const mark =
  'flex size-7 shrink-0 items-center justify-center rounded-md text-[13px] font-bold text-white transition-transform duration-300 group-hover/lk:scale-110';

export function Badges({ t }: { t: Dictionary }) {
  const ph = t.authority.items.find((b) => b.tone === 'ph');
  return (
    <div className="mt-9 flex flex-wrap items-center justify-center gap-3">
      {/* YC RFS Hackathon — same scale as the Product Hunt lockup */}
      <div className={lockup}>
        <span className={`${mark} bg-[#f5610f]`}>Y</span>
        <span className="text-left leading-tight">
          <span className="block text-[10px] font-medium uppercase tracking-wide text-muted">Winner of</span>
          <span className="block text-sm font-semibold text-ink">YC RFS Hackathon 2026</span>
        </span>
        <span aria-hidden="true" className="mx-0.5 hidden h-7 w-px self-center bg-border sm:block" />
        <span className="hidden text-left leading-tight sm:block">
          <span className="block text-[10px] font-medium uppercase tracking-wide text-muted">Presented by</span>
          <span className="flex items-center gap-1 text-ink">
            <TransposeMark className="size-3.5" />
            <span className="text-sm font-semibold">Transpose</span>
          </span>
        </span>
      </div>

      {/* Product Hunt — coming soon */}
      {ph && (
        <div className={lockup}>
          <span className={`${mark} bg-[#da552f]`}>{ph.mark}</span>
          <span className="text-left leading-tight">
            <span className="block text-[10px] font-medium uppercase tracking-wide text-muted">{ph.top}</span>
            <span className="block text-sm font-semibold text-ink">{ph.brand}</span>
          </span>
        </div>
      )}
    </div>
  );
}
