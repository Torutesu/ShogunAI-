# PR #90 オンボーディング再構築設計 — 現 main への移植（issue #6）

- 文書ID: `docs/fixes/2026-07-30-onboarding-rebuild-design.md`
- ステータス: 設計確定・Phase 1+2 実装済み（本ブランチ）
- 上位文書: `CLAUDE.md`、`docs/requirements-v1.0.md`
- 参照: PR #90（`claude/ui-commerce-onboarding-za0ns8`、クローズ対象）、PR #76（#46 AXガイド、main 済み `0bcd410`）、PR #91（PostHog、main 済み）、レビュー記録 2026-07-30

---

## 1. 背景と前提

PR #90 のブランチは main と**共通祖先を持たない**独立履歴（259コミット）であり、そのままマージすると会議ノート一式（migrations V7–V11）・音声スタック・analytics（#61/#91）・Full UI・CLAUDE.md の記録済み意思決定を巻き戻す。よってブランチはマージせず、**オンボーディング実装（末尾3コミット）の内容だけを現 main に再構築**する。

対象コミット（PR #90 側）:

| コミット | 内容 | 本移植での扱い |
|---|---|---|
| `22cc779` | 6ステップ初回フロー（frontend）+ `docs/onboarding-design.md` + preview 基盤 | **フロー本体を移植**（後述の適応あり）。preview 基盤（Stage/bridge/build-preview.mjs）は main に存在しないため対象外 |
| `fc40b3b` | ショートカット文法（modifier+gesture） | **対象外**。オンボーディングと独立した別機能。main のショートカット実装（`mod shortcuts` + `comboChips`）と競合するため、必要なら別 issue |
| `814b1a8` | Rust IPC（onboarding state / AX / exclusions / draft-stop / MCP対称） | **大部分を移植**。draft-stop だけは main 既存の `ComposioPolicy` に一本化（§4.3） |

PR #90 で「良い」と判定された性質は全て保存する:

1. オンボーディング状態は **Rust 所有**（不変条件1）、`app_data/onboarding.json`
2. トライアルは**完了時に一度だけ刻印**、再走で再スタートしない（`prev.or(...)`）
3. AX 確認は**非プロンプト API のポーリング**、プロンプトはボタン一回のみ
4. draft-stop は**既定 ON・曖昧入力は ON にフェイルセーフ**（不変条件4）
5. プランは **Standard / Pro のみ**（Free なし、2026-07-26 オーナー判断）
6. 文言は strings カタログ経由・英語・絵文字は ⚔ のみ
7. 除外カテゴリは**生きたポリシー**から数える（ハードコード禁止）
8. MCP/CLI 対称（不変条件6）: `device.onboarding.get`

## 2. 移植インベントリ（ファイル別）

### 2.1 Rust（Phase 1）

| PR #90 のファイル | main での行き先 | 判断 |
|---|---|---|
| `lib.rs` 内 `mod onboarding`（state machine + commands + tests） | `apps/desktop/src-tauri/src/onboarding.rs` に統合 | **移植**。純粋ロジック（`apply` / トライアル刻印）は cfg なしの `state` サブモジュールに置き、Linux でもテストが走る形にする。#76 の `Disposition {completed, skipped}` は新 `OnboardingState` に**吸収**（§4.1 の移行規則） |
| `axcache.rs` の `ax_trusted_silent` / `request_ax_permission` | — | **不要**。`ax_trusted_silent` は #76 で main 済み。`request_ax_permission` は main の `open_accessibility_settings`（プロンプト発火 + ペイン deep-link）と同一機能なのでそれを使う |
| `lib.rs` の `ax_permission` コマンド | — | **不要**。main の `accessibility_status` が同一（非プロンプト bool） |
| `exclusions.rs` の `exclusion_categories` コマンド | 同パス | **移植**（そのまま適用可能） |
| `shogun-core/capture/exclusion.rs` の `category_counts()` + tests | 同パス | **移植**。既存 `default_categories()`（英語ラベル直書き、fullui.rs が使用）は残すが、新規 UI は id ベースの `category_counts()` を使う。`default_categories` の id 化は別途（fullui 側の文言分離と併せて） |
| `connectors.rs` の `get_draft_stop` / `set_draft_stop`（`app_data/draft-stop` マーカーファイル） | — | **移植しない（設計変更）**。main には #90 ブランチに無かった `ComposioPolicy`（`composio.json`: draft_stop + consent + user_id、`set_composio_policy` コマンド、consent 無しで draft_stop OFF を拒否するバリデーション付き）が既にある。マーカーファイルを足すと**同じ設定の保存先が2つ**になり、approvals の送信ゲートと矛盾し得る。オンボーディングの Drafts-only トグルは `composio_settings` / `set_composio_policy` を読む・書く（§4.3） |
| `lib.rs` の `build_runtime(draft_stop_enabled(app))` seed | `setup_macos` | **同等の修正を適用**: 起動時の `build_runtime` を `true` 直書きから `load_composio_policy(app).draft_stop` に変更（永続値でシード） |
| MCP/CLI 対称（`memory_api.rs` / `mcp.rs` / `rest.rs` / `db_backend.rs` / cli 4ファイル） | 同パス | **移植**（main の該当ファイルは同系統の形のままで、機械的に適用可能）。DB-backed face が空を返す（捏造しない）注記ごと移す |

### 2.2 Frontend（Phase 2）

| PR #90 のファイル | main での行き先 | 判断 |
|---|---|---|
| `src/onboarding/ipc.ts` | 同パス（新規） | **移植・適応**。コマンド名を main の実在コマンドに合わせる: `ax_permission`→`accessibility_status`、`request_ax_permission`→`open_accessibility_settings`、draft-stop → `composio_settings`/`set_composio_policy` |
| `src/onboarding/Onboarding.tsx`（6ステップ） | 同パス（#76 のAXガイドを**置換**） | **移植・適応**。ホスト面の決定は §4.2。`Icon`/`TriggerChips`/`parseTrigger` 等 branch 専用モジュールへの依存はインライン SVG と main の `comboChips` 相当に置換 |
| `src/connections.tsx`（ConnectionsList 共有化） | 同パス（新規） | **移植**。main の `App.tsx` 内 `ConnectionsSection` から行 UI を抽出して共有（Settings とオンボーディングで二重化させない、という #90 の判断を踏襲） |
| `src/strings.ts` の `ob*` + `count()` | 同パス | **移植**（ほぼそのまま。`obReadyShortcut` の実バインドは main の `get_shortcuts` から読む） |
| `src/styles.css` の `.ob` 系 330 行 | `src/onboarding/onboarding.css` | **書き直し**。branch の CSS は branch 専用のデザイントークン（`--s-4`/`--ink-2`/`--solid` 等、be17c3b で再構築されたもの）に依存し main に存在しない。main の既存トークン（`--ink`/`--muted`/`--line`/`--accent`/`--fs-*`/`--r-*`）で同じ構造を再実装する |
| `src/tauri.ts`（`IN_TAURI`/`ask`） | `ipc.ts` 内に最小限を内包 | 部分移植（preview 基盤に紐づく部分は落とす） |
| `src/preview/*` / `scripts/build-preview.mjs` | — | **対象外**（main にブラウザ preview 基盤が無い） |
| `docs/onboarding-design.md` | `docs/onboarding-design.md` | **移植**（§3.1「パネル内で走らせる」を §4.2 の決定で上書き注記） |

## 3. ステップ構成（決定）

`welcome → reads → permission → plan → connect → ready` の6ステップ（PR #90 のまま）。

issue 記載の5段（Welcome → Accessibility → Bring your own key → Connect your work → You're set）は本構成の部分集合であり、対応は:

| issue の段 | 実装ステップ |
|---|---|
| Welcome | `welcome` |
| （プライバシー契約 — #90 が追加。権限より前に理由を渡すため） | `reads` |
| Accessibility | `permission` |
| Bring your own key | `plan`（Pro 選択時のみ BYOK 入力を出す。v1 は Anthropic のみ、CLAUDE.md） |
| Connect your work | `connect` |
| You're set | `ready` |

`docs/specs/issue-82-onboarding-connections-ui-spec.md` は #90 ブランチにも main にも**存在しない**（ブランチの docs を `git ls-tree` で確認済み）。存在しない仕様への整合は取れないため、issue #82 が別途仕様化されたら `connect` ステップと Settings の Connections を同時に照合すること。

## 4. 統合の決定事項

### 4.1 (a) #76 AXガイドとの関係 — 「同じ窓・同じ資産で、フローに拡張」

**決定: #76 のオンボーディング webview ウィンドウ（`onboarding.html`）をホスト面として維持し、その中身を6ステップフローに置換する。#76 の AX ガイド画面（2カラムの do/wont、番号付き手順、トラブルシュート、付与瞬間の push 通知）は `permission` ステップの中身としてそのまま再利用する。**

- PR #90 は「ノッチから降りるパネル内」でフローを走らせた。その論拠（居場所を最初に教える）は理解できるが、main では:
  - #76 のウィンドウが**実機検証済み**（Accessory app での前面化、全 Space フロート、watcher、`SHOGUN_FORCE_ONBOARDING` QA ハッチ）
  - パネル内ホストは `App.tsx`（2,078行）への大規模改修と `set_panel_size` の振り付けを要し、リスクがフェーズ規模に見合わない
  - 6ステップの情報量（プラン比較・接続リスト）は 640px カードの方が読ませられる
- したがって「パネル内フロー」への回帰は、Phase 0 のノッチ検証が Go になった後の別 issue とする（本文書がその判断記録）。
- Rust 側の #76 資産の扱い: `accessibility_status` / `open_accessibility_settings` / watcher（`accessibility-changed` push）/ ウィンドウビルダー / QA ハッチは**全て残す**。`onboarding_get` / `onboarding_finish`（旧 Disposition 用）は新 `onboarding_state` / `set_onboarding_state` に**置換**（フロントの唯一の読者を同時に置換するので互換面は不要）。
- **旧 Disposition の移行規則**: `onboarding.json` が旧形式 `{completed, skipped}` の場合、`completed || skipped` → 新形式 `{completed: true, step: "ready", plan: null, trial_started_at: null}` として読む。理由: 旧形式のユーザーは既にアプリを使っており、新フローに閉じ込め直さないのが安全側。`trial_started_at` は捏造しない（次に completed を書く書き込みで刻まれる。§6 未決-1 参照）。
- 表示条件の変更: 旧「AX 未付与 かつ 未完了/未スキップ」→ 新「**未完了**（AX 付与済みでもフローは出す。permission ステップは付与済みなら緑カード＋スキップ非表示で素通りできる）」。`SHOGUN_FORCE_ONBOARDING=1` は維持。

### 4.2 (b) analytics — #91 の PostHog アダプタ経由、opt_out 尊重

**決定: #76 の `onboarding_event`（eprintln のみ）を、main の `analytics::Analytics`（#91）への配線に拡張する。**

- イベント名は Rust 側の**固定 allowlist**（`onboarding_step_viewed` の step prop + `onboarding_completed` 等）にマップし、webview からの任意文字列をそのままイベント名にしない。
- 送信は `Analytics::capture` 経由 → `AnalyticsHandle` の `opt_out` AtomicBool を必ず通る（#91 のゲート。`analytics_set_opt_out` が即時反映）。`SHOGUN_POSTHOG_KEY` 未設定なら従来どおり no-op。
- 内容（キャプチャテキスト・キー・アプリ名）は一切運ばない。プロパティは step id のみ。
- ローカルの `eprintln!` ファネルログは残す（実機デバッグ用、#76 の資産）。
- `ready` ステップに #91 の `AnalyticsToggle`（opt-out トグル）を置き、計測についてオンボーディング内で開示・制御できるようにする（#76 の success 画面の配置を踏襲）。

### 4.3 (c) プラン/トライアル — Rust コア所有、実ゲーティングは既知のギャップ

- `plan` は**意思表明の記録のみ**（キーを訊くかの分岐にだけ使う）。`trial_started_at` は完了時一度だけ刻印。どちらも Rust 側 `onboarding.json`（非秘匿）。
- **既知のギャップ（変わらず）: 実際のプラン権利強制（entitlement enforcement）はコードベースのどこにも存在しない。** `fullui.rs` / `analytics.rs` の "trial" も固定値。課金（Stripe）実装時に「Rust コア側の判定 + `trial_started_at` からの 7 日計算 + 期限後の挙動」を実装する **follow-up issue を必ず切る**こと。webview 側ゲーティングだけの実装は不変条件違反なので不可。
- **draft-stop の一本化（#90 からの設計変更）**: 保存先は main 既存の `composio.json`（`ComposioPolicy.draft_stop`、既定 ON）。オンボーディングの Drafts-only トグルは `composio_settings` で読み、`set_composio_policy` で書く。consent 未取得のまま OFF にしようとするとコマンドがエラーを返す仕様（main 既存）はそのまま活き、**フロント側はエラー時に ON へ戻す**（フェイルセーフ、不変条件4）。オンボーディングでは Composio の 3 開示同意フローまでは踏み込まない（Settings の `ComposioSection` の仕事）。

### 4.4 (d) MCP/CLI 対称

- `Tool::DeviceOnboardingGet`（wire `device.onboarding.get`、Read）を memory_api / MCP tools/list / REST `GET /v1/device/onboarding` / CLI `shogun onboarding` に追加（#90 の 814a8 のまま移植）。
- DB-backed face（`DbBackend`）は空を返す（onboarding 状態は desktop の app-settings にあり、core DB は持たない — 捏造しない）。実データ供給は共有ストア化の follow-up issue。

## 5. フェーズ別 PR 計画

| Phase | 内容 | 状態 |
|---|---|---|
| **1: Rust state + IPC** | `onboarding.rs` の state machine（純粋 `state` モジュール + 移行規則 + tests）、`exclusion_categories` + `category_counts`、`set_onboarding_state` の完了時ウィンドウクローズ、起動時 draft-stop シード修正、MCP/CLI 対称、コマンド登録 | 本ブランチで実装済み |
| **2: frontend フロー** | `ipc.ts` / `Onboarding.tsx`（6ステップ、permission に #76 ガイド内包）/ `connections.tsx` 抽出 / `strings.ts` / `onboarding.css` | 本ブランチで実装済み |
| **3: analytics / polish** | `onboarding_event` の PostHog 配線（allowlist）は Phase 1 に同梱済み。残: Settings からの「Re-run setup」導線（`set_onboarding_state(completed:false)` + ウィンドウ再表示）、ファネルの p50 計測（DL→最初の答え）、`default_categories`（fullui）の id 化と文言分離、entitlement follow-up issue 起票 | **未実装（TODO）** |

### Phase 3 に残した TODO（半端に入れないための明示）

1. **Settings の「Re-run setup」ボタン** — `set_onboarding_state(step:"welcome", completed:false)` + `build_onboarding_window`。トライアルは刻印済みなので再走しても安全（テスト済み性質）。
2. **entitlement enforcement の follow-up issue 起票**（§4.3）。
3. **`DeviceOnboardingGet` の実データ供給**（共有ストア化。現状は空 = 契約のみ）。
4. **fullui.rs の `default_categories` を id ベースへ**（文言分離規約との整合）。
5. **中断離脱（permission で閉じたまま戻らない）の検知**とトライアル未開始問題（#90 設計 doc §7 と同じ未決）。

## 6. オーナーが決める必要がある事項

1. **旧 Disposition ユーザーのトライアル起点**: §4.1 の移行規則では、#76 時代に AX ガイドを完了/スキップした既存デバイスは新フローを見ず、`trial_started_at` が刻まれないまま使い続ける。課金実装時に「刻印なし = 初回課金チェック時に刻む」等の規則が要る（現状は実害なし: enforcement 自体が無い）。
2. **AX 付与済みの新規デバイスにもフルフローを出す**（§4.1 の表示条件変更）で良いか。#76 の挙動（付与済みなら何も出さない）から変わる。
3. **オンボーディングの `connect` ステップに Composio 3 開示同意まで含めるか**（現状: 含めない。Drafts-only トグルの OFF は consent が無い限り失敗し ON に戻る。同意は Settings で）。Gmail 読み取り同期自体も consent が無ければ走らない（main 既存のゲート）ので、オンボーディングで Connect を押しても同意までは読み取りが始まらない — この体験で良いか。
4. **パネル内フロー（#90 原案）への将来回帰**の要否（§4.2）。
5. **`shogun onboarding`（CLI）/ REST が当面空を返す**ことの許容（契約のみ先行、#90 と同じ判断）。

## 7. 検証（Linux 環境でできた範囲）

- `pnpm --filter desktop typecheck` / `build`（webview 3 エントリのビルド）
- `cargo test -p shogun-core -p shogun-mcp -p shogun-cli`（category_counts / MCP 対称 / CLI 面）
- `cargo check -p shogun-desktop-spike`（macOS 専用モジュールは cfg で落ちるが、純粋 `onboarding::state` のテストは Linux で走る）
- macOS 実機でのみ検証可能: ウィンドウ表示・AX プロンプト・watcher push・完了時クローズ・実機 SLO。**実機確認前にマージしない**こと（PR 本文に明記する）。
