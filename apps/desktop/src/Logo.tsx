// The ShogunAI mark: a folded-paper kabuto, six flat facets around a vertical axis.
//
// Same geometry as the marketing site's Logo.tsx and the app icon
// (src-tauri/icons/icon.svg) — one brand, one mark. If any of them changes, all of them must.
//
// The vertices were recovered from the supplied artwork by separating its six facets on the alpha
// channel and reducing each to its corners; the reconstruction matches the original to within the
// anti-aliased edge (1.8% of pixels, all of them one pixel deep).
//
// Self-contained rather than importing from apps/website: that package is a separate workspace
// with its own build, and the desktop app must not depend on it.

/** Left half of the mark in its own 957x614 space. The right half is this mirrored, so the two
 *  sides cannot drift apart under editing — and the source artwork's own 3px asymmetry in the
 *  wing is resolved in favour of true symmetry. */
const FACETS = [
  "M296 254 L469 0 L469 525 Z", // centre peak
  "M0 101 L276 264 L446 524 L176 390 Z", // wing
  "M62 613 L171 413 L331 493 Z", // blade
];

/** Brand blue, sampled from the artwork. Flat, not a gradient: the mark reads as folded paper,
 *  and a gradient across a facet fights the fold it is meant to describe. */
export const MARK_BLUE = "#004CFC";

/** The mark itself, without a plate, in its native 957x614 space. */
export function MarkFacets({ fill = MARK_BLUE }: { fill?: string }): JSX.Element {
  return (
    <g fill={fill}>
      {FACETS.map((d) => (
        <path key={d} d={d} />
      ))}
      <g transform="translate(957,0) scale(-1,1)">
        {FACETS.map((d) => (
          <path key={d} d={d} />
        ))}
      </g>
    </g>
  );
}

/**
 * The mark at `size` square. The artwork is wider than it is tall (957x614), so it is centred in
 * a square box: every existing call site passes one number and lays the result out as a square,
 * and changing that contract would move the logo in every header at once.
 */
export function Logo({ size = 26, className }: { size?: number; className?: string }): JSX.Element {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 957 957"
      className={className}
      role="img"
      aria-label="ShogunAI"
    >
      {/* Centred in the square box: (957 − 614) / 2 = 171.5 */}
      <g transform="translate(0 171.5)">
        <MarkFacets />
      </g>
    </svg>
  );
}
