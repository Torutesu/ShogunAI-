# Phase 0 オンデバイス実装 runbook

- 対象読者: 物理ノッチ搭載 Mac 上で Phase 0 を仕上げ、4つの問いを実測する実装者
- 上位文書: `docs/phase0-dev-instructions.md`（進め方）/ `docs/notch-ui-prototype-spec.md`（仕様） / `docs/phase0-findings.md`（API回答台帳・修正済み事項）
- 本書の役割: **Linux+CI では完了できない「実機挙動・実測」部分だけ**を、既存コードとの接続点を明示して手順化する。コンパイル検証はCI(macos-14)で済んでいる前提。

## 0. 現状（この runbook の開始地点）

CIで確認済み（`git log` / `.github/workflows/phase0-ci.yml`）:

- 純ロジック `crates/spike-core`・`crates/spike-harness`: 67テスト green、clippy deny下clean。
- macOSシェル `apps/desktop/src-tauri`: 全アダプタ（geometry/cpu/panel/hover/axcache/display）＋統合グルー `integrate.rs`（hover→NotchEngine→panel/webview/harness、世代キャンセル式タイマー）が **Apple Silicon でコンパイル green**。
- フロントエンド: typecheck + vite build green。

**まだ実機で「動かして観測」していない。** 本書のタスク D-01〜D-08 を消化して初めて S-11/S-12/S-13 の実測に入れる。

## 1. 前提環境とセットアップ

| 項目 | 要件 |
|---|---|
| マシン | ノッチ搭載 MacBook Pro（14"/16"）+ 外部ディスプレイ1枚（S-3/S-10/擬似ノッチ検証用） |
| OS | macOS 14 Sonoma 以上 / Apple Silicon |
| ツール | Xcode Command Line Tools / rustup（stable）/ pnpm / `cargo install tauri-cli --version '^2'`（または `pnpm --filter @shogun-ai/desktop tauri` 経由） |
| 任意 | `brew install cliclick`（S-12 の自動注入用。無ければ手動200回にフォールバック、spec S-12） |

```bash
git checkout claude/shogunai-requirements-prep-nm2tf4
pnpm install
cargo build -p shogun-desktop-spike            # まず素通りビルド確認
```

**権限**: 初回起動時に `axcache::ax_trusted()` が Accessibility の許可プロンプトを出す。システム設定 > プライバシーとセキュリティ > アクセシビリティ で許可 → アプリ再起動。CGEventTap・グローバル keyDown・AXUIElement はこの単一カテゴリで動く（findings A3）。

## 2. 起動とサニティチェック

```bash
pnpm --filter @shogun-ai/desktop dev           # tauri dev（debugビルド、挙動確認用）
```

起動直後に stderr へ出る想定ログ（`lib.rs` setup）で配線が生きていることを確認する:

```
[spike] geometry: notch=true notch_w=… notch_h=… menubar_h=… screen=1512x982
[spike] accessibility trusted: true
[spike] ax snapshot: … bytes, … elements, depth …, partial=false
```

- `notch=true` かつ notch_w/h が実測値なら geometry OK（未搭載機/外部では `notch=false`＝擬似ノッチ）。
- `ax snapshot` が bytes>0 を返せば AX walk が実データを取れている（Accessibility 許可済みのとき）。
- パネル（`transparent` NSPanel）が画面上端中央に出ていること。マウスをノッチ直下へ運んで 100ms 止め、Expanded 相当のダミーUI（3ボタン＋プレビュー）が出れば hover→engine→panel の統合が生きている。

> **注意**: 計測（S-11/S-12/S-13）は必ず **release ビルド**（`cargo build -p shogun-desktop-spike --release` → `target/release/shogun-desktop-spike`）。debug の数値は無効（spec §2.2）。dev は挙動確認まで。

## 3. オンデバイスで完了させる実装タスク

> **2026-07-19 レビュー修正後の状態**: コードレビュー(10所見)のP0修正により、本書の一部は既に実装済み・CI(Apple Silicon)コンパイル済みになった。
> - **実装済み**: D-02の配線(painted→共有クロック+min-RTTオフセットで`metric.expand_latency`記録、`clock_sync`5往復)、D-07の自動判定側(`event.expand_session` + close_reason + auto-FP)、cpu_sample(5s/1分平均/RSS)、heartbeat(60s/パネル可視/AXカウンタ)、tapの無効化復旧+権限リトライ(`event.tap_status`)、ボタン系マスク+tap内早期リジェクト、タイマーの受信時世代照合、`screens[0]`基準のprimary_height、AX要素ごと100msタイムアウト、reportの空データガード、JSONL日付ローテーション、soak.shのfrontendビルド。
> - **残り(真にオンデバイス)**: D-01(目視)、D-02の較正(rAF×2スロー撮影)とR_enter進入時刻→total_perceived、D-03/D-05(フォーカス購読→`metric.cache_update`発行 — 唯一まだ発行されない必須ストリーム)、D-04(key解放+level切替)、D-06(ディスプレイ変化/ヘルスチェック)、D-07の⌃⌥⌘F手動マーク、D-08(CPU単位のActivity Monitor照合)、S-11/12/13実測。

各タスク: **目的 / 接続点（既存コードのどこに何を足すか）/ 受け入れ基準 / findings対応**。D-02・D-03 は「4つの問い」を測るために必須。D-04〜D-08 は完成度・網羅性。

### D-01 パネル常駐・展開の目視（Q1サニティ）

- 目的: NSPanel が全Space・フルスクリーン上・スリープ復帰をまたいで消えないこと（本測定は S-11）。
- 接続点: 追加実装不要。`panel::install` が設定した level 25 / collectionBehavior で挙動確認。
- 受け入れ: 手動で Space 切替・他アプリのフルスクリーン・スリープ復帰後にパネルが正位置。**level 25 でフルスクリーン上に出ない場合は 101 に切替**（`panel.rs` の `PanelLevel::Status` → `PanelLevel::PopUpMenu`）。ただし 101 は IME をブロックするので D-04 と両立させる。
- findings: 項目6 / A1。

### D-02 展開レイテンシ実測（Q2）★必須

現状: `integrate.rs` の `apply()` は `EngineOutput::ExpandCommit` で **マーカーを記録するのみ**（t0 保存なし）。webview の `painted` コマンドが未登録。以下を足して t1−t0 を `metric.expand_latency` として記録する。

- 接続点（Rust側 `integrate.rs`）:
  1. 共有の `Arc<AtomicU64>`（`last_commit_mono_ns`）と `Recorder` を用意し、`ExpandCommit` 適用時に `clock.elapsed_ns()` を保存。
  2. Tauri コマンド `painted(state, t1_perf_ms)` を登録（`lib.rs` の `.invoke_handler(tauri::generate_handler![painted])`）。ハンドラで `spike_harness::clock::OffsetEstimator` により JS `performance.now()` を Rust monotonic に補正 → `latency_ms = t1_rust_ns - last_commit_mono_ns` を計算し `Body::ExpandLatency{…}` を `record`。
  3. 起動時に `clock_sync` を5往復させ `OffsetEstimator` を確定（frontend は既に `clock_sync_ack` を返す実装がある: `App.tsx` は `painted` を rAF×2 で呼ぶ。`clock_sync` 応答コマンドを追加する）。
- 接続点（webview側 `App.tsx`）: 既に `state==="expanded"` で `notifyPainted()`（rAF×2 → `invoke("painted", {state, t1PerfMs: performance.now()})`）が実装済み。Rust 側コマンド名・引数（`t1PerfMs`→`t1_perf_ms`）を合わせるだけ。
- 較正: rAF×2 とコンポジット完了の乖離を 240fps スロー撮影で一度実測し、補正値をレポートに注記（spec §4.2.1、findings 項目9）。
- 受け入れ: S-12（下記）で `metric.expand_latency` が n≥200 記録され、p95 が出せる。
- findings: 項目9・10。

### D-03 フォーカス購読 → AXキャッシュ更新（Q3-A）★必須

現状: `axcache::snapshot(pid, budget_ms)` はコンパイル済み。**イベント駆動のトリガが未実装**（起動時に1回呼ぶだけ）。NSWorkspace 通知でフォーカス変化ごとに走らせる。

- 接続点（`display.rs` に追加）: `watch_focus(callback: impl Fn(i32) + Send + 'static)` を実装。
  - `block2` クレートを追加（`block2 = "0.6"`、macOS target）。
  - `NSWorkspace::sharedWorkspace().notificationCenter()`（**default center ではなく workspace 専用**、findings §5）に `NSWorkspaceDidActivateApplicationNotification` を `addObserverForName_object_queue_usingBlock` で購読。ブロック内で `frontmostApplication().processIdentifier()` を取り `callback(pid)`。
  - **要実機確認**: `addObserverForName_object_queue_usingBlock` の objc2-app-kit 0.3 での生成名（snake_case 綴り）と、通知名 static の feature（findings 項目 §5「中」）。返り値のオブザーバトークンは `'static` に保持して購読を維持。
- 接続点（`integrate.rs`）: `watch_focus` のコールバックで `axcache::snapshot(pid, 250)` を実行し、結果を `spike_harness::record::CacheUpdate::from_text(latency_ms, trigger, bundle_id, &text, …)` として `record`。text 本文はここで digest 化され保存されない（CLAUDE.md 規約）。`t0` は通知受信時刻、`t1` は書込完了時刻（spec §4.2.2）。
- 「押してから収集」禁止の実証: `axcache::snapshot` は **Expanded 遷移からは絶対に呼ばない**（呼び出し箇所を focus コールバックに限定）。ハーネスに AX 呼び出しカウンタを足し、Expanded 区間中の増加が focus 起因のみであることを記録（spec §3.10.3）。
- 受け入れ: S-13 で `metric.cache_update` が n≥100、p95≤300ms、partial率≤30%。
- findings: 項目11・12（fullscreen space 検知は menubar 可視性 fallback）、§5。

### D-04 検索入力時の動的 key window / IME（level 25↔101）

- 目的: Expanded 内の検索フィールドにフォーカスが要るときだけ panel を key にし、level 101 運用時の IME ブロック（tauri-nspanel #104）を回避。
- 接続点: `ipc.rs` の webview→Rust `focus_field{focused}` コマンドを登録。focused=true で `panel.set_can_become_key_window(true)` → level を 25 に下げ → `make_key_and_order_front`。閉じる/Esc で `resign` → level を元へ。
- 要実機確認: `set_can_become_key_window` の実行時トグルが効くか。効かなければ該当部のみ自前 `define_class!`（案B、findings A1・項目8）。
- 受け入れ: 前面アプリでタイピング継続中に展開→ボタン操作で文字落ち0（S-9）。検索欄で日本語入力が通り、Esc で前面アプリにキーが戻る。

### D-05 AXObserver（同一アプリ内ウィンドウ/タブ切替）

- 目的: `didActivateApplication`（D-03）だけでは拾えない、同一アプリ内のフォーカスウィンドウ/タイトル変化を検知（spec §3.10.1）。
- 接続点（`axcache.rs`）: アクティブアプリの pid に `AXObserver` を作り `kAXFocusedWindowChangedNotification` / `kAXTitleChangedNotification` を登録。タイトル変化は 500ms デバウンス。アプリ切替のたびに旧 Observer を解除し付け替える。
- 要実機確認: `accessibility-sys` 0.2 の AXObserver API 網羅とコールバックの run loop スレッド紐付け（findings「AX Rust利用」）。
- 受け入れ: ブラウザのタブ切替で cache_update が発火する。

### D-06 ディスプレイ変化・スリープ・ヘルスチェック・自己修復

- 接続点（`display.rs`）: `didChangeScreenParametersNotification`（500msデバウンス）→ 強制収束 `EngineInput::ForceCollapse` → 表示先スクリーン再決定（内蔵優先、spec §3.7.1）→ geometry 再取得 → `engine.set_regions(...)` → パネル再配置 → ヘルスチェック → `event.display_change`/`event.panel_recovered` 記録。`willSleepNotification` で強制収束、`didWake` の1000ms後にヘルスチェック（spec §3.9）。
- 受け入れ: S-3/S-4/S-5/S-10 で可視消失（2秒超）0、自己修復は24hで2回以内。

### D-07 手動マークホットキー・誤発火記録（Q4）

- 接続点: グローバル `keyDown` モニタ（Accessibility 権限で動く、findings 項目3）で ⌃⌥⌘F を検知 → 直近の Expanded セッションを `manual_false_positive` としてマーク。`event.expand_session`（opened/closed/interactions/close_reason/auto_false_positive/manual_false_positive）を `integrate.rs` の Collapsing 遷移時に確定・記録。`counter.top_band_entry` は既に `EngineOutput::TopBandEntry` で記録している。
- 受け入れ: S-6〜S-8 台本で誤発火0、フリーワーク8hで≤5回かつ率≤2%（spec §6.4）。

### D-08 CPU 計測の単位検証（Q3-B の前提）★重要

- `cpu.rs` の `read_process_cpu_ns` は `proc_pid_rusage` の値を **mach ティックとみなして mach_timebase_info で ns 変換**している（findings: osquery#7459）。**この単位が正しいか、Activity Monitor と実測突合してから Q3-B を判定する**。ズレがあれば task_info（TASK_THREAD_TIMES_INFO＋TASK_BASIC_INFO）方式へ切替。
- WebKit 補助プロセス（WebContent）の CPU は `scripts/spike-soak.sh` の外部 `ps` 突合で合算（spec §4.2.3、findings 項目11）。

## 4. 計測実行（release ビルド必須）

出力先は `~/Library/Application Support/com.syogun.shogunai/metrics/`（`integrate.rs` の `metrics_path()` と `scripts/*.sh` が一致）。

```bash
cargo build -p shogun-desktop-spike --release
# S-12 展開200回（cliclick 必要。RENTER_X/Y はノッチ中央のpx）
RENTER_X=… RENTER_Y=2 ./scripts/spike-expand-test.sh 200
# S-13 cache 100回
./scripts/spike-cache-test.sh 10
# S-11 24hソーク（日中フリーワーク8h、夜間放置＝アイドルCPU判定区間）
./scripts/spike-soak.sh
```

各シナリオ S-1〜S-14 は仕様書§5の手順・期待結果・記録項目に従う。手動シナリオは実施メモ（日時・ディスプレイ構成・目視所見）をレポート草稿に残す。

## 5. レポート生成（手集計禁止）

```bash
cargo run -p spike-harness --release --bin report -- \
  ~/Library/Application\ Support/com.syogun.shogunai/metrics/*.jsonl \
  -o docs/phase0-report-$(date +%Y%m%d).md
```

レポートは4つの問い別 verdict・層別 p50/p95/p99・誤発火一覧・記録空白（>180s）を `spike_harness::slo` の定数照合で出す（spec §4.6）。

## 6. Go/No-Go 判定（人間）

- 合格ライン（spec §6）: Q1 可視消失0・クラッシュ0・自己修復≤2/24h / Q2 `latency_expand` p95≤100ms（層別すべて）/ Q3-A p95≤300ms・partial≤30% ＋ Q3-B アイドルCPU 1分平均 95%サンプルが≤5%・最大≤8% / Q4 台本0・8h≤5かつ≤2%。
- いずれか不合格: 仕様書§6のリトライ規定（各Q 2営業日、最大1巡）を消化。dwell等を変えたら **Q2・Q4 両方を再測定**（トレードオフのため）。
- 結論・数値・変更パラメータ・残課題を `docs/phase0-report-<date>.md` に確定版として残し、CLAUDE.md の開発フェーズ更新（Phase 0→1 or §7転換）の根拠にする。判定は人間が行う（dev-instructions §1）。

## 7. 提出前チェックリスト（オンデバイス）

- [ ] release ビルドで S-11（24h）・S-12（n≥200）・S-13（n≥100）の JSONL が存在
- [ ] レポートが層別 p50/p95/p99 を含む / 誤発火分母（top_band_entry）あり
- [ ] `docs/phase0-findings.md` の ON-DEVICE / PENDING 項目に結論が入った（特に 5,8,9,11,14,15＋D-02/03/08）
- [ ] JSONL にユーザーテキスト本文が0件（`grep` で確認しコマンドをレポートに記載）
- [ ] webview 側にタイマー・状態・キャッシュ・AX呼び出しが無い（class切替＋painted＋操作転送のみ）
- [ ] CPU 単位を Activity Monitor と突合済み（D-08）
- [ ] website / packages 配下に差分なし

## 8. トラブルシュート

| 症状 | 原因/対処 |
|---|---|
| `CGEventTap create failed` ログ | Accessibility 未許可。システム設定で許可→再起動（findings 項目2/3） |
| フルスクリーン上でパネルが消える | level 25→101 へ（`panel.rs`）。ただし101はIMEブロック→D-04と両立 |
| 検索欄で日本語が入らない | level 101 の #104。入力時のみ25へ下げる（D-04） |
| `ax snapshot: 0 bytes` | Accessibility 未許可、または対象アプリが AX 非公開。別アプリで確認 |
| 透過が効かない/DMGで消える | `macOSPrivateApi`＋`macos-private-api` feature 確認。配布時透過は v1 で別途（findings 項目7） |
| 展開が速すぎ/遅すぎ・誤発火多い | `statemachine::Params` / `hover::HoverParams` / `geometry::GeometryParams` を調整（spec 付録A、値は設定構造体に集約済み） |
