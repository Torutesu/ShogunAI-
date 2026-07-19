import type { Dictionary } from '@/i18n/dictionaries';

/**
 * Credential badges (Product Hunt, Y Combinator, …). PLACEHOLDER — enable a
 * badge only once the claim is actually true. Rendered as compact lockups.
 */
export function Badges({ t }: { t: Dictionary }) {
  const b = t.badges;
  return (
    <div className="mt-9 flex flex-wrap items-center justify-center gap-3">
      {/* Product Hunt */}
      <Lockup
        mark={
          <span className="flex size-8 items-center justify-center rounded-md bg-[#da552f] text-[13px] font-bold text-white">
            P
          </span>
        }
        top={b.productHunt.top}
        brand={b.productHunt.brand}
      />
      {/* Y Combinator */}
      <Lockup
        mark={
          <span className="flex size-8 items-center justify-center rounded-md bg-[#ff6600] text-[15px] font-bold text-white">
            Y
          </span>
        }
        top={b.yc.top}
        brand={b.yc.brand}
      />
      {/* Privacy */}
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
