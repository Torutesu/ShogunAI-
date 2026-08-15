// #122's structural claim, verified: audio levels drive the DOM directly — a burst of
// meeting_level events causes ZERO React re-renders, and silence hands the glyph back to the CSS
// idle animation.
import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { barHeightPct, WaveBars } from "./WaveBars";

type LevelHandler = (e: { payload: { rms: number } }) => void;

const handlers: LevelHandler[] = [];

beforeEach(async () => {
  vi.useFakeTimers();
  handlers.length = 0;
  const { listen } = await import("@tauri-apps/api/event");
  vi.mocked(listen).mockImplementation(async (event: string, cb: unknown) => {
    if (event === "meeting_level") handlers.push(cb as LevelHandler);
    return () => undefined;
  });
});

afterEach(() => {
  vi.useRealTimers();
});

function bars(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll(".ov__wave-bar")) as HTMLElement[];
}

describe("WaveBars", () => {
  it("writes levels to the DOM without re-rendering React", async () => {
    let renders = 0;
    function Counted(): React.ReactElement {
      renders++;
      return <WaveBars active />;
    }
    const { container } = render(<Counted />);
    await act(async () => {}); // let the listen() promise resolve
    const before = renders;
    expect(handlers).toHaveLength(1);

    act(() => {
      for (let i = 0; i < 120; i++) {
        handlers[0]({ payload: { rms: 0.2 + (i % 10) / 20 } });
      }
    });

    expect(renders).toBe(before);
    for (const bar of bars(container)) {
      expect(bar.style.getPropertyValue("--h")).toMatch(/%$/);
    }
  });

  it("returns to the CSS idle pulse after silence", async () => {
    const { container } = render(<WaveBars active />);
    await act(async () => {});
    act(() => {
      handlers[0]({ payload: { rms: 0.8 } });
    });
    expect(bars(container)[0].style.getPropertyValue("--h")).not.toBe("");

    act(() => {
      vi.advanceTimersByTime(1_600);
    });
    for (const bar of bars(container)) {
      expect(bar.style.getPropertyValue("--h")).toBe("");
    }
  });

  it("does not subscribe while inactive, and clears heights when deactivated", async () => {
    const { container, rerender } = render(<WaveBars active={false} />);
    await act(async () => {});
    expect(handlers).toHaveLength(0);

    rerender(<WaveBars active />);
    await act(async () => {});
    expect(handlers).toHaveLength(1);
    act(() => {
      handlers[0]({ payload: { rms: 0.9 } });
    });
    expect(bars(container)[0].style.getPropertyValue("--h")).not.toBe("");

    rerender(<WaveBars active={false} />);
    expect(bars(container)[0].style.getPropertyValue("--h")).toBe("");
  });
});

describe("barHeightPct", () => {
  it("stays inside the glyph's 18..100% envelope for any level", () => {
    for (const level of [0, 0.25, 0.5, 0.75, 1]) {
      for (let i = 0; i < 5; i++) {
        const h = barHeightPct(0.95, i, level);
        expect(h).toBeGreaterThanOrEqual(18);
        expect(h).toBeLessThanOrEqual(100);
      }
    }
  });
});
