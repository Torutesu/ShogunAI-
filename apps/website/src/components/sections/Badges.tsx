/**
 * Credential badges. Only the real YC RFS Hackathon 2026 award is shown.
 * (Product Hunt and the privacy badge were placeholders — removed for now.)
 */
export function Badges() {
  return (
    <div className="mt-9 flex flex-wrap items-center justify-center gap-3">
      <div className="group/lk lift flex items-center gap-2.5 rounded-xl border border-border bg-surface/80 px-3.5 py-2 shadow-[var(--shadow-card)] backdrop-blur hover:border-accent/40">
        <span className="flex size-8 items-center justify-center rounded-md bg-[#f5610f] text-[15px] font-bold text-white transition-transform duration-300 group-hover/lk:scale-110">
          Y
        </span>
        <span className="text-left leading-tight">
          <span className="block text-[10px] font-medium uppercase tracking-wide text-muted">Winner of</span>
          <span className="block text-sm font-semibold text-ink">YC RFS Hackathon 2026</span>
        </span>
      </div>
    </div>
  );
}
