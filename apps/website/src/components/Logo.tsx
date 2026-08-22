/**
 * ShogunAI's official folded-paper kabuto mark. This is the same geometry and
 * brand blue used by the macOS app icon in apps/desktop/src-tauri/icons/icon.svg
 * and by the app's own Logo.tsx. One brand, one mark: if any of them changes, all
 * of them must.
 *
 * Held as points rather than path text so the mark can be refolded into another
 * shape (see AnimatedLogo). The vertices are unchanged.
 */

/** Which fold each facet is, in order. Rides along as a class so the refold can find its pair. */
export const MARK_PARTS = ['peak', 'wing', 'blade'] as const;

export type MarkPolygon = readonly (readonly [number, number])[];

/** Left half of the mark in its own 957x614 space. The right half is this mirrored. */
const KABUTO: readonly MarkPolygon[] = [
  [
    [296, 254],
    [469, 0],
    [469, 525],
  ],
  [
    [0, 101],
    [276, 264],
    [446, 524],
    [176, 390],
  ],
  [
    [62, 613],
    [171, 413],
    [331, 493],
  ],
];

/**
 * The same sheet, refolded into a heart: the same three facets with the same vertex counts in the
 * same order, so every point has somewhere to travel and the facets stay welded to each other the
 * whole way across. The two centre-line points stay at x=469, so the mirror leaves the same
 * 19-unit crease down the middle that the kabuto has.
 */
const HEART: readonly MarkPolygon[] = [
  [
    [368, 50],
    [469, 252],
    [469, 690],
  ],
  [
    [72, 160],
    [368, 50],
    [469, 690],
    [50, 332],
  ],
  [
    [72, 160],
    [200, -8],
    [368, 50],
  ],
];

/** What the sheet can be folded into. `kabuto` is the mark itself. */
export const MARK_SHAPES = { kabuto: KABUTO, heart: HEART };

export type MarkShape = keyof typeof MARK_SHAPES;

export const MARK_BLUE = '#004CFC';

export function markPath(poly: MarkPolygon): string {
  return `M${poly.map(([x, y]) => `${x} ${y}`).join('L')}Z`;
}

export function MarkFacets({ fill = MARK_BLUE }: { fill?: string }) {
  const half = KABUTO.map((poly, i) => (
    <path
      key={MARK_PARTS[i]}
      className={`shogun-mark__facet--${MARK_PARTS[i]}`}
      d={markPath(poly)}
    />
  ));
  return (
    <g fill={fill}>
      {half}
      <g transform="translate(957,0) scale(-1,1)">{half}</g>
    </g>
  );
}

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
      <g transform="translate(0 171.5)">
        <MarkFacets />
      </g>
    </svg>
  );
}

/** The official mark is self-contained, so no shared SVG definitions are needed. */
export function LogoDefs() {
  return null;
}
