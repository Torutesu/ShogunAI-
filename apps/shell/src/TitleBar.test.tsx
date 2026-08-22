import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TitleBar } from "./TitleBar";

describe("TitleBar", () => {
  it("places Windows caption buttons on the right in min / max / close order", () => {
    render(<TitleBar maximized={false} />);
    const buttons = screen.getAllByRole("button");
    expect(buttons.map((b) => b.getAttribute("aria-label"))).toEqual([
      "Minimize",
      "Maximize",
      "Close",
    ]);
    const group = screen.getByRole("group", { name: "Window" });
    expect(group.className).toContain("titlebar__controls");
  });

  it("relabels maximize as restore when the window is maximized", () => {
    render(<TitleBar maximized />);
    expect(screen.getByRole("button", { name: "Restore" })).toBeInTheDocument();
  });
});
