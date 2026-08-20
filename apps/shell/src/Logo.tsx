// Same mark as apps/desktop/src/Logo.tsx and the marketing site. Keep the three in lockstep.

const FACETS = [
  "M296 254 L469 0 L469 525 Z",
  "M0 101 L276 264 L446 524 L176 390 Z",
  "M62 613 L171 413 L331 493 Z",
];

export const MARK_BLUE = "#004CFC";

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
      <g transform="translate(0 171.5)">
        <MarkFacets />
      </g>
    </svg>
  );
}
