'use client';

import { useCallback, useEffect, useRef } from 'react';
import { MARK_PARTS, MARK_SHAPES, MarkFacets, markPath, type MarkShape } from '@/components/Logo';

/**
 * The mark, refolded into another shape while the pointer is on it.
 *
 * The heart is not a second drawing cross-faded to: it is the same three facets and the same ten
 * vertices walked to new positions, so the facets stay welded to each other the whole way across
 * and the crease down the middle survives the trip.
 *
 * Interpolated on requestAnimationFrame rather than declared in CSS, because `d` is not reliably
 * animatable as a CSS property across the browsers this ships to. React writes `d` once at mount
 * and never again — the attribute is only patched when the prop changes, and it never does — so
 * the tween can hold the DOM for the length of a hover without re-rendering.
 *
 * Reversal picks up wherever the tween had got to and shortens itself to match, so flicking the
 * pointer across the mark reads as the paper springing back rather than snapping.
 */
export function AnimatedLogo({
  size = 26,
  shape = 'heart',
  hoverWithin,
  className,
}: {
  size?: number;
  /** What the mark folds into under the pointer. */
  shape?: MarkShape;
  /**
   * Selector for the ancestor whose hover drives the refold. The nav's mark sits inside the brand
   * link and should answer to the whole link, not to its own 26 pixels. Defaults to the mark.
   */
  hoverWithin?: string;
  className?: string;
}) {
  const svgRef = useRef<SVGSVGElement>(null);
  const frame = useRef(0);
  const at = useRef(0);

  const draw = useCallback(
    (t: number) => {
      const svg = svgRef.current;
      if (!svg) return;
      MARK_PARTS.forEach((part, i) => {
        const from = MARK_SHAPES.kabuto[i];
        const to = MARK_SHAPES[shape][i];
        const d = markPath(
          from.map(([x, y], j) => [x + (to[j][0] - x) * t, y + (to[j][1] - y) * t] as const),
        );
        // Both halves: the mirror is a transform, so the two sides share one path string.
        svg.querySelectorAll(`.shogun-mark__facet--${part}`).forEach((p) => p.setAttribute('d', d));
      });
    },
    [shape],
  );

  const run = useCallback(
    (to: 0 | 1) => {
      // A shape that changes under the pointer is decoration, and decoration is what reduced
      // motion asks for less of. Hold the mark still rather than snapping it.
      if (window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches) return;
      const from = at.current;
      const span = Math.abs(to - from);
      if (span === 0) return;
      const ms = (to === 1 ? 420 : 340) * span;
      const start = performance.now();
      cancelAnimationFrame(frame.current);
      const step = (now: number) => {
        const k = Math.min(1, (now - start) / ms);
        at.current = from + (to - from) * (1 - (1 - k) ** 3);
        draw(at.current);
        if (k < 1) frame.current = requestAnimationFrame(step);
      };
      frame.current = requestAnimationFrame(step);
    },
    [draw],
  );

  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    const host: Element = hoverWithin ? (svg.closest(hoverWithin) ?? svg) : svg;
    const enter = () => run(1);
    const leave = () => run(0);
    host.addEventListener('pointerenter', enter);
    host.addEventListener('pointerleave', leave);
    return () => {
      host.removeEventListener('pointerenter', enter);
      host.removeEventListener('pointerleave', leave);
      cancelAnimationFrame(frame.current);
    };
  }, [hoverWithin, run]);

  return (
    <svg
      ref={svgRef}
      width={size}
      height={size}
      viewBox="0 0 957 957"
      className={className}
      role="img"
      aria-label="ShogunAI"
    >
      <g transform="translate(0 171.5)">
        <MarkFacets />
      </g>
    </svg>
  );
}
