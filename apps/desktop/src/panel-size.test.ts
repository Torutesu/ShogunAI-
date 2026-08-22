import { describe, expect, it } from "vitest";

import { compactCssMetrics, panelSizeForView } from "./App";

describe("panel view sizing", () => {
  const resizedMain = { w: 742, h: 518 };
  const overview = { w: 610, h: 560 };

  it("keeps Settings at the resized main-window size", () => {
    expect(panelSizeForView("settings", resizedMain, overview)).toEqual(resizedMain);
    expect(panelSizeForView("chat", resizedMain, overview)).toEqual(resizedMain);
  });

  it("keeps Overview as the only separately sized workspace", () => {
    expect(panelSizeForView("hub", resizedMain, overview)).toEqual(overview);
  });

  it("fits an external-display pseudo-notch inside the menu bar", () => {
    expect(compactCssMetrics({ is_notch: false, notch_height: 24, handle_height: 24 })).toEqual({
      deadHeight: 0,
      rowHeight: 24,
      pillHeight: 24,
    });
  });

  it("keeps hardware-notch content below the cutout", () => {
    expect(compactCssMetrics({ is_notch: true, notch_height: 38, handle_height: 82 })).toEqual({
      deadHeight: 38,
      rowHeight: 44,
      pillHeight: 38,
    });
  });
});
