# SHOGUN デザインシステム — Components

このカタログは、マーケサイト `apps/website` が使う UI コンポーネント層を記述する。実装は **Tailwind v4 + [class-variance-authority (CVA)](https://cva.style) + `@radix-ui/react-slot`**（shadcn 風、React 19）。各コンポーネントは Tailwind の意味名クラス（`bg-ink`, `bg-surface`, `text-accent` 等）でスタイルされ、それらは website `globals.css` の `@theme` を通じて `@shogun-ai/tokens` の **web トークンセット**に結合している。

> **注意（フレームワーク断層）**: これらは website の `@theme` 意味名に結合しており、そのままでは他アプリに流用できない。desktop（React 18・素CSS・Tailwind 非使用）はこのコンポーネント層を消費しない。共有パッケージ（`@shogun-ai/ui`）への抽出は、2つ目の消費者が現れた時点で別途検討する。
>
> ソース: `apps/website/src/components/ui/`（`button.tsx` / `card.tsx` / `input.tsx` / `badge.tsx`）。共通ヘルパ `cn`（clsx + tailwind-merge）は `apps/website/src/lib/utils`。

## 使用トークン（横断）

意味名（website `@theme` → web トークン）: `ink` / `on-ink` / `surface` / `cloud` / `border` / `accent` / `accent-strong` / `sky-soft` / `faint`。
生CSS変数（website `globals.css` の `@theme`）: `--shadow-card` / `--shadow-float` / `--ease-out-soft`。
形状: ボタン・入力・バッジは pill（`rounded-full`）、カードは `rounded-lg`。

---

## Buttons

- **ソース**: `apps/website/src/components/ui/button.tsx`
- **目的**: 主要アクション用ボタン。CVA で variant/size を切り替える。
- **variant**:
  - `primary`（既定）— `bg-ink text-on-ink`、hover で不透明度と `--shadow-float`。
  - `secondary` — `border-border` の透明背景、hover で `bg-cloud`。
  - `tertiary` — インラインのテキストリンク調（`text-accent`、hover `text-accent-strong`、パディングなし）。
  - `ghost` — 透明背景、hover で `bg-cloud`。
- **size**: `sm`（h-9）/ `md`（h-11、既定）/ `lg`（h-12）。
- **props**: 標準の `<button>` 属性 + `asChild?: boolean`（Radix `Slot`。`true` で子要素をボタンとしてレンダリング、例: リンクをボタン外観に）。`buttonVariants` を別途 export（クラス生成の再利用用）。
- **使用トークン**: `ink` / `on-ink` / `border` / `cloud` / `accent` / `accent-strong` / `--shadow-float` / `--ease-out-soft`。
- **状態**: hover（variant 別）、`focus-visible`（`outline-2 outline-offset-2 outline-accent`）、`active`（`translate-y-px`）、`disabled`（`pointer-events-none opacity-60`）。
- **使用例**:
  ```tsx
  import { Button } from '@/components/ui/button';

  <Button>送信</Button>
  <Button variant="secondary" size="lg">詳細</Button>
  <Button asChild><a href="/pricing">価格を見る</a></Button>
  ```
- **a11y**: フォーカスは `focus-visible` で可視リング。`asChild` 使用時はラップ要素側で適切なロール/属性を付与する。

## Cards

- **ソース**: `apps/website/src/components/ui/card.tsx`
- **目的**: コンテンツをまとめる面。`div` の薄いラッパ。
- **props**: 標準の `<div>` 属性（`className` は `cn` でマージ）。variant なし。
- **スタイル/使用トークン**: `bg-surface` / `border-border` / `rounded-lg` / `p-6` / `--shadow-card`。
- **状態**: なし（静的な面）。インタラクションが要る場合は呼び出し側で付与。
- **使用例**:
  ```tsx
  import { Card } from '@/components/ui/card';

  <Card>
    <h3>見出し</h3>
    <p>本文</p>
  </Card>
  ```
- **a11y**: 意味的なランドマーク/見出しは中身側で担保する（Card 自体は装飾）。

## Inputs

- **ソース**: `apps/website/src/components/ui/input.tsx`
- **目的**: 単一行テキスト入力。`forwardRef` で ref 透過。
- **props**: 標準の `<input>` 属性（`type`, `placeholder`, `value` 等）。variant なし。
- **スタイル/使用トークン**: pill（`rounded-full`）/ `h-11` / `w-full` / `bg-surface` / `border-border` / `text-ink` / `placeholder:text-faint`。
- **状態**: focus — `border-accent` + `ring-4 ring-accent/15`、`outline-none`。
- **使用例**:
  ```tsx
  import { Input } from '@/components/ui/input';

  <Input type="email" placeholder="you@example.com" />
  ```
- **a11y**: ラベルは呼び出し側で `<label>`／`aria-label` を付与する。focus リングで可視性を確保。

## Badges

- **ソース**: `apps/website/src/components/ui/badge.tsx`
- **目的**: 見出しのアイブロウやステータスチップに使う sky 色の pill。
- **props**: 標準の `<span>` 属性 + `dot?: boolean`（`true` で先頭に `bg-accent` の小ドット、`aria-hidden`）。
- **スタイル/使用トークン**: pill / `bg-sky-soft` / `text-ink` / `text-xs` / `font-medium` / `tracking-[0.02em]`。ドットは `accent`。
- **使用例**:
  ```tsx
  import { Badge } from '@/components/ui/badge';

  <Badge>New</Badge>
  <Badge dot>Live</Badge>
  ```
- **a11y**: ドットは装飾（`aria-hidden`）。意味はテキストで伝える。

---

## メンテナンス

このカタログはコンポーネント実装のミラーである。`apps/website/src/components/ui/*.tsx` を変更したら本ファイルも更新すること。将来 `@shogun-ai/ui` へコード抽出する場合は、カタログを正本側へ寄せる。
