# SHOGUN Phase 0 — ノッチUIスパイク 完全仕様書

- 文書ID: `docs/notch-ui-prototype-spec.md`
- ステータス: 確定（Phase 0 実施基準）
- 上位文書: `CLAUDE.md`（プロダクト憲法。SLO表・技術スタック・絶対不変条件はそちらが正）
- 対象環境: macOS 14+ / Apple Silicon (arm64) のみ
- 実装言語: Tauri v2 / Rust / React + TypeScript、ネイティブ層は objc2 クレート
- スパイクの性格: **使い捨て**。唯一の目的は「4つの問い」に数値で答えること。ただし §4 の計測ハーネス（`crates/spike-harness` の計測コア）のみ本実装に持ち越す
- タイムボックス: 実装+計測+判定で **15営業日** を上限とする（超過見込みが出た時点で §6 の判定手順を前倒しで開始する）

> 表記規約: 本書の「pt」はすべて macOS のポイント（論理座標）。時間はすべて ms。座標系は明示がない限り NSScreen 系（**左下原点**）。CGEvent 系 API は左上原点なので実装時に変換すること（§3.4.7）。

---

## 1. 目的と4つの問い

Phase 0 は「ノッチ常駐UIというプロダクトの背骨が、技術的に成立するか」を判定するスパイクである。以下の4つの問いを**検証可能な仮説**として再定義し、それぞれに合格ライン（§6）を設ける。1つでも最終不合格なら Phase 1 のノッチUI本実装には進まず、§7 の転換パス（メニューバー常駐+コマンドパレット）に移行する。

### Q1. 常駐安定性

**仮説**: `.nonactivatingPanel + .canJoinAllSpaces + .fullScreenAuxiliary` を持つ borderless NSPanel は、以下のすべてをまたいで「消えない・壊れない・位置がずれない」状態を維持できる。

- 全 Space の切替（Mission Control / 4本指スワイプ / ⌃→）
- 他アプリのフルスクリーンモード上（ネイティブフルスクリーン: Keynote 再生、Safari 動画全画面、ゲーム系は対象外）
- ディスプレイ構成変化: 外部ディスプレイの接続 / 切断 / 解像度・スケーリング変更 / ノッチ有無混在（内蔵=ノッチあり + 外部=ノッチなし）/ クラムシェルモード出入り
- スリープ→復帰（短時間スリープ×複数回 + 一晩スリープ×1回）
- 24時間連続稼働（ソーク試験）

**検証方法**: 24時間ソーク（§5 S-11）+ シナリオ試験（§5 S-1〜S-5, S-10）。ヘルスチェック（§3.9）で消失・位置ずれを自動検出し JSONL に記録する。

**合格の骨子**（詳細は §6.1）: ユーザー可視のパネル消失（2秒超）0回、プロセスクラッシュ0回。

### Q2. 展開レイテンシ

**仮説**: ホバー意図の確定（HoverIntent の dwell タイマー満了 = 展開コミット時点、§3.3 の T2 遷移）から、Expanded UI の描画完了までを **p95 ≤ 100ms** で達成できる（CLAUDE.md SLO「Notch展開（Idle→Expanded）100ms」に対応）。

**計測点の定義**（§4.2.1 に正式定義）:

- `t0` = dwell タイマー満了を Rust 側が検知した時刻（展開コミット）
- `t1` = webview 内で Expanded レイアウトの描画が画面に反映された時刻（`requestAnimationFrame` 2回目コールバック時刻。§4.2.1）
- `latency_expand = t1 - t0` — **これが SLO 対象**
- 参考値として `total_perceived = t1 - (判定領域進入時刻)` も必ず併記する（dwell 100ms を含むため目標 p95 ≤ 250ms。SLO ではなく体感の監視値）

dwell（意図確認時間）を SLO の分子に含めない理由: dwell は誤発火対策のプロダクト判断であり描画性能ではない。dwell を 0 にすれば Q2 は楽になるが Q4 が壊れる。2つの問いを独立に測るため計測点を分離する。

**検証方法**: 自動展開試験 200 回（§5 S-12）+ 手動操作中の常時記録。

### Q3. context cache（フォーカス切替 300ms + アイドルCPU 5%）

**仮説A（レイテンシ）**: フォーカス切替の検知（`NSWorkspace.didActivateApplicationNotification` 等の通知受信時刻）→ Accessibility API によるアクティブウィンドウのタイトル+可視テキスト取得 → インメモリキャッシュ書き込み完了、までを **p95 ≤ 300ms** で達成できる（CLAUDE.md SLO「context cache更新 300ms」）。

**仮説B（CPU）**: 上記の常時監視（NSEvent モニタ + NSWorkspace 通知 + AXObserver + キャッシュ更新）を動かし続けた状態で、SHOGUN 自身のアイドル時 CPU 使用率が **1分平均 ≤ 5%**（CLAUDE.md SLO。Activity Monitor 互換の「1コア=100%」換算、§4.3）に収まる。

**検証方法**: アプリ切替 100 回の自動試験（§5 S-13）+ 24時間ソーク中の CPU 常時サンプリング。

**付随して実証すること**: CLAUDE.md の「context cache は『押してから収集』禁止。常時プリアセンブル」を Phase 0 の時点で実証する。具体的には、Expanded 表示中に AX 呼び出しが**1回も発生しない**ことをハーネスがカウンタでアサートし（§3.8.5）、表示されるコンテキストは常にキャッシュ済みのもの（cache age を記録）であることを示す。

### Q4. ホバー誤発火

**仮説**: 通常作業（メニューバー操作、画面上端へのマウス移動、ウィンドウの上端ドラッグ、Spotlight 起動等）において、意図しない Expanded 展開を実用上ゼロに抑えられる。

**誤発火の定義**（自動判定。§4.2.4）: Expanded に遷移したが、収束までの間にパネル内でのクリック・キー入力・スクロールが 0 件、かつ Expanded 滞在時間が **1500ms 未満**で自動収束した展開。加えてグローバルホットキー ⌃⌥⌘F による手動マーク（「今のは誤発火」）も記録する。自動判定と手動マークの和集合を誤発火とする。

**閾値**（詳細は §6.4）:

- 台本シナリオ（§5 S-6〜S-8、計 90 試行）: 誤発火 **0 回**
- フリーワーク 8 時間（実作業）: 誤発火 **≤ 5 回**、かつ誤発火率（誤発火回数 ÷ 上端バンド進入回数）**≤ 2%**
  - 「上端バンド進入」= マウスが画面上端から 40pt 以内の帯に進入したイベント（§4.2.4 でカウント）

---

## 2. スパイクのスコープ

### 2.1 作るもの

| # | 成果物 | 本物/ダミー |
|---|---|---|
| 1 | NSPanel 化された Tauri WebviewWindow（実ノッチモード + 擬似ノッチモードの両方） | 本物 |
| 2 | ノッチ検出とパネル配置（`NSScreen.safeAreaInsets` / `auxiliaryTopLeftArea` / `auxiliaryTopRightArea`） | 本物 |
| 3 | 状態機械 Idle → HoverIntent → Expanded → Collapsing → Idle（§3.3） | 本物 |
| 4 | ホバー判定（NSEvent グローバルモニタ、イベント駆動。§3.4） | 本物 |
| 5 | context cache スパイク（NSWorkspace 通知 + AXUIElement 取得 + インメモリキャッシュ。§3.8） | **本物必須**（AX 取得・イベント監視をダミーにしたら Q3 が無意味になる） |
| 6 | Expanded パネルの中身 | ダミー（静的なアクションボタン3個 + コンテキストプレビュー領域。ただしプレビューに流し込むデータは #5 の本物キャッシュ） |
| 7 | 計測ハーネス `crates/spike-harness`（§4） | 本物。**本実装へ持ち越す唯一の資産** |
| 8 | 24時間ソークの自動記録 + Markdown レポート生成 | 本物 |
| 9 | 検証シナリオの自動化スクリプト（マウスイベント注入、アプリ切替注入。§5） | 本物 |

### 2.2 作らないもの

- DB / SQLite / 永続化（キャッシュはインメモリのみ。JSONL ログはファイルだが「計測記録」であり製品データではない）
- Context Fusion / エージェント / LLM 呼び出し / MCP / ネットワーク通信一切
- 設定画面・オンボーディング・Accessibility 権限の丁寧な誘導 UI（`AXIsProcessTrustedWithOptions` のシステムプロンプトを出すだけでよい）
- Full UI ウィンドウ、メニューバー常駐アイコン（No-Go 転換時に作る）
- 配布・署名・notarization・updater（ローカルの `pnpm tauri dev` / `--release` ビルドで検証。ただし **SLO 計測は必ず release ビルド**で行う。debug ビルドの数値は無効）
- アニメーションの磨き込み（§3.3 の数値を満たす最低限でよい。イージング調整に時間を使わない）
- Intel (x86_64) 対応、macOS 13 以下対応

### 2.3 コード配置

既存 pnpm ワークスペースのルートに Cargo workspace を追加する。

```
/ (リポジトリルート)
├── Cargo.toml            # [workspace] members = ["apps/desktop/src-tauri", "crates/spike-harness"]
├── apps/desktop/         # 既存プレースホルダを Tauri v2 アプリに置き換え
│   ├── package.json      # @shogun-ai/desktop（pnpm ワークスペース member のまま）
│   ├── src/              # React + TS（Idle/Expanded の webview UI）
│   └── src-tauri/        # Rust: パネル生成、状態機械、ホバー監視、AXキャッシュ
└── crates/
    └── spike-harness/    # 計測コア（§4）。本実装で shogun-core 配下へ移植する前提の独立クレート
```

- スパイク終了後、`apps/desktop/src*` は捨てる（または `legacy/` へ移す）判断を Go/No-Go と同時に行う。`crates/spike-harness` は残す。
- Rust コードもスパイクとはいえ CLAUDE.md コード規約に従う: `unwrap()` はテスト以外禁止、clippy warnings deny。**ただし**マイグレーション必須等の DB 規約は対象外（DB を作らないため）。
- JSONL・デバッグログにキャプチャ本文（ユーザーのテキスト）を含めない（CLAUDE.md 絶対規約）。テキストは長さ(bytes)と xxhash64 のみ記録する（§4.4）。

---

## 3. 技術仕様

### 3.1 NSPanel 生成

#### 3.1.1 Tauri WebviewWindow の NSPanel 化 — 手順方針

Tauri v2 が生成する `NSWindow` を実行時に NSPanel サブクラスへ差し替える。手順は2案あり、**調査タスク T-01（タイムボックス1日）で A を先に検証**する。

- **案A（優先）**: `tauri-nspanel` クレート（ahkohd/tauri-nspanel、v2 対応ブランチ）を利用する。調査項目: (1) Tauri v2.x 最新との互換、(2) `object_setClass` 方式でのクラス差し替え後に `styleMask` / `collectionBehavior` / `level` を本仕様の値へ設定できるか、(3) `becomesKeyOnlyIfNeeded` 相当の制御が可能か。
- **案B（フォールバック）**: 自前実装。`WebviewWindow::ns_window()` で `*mut c_void` を取得し、objc2 で `NSPanel` サブクラス（`canBecomeKeyWindow` を条件付き true、`canBecomeMainWindow` を常に false にオーバーライド）を宣言して `object_setClass` で差し替える。既知のリスク: クラス差し替え後の KVO / delegate の整合。実装時に要検証。

いずれの案でも、差し替えは Tauri の `setup` フック内（ウィンドウ生成直後、表示前）に行う。

#### 3.1.2 ウィンドウ属性（確定値）

| 属性 | 値 | 備考 |
|---|---|---|
| `styleMask` | `.borderless \| .nonactivatingPanel` | タイトルバーなし。クリックしてもアプリをアクティブ化しない |
| `level` | 初期値: `kCGStatusWindowLevel` (= 25) | メニューバー(`kCGMainMenuWindowLevel`=24)の1段上。**フルスクリーン検証(S-2)で隠れる場合は `kCGPopUpMenuWindowLevel` (=101) へ切替**。どちらで安定するかは実装時に要検証。切替はビルドフラグでなく実行時設定にしてソークで両方試せるようにする |
| `collectionBehavior` | `.canJoinAllSpaces \| .fullScreenAuxiliary \| .stationary \| .ignoresCycle` | `.stationary` は Mission Control でパネルが動かないため。Exposé 中の見え方は要観察（S-1 の記録項目） |
| `isOpaque` | `false` | |
| `backgroundColor` | `NSColor.clearColor` | |
| `hasShadow` | `false` 固定 | 影は CSS（webview 内）で描く。ネイティブ影の on/off 切替は再描画コストがあるため使わない |
| `hidesOnDeactivate` | `false` | |
| `isFloatingPanel` | `true` | |
| `becomesKeyOnlyIfNeeded` | `true` | §3.5 のフォーカス方針の要 |
| `isMovable` / `isMovableByWindowBackground` | `false` | |
| `animationBehavior` | `.none` | 独自アニメーションと二重にしない |
| `ignoresMouseEvents` | 状態依存（§3.1.3） | |

Tauri 側 `tauri.conf.json`: `transparent: true`, `decorations: false`, `resizable: false`, `alwaysOnTop` は使わない（level を直接制御するため）、`macOSPrivateApi: true`（透過 webview に必要。v2 での設定キー名は実装時に要検証）。

#### 3.1.3 ウィンドウフレーム戦略と `ignoresMouseEvents` の使い分け

**パネルの NSWindow フレームは Expanded 最大サイズで固定し、状態遷移で `setFrame` によるリサイズを行わない。** 理由: WKWebView のウィンドウリサイズは再レイアウト+再合成を伴い、Q2 の 100ms 予算に対する最大のリスクであるため。Idle⇄Expanded の見た目の変化はすべて webview 内の DOM（transform / opacity）で表現する。

- パネルフレーム: 幅 `max(ノッチ幅, Expanded幅400) + 32pt`、高さ `Expanded高さ180 + 8pt` = 実ノッチ機では概ね **432 × 188pt**。上端中央アンカー（パネル上辺 = 画面上辺、水平中央 = ノッチ中央）。
- `ignoresMouseEvents` の使い分け:
  - **Idle / HoverIntent / Collapsing: `true`**。パネルは全面クリック透過。ホバー検知はパネルではなく NSEvent グローバルモニタ（§3.4）が担うため、パネルがマウスイベントを受ける必要がない。これにより「透明部分の下のメニューバーやアプリがクリックできない」問題を根本回避する。
  - **Expanded: `false`**。展開 UI を操作可能にする。Expanded の可視矩形（400×180pt）の外側の透明マージン部分もクリックを受けてしまうが、マージンは左右16pt/下8ptのみであり、かつ webview 側で透明部分のクリックを「収束トリガ」として扱う（§3.3 T4）ので実害を仕様として吸収する。
- 遷移時の切替タイミング: `ignoresMouseEvents = false` は Expanded コミット（T2）と同時、`= true` は Collapsing 開始（T4）と同時に設定する。

### 3.2 ノッチ検出と寸法

#### 3.2.1 実ノッチの検出と実寸取得

- ノッチ有無: `NSScreen.safeAreaInsets.top > 0`（macOS 12+）。
- ノッチ実寸（すべて実行時に取得。**ハードコード禁止**）:
  - 高さ `notch_h = safeAreaInsets.top`（MacBook Pro 14"/16" で 32pt 前後の想定だが機種依存。実測値をログに記録）
  - 幅 `notch_w = screen.frame.width - auxiliaryTopLeftArea.width - auxiliaryTopRightArea.width`（`NSScreen.auxiliaryTopLeftArea` / `auxiliaryTopRightArea` は macOS 12+。`nil` の場合＝ノッチなし）
  - `auxiliaryTop*Area` の返す矩形の座標系・スケーリング時の丸めは実装時に要検証。起動時に実測値4値（notch_w, notch_h, left.width, right.width）を JSONL `event.notch_geometry` として記録すること。
- ノッチ矩形（NS座標、そのスクリーンのグローバル座標）:
  `N = { x: screen.frame.midX - notch_w/2, y: screen.frame.maxY - notch_h, w: notch_w, h: notch_h }`

#### 3.2.2 擬似ノッチ（ノッチ非搭載Mac・外部ディスプレイ）の寸法規定

- 表示位置: 対象スクリーンのメニューバー中央。パネル上辺 = 画面上辺。
- Idle 時の可視部（擬似ノッチ本体）: **幅 180pt × 高さ = メニューバー実測高さ `h_mb`**。
  - `h_mb = screen.frame.maxY - screen.visibleFrame.maxY`（メニューバー表示時）。この式は Dock 位置の影響を受けない想定だが実装時に要検証。取得不能・0 の場合のフォールバック: **24pt**。
  - 下辺の左右2隅に角丸 **8pt**（実ノッチのシルエットに寄せる）。塗りは #000000 不透明。
- 実ノッチ機の内蔵ディスプレイでは擬似ノッチを描画しない（物理ノッチがそのまま Idle 表示。webview は Idle 時に何も描かない、または notch 矩形と同寸の黒矩形を重ねて一体化させる。黒矩形を重ねる方式を既定とする — Expanded への連続アニメーションの起点になるため）。

#### 3.2.3 Expanded の寸法（両モード共通）

- 可視部: **幅 400pt × 高さ 180pt**、上端中央アンカー、下辺角丸 **16pt**。上辺は角丸なし（画面上端に密着）。
- 実ノッチ幅が 400pt を超える機種が現れた場合は `max(notch_w + 40, 400)` に広げる（現行機種では 400 固定になる想定）。

### 3.3 状態機械（確定。全遷移・全数値）

状態は Rust 側が単独所有する（webview は表示指示を受けるだけ。CLAUDE.md「データの重心はRustコアに置く」に整合）。状態遷移はすべて JSONL `event.state_transition` に記録する。

```
        T1: 進入領域R_enterに進入(条件付き)          T2: dwellタイマー満了
 Idle ──────────────────────────────▶ HoverIntent ─────────────────▶ Expanded
  ▲                                        │                            │
  │            T3: R_stay外へ退出(即時)      │                            │ T4: 収束トリガ
  │◀───────────────────────────────────────┘                            ▼
  │                        T6: アニメ完了(160ms)                    Collapsing
  └◀────────────────────────────────────────────────────────────────────┘
                                     ▲──── T5: R_enter再進入 ────▶ Expanded（復帰120ms）
```

| 遷移 | トリガ | 数値・条件 |
|---|---|---|
| **T1** Idle→HoverIntent | マウスが `R_enter`（§3.4.2）に進入 | ただし以下をすべて満たす場合のみ: (a) `NSEvent.pressedMouseButtons == 0`（ドラッグ中は展開しない）、(b) メニュー抑制中でない（§3.4.5）、(c) 対象スクリーンがパネル表示スクリーンである |
| **T2** HoverIntent→Expanded | dwell タイマー満了 | dwell = **100ms**（基準値）。進入速度 `v_enter > 1200pt/s` のとき **250ms** に延長（fly-by 対策、§3.4.4）。満了時刻 = Q2 の `t0`。`ignoresMouseEvents=false` に切替。展開アニメーション **120ms**、イージング cubic-bezier(0.32, 0.72, 0, 1) |
| **T3** HoverIntent→Idle | マウスが `R_stay`（R_enter を全方向+4pt 拡張したヒステリシス領域）から退出 | 即時（0ms）。視覚変化なし（HoverIntent は不可視状態。実ノッチで 2pt のグロー等の予告表示は**入れない** — 誤発火の視覚ノイズになるため） |
| **T4** Expanded→Collapsing | 次のいずれか: (a) マウスが `R_exp`（Expanded 可視矩形を全方向 **+16pt** 拡張）の外に出て **300ms** 経過（grace タイマー。R_exp 内に戻れば解除）、(b) Esc キー（パネルがキーのときのみ、§3.6）、(c) Expanded 内の透明マージン部クリック、(d) 状態遷移によらない強制収束（ディスプレイ構成変化 §3.7、スリープ §3.9） | Collapsing アニメーション **160ms**、ease-in。`ignoresMouseEvents=true` に切替（Collapsing 開始と同時） |
| **T5** Collapsing→Expanded | Collapsing 中に `R_enter` に再進入 | dwell なしで即時復帰。復帰アニメーション **120ms**（現在の transform 値から順方向へ） |
| **T6** Collapsing→Idle | Collapsing アニメーション完了 | 160ms 後。webview から完了通知（アニメーション `transitionend`）を受けて Rust 側状態を Idle に確定。通知が **400ms** 以内に来なければタイムアウトで強制 Idle（webview ハング検知としてJSONLに `event.anim_timeout` を記録） |

補足:

- Expanded 中の再展開（R_enter 再進入）は無視（すでに Expanded）。
- HoverIntent 中にマウスボタンが押下されたら即 Idle に戻す（メニューバークリックの前兆とみなす）。
- 状態機械のタイマーはすべて Rust 側（tokio の sleep もしくは dispatch timer）。webview 側にタイマーロジックを置かない。
- Expanded DOM は**常時マウント**しておく（`opacity:0; transform: scaleY(0.2); pointer-events:none` で待機、display:none は使わない）。T2 で class 切替のみ行う。レイアウト計算を展開時に走らせないことが Q2 達成の主要施策。

### 3.4 ホバー判定

#### 3.4.1 監視方式（イベント駆動。ポーリング禁止）

- 主方式: `NSEvent.addGlobalMonitorForEvents(matching: [.mouseMoved, .leftMouseDown, .leftMouseUp, .leftMouseDragged])` + 自アプリがイベントを受ける場合に備えた `addLocalMonitorForEvents`（同 matching）。
  - 要検証（T-02）: グローバルモニタは「他アプリに配信されたイベント」のみを受ける仕様のため、`.mouseMoved` が全ケースで届くか（特にメニュー追跡中・フルスクリーンアプリ上）を確認する。届かないケースがあれば listen-only の `CGEventTap`（`kCGEventMouseMoved`、`kCGEventTapOptionListenOnly`）へ切替。CGEventTap は Accessibility 権限（`AXIsProcessTrusted`）が必要。どちらを採用したかと理由を判定レポートに記録する。
  - `NSTimer` / tokio interval で `NSEvent.mouseLocation` をポーリングする実装は**禁止**（Q3 の CPU 予算を恒常的に食うため）。
- ハンドラのコスト規律（CPU 5% を守るための必須要件）:
  1. **早期リジェクト**: イベント座標 `y < screen.frame.maxY - 40`（上端 40pt バンド外）なら他の計算を一切せず return。この分岐までに許されるのは座標取り出しと1比較のみ。
  2. **コアレス**: 上端バンド内でも、前回処理から **16ms** 未満のイベントは座標だけ更新して判定をスキップ（120Hz でも実質 60fps 判定）。
  3. ハンドラ内でのメモリ確保・ログ書き込み禁止（判定結果はチャネルで状態機械スレッドに送る。ログはそちらで書く）。

#### 3.4.2 判定領域の形状と座標（NS座標、対象スクリーンのグローバル座標）

- **進入領域 `R_enter`**:
  - 実ノッチ: ノッチ矩形 `N`（§3.2.1）を **左右 +8pt、下方向 +4pt** 拡張（上方向は画面端のまま）。
    `R_enter = { x: N.x - 8, y: N.y - 4, w: N.w + 16, h: N.h + 4 }`
  - 擬似ノッチ: Idle 可視部矩形（180 × h_mb、上端中央）を同じく左右 +8pt、下 +4pt 拡張。
- **維持領域 `R_stay`** = `R_enter` を全方向 **+4pt** 拡張（ヒステリシス。境界ジッタで HoverIntent が明滅しないため）。
- **Expanded 維持領域 `R_exp`** = Expanded 可視矩形（400×180、上端中央）を全方向 **+16pt** 拡張。
- 左右拡張を 8pt に留める根拠: ノッチ左右はメニューバー項目（ステータスアイコン・メニューエクストラ）の領地であり、拡張しすぎると Q4 に直撃する。8pt はノッチ際のアイコンとの間隙の想定値。S-6 で誤発火が出たら第一に削るパラメータ（→ 4pt → 0pt）。

#### 3.4.3 dwell（滞留）判定

- `R_enter` 進入で HoverIntent 開始、dwell タイマー **100ms** をセット。
- タイマー動作中に `R_stay` 退出（T3）・マウス押下・メニュー抑制発動があればキャンセル。
- 100ms の根拠: 人間の「狙って止める」動作は 100ms 以上の滞留を伴い、水平横断（メニューバー上をマウスが横切るだけ）は幅 196pt の R_enter を通常 100ms 未満で通過する（600pt/s でも約 320ms → 通過しきらない場合があるため速度条件 §3.4.4 を併用する）。この値は S-6〜S-8 の結果で 80〜200ms の範囲で調整してよい（調整したら §6.4 のリトライ規定に従い再測定）。

#### 3.4.4 進入速度による dwell 延長（fly-by 対策）

- 直近 3 イベント（コアレス後、約 48ms 窓）の移動距離から速度 `v_enter` を算出（pt/s）。
- `v_enter > 1200 pt/s` で R_enter に進入した場合、dwell を **250ms** に延長。高速で画面上端に向かう動作（ウィンドウを画面上部へ投げる、メニューバーの遠いアイコンへ直行する）の途中通過を弾く。
- 1200pt/s は仮置き値。S-7 の記録から p90 通過速度を実測して再設定する（要実測）。

#### 3.4.5 メニューバークリックとの弁別（メニュー抑制）

- グローバル/ローカルモニタで `leftMouseDown` を監視し、押下座標が**メニューバー帯**（`y ≥ screen.frame.maxY - h_mb`、ただし `R_enter` の外）だった場合、「メニュー抑制」フラグを立てる。
- 解除条件: 対応する `leftMouseUp` から **300ms** 経過。抑制中は T1 を発火しない（メニューを開いてノッチ脇の項目へポインタを滑らせる操作で展開しないため)。
- NSMenu のトラッキング開始をアプリ外から直接検知する公開 API は存在しない認識（実装時に要検証。`NSMenuDidBeginTrackingNotification` は自アプリのメニュー限定）。上記の座標ベース抑制で代替し、S-6 で不足が判明したら CGEventTap での down-up 系列解析に切り替える。
- ドラッグ弁別: `pressedMouseButtons != 0` の間は T1 禁止（§3.3 T1 条件(a)）。ウィンドウ上端ドラッグ・メニューバードラッグを一括で弾く。

#### 3.4.6 クリックによる即時展開（補助経路）

- 擬似ノッチの可視部を直接クリックした場合は dwell を待たず即 T2 相当（展開コミット）とする。ただし Idle 時はパネルが `ignoresMouseEvents=true` なので、クリック検知はグローバルモニタの `leftMouseDown` 座標が擬似ノッチ可視矩形内にあることで判定する。実ノッチモードではこの経路はない（物理ノッチはクリック不能領域）。

#### 3.4.7 座標系の注意（実装規律）

- `NSEvent.mouseLocation` / NSScreen 系 = 左下原点。CGEventTap のイベント座標 = メインディスプレイ左上原点。混在バグは Q4 の典型的な偽因になるため、**判定モジュールの入口で NS 座標に正規化**し、内部表現を1本化する。変換関数に単体テストを付ける（マルチディスプレイ配置3パターン以上）。

### 3.5 展開時のフォーカス扱い

- 原則: **Expanded になってもキーフォーカスを奪わない。** `.nonactivatingPanel` + `becomesKeyOnlyIfNeeded=true` により、パネル内ボタンのクリックは前面アプリをディアクティベートせずに処理される。検証項目（S-9 に含める）: Expanded 中に前面アプリでそのままタイピングが継続できること、パネル内ボタンクリック後もメニューバー表示（前面アプリ名）が変わらないこと。
- **例外 — 検索入力**: Expanded 内のダミーテキストフィールドをクリックしたときのみ `makeKeyWindow`（`canBecomeKeyWindow` を「テキスト入力要素がフォーカス要求中のみ true」とする条件付きオーバーライド。要検証: NSPanel 差し替え後の webview の first responder 連携が期待通りか — T-01 の調査項目に含める）。
- キー返却: (a) Esc、(b) 収束（T4）、のいずれでも `resignKeyWindow` し、直前のアクティブアプリにキーが戻ることを確認する（`NSWorkspace.frontmostApplication` が変化していないことをアサート）。
- スパイクでの合否観点: 「キーを奪わないままボタン操作」「テキスト入力時のみキー取得→復帰」の2系統が両立するか。両立しない場合はその内容を判定レポートに記載（Phase 1 の UI 設計制約になる）。

### 3.6 キーボード

- Esc: パネルがキーのときのみ webview の keydown で受けて T4(b)。パネルが非キーのときのグローバル Esc 監視は**しない**（他アプリの Esc を奪う事故を避ける)。
- 手動マーク用ホットキー ⌃⌥⌘F（誤発火マーク、§4.2.4）: `addGlobalMonitorForEvents(matching: .keyDown)`。グローバル keyDown モニタは Accessibility 権限（または Input Monitoring 権限）が必要 — どちらの権限で動くかは実装時に要検証（T-02 に含める）。

### 3.7 マルチディスプレイ

#### 3.7.1 表示先の規定（スパイクでの確定方針）

- **パネルは常に1枚だけ表示する。**
- 内蔵ディスプレイがアクティブ（リッドオープン）なら **常に内蔵ディスプレイ**に表示（実ノッチモード。内蔵にノッチがなければ擬似ノッチモード）。外部ディスプレイには出さない。マウスが外部ディスプレイにあるときはホバー判定も休止（早期リジェクトで弾く）。
- 内蔵ディスプレイが無効（クラムシェル / デスクトップMacは対象外だがクラムシェルで等価になる）なら、`NSScreen.main`（キーウィンドウのあるスクリーン）ではなく **`NSScreen.screens[0]`（メニューバーのあるプライマリディスプレイ）** に擬似ノッチモードで表示。
- 「メインディスプレイ追従」（マウスやフォーカスのあるディスプレイへの移動）は**スパイクでは実装しない**。Phase 1 の設計課題として判定レポートに所感のみ残す。

#### 3.7.2 構成変化への追従

- `NSApplication.didChangeScreenParametersNotification` を購読。受信後 **500ms デバウンス**（接続時は通知が連発するため）してから: (1) 強制収束 T4(d)、(2) 表示先スクリーン再決定（§3.7.1）、(3) ノッチ再検出とパネル再配置、(4) ヘルスチェック（§3.9）、(5) JSONL `event.display_change` 記録。
- 再配置は既存パネルの `setFrame` を第一手段とし、失敗（フレームが反映されない・スクリーン喪失）時のみ再生成。再生成した場合は `event.panel_recreated` を記録（Q1 の判定材料）。
- スパイクの検証構成: **内蔵（ノッチあり）+ 外部1枚**。外部2枚以上は対象外。

### 3.8 フルスクリーンと擬似ノッチの挙動

- 実ノッチモード: 他アプリのネイティブフルスクリーン上でも `.fullScreenAuxiliary` によりパネルは表示継続（物理ノッチは常在なので違和感がない）。S-2 で、フルスクリーン中の展開・収束・レイテンシが通常時と同等（p95 差 +20ms 以内）であることを確認。
- 擬似ノッチモード（外部ディスプレイ / 非ノッチ機）でのフルスクリーン: メニューバーが自動非表示になるため、擬似ノッチが常時見えていると異物になる。挙動規定:
  1. フルスクリーン Space に入った検知（要検証: 確実な公開 API がない認識。第一候補はメニューバー可視性の変化 = `screen.visibleFrame.maxY == screen.frame.maxY` の変化を display_change / Space 切替のタイミングで確認。第二候補は `CGWindowListCopyWindowInfo` で Menubar ウィンドウの有無を確認 — ただしこれはポーリングになるため display 系イベント受信時のみ実行）→ 擬似ノッチの可視部を **opacity 0** にする（パネル自体は表示・監視は継続）。
  2. マウスが上端 **y ≥ maxY - 2pt** のホットゾーンに **500ms** 滞留（macOS のメニューバー再表示と同程度の感覚）→ 擬似ノッチ可視部を復帰（fade-in 120ms）→ 以降は通常の R_enter/dwell パイプライン。
  3. メニューバーが再び常時表示に戻ったら即 opacity 1 に復帰。
- この項は完成度よりも「破綻しないこと」を確認するのが目的。検知が不安定な場合は「擬似ノッチはフルスクリーン中も常時表示」へフォールバックし、その旨を判定レポートに記録（Phase 1 で解決）。

### 3.9 常駐ヘルスチェックとスリープ復帰

- ヘルスチェック内容: `panel.isVisible == true`、`panel.windowNumber > 0`、`panel.screen != nil`、`panel.frame` が期待値（§3.1.3）と一致（許容誤差 ±1pt）、`level` / `collectionBehavior` が設定値のまま。
- 実行タイミング（**定期ポーリングはソーク時のみ**）: (1) `NSWorkspace.didWakeNotification` の 1000ms 後、(2) `screensDidWakeNotification` の 1000ms 後、(3) display_change 処理後、(4) ソーク中は heartbeat（60s 間隔、§4.5)に同乗。
- 失敗時の自己修復: `orderFrontRegardless` → 直らなければ `setFrame` 再設定 → 直らなければパネル破棄+再生成。修復所要時間と手段を `event.panel_recovered` に記録。**修復開始から表示復帰まで 1000ms 以内**を自己修復の成功条件とする。
- スリープ時: `NSWorkspace.willSleepNotification` で強制収束（T4(d)）して Idle でスリープに入る。

### 3.10 context cache スパイク

#### 3.10.1 トリガ（イベント駆動）

1. `NSWorkspace.shared.notificationCenter` の `NSWorkspace.didActivateApplicationNotification`（アプリ切替）。
2. アクティブアプリの `AXObserver` に `kAXFocusedWindowChangedNotification`（同一アプリ内のウィンドウ切替）と `kAXTitleChangedNotification`（ブラウザのタブ切替検知の代替）を登録。アプリ切替のたびに旧アプリの Observer を解除し新アプリに付け替える。
3. `kAXTitleChangedNotification` は高頻度発火するアプリがある（プログレス表示をタイトルに出すアプリ等）ため、**同一ウィンドウのタイトル変化は 500ms デバウンス**。アプリ/ウィンドウ切替（1,2）はデバウンスなしで即時開始（300ms 予算を食わないため）。

#### 3.10.2 取得パイプライン（1回の更新）

時間予算（p95 300ms の内訳目標）: 検知→タスク起動 ≤10ms / AX 取得 ≤250ms / キャッシュ書込+記録 ≤10ms。残り 30ms はマージン。

1. 世代カウンタ `gen` をインクリメント。実行中の旧世代タスクにはキャンセルを通知（AX 呼び出しは同期ブロックするため、実際は「各要素処理の合間に gen を確認して中断」方式。専用スレッド1本で直列実行し、並行 AX 呼び出しはしない）。
2. `AXUIElementCreateApplication(pid)` → `AXUIElementCopyAttributeValue(app, kAXFocusedWindowAttribute)` でフォーカスウィンドウ取得。
3. `AXUIElementSetMessagingTimeout(element, 0.1)`（**100ms**）を app 要素に設定（応答しないアプリで 300ms 予算を溶かさないため。デフォルトは約6秒）。
4. ウィンドウタイトル: `kAXTitleAttribute`。
5. 可視テキスト収集 — **取得深さ・量の上限（確定値）**:
   - 走査: フォーカスウィンドウを根とする BFS。**深さ ≤ 8、訪問要素数 ≤ 300**。
   - 収集対象ロール: `AXStaticText`, `AXTextArea`, `AXTextField`, `AXHeading`, `AXLink`, `AXCell`, `AXMenuItem` 除外、`AXSecureTextField` は**存在ごとスキップ**（値を読まない・子孫にも入らない）。
   - 収集属性: `kAXValueAttribute` → 空なら `kAXTitleAttribute` → `kAXDescriptionAttribute` の順。
   - 総テキスト上限 **32KB (UTF-8)**。超過時は打ち切り、`truncated=true`。
   - 全体タイムボックス **250ms**: 超過したら打ち切って部分結果でキャッシュ更新（`partial=true`）。**部分結果は失敗ではない**（Q3 判定では partial も「更新完了」に数える。partial 率は別途記録し 30% 超なら深さ/要素数を削る）。
6. キャッシュ書込: `RwLock<ContextCache>`（構造: `{ gen, bundle_id, pid, window_title, text, text_bytes, captured_at, duration_ms, partial, truncated }`）。書込完了時刻が Q3 の `t1`（§4.2.2）。
7. Expanded 表示中に更新が完了したら webview へ push（表示はダミーのプレビュー領域に反映）。

#### 3.10.3 「押してから収集」禁止の実証

- Expanded への遷移（T2）時、webview に渡すコンテキストは**キャッシュの現在値のみ**。T2 をトリガとした AX 呼び出しをコード上不可能にする（AX 呼び出しは §3.10.2 のパイプラインからしか到達できないモジュール構成にする）。
- ハーネスが AX 呼び出し回数をカウント（パイプライン入口でインクリメント）し、Expanded 区間中のカウンタ増加が「表示のための取得」でないこと（= 増加はフォーカス切替イベント起因のみ）を記録。加えて T2 ごとに `cache_age_ms = t2 - captured_at` を JSONL 記録し、レポートに cache age の分布を出す（Phase 1 の鮮度設計の材料）。

#### 3.10.4 権限

- 起動時に `AXIsProcessTrustedWithOptions(kAXTrustedCheckOptionPrompt=true)`。未許可の間は cache パイプラインを停止し、パネル UI に「AX権限なし」インジケータ（ダミーで可）を出す。CGEventTap 採用時（§3.4.1）も同じ権限に相乗りする。

### 3.11 モジュール構成と Rust⇄webview IPC 契約

#### 3.11.1 Rust 側モジュール分割（`apps/desktop/src-tauri/src/`）

```
main.rs            # setup フック: パネル生成→NSPanel化→モジュール起動
panel.rs           # NSPanel 差し替え・属性設定・フレーム・ignoresMouseEvents 切替（§3.1）
geometry.rs        # ノッチ/擬似ノッチ検出、R_enter/R_stay/R_exp 算出、座標正規化（§3.2, §3.4.7）
hover.rs           # NSEvent/CGEventTap 監視、早期リジェクト、コアレス、速度算出（§3.4）
statemachine.rs    # 状態機械の唯一の実装。hover.rs からはチャネル経由で入力のみ（§3.3）
axcache.rs         # context cache パイプライン。AX 呼び出しはこのモジュール内に閉じる（§3.10.3）
display.rs         # display_change / sleep-wake / ヘルスチェック / 自己修復（§3.7〜§3.9）
ipc.rs             # webview への通知と webview からのイベント受信（§3.11.2）
```

- 依存方向の規律: `hover → statemachine → ipc/panel` の一方向。`axcache` は `statemachine` に依存しない（フォーカスイベントで独立に動く）。`statemachine` から `axcache` への参照は「キャッシュ現在値の読み取り」のみ（更新のトリガにしてはならない — §3.10.3 の実証条件）。
- 全モジュールは `spike-harness` の記録 API（非ブロッキング、リングバッファ投入のみ）を呼んでよい。

#### 3.11.2 IPC メッセージ契約（全メッセージを列挙。これ以外を追加しない）

Rust → webview（Tauri event。event 名とペイロード）:

| event | payload | 発火タイミング |
|---|---|---|
| `state` | `{ state: "idle"\|"hoverintent"\|"expanded"\|"collapsing", t0_mono_ns: u64 }` | 全状態遷移。webview は class 切替のみ行う |
| `geometry` | `{ mode: "notch"\|"pseudo", notch_w: f64, notch_h: f64, expanded_w: 400, expanded_h: 180, h_mb: f64 }` | 起動時・display_change 後 |
| `context` | `{ bundle_id: string, title_masked: string, text: string, captured_at_ms: u64, partial: bool }` | Expanded 中のキャッシュ更新時、および T2 直後の初期表示。`text`/`title` は webview 表示専用であり、webview 側で console.log 等に出力してはならない |
| `fs_mode` | `{ menubar_hidden: bool }` | 擬似ノッチのフルスクリーン挙動切替（§3.8） |
| `clock_sync` | `{ seq: u32, rust_mono_ns: u64 }` | 起動時5往復 + 10分ごと（§4.1） |

webview → Rust（Tauri command）:

| command | 引数 | 意味 |
|---|---|---|
| `painted` | `{ state: string, t1_perf_ms: f64 }` | rAF×2 完了通知。expand_latency / action_present の t1 |
| `anim_done` | `{ state: string }` | transitionend。T6 の確定入力 |
| `interact` | `{ kind: "click"\|"key"\|"scroll" }` | 誤発火判定用カウント（§4.2.4） |
| `collapse_request` | `{ reason: "esc"\|"outside_click" }` | T4(b)(c) |
| `focus_field` | `{ focused: bool }` | 検索フィールドのフォーカス要求（§3.5 の makeKey/resign トリガ） |
| `clock_sync_ack` | `{ seq: u32, js_perf_ms: f64 }` | クロック校正応答 |

- webview 側にタイマー・状態分岐・キャッシュを置かない（CLAUDE.md「webview 側にデータ層のロジックを置かない」に整合）。webview の責務は「class 切替」「描画完了通知」「操作イベント転送」の 3 つのみ。
- `title_masked` について: プレビュー表示にはタイトルを出してよい（画面上の表示は保存ではない）が、JSONL には §4.4 の通り本文を書かない。webview から Rust へタイトル/テキストを送り返す経路は作らない。

---

## 4. 計測ハーネス仕様（`crates/spike-harness` — 本実装へ持ち越す）

### 4.1 設計原則

- 計測点は「Rust 側 monotonic クロック（`std::time::Instant`、内部は `mach_absolute_time`）」を正とする。
- JS 側時刻（`performance.now()`）との整合: 起動時に IPC を **5 往復**させ、往復遅延の最小サンプルからオフセットを推定（NTP 方式の簡易版）。推定誤差の目安 ±2ms を計測誤差として全レポートに注記する。オフセットは 10 分ごとに再校正（webview プロセス再起動対策）。実装時に要検証: Tauri v2 IPC の片道遅延が安定して 1ms 台か（不安定なら校正回数を 20 往復に増やす）。
- ハーネスはリングバッファ（容量 8192 イベント）に貯め、**専用スレッドが 1 秒ごとにまとめて JSONL へ追記**。計測対象のホットパス（イベントハンドラ・状態機械）でファイル I/O をしない。
- SLO 数値（CLAUDE.md 表と完全一致）を定数モジュール `slo.rs` に一元定義: 展開 100ms / アクション提示 150ms / cache 300ms / アイドル CPU 5%。レポート生成はこの定数を参照して合否を出す（本実装持ち越し時にこのモジュールが SLO の単一の真実になる）。

### 4.2 計測点の定義

#### 4.2.1 `metric.expand_latency`（Q2）

- `t0`: 状態機械が T2（dwell 満了）を処理した時刻（Rust、Instant）。
- `t1`: webview が Expanded 用 class を適用 → `requestAnimationFrame` を2段ネストし、**2回目のコールバック時刻**（= 適用後フレームが提示されたことの近似。`performance.now()` をオフセット補正して Rust 時刻に変換）。1段目は「次フレームの開始」しか保証しないため2段とする。要検証: WKWebView での rAF とコンポジット完了の乖離。乖離検証として、開発時に一度だけ 240fps スロー撮影（iPhone）で実測し rAF 法との差を較正、レポートに補正値を記載する。
- 記録フィールド: `t0_epoch_ms`, `latency_ms (=t1-t0)`, `hover_enter_offset_ms (=t0-進入時刻)`, `total_perceived_ms`, `mode ("notch"|"pseudo")`, `fullscreen (bool)`, `display_count`。

#### 4.2.2 `metric.cache_update`（Q3-A）

- `t0`: フォーカス切替通知のハンドラ先頭（NSWorkspace 通知 or AXObserver コールバック受信時刻）。
- `t1`: `RwLock<ContextCache>` 書込完了直後。
- 記録フィールド: `latency_ms`, `trigger ("app_switch"|"window_switch"|"title_change")`, `bundle_id`, `text_bytes`, `text_xxh64`, `elements_visited`, `depth_reached`, `partial`, `truncated`, `cancelled (bool)`。**text 本文は記録しない**（CLAUDE.md 規約）。
- キャンセルされた世代（`cancelled=true`）はレイテンシ集計から除外し、件数のみ報告。

#### 4.2.3 `metric.cpu_sample`（Q3-B）

- 方式: **自プロセスの `task_info`**。`host_processor_info` / システム全体の使用率は使わない。
  - `task_info(mach_task_self(), MACH_TASK_BASIC_INFO)` の user/system 時間（終了スレッド分）+ `TASK_THREAD_TIMES_INFO`（生存スレッド分）の合算をΔ計算。等価な代替として `proc_pid_rusage(getpid(), RUSAGE_INFO_V4)` の `ri_user_time + ri_system_time` を用いてもよいが、**スパイク全体で1方式に統一**する（方式名を全サンプルに記録）。
  - CPU% = `Δcpu_time / Δwall_time × 100`（全スレッド合算、コア数で割らない = Activity Monitor 互換の 1 コア=100% 表記）。
- サンプリング間隔 **5s**、そこから **1 分移動平均**（12 サンプル）を算出して記録。
- **WebContent プロセスの扱い**: Tauri (WKWebView) のレンダリングは別プロセス（`com.apple.WebKit.WebContent` 等）で走るため、自プロセスだけでは過小評価になる。SLO 判定は「自プロセス + WebKit 補助プロセス」の合算で行う。補助プロセスの PID 特定は公開 API が乏しい認識（実装時に要検証）。スパイクでは外部計測で代替する: ソークスクリプトが 10s 間隔で `ps -axo pid,ppid,%cpu,comm` を取り、SHOGUN 起動後に出現し responsible が SHOGUN と推定される WebKit 系プロセスを突合して `metric.cpu_external` として記録（突合方法: `launchctl procinfo` の responsible pid、うまく取れなければ起動時刻±5s の WebContent を対応付け、いずれも要検証）。レポートには自プロセス単独値と合算値を両方出す。
- 「アイドル時」の定義: 直近 60s に T2（展開）が発生しておらず、かつフォーカス切替が 10 回未満の区間。ソークレポートではアイドル区間のサンプルのみを Q3-B 判定に使う（フリーワーク中の高負荷サンプルは参考値）。

#### 4.2.4 `event.expand_session` / 誤発火（Q4）

- Expanded 1 回ごとに 1 レコード: `opened_at`, `closed_at`, `duration_ms`, `interactions {clicks, keys, scrolls}`, `close_reason ("timeout"|"esc"|"outside_click"|"forced")`, `auto_false_positive (bool: interactions合計0 かつ duration<1500ms)`, `manual_false_positive (bool: 直後60s以内の⌃⌥⌘F)`。
- 分母カウンタ: `counter.top_band_entry` — 上端 40pt バンドへの進入回数（§3.4.1 の早期リジェクト通過数をコアレス後にカウント）。1 分ごとに集計値を JSONL へ。

#### 4.2.5 参考計測: アクション提示 150ms

- CLAUDE.md SLO「コンテキストアクションボタン提示 150ms」は Phase 0 では合否対象外（アクションはダミーのため）。ただしハーネスには `metric.action_present`（t0=T2、t1=ダミーボタン+キャッシュプレビューの rAF×2 描画完了）を実装し、参考値としてレポートに載せる。Phase 1 でこの計測点をそのまま使う。

### 4.3 集計

- p50 / p95 / p99 / max / n を算出。パーセンタイルは全件保持からの nearest-rank 法（サンプル数は高々数千のため近似アルゴリズム不要）。
- 集計軸: mode（notch/pseudo）× fullscreen（true/false）で層別。Q2/Q3 の合否は**層別すべて**が閾値内であること（例: 擬似ノッチだけ遅い、を見逃さない）。

### 4.4 記録フォーマット（JSONL）

- パス: `~/Library/Application Support/com.syogun.shogunai/metrics/YYYYMMDD.jsonl`（日次ローテーション。ソーク24hで最大2ファイル）。
- 1 行 = 1 レコード。共通フィールド: `{"ts": <epoch_ms>, "mono": <起動からのns>, "type": "<metric.*|event.*|counter.*|soak.*>", "v": 1, "payload": {...}}`
- 代表例:

```json
{"ts":1752896400123,"mono":8123456789,"type":"metric.expand_latency","v":1,"payload":{"latency_ms":62.4,"total_perceived_ms":168.1,"mode":"notch","fullscreen":false,"display_count":2}}
{"ts":1752896400500,"mono":8123833166,"type":"metric.cache_update","v":1,"payload":{"latency_ms":141.0,"trigger":"app_switch","bundle_id":"com.apple.Safari","text_bytes":18240,"text_xxh64":"9f3a...","elements_visited":211,"depth_reached":6,"partial":false,"truncated":false,"cancelled":false}}
{"ts":1752896460000,"mono":8183333333,"type":"soak.heartbeat","v":1,"payload":{"panel_visible":true,"panel_frame_ok":true,"state":"Idle","cpu_1min_avg":2.1,"rss_mb":184,"ax_calls_total":1420,"uptime_s":8183}}
```

- 禁止事項: payload にキャプチャテキスト本文・ウィンドウタイトル**本文**を入れない（タイトルも本文扱い。bundle_id と xxh64 のみ可）。

### 4.5 24時間ソークの自動記録

- `soak.heartbeat` を **60s 間隔**で記録: パネル健全性（§3.9 のチェック結果）、状態機械の現在状態、CPU 1分平均、RSS、AX 呼び出し累計、直近1分のイベント数。
- heartbeat が **180s** 以上途絶した区間はレポートで「記録空白（プロセス死またはハング疑い）」として必ず表示する（沈黙をもって合格にしない）。
- ソーク実行はスクリプト `scripts/spike-soak.sh` で開始: release ビルド起動 → `caffeinate -dims` は**使わない**（スリープ復帰も検証対象のため。ただし S-11 の 24h 走行中はディスプレイスリープのみ許可し、S-5 のスリープ試験は別枠で行う）→ 外部 CPU 計測（§4.2.3）を並走。

### 4.6 結果レポート（Markdown 出力）

- コマンド: `cargo run -p spike-harness --bin report -- <jsonl files...> -o docs/phase0-report-<date>.md`
- 内容: (1) 4 つの問いごとの合否と根拠数値（§6 の閾値を定数参照で照合）、(2) p50/p95/p99 表（層別込み）、(3) CPU タイムライン（1 分平均の折れ線を ASCII/テーブルで）、(4) 誤発火一覧（発生時刻・close_reason・手動/自動）、(5) パネル異常イベント一覧、(6) 記録空白区間、(7) 計測誤差の注記（クロックオフセット、rAF 較正値）。
- このレポートが §6 の判定会の一次資料になる。手集計は禁止（再現性のため）。

---

## 5. 検証シナリオ一覧

各シナリオ共通の記録: 実施日時、ビルド（release/commit hash）、ディスプレイ構成、mode。自動シナリオはスクリプト名を記載する。

| ID | 種別 | 手順 | 期待結果 | 記録項目 |
|---|---|---|---|---|
| S-1 | 手動 | Space を 4 つ作成し、⌃→ / 4本指スワイプ / Mission Control 経由で計 20 回切替。各 Space で 1 回展開 | 全 Space でパネル表示継続・展開可能。Mission Control 中の見え方に破綻がない（`.stationary` の挙動を目視記録） | `event.state_transition`、目視メモ |
| S-2 | 手動 | Safari で動画全画面 5 分、Keynote 再生 5 分。各中に展開×5 | フルスクリーン上に表示・展開できる。level 25 で不足なら 101 で再試験 | `metric.expand_latency (fullscreen=true)`、採用 level |
| S-3 | 手動 | 外部ディスプレイの接続/切断を各 10 回（ケーブル抜き差し）。切断中・接続中それぞれで展開確認 | 各操作後 2s 以内にパネルが正位置。再生成発生は記録されるが可視消失 2s 超なし | `event.display_change`, `event.panel_recreated/recovered` |
| S-4 | 手動 | 内蔵・外部それぞれで解像度/スケーリングを 3 段階変更（システム設定） | 同上。擬似ノッチ寸法が h_mb 追従 | 同上 + `event.notch_geometry` |
| S-5 | 手動 | 短時間スリープ（1〜5 分）×5 回 + 一晩スリープ×1 回。各復帰後 60s 以内に展開確認 | 復帰 2s 以内にパネル正常。ヘルスチェックが自動実行されている | `event.panel_recovered`、復帰後の `metric.expand_latency` |
| S-6 | 手動(台本) | メニューバー操作 50 回: ノッチ両脇のステータスアイコン各種をクリック→メニュー選択。うち 20 回はノッチ至近のアイコン | 誤発火 0 回 | `event.expand_session`、`counter.top_band_entry` |
| S-7 | 手動(台本) | 画面上端往復 20 回: 下半分から上端へマウスを投げる（速度まちまち）+ ウィンドウを画面上端へドラッグ 10 回 | 誤発火 0 回。ドラッグ中の T1 抑制が機能 | 同上 + `v_enter` 分布 |
| S-8 | 手動(台本) | Spotlight 起動 10 回、通知センター開閉 5 回、メニューバー日時クリック 5 回 | 誤発火 0 回 | 同上 |
| S-9 | 手動 | 前面アプリ（テキストエディタ）で連続タイピングしながら展開→ボタンクリック→収束×10。次に検索フィールドクリック→入力→Esc×10 | タイピングが途切れない（文字落ち 0）。Esc 後にエディタへキー復帰 | 目視 + `frontmostApplication` アサートログ |
| S-10 | 手動 | クラムシェルモード出入り×3（外部接続状態でリッド開閉） | 表示先が §3.7.1 通りに移動。可視消失 2s 超なし | `event.display_change` |
| S-11 | 自動 | 24 時間ソーク（`scripts/spike-soak.sh`）。日中はフリーワーク（実作業 8h、Q4 の分母を稼ぐ）、夜間は放置（アイドル CPU の判定区間） | heartbeat 欠損なし、可視消失 0、アイドル CPU 1分平均が §6.3 内 | `soak.heartbeat`, `metric.cpu_sample`, `metric.cpu_external`, `event.expand_session` |
| S-12 | 自動 | 展開試験 200 回: `scripts/spike-expand-test.sh` が CGEventPost で「画面中央→R_enter 内へ移動→150ms 静止→R_enter 外へ→収束待ち」を注入。100 回は通常デスクトップ、50 回は fullscreen 上、50 回は擬似ノッチ（外部）で実施 | `latency_expand` p95 ≤ 100ms（全層別）。注入イベントがグローバルモニタに届くことは事前確認（届かない場合は手動 200 回に切替、要検証） | `metric.expand_latency` n≥200 |
| S-13 | 自動 | cache 試験 100 回: `osascript` で 10 アプリ（Safari/Notes/Mail/Finder/Terminal/VS Code/Slack/Preview/Calendar/Music 相当、手元にある10種で読み替え可）を 3s 間隔でラウンドロビン activate ×10 周 | `metric.cache_update` p95 ≤ 300ms、partial 率 ≤ 30% | `metric.cache_update` n≥100 |
| S-14 | 手動 | AX 権限を一度剥奪→再付与（システム設定）。剥奪中の挙動確認 | クラッシュしない。cache 停止インジケータ表示。再付与後に自動 or 再起動で復帰（どちらかは実装時に決めて記録） | 目視 + JSONL |

---

## 6. Go/No-Go 判定基準

判定は S-1〜S-14 完了後、§4.6 のレポートを一次資料として行う。**4 問すべて合格で Go**（Phase 1 ノッチ本実装へ）。いずれか不合格の場合、下記リトライ規定（各 Q につきタイムボックス **2 営業日**、最大 1 巡）を消化してから最終判定する。リトライで仕様パラメータ（dwell 等）を変更した場合、**Q2・Q4 は両方再測定**する（トレードオフ関係にあるため片方だけの再測定は無効）。

### 6.1 Q1 常駐安定性

- 合格ライン:
  - 24h ソーク + S-1〜S-5, S-10 を通じて、**ユーザー可視のパネル消失（2 秒超）0 回**、プロセスクラッシュ 0 回、heartbeat 空白 0 区間。
  - 自己修復（`panel_recovered`、1s 以内復帰）は **24h で 2 回まで許容**。ただし発生した場合は原因（直前イベント）をレポートに記載し、Phase 1 の課題として登録。3 回以上は不合格。
- リトライで試すこと（順に）: (1) window level 変更（25⇄101）、(2) collectionBehavior から `.stationary` を外す/入れる、(3) display_change 時の「再配置→再生成」の閾値見直し、(4) 差し替え方式の変更（tauri-nspanel ⇄ 自前 object_setClass）。

### 6.2 Q2 展開レイテンシ

- 合格ライン: S-12 の n≥200 で `latency_expand` **p95 ≤ 100ms**、かつ層別（notch/pseudo × fullscreen 有無）のすべてで p95 ≤ 100ms。p50 ≤ 60ms を参考目標（未達でも不合格にしない）。`total_perceived` p95 ≤ 250ms を参考監視。
- リトライで試すこと（順に）: (1) Expanded DOM の事前マウント徹底・スタイル再計算の排除（`will-change: transform, opacity` 付与）、(2) webview の透過合成コスト調査（背景ブラー等の装飾を全部落として再測）、(3) IPC 経路短縮（T2 通知を Tauri event でなく生の `evaluateJavaScript` 相当で送る）、(4) 最終手段: Expanded の初回フレームだけネイティブ CALayer で出し webview を遅延表示するハイブリッド案の 1 日検証。 (4) まで行って未達なら No-Go（webview でのノッチ UI は不成立と結論）。

### 6.3 Q3 context cache + CPU

- 合格ライン A: S-13 の n≥100 で `cache_update` **p95 ≤ 300ms**（cancelled 除外、partial 含む）、partial 率 ≤ 30%。
- 合格ライン B: S-11 のアイドル区間（§4.2.3 定義）において、CPU 1 分平均（自プロセス + WebKit 補助合算）の**サンプルの 95% が ≤ 5%**、かつ最大値 ≤ 8%。中央値 ≤ 2.5% を参考目標。
- リトライで試すこと: A 未達→ (1) 深さ 8→6、要素 300→200 に削減、(2) MessagingTimeout 100→80ms、(3) 重量アプリ（Electron 系等）の層別を確認し特定アプリ起因なら「対象外アプリリスト」を仕様化して再判定。B 未達→ (1) mouseMoved コアレス 16→33ms、(2) rAF/アニメーション停止漏れ・webview のアイドル描画を調査、(3) AXObserver の付け替え頻度削減。両方 2 日で未達なら No-Go。

### 6.4 Q4 ホバー誤発火

- 合格ライン: S-6〜S-8（計 90 試行）で誤発火 **0 回**。かつ S-11 のフリーワーク 8h で誤発火（自動判定∪手動マーク）**≤ 5 回**かつ誤発火率 ≤ 2%。
- リトライで試すこと（順に、各変更後に S-6〜S-8 を再実施 + Q2 再測定）: (1) R_enter 左右拡張 8→4→0pt、(2) dwell 100→150→200ms、(3) 速度閾値 1200pt/s の実測再設定、(4) メニュー抑制の CGEventTap 化（§3.4.5）。dwell 200ms でも誤発火が閾値超過、または dwell 延長で `total_perceived` p95 が 400ms を超えて操作感が破綻する場合は No-Go。

### 6.5 判定の記録

- 判定結果（Go / No-Go、各 Q の数値、変更したパラメータ、残課題）を `docs/phase0-report-<date>.md` に確定版として残し、CLAUDE.md の開発フェーズ更新（Phase 0 → 1 または転換）の根拠にする。

---

## 7. No-Go 時の転換パス（メニューバー常駐 + コマンドパレット方式)

### 7.1 方式概要

- 常駐: `NSStatusItem`（メニューバーアイコン。SHOGUN ブランドルールに従い ⚔ モチーフ）。クリックでパレット表示。
- 呼び出し: グローバルホットキー（既定 ⌥Space、変更可能）でコマンドパレット表示。
- パレットの実体: **本スパイクと同じ borderless NSPanel**（`.nonactivatingPanel` + `.canJoinAllSpaces` + `.fullScreenAuxiliary`）を画面上部中央（メニューバー下 8pt、幅 560pt × 高さ可変 ≤ 400pt）に表示。ホバー展開は廃止し、明示呼び出しのみ。
- SLO の読み替え: 「Notch 展開 100ms」→「ホットキー押下→パレット描画完了 100ms」。cache 300ms / CPU 5% / アクション提示 150ms はそのまま適用。

### 7.2 スパイク成果物の流用可否

| 成果物 | 流用 |
|---|---|
| 計測ハーネス（§4 全部、slo.rs 含む） | **そのまま流用**（計測点名だけ読み替え） |
| context cache スパイク（§3.10 全部） | **そのまま流用**（方式に依存しない。Q3 が合格していれば転換後の再検証は不要） |
| NSPanel 生成・差し替えコード（§3.1） | 流用（フレーム戦略と level は再調整） |
| ノッチ検出（§3.2） | 廃棄（パレット位置決めに h_mb 取得のみ再利用） |
| 状態機械（§3.3） | 縮退流用（HoverIntent 削除、Idle→Expanded→Collapsing の 3 状態 + ホットキートリガ） |
| ホバー判定（§3.4） | 廃棄（グローバルホットキー処理のみ新規） |
| フォーカス扱い（§3.5） | 方針ごと流用（パレットは検索入力が主役になるため「例外」が既定になる点のみ再設計） |
| 検証シナリオ | S-1〜S-5, S-9〜S-14 を読み替えて流用。S-6〜S-8（誤発火）は不要になる |

- 転換時は本仕様書を改訂せず、新規に `docs/palette-ui-spec.md` を起こす（本書は Phase 0 の判断記録として凍結）。

---

## 8. スパイク実施チェックリスト（実装タスク分解・目安順序）

目安工数は実装エージェントの 1 セッション単位ではなく人日相当の粒度。依存関係順。

| # | タスク | 内容 | 目安 | 依存 |
|---|---|---|---|---|
| T-01 | 調査: tauri-nspanel | §3.1.1 案A の検証。不成立なら案B 実装に切替判断 | 1d | - |
| T-02 | 調査: グローバル監視の権限と配信 | mouseMoved グローバルモニタの配信範囲、CGEventTap 要否、keyDown モニタの必要権限（§3.4.1, §3.6） | 0.5d | - |
| T-03 | ワークスペース骨組み | ルート Cargo.toml、apps/desktop の Tauri v2 化、crates/spike-harness 空実装、release ビルド確認（arm64） | 0.5d | - |
| T-04 | ハーネスコア | クロック校正、リングバッファ、JSONL writer、slo.rs、cpu_sample（task_info） | 1d | T-03 |
| T-05 | NSPanel 化 + 属性設定 | §3.1.2 の全属性、フレーム戦略 §3.1.3、透過確認 | 1d | T-01, T-03 |
| T-06 | ノッチ検出 + 擬似ノッチ | §3.2。両モードの Idle 表示、notch_geometry 記録 | 1d | T-05 |
| T-07 | ホバー監視 | §3.4 モニタ + 早期リジェクト + コアレス + 座標正規化（単体テスト付き） | 1d | T-02, T-04 |
| T-08 | 状態機械 | §3.3 全遷移、タイマー、webview への遷移通知、Expanded ダミー UI（事前マウント） | 1.5d | T-05, T-07 |
| T-09 | 誤発火対策一式 | dwell、速度延長、メニュー抑制、ドラッグ抑制、手動マークホットキー | 1d | T-08 |
| T-10 | expand_latency 計測 | §4.2.1（rAF×2、較正含む）、action_present 参考計測 | 0.5d | T-04, T-08 |
| T-11 | context cache | §3.10 全部 + cache_update 計測 + AX カウンタアサート | 2d | T-04 |
| T-12 | マルチディスプレイ/フルスクリーン/スリープ | §3.7〜§3.9（display_change、ヘルスチェック、自己修復、擬似ノッチのFS挙動） | 1.5d | T-06, T-08 |
| T-13 | ソーク/自動化スクリプト | spike-soak.sh、spike-expand-test.sh（CGEventPost）、S-13 osascript、外部 CPU 計測 | 1d | T-10, T-11 |
| T-14 | レポート生成 | §4.6 の report バイナリ | 0.5d | T-04 |
| T-15 | シナリオ実施 | S-1〜S-10, S-12〜S-14 実施 + S-11 24h ソーク（並行で他作業可） | 2d + 24h | T-09〜T-14 |
| T-16 | 判定 | §6 手順、必要ならリトライ（最大 +2d×該当Q）、レポート確定 | 1d | T-15 |

合計目安: 約 13 人日 + ソーク 24h + リトライ余地 → タイムボックス 15 営業日に収まる想定。遅延時は §2.2 の「作らないもの」を増やす方向で調整し、4 つの問いへの回答可能性だけは削らない。

---

## 付録A. 本仕様で確定した主要パラメータ一覧（早見表）

| パラメータ | 値 | 根拠/備考 |
|---|---|---|
| dwell（通常） | 100ms | §3.4.3。調整可能域 80〜200ms |
| dwell（高速進入時） | 250ms | §3.4.4 |
| 高速進入の速度閾値 | 1200 pt/s | 仮置き。S-7 で実測再設定 |
| R_enter 拡張 | 左右+8pt / 下+4pt | §3.4.2。誤発火時に最初に削る |
| R_stay ヒステリシス | +4pt | §3.4.2 |
| R_exp 拡張 | +16pt | §3.4.2 |
| Expanded 退出 grace | 300ms | §3.3 T4 |
| 展開アニメーション | 120ms | §3.3 T2 |
| 収束アニメーション | 160ms（タイムアウト 400ms） | §3.3 T6 |
| Collapsing 復帰 | 120ms | §3.3 T5 |
| メニュー抑制解除 | mouseUp + 300ms | §3.4.5 |
| mouseMoved コアレス | 16ms | §3.4.1 |
| 早期リジェクト帯 | 上端 40pt | §3.4.1 |
| Expanded 可視寸法 | 400×180pt / 角丸16pt | §3.2.3 |
| 擬似ノッチ Idle | 180pt × h_mb（fallback 24pt）/ 角丸8pt | §3.2.2 |
| パネル固定フレーム | 約 432×188pt | §3.1.3 |
| window level | 25（fallback 101） | §3.1.2 |
| AX MessagingTimeout | 100ms | §3.10.2 |
| AX 走査上限 | 深さ8 / 300要素 / 32KB / 全体250ms | §3.10.2 |
| タイトル変化デバウンス | 500ms | §3.10.1 |
| display_change デバウンス | 500ms | §3.7.2 |
| wake 後ヘルスチェック | +1000ms | §3.9 |
| 自己修復成功条件 | 1000ms 以内 | §3.9 |
| 誤発火自動判定 | 操作0 かつ滞在<1500ms | §4.2.4 |
| 誤発火閾値 | 台本0回 / 8hで≤5回かつ≤2% | §6.4 |
| CPU サンプリング | 5s間隔・1分移動平均 | §4.2.3 |
| soak heartbeat | 60s（空白判定 180s） | §4.5 |
| SLO（CLAUDE.md 準拠） | 展開100ms / アクション提示150ms / cache300ms / アイドルCPU5% | slo.rs に一元定義 |

## 付録B. 「実装時に要検証」項目の一覧

1. tauri-nspanel の Tauri v2 互換と class 差し替え後の挙動（T-01）
2. NSEvent グローバルモニタでの mouseMoved 配信範囲（メニュー追跡中・フルスクリーン上）と CGEventTap への切替要否（T-02）
3. グローバル keyDown モニタに必要な権限（Accessibility か Input Monitoring か）（T-02）
4. `auxiliaryTopLeftArea` / `auxiliaryTopRightArea` の座標系とスケーリング時の丸め（§3.2.1）
5. `h_mb = frame.maxY - visibleFrame.maxY` 式の信頼性（メニューバー自動非表示時含む）（§3.2.2）
6. window level 25 でフルスクリーン上に表示されるか（不可なら 101）（§3.1.2）
7. `macOSPrivateApi` 相当の Tauri v2 設定キーと透過 webview の成立（§3.1.2）
8. NSPanel 差し替え後の webview first responder 連携（検索入力の makeKey/resign）（§3.5）
9. WKWebView の rAF×2 とコンポジット完了の乖離（スロー撮影で較正）（§4.2.1）
10. Tauri v2 IPC の片道遅延の安定性（クロック校正精度）（§4.1）
11. WebKit 補助プロセスの PID 特定と CPU 帰属（§4.2.3）
12. フルスクリーン Space 検知の確実な方法（§3.8）
13. NSMenu トラッキングの外部検知不可の確認と座標ベース抑制の十分性（§3.4.5）
14. CGEventPost 注入イベントがグローバルモニタに届くか（S-12 自動化の成立性）
15. AX 権限の再付与後に再起動なしで AXObserver が復帰するか（S-14）
