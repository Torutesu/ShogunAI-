#!/usr/bin/env node
// Build the browser preview (apps/desktop/preview.html) into ONE self-contained HTML file.
//
// Why a single file: the preview is passed around for design review — dropped in a browser, sent
// to a reviewer, opened from a phone. A folder of hashed assets only works when it is served, and
// a file:// page with external <script type="module"> is blocked by CORS. Inlining sidesteps both.
//
// Usage:  node scripts/build-preview.mjs [--out <file>] [--fragment]
// Output: apps/desktop/dist-preview/shogun-preview.html (open it directly in any browser)
//
// --fragment emits the same page without the <!doctype>/<html>/<head>/<body> wrapper, for hosts
// that supply their own document shell (a review page, a docs embed).

import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, writeFileSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const APP = join(ROOT, "apps/desktop");
const DIST = join(APP, "dist-preview");

const outFlag = process.argv.indexOf("--out");
const FRAGMENT = process.argv.includes("--fragment");
const OUT = outFlag > -1 ? resolve(process.argv[outFlag + 1]) : join(DIST, "shogun-preview.html");

execFileSync("pnpm", ["exec", "vite", "build"], {
  cwd: APP,
  stdio: "inherit",
  env: { ...process.env, SHOGUN_PREVIEW: "1" },
});

const assetsDir = join(DIST, "assets");
const assets = readdirSync(assetsDir);
const js = assets.filter((f) => f.endsWith(".js"));
const css = assets.filter((f) => f.endsWith(".css"));

// One module, one stylesheet. More than one means code splitting crept back in and the inlining
// below would silently drop a chunk — fail loudly instead of shipping a half-built page.
if (js.length !== 1 || css.length !== 1) {
  throw new Error(`expected exactly 1 js + 1 css asset, got ${js.length} js / ${css.length} css`);
}

const html = readFileSync(join(DIST, "preview.html"), "utf8");
const jsCode = readFileSync(join(assetsDir, js[0]), "utf8");
const cssCode = readFileSync(join(assetsDir, css[0]), "utf8");

// `</script>` inside the bundle (it appears inside a string) would close the tag early.
const safeJs = jsCode.replace(/<\/script>/g, "<\\/script>");

const inlined = html
  .replace(/<script type="module" crossorigin src="[^"]+"><\/script>/, "")
  .replace(/<link rel="stylesheet" crossorigin href="[^"]+">/, `<style>${cssCode}</style>`)
  .replace("</body>", `<script type="module">${safeJs}</script></body>`);

if (inlined.includes("/assets/")) {
  throw new Error("an asset reference survived inlining — the built HTML changed shape");
}

const bootstrap = /<script>[\s\S]*?<\/script>/.exec(html)?.[0] ?? "";
const fragment = [
  "<title>SHOGUN — panel preview</title>",
  bootstrap, // seeds the __TAURI_INTERNALS__ placeholder; see preview.html
  `<style>${cssCode}</style>`,
  '<div id="root"></div>',
  `<script type="module">${safeJs}</script>`,
].join("\n");

writeFileSync(OUT, FRAGMENT ? fragment : inlined);
rmSync(assetsDir, { recursive: true, force: true });

const kb = (Buffer.byteLength(inlined) / 1024).toFixed(0);
console.log(`\npreview → ${OUT} (${kb} KB, self-contained)`);
