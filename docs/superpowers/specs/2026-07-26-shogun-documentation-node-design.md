# SHOGUN デザインシステム — Documentation 節（第1スライス）設計

- 日付: 2026-07-26
- 対象ブランチ: `design-system/documentation-node`（ベース: `design-system/components-catalog`。スタック: foundation → website → shadow-blur → components → documentation）
- スコープ: デザインシステムの **Documentation 節**のうち、既存コード/規約に裏付けのある3ドキュメント（Engineering Guide / Copywriting / Accessibility）＋ index を `docs/design-system/` に整備。**コード無変更・内容の発明なし。**

## 背景 / 課題

デザインシステムのツリーで Foundation（`packages/tokens/README.md`）と Components（`docs/design-system/components.md`）は整備済み。**Documentation 節**（Brand Guide / UX Principles / Accessibility / Copywriting / Engineering Guide）が未整備で、規約・原則が散在している:

- `CLAUDE.md`（103行）: ブランドコピー規約（英語v1・i18n-ready、競合名/技術名を出さない、絵文字は⚔のみ、"AI-powered/revolutionary/second brain"禁止）。
- 実コード: focus-visible outline（`apps/desktop/src/styles.css` `:focus-visible`、`apps/website/src/app/globals.css` `:focus-visible`）、`prefers-reduced-motion`（両アプリ）、`color-scheme`（website）、装飾要素の `aria-hidden`（Badge ドット）。
- `docs/wireframe-spec.md`: UX インベントリ・トークン表・「パネル内は14px超の文字なし」等（既に大部の UX 資料）。

内容を発明せず**既存の裏付けがあるものだけ**を体系化する。Brand Guide / UX Principles は解釈・発明が増える（Brand は別ノードとも重複）ため本スライスでは扱わず、index に「未整備（後続）」として明示する。

## ゴール / 非ゴール

**ゴール**
- `docs/design-system/` に文書の目次（index）と、根拠の固い3ドキュメントを整備する。
- 各ドキュメントは既存コード/規約/これまでの成果物（`@shogun-ai/tokens` 等）に忠実で、参照が実在する。

**非ゴール**
- コード変更（一切なし）。
- UX Principles / Brand Guide の作成（発明を伴うため後続。index にリンク先未整備として記載）。
- 新規の規約・原則の考案（既存の体系化のみ）。
- Foundation（`packages/tokens/README.md`）/ Components（`components.md`）の内容変更（index からリンクするのみ）。

## アーキテクチャ

`docs/design-system/` に4ファイルを新設。

- `docs/design-system/README.md`（index）
  - デザインシステム文書の目次。既存へのリンク: Foundation=`../../packages/tokens/README.md`、Components=`components.md`。本スライスの3点へのリンク: `engineering-guide.md` / `copywriting.md` / `accessibility.md`。
  - 未整備節（UX Principles / Brand Guide / Patterns / Assets）を「後続」として明示。
- `docs/design-system/engineering-guide.md`
  - `@shogun-ai/tokens` の構造: `src/tokens.json`（`static` / `themed`=product light/dark / `web`=website light/dark）。`scripts/build.mjs` の関数（`validate` / `generateCss` / `generateWebCss` / `generateTs` / `main`）。`dist/` は gitignore、turbo build で生成（`build` は `outputs: dist/**`、`dev` は `dependsOn: ["^build"]`）。
  - exports と消費方法: `./css`（desktop `main.tsx` で `import "@shogun-ai/tokens/css"`）、`./web.css`（website `globals.css` で `@import '@shogun-ai/tokens/web.css'` → `@theme inline` で Tailwind に接続）、`./ts`（型付き定数）。
  - テーマ切替: product=`data-appearance="dark|light|auto"`、website=`data-theme="dark|light"`（+ system media）。2セットは別パレット（統一しない）。
  - shadow/blur は static（desktop 実値、`--blur`/`--blur-sm`/`--shadow`/`--shadow-sm`）。
  - 規約: Conventional Commits（`feat:`/`fix:`/`docs:` 等、CLAUDE.md 準拠）。トークン変更は tokens.json を正本に、`pnpm --filter @shogun-ai/tokens build`/`test`。
- `docs/design-system/copywriting.md`
  - CLAUDE.md 103行のブランドコピー規約を整理: UI文言は英語（v1）でコードから分離し i18n-ready。ブランドルール: 競合名を出さない / 技術スタック名を出さない / 絵文字は ⚔ のみ / 禁止フレーズ "AI-powered" "revolutionary" "second brain"。
  - 補足として出典（`CLAUDE.md`「コード規約」節）を明記。
- `docs/design-system/accessibility.md`
  - focus 可視性: 両アプリの `:focus-visible`（2px の accent アウトライン + offset）。Input はフォーカスリング（`ring-4 ring-accent/15` + `border-accent`）。
  - モーション: `prefers-reduced-motion: reduce` を両アプリで尊重（アニメを無効化/低減）。
  - カラースキーム: website の `color-scheme: light dark`。テーマは `data-appearance`/`data-theme`。
  - 装飾要素: 意味を持たない要素は `aria-hidden`（例: Badge の `dot`）。ラベルは呼び出し側で `<label>`/`aria-label` を付与。
  - 各項目に出典（ファイル/セレクタ）を併記。

将来 UX Principles / Brand Guide を追加する際は同ディレクトリに並べ、index を更新する。

## データフロー

なし（ドキュメントのみ）。各ドキュメントは既存コード/規約を唯一の参照元とし、記述はそこから転記・整理する。

## エラー処理 / 堅牢性

該当なし。ドリフト（記述と実体の乖離）がリスクなので、レビューで出典（コード/CLAUDE.md）と突き合わせる。

## テスト / 検証

- **一致レビュー**: 各ドキュメントの記述が出典（`CLAUDE.md`、`apps/*/…` のコード、`packages/tokens/*`、これまでの成果物）と一致することを突き合わせる。
- **参照実在**: index のリンク先（`../../packages/tokens/README.md`、`components.md`、3ドキュメント）が実在。engineering-guide が挙げる関数名/exports/ファイルが `build.mjs`/`package.json` に実在。a11y が挙げるセレクタ（`:focus-visible`、`prefers-reduced-motion`、`color-scheme`）が実コードに存在。copywriting の規約が CLAUDE.md と一致。
- **プレースホルダ無し**: TBD/TODO が無い（未整備節は「後続」と明示するのは可、曖昧な穴埋めは不可）。リンク切れ無し。
- **コード無変更**: `git diff` は `docs/` のみ。

## リスク / 留意点

- ドキュメントは実体のミラーであり、基盤/規約の変更時に更新が必要（ドリフト）。engineering-guide 冒頭に「正本はコード、本書は要約」と明記する。
- 未整備節を index に載せるが、リンク先を作らずプレースホルダにしない（「後続で追加」と文で示す）。

## 完了条件

1. `docs/design-system/` に `README.md`（index）/ `engineering-guide.md` / `copywriting.md` / `accessibility.md` の4ファイルが存在。
2. 各記述が出典（CLAUDE.md / コード / packages/tokens）と一致し、参照が実在する。
3. プレースホルダ・リンク切れが無い。
4. コードは無変更（`git diff` は docs のみ）。

## 申し送り

- 本ブランチは components-catalog（#18）にスタック。マージ順は #14 → #15 → #17 → #18 → 本ブランチ。
- 後続: Documentation の UX Principles / Brand Guide、Brand 節（Logo/Colors/Typography/Icons）、Patterns / Assets。
