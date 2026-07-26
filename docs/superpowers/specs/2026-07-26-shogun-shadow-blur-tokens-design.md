# SHOGUN デザインシステム — shadow/blur トークン化 設計

- 日付: 2026-07-26
- 対象ブランチ: `design-system/shadow-blur-tokens`（ベース: `design-system/website-tokens` = PR #15。スタック: foundation → website → shadow-blur）
- スコープ: desktop が直書きしている構造ガラスの shadow/blur を `@shogun-ai/tokens` の static トークンへ抽出。**見た目不変。**

## 背景 / 課題

Foundation で desktop のトークンは集約済みだが、shadow/blur は対象外だった。desktop `apps/desktop/src/styles.css` は構造ガラス面の影・ブラーを**トークン化せず直書き**し、一部が**重複**している:

| 値 | 用途 | 使用箇所 | 備考 |
|---|---|---|---|
| `blur(30px) saturate(1.7)` | 小サーフェス（handle/mpill） | 51-52, 927-928 | 重複 |
| `blur(38px) saturate(1.8)` | 展開パネル | 101-102 | |
| `0 10px 26px rgba(0, 0, 0, 0.4)` | 小サーフェス影 | 56, 932 | 重複 |
| `0 28px 66px rgba(0, 0, 0, 0.5), inset 0 1px 0 rgba(255, 255, 255, 0.06)` | パネル影 | 106 | |

これらは基底ルールに直書きで **light/dark に依存しない**（現状テーマ非依存）。

wireframe（`docs/wireframes/shogun-ui.css`）にも `--blur`/`--shadow` トークンがあるが**値が別物**で参照専用。**正本は desktop の実値**とする（見た目を変えないため）。

component 級の影は構造トークンではないため対象外: live-dot グロー（127, `--live` 依存の一点物）、フォーカスリング（410, `0 0 0 3px var(--accent-soft)`）、ボタン影（510, `0 4px 13px var(--accent-soft)`）。これらは据え置く。

## ゴール / 非ゴール

**ゴール**
- desktop の構造ガラス shadow/blur（上記4値）を `@shogun-ai/tokens` の `static` に抽出し正本化。
- desktop の直書き6箇所を `var(--…)` 参照に置換し、重複（blur-sm×2, shadow-sm×2）を解消。
- 描画結果は**完全に不変**（値そのまま）。

**非ゴール**
- shadow/blur を light/dark で変える（themed 化）＝見た目変更。しない。
- wireframe 値の採用。しない。
- component 級の影（focus/button/live-dot）のトークン化。しない（別途必要になれば後続）。
- `build.mjs` のロジック変更（static は既存の `staticBlock` が汎用処理するため不要）。
- website / product themed / desktop の他スタイルの変更。

## アーキテクチャ

`@shogun-ai/tokens` の `static` を**加算的に**拡張する。

- `packages/tokens/src/tokens.json` の `static` に4トークンを追加:
  - `blur`: `blur(38px) saturate(1.8)`
  - `blur-sm`: `blur(30px) saturate(1.7)`
  - `shadow`: `0 28px 66px rgba(0, 0, 0, 0.5), inset 0 1px 0 rgba(255, 255, 255, 0.06)`
  - `shadow-sm`: `0 10px 26px rgba(0, 0, 0, 0.4)`
- `packages/tokens/scripts/build.mjs`: **変更なし**。`staticBlock` が `--blur: …;` 等を base `:root` に出力し、`generateTs` にも含まれる。static は `validate` の対象外なので影響なし。カンマ・括弧を含む値も文字列補間で問題なし。
- `apps/desktop/src/styles.css`: 直書き6箇所を `var()` に置換:
  - 51-52, 927-928（`-webkit-backdrop-filter`/`backdrop-filter` の `blur(30px) saturate(1.7)`）→ `var(--blur-sm)`
  - 101-102（`blur(38px) saturate(1.8)`）→ `var(--blur)`
  - 56, 932（`0 10px 26px rgba(0, 0, 0, 0.4)`）→ `var(--shadow-sm)`
  - 106（パネル影）→ `var(--shadow)`
  - 127 / 410 / 510 / 516 は据え置き。
- `packages/tokens/README.md`: 「Shadow / Spacing 未定義」の節を、追加した shadow/blur の記述へ更新。

命名は wireframe 慣習（`--blur`/`--blur-sm`/`--shadow`/`--shadow-sm`）に一致（値は desktop 実値）。

## データフロー

- ビルド時 `main()` が `dist/tokens.css` を生成（既存経路）。static に4トークンが加わるため base `:root` に `--blur`/`--blur-sm`/`--shadow`/`--shadow-sm` が出力される。
- desktop の `styles.css`（`@import "@shogun-ai/tokens/css"` 済み）が `var(--blur)` 等を解決。値は現行と同一のため描画不変。

## エラー処理 / 堅牢性

- static トークンは `validate` の検証対象外（product themed の color 検証・web のモード検証には影響しない）。
- `dist/` は従来どおり gitignore、turbo build で生成。

## テスト / 検証

- **単体**（build.test.mjs 追記）: `generateCss(realTokens)` もしくは生成物に、base `:root` へ `--blur: blur(38px) saturate(1.8)`、`--shadow-sm: 0 10px 26px rgba(0, 0, 0, 0.4)` 等が出力されることを確認。
- **等価性**: 生成 `dist/tokens.css` の4トークン値が desktop の旧直書き値と完全一致。
- **置換完全性**: `apps/desktop/src/styles.css` に構造ガラスの直書き（`blur(30px)`, `blur(38px)`, `0 10px 26px`, `0 28px 66px`）が残っていないことを grep 確認。据え置き対象（127/410/510）は残る。
- **ビルド**: `pnpm --filter @shogun-ai/tokens build` → 生成、`pnpm --filter @shogun-ai/desktop build:vite` → 成功（`var()` 解決）。`pnpm --filter @shogun-ai/tokens test` → 全 pass。
- **回帰**: 値が同一のため見た目不変。最終的に desktop の目視スポットチェック推奨。

## リスク / 留意点

- 重複2ペアを1トークンに集約するため、将来これらを個別に変えたい場合は分離が必要（現状は同一値なので集約が正しい）。
- 置換は `-webkit-backdrop-filter` と `backdrop-filter` の両方を必ず `var(--blur*)` にする（ベンダープレフィックスの片方だけ残さない）。

## 完了条件

1. `tokens.json` の `static` に `blur`/`blur-sm`/`shadow`/`shadow-sm` が追加されている。
2. `pnpm --filter @shogun-ai/tokens build` の `dist/tokens.css` に4トークンが desktop 実値で出力される。
3. desktop `styles.css` の構造ガラス直書き6箇所が `var()` に置換され、直書きが残っていない（据え置き対象を除く）。
4. `pnpm --filter @shogun-ai/tokens test` 全 pass、`pnpm --filter @shogun-ai/desktop build:vite` 成功、見た目不変。
5. website / product themed / build.mjs ロジックは無変更。

## 申し送り

- 本ブランチは website-tokens（#15）にスタック。マージ順は #14 → #15 → 本ブランチ。
- 後続: Components（`packages/ui` が `@shogun-ai/tokens/ts` を参照）。component 級の影が必要ならそこで semantic 化を検討。
