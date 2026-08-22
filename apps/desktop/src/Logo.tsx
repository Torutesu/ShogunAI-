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

import { useCallback, useEffect, useRef } from "react";
import type { JSX } from "react";

/** Which fold each facet is. Rides along as a class so styles/logo-motion.css can turn each one
 *  about its own crease, and so the morph below can find its pair on both halves. */
const PARTS = ["peak", "wing", "blade"] as const;

type Point = readonly [number, number];
type Polygon = readonly Point[];

/** Left half of the mark in its own 957x614 space. The right half is this mirrored, so the two
 *  sides cannot drift apart under editing — and the source artwork's own 3px asymmetry in the
 *  wing is resolved in favour of true symmetry.
 *
 *  Held as points rather than path text only so a second shape can be interpolated against it;
 *  the vertices are exactly the ones the app icon and the marketing site draw. */
const KABUTO: readonly Polygon[] = [
  [[296, 254], [469, 0], [469, 525]], // centre peak
  [[0, 101], [276, 264], [446, 524], [176, 390]], // wing
  [[62, 613], [171, 413], [331, 493]], // blade
];

/**
 * The same sheet, refolded into a heart.
 *
 * Not a heart drawn from scratch and cross-faded to: the same three facets, the same vertex counts,
 * in the same order, so every point has somewhere to travel and the facets stay welded to each
 * other the whole way across. Six of the ten points sit on the silhouette — notch, inner shoulder,
 * lobe, outer edge, waist, and the bottom point — which is what lets a lobe read as a curve rather
 * than one flat diagonal.
 *
 * The two centre-line points stay at x=469, so the mirror leaves the same 19-unit crease down the
 * middle that the kabuto has. It is the only seam in the heart: wider gaps between the facets were
 * tried and they shatter the shape rather than folding it.
 */
const HEART: readonly Polygon[] = [
  [[368, 50], [469, 252], [469, 690]], // peak → the inner wedge, notch down to the point
  [[72, 160], [368, 50], [469, 690], [50, 332]], // wing → the body of the lobe
  [[72, 160], [200, -8], [368, 50]], // blade → the cap of the lobe
];

const SHAPES = { kabuto: KABUTO, heart: HEART };

/** What the sheet can refold into. `kabuto` is the mark itself — its own resting shape. */
export type MarkShape = keyof typeof SHAPES;

/** Brand blue, sampled from the artwork. Flat, not a gradient: the mark reads as folded paper,
 *  and a gradient across a facet fights the fold it is meant to describe. */
export const MARK_BLUE = "#004CFC";

function pathOf(poly: Polygon): string {
  return `M${poly.map(([x, y]) => `${x} ${y}`).join("L")}Z`;
}

/** Facet `i` of the kabuto, `t` of the way to the same facet of `shape`. */
function blend(shape: MarkShape, i: number, t: number): string {
  const from = KABUTO[i];
  const to = SHAPES[shape][i];
  return pathOf(from.map(([x, y], j) => [x + (to[j][0] - x) * t, y + (to[j][1] - y) * t] as Point));
}

/** The mark itself, without a plate, in its native 957x614 space. */
export function MarkFacets({ fill = MARK_BLUE }: { fill?: string }): JSX.Element {
  const half = KABUTO.map((poly, i) => (
    <path
      key={PARTS[i]}
      className={`shogun-mark__facet shogun-mark__facet--${PARTS[i]}`}
      d={pathOf(poly)}
    />
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

const REDUCE_MOTION = "(prefers-reduced-motion: reduce)";

function prefersStillness(): boolean {
  return window.matchMedia?.(REDUCE_MOTION)?.matches ?? false;
}

/**
 * Marks the element whose hover drives a refold. Put it on the thing a person would actually aim
 * at — the brand row, the badge, the card — and the mark inside it answers for the whole thing.
 * A 20px mark beside its wordmark is a hard target, and the pair reads as one brand anyway.
 *
 * A mark with no such ancestor answers for itself, which is the right default for a large one
 * standing alone.
 */
export const MARK_HOVER_HOST = "[data-mark-hover]";

/**
 * Refolds the mark into `shape` while the pointer is over its host, and back when it leaves.
 *
 * Exported because not every mark in the app is an {@link AnimatedLogo}: the daily card draws
 * {@link MarkFacets} into its own box at the artwork's native aspect, and refolds the same way.
 * Pass a ref to the `<svg>` that contains the facets.
 *
 * Imperative on purpose. React owns `d` at mount and then never writes it again — the attribute
 * only gets patched when the prop changes, and it never does — so the tween can hold the DOM for
 * the length of a hover without re-rendering the tree sixty times a second.
 *
 * Reversal picks up wherever the tween had got to and shortens itself to match, so flicking the
 * pointer across the mark reads as the paper springing back rather than snapping.
 */
export function useMarkRefold(
  svgRef: React.RefObject<SVGSVGElement>,
  shape: MarkShape | undefined,
  hoverWithin: string = MARK_HOVER_HOST,
): void {
  const frame = useRef(0);
  const at = useRef(0);

  const draw = useCallback(
    (t: number) => {
      const svg = svgRef.current;
      if (!svg || !shape) return;
      PARTS.forEach((part, i) => {
        const d = blend(shape, i, t);
        // Both halves: the mirror is a transform, so the two sides share one path string.
        svg.querySelectorAll(`.shogun-mark__facet--${part}`).forEach((p) => p.setAttribute("d", d));
      });
    },
    [shape, svgRef],
  );

  const run = useCallback(
    (to: 0 | 1) => {
      // A shape that changes under the pointer is decoration, and decoration is exactly what
      // reduced motion asks for less of. Hold the mark still rather than snapping it.
      if (!shape || prefersStillness()) return;
      const from = at.current;
      const span = Math.abs(to - from);
      if (span === 0) return;
      const ms = (to === 1 ? 420 : 340) * span;
      const start = performance.now();
      cancelAnimationFrame(frame.current);
      const step = (now: number): void => {
        const k = Math.min(1, (now - start) / ms);
        at.current = from + (to - from) * (1 - (1 - k) ** 3); // ease-out, to match the fold's settle
        draw(at.current);
        if (k < 1) frame.current = requestAnimationFrame(step);
      };
      frame.current = requestAnimationFrame(step);
    },
    [draw, shape],
  );

  useEffect(() => {
    const svg = svgRef.current;
    if (!svg || !shape) return;
    const host: Element = svg.closest(hoverWithin) ?? svg;
    const enter = (): void => run(1);
    const leave = (): void => run(0);
    host.addEventListener("pointerenter", enter);
    host.addEventListener("pointerleave", leave);
    return () => {
      host.removeEventListener("pointerenter", enter);
      host.removeEventListener("pointerleave", leave);
      cancelAnimationFrame(frame.current);
    };
  }, [hoverWithin, run, shape, svgRef]);
}

/**
 * {@link Logo}, folding.
 *
 * `interactive` adds a fold under the pointer, which composes with `unfold` (that one hands the
 * transform back when it finishes) but not with the two looping motions.
 *
 * `morphTo` refolds the whole mark into another shape under the pointer — `heart` is the one that
 * exists. It replaces `interactive` rather than stacking with it: both answer the pointer, and two
 * answers to one gesture is one too many.
 *
 * The hover host is the nearest ancestor carrying {@link MARK_HOVER_HOST}, or the mark itself if
 * there is none; `hoverWithin` overrides the selector.
 *
 * Restarting the fold is the caller's job, and React already has the verb for it: change the
 * element's `key` and the animation runs again from a fresh node.
 */
export function AnimatedLogo({
  size = 26,
  motion = "unfold",
  interactive = false,
  morphTo,
  hoverWithin,
  className,
}: {
  size?: number;
  motion?: MarkMotion;
  interactive?: boolean;
  morphTo?: MarkShape;
  /** Overrides which ancestor's hover drives the refold. See {@link MARK_HOVER_HOST}. */
  hoverWithin?: string;
  className?: string;
}): JSX.Element {
  const svgRef = useRef<SVGSVGElement>(null);
  useMarkRefold(svgRef, morphTo, hoverWithin);

  const classes = [
    "shogun-mark",
    motion === "static" ? null : `shogun-mark--${motion}`,
    interactive && !morphTo ? "shogun-mark--interactive" : null,
    className,
  ].filter(Boolean);

  return (
    <svg
      ref={svgRef}
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
