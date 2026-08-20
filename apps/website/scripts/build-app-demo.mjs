// Builds the real desktop Full UI as a static bundle and drops it in public/app-demo/,
// so the marketing site can show the actual app rather than a drawing of it.
//
// Why a separate bundle behind an iframe rather than importing the component:
// apps/desktop styles the Full UI with unprefixed class names (`pane`, `side`, `frow`)
// and defines :root tokens (--ink, --muted, --accent) that collide head-on with this
// site's own. An iframe is the only boundary that keeps both intact — and it means the
// demo runs the app's own React tree, not a port of it that would drift.
//
// Outside Tauri the Full UI entry already falls back to its sample fixture, which is the
// mock data this demo shows.

import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const websiteRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(websiteRoot, '..', '..');
const desktopRoot = join(repoRoot, 'apps', 'desktop');
const out = join(websiteRoot, 'public', 'app-demo');

if (!existsSync(join(desktopRoot, 'fullui.html'))) {
  console.warn('[app-demo] apps/desktop not present — skipping');
  process.exit(0);
}

try {
  // `--base ./` makes the emitted asset paths relative, so the bundle works from
  // /app-demo/ instead of expecting to own the site root.
  execFileSync('pnpm', ['exec', 'vite', 'build', '--base', './'], {
    cwd: desktopRoot,
    stdio: 'inherit',
  });
} catch (error) {
  console.warn(`[app-demo] desktop build failed — skipping (${error.message})`);
  process.exit(0);
}

const dist = join(desktopRoot, 'dist');
if (!existsSync(join(dist, 'fullui.html'))) {
  console.warn('[app-demo] fullui.html missing from the desktop build — skipping');
  process.exit(0);
}

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
// Two surfaces, both real: the Full UI window for the features page, and the
// notch panel — the app's signature surface — for the hero. Both fall back to
// their own mock fixtures outside Tauri.
cpSync(join(dist, 'fullui.html'), join(out, 'index.html'));
if (existsSync(join(dist, 'index.html'))) cpSync(join(dist, 'index.html'), join(out, 'panel.html'));
cpSync(join(dist, 'assets'), join(out, 'assets'), { recursive: true });

const bytes = readdirSync(join(out, 'assets')).length;
console.log(`[app-demo] wrote public/app-demo (index.html + ${bytes} assets)`);
