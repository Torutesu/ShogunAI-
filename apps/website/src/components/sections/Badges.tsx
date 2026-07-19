import type { Dictionary } from '@/i18n/dictionaries';

/**
 * Authority credentials below the hero form.
 * Centerpiece: the real "Winner of YC RFS Hackathon 2026 · Presented by
 * Transpose" badge, reproduced faithfully as a theme-aware vector at a
 * moderate size (matches the supplied light/dark artwork). Product Hunt
 * "coming soon" sits beneath it.
 *
 * To use the exact supplied raster instead, drop it at
 * `public/badges/yc-hackathon-light.png` (+ `-dark.png`) and swap the inner
 * markup for two <img> tags gated by the theme.
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
    <div className="lift inline-flex items-center gap-3.5 rounded-2xl border border-border bg-surface px-4 py-2.5 shadow-[var(--shadow-card)] sm:gap-4 sm:px-5 sm:py-3">
      <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-[#f5610f] font-display text-[22px] font-bold leading-none text-white sm:size-10 sm:text-[24px]">
        Y
      </span>
      <span className="text-left leading-tight">
        <span className="block text-[11px] font-medium text-muted sm:text-xs">Winner of</span>
        <span className="block font-display text-[15px] font-semibold tracking-[-0.01em] text-ink sm:text-base">
          YC RFS Hackathon 2026
        </span>
      </span>
      <span aria-hidden="true" className="hidden h-8 w-px self-center bg-border sm:block" />
      <span className="hidden text-left leading-tight sm:block">
        <span className="block text-[11px] font-medium text-muted sm:text-xs">Presented by</span>
        <span className="flex items-center gap-1.5 text-ink">
          <TransposeMark className="size-4" />
          <span className="font-display text-[15px] font-semibold tracking-[-0.01em] sm:text-base">Transpose</span>
        </span>
      </span>
    </div>
  );
}

export function Badges({ t }: { t: Dictionary }) {
  const ph = t.authority.items.find((b) => b.tone === 'ph');
  return (
    <div className="mt-9 flex flex-col items-center gap-3.5">
      <HackathonBadge />
      {ph && (
        <div className="group/lk lift flex items-center gap-2.5 rounded-xl border border-border bg-surface/80 px-3 py-1.5 shadow-[var(--shadow-card)] backdrop-blur hover:border-accent/40">
          <span className="flex size-6 items-center justify-center rounded-md bg-[#da552f] text-[11px] font-bold text-white transition-transform duration-300 group-hover/lk:scale-110">
            {ph.mark}
          </span>
          <span className="text-left leading-tight">
            <span className="block text-[9px] font-medium uppercase tracking-wide text-muted">{ph.top}</span>
            <span className="block text-[13px] font-semibold text-ink">{ph.brand}</span>
          </span>
        </div>
      )}
    </div>
  );
}
