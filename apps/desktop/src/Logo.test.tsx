// The mark's motion is CSS (styles/logo-motion.css) keyed off classes the component puts on the
// facets. jsdom applies no stylesheet, so what is worth pinning here is the contract between the
// two: the part classes the fold rules hinge on, and the mode class that turns them on.
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { AnimatedLogo, Logo } from "./Logo";

afterEach(cleanup);

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
});
