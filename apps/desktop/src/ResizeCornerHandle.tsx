// Reusable bottom-right corner resize grip for floating overlays.
// Meeting asset: resize.svg (diagonal collapse arrows). Opacity 0 until parent hover
// (see `.resize-corner-handle` + parent `:hover` in styles.css).

import type { JSX } from "react";

import resizeIcon from "./assets/meeting/resize.svg";
import { usePointerResize, type ResizeAnchor, type Size2 } from "./usePointerResize";

export interface ResizeCornerHandleProps {
  getSize: () => Size2;
  onResize: (w: number, h: number) => void;
  onCommit?: () => void;
  anchor?: ResizeAnchor;
  min?: Size2;
  max?: Size2;
  title?: string;
  className?: string;
}

/**
 * Bottom-right resize handle. Wire `onResize` to Tauri `setSize` / overlay size commands.
 * Parent should use `:hover .resize-corner-handle { opacity: 1 }` so it stays hidden at rest.
 */
export function ResizeCornerHandle(props: ResizeCornerHandleProps): JSX.Element {
  const { getSize, onResize, onCommit, anchor, min, max, title, className } = props;
  const handlers = usePointerResize({ getSize, onResize, onCommit, anchor, min, max });

  return (
    <div
      className={`resize-corner-handle${className ? ` ${className}` : ""}`}
      data-no-drag
      title={title}
      aria-label={title}
      role="separator"
      aria-orientation="horizontal"
      onPointerDown={handlers.onPointerDown}
      onPointerMove={handlers.onPointerMove}
      onPointerUp={handlers.onPointerUp}
      onPointerCancel={handlers.onPointerCancel}
    >
      <img
        className="resize-corner-handle__glyph"
        src={resizeIcon}
        alt=""
        width={14}
        height={14}
        draggable={false}
        aria-hidden
      />
    </div>
  );
}
