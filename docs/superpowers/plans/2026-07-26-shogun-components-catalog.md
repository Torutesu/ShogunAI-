# SHOGUN Components カタログ 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** website の既存 UI コンポーネント（button/card/input/badge）を、実ソース準拠の単一 Components カタログ `docs/design-system/components.md` として文書化する（コード無変更）。

**Architecture:** `docs/design-system/` を新設し、`components.md` 1ファイルを作成。内容は下記の「正本の値」を実ソース（`apps/website/src/components/ui/*.tsx`）から転記したもの。ドキュメントのみ、実行時挙動に影響しない。

**Tech Stack:** Markdown。参照元は Tailwind v4 + CVA の website コンポーネント。

---

## 正本の値（website ソースより確定・転記元）

- `button.tsx`: CVA。base クラスに `inline-flex ... rounded-full font-medium ... transition-all duration-200 ease-[var(--ease-out-soft)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent active:translate-y-px disabled:pointer-events-none disabled:opacity-60`。
  - variant: `primary`=`bg-ink text-on-ink hover:opacity-90 hover:shadow-[var(--shadow-float)]` / `secondary`=`border border-border bg-transparent text-ink hover:bg-cloud` / `tertiary`=`rounded-none px-0 text-accent hover:text-accent-strong` / `ghost`=`bg-transparent text-ink hover:bg-cloud`。
  - size: `sm`=`h-9 px-4 text-sm` / `md`=`h-11 px-[22px] text-[15px]` / `lg`=`h-12 px-7 text-base`。
  - defaultVariants: `variant: primary, size: md`。props: `ButtonHTMLAttributes` + `asChild?`（Radix `Slot`）。`buttonVariants` も export。
- `card.tsx`: `rounded-lg border border-border bg-surface p-6 shadow-[var(--shadow-card)]`。`div` 属性を透過。
- `input.tsx`（`forwardRef`）: `h-11 w-full rounded-full border border-border bg-surface px-[18px] text-[15px] text-ink` + `placeholder:text-faint focus:border-accent focus:outline-none focus:ring-4 focus:ring-accent/15`。
- `badge.tsx`: `inline-flex items-center gap-2 rounded-full bg-sky-soft px-3 py-1.5 text-xs font-medium tracking-[0.02em] text-ink`。props: `HTMLAttributes<HTMLSpanElement>` + `dot?: boolean`（true で先頭に `size-1.5 rounded-full bg-accent` のドット、`aria-hidden`）。

---

## File Structure

- 新規: `docs/design-system/components.md` — Components カタログ（唯一の成果物）

---

## Task 1: Components カタログを作成

**Files:**
- Create: `docs/design-system/components.md`

- [ ] **Step 1: `docs/design-system/components.md` を作成**

以下の内容で新規作成する（値は上記「正本の値」に一致。改変しない）:

````markdown
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
- **状態**: なし（静的）。
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
````

- [ ] **Step 2: 参照パスが実在することを確認**

Run: `ls apps/website/src/components/ui/button.tsx apps/website/src/components/ui/card.tsx apps/website/src/components/ui/input.tsx apps/website/src/components/ui/badge.tsx apps/website/src/lib/utils.ts 2>&1`
Expected: 5パス全て存在（`utils` は `.ts` か `.tsx`。存在しなければ `ls apps/website/src/lib/` で実ファイル名を確認し、カタログ本文の `apps/website/src/lib/utils` 記述が実在パスと一致するか確認。ディレクトリ import なら `utils` のままで可）。

- [ ] **Step 3: 記載トークン名が website の @theme に実在することを確認**

Run: `grep -oE "\-\-color-(ink|on-ink|surface|cloud|border|accent|accent-strong|sky-soft|faint)" apps/website/src/app/globals.css | sort -u`
Expected: 9個の意味名が全てヒット（`@theme inline` の `--color-*` として定義済み）。
Run: `grep -oE "\-\-(shadow-card|shadow-float|ease-out-soft)" apps/website/src/app/globals.css | sort -u`
Expected: `--shadow-card` / `--shadow-float` / `--ease-out-soft` がヒット。

- [ ] **Step 4: プレースホルダが無いこと & docs のみの変更を確認**

Run: `grep -nE "TBD|TODO|FIXME|要確認|後で" docs/design-system/components.md`
Expected: 出力なし。
Run: `git status -s`
Expected: 追加は `docs/design-system/components.md` のみ（他に tracked 変更が無い。website/packages/desktop に変更が無い）。

- [ ] **Step 5: Commit**

```bash
git add docs/design-system/components.md
git commit -m "docs(design-system): Components カタログ(button/card/input/badge)"
```

---

## Task 2: 実ソースとの一致最終確認

**Files:**
- Read-only cross-check（必要なら `docs/design-system/components.md` を修正）

- [ ] **Step 1: 4ソースとカタログを突き合わせる**

`apps/website/src/components/ui/button.tsx` / `card.tsx` / `input.tsx` / `badge.tsx` を READ し、カタログの以下が実ソースと一致することを確認する:
- Button の variant 4種（primary/secondary/tertiary/ghost）と各クラス、size 3種（sm/md/lg）、defaultVariants、`asChild`、`buttonVariants` export。
- Card の `bg-surface border-border rounded-lg p-6 shadow-[var(--shadow-card)]`。
- Input の pill/h-11/w-full/border-border/bg-surface/text-ink/placeholder:text-faint/focus(border-accent, ring-4 ring-accent/15)、`forwardRef`。
- Badge の bg-sky-soft/text-ink/text-xs/tracking と `dot` 挙動（`bg-accent` ドット・`aria-hidden`）。

- [ ] **Step 2: 乖離があれば修正**

一致していれば変更なし。乖離があればカタログを実ソースに合わせて修正し、`git add docs/design-system/components.md && git commit -m "docs(design-system): カタログを実ソースに整合"` する。乖離が無ければコミット不要（このタスクは確認のみ）。

- [ ] **Step 3: 最終確認**

Run: `git diff --stat design-system/shadow-blur-tokens..HEAD`
Expected: 変更は `docs/` 配下のみ（`docs/superpowers/specs/...`, `docs/superpowers/plans/...`, `docs/design-system/components.md`）。website/packages/desktop に変更が無い。

---

## 完了条件（Definition of Done）

1. `docs/design-system/components.md` が存在し、概要・使用トークン一覧・Buttons/Cards/Inputs/Badges の4節を含む。
2. variants/props/使用トークンが4つの実ソースと一致する。
3. 参照パス・トークン名が実在（Task 1 Step 2–3 で確認済み）。
4. プレースホルダ・リンク切れが無い。
5. コード（website/packages/desktop）は無変更（`git diff` は docs のみ）。

## 申し送り

- 本ブランチは shadow-blur-tokens（#17）にスタック。マージ順は #14 → #15 → #17 → 本ブランチ。
- 後続候補: 消費者が増えたら `packages/ui` へコード抽出。Brand / Patterns / Assets / Documentation の各節。
