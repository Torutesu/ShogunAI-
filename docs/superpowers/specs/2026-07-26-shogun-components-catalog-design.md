# SHOGUN デザインシステム — Components カタログ（ドキュメント先行）設計

- 日付: 2026-07-26
- 対象ブランチ: `design-system/components-catalog`（ベース: `design-system/shadow-blur-tokens`。スタック: foundation → website → shadow-blur → components-catalog）
- スコープ: website の既存 UI コンポーネント（button/card/input/badge）を、デザインシステムの **Components カタログ**として文書化する。**コードは一切変更しない。**

## 背景 / 課題

デザインシステムのツリーで Foundation（トークン基盤）は整備済み。次の **Components** 節が未整備。

現状のコンポーネント事情:
- `packages/ui`（`@shogun-ai/ui`）は空。index.ts のコメントに「website の shadcn 風コンポーネントを、2つ目のアプリ（desktop 等）が必要になったらここへ移す」とある。
- website には shadcn 風コンポーネントが実装済み: `apps/website/src/components/ui/` に `button.tsx` / `card.tsx` / `input.tsx` / `badge.tsx`。**Tailwind v4 + CVA + clsx + tailwind-merge + @radix-ui/react-slot + lucide**、React 19。`@theme`（website `globals.css`）経由で web トークンに結合。
- desktop は素CSS + React 18 で Tailwind/CVA を使わず、これらの共有コンポーネントを消費できない（**フレームワーク断層**）。

`packages/ui` への移設は、消費者が当面 website のみで、index.ts の基準（2つ目のアプリが必要になったら）も満たさないため**早すぎる抽象化**。よって本サブプロジェクトは**コード移設せず、既存コンポーネントを正確に文書化**して Components 節を確立する。

## ゴール / 非ゴール

**ゴール**
- website の4コンポーネントを、実ソースに忠実な単一の Components カタログとして文書化する。
- 各コンポーネントの variants / props / 使用トークン / 状態 / 使用例 / a11y を、迷わず使える形で記述する。
- デザインシステム文書の置き場所（`docs/design-system/`）を確立する。

**非ゴール**
- コンポーネントのコード変更・移設・リファクタ（一切しない）。
- `packages/ui` への抽出（消費者が増えたら別サブプロジェクトで検討）。
- 新規コンポーネントの追加。
- desktop 向けコンポーネントの設計。
- Storybook 等のツール導入。

## アーキテクチャ

新規ファイル1点のみ。

- `docs/design-system/components.md`（新規）
  - **概要**: これは website の Tailwind+CVA / shadcn 風レイヤーであること。`@theme`（website `globals.css`）→ web トークン（`@shogun-ai/tokens` の web セット）に結合していること。消費者は website（React 19）、desktop は断層で当面非対象であること。
  - **使用トークン一覧**（横断）: `ink` / `on-ink` / `surface` / `cloud` / `border` / `accent` / `accent-strong` / `sky-soft` / `faint`、および生CSS変数 `--shadow-card` / `--shadow-float` / `--ease-out-soft`。
  - **各コンポーネント節**（Buttons / Cards / Inputs / Badges）。各節に: 目的 / ソースパス / variants・sizes・props / 使用トークン / 状態(hover/focus/active/disabled) / 使用例 / a11y。

将来コンポーネントが増えたら `docs/design-system/components/` へ分割する（本設計では単一ファイル）。

### 各コンポーネントの記述内容（実ソース準拠）

- **Button**（`apps/website/src/components/ui/button.tsx`）
  - CVA。`variant`: `primary`（`bg-ink text-on-ink` + hover `shadow-[var(--shadow-float)]`）/ `secondary`（`border-border` 透明背景 / hover `bg-cloud`）/ `tertiary`（下線なしテキスト `text-accent` / hover `text-accent-strong`）/ `ghost`（透明 / hover `bg-cloud`）。
  - `size`: `sm`（h-9）/ `md`（h-11、既定）/ `lg`（h-12）。
  - `asChild`（Radix Slot）。既定 variant=primary, size=md。
  - 形状 pill、`transition-all 200ms ease-[var(--ease-out-soft)]`、focus-visible outline=accent、`active:translate-y-px`、`disabled:opacity-60`。
- **Card**（`card.tsx`）: `bg-surface` / `border-border` / `rounded-lg` / `p-6` / `shadow-[var(--shadow-card)]`。`div` 属性を透過。
- **Input**（`input.tsx`、`forwardRef`）: pill / `h-11` / `w-full` / `bg-surface` / `border-border` / `text-ink` / `placeholder:text-faint`。focus: `border-accent` + `ring-4 ring-accent/15`、`outline-none`。
- **Badge**（`badge.tsx`）: pill / `bg-sky-soft` / `text-ink` / `text-xs` / `tracking-[0.02em]`。`dot`（真偽）で先頭に `bg-accent` の小ドット。

## データフロー

なし（ドキュメントのみ、実行時挙動に影響しない）。カタログは実ソースを唯一の参照元とし、値はソースから転記する。

## エラー処理 / 堅牢性

該当なし（ドキュメント）。ただし記述と実ソースの乖離＝ドリフトがリスクなので、レビューで4ファイルと突き合わせる。

## テスト / 検証

- **一致レビュー**: カタログの variants / sizes / props / 使用トークンが、`button.tsx` / `card.tsx` / `input.tsx` / `badge.tsx` の実ソースと一致することを4ファイルと突き合わせて確認。
- **参照実在**: 記載のソースパス（`apps/website/src/components/ui/*.tsx`）が実在。記載トークン名（website `@theme` の意味名 / 生 `--shadow-*` / `--ease-out-soft`）が website `globals.css`・`@shogun-ai/tokens` の web セットに実在。
- **プレースホルダ無し**: TBD/TODO/未確定が無い。リンク切れ無し。
- **コード無変更**: `git diff` の変更は `docs/` のみ。website / packages / desktop に変更が無い。

## リスク / 留意点

- カタログはコードのミラーであり、コンポーネント改修時に更新が必要（ドリフト）。将来コード移設（`packages/ui`）を行う際にカタログを正本側へ寄せる。
- これらのコンポーネントは website の `@theme` 意味名（`bg-ink` 等）に結合しており、他アプリ（desktop 等）へそのまま流用はできない旨をカタログに明記する。

## 完了条件

1. `docs/design-system/components.md` が存在し、概要・使用トークン一覧・4コンポーネント節を含む。
2. 記述が4つの実ソースと一致し、参照パス・トークン名が実在する。
3. プレースホルダ・リンク切れが無い。
4. コード（website / packages / desktop）は無変更。

## 申し送り

- 本ブランチは shadow-blur-tokens（#17）にスタック。マージ順は #14 → #15 → #17 → 本ブランチ。
- 後続候補: 消費者が増えた時点で `packages/ui` へコード抽出（別サブプロジェクト）。Brand / Patterns / Assets / Documentation の各節。
