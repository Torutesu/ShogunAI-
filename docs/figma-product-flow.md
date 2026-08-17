# Figma — Product Flow（仮想Macの中で動くSHOGUN）

ダウンロードから会議ノートまで、**macOS デスクトップの中で SHOGUN がどう動くか**を1本の
ストーリーボードとして Figma に落としたもの。デザイナーのブラッシュアップ元であり、
実装との対応表でもある。

- **Figma ファイル**: [Design](https://www.figma.com/design/OwsRsTjk2jua7tuTyMgc90/Design) (`OwsRsTjk2jua7tuTyMgc90`)
- **ページ**: `Product Flow — in a Mac`
- **作成日**: 2026-08-16
- **上位文書**: `CLAUDE.md` / `docs/requirements-v1.0.md` / `docs/wireframe-spec.md`
- 既存の `Notch UI` ページ（ノッチ単体のステート集）とは別物。本ページは**フロー**が主語

---

## 1. 何が正で、何がモックか

| | 出どころ | 変えてよいか |
|---|---|---|
| **UI 文言** | `apps/desktop/src/strings.ts` から逐語 | ❌ 変えるなら `strings.ts` を先に変える |
| **トークン**（色・文字・角丸・パネル寸法） | `apps/desktop/src/styles.css` の `:root` | ❌ 同上。Foundations セクションが写し |
| **ロゴ形状** | `apps/desktop/src/Logo.tsx` の `FACETS`（957×614）を SVG で再現 | ❌ 3箇所（app / desktop / website）同時にしか変えない |
| **パネル寸法** | `App.tsx` の `W=560` / `H_OPEN=360` / `H_SETTINGS=460` / `H_DEAD=32` / `H_CHIN_ROW=44` | ❌ SLO・実装と連動 |
| **背景のアプリ**（Mail / ビデオ通話 / Finder / System Settings） | モック。SHOGUN が読んでいる対象を具体化するための舞台装置 | ✅ 自由 |
| **本文のダミーデータ**（Aiko / 12k / 14日 のシナリオ） | モック。全レーンで同一シナリオを通している | ✅ 自由（ただし全レーンで揃える） |
| **キャプション**（各フレーム下の白文字） | 設計意図の注記。実装には存在しない | ✅ 自由 |

**意図的な逸脱が1つある**: オンボーディングウィンドウは実装では 720×640 固定でボディがスクロール
するが、本ページではカード全体が読めるようウィンドウ高さをコンテンツに合わせている
（`onboarding window (720×750)` 等、名前に実寸を残した）。

---

## 2. セクション構成（8レーン・40フレーム）

各レーンは左から右に読む。フレーム間の `→` が遷移。すべてのフレームは
`Mac / Desktop 14″` コンポーネント（1512×982、ノッチ 200×32、メニューバー 38）のインスタンスの上に描かれている。

### 0 · Foundations — tokens & components
`Brand / Kabuto` / `Mac / Desktop 14″` / `Mac / Desktop 14″ Light` の3コンポーネントと、トークン一覧ボード。
ローカル variable collection `SHOGUN Tokens`（色17 / 数値17）も同じ値で登録済み。

**実素材（2026-08-17 追加）**: 壁紙は本物の **macOS Sequoia** 公式壁紙（Dark / Light、6K原本を
image fill で取り込み）。メニューバーの Appleロゴ、Dock の Slack ピンホイール・Notion キューブ・
Linear マークは実ロゴ SVG/PNG。Finder / Mail / Calendar / Terminal / ゴミ箱は実アイコンに忠実な
ベクター再現。Dock・壁紙は shell コンポーネント側にあるため、**全フレームに自動反映**される。

### 1 · Download & install（4）

| フレーム | 対応 |
|---|---|
| 1.1 Site — Download for macOS | `apps/website` Hero（`dictionaries.ts` の `hero.lineA/lineB/sub/note`） |
| 1.2 Download — notarised .dmg | `docs/onboarding-design.md` §2 ビート1 |
| 1.3 DMG — app and Applications, nothing else | 同 ビート2（README を同梱しない判断） |
| 1.4 Gatekeeper — one confirm | 同 ビート3（**自前ダイアログを重ねない**） |

### 2 · Onboarding（8）
実装 `apps/desktop/src/onboarding/Onboarding.tsx`、文言 `strings.ts` の `ob*` + `onboarding` ブロック。

| フレーム | step id | 文言キー |
|---|---|---|
| 2.1 SHOGUN lives in the notch | `welcome` | `obWelcome*` |
| 2.2 what it reads, what it never keeps | `reads` | `obReads*` / `obNever*` / `obExclusion` |
| 2.3 the one permission (asking) | `permission` | `obPerm*` / `onboarding.steps` |
| 2.4 macOS System Settings — Accessibility | （アプリ外） | `open_accessibility_settings` の着地点 |
| 2.5 granted, proved on the spot | `permission` | `obPermGranted` / `obPermProof` |
| 2.6 seven days of everything | `plan` | `obPlan*` / `obKey*` |
| 2.7 connect, drafts-only by default | `connect` | `obConnect*` / `obDraftStop*` |
| 2.8 you're set | `ready` | `obReady*` / `analyticsToggle*` |

### 3 · The notch, day to day（8）
実装 `apps/desktop/src/App.tsx`。

| フレーム | 実装の該当 |
|---|---|
| 3.1 idle chin, welded to the bezel | `.chin`（`--chin` = 不透明黒、`--r-idle-bot: 8px`） |
| 3.2 hover peek | ホバー滞留のプレビュー。**自動展開しない** |
| 3.3 expanded, empty thread | `welcomeTitle` / `welcomeSub` / `noKey` / `.composer` |
| 3.4 context actions, L1 / L2 / L3 | `.acts` / `.acts__btn`（`actionsAria`） |
| 3.5 answer, with the evidence under it | `.msg--me` / `.msg--shogun` / `sources` |
| 3.6 what it is tracking | `.state__row`（`stateList` / `resolveHint` / `stateEmpty`） |
| 3.7 press / to search memory | `searchPlaceholder` / `searchHint` |
| 3.8 L3 never runs without you | `.acts__confirm`（`actionConfirmQ` / `approvalsVia`） |

### 4 · Daily summaries（3）
設計 `docs/daily-summaries-design.md`、実装 `apps/desktop/src/daily.tsx`。

| フレーム | 対応 |
|---|---|
| 4.1 it waits for you to arrive | §3.1 ハンドル通知（**赤丸なし**・兜マーク＋ブランドブルーのグロー） |
| 4.2 Morning brief | §3.2（`goodMorning` / charm line / `dsToday` / `dsCommitments` / `dsOpenLoops` / ソースチップ） |
| 4.3 Evening wrap | §3.3（`goodEvening` / `dsDone` / `dsStillOpen` / `dsTomorrowFirst` / `dsLooseEnds`） |

### 5 · Meeting notes（8）
設計 `docs/meeting-notes-ui-design.md`（§3.1 の 2026-07-26 訂正＝フローティングパネルが正）、
実装 `apps/desktop/src/MeetingOverlay.tsx`。

| フレーム | 実装の該当 |
|---|---|
| 5.1 Meeting detected | `.ov-offer` / `.ov__offer-*`（`meetingDetected` / `meetingTakeNotes` / `meetingNotNow`） |
| 5.2 taking notes | `.ov__bar`（黒カプセル 52px / 36px ボタン / `--danger` stop） |
| 5.3 AI Canvas — Live Summary | `.ov__canvas`（`meetingCanvasLiveSummary` / `meetingCanvasListening`） |
| 5.4 AI Canvas — Timeline | `.ov__canvas-timeline` / `.ov__canvas-step` |
| 5.5 Captions — one-way translation | `.ov__live`（`meetingModeOneWay` / `meetingLangArrow`） |
| 5.6 display settings | `.ov__disp`（`meetingDisplayText/Weight/Split/Original`） |
| 5.7 Chat | `.ov__chat`（`meetingChatPlaceholder` / `meetingChatNew`） |
| 5.8 Recap | `meetingMinutes*` / `meetingRecapYourNotes` / `meetingDisclosureRecap` / `[Track]` |

Deepgram 開示（`meetingDisclosure`）は 5.1・5.5・5.8・7.7 の4面に載せている。CLAUDE.md 不変条件2の
明示的例外は、例外が効く画面すべてで開示する。

### 6 · Visual recall（2）
実装 `apps/desktop/src/visual-recall.tsx`、文言 `visualRecall*`。
`screen_frames` の 72 時間・暗号化メモリ DB・自動削除という例外条件を、
タイムライン（6.1）と設定のディスクロージャ（6.2）の両方に出す。

### 7 · Settings & governance（7）

| フレーム | 対応 |
|---|---|
| 7.1 Hub | `App.tsx` の `HUB_TABS`（today / sources / memory / activity / system） |
| 7.2 two keys, two lanes | 不変条件5。`model` / `key*` / `sub*`（Issue #110 委譲）/ `selectKk*` |
| 7.3 Privacy & Security | `privacyTitle` / `policy*` / `delete*` / `analyticsToggle*` |
| 7.4 Connections | `connections*` / `aiSessions*` / `dream*` / third-party バッジ |
| 7.5 Approvals | `approvals*`（L3キュー）/ `composio*`（draft-stop） |
| 7.6 Plan & billing | `plan*`（Standard $49 / Pro $99 年額・7日フルトライアル） |
| 7.7 summaries · meetings · sounds | `ds*` / `meeting*` / `sound*` |

### 8 · Light appearance（5）
`styles.css` の `:root[data-appearance="light"]` トークン（glass `#FCFDFF.94` / shell `#F7F8FB` /
ink `#1B1D22` / accent `#2F6FED` / live `#1F9E3C` / warn `#A4560A`）で、代表5面をライトで再現:
回答＋出典 / Morning brief / Settings（Connections）/ オンボーディング / idle chin＋peek。
**idle chin は両アピアランスで不透明黒**（ベゼルとして読める必要があり、ベゼルにテーマは無い）——
8.5 はその設計事実を1枚にしたもの。

### 9 · Concept — Liquid Glass（3）
**仕様ではなく探索。** パネルを「レンズ」として扱う未来コンセプト:
Figma の実 background blur ＋ スペキュラエッジ（inner shadow ハイライト）＋ 不透明シートなし。
9.1 パネル全体 / 9.2 朝のブリーフを奥行きつきガラス積層で / 9.3 会議 HUD（黒カプセル廃止、
キャプションは屈折ストリップ）。採用するなら SLO（ブラー負荷）と可読性の実測が先。

### 10 · Tone — Ambient（3）
オーナー提供のリファレンス（緑の葉背景×ティントガラス）からのトンマナスタディ。
**UI構造・文言・ガバナンス表示は既存のまま**、トーンだけを差し替える:
背景（デフォーカスした実写の新緑）が透けるティントガラス、丸型コントロール、タイポに一段の余白。
10.1 welcome / 10.2 回答＋L1-L3アクション / 10.3 Morning brief。
キャプションに明記したとおり「トーンがガバナンス表示を運べないならそれはトーンではなく再設計」——
L1緑・L3アンバー・出典チップはこの天気でも変わらない。シェルは `Mac / Desktop 14″ Ambient`。

---

## 3. このページが主張していること

フレームを個別に見るのではなく、レーンとして読んだときに出るはずの3点。

1. **権限を要求する前に理由を渡している**（レーン2の順序）。2.4 が示すとおり、
   付与はユーザーが macOS 側で行い、SHOGUN は黙ってポーリングして緑になる
2. **外に出るものは必ず止まる**（3.4 → 3.8 → 7.5）。L3 は色とタグで一貫し、
   Composio 経由は同意の瞬間にバッジで出る
3. **記録ではなく状態**（3.6 / 4.2 / 5.8）。行には provenance と confidence があり、
   低確度は "possibly" のまま提案に昇格しない。5.8 の `[Track]` を押すまで state には入らない

---

## 4. 更新の手順

Figma を直接編集してよい。ただし §1 の「❌」列を変える場合は、
**先にコードを変えて、そのあとで Figma を合わせる**。逆順にすると本ファイルが嘘になる。

MCP から読み戻すときは `get_design_context` にノード指定の URL を渡す
（フレーム名は `1.1 …` 形式で安定させてある）。
