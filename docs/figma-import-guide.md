# Figma取り込みガイド — ワイヤーフレームをFigmaでブラッシュアップする

HTMLワイヤーフレーム(`docs/wireframes/`)をFigmaに取り込み、デザイナーがクオリティを上げ、
仕上がったデザインをFigma MCP経由で読み取って `apps/desktop` に実装する——その往復のための手順書。

- 対象Figmaファイル: **SHOGUN — App Wireframes**
- 取り込み元: `docs/wireframes/figma-import/` (本リポジトリで自動生成した静的フレーム)
- 作成日: 2026-08-07

## 全体ワークフロー

```
docs/wireframes/*.html (インタラクティブな正本)
        │  scripts/export-wireframes-figma.mjs
        ▼
docs/wireframes/figma-import/{dark,light}/*.html (静的フレーム 70枚)
        │  html.to.design プラグインでインポート (デザイナー、手作業)
        ▼
Figma「SHOGUN — App Wireframes」 (デザイナーがクオリティを上げる)
        │  Figma MCP (get_design_context / get_screenshot / get_variable_defs)
        ▼
apps/desktop の React 実装へ反映 (Claude)
```

## 1. 生成物の中身

`figma-import/` には**画面 × ステート × テーマ**ごとに完全に自己完結した静的HTMLが入っている
(CSS・ロゴSVGはインライン済み、スクリプト・アニメーションは除去済み。単体で開いてもプラグインに
上げても同じ見た目になる)。

> ⚠️ **方針変更(2026-08-07)**: ノッチ以外のウィンドウUIは廃止(`docs/notch-first-ui-decision.md`)。
> 設定・ビューは `notch-full--*` が正。`fullui-*` / `settings--*` は参照用として残している。

| ファイル群 | フレーム数/テーマ | 内容 |
|---|---|---|
| `notch--{idle,welcome,answer,tracked,meeting}` | 5 | ノッチパネルの各ステート(idle=折りたたみピル、他は展開固定) |
| `notch-full--{set-general,set-privacy,set-conn,set-model,set-approvals,today,memory,status}` | 8 | **ノッチファースト**: 設定5タブ+Today/Memory/Statusビュー(すべてパネル内、620×460) |
| `fullui-pro--{today,health,sources,memory,activity,trace}` | 6 | Full UI (Pro) の各タブ |
| `fullui-standard--{…同6タブ}` | 6 | Full UI (Standard) の各タブ |
| `settings--{account,privacy,appearance,shortcuts,memory,connections,aisessions,model,nightly,approvals}` | 10 | 設定の各セクション |
| `onboarding--step{1..5}` | 5 | オンボーディングの各ステップ |
| `plans--{annual,monthly}` | 2 | プラン選択(年払い/月払い) |
| `standard-locks--default` | 1 | Standardプランのロック表示 |

`dark/` と `light/` で同じ35枚ずつ、計70枚。`index.html` をブラウザで開くと全フレームを一覧できる
(これは閲覧用で、インポート対象ではない)。

## 2. デザイナー向け: インポート手順

1. Figmaで「SHOGUN — App Wireframes」を開き、プラグイン **html.to.design** を起動する
2. **Upload file** タブで `figma-import/` 配下のHTMLを1枚選んでインポートする
   (URLインポートではなくファイルアップロード。フレームは1440×900で入る)
3. インポートされたフレーム名は `SHOGUN / notch / answer / dark` の形式になる。**この名前は変えない**
   (後述のMCP読み戻しで、コード側との対応付けに使う)
4. Figmaのページは画面単位で分ける: `Notch` / `Full UI` / `Settings` / `Onboarding` / `Plans`

### まずこれだけ入れれば始められる(優先セット)

70枚全部を入れる必要はない。製品の中心はノッチなので、初回は以下の12枚を推奨:

- `dark/notch--*` 5枚(プロダクトの顔。ダークガラスが正)
- `dark/fullui-pro--today` / `dark/fullui-pro--health` 2枚
- `light/settings--connections` / `light/settings--model` 2枚(Settingsはライトウィンドウが基準)
- `light/onboarding--step1` / `light/onboarding--step4` 2枚
- `light/plans--annual` 1枚

残りは磨く対象が広がったタイミングで追加すればよい。

## 3. 何が「正」で、何を変えてよいか

デザイナーが自由に上げてよいもの:

- 余白・階層・影・グラデーション・アイコンの造形、マイクロコピー以外のビジュアル全般
- ステート間の見た目の一貫性、light テーマの完成度(現状 dark 基準で light は機械変換に近い)

変える場合は必ず相談(実装側の制約・製品判断に直結):

- **トークン値**(色・文字サイズ・角丸): 正は `docs/wireframe-spec.md` §0。特に「パネル内の文字は
  14pxまで」の上限と、パネル寸法(W=560 / H_OPEN=300 等)はSLO・実装と連動している
  (※ノッチプロトタイプは意図的に答えテキストのみ16pxまで緩めている。経緯は
  `shogun-notch.html` 冒頭コメント)
- **UI文言**: 実装の `apps/desktop/src/strings.ts` にあるものはそれが正。文言は英語(v1方針)
- **ステートの増減・導線の変更**: 仕様(`docs/wireframe-spec.md` / `docs/notch-ui-prototype-spec.md`)の変更を伴う
- ブランド面は SHOGUN ブランドルールに従う(絵文字は⚔のみ、等)

## 4. 読み戻し: Figma → コード

デザイナーの更新をコードに反映するときは、このリポジトリのClaudeセッションで:

1. チャットのコネクタ設定で **Figma を有効化**する(ワークスペース接続は済んでいる)
2. 対象フレームのFigmaリンク(node link)を貼って依頼する。フレーム名が
   `SHOGUN / <画面> / <ステート> / <テーマ>` のままなら、どの実装・どの仕様セクションに
   対応するかを機械的に辿れる
3. Claudeが `get_design_context` / `get_screenshot` / `get_variable_defs` で読み取り、
   `apps/desktop` に実装する。トークンとの差分(仕様§0からの逸脱)は実装前に列挙して確認する

## 5. フレームの再生成

ワイヤーの正本(`docs/wireframes/*.html`)を変更したら再生成する:

```sh
npm i playwright-core   # 任意の場所に1回だけ
node scripts/export-wireframes-figma.mjs
```

- Chromium は Playwright 標準の探索パス、または `CHROMIUM_PATH` 環境変数で指定
- `figma-import/` 配下は**生成物なので手編集しない**。差分はコミットに含めてよい
  (デザイナーがリポジトリを見ずにダウンロードできるようにするため)
- 画面やステートを追加した場合は `scripts/export-wireframes-figma.mjs` の `PAGES` 定義に追記する

## 6. 既知の制限

- ガラスの `backdrop-filter`(すりガラス)はhtml.to.designでは近似になる。ノッチパネルの
  「背景が透ける」質感はFigma側で background blur を再設定するのがきれい
- ホバー・展開アニメーション(160ms 等)は静的フレームでは表現されない。モーション仕様は
  `docs/wireframe-spec.md` §0 と `docs/notch-ui-prototype-spec.md` を正とする
- `notch--idle` は折りたたみピルのみが写る(ホバー前の状態)。展開後は各ステートのフレームを参照
