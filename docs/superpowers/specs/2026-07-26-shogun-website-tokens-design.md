# SHOGUN デザインシステム — website トークン統合 設計

- 日付: 2026-07-26
- 対象ブランチ: `design-system/website-tokens`（ベース: `design-system/foundation-tokens` = PR #14。`@shogun-ai/tokens` パッケージに依存するため、その上に積む）
- スコープ: マーケサイト `apps/website` の生トークン定義を `@shogun-ai/tokens` の単一正本へ引っ越す。**見た目は不変。**

## 背景 / 課題

Foundation ブランチで desktop（product）のトークンは `@shogun-ai/tokens` に集約済み。一方 `apps/website` の色・効果トークンは `apps/website/src/app/globals.css`（7〜71行）に直書きのまま。

重要な前提: **website と desktop は意図的に別パレット**。desktop はダークなガラス（`--accent #6ea8fe`、`data-appearance` 切替、dark 基準）、website は明るい "Skyglass"（`--accent #00a6f4`、`--sky`/`--cloud`/`--band`、`data-theme` 切替、light 基準）。`CLAUDE.md` にも「website は本ファイル対象外・独自規約」と明記。

方針は**「2テーマ共存」**: 単一パッケージ `@shogun-ai/tokens` の中に product / web の2セットを持ち、それぞれの値はそのまま、置き場所（正本・生成・型の仕組み）だけ統一する。

Foundation ブランチで暫定的に入れた `tokens.json` の `$website_mapping`（`bg→glass` 等）は**誤り**（glass は半透明パネル値でページ背景に使えず、accent 色も別物）。本ブランチで削除し、正しい web 実トークンに差し替える。

## ゴール / 非ゴール

**ゴール**
- website の生トークンを `tokens.json` の `web` セクションに集約（single source）。
- そこから `dist/tokens.web.css` を生成し、website がそれを参照。
- website の描画結果は**完全に不変**（値をそのまま移設）。

**非ゴール**
- website のパレット変更・デザイン変更（一切しない）。
- product 側トークン / desktop / `tokens.ts`（型）の変更（一切しない。今回の変更は加算のみ）。
- `@theme inline` マッピング・fonts・radius・shadow・ease・`@layer base`・utilities・animations の変更（website 固有の配線であり触らない）。
- website 用 TS 型の提供（website は CSS 消費のみ。YAGNI）。

## アーキテクチャ

`@shogun-ai/tokens` を**加算的に**拡張する（product 経路は不変）。

- `packages/tokens/src/tokens.json`
  - 既存の top-level `static` / `themed`（=product）は**そのまま**。
  - `$website_mapping` を**削除**。
  - `web: { themed: { <18トークン, light/dark> } }` を**追加**。
- `packages/tokens/scripts/build.mjs`
  - `generateWebCss(webThemed)` を追加（web の3ブロック構造を生成、light 基準）。
  - `validate()` を拡張: `web.themed` は dark/light 両モードの存在のみ検証（**color 形式検証はかけない** — `orb-blend`/`orb-opacity` など非色値を含むため）。
  - `main()` を拡張: `dist/tokens.web.css` も書き出す。
- `packages/tokens/package.json`
  - `exports` に `"./web.css": "./dist/tokens.web.css"` を追加。
- `apps/website/package.json`
  - `dependencies` に `"@shogun-ai/tokens": "workspace:*"` を追加。
- `apps/website/src/app/globals.css`
  - 7〜71行（`:root` / `:root[data-theme='dark']` / system `@media` の3トークンブロック）を削除。
  - `@import 'tailwindcss';`（1行目）の直後に `@import '@shogun-ai/tokens/web.css';` を追加。
  - それ以外（`@theme inline` 以下すべて）は無変更。

### web トークン一覧（globals.css の値に厳密一致）

`web.themed` の各キーは `{ "light": <:root値>, "dark": <dark値> }`。light は現行 `:root`(7〜26行)、dark は現行 `:root[data-theme='dark']`(28〜47行) と system `@media`(50〜71行、dark と同値) の値。

| token | light | dark |
|---|---|---|
| bg | #ffffff | #090b0d |
| surface | #ffffff | #14181b |
| cloud | #f7fdff | #10151a |
| ink | #090b0c | #eef2f4 |
| on-ink | #fafafa | #090b0d |
| muted | #5f6b73 | #97a3ac |
| faint | #9aa3a9 | #6b7780 |
| border | #e5e7eb | #262d33 |
| sky | #97e5ff | #2a7ba3 |
| sky-soft | #d8f6ff | #103245 |
| accent | #00a6f4 | #38bdf8 |
| accent-strong | #0089cf | #7dd3fc |
| danger | #ef4444 | #f87171 |
| band | #090b0c | #05070a |
| band-ink | #ffffff | #ffffff |
| orb-blend | multiply | screen |
| orb-opacity | 0.55 | 0.4 |
| hairline | rgba(9, 11, 12, 0.06) | rgba(255, 255, 255, 0.06) |

### 生成する web CSS の構造（現行を忠実再現）

```
:root { <web light values> }
:root[data-theme='dark'] { <web dark values> }
@media (prefers-color-scheme: dark) {
  :root:not([data-theme='light']) { <web dark values> }
}
```

## データフロー

- ビルド時に `main()` が product 用 `dist/tokens.css`（不変）と web 用 `dist/tokens.web.css`（新規）を生成。
- website の CSS パイプライン（Next 16 + Tailwind v4）が `@import '@shogun-ai/tokens/web.css'` を解決し `--bg` 等を `:root` に供給。
- 既存の `@theme inline { --color-bg: var(--bg); … }` は変更なし。Tailwind ユーティリティ（例 `.bg-bg`）は `var(--bg)` を出力し、色はブラウザのカスケードで解決される（現行と同じ挙動）。生トークンを import に移しても解決タイミング・結果は不変。

## エラー処理 / 堅牢性

- `validate()` の web 検証: `web.themed` の各トークンが `light` と `dark` の両方を持つことを確認。欠損時はエラー配列に追加し、`main()` が `process.exit(1)` で停止。
- color 形式検証は product（`themed`）のみに適用し、web には適用しない（非色トークンを含むため）。
- `dist/` は従来どおり gitignore。turbo build で生成。

## テスト / 検証

- **単体**（build.test.mjs 追記）: `generateWebCss` が3ブロック（light 基準の `:root`、`[data-theme='dark']`、system `@media` の `:root:not([data-theme='light'])`）を出力し、light 値が base に、dark 値が dark ブロックと system ブロックに入ること。`validate` が web のモード欠損を検出すること。
- **等価性**: 生成した `dist/tokens.web.css` の各ブロックの `--name: value` 集合が、globals.css から削除した3ブロックと**完全一致**することを確認（独立照合）。
- **ビルド**: website のビルド（`pnpm --filter @shogun-ai/website build` もしくは typecheck+build）が成功し、`@import '@shogun-ai/tokens/web.css'` が解決され、Tailwind ユーティリティが従来どおり生成されること。**これが最重要リスクポイント**（Tailwind v4 のパッケージ `@import` 解決）。失敗時は import 方式を見直す。
- **回帰**: 可能ならビルド出力の CSS に `--bg`/`--accent` 等が現行値で存在することを確認。最終的な目視スポットチェックを推奨。

## リスク / 留意点

- **Tailwind v4 のパッケージ `@import` 解決**が最大の不確実性。`@import '@shogun-ai/tokens/web.css'` が Next/Tailwind のパイプラインで解決されない場合、代替（例: globals.css で相対 import 不可のため、パッケージ側の配布形態やビルド前コピー等）を検討。実装時に必ずビルドで確認する。
- website の基準テーマ（light）・切替属性（`data-theme`）・system セレクタ（`:root:not([data-theme='light'])`）は product と異なるため、web 専用ジェネレータで正確に再現する。
- product 経路は一切変更しないため、既存の9テストと desktop は影響を受けない。

## 完了条件

1. `tokens.json` に `web.themed`（18トークン）が入り、`$website_mapping` が削除されている。
2. `pnpm --filter @shogun-ai/tokens build` が `dist/tokens.web.css` を生成し、内容が globals.css の旧3ブロックと完全一致。
3. `pnpm --filter @shogun-ai/tokens test` が全 pass（product 既存 + web 新規）。
4. website がビルド成功、`@import` 解決 OK、Tailwind ユーティリティ生成が従来どおり。見た目不変。
5. product / desktop / `tokens.ts` は無変更。

## 後続への申し送り

- shadow/blur 統合（product 側、wireframe 由来）、Components（`packages/ui`）は別ブランチ。
- 本ブランチは `design-system/foundation-tokens` にスタックしているため、PR は #14 マージ後にベースを feat へ張り替えるか、#14 → 本ブランチの順でマージする。
