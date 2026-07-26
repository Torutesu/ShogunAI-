import { test } from "node:test";
import assert from "node:assert/strict";
import { validate } from "./build.mjs";

const good = {
  static: { "r-sm": "10px", "accent-soft": "color-mix(in srgb, var(--accent) 26%, transparent)" },
  themed: { accent: { dark: "#6ea8fe", light: "#2f6fed" }, glass: { dark: "rgba(21, 24, 31, 0.85)", light: "rgba(249, 250, 252, 0.92)" } },
};

test("validate passes for well-formed tokens", () => {
  assert.deepEqual(validate(good), []);
});

test("validate flags a themed token missing a mode", () => {
  const bad = { static: {}, themed: { accent: { dark: "#6ea8fe" } } };
  const errors = validate(bad);
  assert.ok(errors.some((e) => e.includes("accent") && e.includes("light")));
});

test("validate flags an invalid color value", () => {
  const bad = { static: {}, themed: { accent: { dark: "notacolor", light: "#2f6fed" } } };
  const errors = validate(bad);
  assert.ok(errors.some((e) => e.includes("accent")));
});
