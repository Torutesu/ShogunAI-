/**
 * Renders scripts/og-image.html at 1200x630 into public/og-image.png — the card every
 * share of the site shows. The template, not this script, holds the design; keep its
 * headline in step with siteConfig.tagline.
 *
 * Playwright is not a dependency of this package (it is only ever needed to re-cut this
 * one asset), so run it with a Playwright install on PATH, e.g.
 *
 *   npx --yes playwright@1.49 install chromium
 *   node scripts/generate-og-image.mjs
 *
 * Set PLAYWRIGHT_CHROMIUM to point at an existing Chromium binary instead.
 */
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const template = path.join(here, 'og-image.html');
const out = path.join(here, '..', 'public', 'og-image.png');

const { chromium } = await import('playwright');
const browser = await chromium.launch(
  process.env.PLAYWRIGHT_CHROMIUM ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM } : {},
);
const page = await browser.newPage({ viewport: { width: 1200, height: 630 }, deviceScaleFactor: 1 });
await page.goto(`file://${template}`);
await page.evaluate(() => document.fonts.ready);
await page.screenshot({ path: out });
await browser.close();
console.log(`wrote ${out}`);
