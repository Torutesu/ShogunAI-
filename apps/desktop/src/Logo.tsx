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
 *  wing is resolved in favour of true symmetry.
 *
 *  `part` names which fold each facet is, and rides along as a class so styles/logo-motion.css can
 *  turn it about its own crease. It costs the static mark nothing: the classes carry an origin and
 *  no transform until a motion class is present. */
const FACETS = [
  { part: "peak", d: "M296 254 L469 0 L469 525 Z" }, // centre peak
  { part: "wing", d: "M0 101 L276 264 L446 524 L176 390 Z" }, // wing
  { part: "blade", d: "M62 613 L171 413 L331 493 Z" }, // blade
] as const;

/** Brand blue, sampled from the artwork. Flat, not a gradient: the mark reads as folded paper,
 *  and a gradient across a facet fights the fold it is meant to describe. */
export const MARK_BLUE = "#004CFC";

/** The mark itself, without a plate, in its native 957x614 space. */
export function MarkFacets({ fill = MARK_BLUE }: { fill?: string }): JSX.Element {
  const half = FACETS.map(({ part, d }) => (
    <path key={d} className={`shogun-mark__facet shogun-mark__facet--${part}`} d={d} />
  ));
  return (
    <g fill={fill}>
      {half}
      <g transform="translate(957,0) scale(-1,1)">{half}</g>
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

/**
 * How the mark moves. The fold itself lives in styles/logo-motion.css, which also drops all of it
 * under `prefers-reduced-motion: reduce`.
 *
 * - `unfold`   — the mark folds itself out of the sheet once, over ~760ms. For arrivals: first
 *                run, a window opening. This is the default.
 * - `idle`     — the creases breathe, barely. "Running, nothing to report."
 * - `thinking` — a crease travels centre → edge on a loop. "Working on it."
 * - `static`   — the mark, unmoved. Same pixels as {@link Logo}.
 *
 * `idle` and `thinking` never stop on their own, so a call site that leaves one mounted is paying
 * for it for as long as it is on screen — which the 5% idle-CPU budget notices. Mount them against
 * a state that ends.
 */
export type MarkMotion = "unfold" | "idle" | "thinking" | "static";

/**
 * {@link Logo}, folding. `interactive` adds a fold under the pointer, which composes with `unfold`
 * (that one hands the transform back when it finishes) but not with the two looping motions.
 *
 * Restarting the fold is the caller's job, and React already has the verb for it: change the
 * element's `key` and the animation runs again from a fresh node.
 */
export function AnimatedLogo({
  size = 26,
  motion = "unfold",
  interactive = false,
  className,
}: {
  size?: number;
  motion?: MarkMotion;
  interactive?: boolean;
  className?: string;
}): JSX.Element {
  const classes = [
    "shogun-mark",
    motion === "static" ? null : `shogun-mark--${motion}`,
    interactive ? "shogun-mark--interactive" : null,
    className,
  ].filter(Boolean);

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 957 957"
      className={classes.join(" ")}
      role="img"
      aria-label="ShogunAI"
    >
      <g transform="translate(0 171.5)">
        <MarkFacets />
      </g>
    </svg>
  );
}
