# ドキュメント / デザインシステム PR レビュー報告（2026-07-30）

対象リポジトリ: /home/user/ShogunAI-（read-only レビュー。作業ツリーは変更していない）
main HEAD: `b9c1f23`。レビューは `git show` / `git diff origin/main...<branch>` によるブランチ内容比較で実施（shallow clone だったため unshallow 済み）。

---

## PR #79 「docs(mcp): MCP接続レイヤーの全体像・アーキテクチャ設計 (#59 リリース1)」
branch: `origin/docs/issue-59-mcp-design` → main / base: `efca5d0`（2026-07-29）/ 差分: `docs/mcp/00〜05` の新規6ファイル・+623行のみ（既存ファイル変更なし）

### (a) 判定: **request-changes**

文書としての完成度は高い（実コードとの対応付け・未実装/未検証の区別・後続Issue切り出しはいずれも良質）。しかし **Gmail アーキテクチャの記述全体が、CLAUDE.md に記録済みのオーナー決定（2026-07 Gmail全面Composio化）と正面から矛盾**しており、このままマージすると実装を旧設計へ誘導する。

### (b) 指摘事項

#### 【重大】Gmail 全面 Composio 決定（2026-07-27, commit `63aa4af`）との全面矛盾
CLAUDE.md「連携実装ルール」の決定: **Gmail は読み取り・ドラフト・送信のすべてを Composio 経由**（Google Cloud OAuth 不要、受信箱内容の第三者経由を明示的に受容、opt-in 3開示同意必須、読み取り egress にもトレーサビリティ必須）。

これに対し docs/mcp は一貫して**旧設計（読み取り/ドラフト＝公式MCP直結、Composio＝送信のみ）**を「現行アーキテクチャ」として記述している:

| ファイル | 矛盾箇所 |
|---|---|
| `docs/mcp/00-overview.md` | 用語定義「第2層…**現状は Gmail 送信のみ**」、設計の核1「公式に無い能力（**Gmail 送信**）だけ第三者で補う」 |
| `docs/mcp/01-architecture.md` | §1 図（Composio 箱=「現状 Gmail 送信のみ」）、§2-3 / §3-3（email 送信のみ第2層ルーティング）、§4 比較表（第2層の用途=「現状 Gmail 送信のみ」）、§5-1 プロンプト例（mail: Read-only を第1層接続前提で記述） |
| `docs/mcp/02-user-guide.md` | §1「Gmail…読み取り + 下書き（`gmail.readonly` / `gmail.compose`）」→ 決定後は Gmail の Google OAuth スコープ自体が存在しない。§2「Composio のオプトインはここ（オンボーディング）に入れない」→ 全面Composio化後は **Gmail 接続=Composio 同意**なので初回接続時に3開示同意が必須になり、この方針は成立しない。§5 Q&A「**読み取りは各サービスの公式サーバーと端末の直接通信のみ。唯一の例外は Gmail 送信**」→ 決定後は Gmail 読み取りも Composio を経由するため、**ユーザー向けプライバシー説明として虚偽になる**（最も危険な一文） |
| `docs/mcp/03-product-design.md` | §1「唯一の例外 = Gmail 送信の Composio」、§2「GCP consent 画面1つ・`docs/oauth-client-setup.md` の手順1本」（Gmail が GCP OAuth から外れるため根拠が変わる）、§3「Composio（Gmail 送信）を初回に出さない理由」 |
| `docs/mcp/04-dev-implementation.md` | §2-A チェックリスト「GCP OAuth クライアント作成 → Calendar/**Gmail**/Drive で end-to-end」（Gmail の第1層 OAuth 検証は決定により不要/無効な作業） |
| `docs/mcp/05-followup-issues.md` | Issue A（#80 として作成済み）が上記の旧設計検証をそのまま含む |

重要な事実関係: 決定コミット `63aa4af`（2026-07-27）は **PR #79 のベース `efca5d0` の祖先**であり、ブランチ自身の CLAUDE.md にも「全面 Composio」決定が既に書かれている。つまり「古いベースに書いたから」ではなく、**決定記録を反映せずにコード（`crates/shogun-mcp/src/scope.rs` / `shogun-integrations/src/endpoints.rs` / `send_bridge.rs` — これらも旧設計のまま）だけを言語化した**もの。ドキュメントの方法論（実コードの言語化）自体が、コードが未追従の決定を覆い隠す結果になっている。

#### 【重大】3開示 opt-in 同意・読み取り egress トレーサビリティの欠落
CLAUDE.md 必須条件「Composio の使用（**読み取りを含む**）に opt-in 同意（**3開示**）を必須」「**読み取り egress にもトレーサビリティを記録**」が docs/mcp のどこにも現れない（同意は送信文脈のみ、トレーサビリティは一般論のみ）。ユーザーガイド・オンボーディング設計・検証チェックリストのいずれにも反映が必要。

#### 【中】draft-stop の既定値が未記載
CLAUDE.md は「draft-stop（**既定ON**、同意後のみOFF可）」。00 の用語定義・02 §3/§5 は draft-stop を説明するが既定値と解除条件を書いていない。

#### 【小】Wave 1 = 3サービス（Drive 含む）と要件書 FR-INT-03 の不整合
docs/mcp は Wave 1 = Gmail/Calendar/Drive とするが、`docs/requirements-v1.0.md` FR-INT-03 と CLAUDE.md Phase 1 は「Wave 1 = Gmail + Google Calendar」。Drive 追加自体は `scope.rs` に 2026-07-23 のプロダクト判断としてコメント記録済みなので docs/mcp が正しいが、docs/mcp 自身が「設計の正本は requirements §6.9〜6.10」と宣言しつつ正本と食い違っている。要件書側の更新（または食い違いの明記）が必要。

#### 【小】ベース鮮度（efca5d0 vs b9c1f23）
main はベース以降 17 コミット進んでいるが、すべて PostHog analytics（#61/#91）で `docs/mcp/` に競合するドキュメントは main に存在しない。差分は新規ファイルのみなので機械的にはクリーンにマージ可能。**stale はコンフリクトの問題ではなく内容の問題**。

### (c) マージ前必須修正
1. Gmail 経路の記述を全ファイルで 2026-07 決定に合わせて書き直す（読み取り/ドラフト/送信=Composio、Gmail の第1層 OAuth・`gmail.readonly`/`gmail.compose` 記述の削除、公式MCP GA 時に第1層へ戻す余地の明記）。※もし「コードの現状を書いた」ことを優先するなら、少なくとも各 Gmail 該当節に「2026-07 決定により本節は移行予定。正は CLAUDE.md」の明示的コールアウトを置くこと。無印のままのマージは不可
2. `02-user-guide.md` §5 のプライバシー説明を訂正（「Gmail は読み取りも第三者(Composio)を経由する」ことを明示。虚偽説明の芽を残さない）
3. 3開示 opt-in 同意フローと読み取り egress トレーサビリティを 02（UI/同意）・03（オンボーディング方針の再設計: Gmail 接続=Composio 同意）・04（実装チェックリスト）に追加
4. draft-stop 既定ON・同意後のみOFF可を明記
5. FR-INT-03（Wave 1 構成）との食い違いの解消方針を一行明記（要件書更新の後続Issue化で可）
6. 作成済み Issue #80〜#82 の本文も同決定に合わせて更新（Issue A の「GCP OAuth で Gmail end-to-end」等）

---

## デザインシステム 4連 PR スタック（#14 → #15 → #17 → #18）

branch 構成（スタックの共通ベースは `a385259`＝2026-07-26 の「Free廃止・全員課金」決定記録コミット。main はその後 **137 コミット**進行）:
- **#14** `design-system/foundation-tokens`: `packages/tokens` 新設（tokens.json 正本 → CSS/TS 生成・validate・node:test）、desktop の `:root` トークンをパッケージ参照へ、turbo 配線
- **#15** `design-system/website-tokens`: web（Skyglass）トークンセット追加、`apps/website/globals.css` の生トークンを `@shogun-ai/tokens/web.css` 参照へ
- **#17** `design-system/shadow-blur-tokens`: shadow/blur トークン追加、desktop ガラスの shadow/blur を var() 参照へ
- **#18** `design-system/components-catalog`: `docs/design-system/components.md`（button/card/input/badge カタログ、ドキュメントのみ）

### (a) 判定
- **#14 foundation-tokens: request-changes**（設計は良いが、トークン値が現 main に対して古く、マージすると見た目が退行する）
- **#15 website-tokens: approve-with-nits**（#14 依存。website は base 以降 main 側変更ゼロで値も一致、クリーン）
- **#17 shadow-blur-tokens: request-changes**（#14 と同じ staleness。main 側に未トークン化の新 blur/shadow 変種も出現）
- **#18 components-catalog: approve-with-nits**（ドキュメントのみ。対象の `apps/website/src/components/ui/*.tsx` は base 以降 main 側変更なしで内容一致）
- スタック全体としては「close-as-stale ではないが、**現状のまま順次マージは不可**。rebase + 値再同期が必須」

### (b) 指摘事項

#### 【確認済】CLAUDE.md の Free プラン矛盾は「ブランチ上では」解消済み
- 4ブランチすべての CLAUDE.md はベース `a385259` から**無変更**で、「Freeプランなし。7日間フルトライアル → Standard / Pro」の正しい記述（main と同一）。スタックの全差分にも Free プラン/$0/降格の追加テキストは一切ない（grep で確認）。main CLAUDE.md の ⚠️ 警告が指す「Free ありの foundation-tokens CLAUDE.md」は**旧版のブランチ**の話で、現在のブランチ先端は決定コミットの上にリベース済み。
- ただし ⚠️ 注記の後段「**マージ時に…LP（apps/website の pricing）も併せて更新すること**」は未達のまま: **main の LP は今も Free/$0 プランを表示している**（`apps/website/src/i18n/dictionaries.ts` L158-159, L459-460, L729-730, L999-1000 に `name: 'Free', price: '$0'`）。スタックはこれに触れていない。マージ時に (1) CLAUDE.md の ⚠️ 注記を削除し、(2) LP pricing の Free 撤去を同時に行うか、マージ直後の必須フォローアップ Issue として紐付けること。CLAUDE.md の指示上、放置したままのスタックマージ完了は不可。

#### 【重大】desktop トークン値が現 main から乖離（マージすると視覚退行）
base 以降、main の `apps/desktop/src/styles.css` は **11 コミット・+935 行**変化。特に「real glass」コミット `3e30815` でガラス値が変更済み:
- main 現在値: `--glass: rgba(18, 21, 28, 0.62)` / `--glass-2: rgba(28, 32, 41, 0.70)` / `--line: 0.11` / `--line-strong: 0.18`（light 側も変更）
- スタックの `packages/tokens/src/tokens.json`: `--glass: rgba(21, 24, 31, 0.85)` / `--glass-2: rgba(31, 35, 43, 0.85)` / `--line: 0.09` / `--line-strong: 0.16`（旧値）

スタックは styles.css から `:root` ブロックを削除して生成 CSS に置換するため、このままマージ（コンフリクトを機械的に解消）すると**新ガラスが旧ガラスに巻き戻る**。「見た目不変リファクタ」の前提が崩れている。さらに main には未トークン化の新変種（`blur(34px) saturate(1.7)`、`0 18px 44px rgba(0,0,0,0.45)`＝Full UI ウィンドウ系）が出現しており、#17 のトークン網羅が現状と合わない。`apps/desktop/src/main.tsx` も main 側で meeting overlay 分岐が入り大きく変化（スタック側の変更は import 1 行なので再適用は容易だがコンフリクトする）。

#### 【良】トークン基盤の構造は健全
- `tokens.json` 単一正本 → `build.mjs` で `dist/tokens.css` / `tokens.web.css` / `tokens.ts` 生成、dist 非コミット、validate（themed の dark/light 両モード必須 + 色値正規表現）+ `node --test`、exports マップ（`./css` `./ts` `./web.css` `./tokens.json`）、turbo `dev.dependsOn: ["^build"]` で dist 生成順序を保証 — いずれも妥当
- desktop（`data-appearance`、dark 既定ブートストラップ）と web（`data-theme`、light 基準）の 2 テーマ共存を意図的に分離し README に明記している点も良い
- nit: web セットの非色値（`--orb-blend` 等）は validate が存在チェックのみ — README に明記済みで許容。`:root` ベースに dark 値を重複展開するのは既存の「JS 未起動時 dark フォールバック」踏襲で意図的

#### 【良】ブランドルール準拠
スタック全差分に競合名・「AI-powered / revolutionary / second brain」・絵文字（⚔含む）は無し。Tailwind/shadcn/Radix/CVA 等の技術スタック名は内部開発ドキュメント（`docs/design-system/`・`docs/superpowers/`）にのみ登場し、UI 文言・外部向けコピーには一切変更なし → ブランドルール（対象は UI 文言・外部コピー）違反なし。

#### 【小】website 側はクリーン
`apps/website` は base 以降 main 側変更ゼロ（waitlist/Supabase/gamification 群は 2026-07-19 までにマージ済みで**ベースに包含**）。`web.themed` の値は main の `globals.css` 現行値と一致。#15 の置換は見た目不変が成立している。#18 のカタログも現行 `ui/*.tsx` と整合。

### (c) マージ前必須修正
1. `tokens.json` の desktop 値を main 現行の `styles.css`（`3e30815` 以降）から再抽出（glass/glass-2/line/line-strong の dark/light、および #17 の shadow/blur に main の新変種を追加するか対象外と明記）
2. スタックを現 main にリベースし、`styles.css`（+935行）・`main.tsx`（meeting overlay）とのコンフリクトを手動解消。解消後「生成 CSS 適用前後で computed style 不変」を再検証（少なくとも dark/light 両テーマの目視 + 値 diff）
3. CLAUDE.md の ⚠️「要解消の分岐」注記の削除と、LP pricing（`dictionaries.ts` の Free/$0 ×4言語）の Free 撤去を、スタックマージと同時 or 直後の必須フォローアップとして実施
4. `pnpm-lock.yaml` / turbo はリベース後に再生成・再確認

### (d) 推奨マージ戦略: **rebase-and-fold（2 PR に畳む）**
4 PR を順次 main に流す（14→15→17→18 で都度リターゲット）ことも可能だが、#14 と #17 は値の再同期で内容自体を修正する必要があり、3 本とも `packages/tokens` を触る小さな積み木なので、個別に通す価値が薄い。推奨:
1. 現 main から統合ブランチを切り、#14+#15+#17 をリベース＆fold → **PR-A「feat(tokens): デザイントークン基盤（desktop + web + shadow/blur）」**として squash マージ（上記必須修正 1〜2 を含める）
2. #18 は docs のみで独立性が高いので、PR-A マージ後に**PR-B「docs(design-system): Components カタログ」**として単独 squash マージ（カタログが実装のミラーである以上、トークン変更後に最終確認してから）
3. CLAUDE.md ⚠️ 注記削除 + LP pricing Free 撤去は PR-A に同梱するか、PR-A と同日の小 PR-C で処理
4. 旧 4 ブランチは fold 後クローズ（close-as-superseded）

---

## 総括

| PR | 判定 | 一言 |
|---|---|---|
| #79 docs/mcp | **request-changes** | 文書品質は高いが Gmail 記述全体が 2026-07 全面Composio決定と矛盾。02 §5 はユーザー向け虚偽説明になり得る。3開示同意・読み取りegressトレーサビリティ欠落 |
| #14 foundation-tokens | **request-changes** | 構造は良い。トークン値が main の real-glass 更新より古く、マージで視覚退行 |
| #15 website-tokens | approve-with-nits | website は無風でクリーン。#14 依存 |
| #17 shadow-blur-tokens | **request-changes** | #14 と同じ staleness + main の新 blur/shadow 変種未対応 |
| #18 components-catalog | approve-with-nits | docs のみ、現行実装と整合。#14/15 の後で |

- Free プラン矛盾: **ブランチ CLAUDE.md では解消済み**（全ブランチが決定コミット上にある）。ただし main の LP に Free/$0 が残存しており、CLAUDE.md の「マージ時に LP も更新」条項が未消化。
- スタックは close-as-stale にはしない（website/tokens 基盤部分は生きている）。rebase + 値再同期 + 2 PR fold を推奨。
