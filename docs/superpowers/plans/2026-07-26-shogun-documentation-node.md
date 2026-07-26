# SHOGUN Documentation 節（第1スライス）実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `docs/design-system/` に index + Engineering Guide + Copywriting + Accessibility の4ドキュメントを、既存コード/規約の裏付けに忠実に整備する（コード無変更・発明なし）。

**Architecture:** 4つの markdown ファイルを新規作成。内容は既存の `CLAUDE.md`・`apps/*` のコード・`packages/tokens`・これまでの成果物から転記・整理したもの。実行時挙動に影響しない。

**Tech Stack:** Markdown。裏付け元は `@shogun-ai/tokens`、desktop/website の CSS、CLAUDE.md。

---

## File Structure

- 新規: `docs/design-system/engineering-guide.md`
- 新規: `docs/design-system/copywriting.md`
- 新規: `docs/design-system/accessibility.md`
- 新規: `docs/design-system/README.md`（index。他3点 + 既存 Foundation/Components へリンク）

各タスクは1ファイルを作成し、参照実在を検証してコミットする。全内容は各タスクに明記。

---

## Task 1: engineering-guide.md

**Files:**
- Create: `docs/design-system/engineering-guide.md`

- [ ] **Step 1: ファイルを作成**

以下の内容で新規作成する:

````markdown
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
````

- [ ] **Step 2: 参照実在を確認**

Run: `ls packages/tokens/src/tokens.json packages/tokens/scripts/build.mjs packages/tokens/package.json docs/design-system/components.md apps/desktop/src/main.tsx apps/website/src/app/globals.css 2>&1`
Expected: 全て存在。
Run: `grep -oE "export function (validate|generateCss|generateWebCss|generateTs|main)" packages/tokens/scripts/build.mjs | sort -u`
Expected: 5関数すべてヒット。
Run: `grep -oE '"\./(css|web\.css|ts|tokens\.json)"' packages/tokens/package.json | sort -u`
Expected: `./css` `./ts` `./web.css` `./tokens.json` がヒット。
Run: `grep -n "@shogun-ai/tokens/css" apps/desktop/src/main.tsx && grep -n "@shogun-ai/tokens/web.css" apps/website/src/app/globals.css`
Expected: 両方ヒット。

- [ ] **Step 3: Commit**

```bash
git add docs/design-system/engineering-guide.md
git commit -m "docs(design-system): Engineering Guide"
```

---

## Task 2: copywriting.md

**Files:**
- Create: `docs/design-system/copywriting.md`

- [ ] **Step 1: ファイルを作成**

以下の内容で新規作成する:

````markdown
# SHOGUN デザインシステム — Copywriting

> 出典: `CLAUDE.md`「コード規約」節。UI 文言・外部向けコピーを書く際の規約。

## 言語 / 構造

- UI 文言は**英語（v1）**。
- 文言はコードから分離し **i18n-ready** に保つ（ハードコードしない）。

## ブランドルール

UI 文言・外部向けコピーは SHOGUN ブランドルールに準拠する:

- **競合名を出さない。**
- **技術スタック名を出さない**（実装技術をコピーに露出しない）。
- **絵文字は ⚔ のみ。** 他の絵文字を使わない。
- **禁止フレーズ**: "AI-powered" / "revolutionary" / "second brain"。

## 適用範囲

アプリ内 UI、マーケサイト（`apps/website`）のコピー、リリースノート等の外部向け文言に適用する。プロダクトの一言定義や不変条件は `CLAUDE.md` を参照。
````

- [ ] **Step 2: 出典と一致することを確認**

Run: `grep -nE "競合名を出さない|技術スタック名を出さない|絵文字は⚔のみ|AI-powered|revolutionary|second brain|i18n-ready|UI文言は英語" CLAUDE.md`
Expected: これらの規約が CLAUDE.md に存在（copywriting.md の記述と一致）。

- [ ] **Step 3: Commit**

```bash
git add docs/design-system/copywriting.md
git commit -m "docs(design-system): Copywriting 規約"
```

---

## Task 3: accessibility.md

**Files:**
- Create: `docs/design-system/accessibility.md`

- [ ] **Step 1: ファイルを作成**

以下の内容で新規作成する:

````markdown
# SHOGUN デザインシステム — Accessibility

> 各項目は実コードが裏付け。出典（ファイル/セレクタ）を併記する。

## フォーカス可視性

- 両アプリが `:focus-visible` で 2px の accent アウトライン + offset を付与する。
  - desktop: `apps/desktop/src/styles.css` の `:focus-visible`（`outline: 2px solid var(--accent); outline-offset: 2px;`）。
  - website: `apps/website/src/app/globals.css` の `:focus-visible`（同上）。
- Input はフォーカス時にリング表示（`focus:border-accent` + `focus:ring-4 focus:ring-accent/15`、`apps/website/src/components/ui/input.tsx`）。

## モーション

- `@media (prefers-color-scheme` ではなく `@media (prefers-reduced-motion: reduce)` を両アプリで尊重し、アニメーション/トランジションを無効化・低減する（desktop `styles.css`、website `globals.css`）。

## カラースキーム / テーマ

- website は `html { color-scheme: light dark; }`（`globals.css`）。
- テーマ切替は product=`data-appearance`、website=`data-theme`（詳細は engineering-guide）。

## セマンティクス

- 装飾のみの要素は `aria-hidden` にする（例: Badge の `dot`、`apps/website/src/components/ui/badge.tsx`）。
- 入力のラベルは呼び出し側で `<label>` / `aria-label` を付与する（Input 自体はラベルを内包しない）。

## 参考

コンポーネント個別の a11y メモは `docs/design-system/components.md` を参照。
````

- [ ] **Step 2: 裏付けセレクタが実在することを確認**

Run: `grep -nE ":focus-visible" apps/desktop/src/styles.css apps/website/src/app/globals.css`
Expected: 両ファイルでヒット。
Run: `grep -nE "prefers-reduced-motion: reduce" apps/desktop/src/styles.css apps/website/src/app/globals.css`
Expected: 両ファイルでヒット。
Run: `grep -nE "color-scheme" apps/website/src/app/globals.css && grep -nE "aria-hidden" apps/website/src/components/ui/badge.tsx && grep -nE "ring-accent|border-accent" apps/website/src/components/ui/input.tsx`
Expected: それぞれヒット。

- [ ] **Step 3: Commit**

```bash
git add docs/design-system/accessibility.md
git commit -m "docs(design-system): Accessibility ガイド"
```

---

## Task 4: README.md（index）+ 最終検証

**Files:**
- Create: `docs/design-system/README.md`

- [ ] **Step 1: ファイルを作成**

以下の内容で新規作成する:

````markdown
# SHOGUN デザインシステム

デザインシステムの文書の目次。

## Foundation
トークン基盤（Color / Radius / Type / Motion / Shadow / Blur）。正本と一覧: [`packages/tokens/README.md`](../../packages/tokens/README.md)。

## Components
UI コンポーネント（button / card / input / badge）のカタログ: [`components.md`](./components.md)。

## Documentation
- [Engineering Guide](./engineering-guide.md) — トークンパッケージの構造・消費方法・規約。
- [Copywriting](./copywriting.md) — UI 文言・ブランドコピー規約。
- [Accessibility](./accessibility.md) — フォーカス・モーション・セマンティクス。

## 後続（未整備）
以下の節は今後追加する:
- UX Principles（Notch UX / SLO / 割り込まない原則。根拠は `CLAUDE.md`・`docs/wireframe-spec.md`）。
- Brand Guide（Logo / Colors / Typography / Icons / Illustration）。
- Patterns（Landing / Dashboard / AI Chat / Search / Workspace）。
- Assets（Backgrounds / 3D / Device Mockups / Photos）。
````

- [ ] **Step 2: index のリンク先が実在することを確認**

Run: `ls packages/tokens/README.md docs/design-system/components.md docs/design-system/engineering-guide.md docs/design-system/copywriting.md docs/design-system/accessibility.md 2>&1`
Expected: 全て存在（相対リンクの実体）。

- [ ] **Step 3: プレースホルダ無し & docs のみを確認**

Run: `grep -rnE "TBD|TODO|FIXME|要確認|後で埋める" docs/design-system/`
Expected: 出力なし（「後続（未整備）」の文言は可、曖昧な穴埋めは不可）。
Run: `git status -s`
Expected: `docs/design-system/` 配下の新規4ファイルのみ（website/packages/desktop に変更なし）。

- [ ] **Step 4: Commit**

```bash
git add docs/design-system/README.md
git commit -m "docs(design-system): 文書 index (README)"
```

---

## 完了条件（Definition of Done）

1. `docs/design-system/` に `README.md` / `engineering-guide.md` / `copywriting.md` / `accessibility.md` が存在。
2. 各記述が出典（CLAUDE.md / コード / packages/tokens）と一致し、参照が実在（各タスクの検証ステップで確認済み）。
3. プレースホルダ・リンク切れが無い。
4. コードは無変更（`git diff` は docs のみ）。

## 申し送り

- 本ブランチは components-catalog（#18）にスタック。マージ順は #14 → #15 → #17 → #18 → 本ブランチ。
- 後続: UX Principles / Brand Guide、Brand 節、Patterns / Assets。
