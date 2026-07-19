import type { Dictionary } from '@/i18n/dictionaries';

/** Transpose mark — rounded square containing a diamond + center dot. */
function TransposeMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" className={className} fill="none" stroke="currentColor" strokeWidth={2.1} aria-hidden="true">
      <rect x="4" y="4" width="24" height="24" rx="7" />
      <path d="M16 9 L23 16 L16 23 L9 16 Z" strokeLinejoin="round" />
      <circle cx="16" cy="16" r="2.4" />
    </svg>
  );
}

/** Real award: Winner of YC RFS Hackathon 2026, presented by Transpose. */
function HackathonBadge() {
  return (
    <div className="lift inline-flex items-center gap-4 rounded-[18px] bg-[linear-gradient(135deg,#ff8a3d_0%,#f5610f_45%,#e8500a_100%)] px-4 py-3 text-white shadow-[0_12px_36px_rgba(232,80,10,0.28)] sm:gap-5 sm:px-5">
      {/* YC mark */}
      <span className="flex size-12 shrink-0 items-center justify-center rounded-xl bg-white sm:size-14">
        <span className="font-display text-2xl font-bold text-[#f5610f] sm:text-3xl">Y</span>
      </span>
      {/* Award */}
      <span className="text-left leading-tight">
        <span className="block text-[13px] font-medium text-white/85">Winner of</span>
        <span className="block font-display text-lg font-semibold leading-tight sm:text-xl">
          YC RFS Hackathon 2026
        </span>
      </span>
      {/* Divider */}
      <span className="mx-0.5 h-10 w-px bg-white/30 sm:h-12" aria-hidden="true" />
      {/* Presenter */}
      <span className="hidden text-left leading-tight sm:block">
        <span className="block text-[13px] font-medium text-white/85">Presented by</span>
        <span className="mt-0.5 flex items-center gap-2">
          <TransposeMark className="size-6 text-white" />
          <span className="font-display text-lg font-semibold">Transpose</span>
        </span>
      </span>
    </div>
  );
}

/**
 * Credential badges. The YC RFS Hackathon award is real; Product Hunt and the
 * privacy badge are placeholders — enable each only once it's actually true.
 */
export function Badges({ t }: { t: Dictionary }) {
  const b = t.badges;
  return (
    <div className="mt-9 flex flex-col items-center gap-4">
      <HackathonBadge />
      <div className="flex flex-wrap items-center justify-center gap-3">
        <Lockup
          mark={
            <span className="flex size-8 items-center justify-center rounded-md bg-[#da552f] text-[13px] font-bold text-white">
              P
            </span>
          }
          top={b.productHunt.top}
          brand={b.productHunt.brand}
        />
        <Lockup
          mark={
            <span className="flex size-8 items-center justify-center rounded-md bg-sky-soft text-accent">
              <svg viewBox="0 0 24 24" className="size-4" fill="none" stroke="currentColor" strokeWidth={2}>
                <path d="M12 2l8 3v6c0 5-3.5 8-8 11-4.5-3-8-6-8-11V5l8-3z" strokeLinejoin="round" />
              </svg>
            </span>
          }
          top={b.privacy.top}
          brand={b.privacy.brand}
        />
      </div>
    </div>
  );
}

function Lockup({ mark, top, brand }: { mark: React.ReactNode; top: string; brand: string }) {
  return (
    <div className="lift flex items-center gap-2.5 rounded-xl border border-border bg-surface/80 px-3.5 py-2 shadow-[var(--shadow-card)] backdrop-blur hover:border-accent/40">
      {mark}
      <span className="text-left leading-tight">
        <span className="block text-[10px] font-medium uppercase tracking-wide text-muted">{top}</span>
        <span className="block text-sm font-semibold text-ink">{brand}</span>
      </span>
    </div>
  );
}
