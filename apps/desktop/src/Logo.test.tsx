// The mark's motion is CSS (styles/logo-motion.css) keyed off classes the component puts on the
// facets. jsdom applies no stylesheet, so what is worth pinning here is the contract between the
// two: the part classes the fold rules hinge on, and the mode class that turns them on.
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AnimatedLogo, Logo } from "./Logo";

/* The refold runs on rAF against a real clock, so the tests drive both. */
let now = 0;
let queued: FrameRequestCallback[] = [];

beforeEach(() => {
  now = 0;
  queued = [];
  vi.spyOn(performance, "now").mockImplementation(() => now);
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => queued.push(cb));
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

/** Run the tween out. 60ms a frame clears any duration this component uses well inside the cap. */
function settle(): void {
  for (let i = 0; i < 100 && queued.length > 0; i += 1) {
    const batch = queued;
    queued = [];
    now += 60;
    act(() => batch.forEach((cb) => cb(now)));
  }
}

/** Every path drawing `part`, on both halves of the mark. */
function drawn(root: HTMLElement, part: string): (string | null)[] {
  return [...root.querySelectorAll(`.shogun-mark__facet--${part}`)].map((p) => p.getAttribute("d"));
}

/** Both halves of the mark, so the mirror is covered too: three facets each. */
function facets(root: HTMLElement, part: string): Element[] {
  return [...root.querySelectorAll(`.shogun-mark__facet--${part}`)];
}

describe("the mark", () => {
  it("names every facet's fold, on both halves", () => {
    const { container } = render(<AnimatedLogo />);
    for (const part of ["peak", "wing", "blade"]) {
      expect(facets(container, part)).toHaveLength(2);
    }
  });

  it("carries the part classes into the static logo too, so one stylesheet serves both", () => {
    const { container } = render(<Logo />);
    expect(facets(container, "wing")).toHaveLength(2);
  });

  it("unfolds by default", () => {
    const { container } = render(<AnimatedLogo />);
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("class")).toContain("shogun-mark--unfold");
  });

  it("asks for exactly the motion it was given, and nothing for `static`", () => {
    for (const motion of ["idle", "thinking"] as const) {
      const { container, unmount } = render(<AnimatedLogo motion={motion} />);
      expect(container.querySelector("svg")?.getAttribute("class")).toContain(
        `shogun-mark--${motion}`,
      );
      unmount();
    }
    const { container } = render(<AnimatedLogo motion="static" />);
    const cls = container.querySelector("svg")?.getAttribute("class") ?? "";
    expect(cls).toContain("shogun-mark");
    expect(cls).not.toMatch(/shogun-mark--(unfold|idle|thinking)/);
  });

  it("adds the pointer fold only when asked, and keeps the caller's own class", () => {
    const { container } = render(<AnimatedLogo interactive className="side__mark" />);
    const cls = container.querySelector("svg")?.getAttribute("class") ?? "";
    expect(cls).toContain("shogun-mark--interactive");
    expect(cls).toContain("side__mark");
  });

  it("is one image to a screen reader, not six paths", () => {
    const { container } = render(<AnimatedLogo />);
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("role")).toBe("img");
    expect(svg?.getAttribute("aria-label")).toBe("ShogunAI");
  });

  it("draws the kabuto's own vertices until something asks it not to", () => {
    const { container } = render(<Logo />);
    expect(drawn(container, "peak")[0]).toBe("M296 254L469 0L469 525Z");
    expect(drawn(container, "wing")[0]).toBe("M0 101L276 264L446 524L176 390Z");
    expect(drawn(container, "blade")[0]).toBe("M62 613L171 413L331 493Z");
  });
});

describe("the refold", () => {
  it("carries the mark all the way to the heart, both halves in step", () => {
    const { container } = render(<AnimatedLogo motion="static" morphTo="heart" />);
    const svg = container.querySelector("svg") as SVGSVGElement;

    fireEvent.pointerOver(svg, { relatedTarget: document.body });
    settle();

    expect(drawn(container, "peak")).toEqual(["M368 50L469 252L469 690Z", "M368 50L469 252L469 690Z"]);
    expect(drawn(container, "wing")[0]).toBe("M72 160L368 50L469 690L50 332Z");
    expect(drawn(container, "blade")[0]).toBe("M72 160L200 -8L368 50Z");
  });

  it("comes back to the mark when the pointer leaves", () => {
    const { container } = render(<AnimatedLogo motion="static" morphTo="heart" />);
    const svg = container.querySelector("svg") as SVGSVGElement;

    fireEvent.pointerOver(svg, { relatedTarget: document.body });
    settle();
    fireEvent.pointerOut(svg, { relatedTarget: document.body });
    settle();

    expect(drawn(container, "peak")[0]).toBe("M296 254L469 0L469 525Z");
  });

  it("leaves the sheet alone when the viewer asked for less motion", () => {
    vi.spyOn(window, "matchMedia").mockImplementation(
      (q: string) => ({ matches: q.includes("reduced-motion") }) as MediaQueryList,
    );
    const { container } = render(<AnimatedLogo motion="static" morphTo="heart" />);
    const svg = container.querySelector("svg") as SVGSVGElement;

    fireEvent.pointerOver(svg, { relatedTarget: document.body });
    settle();

    expect(drawn(container, "peak")[0]).toBe("M296 254L469 0L469 525Z");
  });

  it("takes over the pointer from the hover fold rather than stacking with it", () => {
    const { container } = render(<AnimatedLogo interactive morphTo="heart" />);
    const cls = container.querySelector("svg")?.getAttribute("class") ?? "";
    expect(cls).not.toContain("shogun-mark--interactive");
  });
});