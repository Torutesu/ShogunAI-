// #122: resize IPC is rAF-batched — a burst of pointer moves within one frame delivers exactly
// one onResize (the latest), and pointer-up flushes the pending size before onCommit so the
// committed window is never one move behind the pointer.
import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useRef } from "react";

import { usePointerResize, type Size2 } from "./usePointerResize";

let rafQueue: FrameRequestCallback[] = [];

beforeEach(() => {
  rafQueue = [];
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
    rafQueue.push(cb);
    return rafQueue.length;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => {
    rafQueue[id - 1] = () => undefined;
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function frame(): void {
  const q = rafQueue;
  rafQueue = [];
  for (const cb of q) cb(performance.now());
}

function Harness({
  onResize,
  onCommit,
}: {
  onResize: (w: number, h: number) => void;
  onCommit?: () => void;
}): React.ReactElement {
  const size = useRef<Size2>({ w: 300, h: 200 });
  const handlers = usePointerResize({
    getSize: () => size.current,
    onResize: (w, h) => {
      size.current = { w, h };
      onResize(w, h);
    },
    onCommit,
    min: { w: 100, h: 100 },
    max: { w: 800, h: 800 },
  });
  return <div data-testid="grip" {...handlers} />;
}

function pointer(type: string, x: number, y: number): PointerEvent {
  // jsdom lacks PointerEvent; a MouseEvent with the fields the hook reads is enough.
  const e = new MouseEvent(type, { bubbles: true, button: 0, screenX: x, screenY: y });
  Object.defineProperty(e, "pointerId", { value: 1 });
  return e as unknown as PointerEvent;
}

function grip(container: HTMLElement): HTMLElement {
  const el = container.querySelector("[data-testid=grip]") as HTMLElement;
  el.setPointerCapture = () => undefined;
  el.releasePointerCapture = () => undefined;
  return el;
}

describe("usePointerResize", () => {
  it("delivers one onResize per frame, with the latest size", () => {
    const sizes: Array<[number, number]> = [];
    const { container } = render(<Harness onResize={(w, h) => sizes.push([w, h])} />);
    const el = grip(container);

    act(() => {
      el.dispatchEvent(pointer("pointerdown", 0, 0));
      for (let i = 1; i <= 30; i++) {
        el.dispatchEvent(pointer("pointermove", i, i)); // 30 moves, no frame yet
      }
    });
    expect(sizes).toHaveLength(0);

    act(() => frame());
    expect(sizes).toEqual([[330, 230]]); // only the last move landed
  });

  it("flushes the pending size before commit on pointer-up", () => {
    const events: string[] = [];
    const { container } = render(
      <Harness
        onResize={(w, h) => events.push(`resize:${w}x${h}`)}
        onCommit={() => events.push("commit")}
      />,
    );
    const el = grip(container);

    act(() => {
      el.dispatchEvent(pointer("pointerdown", 0, 0));
      el.dispatchEvent(pointer("pointermove", 50, 10));
      el.dispatchEvent(pointer("pointerup", 50, 10)); // no frame ran in between
    });
    expect(events).toEqual(["resize:350x210", "commit"]);
  });

  it("clamps to min/max while batching", () => {
    const sizes: Array<[number, number]> = [];
    const { container } = render(<Harness onResize={(w, h) => sizes.push([w, h])} />);
    const el = grip(container);
    act(() => {
      el.dispatchEvent(pointer("pointerdown", 0, 0));
      el.dispatchEvent(pointer("pointermove", 5_000, -5_000));
    });
    act(() => frame());
    expect(sizes).toEqual([[800, 100]]);
  });
});
