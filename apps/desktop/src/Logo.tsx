// The ShogunAI "S" ribbon mark.
//
// Same geometry and gradient as the marketing site's Logo.tsx — one brand, one mark. If the site's
// version changes, this must change with it; a desktop app drawing a different logo from the
// landing page is the kind of drift users notice immediately.
//
// Self-contained rather than importing from apps/website: that package is a separate workspace
// with its own build, and the desktop app must not depend on it. The gradient ids are namespaced
// so they can't collide with anything else the webview renders.

export function Logo({ size = 26, className }: { size?: number; className?: string }): JSX.Element {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 100 100"
      className={className}
      role="img"
      aria-label="ShogunAI"
    >
      <defs>
        <linearGradient id="shogun-mark-grad" x1="0.15" y1="0.08" x2="0.85" y2="0.92">
          <stop offset="0" stopColor="#38bdf8" />
          <stop offset="0.5" stopColor="#0aa5f4" />
          <stop offset="1" stopColor="#0b74d6" />
        </linearGradient>
        <linearGradient id="shogun-mark-fold" x1="0.3" y1="0.25" x2="0.7" y2="0.72">
          <stop offset="0" stopColor="#ffffff" stopOpacity="0.55" />
          <stop offset="1" stopColor="#ffffff" stopOpacity="0" />
        </linearGradient>
      </defs>
      {/* The ribbon, then a soft highlight along its fold — two passes of the same path. */}
      <path
        d="M66 20 L34 34 L66 60 L34 80"
        fill="none"
        stroke="url(#shogun-mark-grad)"
        strokeWidth="26"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M66 20 L34 34 L66 60 L34 80"
        fill="none"
        stroke="url(#shogun-mark-fold)"
        strokeWidth="26"
        strokeLinecap="round"
        strokeLinejoin="round"
        opacity="0.5"
      />
    </svg>
  );
}
