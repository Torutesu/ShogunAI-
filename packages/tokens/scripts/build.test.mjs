import { test } from "node:test";
import assert from "node:assert/strict";
import { validate } from "./build.mjs";
import { generateCss } from "./build.mjs";

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
  assert.ok(errors.some((e) => e.includes("accent") && e.includes("dark") && e.includes("notacolor")));
});

const sample = {
  static: { "r-sm": "10px", "accent-soft": "color-mix(in srgb, var(--accent) 26%, transparent)" },
  themed: {
    glass:  { dark: "rgba(21, 24, 31, 0.85)", light: "rgba(249, 250, 252, 0.92)" },
    accent: { dark: "#6ea8fe", light: "#2f6fed" },
  },
};

test("generateCss emits base :root with static + dark themed values", () => {
  const css = generateCss(sample);
  assert.match(css, /:root\s*\{[^}]*--r-sm:\s*10px/);
  assert.match(css, /:root\s*\{[^}]*--accent-soft:\s*color-mix/);
  assert.match(css, /:root\s*\{[^}]*--glass:\s*rgba\(21, 24, 31, 0\.85\)/);
});

test("generateCss emits the light appearance block", () => {
  const css = generateCss(sample);
  assert.match(css, /:root\[data-appearance="light"\]\s*\{[^}]*--accent:\s*#2f6fed/);
});

test("generateCss emits the auto media query with light values", () => {
  const css = generateCss(sample);
  assert.match(css, /@media \(prefers-color-scheme: light\)\s*\{\s*:root\[data-appearance="auto"\]\s*\{[^}]*--glass:\s*rgba\(249, 250, 252, 0\.92\)/);
});

test("generateCss does NOT repeat static tokens in the light block", () => {
  const css = generateCss(sample);
  const lightBlock = css.slice(css.indexOf('[data-appearance="light"]'));
  assert.doesNotMatch(lightBlock.slice(0, lightBlock.indexOf("}")), /--r-sm/);
});

test("validate passes for well-formed web tokens (incl. non-color values)", () => {
  const good = {
    static: {}, themed: {},
    web: { themed: {
      bg: { light: "#ffffff", dark: "#090b0d" },
      "orb-blend": { light: "multiply", dark: "screen" },
      "orb-opacity": { light: "0.55", dark: "0.4" },
    } },
  };
  assert.deepEqual(validate(good), []);
});

test("validate flags a web token missing a mode", () => {
  const bad = { static: {}, themed: {}, web: { themed: { bg: { light: "#ffffff" } } } };
  const errors = validate(bad);
  assert.ok(errors.some((e) => e.includes("bg") && e.includes("dark") && e.includes("web")));
});

import { generateTs, main } from "./build.mjs";
import { readFileSync as _read, existsSync } from "node:fs";
import { resolve as _resolve } from "node:path";
import { dirname as _dirname } from "node:path";
import { fileURLToPath as _ftu } from "node:url";
const PKG_ROOT = _resolve(_dirname(_ftu(import.meta.url)), "..");

test("generateTs emits a typed const with themed + static values", () => {
  const ts = generateTs(sample);
  assert.match(ts, /export const tokens = \{/);
  assert.match(ts, /as const/);
  assert.match(ts, /"accent"/);
  assert.match(ts, /export type TokenName/);
});

test("main writes dist/tokens.css and dist/tokens.ts", () => {
  main();
  assert.ok(existsSync(_resolve(PKG_ROOT, "dist/tokens.css")));
  assert.ok(existsSync(_resolve(PKG_ROOT, "dist/tokens.ts")));
  const css = _read(_resolve(PKG_ROOT, "dist/tokens.css"), "utf8");
  assert.match(css, /--glass: rgba\(21, 24, 31, 0\.85\)/);
});
