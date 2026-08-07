/*
  Export the interactive wireframe prototypes (docs/wireframes/*.html) as static,
  self-contained HTML frames for import into Figma via the html.to.design plugin.

  Each source page is driven in headless Chromium into every screen state
  (notch panel states, Full UI tabs, Settings tabs, onboarding steps, billing
  toggle), in both dark and light appearance, then serialized with all scripts
  removed, animations frozen, and the shared CSS / mark SVG inlined — so every
  output file renders identically when opened standalone or uploaded to the
  plugin, with no sibling assets needed.

  Output: docs/wireframes/figma-import/{dark,light}/<page>--<state>.html
          docs/wireframes/figma-import/index.html (browsable gallery)

  Run:  npm i playwright-core   (any dir; or set NODE_PATH to an install)
        node scripts/export-wireframes-figma.mjs
  Uses the Chromium at PLAYWRIGHT_BROWSERS_PATH (or a local playwright install).
*/
import { chromium } from "playwright-core";
import { readFileSync, writeFileSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SRC = join(ROOT, "docs", "wireframes");
const OUT = join(SRC, "figma-import");

const css = readFileSync(join(SRC, "shogun-ui.css"), "utf8");
const markDataUri =
  "data:image/svg+xml;base64," +
  readFileSync(join(SRC, "shogun-mark.svg")).toString("base64");

const clickTab = (tab) => async (page) => {
  await page.click(`.nav[data-tab="${tab}"]`);
};

const PAGES = [
  {
    file: "shogun-notch.html",
    name: "notch",
    states: [
      { id: "idle", drive: null },
      ...["welcome", "answer", "tracked", "meeting"].map((s) => ({
        id: s,
        drive: async (page) => {
          await page.click(`#sw button[data-go="${s}"]`);
        },
      })),
    ],
  },
  {
    file: "shogun-fullui.html",
    name: "fullui-pro",
    states: ["today", "health", "sources", "memory", "activity", "trace"].map(
      (t) => ({ id: t, drive: clickTab(t) })
    ),
  },
  {
    file: "shogun-fullui-standard.html",
    name: "fullui-standard",
    states: ["today", "health", "sources", "memory", "activity", "trace"].map(
      (t) => ({ id: t, drive: clickTab(t) })
    ),
  },
  {
    file: "shogun-settings.html",
    name: "settings",
    states: [
      "account", "privacy", "appearance", "shortcuts", "memory",
      "connections", "aisessions", "model", "nightly", "approvals",
    ].map((t) => ({ id: t, drive: clickTab(t) })),
  },
  {
    file: "shogun-onboarding.html",
    name: "onboarding",
    states: [0, 1, 2, 3, 4].map((i) => ({
      id: `step${i + 1}`,
      drive:
        i === 0
          ? null
          : async (page) => {
              for (let k = 0; k < i; k++) {
                await page.click("#rightbtns .btn.primary");
                await page.waitForTimeout(60);
              }
            },
    })),
  },
  {
    file: "shogun-plans.html",
    name: "plans",
    states: [
      { id: "annual", drive: null },
      {
        id: "monthly",
        drive: async (page) => {
          await page.click('[data-billing] span[data-bill="month"]');
        },
      },
    ],
  },
  { file: "shogun-standard-locks.html", name: "standard-locks", states: [{ id: "default", drive: null }] },
  {
    file: "shogun-notch-full.html",
    name: "notch-full",
    states: [
      "set-general", "set-privacy", "set-conn", "set-model", "set-approvals",
      "today", "memory", "status",
    ].map((s) => ({
      id: s,
      drive:
        s === "set-general"
          ? null
          : async (page) => {
              await page.click(`#sw button[data-go="${s}"]`);
              await page.waitForTimeout(350); // panel height transition
            },
    })),
  },
];

const THEMES = ["dark", "light"];

function postProcess(html, { name, stateId, theme }) {
  // Inline the shared stylesheet (both reference styles used across the pages).
  html = html
    .replace(/<link rel="stylesheet" href="shogun-ui.css"\s*\/?>/, `<style>\n${css}\n</style>`)
    .replace(/@import url\("shogun-ui.css"\);/, css);
  // Inline the mark so the file has no sibling-asset dependency.
  html = html.replaceAll('src="shogun-mark.svg"', `src="${markDataUri}"`);
  // Frame-friendly title: html.to.design names imported layers after it.
  html = html.replace(
    /<title>[^<]*<\/title>/,
    `<title>SHOGUN / ${name} / ${stateId} / ${theme}</title>`
  );
  return html;
}

const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH || undefined,
});
const ctx = await browser.newContext({
  viewport: { width: 1440, height: 900 },
  deviceScaleFactor: 1,
  reducedMotion: "reduce",
});

const manifest = [];
for (const theme of THEMES) {
  mkdirSync(join(OUT, theme), { recursive: true });
  for (const pageDef of PAGES) {
    for (const state of pageDef.states) {
      const page = await ctx.newPage();
      await page.goto(pathToFileURL(join(SRC, pageDef.file)).href);
      await page.evaluate((t) => {
        localStorage.setItem("shogun-appearance", t);
        document.documentElement.setAttribute("data-theme", t);
        document.documentElement.setAttribute("data-appearance", t);
      }, theme);
      if (state.drive) await state.drive(page);
      await page.waitForTimeout(450); // let state JS and layout settle
      const html = await page.evaluate(() => {
        document
          .querySelectorAll("script, .theme-switch, .switcher, .caption, .tip")
          .forEach((el) => el.remove());
        const freeze = document.createElement("style");
        freeze.textContent =
          "*,*::before,*::after{animation:none!important;transition:none!important}";
        document.head.appendChild(freeze);
        return "<!doctype html>\n" + document.documentElement.outerHTML;
      });
      const fileName = `${pageDef.name}--${state.id}.html`;
      writeFileSync(
        join(OUT, theme, fileName),
        postProcess(html, { name: pageDef.name, stateId: state.id, theme })
      );
      manifest.push({ theme, page: pageDef.name, state: state.id, path: `${theme}/${fileName}` });
      await page.close();
      process.stdout.write(`${theme}/${fileName}\n`);
    }
  }
}
await browser.close();

// Browsable gallery for humans (not meant for plugin import).
const groups = [...new Set(manifest.map((m) => m.page))];
const gallery = `<!doctype html>
<html lang="en"><head><meta charset="utf-8" />
<title>SHOGUN — Figma import frames</title>
<style>
  body{font-family:-apple-system,system-ui,sans-serif;margin:32px;background:#101216;color:#EEF1F5}
  h1{font-size:20px} h2{font-size:15px;margin:34px 0 12px;color:#9AA3AF;text-transform:uppercase;letter-spacing:.06em}
  .grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(340px,1fr));gap:18px}
  .card{border:1px solid #2A2E36;border-radius:12px;overflow:hidden;background:#15181F}
  .card iframe{width:1440px;height:900px;border:0;transform:scale(.236);transform-origin:top left;pointer-events:none}
  .card .vp{width:100%;height:212px;overflow:hidden}
  .card a{display:block;padding:9px 13px;font-size:12.5px;color:#6EA8FE;text-decoration:none;border-top:1px solid #2A2E36}
</style></head><body>
<h1>SHOGUN — static wireframe frames for Figma import (${manifest.length})</h1>
<p style="font-size:13px;color:#9AA3AF">Generated by scripts/export-wireframes-figma.mjs — do not edit by hand. Import individual files with the html.to.design Figma plugin (see docs/figma-import-guide.md).</p>
${groups
  .map(
    (g) => `<h2>${g}</h2>\n<div class="grid">\n${manifest
      .filter((m) => m.page === g)
      .map(
        (m) => `<div class="card"><div class="vp"><iframe loading="lazy" src="${m.path}"></iframe></div><a href="${m.path}">${m.path}</a></div>`
      )
      .join("\n")}\n</div>`
  )
  .join("\n")}
</body></html>\n`;
writeFileSync(join(OUT, "index.html"), gallery);

const perTheme = readdirSync(join(OUT, "dark")).length;
console.log(`\n${manifest.length} frames written (${perTheme} per theme) + index.html`);
