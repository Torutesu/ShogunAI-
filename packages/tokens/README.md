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
