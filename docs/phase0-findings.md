# Phase 0 findings ledger

Answer台帳 for the "実装時に要検証" items (spec `docs/notch-ui-prototype-spec.md` 付録B) plus the
Stage-A investigation (T-01/T-02). Every item is `RESOLVED` (with a decision), `PENDING`
(deferred with reason), or `ON-DEVICE` (needs an Apple Silicon Mac to close). T-16 requires
every item to be non-open.

- Environment for this pass: Linux x86_64 (no macOS build, no measurement). Web research +
  Linux-verifiable harness only. Sources are inline.
- Legend — Confidence: H(igh, primary source) / M(edium) / L(ow, unconfirmed).

## Stage-A decisions (drive the on-device implementation)

| # | Question | Decision | Confidence |
|---|---|---|---|
| A1 | NSPanel化の方式 | **案A採用**: `tauri-nspanel` branch `v2.1`（git依存、crates.io未公開）。`object_setClass` でNSWindow差し替え、`to_panel()` / `set_level` / `set_collection_behavior` / `set_becomes_key_only_if_needed` が揃う。**例外**: `canBecomeKeyWindow` がビルド時固定のため、検索入力の動的key化は `set_can_become_key_window` 実行時トグル可否を実機確認し、不可なら該当部のみ案B（自前 `define_class!`）で補う | H / 例外部分 M |
| A2 | ホバー監視方式 | **最初からCGEventTap（listen-only, `kCGEventMouseMoved`）**。`addGlobalMonitorForEvents` は「他アプリに配信されたイベントのコピー」のみ受け、メニュー追跡中・他アプリfullscreen上で mouseMoved を取りこぼす一次報告あり。S-6実測を待たずtap前提で組む（手戻り最小、spec §3.4.1のフォールバックを既定に繰り上げ） | H |
| A3 | 必要権限 | **Accessibility 単一カテゴリ**（`kTCCServiceAccessibility`）。CGEventTap・グローバルkeyDown・AXUIElement すべて相乗り。非サンドボックス運用なら Input Monitoring 不要の見込み | M〜H |

案A採用の帰結として `apps/desktop/src-tauri/Cargo.toml` の macOS依存（tauri-nspanel/objc2系）を有効化済み。

## macOS アダプタのコンパイル検証（Apple Silicon CI, macos-14）

`.github/workflows/phase0-ci.yml` の macOS ジョブで、実機ランナー上のビルドを CI として回した。以下は**コンパイル green を確認済み**（実行時の挙動・4つの問いの実測は依然オンデバイス）。ビルド過程で判明した一次資料の誤りも修正済み。

| アダプタ | 実装 | CI |
|---|---|---|
| geometry (T-06) | NSScreen frame/visibleFrame/safeAreaInsets + auxiliaryTopLeft/RightArea → spike_core::regions | green |
| cpu (T-04/§4.2.3) | proc_pid_rusage + mach_timebase_info 変換 | green |
| panel (T-05) | tauri-nspanel v2.1 `tauri_panel!`＋`to_panel::<NotchPanel>()`、level/collectionBehavior/styleMask 設定 | green |
| hover (T-07) | listen-only CGEventTap（CFRunLoop スレッド）で生サンプル転送 | green |
| axcache (T-11) | AXUIElement を `AxNode` 実装（Clone=CFRetain/Drop=CFRelease、create-rule/get-rule 厳密化）、snapshot→walk | green |
| display (T-12) | NSWorkspace frontmostApplication().processIdentifier() | green |

**ビルドで判明した一次資料の誤り（修正済み）**:
- objc2 の版: core クレート = **0.6.x**、framework クレート（app-kit/foundation/core-graphics/core-foundation）= **0.3.x** の混成（当初 0.3 系で誤認）。
- `tauri_panel!` は snake_case config（`can_become_key_window` 等）＋`to_panel` は**具体ランタイム Wry** に対して呼ぶ（generic R では `FromWindow<R>` 未充足）。マクロ展開は `tauri::Manager` をスコープに要求。
- `NSScreen::safeAreaInsets/auxiliaryTop*Area` は objc2-app-kit 0.3.2 では **safe fn**（当初 unsafe と誤認）。
- **cpu の単位バグ**: `ri_user_time/ri_system_time` は Apple Silicon で ns ではなく **mach ティック**（osquery#7459）。mach_timebase_info で変換するよう修正。**Q3-B 判定前に Activity Monitor と実機照合が必須**。

## 統合レイヤ（挙動配線）— 純ロジックはテスト済み・macOSグルーはコンパイル検証済み

| 配線 | 実装場所 | 検証 |
|---|---|---|
| hover生サンプル→CG/NS正規化→HoverTracker→StateMachine 統合 | `spike_core::engine::NotchEngine`（純, 3統合テスト） | Linux green |
| 状態遷移→出力（webview state / ignoresMouse / timer / expand-commit / top-band） | 同上（`EngineOutput`） | Linux green |
| ハーネス JSONL writer（非ブロッキング record＋1秒 file flusher） | `spike_harness::recorder::Recorder`（2テスト） | Linux green |
| エンジンループ＋世代キャンセル式ワンショットタイマー＋出力適用（panel `set_ignores_mouse_events`＝run_on_main_thread、webview `emit("state")`） | `apps/desktop/src-tauri/src/integrate.rs`（macOS） | **Apple Silicon CI green** |

これで「イベント駆動→状態遷移→UIレンダリング＋計測記録」の統合が、純ロジックは単体テスト、macOS配線はコンパイルまで検証された。

**真にオンデバイスでしか完了しない残作業（挙動観測・実測・ハードウェア依存）**:
- 実行して観測: パネルの実表示/展開アニメ、`ignoresMouseEvents` 切替の体感、常駐安定性。
- webview `painted` 往復による**展開レイテンシ実測**（t1−t0＋クロックオフセット）。現状はエンジンが t0（expand-commit）を記録するのみ。
- **AXObserver/NSWorkspace 通知購読**（フォーカス変化でAX walkを起動）。`axcache::snapshot` はコンパイル済み、イベント購読（block2 の `addObserverForName…` セレクタは要実機確認）はオンデバイスで追加。
- 検索入力時の動的 key window（#104 の level 25/101 切替）。
- **S-11/S-12/S-13 の実測**と**4つの問いの Go/No-Go 判定**（物理ノッチMac＋人間の判断）。

## 付録B 15項目

| # | 項目 | 状態 | 結論 / 次アクション | 出典・確信度 |
|---|---|---|---|---|
| 1 | tauri-nspanel の v2互換とクラス差し替え後挙動 | RESOLVED（実装はON-DEVICE） | v2.1ブランチで対応。既知Issue: #104（高level でIMEブロック）, #119（close時クラッシュ）, #115（mouse tracking area）。→ level運用とclose処理を実機で確認 | github.com/ahkohd/tauri-nspanel（H） |
| 2 | mouseMoved グローバルモニタ配信範囲 / CGEventTap要否 | RESOLVED | A2の通りCGEventTap前提。フルスクリーン上の取得は実機で最終確認 | Apple NSEvent docs「copies of events posted to other applications」（H） |
| 3 | グローバルkeyDownモニタの権限 | RESOLVED（実機で最終確認） | Accessibility。実機で「Accessibilityのみ許可・Input Monitoring未許可」でkeyDown到達を確認 | Apple EventOverview / TCC資料（M〜H） |
| 4 | auxiliaryTopLeft/RightArea の座標系・スケール丸め | RESOLVED（丸めはON-DEVICE） | 型 `NSRect?`、ノッチ無しは `nil`、NS座標=左下原点。`notch_w = frame.w - left.w - right.w`。HiDPIスケール変更時のpt/px丸めのみ実機実測 | Apple NSScreen docs（H） |
| 5 | `h_mb = frame.maxY - visibleFrame.maxY` の信頼性 | ON-DEVICE | メニューバー自動非表示・Dock位置・複数ディスプレイでの値を実測。取得不能時fallback 24pt（spec §3.2.2） | — （実機） |
| 6 | level 25 でfullscreen上表示可否（不可なら101） | RESOLVED（実挙動ON-DEVICE） | 値: status=25 / popUpMenu=101。fullscreen上表示の本質は level ではなく `collectionBehavior` の `.fullScreenAuxiliary`＋`.canJoinAllSpaces`。**101はIMEブロック（#104）なので検索入力中は25へ下げる**。実行時トグルで両方ソーク | CGWindowLevel.h（H）/ 実挙動（M） |
| 7 | Tauri v2 の透過設定キー | RESOLVED | v2は `app.macOSPrivateApi: true`＋Cargo feature `macos-private-api`（tauri.conf.json / Cargo.toml 反映済み）。DMG化で透過が失われる報告（#13415）は配布時（v1）に確認、Phase 0は無関係 | v2.tauri.app/reference/config（H） |
| 8 | NSPanel差し替え後の first responder / makeKey | ON-DEVICE | 検索入力時のみkey化。`set_can_become_key_window` 実行時トグルで足りるか→不可なら案B。level 25へ下げてから makeKey、閉じたら戻す状態遷移（spec §3.5） | tauri-nspanel #104（L〜M） |
| 9 | WKWebView の rAF×2 とコンポジット完了の乖離 | PENDING（ON-DEVICE較正） | rAFは「次フレーム描画前」保証でありコンポジット完了保証ではない（WebKit Bug 177484）。WKWebView固有のコンポジット完了API公開は**未確認**。→ 二段rAF近似で実装（frontend実装済み）、240fpsスロー撮影で実機較正しレポートに補正値記載（spec §4.2.1） | WebKit Bug 177484（L） |
| 10 | Tauri v2 IPC 片道遅延の安定性（クロック校正精度） | ON-DEVICE | 起動時5往復の最小RTTでオフセット推定（harness `clock.rs` 実装済み・単体テスト済み）。片道が1ms台で安定しなければ校正20往復へ増やす（spec §4.1） | harness実装（Linux検証済み）/ 実機遅延（—） |
| 11 | WebContent プロセスのCPU帰属 | PENDING（SPI要検討） | WKWebViewはUI/Networking/WebContent複数プロセス。親からの特定は `responsibility_get_pid_responsible_for_pid`（**非公開SPI**）併用が実質必要。App Store非配布なので採用可。代替: ソークスクリプトで `ps` 突合（spec §4.2.3）。harness側は自プロセスtask_info実装済み | WKWebView資料（L〜M） |
| 12 | 他アプリfullscreen Space検知の公開API | RESOLVED（不在を確認） | 公開APIでは直接検知不可。公開 `NSWorkspaceActiveSpaceDidChangeNotification`＋必要なら `CGSSpaceGetType`（SPI, `CGSSpaceTypeFullscreen==4`）。→ spec §3.8 のメニューバー可視性fallbackを既定に、SPIは任意 | alt-tab-macos #447（M） |
| 13 | NSMenuトラッキングの外部検知不可の確認 | RESOLVED | 他アプリのNSMenuトラッキング開始を検知する公開APIなし。→ spec §3.4.5 の座標ベース抑制で代替、不足時のみCGEventTapのdown-up系列解析 | 一般資料（M） |
| 14 | CGEventPost 注入がグローバルモニタ/tapに届くか | ON-DEVICE | S-12自動化（`spike-expand-test.sh`）の成立性。届かなければ手動200回に切替（spec S-12） | —（実機） |
| 15 | AX権限再付与後にAXObserverが再起動なしで復帰するか | ON-DEVICE | S-14で確認。復帰不可なら再起動導線を出す（どちらか実機で決定しレポート記録） | —（実機） |

## 使用予定クレート（研究時点の最新・要実機ビルド確認）

| 用途 | クレート | バージョン | 備考 |
|---|---|---|---|
| NSPanel化 | tauri-nspanel | git branch `v2.1` | crates.io未公開。Cargo.tomlにコメントで用意 |
| AppKit/CoreGraphics | objc2 / objc2-app-kit / objc2-core-graphics | 0.3.2 | madsmtm保守、Xcode 16.4 SDK生成、MSRV 1.71 |
| Accessibility | accessibility-sys もしくは axuielement | 0.1 系 / doom-fish版 | `AXUIElementSetMessagingTimeout` / `kAXFocusedWindowChangedNotification` の網羅は実機確認 |

## 純ロジックのLinux先行実装（`crates/spike-core`）

Q2（展開）/Q4（誤発火）の挙動はプラットフォーム非依存の決定ロジックなので、macOS実機を待たず先行実装し単体テストで固めた。macOS層は薄いアダプタ（OSイベントを流し込み、Effectを適用するだけ）に縮小される。

| モジュール | 内容 | テスト |
|---|---|---|
| `geometry` | Rect/Regions、idle_rect、R_enter/R_stay/R_exp導出、CG↔NS座標正規化（involution） | 6件 |
| `hover` | early-reject、16msコアレス、速度推定/fast-dwell、メニュー/ドラッグ抑制 → HoverSignal | 7件 |
| `statemachine` | T1〜T6の決定的遷移、タイマー注入、Effect（Transition/Timer/SetIgnoresMouse/MarkExpandCommit） | 11件 |
| `axcache` | BFS walk policy（深さ8/300要素/32KB/SecureTextField subtree skip/cancel→partial） | 7件 |

**注（dev-instructions §5.7との整合・確定）**: 同§は「spike-harness以外は使い捨て・製品基礎の作り込み禁止」。本クレートは *4つの問いに答えるために正しい状態機械とヒット領域計算が必須で、かつそれをテストする*（macOSクレートはLinuxでテスト不可）ため設けた、テスト可能化のための意図的な逸脱。**扱いは「残す」で確定**（dev-instructions §8.1 に正式化）: `spike-harness` と同じく Phase 0 から持ち越す資産とし、逸脱の許容範囲は spike-core（純ロジック＋テストのみ）に限定する。`apps/desktop/src-tauri` は引き続き使い捨て。

## この環境（Linux）で検証済みのこと

- `crates/spike-harness`: `cargo test`（27件green）, clippy（deny下clean）, release build成功。計測核（slo定数/クロックオフセット/JSONLスキーマ/リングバッファ/パーセンタイル/CPU差分算術/digest）を単体テスト。
- `crates/spike-core`: `cargo test`（31件green）, clippy clean。上表の挙動ロジック。
- `report` バイナリ: 合成JSONLで層別p50/p95/p99・4問別verdict出力をE2E確認。
- `apps/desktop` frontend: `tsc --noEmit` clean, `vite build` 成功。
- **未検証（要macOS）**: Tauri/AppKitビルド全般、cpu.rs のmacOSリーダー（task_info/proc_pid_rusage）、`pnpm tauri dev`、CGEventTap/AXUIElement/NSPanelアダプタ、すべての実測（S-11/S-12/S-13、4つの問い本体）。


## 準実機セッション(GitHub Actions macos-14ランナー、擬似ノッチ・仮想ディスプレイ) — 2026-07-19

`phase0-smoke.yml` で release ビルドを Apple Silicon 実機ランナー上で7回実行(TCC DB直接付与+Swift製CGEventPost注入)。物理ノッチ・人間なしのため正式なS-11/12/13の代替ではないが、初の「動かして測る」証拠。

**実証できたこと(runs #2/#3/#7、各150秒・221レコード)**:
- 起動〜常駐150秒: クラッシュ0、heartbeat 2回、パネル自己修復0。メニューバーに前面アプリとして常駐(スクリーンショット確認)
- **Q3-B(アイドルCPU)**: 1分平均 最大 0.32〜0.39%、全サンプル5%以内 → SLO(5%)に対し1桁以上の余裕で PASS 圏。RSS 65MB
- hover→engine→panel ループ: 注入25サイクル中 22回の Expanded セッションが発生・記録(expand_commit/state_transition/top_band_entry/tap_status すべて発行)
- 計測パイプライン: JSONL日付ローテーション、report生成、空データガード(MISSING表示)が実データで機能

**発見した問題と結論**:
1. **wry 0.55.1 panic**: `WebviewWindow::eval()`/`url()` を(NSPanel化有無に関わらずこの構成で)呼ぶと `wkwebview/mod.rs:1349` unwrap-None で main スレッド即死(runs #4/#5)。`.on_page_load` も同様。**Rust側からのwebviewプローブ禁止**をlib.rsに明記。上流Issue要確認。
2. **webviewサイレント(未解決)**: webview→Rust コマンドが boot ping 含め0件。SPIKE_NO_PANEL=1(swapなし)でも同じ(run #7)→ **NSPanel swapは無罪**。capabilities(core:default)付与でも変化なし。残る仮説はヘッドレスCI環境でのWKWebViewコンテンツロード/JS実行/IPCブリッジ注入の不成立。**実機D-01でdevtools(またはSafari Web Inspector)を開けば数分で確定する類** — CI経由での深追いはeval panicにより手段が尽きた。
   - 影響: expand_latency(Q2のt1)とinteract集計はCI上で取得不可。**Q2実測とQ4の正確な集計は物理Mac必須のまま**。
3. Q4のFP率73%はwebview沈黙の帰結(全収束がAnimTimeout・interactions=0で自動FP判定)であり、ホバー判定の欠陥ではない(台本注入は操作ゼロのため定義上FPになる)。

## 物理ノッチ実機セッション（TorunoMacBook-Pro-8、ノッチ搭載・2画面）— 2026-07-19/20

ユーザー実機で `pnpm dev` により起動。geometry: notch=true notch_w=220 notch_h=38 menubar_h=39 screen=1800x1169 displays=2。

**確定した事実**:
1. **CIのwebviewサイレントは環境固有と確定**: 実機では boot ping / clock_sync_ack / painted / anim_done / interact が全て正常発火。コード欠陥ではなくヘッドレスVMのWKWebView問題（上記「未解決」を実機で解消）。
2. **Q2（展開レイテンシ）: p50 12.3ms / p95 18.1ms → PASS**（SLO 100msの約1/5）。n=15のクリーンなrun。
3. **Q3-B（アイドルCPU）: 1分平均 max 0.92%・全サンプル5%以内 → PASS**。
4. **Q4（誤発火）: 人間マーク0件・体感誤発火なし → PASS**。dwell gateがtop-band進入の通過(素通り)を正しく拒否（18進入→16展開、40進入→30展開等の比で機能確認）。
5. **Q3-A（cache更新）: p50 29.5〜80.4ms / p95 179.2〜246.9ms → PASS**（SLO 300ms）。フォーカス配線実装後、実データで2 run連続合格。
6. **Q1（常駐）: 未計測のまま**。全runが数分以内でheartbeat（60s周期）が0〜1件。長時間ソーク（≥15分、望ましくは24h）は**Phase 1に持ち越し**（M2完了条件の24h連続稼働で回収する。ユーザー判断で後回し決定）。

**実機で発見し修正した問題（コミット順）**:
- `e95f09b` パネル位置未指定→画面中央に出現（Tauriデフォルト）。プライマリ上端中央に固定。
- `e95f09b` Q2外れ値298179ms: T5 revive（Collapsing→Expanded）がExpandCommitなしにpaintedを発火し、古いt0と誤ペア。painted側でcommitをswap(0)消費し1commit=1paintに。
- `1e34181` Q2外れ値1658ms（暖機）: クロックオフセット校正前のpaintがt0を残し後続と誤ペア。commit消費をoffsetゲート前に移動。
- `e95f09b` Q4のauto-FP定義がダミースパイクで構造的過剰カウント（押す価値のあるボタンが無い→意図的な覗きが全部FP扱い）。判定入力を人間マークに変更、autoは参考値に格下げ、dwell gate拒否数を表示。
- `c99b098` コンテキスト未配線（「読めてない」報告）→ フォーカス監視スレッド+AX walk+contextイベント+metric.cache_update実装。
- `83b8f05` ブラウザのAXツリー遅延構築で切替直後のwalkが空→同一pidのまま500ms後に1回だけ再試行。
- `4ce369e` **タブ切替が検知されない**（pid不変のため）→ 約2秒周期の内容再walk+digestデデュープ追加。イベント駆動AX購読（AXFocusedUIElementChanged等）への置換はPhase 1で実施。

**既知の残課題（Phase 1へ）**:
- Q1長時間ソーク未実施（上記6）。anim_timeout 2〜3件/runの原因確認もソーク時に。
- 外部ディスプレイ: パネルはプライマリのみ（設計通りだが製品ではFR-NU §6.1.2で対応必須）。
- Q2暖機直後の1サンプル外れ値（4607ms、再ビルド直後のrunでn=3中1件）: コールドスタートの初回描画と推定。長時間runで再発監視。
- 2秒ポーリングはスパイク限定の暫定。製品はAX通知駆動+500msデバウンス（FR-CAP-02）。

**Phase 0 総括**: 4つの問いのうちQ2/Q3/Q4は物理ノッチ実機でSLO合格を確認。Q1のみ長時間データ未取得だが、150秒×7回(CI)+実機複数runでクラッシュ0・自己修復0。**ユーザー判断: ノッチ方式でGo、Phase 1本実装へ**（2026-07-20）。
