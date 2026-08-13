// Shared pointer-drag resize for borderless Tauri overlays (no OS resize edges).
// screenX/Y — client coords shift when the window moves/resizes mid-drag.

import { useCallback, useRef, type PointerEvent as ReactPointerEvent } from "react";

export type ResizeAnchor = "top-left" | "center";

export interface Size2 {
  w: number;
  h: number;
}

export interface PointerResizeOptions {
  getSize: () => Size2;
  onResize: (w: number, h: number) => void;
  onCommit?: () => void;
  /** top-left: grow down/right (Δw=dx, Δh=dy). center: keep horizontal centre (Δw=2·dx). */
  anchor?: ResizeAnchor;
  min?: Size2;
  max?: Size2;
}

function clampSize(w: number, h: number, min?: Size2, max?: Size2): Size2 {
  let nw = w;
  let nh = h;
  if (min) {
    nw = Math.max(min.w, nw);
    nh = Math.max(min.h, nh);
  }
  if (max) {
    nw = Math.min(max.w, nw);
    nh = Math.min(max.h, nh);
  }
  return { w: Math.round(nw), h: Math.round(nh) };
}

/** Hook: pointer handlers that map a corner drag to width/height updates. */
export function usePointerResize(opts: PointerResizeOptions): {
  onPointerDown: (e: ReactPointerEvent) => void;
  onPointerMove: (e: ReactPointerEvent) => void;
  onPointerUp: (e: ReactPointerEvent) => void;
  onPointerCancel: (e: ReactPointerEvent) => void;
} {
  const optsRef = useRef(opts);
  optsRef.current = opts;
  const start = useRef<{ x: number; y: number; w: number; h: number } | null>(null);

  const onPointerDown = useCallback((e: ReactPointerEvent): void => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const s = optsRef.current.getSize();
    start.current = { x: e.screenX, y: e.screenY, w: s.w, h: s.h };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }, []);

  const onPointerMove = useCallback((e: ReactPointerEvent): void => {
    const s = start.current;
    if (!s) return;
    const { anchor = "top-left", min, max, onResize } = optsRef.current;
    const dx = e.screenX - s.x;
    const dy = e.screenY - s.y;
    const rawW = anchor === "center" ? s.w + 2 * dx : s.w + dx;
    const rawH = s.h + dy;
    const next = clampSize(rawW, rawH, min, max);
    onResize(next.w, next.h);
  }, []);

  const end = useCallback((e: ReactPointerEvent): void => {
    if (!start.current) return;
    start.current = null;
    optsRef.current.onCommit?.();
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }
  }, []);

  return {
    onPointerDown,
    onPointerMove: onPointerMove,
    onPointerUp: end,
    onPointerCancel: end,
  };
}
