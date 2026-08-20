import { describe, expect, it } from "vitest";
import { chromeContainsBanned, copy } from "./strings";

describe("chrome copy", () => {
  it("does not use banned brand or stack words", () => {
    for (const value of Object.values(copy)) {
      expect(chromeContainsBanned(value)).toBe(false);
    }
  });
});
