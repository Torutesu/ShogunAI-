# 閉じたピルのドラッグ移動 — main への移植設計（Issue #21 / PR #66 再適用）

日付: 2026-07-30
状態: 実装済み（本ドキュメントと同一コミット）

## 1. 背景

Issue #21「閉じた状態でピルをドラッグして移動できるようにする」は PR #66
（head `6b705dc`、実体は `59ec99c`）で実装されたが、base ブランチが
`design-system/documentation-node` だったため **main に一度も入っていない**
（`beginPillDrag` は main 全ツリーに存在しない）。Issue は completed でクローズ済み＝
Done 管理と実体の乖離（監査レポート項目5）。

一方、その後 main には PR #71「Castle Position」（Issue #20）がマージされた。
両者はどちらも「パネルの位置」を支配する機能であり、単純 cherry-pick では
**Castle の redock パスがドラッグ位置を毎回上書きする**ため、優先規則を決めて
統合する必要がある。

## 2. PR #66 が行ったこと（原実装）

フロントエンドのみの変更（`apps/desktop/src/App.tsx` + `styles.css`）:

- `beginPillDrag` を追加し、閉じたハンドル（ピル）の `onMouseDown` に付与。
  展開時ヘッダーで実績のあるネイティブドラッグ `start_panel_drag`
  （Rust 側 `performWindowDragWithEvent:`）を流用。
  **実ドラッグ＝ウィンドウ移動＋クリック飲み込み / 静止クリック＝onClick（展開）**を
  閾値判定なしで両立（AppKit が区別してくれる）。
- 会議中ピル（`MeetingPill`）のラッパーにも `beginDrag` を付与。
- `.handle` のカーソルを `grab` / `:active` で `grabbing` に。

限界（当時の base にはなかった問題）:

- **ドラッグ位置は永続化されない**（再起動で消える）。
- main の Castle Position 導入後は、summon・スペース切替の reassert・
  展開/折りたたみのリサイズ・Castle 変更のたびに `castle_origin` へ強制的に
  戻される＝ドラッグしても数秒で元の位置に戻る。

## 3. main 側で変わったこと（PR #71 Castle Position）

- `crates/shogun-core/src/notch/geometry.rs` — `CastlePosition`（6配置）と純関数
  `castle_origin(vis, w, h, pos)`（画面外クランプ＋縮退 visible frame ガード、単体テスト付き）。
- `apps/desktop/src-tauri/src/lib.rs` — lock-free `AtomicU8 CASTLE`、`castle.json` 永続化
  （app_data_dir、非シークレット）、`get/set_castle_position` コマンド（UI/API 対称）。
- **castle_origin を呼ぶ配置パスは4つ**（すべてメインスレッド）:
  1. `reposition_to_cursor_screen` — summon（⌥J）/ reassert / パネル生成時
  2. `pin_top_centre` — 起動時のメニューバーディスプレイへのドック
  3. `set_panel_size` の castle アンカー — ピル⇔パネルのビュー切替リサイズ
  4. `redock_to_castle` — `set_castle_position` 直後の即時移動

## 4. 統合規則（本移植で採用する優先規則）

**「ユーザーのドラッグ＝明示的な位置オーバーライド。Castle の選択＝オーバーライドの解除と帰城」**

1. ユーザーがパネルをドラッグ（閉じたピル・展開ヘッダーのどちらでも）すると、
   ドロップ位置が **drag override** として記録される。
2. override が存在する間、上記4つの配置パスはすべて `castle_origin` ではなく
   override 位置に置く（＝summon・reassert・リサイズがドラッグ位置を尊重する）。
3. 設定で Castle Position を選び直す（`set_castle_position`、UI/API 両方）と
   **override はクリアされ**、パネルは選んだ Castle へ redock する。
   これが「ホームに戻す」唯一の明示操作。
4. override は `castle.json` に position と並べて永続化し、**再起動後も生きる**。
5. 座標は絶対座標ではなく **対象スクリーンの visible frame からの相対オフセット**
   （左端からの dx、上端からの dy、アンカーはパネル左上角）で保存する。
   - ディスプレイ構成が変わっても / 別ディスプレイへ summon しても、
     純関数 `drag_origin` が **visible frame 内へ必ずクランプ**するので
     パネルが画面外に消えることはない（castle_origin と同じ縮退ガード）。
   - 左上角アンカーなので、ピルの位置でパネルを展開すると左上を固定して
     右下方向へ育つ（手動グリップリサイズと同じ読まれ方）。

### 採らなかった案

- 「ドラッグ位置を最も近い CastlePosition にスナップ」— 6配置しかなく
  ユーザーの意図（好きな場所に置く）を破壊する。
- 「ドラッグを castle.json の position とは別ファイルに保存」— パネルの安置場所は
  1ファイル1責務で `castle.json` が既にその責務。フィールド追加は
  serde `default` + `skip_serializing_if` で前方/後方互換（旧ファイルは読める、
  override なしなら旧形式と同一の JSON）。

## 5. 実装

### 5.1 shogun-core（純関数・Linux で単体テスト可能）

`notch/geometry.rs` に追加:

- `DragOffset { dx, dy }` — visible frame 左上からパネル左上角へのオフセット。
- `drag_origin(vis, w, h, off) -> Point` — 配置先原点（bottom-left）。クランプ付き。
- `drag_offset(vis, origin, h) -> DragOffset` — 実フレームからの逆算（記録側）。
- 単体テスト: 往復（roundtrip）、小さい画面でのクランプ、縮退 frame ガード。

### 5.2 apps/desktop/src-tauri（macOS シェル）

- `static DRAG_OVERRIDE: Mutex<Option<DragOffset>>` — 実行時の override。
- `resting_origin(vis, w, h)` — override があれば `drag_origin`、なければ
  `castle_origin`。4つの配置パスの `castle_origin` 呼び出しをこれに置換。
- **ドラッグ終了位置の捕捉**: `performWindowDragWithEvent:` は完全ネイティブで
  webview にもコマンドにも終了イベントが来ないため、パネルの
  `NSWindowDidMoveNotification`（object=パネル限定）を購読する。
  - 自前の配置（dock/redock/resize）も同通知を発火するので、
    `PROGRAMMATIC_MOVE: AtomicBool` で全プログラム的 `setFrame*` を括る。
    通知は同一スレッド（メインスレッド）で同期配送されるため、この括りは正確。
  - フラグが立っていない move ＝ユーザードラッグ → `drag_offset` を計算して
    `DRAG_OVERRIDE` を即時更新。
- **永続化**: `castle.json` の `CastleFile` に `drag: Option<{dx, dy}>` を追加。
  didMove はドラッグ中に連続発火するため、書き込みは **trailing debounce**
  （保存ペンディングフラグ＋400ms 後に最新値を1回書く。タイマースレッドは
  ドラッグ中しか生まれない）。メモリ上の override は毎回更新なので実行時挙動は常に正確。
- `set_castle_position` は override をクリアしてから保存・redock。

### 5.3 apps/desktop（フロントエンド、PR #66 の適応移植）

- `beginPillDrag` を追加、閉じたハンドルの `onMouseDown` に付与（原実装どおり）。
- **main 側で増えたホバー展開（dwell）との干渉対策**: mousedown の瞬間に
  `cancelHoverOpen()` を呼ぶ。これがないと HOVER_DWELL_MS 経過でドラッグ中に
  パネルが展開し得る（「ドラッグは展開を発火しない」要件）。
- `.handle` の cursor を `grab`/`grabbing` に（原実装どおり）。
- PR #66 の `MeetingPill` ラッパーへの `beginDrag` 付与は、**main に MeetingPill の
  描画が存在しない**ため対象外（会議ピル UI が main へ移植される際に同じ1行を足すこと）。
- 新規 UI 文言なし（i18n 影響なし）。

## 6. SLO 考察

- **Notch展開 100ms**: 配置パスの追加コストは「Mutex（無競合）1 lock ＋分岐」のみで
  ナノ秒オーダー。展開経路のホットパス（webview アニメーション・`set_panel_size`）の
  構造は不変。実測 p50/p95 は macOS 実機でのみ取得可能（本移植は Linux 環境で実装。
  実機計測は on-device runbook に従い追試すること）。
- **ドラッグが展開を発火しない**:
  - JS 側 dwell タイマーは mousedown で必ずキャンセル。
  - ネイティブドラッグは click を飲み込むため onClick（展開）は発火しない。
- **ポーリングなし**: ドラッグ捕捉は NSNotification（イベント駆動）。
  debounce スレッドはドラッグ発生時のみ 1 本・400ms で消える。
- **アイドル CPU への追加コスト 0**: 常駐タイマー・ループ・監視スレッドは増えない。

## 7. 残リスク（macOS 実機での確認が必要な項目）

1. **Rust 側ホバートラッカーとの干渉**: shogun-core の hover バンドは
   ノッチ位置基準で固定。ピルをノッチ付近でドラッグ中（あるいはドラッグで
   ノッチ帯を横切った時）に `state: hover` が emit されて展開する可能性が残る
   （JS 側 dwell とは別経路）。また override でピルがノッチ以外に居る場合、
   ノッチ帯ホバーで「ピルの無い場所」から展開する。挙動確認と、必要なら
   ドラッグ中サプレッション／バンド追従を follow-up とする。
2. `NSWindowDidMoveNotification` の配送スレッド・頻度は実機検証（設計は
   メインスレッド同期配送を前提。異なる場合は PROGRAMMATIC_MOVE の括りが漏れる）。
3. `SHOGUN_NO_NOTCH=1` のフォールバック（tao プレーンウィンドウ）では didMove
   監視を張らないため、ドラッグ位置は記憶されない（デバッグ経路、許容）。
4. 展開状態で画面下端付近に override がある場合、クランプにより上方向へ
   押し戻される（仕様どおりだが体感確認要）。
