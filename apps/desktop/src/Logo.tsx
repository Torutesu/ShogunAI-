// The ShogunAI mark: a folded-paper kabuto, six flat facets around a vertical axis.
//
// Same geometry as the marketing site's Logo.tsx and the app icon
// (src-tauri/icons/icon.svg) — one brand, one mark. If any of them changes, all of them must.
//
// Self-contained rather than importing from apps/website: that package is a separate workspace
// with its own build, and the desktop app must not depend on it.

/** Left half of the mark, in its own 1000x644 space. The right half is this mirrored, so the two
 *  sides cannot drift apart under editing. */
const FACETS = [
  "M497 4 L307 266 L487 552 Z", // centre peak
  "M0 109 L312 279 L422 531 L179 415 Z", // upper wing
  "M179 435 L370 508 L62 644 Z", // lower blade
];

/** Brand blue. Flat, not a gradient: the mark reads as folded paper, and a gradient across a facet
 *  fights the fold it is meant to describe. */
export const MARK_BLUE = "#0B4DFF";

/** The mark itself, without a plate. Callers place it on whatever surface they already have. */
export function MarkFacets({ fill = MARK_BLUE }: { fill?: string }): JSX.Element {
  return (
    <g fill={fill}>
      {FACETS.map((d) => (
        <path key={d} d={d} />
      ))}
      <g transform="translate(1000,0) scale(-1,1)">
        {FACETS.map((d) => (
          <path key={d} d={d} />
        ))}
      </g>
    </g>
  );
}

/**
 * The mark at `size` square. The artwork is wider than it is tall (1000x644), so it is centred in
 * a square box: every existing call site passes one number and lays the result out as a square,
 * and changing that contract would move the logo in every header at once.
 */
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
      {/* 1000 → 100 wide, then centred vertically: (100 − 64.4) / 2 = 17.8 */}
      <g transform="translate(0 17.8) scale(0.1)">
        <MarkFacets />
      </g>
    </svg>
  );
}
