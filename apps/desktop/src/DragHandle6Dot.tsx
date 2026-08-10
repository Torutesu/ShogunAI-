// Reusable 6-dot grabber for floating overlay drag (meeting asset: grip.svg).

import type { JSX, PointerEventHandler } from "react";

import gripIcon from "./assets/meeting/grip.svg";

export interface DragHandle6DotProps {
  title?: string;
  className?: string;
  onPointerDown?: PointerEventHandler<HTMLDivElement>;
}

/** Compact pill with the meeting grip SVG — wire `onPointerDown` to window drag. */
export function DragHandle6Dot(props: DragHandle6DotProps): JSX.Element {
  const { title, className, onPointerDown } = props;
  return (
    <div
      className={`drag-handle-6dot${className ? ` ${className}` : ""}`}
      title={title}
      aria-label={title}
      role="presentation"
      onPointerDown={onPointerDown}
    >
      <img
        className="drag-handle-6dot__img"
        src={gripIcon}
        alt=""
        width={18}
        height={18}
        draggable={false}
        aria-hidden
      />
    </div>
  );
}
