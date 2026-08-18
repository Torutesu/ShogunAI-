import { describe, expect, it } from "vitest";

import { panelSizeForView } from "./App";

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
});
