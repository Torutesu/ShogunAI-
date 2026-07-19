import type { Dictionary } from '@/i18n/dictionaries';

/**
 * Authority credentials shown below the hero form.
 * Centerpiece: the real "Winner of YC RFS Hackathon 2026 · Presented by
 * Transpose" badge, reproduced as a crisp, theme-aware vector (matches the
 * light/dark artwork we were given). Below it: Product Hunt "coming soon".
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

function HackathonBadge() {
  return (
    <div className="lift inline-flex items-center gap-4 rounded-2xl border border-border bg-surface px-4 py-3 shadow-[var(--shadow-card)] sm:gap-5 sm:px-5">
      {/* YC orange mark */}
      <span className="flex size-11 shrink-0 items-center justify-center rounded-[11px] bg-[#f5610f] font-display text-[26px] font-bold leading-none text-white sm:size-12">
        Y
      </span>

      {/* Award title */}
      <span className="text-left leading-tight">
        <span className="block text-[12px] font-medium text-muted sm:text-[13px]">Winner of</span>
        <span className="block font-display text-[17px] font-semibold tracking-[-0.01em] text-ink sm:text-[19px]">
          YC RFS Hackathon 2026
        </span>
      </span>

      {/* Divider */}
      <span aria-hidden="true" className="hidden h-10 w-px self-center bg-border sm:block" />

      {/* Presented by Transpose */}
      <span className="hidden text-left leading-tight sm:block">
        <span className="block text-[12px] font-medium text-muted sm:text-[13px]">Presented by</span>
        <span className="flex items-center gap-1.5 text-ink">
          <TransposeMark className="size-[18px]" />
          <span className="font-display text-[17px] font-semibold tracking-[-0.01em] sm:text-[19px]">Transpose</span>
        </span>
      </span>
    </div>
  );
}

export function Badges({ t }: { t: Dictionary }) {
  const ph = t.authority.items.find((b) => b.tone === 'ph');
  return (
    <div className="mt-9 flex flex-col items-center gap-4">
      <HackathonBadge />
      {ph && (
        <div className="group/lk lift flex items-center gap-2.5 rounded-xl border border-border bg-surface/80 px-3.5 py-2 shadow-[var(--shadow-card)] backdrop-blur hover:border-accent/40">
          <span className="flex size-7 items-center justify-center rounded-md bg-[#da552f] text-[13px] font-bold text-white transition-transform duration-300 group-hover/lk:scale-110">
            {ph.mark}
          </span>
          <span className="text-left leading-tight">
            <span className="block text-[10px] font-medium uppercase tracking-wide text-muted">{ph.top}</span>
            <span className="block text-sm font-semibold text-ink">{ph.brand}</span>
          </span>
        </div>
      )}
    </div>
  );
}
