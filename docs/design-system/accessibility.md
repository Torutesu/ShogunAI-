# SHOGUN デザインシステム — Accessibility

> 各項目は実コードが裏付け。出典（ファイル/セレクタ）を併記する。

## フォーカス可視性

- 両アプリが `:focus-visible` で 2px の accent アウトライン + offset を付与する。
  - desktop: `apps/desktop/src/styles.css` の `:focus-visible`（`outline: 2px solid var(--accent); outline-offset: 2px;`）。
  - website: `apps/website/src/app/globals.css` の `:focus-visible`（同上）。
- Input はフォーカス時にリング表示（`focus:border-accent` + `focus:ring-4 focus:ring-accent/15`、`apps/website/src/components/ui/input.tsx`）。

## モーション

- `@media (prefers-reduced-motion: reduce)` を両アプリで尊重し、アニメーション/トランジションを無効化・低減する（desktop `styles.css`、website `globals.css`）。

## カラースキーム / テーマ

- website は `html { color-scheme: light dark; }`（`globals.css`）。
- テーマ切替は product=`data-appearance`、website=`data-theme`（詳細は engineering-guide）。

## セマンティクス

- 装飾のみの要素は `aria-hidden` にする（例: Badge の `dot`、`apps/website/src/components/ui/badge.tsx`）。
- 入力のラベルは呼び出し側で `<label>` / `aria-label` を付与する（Input 自体はラベルを内包しない）。

## 参考

コンポーネント個別の a11y メモは `docs/design-system/components.md` を参照。
