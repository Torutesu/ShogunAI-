// Line-icon set. One stroke weight, one grid, one optical size — the panel used typographic
// glyphs (⚙︎ ▁ ✕ ✎ ↑ ⌄) whose weight and baseline came from whatever font the system picked, so
// no two controls in a row ever looked like they belonged to the same product.
//
// Rules: 20×20 grid, 1.6 stroke, round caps and joins, `currentColor` — colour and size are the
// caller's business. Icons are decorative here; every control that uses one carries its own
// aria-label, so each <svg> is aria-hidden.

import type { JSX } from "react";

export type IconName =
  | "settings"
  | "minimize"
  | "close"
  | "send"
  | "draft"
  | "chevron"
  | "check"
  | "plus"
  | "moon"
  | "shield"
  | "arrow"
  | "back";

/** Path data on a 20×20 grid, stroked. */
const PATHS: Record<IconName, JSX.Element> = {
  // Sliders, not a cogwheel: a gear's teeth turn to mush at 17px, and this panel's settings are
  // a short list of choices, which is what sliders read as.
  settings: (
    <>
      <path d="M3.6 6.6h12.8M3.6 13.4h12.8" />
      <circle cx="7.6" cy="6.6" r="2" />
      <circle cx="12.4" cy="13.4" r="2" />
    </>
  ),
  minimize: <path d="M5 13.5h10" />,
  close: <path d="M5.6 5.6l8.8 8.8M14.4 5.6l-8.8 8.8" />,
  send: <path d="M10 15.6V4.8M5.4 9.2L10 4.6l4.6 4.6" />,
  draft: (
    <>
      <path d="M13.4 3.9l2.7 2.7-8.2 8.2-3.4.7.7-3.4 8.2-8.2z" />
      <path d="M11.9 5.4l2.7 2.7" />
    </>
  ),
  chevron: <path d="M6 8.4l4 4 4-4" />,
  check: <path d="M4.8 10.4l3.4 3.4 7-7.6" />,
  plus: <path d="M10 4.8v10.4M4.8 10h10.4" />,
  moon: <path d="M16 11.7A6.6 6.6 0 018.3 4a6.6 6.6 0 107.7 7.7z" />,
  shield: (
    <>
      <path d="M10 2.9l5.4 2.2v4.2c0 3.3-2.2 6.3-5.4 7.8-3.2-1.5-5.4-4.5-5.4-7.8V5.1L10 2.9z" />
      <path d="M7.7 9.9l1.7 1.7 3.1-3.4" />
    </>
  ),
  arrow: <path d="M4.6 10h10.2M10.4 5.6L14.8 10l-4.4 4.4" />,
  back: <path d="M12 5.6L7.6 10l4.4 4.4" />,
};

export function Icon(props: { name: IconName; size?: number }): JSX.Element {
  const s = props.size ?? 17;
  return (
    <svg
      className="ico"
      width={s}
      height={s}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {PATHS[props.name]}
    </svg>
  );
}
