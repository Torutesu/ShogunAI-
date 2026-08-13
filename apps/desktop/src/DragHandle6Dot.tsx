// Reusable 6-dot grabber for floating overlay drag (meeting asset: grip.svg).

import type { JSX, PointerEventHandler } from "react";

import gripIcon from "./assets/meeting/grip.svg";

export interface DragHandle6DotProps {
  title?: string;
  className?: string;
  onPointerDown?: PointerEventHandler<HTMLDivElement>;
  onPointerMove?: PointerEventHandler<HTMLDivElement>;
  onPointerUp?: PointerEventHandler<HTMLDivElement>;
  onPointerCancel?: PointerEventHandler<HTMLDivElement>;
}

/** Compact pill with the meeting grip SVG — wire pointer handlers to move a panel or window. */
export function DragHandle6Dot(props: DragHandle6DotProps): JSX.Element {
  const { title, className, onPointerDown, onPointerMove, onPointerUp, onPointerCancel } = props;
  return (
    <div
      className={`drag-handle-6dot${className ? ` ${className}` : ""}`}
      title={title}
      aria-label={title}
      role="presentation"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerCancel}
    >
      <img
        className="drag-handle-6dot__img"
        src={gripIcon}
        alt=""
        width={14}
        height={14}
        draggable={false}
        aria-hidden
      />
    </div>
  );
}
