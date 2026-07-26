# SHOGUN デザインシステム — Engineering Guide

> 正本はコード（`packages/tokens/` と各アプリ）。本書はその要約と使い方であり、齟齬があればコードが正。

## トークンパッケージ `@shogun-ai/tokens`

`packages/tokens/` がデザイントークンの単一正本。

- **正本**: `src/tokens.json`
  - `static` — テーマ非依存（`--sys` / `--sp` / `--fs-*` / `--r-*` / `--accent-soft` / `--shadow`・`--shadow-sm` / `--blur`・`--blur-sm`）。
  - `themed` — product（desktop）の light/dark（`--glass` / `--ink` / `--accent` など）。
  - `web` — website（"Skyglass"）の light/dark（`--bg` / `--surface` / `--accent` / `--sky-soft` など）。product とは別パレット（統一しない）。
- **生成**: `scripts/build.mjs`
  - `validate(tokens)` — themed は両モード＋色形式、web は両モードの存在のみ検証（web は `orb-blend` 等の非色値を含むため色検証しない）。
  - `generateCss(tokens)` — product 用。`:root`（static + dark）/ `:root[data-appearance="dark"|"light"]` / `@media (prefers-color-scheme: light) :root[data-appearance="auto"]`。
  - `generateWebCss(tokens)` — website 用。`:root`（light 基準）/ `:root[data-theme='dark']` / `@media (prefers-color-scheme: dark) :root:not([data-theme='light'])`。
  - `generateTs(tokens)` — 型付き定数 `tokens` と `TokenName`。
  - `main()` — 検証 → `dist/tokens.css` / `dist/tokens.web.css` / `dist/tokens.ts` を出力。検証失敗で `process.exit(1)`。
- **配布**: `package.json` の `exports` — `./css`（product CSS）/ `./web.css`（website CSS）/ `./ts`（型）/ `./tokens.json`（正本）。
- **ビルド**: `dist/` は gitignore。turbo が生成（`build` の `outputs: dist/**`、`dev` は `dependsOn: ["^build"]` で消費側 dev の前に生成）。テスト: `pnpm --filter @shogun-ai/tokens test`（`node --test`）。

## 消費方法

- **desktop**（React 18 / Vite / 素CSS）: `apps/desktop/src/main.tsx` で `import "@shogun-ai/tokens/css";` を `import "./styles.css";` より前に読み込む。`styles.css` は `var(--…)` を参照。
- **website**（React 19 / Next / Tailwind v4）: `apps/website/src/app/globals.css` の `@import 'tailwindcss';` 直後に `@import '@shogun-ai/tokens/web.css';`。`@theme inline { --color-bg: var(--bg); … }` で Tailwind ユーティリティに接続。

## テーマ切替

- product: `html[data-appearance="dark"|"light"|"auto"]`（`auto` は OS 設定に追従）。
- website: `html[data-theme="dark"|"light"]`（未指定時は system media で判定）。

## コンポーネント

website の UI コンポーネント（`apps/website/src/components/ui/` の button/card/input/badge）は `docs/design-system/components.md` を参照。共有パッケージ `@shogun-ai/ui` への抽出は消費者が増えた時点で検討。

## リポジトリ規約

- コミットは Conventional Commits（`feat:` / `fix:` / `perf:` / `docs:`）。
- トークン変更は `src/tokens.json` を編集し `pnpm --filter @shogun-ai/tokens build` / `test` で確認。desktop/website 側の見た目を変えない場合は値を実装と一致させる。
