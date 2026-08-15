// #120's bounds, verified with fake timers and mocked IPC: scrubbing cannot queue unbounded
// image loads, and a list refresh that changed nothing does not reload the selected image.
import { act, fireEvent, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ScrubBar, VisualRecallBrowse } from "./visual-recall";

let rafQueue: FrameRequestCallback[] = [];

beforeEach(() => {
  vi.useFakeTimers();
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
  vi.useRealTimers();
});

function frame(): void {
  const q = rafQueue;
  rafQueue = [];
  for (const cb of q) cb(performance.now());
}

function frameRow(id: number) {
  return {
    id,
    ts: 1_000 + id,
    app: "com.apple.Safari",
    window: "Docs",
    width: 100,
    height: 80,
    ocr_excerpt: `frame ${id}`,
    source: "screen_ocr",
  };
}

describe("ScrubBar", () => {
  function renderBar(onChange: (n: number) => void) {
    const frames = Array.from({ length: 50 }, (_, i) => frameRow(i + 1));
    const { container } = render(
      <ScrubBar frames={frames} value={25} label="t" onChange={onChange} />,
    );
    const bar = container.querySelector(".vr-scrub__viewport") as HTMLElement;
    return bar;
  }

  it("collapses a burst of wheel ticks into one accumulated change per frame", () => {
    const changes: number[] = [];
    const bar = renderBar((n) => changes.push(n));

    act(() => {
      for (let i = 0; i < 12; i++) {
        fireEvent.wheel(bar, { deltaY: 18, deltaX: 0 });
      }
    });
    expect(changes).toHaveLength(0); // nothing before the frame

    act(() => frame());
    expect(changes).toHaveLength(1);
    // 12 ticks accumulated against the pending value, not 12 re-reads of the same stale prop
    expect(changes[0]).toBeGreaterThan(26);
  });

  it("delivers at most one change per frame while dragging", () => {
    const changes: number[] = [];
    const bar = renderBar((n) => changes.push(n));
    bar.setPointerCapture = () => undefined;
    bar.releasePointerCapture = () => undefined;
    bar.hasPointerCapture = () => true;

    act(() => {
      fireEvent.pointerDown(bar, { button: 0, pointerId: 1, clientX: 200 });
      for (let i = 1; i <= 40; i++) {
        fireEvent.pointerMove(bar, { pointerId: 1, clientX: 200 - i * 4 });
      }
    });
    expect(changes).toHaveLength(0);
    act(() => frame());
    expect(changes).toHaveLength(1);
  });
});

describe("VisualRecallBrowse preview loading", () => {
  it("loads only the settled frame, and an unchanged refresh does not reload it", async () => {
    const imageCalls: number[] = [];
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "list_screen_frames") {
        // The API returns newest-first; Browse reverses to oldest→newest and centers on newest.
        return [frameRow(3), frameRow(2), frameRow(1)];
      }
      if (cmd === "get_screen_frame_image") {
        const id = (args as { frameId: number }).frameId;
        imageCalls.push(id);
        return {
          jpeg_base64: "aGk=",
          ocr_text: "hi",
          ts: 1_000 + id,
          app: null,
          window: null,
          source: "screen_ocr",
        };
      }
      return undefined;
    });

    render(<VisualRecallBrowse />);
    await act(async () => {}); // list resolves; selection centers on the newest frame (id 3)

    // The debounce means the initial selection has not fired yet…
    expect(imageCalls).toHaveLength(0);
    await act(async () => {
      vi.advanceTimersByTime(150);
    });
    expect(imageCalls).toEqual([3]);

    // The 12s list refresh returns identical rows — a NEW array, same ids. Keying the preview on
    // the selected id means no reload.
    await act(async () => {
      vi.advanceTimersByTime(12_100);
    });
    await act(async () => {
      vi.advanceTimersByTime(200);
    });
    expect(imageCalls).toEqual([3]);
  });
});
