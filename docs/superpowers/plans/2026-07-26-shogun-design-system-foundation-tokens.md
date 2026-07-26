# SHOGUN デザインシステム Foundation（トークン基盤）実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 散在する SHOGUN のデザイントークンを `@shogun-ai/tokens` パッケージに単一正本（JSON）として集約し、そこから CSS 変数と TS を生成、desktop を見た目不変のまま正本参照へ切り替える。

**Architecture:** `packages/tokens/src/tokens.json` を機械可読な正本とし、`scripts/build.mjs`（検証付き）が `dist/tokens.css`（`:root` / `[data-appearance=dark|light]` / `@media auto` の各ブロック）と `dist/tokens.ts`（型付き定数）を生成する。desktop は `main.tsx` で `@shogun-ai/tokens/css` を import し、`styles.css` から重複するトークン定義ブロックを削除する。website は本計画では未接続（tokens.json 内にマッピング表のみ記載）。

**Tech Stack:** pnpm workspaces + turbo、Node ESM（`.mjs`）、`node --test`、Vite（desktop）、TypeScript。

---

## 前提と正本の値（styles.css から確定）

正本は `apps/desktop/src/styles.css` の `:root` 系ブロック（1〜93行目相当）。構造は以下:

- **static トークン**（テーマ非依存、base `:root` にのみ出力）:
  - `--sys: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`
  - `--sp: 160ms cubic-bezier(0.32, 0.72, 0, 1)`
  - `--fs-xs:10px --fs-sm:11px --fs-md:12px --fs-lg:13px --fs-xl:14px`
  - `--r-xs:7px --r-sm:10px --r-md:13px --r-lg:16px --r-xl:20px --r-pill:999px`
  - `--accent-soft: color-mix(in srgb, var(--accent) 26%, transparent)`（base のみ。`var(--accent)` 参照でモード追従）
- **themed トークン**（`dark` / `light` の2値。base=dark も出力）:
  | token | dark | light |
  |---|---|---|
  | `--glass` | `rgba(21, 24, 31, 0.85)` | `rgba(249, 250, 252, 0.92)` |
  | `--glass-2` | `rgba(31, 35, 43, 0.85)` | `rgba(255, 255, 255, 0.92)` |
  | `--line` | `rgba(255, 255, 255, 0.09)` | `rgba(0, 0, 0, 0.09)` |
  | `--line-strong` | `rgba(255, 255, 255, 0.16)` | `rgba(0, 0, 0, 0.16)` |
  | `--ink` | `#eef1f5` | `#1b1d22` |
  | `--muted` | `#9aa3af` | `#5c616a` |
  | `--faint` | `#8b95a3` | `#6b7280` |
  | `--accent` | `#6ea8fe` | `#2f6fed` |
  | `--accent-2` | `#8f7dfb` | `#6a53e6` |
  | `--accent-ink` | `#06101c` | `#ffffff` |
  | `--live` | `#30d158` | `#1f9e3c` |
  | `--warn` | `#ff9f0a` | `#a4560a` |
  | `--fill` | `rgba(255, 255, 255, 0.06)` | `rgba(0, 0, 0, 0.045)` |
  | `--fill-strong` | `rgba(255, 255, 255, 0.12)` | `rgba(0, 0, 0, 0.09)` |
  | `--card` | `rgba(255, 255, 255, 0.045)` | `rgba(0, 0, 0, 0.035)` |

生成 CSS のブロック順（styles.css を忠実再現）:
```
:root { <static> <themed.dark> }
:root[data-appearance="dark"] { <themed.dark> }
:root[data-appearance="light"] { <themed.light> }
@media (prefers-color-scheme: light) { :root[data-appearance="auto"] { <themed.light> } }
```

> shadow / blur トークンは desktop 正本に存在しないため本計画では扱わない（wireframe 側にのみあり、将来ブランチで統合）。

---

## File Structure

- `packages/tokens/package.json` — パッケージ定義（build スクリプト、exports）
- `packages/tokens/tsconfig.json` — `@shogun-ai/config` の base を継承
- `packages/tokens/.gitignore` — `dist/`
- `packages/tokens/src/tokens.json` — 正本（static / themed / website マッピング表）
- `packages/tokens/scripts/build.mjs` — 検証 + 生成（`validate` / `generateCss` / `generateTs` を export）
- `packages/tokens/scripts/build.test.mjs` — 生成・検証の単体テスト
- `packages/tokens/README.md` — Foundation ドキュメント（トークン表）
- `packages/tokens/dist/tokens.css`（生成物）
- `packages/tokens/dist/tokens.ts`（生成物）
- 変更: `apps/desktop/package.json` — dependencies に `@shogun-ai/tokens`
- 変更: `apps/desktop/src/main.tsx` — トークン CSS を先に import
- 変更: `apps/desktop/src/styles.css` — トークン定義ブロック（4〜93行）を削除
- 変更: `turbo.json` — `dev` タスクに `dependsOn: ["^build"]`

---

## Task 1: `@shogun-ai/tokens` パッケージの雛形

**Files:**
- Create: `packages/tokens/package.json`
- Create: `packages/tokens/tsconfig.json`
- Create: `packages/tokens/.gitignore`

- [ ] **Step 1: package.json を作成**

`packages/tokens/package.json`:
```json
{
  "name": "@shogun-ai/tokens",
  "version": "0.0.0",
  "private": true,
  "type": "module",
  "exports": {
    "./css": "./dist/tokens.css",
    "./ts": "./dist/tokens.ts",
    "./tokens.json": "./src/tokens.json"
  },
  "scripts": {
    "build": "node scripts/build.mjs",
    "test": "node --test scripts/build.test.mjs"
  },
  "devDependencies": {
    "@shogun-ai/config": "workspace:*"
  }
}
```

- [ ] **Step 2: tsconfig.json を作成**

`packages/tokens/tsconfig.json`:
```json
{
  "extends": "@shogun-ai/config/tsconfig.base.json",
  "include": ["scripts/**/*", "src/**/*"]
}
```

- [ ] **Step 3: .gitignore を作成**

`packages/tokens/.gitignore`:
```
dist/
```

- [ ] **Step 4: 依存をインストール**

Run: `pnpm install`
Expected: `@shogun-ai/tokens` がワークスペースに認識され、エラーなく完了。

- [ ] **Step 5: Commit**

```bash
git add packages/tokens/package.json packages/tokens/tsconfig.json packages/tokens/.gitignore pnpm-lock.yaml
git commit -m "feat(tokens): @shogun-ai/tokens パッケージの雛形"
```

---

## Task 2: 正本 `tokens.json` を作成

**Files:**
- Create: `packages/tokens/src/tokens.json`

- [ ] **Step 1: tokens.json を作成**

`packages/tokens/src/tokens.json`:
```json
{
  "$schema-note": "SHOGUN design tokens — single source of truth. Values mirror apps/desktop/src/styles.css. See build.mjs for CSS/TS generation.",
  "static": {
    "sys": "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif",
    "sp": "160ms cubic-bezier(0.32, 0.72, 0, 1)",
    "fs-xs": "10px",
    "fs-sm": "11px",
    "fs-md": "12px",
    "fs-lg": "13px",
    "fs-xl": "14px",
    "r-xs": "7px",
    "r-sm": "10px",
    "r-md": "13px",
    "r-lg": "16px",
    "r-xl": "20px",
    "r-pill": "999px",
    "accent-soft": "color-mix(in srgb, var(--accent) 26%, transparent)"
  },
  "themed": {
    "glass":        { "dark": "rgba(21, 24, 31, 0.85)",   "light": "rgba(249, 250, 252, 0.92)" },
    "glass-2":      { "dark": "rgba(31, 35, 43, 0.85)",   "light": "rgba(255, 255, 255, 0.92)" },
    "line":         { "dark": "rgba(255, 255, 255, 0.09)","light": "rgba(0, 0, 0, 0.09)" },
    "line-strong":  { "dark": "rgba(255, 255, 255, 0.16)","light": "rgba(0, 0, 0, 0.16)" },
    "ink":          { "dark": "#eef1f5", "light": "#1b1d22" },
    "muted":        { "dark": "#9aa3af", "light": "#5c616a" },
    "faint":        { "dark": "#8b95a3", "light": "#6b7280" },
    "accent":       { "dark": "#6ea8fe", "light": "#2f6fed" },
    "accent-2":     { "dark": "#8f7dfb", "light": "#6a53e6" },
    "accent-ink":   { "dark": "#06101c", "light": "#ffffff" },
    "live":         { "dark": "#30d158", "light": "#1f9e3c" },
    "warn":         { "dark": "#ff9f0a", "light": "#a4560a" },
    "fill":         { "dark": "rgba(255, 255, 255, 0.06)","light": "rgba(0, 0, 0, 0.045)" },
    "fill-strong":  { "dark": "rgba(255, 255, 255, 0.12)","light": "rgba(0, 0, 0, 0.09)" },
    "card":         { "dark": "rgba(255, 255, 255, 0.045)","light": "rgba(0, 0, 0, 0.035)" }
  },
  "$website_mapping": {
    "_note": "Documentation only — NOT consumed by build.mjs. Guides the future website migration branch (apps/website uses data-theme + --bg/--surface/--cloud vocabulary).",
    "bg": "glass",
    "surface": "glass-2",
    "cloud": "fill",
    "ink": "ink",
    "on-ink": "accent-ink"
  }
}
```

- [ ] **Step 2: JSON が妥当か確認**

Run: `node -e "JSON.parse(require('fs').readFileSync('packages/tokens/src/tokens.json','utf8')); console.log('ok')"`
Expected: `ok`

- [ ] **Step 3: Commit**

```bash
git add packages/tokens/src/tokens.json
git commit -m "feat(tokens): デザイントークン正本(tokens.json)"
```

---

## Task 3: `build.mjs` の検証ロジック（TDD）

**Files:**
- Create: `packages/tokens/scripts/build.mjs`
- Test: `packages/tokens/scripts/build.test.mjs`

- [ ] **Step 1: 失敗するテストを書く（validate）**

`packages/tokens/scripts/build.test.mjs`:
```js
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
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs`
Expected: FAIL（`Cannot find module` または `validate is not a function` 系）

- [ ] **Step 3: build.mjs に validate を実装**

`packages/tokens/scripts/build.mjs`:
```js
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
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs`
Expected: PASS（3 tests）

- [ ] **Step 5: Commit**

```bash
git add packages/tokens/scripts/build.mjs packages/tokens/scripts/build.test.mjs
git commit -m "feat(tokens): tokens.json 検証ロジック(validate) + テスト"
```

---

## Task 4: CSS 生成（TDD）

**Files:**
- Modify: `packages/tokens/scripts/build.mjs`
- Test: `packages/tokens/scripts/build.test.mjs`

- [ ] **Step 1: 失敗するテストを追加（generateCss）**

`packages/tokens/scripts/build.test.mjs` の末尾に追記:
```js
import { generateCss } from "./build.mjs";

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
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs`
Expected: FAIL（`generateCss is not a function`）

- [ ] **Step 3: build.mjs に generateCss を実装**

`packages/tokens/scripts/build.mjs` に追記:
```js
const HEADER =
  "/* GENERATED by @shogun-ai/tokens (scripts/build.mjs from src/tokens.json). Do not edit by hand. */\n";

function themedBlock(themed, mode) {
  return Object.entries(themed)
    .map(([name, byMode]) => `  --${name}: ${byMode[mode]};`)
    .join("\n");
}

function staticBlock(statics) {
  return Object.entries(statics)
    .map(([name, value]) => `  --${name}: ${value};`)
    .join("\n");
}

export function generateCss(tokens) {
  const statics = tokens.static ?? {};
  const themed = tokens.themed ?? {};
  return (
    HEADER +
    `:root {\n${staticBlock(statics)}\n${themedBlock(themed, "dark")}\n}\n` +
    `:root[data-appearance="dark"] {\n${themedBlock(themed, "dark")}\n}\n` +
    `:root[data-appearance="light"] {\n${themedBlock(themed, "light")}\n}\n` +
    `@media (prefers-color-scheme: light) {\n  :root[data-appearance="auto"] {\n${themedBlock(themed, "light")
      .split("\n")
      .map((l) => "  " + l)
      .join("\n")}\n  }\n}\n`
  );
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs`
Expected: PASS（全 test）

- [ ] **Step 5: Commit**

```bash
git add packages/tokens/scripts/build.mjs packages/tokens/scripts/build.test.mjs
git commit -m "feat(tokens): tokens.json → CSS 生成(generateCss) + テスト"
```

---

## Task 5: TS 生成 + main エントリ（TDD）

**Files:**
- Modify: `packages/tokens/scripts/build.mjs`
- Test: `packages/tokens/scripts/build.test.mjs`

- [ ] **Step 1: 失敗するテストを追加（generateTs / main）**

`packages/tokens/scripts/build.test.mjs` の末尾に追記:
```js
import { generateTs, main } from "./build.mjs";
import { readFileSync as _read, existsSync } from "node:fs";
import { resolve as _resolve } from "node:path";

test("generateTs emits a typed const with themed + static values", () => {
  const ts = generateTs(sample);
  assert.match(ts, /export const tokens = \{/);
  assert.match(ts, /as const/);
  assert.match(ts, /"accent"/);
  assert.match(ts, /export type TokenName/);
});

test("main writes dist/tokens.css and dist/tokens.ts", () => {
  main();
  const pkg = _resolve(process.cwd());
  assert.ok(existsSync(_resolve(pkg, "dist/tokens.css")));
  assert.ok(existsSync(_resolve(pkg, "dist/tokens.ts")));
  const css = _read(_resolve(pkg, "dist/tokens.css"), "utf8");
  assert.match(css, /--glass: rgba\(21, 24, 31, 0\.85\)/);
});
```
> 注: `main()` テストは cwd が `packages/tokens` 前提（Step 実行の `cd` で満たす）。

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs`
Expected: FAIL（`generateTs is not a function`）

- [ ] **Step 3: build.mjs に generateTs と main を実装**

`packages/tokens/scripts/build.mjs` に追記:
```js
export function generateTs(tokens) {
  const json = JSON.stringify(
    { static: tokens.static ?? {}, themed: tokens.themed ?? {} },
    null,
    2,
  );
  return (
    "// GENERATED by @shogun-ai/tokens (scripts/build.mjs). Do not edit by hand.\n" +
    `export const tokens = ${json} as const;\n` +
    "export type TokenName = keyof (typeof tokens)[\"themed\"] | keyof (typeof tokens)[\"static\"];\n"
  );
}

export function main() {
  const tokens = JSON.parse(readFileSync(resolve(PKG, "src/tokens.json"), "utf8"));
  const errors = validate(tokens);
  if (errors.length) {
    console.error("Token validation failed:\n" + errors.map((e) => "  - " + e).join("\n"));
    process.exit(1);
  }
  mkdirSync(resolve(PKG, "dist"), { recursive: true });
  writeFileSync(resolve(PKG, "dist/tokens.css"), generateCss(tokens));
  writeFileSync(resolve(PKG, "dist/tokens.ts"), generateTs(tokens));
  console.log("Wrote dist/tokens.css and dist/tokens.ts");
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs`
Expected: PASS（全 test）

- [ ] **Step 5: build を実行して生成物を確認**

Run: `cd packages/tokens && pnpm build && head -20 dist/tokens.css`
Expected: `Wrote dist/tokens.css and dist/tokens.ts` の後、`:root {` ブロックに `--glass: rgba(21, 24, 31, 0.85);` 等が並ぶ。

- [ ] **Step 6: 生成 CSS が正本と一致するか差分確認**

Run:
```bash
cd packages/tokens
node -e "const c=require('fs').readFileSync('dist/tokens.css','utf8'); for (const t of ['--glass: rgba(21, 24, 31, 0.85)','--accent: #6ea8fe','--accent: #2f6fed','--r-md: 13px','--sp: 160ms cubic-bezier(0.32, 0.72, 0, 1)','--accent-soft: color-mix']) if(!c.includes(t)){console.error('MISSING',t);process.exit(1)} console.log('all present')"
```
Expected: `all present`

- [ ] **Step 7: Commit**

```bash
git add packages/tokens/scripts/build.mjs packages/tokens/scripts/build.test.mjs
git commit -m "feat(tokens): TS 生成(generateTs) + main 生成パイプライン"
```

---

## Task 6: desktop を正本参照へ切替（見た目不変）

**Files:**
- Modify: `apps/desktop/package.json`
- Modify: `apps/desktop/src/main.tsx`
- Modify: `apps/desktop/src/styles.css`

- [ ] **Step 1: desktop に依存を追加**

`apps/desktop/package.json` の `dependencies` に以下を追加（既存の並びに合わせる）:
```json
"@shogun-ai/tokens": "workspace:*"
```

- [ ] **Step 2: 依存をインストール**

Run: `pnpm install`
Expected: エラーなく完了。

- [ ] **Step 3: main.tsx でトークン CSS を先に import**

`apps/desktop/src/main.tsx` の `import "./styles.css";`（4行目）の**直前**に追加:
```ts
import "@shogun-ai/tokens/css";
```
結果（順序が重要 — トークンを先に定義してから styles.css が参照）:
```ts
import "@shogun-ai/tokens/css";
import "./styles.css";
```

- [ ] **Step 4: styles.css からトークン定義ブロックを削除**

`apps/desktop/src/styles.css` の **4行目 `:root {` から 93行目 `}`（`@media` ブロックの閉じ）まで**を削除する。ファイル冒頭は 1〜3 行のコメントを残し、その直後に `* {` 以降（元 95 行目〜）が続く形にする。削除後のファイル先頭は次のようになる:
```css
/* SHOGUN panel — a translucent glass panel that hangs from the notch. The window is resized to
   fit (handle vs open), so this file just styles the two surfaces. Theme-aware; dark default.
   Design language: soft glass, generous corner radii, layered rounded cards. */

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}
```
> 削除するのはトークンを定義している `:root` / `:root[data-appearance=...]` / `@media (prefers-color-scheme: light)` の3種のブロックのみ。それ以外（`* {}`, `body`, `.stage` 等）は一切変更しない。

- [ ] **Step 5: 残存する重複トークン定義がないことを確認**

Run: `grep -nE "^\s*--(glass|ink|accent|r-md|sp|fs-md):" apps/desktop/src/styles.css`
Expected: 出力なし（トークン定義は tokens パッケージへ移動済み）。

- [ ] **Step 6: desktop の型チェック / ビルドが通ることを確認**

Run: `pnpm --filter @shogun-ai/tokens build && pnpm --filter @shogun-ai/desktop build`
Expected: tokens が dist を生成した後、desktop の tsc + vite build が成功（`@shogun-ai/tokens/css` が解決される）。

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/package.json apps/desktop/src/main.tsx apps/desktop/src/styles.css pnpm-lock.yaml
git commit -m "refactor(desktop): トークンを @shogun-ai/tokens 正本から参照(見た目不変)"
```

---

## Task 7: turbo 配線 + 回帰確認 + Foundation ドキュメント

**Files:**
- Modify: `turbo.json`
- Create: `packages/tokens/README.md`

- [ ] **Step 1: turbo の dev がトークンを先にビルドするよう配線**

`turbo.json` の `tasks.dev` を次のように変更（`dependsOn` を追加）:
```json
"dev": {
  "dependsOn": ["^build"],
  "cache": false,
  "persistent": true
}
```
> これで `pnpm dev` 実行時、desktop の dev 前に依存（`@shogun-ai/tokens`）の `build` が走り `dist/tokens.css` が生成される。

- [ ] **Step 2: dev 起動でトークンが解決されることを確認（回帰の一次確認）**

Run: `pnpm --filter @shogun-ai/desktop dev`（数秒起動して Vite がエラーなく listen することを確認したら停止）
Expected: Vite が起動し `@shogun-ai/tokens/css` の解決エラーが出ない。

- [ ] **Step 3: 見た目回帰をスクリーンショットで確認**

desktop アプリ（Notch / 設定画面）を起動し、この変更**前後**で見た目が同一であることをスクリーンショット比較で確認する。トークン値は正本と一致させているため差異は出ないはず。差異が出た場合は Task 6 Step 6 の差分確認に戻る。
> 記録: 確認結果（差異なし/あり）をこの計画のチェックボックス脇か PR 本文に残す。

- [ ] **Step 4: Foundation ドキュメント（README）を作成**

`packages/tokens/README.md`:
```markdown
# @shogun-ai/tokens

SHOGUN デザインシステムの **Foundation（トークン基盤）**。`src/tokens.json` を単一正本とし、`scripts/build.mjs` が `dist/tokens.css`（CSS 変数）と `dist/tokens.ts`（型付き定数）を生成する。

## 使い方

- CSS（Vite/バンドラ経由）: `import "@shogun-ai/tokens/css";`（トークンを参照する CSS より前に読み込む）
- TS: `import { tokens, type TokenName } from "@shogun-ai/tokens/ts";`

生成物 `dist/` はコミットせず、`pnpm build`（turbo）で生成する。

## トークン

### Color（themed: dark / light）
`--glass` `--glass-2` `--line` `--line-strong` `--ink` `--muted` `--faint` `--accent` `--accent-2` `--accent-ink` `--live` `--warn` `--fill` `--fill-strong` `--card`
※ `--accent-soft` は `var(--accent)` を参照しモードに追従（base のみ定義）。

### Radius
`--r-xs:7px` `--r-sm:10px` `--r-md:13px` `--r-lg:16px` `--r-xl:20px` `--r-pill:999px`

### Type
`--fs-xs:10px` `--fs-sm:11px` `--fs-md:12px` `--fs-lg:13px` `--fs-xl:14px` / フォント `--sys`

### Motion
`--sp: 160ms cubic-bezier(0.32, 0.72, 0, 1)`

### Shadow / Spacing
本 Foundation では未定義（desktop 正本に無いため）。将来のブランチで wireframe（`docs/wireframes/shogun-ui.css`）の shadow/blur を統合する。

## テーマ切替
`html[data-appearance="dark"|"light"|"auto"]`。`auto` は OS 設定（`prefers-color-scheme`）に追従。

## website について
`apps/website` は別語彙（`--bg`/`--surface`/`--cloud`、`data-theme`）を使う。対応表は `src/tokens.json` の `$website_mapping`（ドキュメントのみ）にあり、移行は別ブランチで行う。
```

- [ ] **Step 5: パッケージ全体のビルド・型チェック・テストが通ることを確認**

Run: `pnpm build && pnpm typecheck && pnpm --filter @shogun-ai/tokens test`
Expected: turbo build 成功、typecheck パス、tokens テスト全 PASS。
> `pnpm typecheck` は `typecheck` スクリプトを持つパッケージのみ turbo が実行する（tokens パッケージは TS ソースを持たないためスキップされ得る。desktop は tsc により `@shogun-ai/tokens/css` 参照を含めて検証される）。

- [ ] **Step 6: Commit**

```bash
git add turbo.json packages/tokens/README.md
git commit -m "feat(tokens): turbo dev 配線 + Foundation ドキュメント(README)"
```

---

## 完了条件（Definition of Done）

1. `pnpm --filter @shogun-ai/tokens build` が `dist/{tokens.css,tokens.ts}` を生成する。
2. `pnpm --filter @shogun-ai/tokens test` が全 PASS。
3. desktop が `@shogun-ai/tokens/css` を参照し、`pnpm --filter @shogun-ai/desktop build` が成功、見た目回帰なし（スクショ確認済み）。
4. `pnpm build`（turbo 全体）と `pnpm typecheck` がパス。
5. `packages/tokens/README.md` にトークン表（Color/Radius/Type/Motion）が記載されている。
6. website は未接続のまま（`$website_mapping` のみ）。

## 後続ブランチへの申し送り

- **website 移行ブランチ**: `$website_mapping` を基に `apps/website/src/app/globals.css` の `@theme` を tokens 参照へ。`data-theme` ↔ `data-appearance` の属性差を吸収。
- **shadow/blur 統合**: `docs/wireframes/shogun-ui.css` の `--shadow*` `--blur*` を tokens.json へ追加。
- **Components ブランチ**: `packages/ui` が `@shogun-ai/tokens/ts` を参照してコンポーネント実装。
- **退避中の作業**: `feat/meeting-notes-issue-7` の `stash@{0}`（CLAUDE.md 料金記述）を作業後に復元すること。
