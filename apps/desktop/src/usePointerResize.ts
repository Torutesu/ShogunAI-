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
  // rAF batching (#122): pointer events arrive faster than frames, and each onResize crosses the
  // IPC bridge to move a native window. Only the latest size within a frame matters — the
  // intermediate ones would be painted over before anyone saw them — so moves park the size here
  // and one rAF callback per frame delivers it.
  const pending = useRef<Size2 | null>(null);
  const rafId = useRef<number | null>(null);

  const flush = useCallback((): void => {
    rafId.current = null;
    const next = pending.current;
    if (!next) return;
    pending.current = null;
    optsRef.current.onResize(next.w, next.h);
  }, []);

  const onPointerDown = useCallback((e: ReactPointerEvent): void => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const s = optsRef.current.getSize();
    start.current = { x: e.screenX, y: e.screenY, w: s.w, h: s.h };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }, []);

  const onPointerMove = useCallback(
    (e: ReactPointerEvent): void => {
      const s = start.current;
      if (!s) return;
      const { anchor = "top-left", min, max } = optsRef.current;
      const dx = e.screenX - s.x;
      const dy = e.screenY - s.y;
      const rawW = anchor === "center" ? s.w + 2 * dx : s.w + dx;
      const rawH = s.h + dy;
      pending.current = clampSize(rawW, rawH, min, max);
      if (rafId.current === null) {
        rafId.current = window.requestAnimationFrame(flush);
      }
    },
    [flush],
  );

  const end = useCallback(
    (e: ReactPointerEvent): void => {
      if (!start.current) return;
      start.current = null;
      // Deliver the final size before committing — the commit persists whatever the window is,
      // and a pending frame dropped here would commit one move behind the pointer.
      if (rafId.current !== null) {
        window.cancelAnimationFrame(rafId.current);
      }
      flush();
      optsRef.current.onCommit?.();
      try {
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
      } catch {
        /* already released */
      }
    },
    [flush],
  );

  return {
    onPointerDown,
    onPointerMove: onPointerMove,
    onPointerUp: end,
    onPointerCancel: end,
  };
}
