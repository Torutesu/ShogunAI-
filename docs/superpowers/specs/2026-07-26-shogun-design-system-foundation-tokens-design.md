# SHOGUN デザインシステム — Foundation（トークン基盤）設計

- 日付: 2026-07-26
- 対象ブランチ: `design-system/foundation-tokens`（ベース: `origin/claude/shogunai-ui-lp-lisvsd`）
- スコープ: デザインシステム全体ツリー（Brand / Foundation / Components / Patterns / Assets / Documentation）のうち **Foundation（トークン基盤）のみ**。以降のセクションは別ブランチで継続する。

## 背景 / 課題

トークン相当の定義が 3 系統に分岐している：

1. `apps/desktop/src/styles.css` — グラス系命名（`--glass` / `--accent #6ea8fe` / `--fs-*` / `--r-*` / `--sp`）、`data-appearance` で dark/light/auto 切替。**実装が最も進んでおり基準とする。**
2. `docs/wireframes/shogun-ui.css` — 似ているが値が微妙に相違（`--accent #2F6FED`、`--r-md` 14 vs 13、glass rgba 差）。
3. `apps/website/src/app/globals.css` — 別語彙（`--bg` / `--surface` / `--cloud` / `--ink` / `--on-ink`）、Tailwind v4 `@theme inline`、`data-theme` で切替。

統合された「デザインシステム」は未整備。`packages/ui`（`@shogun-ai/ui`）は器のみで中身は空。

## ゴール / 非ゴール

**ゴール**
- 散在するトークンを **単一の機械可読な正本**に集約する。
- desktop を正本参照へ切替え、**見た目を一切変えず**に二重定義を解消する。
- 以降（Components / website 移行）の土台となる、型付き・テーマ対応の配布物を用意する。

**非ゴール（この段では扱わない）**
- website（Tailwind `@theme` / `--bg` 語彙）の移行 — 次ブランチ。マッピング表のみ用意。
- Components / Patterns / Assets / Brand ロゴ等の実装。
- 生成ツールチェーンの高度化（Style Dictionary 等の導入）。手製の最小スクリプトに留める。

## アーキテクチャ

新パッケージ **`@shogun-ai/tokens`（`packages/tokens`）** を単一正本とする。

```
packages/tokens/
├─ src/tokens.json      # 正本（機械可読）: primitives + semantic, light/dark/auto
├─ scripts/build.mjs    # tokens.json → CSS変数 + TS を生成（検証付き）
├─ scripts/build.test.mjs  # 生成結果の検証テスト
├─ dist/                # 生成物（.gitignore、turbo build で生成）
│   ├─ tokens.css       # :root[data-appearance=dark|light|auto] の CSS変数
│   └─ tokens.ts        # 型付きトークン定数
├─ package.json         # build スクリプト / exports (./css, ./ts)
├─ tsconfig.json        # packages/config/tsconfig.base.json を継承
└─ README.md            # Foundation ドキュメント
```

- pnpm workspaces（`packages/*`）に自然に所属。
- turbo `build` タスク（`outputs: dist/**`）に乗る。`build.mjs` が dist を生成。
- desktop の `build` / `dev` が `@shogun-ai/tokens` の生成物に依存するよう、依存関係（`^build`）を通す。

### トークンの二層構造

- **primitives**: 生の値（色 hex / rgba、blur、数値）。テーマ非依存の原子。
- **semantic**: 用途名。primitives を参照し、`dark` / `light` / `auto` の各モードで値を割り当てる。
  - 対象セマンティック（desktop 基準）: `--glass` `--glass-2` `--line` `--line-strong` `--ink` `--muted` `--faint` `--accent` `--accent-2` `--accent-ink` `--accent-soft` `--live` `--warn` `--fill` `--fill-strong` `--card` / タイプスケール `--fs-xs…xl` / 半径 `--r-xs…xl` `--r-pill` / モーション `--sp` / フォント `--sys`。

### website マッピング表

`tokens.json` に `website` セクションを設け、website 語彙（`--bg` `--surface` `--cloud` `--on-ink` 等）→ セマンティックトークンの対応表のみを記述する。**この段では生成にもアプリにも接続しない**。次ブランチの移行を楽にするための覚書。

## データフロー（消費のされ方）

- **desktop**: `apps/desktop/src/styles.css` 冒頭の手書き `:root{…}` / `:root[data-appearance=...]` トークン定義ブロックを削除し、`@import "@shogun-ai/tokens/css";` に置換。primitives / semantic の**値は desktop 現行と一致**させるため、描画結果は不変。
- **website**: 本ブランチでは未接続（マッピング表のみ）。
- **packages/ui**: 将来のコンポーネントが `@shogun-ai/tokens` の TS 型を参照できる導線を用意（依存追加のみ、実装はしない）。

## エラー処理 / 堅牢性

- `build.mjs` は tokens.json を検証する：
  - semantic が参照する primitive が未定義でないか。
  - キー重複がないか。
  - カラー値が hex / rgba / color-mix のいずれかとして妥当か。
  - 全 semantic が全モード（dark/light/auto）で解決できるか。
- 検証失敗時は **非ゼロ終了**して turbo を止める。
- `dist/` はコミットせず `.gitignore`。turbo がビルドで生成する。

## テスト / 検証

- **単体**（`scripts/build.test.mjs`）: 生成した `tokens.css` に主要セマンティックが全モードで存在し、`tokens.ts` の型が引けることを検証。
- **回帰**: desktop を起動し、置換前後で Notch / 設定画面のスクリーンショットを比較（見た目不変を確認）。
- **型**: `pnpm typecheck` がパス。
- **ビルド**: `pnpm build`（turbo）がパス、`dist/` が生成される。

## リスク / 留意点

- desktop の値を「正本」とするため、`shogun-ui.css` と食い違う箇所（accent 等）は desktop 側に寄せる。wireframe 側は将来同期する前提でこの段では変更しない。
- `data-appearance`（desktop）と `data-theme`（website）で切替属性が異なる。正本 CSS は desktop の `data-appearance` を採用し、website 用の属性差は移行ブランチで吸収する。
- 退避した `CLAUDE.md`（料金記述）は `feat/meeting-notes-issue-7` の stash@{0} にある。作業完了後に復元が必要。

## 完了条件

1. `packages/tokens` が存在し `pnpm build` で `dist/{tokens.css,tokens.ts}` を生成する。
2. desktop が正本を参照し、見た目が回帰していない（スクショ確認）。
3. `pnpm typecheck` / `pnpm build` がパス。
4. Foundation README にトークン一覧（Color / Radius / Shadow / Motion / Spacing / Type）が記載されている。
