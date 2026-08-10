/**
 * The ShogunAI mark: a folded-paper kabuto, six flat facets around a vertical axis.
 *
 * Same geometry as apps/desktop/src/Logo.tsx and the app icon — one brand, one mark. If any of
 * them changes, all of them must.
 */

/** Left half of the mark in its own 957x614 space. The right half is this mirrored, so the two
 *  sides cannot drift apart under editing. */
const FACETS = [
  'M296 254 L469 0 L469 525 Z', // centre peak
  'M0 101 L276 264 L446 524 L176 390 Z', // wing
  'M62 613 L171 413 L331 493 Z', // blade
];

/** Brand blue, sampled from the artwork. Flat, not a gradient: the mark reads as folded paper,
 *  and a gradient across a facet fights the fold it is meant to describe. */
export const MARK_BLUE = '#004CFC';

function Facets({ fill = MARK_BLUE }: { fill?: string }) {
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
 * a square box: every call site passes one number and lays the result out as a square.
 */
export function Logo({ size = 26, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 957 957"
      className={className}
      role="img"
      aria-label="ShogunAI"
    >
      <use href="#logoMark" />
    </svg>
  );
}

/**
 * Hero variant. The facets are inlined rather than referenced through <use> so a page can animate
 * them individually. Note that the previous mark animated by drawing its stroke on; a mark built
 * from filled facets cannot be drawn that way, so the `logo-draw` hook is kept for any existing
 * rule but no longer implies a stroke-dash animation.
 */
export function AnimatedLogo({ size = 26, className }: { size?: number; className?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 957 957" className={className} role="img" aria-label="ShogunAI">
      <g className="logo-draw" transform="translate(0 171.5)">
        <Facets />
      </g>
    </svg>
  );
}

/** Hidden symbol definition — render once near the root. */
export function LogoDefs() {
  return (
    <svg width="0" height="0" style={{ position: 'absolute' }} aria-hidden="true">
      <defs>
        <symbol id="logoMark" viewBox="0 0 957 957">
          {/* Centred in the square box: (957 − 614) / 2 = 171.5 */}
          <g transform="translate(0 171.5)">
            <Facets />
          </g>
        </symbol>
      </defs>
    </svg>
  );
}
