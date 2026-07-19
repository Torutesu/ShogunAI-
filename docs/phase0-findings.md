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

**オンデバイス残作業（コンパイルではなく挙動）**: 状態機械タイマー→パネル駆動、hover生サンプル→NS正規化→HoverTracker→StateMachine 統合、AXObserver/NSWorkspace 通知購読、ハーネス JSONL writer スレッド起動、検索入力時の動的 key window（#104 の level 25/101 切替）、そして S-11/S-12/S-13 の実測と4つの問いの Go/No-Go 判定。

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

**注（dev-instructions §5.7との整合）**: 同§は「spike-harness以外は使い捨て・製品基礎の作り込み禁止」。本クレートは *4つの問いに答えるために正しい状態機械とヒット領域計算が必須で、かつそれをテストする*（macOSクレートはLinuxでテスト不可）ためだけに設けた、テスト可能化のための意図的な逸脱。製品コアではなくスパイクロジック。Go判定後の本実装で再利用/破棄は自由。**大きな意思決定ではないが逸脱として明示** — 不要ならこのクレートは破棄可。

## この環境（Linux）で検証済みのこと

- `crates/spike-harness`: `cargo test`（27件green）, clippy（deny下clean）, release build成功。計測核（slo定数/クロックオフセット/JSONLスキーマ/リングバッファ/パーセンタイル/CPU差分算術/digest）を単体テスト。
- `crates/spike-core`: `cargo test`（31件green）, clippy clean。上表の挙動ロジック。
- `report` バイナリ: 合成JSONLで層別p50/p95/p99・4問別verdict出力をE2E確認。
- `apps/desktop` frontend: `tsc --noEmit` clean, `vite build` 成功。
- **未検証（要macOS）**: Tauri/AppKitビルド全般、cpu.rs のmacOSリーダー（task_info/proc_pid_rusage）、`pnpm tauri dev`、CGEventTap/AXUIElement/NSPanelアダプタ、すべての実測（S-11/S-12/S-13、4つの問い本体）。
