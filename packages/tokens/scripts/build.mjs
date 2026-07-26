import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const PKG = resolve(HERE, "..");

const COLOR_RE = /^(#[0-9a-fA-F]{3,8}|rgba?\([^)]*\)|color-mix\([^)]*\)|hsla?\([^)]*\))$/;
const MODES = ["dark", "light"];

/** @returns {string[]} list of human-readable errors (empty = valid) */
export function validate(tokens) {
  const errors = [];
  const themed = tokens.themed ?? {};
  for (const [name, byMode] of Object.entries(themed)) {
    for (const mode of MODES) {
      const v = byMode?.[mode];
      if (v == null) {
        errors.push(`themed token "${name}" is missing mode "${mode}"`);
        continue;
      }
      if (!COLOR_RE.test(String(v).trim())) {
        errors.push(`themed token "${name}" (${mode}) has an invalid color value: ${v}`);
      }
    }
  }
  return errors;
}
