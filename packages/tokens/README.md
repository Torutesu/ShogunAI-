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

### Shadow / Blur（static）
`--shadow: 0 28px 66px rgba(0, 0, 0, 0.5), inset 0 1px 0 rgba(255, 255, 255, 0.06)`（展開パネル）/ `--shadow-sm: 0 10px 26px rgba(0, 0, 0, 0.4)`（小サーフェス）
`--blur: blur(38px) saturate(1.8)`（展開パネル）/ `--blur-sm: blur(30px) saturate(1.7)`（小サーフェス）
値は desktop 実値。light/dark 非依存のため static（base `:root` のみ）。`--blur*` は `backdrop-filter` 用。

### Spacing
本 Foundation では未定義（desktop 正本に汎用 spacing スケールが無いため）。必要になれば後続ブランチで追加する。

## テーマ切替
生成CSSは `:root[data-appearance="dark"|"light"|"auto"]` セレクタで切替（`:root` は `<html>` 要素）。属性は `<html>`（`document.documentElement`）に付与する。`auto` は OS 設定（`prefers-color-scheme`）に追従。

## website セット（web）
`apps/website` は独自パレット（"Skyglass"、light 基準、`data-theme` 切替）を使う。その生トークンは `src/tokens.json` の `web.themed` を正本とし、`dist/tokens.web.css` を生成する。

- 使い方（CSS）: `@import '@shogun-ai/tokens/web.css';`（`@import 'tailwindcss';` の直後）。
- ブロック構造: `:root`（light 基準）/ `:root[data-theme='dark']` / `@media (prefers-color-scheme: dark) { :root:not([data-theme='light']) }`。
- web トークンには非色値（`--orb-blend`, `--orb-opacity`）を含むため、`validate` は web についてはモード存在のみ検証する。
- product（desktop）セットとは別パレット。統一はしない（2テーマ共存）。
