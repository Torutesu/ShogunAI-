/** ShogunAI "S" ribbon mark. Gradient sprite defined once in the layout. */
export function Logo({ size = 26, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 100 100"
      className={className}
      role="img"
      aria-label="ShogunAI"
    >
      <use href="#logoS" />
    </svg>
  );
}

/** Hidden gradient/symbol definitions — render once near the root. */
export function LogoDefs() {
  return (
    <svg width="0" height="0" style={{ position: 'absolute' }} aria-hidden="true">
      <defs>
        <linearGradient id="s-grad" x1="0.15" y1="0.08" x2="0.85" y2="0.92">
          <stop offset="0" stopColor="#38bdf8" />
          <stop offset="0.5" stopColor="#0aa5f4" />
          <stop offset="1" stopColor="#0b74d6" />
        </linearGradient>
        <linearGradient id="s-fold" x1="0.3" y1="0.25" x2="0.7" y2="0.72">
          <stop offset="0" stopColor="#ffffff" stopOpacity="0.55" />
          <stop offset="1" stopColor="#ffffff" stopOpacity="0" />
        </linearGradient>
        <symbol id="logoS" viewBox="0 0 100 100">
          <path
            d="M66 20 L34 34 L66 60 L34 80"
            fill="none"
            stroke="url(#s-grad)"
            strokeWidth="26"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
          <path
            d="M66 20 L34 34 L66 60 L34 80"
            fill="none"
            stroke="url(#s-fold)"
            strokeWidth="26"
            strokeLinecap="round"
            strokeLinejoin="round"
            opacity="0.5"
          />
        </symbol>
      </defs>
    </svg>
  );
}
