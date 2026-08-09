# 課金エンタイトルメント強制（issue #97）設計記録

日付: 2026-07-31
関連: CLAUDE.md「プラン構成」（2026-07-30 境界決定）/ docs/fixes/2026-07-30-onboarding-rebuild-design.md §4.3(c)・§5 Phase 3 TODO-2 / Stripe 実装 = issue #8

これまで「実際のプラン権利強制はコードベースのどこにも存在しない」（オンボーディング設計 doc §4.3 の既知ギャップ）だったものを、**Rust コア側の純粋モジュール + 各実行面のゲート配線**として実装した。webview は表示のみ（不変条件1 / 「プラン判定はRustコア側で行う」）。

## 1. プラン境界（実装した allow/deny マトリクス）

`crates/shogun-agents/src/entitlement.rs` の `entitlements(plan, now_ms)` が返す値。トライアルは7日間フル（Pro相当）、刻印なし（オンボーディング未完了）は「トライアル未開始 = フルアクセス」（時計は完了時に始まる）。

| 権利 | Trial（未開始含む） | Standard | Pro | **TrialExpired（期限切れ・未課金）** |
|---|---|---|---|---|
| ローカルキャプチャ | ✓ | ✓ | ✓ | ✓（下記 §4） |
| ローカル検索 / メモリ閲覧 | ✓ | ✓ | ✓ | ✓（read-only view） |
| 第1層読み取り（**Gmail read via Composio 含む**） | ✓ | ✓ | ✓ | ✗ |
| Dream Cycle / Morning Brief（Select KKレーン） | ✓ | ✓ | ✓ | ✗ |
| エージェント実行（L1/L2/L3 エンジン・第1層書き込み） | ✓ | ✗ | ✓ | ✗ |
| Memory API（MCP / CLI / REST の3面） | ✓ | ✗ | ✓ | ✗ |
| Composio **送信解放**（draft-stop OFF での実送信） | ✓ | ✗ | ✓ | ✗ |

- 会議ノート（FR-MT群）は**プランでゲートしない**（全プラン。FR-MT-22 の Memory API 経由参照だけが上の Memory API 行に乗る）。
- 3開示 read 同意はプラン直交（全プラン必須）。エンタイトルメントは既存の consent / draft-stop / L3 ゲートを**置き換えず合成**する。
- 課金（`BillingState::Active`）はトライアル時計に**常に勝つ**（課金ユーザーが expired になることはない）。

## 2. モジュール配置と依存方向

**`crates/shogun-agents/src/entitlement.rs`**（新規・純粋・依存ゼロ）。

理由: 依存グラフは agents ← fusion ← mcp ← integrations ← core ← desktop（agents が最下層）。ゲートは (a) shogun-agents の実行エンジン、(b)(c)(d) shogun-mcp の各面の両方に要るため、両者から到達できて方向を反転させない場所は shogun-agents のみ（新クレートを切るほどの量ではない）。同クレートの既存責務「L1/L2/L3 permission model」とも整合する（プラン権利は permission の一段上のレイヤ）。

主要API:
- `Plan { Trial{started_at_ms: Option<u64>}, Standard, Pro }`
- `BillingState { Unknown(既定・Stripe前スタブ), Active(PaidPlan), Lapsed }` / `resolve_plan(trial_stamp_ms, billing) -> Plan`
- `entitlements(plan, now_ms) -> Entitlements`（**純粋**。now_ms は常に引数 — リポジトリ規約どおり時計を読まない）
- `TRIAL_DURATION_MS = 7日`。境界は `now - start >= 7日` で失効（`start+7d-1ms` は Trial、`start+7d` ちょうどで TrialExpired。テストで固定）
- `EntitlementSource` trait（プロバイダ seam）+ `StaticPlan`（テスト/固定用）

## 3. ゲート配線（file:line は本コミット時点）

| # | ゲート | 場所 | 挙動 |
|---|---|---|---|
| a | エージェント実行入口 | `crates/shogun-agents/src/engine.rs:119` `ExecutionEngine::submit(action, now_ms, &Entitlements)` | `!agent_execution` → `Rejected(PlanNotEntitled)`。effector にも queue にも触れない。レベルゲートより先だが、entitled でも send は従来どおり L3 拒否（不変条件4は背後で維持） |
| b | Memory API 共有ゲート | `crates/shogun-mcp/src/dispatch.rs:96` `MemoryApi::new(.., Entitlements)` / `authed()` | token 認証 → プラン。`Denied::PlanNotEntitled`（reads 含む全ハンドラ、send は enqueue 前に拒否） |
| b | REST/CLI 面 | `crates/shogun-mcp/src/rest.rs:150` `route(req, tokens, &ent)` → `Routed::PlanLocked` = **403 `plan_required`** | `/v1/status`・`/v1/metrics` は開放のまま。CLI は REST 経由なので同じゲートを通る（CLI側変更なし）。認証順は 401 が先（未認証者にプラン状態を開示しない） |
| b | RESTリスナー | `crates/shogun-mcp/src/server.rs` `AppState::with_entitlements(EntitlementProvider)`（リクエスト毎に再解決） | 実行中にトライアルが切れても次のリクエストから 403 |
| b | MCP 面（stdio） | `crates/shogun-mcp/src/mcp.rs:84` `tools_call` 冒頭 | JSON-RPC error `-32003 plan_required`（tools/list・initialize は応答継続）。プロバイダはクロージャで毎 call 解決 |
| c | Composio 送信解放 | `crates/shogun-mcp/src/composio.rs:121` `ComposioSender::send_capability(&Entitlements)` | 型グラフに第3のゲートを追加: consent → draft-stop OFF → **plan unlock** が揃わない限り `SendCapability` が存在せず `prepare_send` 到達不能 |
| c | デスクトップ実行点 | `apps/desktop/src-tauri/src/approvals.rs` `confirm_send`（冒頭で `agent_execution` チェック → `"plan_required"` 返却・アイテムは pending のまま）/ `composio_send_allowed(policy, &ent)` | L3確認・consent・draft-stop の**上に**合成。first-layer send も同じ入口で塞がる |
| d | 第1層サービスゲート | `crates/shogun-mcp/src/service_gate.rs:109`（`OpContext.plan` 追加） | Read → `first_layer_reads`（Standard以上）。書き込み/送信 → `agent_execution`、Composio送信はさらに `composio_send_unlock`。draft-stop 拒否はプラン拒否より先（ユーザー設定の安全ゲートを課金状態で隠さない） |
| d | 同期ランタイム | `crates/shogun-integrations/src/runtime.rs` `ConnectorRuntime::set_plan` / `ctx()` | desktop が tick/fetch/confirm 前に `set_plan(current(&app))` で更新（`apps/desktop/src-tauri/src/connectors.rs` poller・`fetch_on_demand`、`approvals.rs` confirm_send） |

### プロバイダ seam（プラン状態の供給）

- **desktop**: `apps/desktop/src-tauri/src/entitlement.rs` `mac::current(&AppHandle)` — onboarding.json の `trial_started_at`（unix秒 → ms）+ `BillingState::Unknown`（スタブ）。判定点ごとに再解決（インメモリ読み + 純関数なので安価）。UI 表示用に `entitlement_status` コマンド（表示専用 view）を追加。
- **スタンドアロン bin**（`shogun-api` / `shogun-mcp`）: `crates/shogun-mcp/src/plan_source.rs` `FilePlanSource` — `SHOGUN_ONBOARDING_JSON` env → macOS 既定パス（`~/Library/Application Support/com.syogun.shogunai/onboarding.json`）を**毎回再読込**。ファイル無し = トライアル未開始（フルアクセス）。identifier は tauri.conf.json と lockstep（コメントで明記）。
- **既定値の決定**: 何も分からないとき（オンボーディング未完了・ファイル無し）= `Plan::Trial{started_at_ms: None}` = フルアクセス。刻印はオンボーディング完了時に一度だけ打たれ、そこから7日（onboarding.rs の既存性質）。オンボーディングの `plan` フィールドは**意思表明のみで権利を与えない**。

## 4. 期限切れトライアルの姿勢（**2026-07-31 オーナー確定**）

**決定: 下記の現実装（ローカルのみ生存）で確定。** 代替案 (A)(B) は不採用。

CLAUDE.md「トライアル後は全員課金」（Free なし）に従い、期限切れ・未課金は **Standard 機能もロック**する。ただしアプリは破壊的に死なない:

- **動き続ける**: ローカルキャプチャ、ローカル検索/メモリの read-only 閲覧。理由: メモリは年単位で生きるデータで、後から課金しても失効期間の穴は埋め戻せない（不変条件レベルの非破壊性）。ローカルONNXなのでクラウド費用もゼロ。
- **ロック**: エージェント実行、Memory API 3面、全送信経路、第1層読み取り/同期、Dream Cycle / Morning Brief（Select KKキーを消費するもの・デバイス外に出るもの全部）。
- UI は `entitlement_status`（status = `trial_expired`）で「trial ended」状態を表示する（表示は webview、判定は Rust）。

**検討した代替案（いずれも不採用・2026-07-31）**: (A) キャプチャもロック（完全停止）— メモリに穴が空き、後から課金しても埋め戻せないため却下 / (B) 期限切れでも第1層読み取りだけ残す（Standard相当へ軟着陸）— 「トライアル後は全員課金」と矛盾するため却下。確定した実装は両者の中間（ローカルのみ生存）。

**注意（#8 前の運用リスク）**: Stripe 実装前は購入経路が無いため、刻印から7日を過ぎた実機はロック状態に落ちる。dev/QA は `SHOGUN_FORCE_ONBOARDING` とは別に、onboarding.json の刻印を消す/進めることで回避できるが、**#8 マージまで本ブランチを一般配布ビルドに載せない**こと。

## 5. Stripe（#8）統合 seam

差し替え点は2箇所だけ:
1. `apps/desktop/src-tauri/src/entitlement.rs` `mac::current` の `BillingState::Unknown` を実サブスクリプション参照に置換
2. `crates/shogun-mcp/src/plan_source.rs` `FilePlanSource::resolve` の同スタブ（または billing を含む共有ファイル/IPC に拡張）

`resolve_plan(trial_stamp, billing)` は既に billing 優先（Active はトライアル時計に勝ち、Lapsed はトライアル規則へフォールバック）。`PaidPlan::{Standard, Pro}` が Stripe の price/product にマップされる想定。旧 #46 移行デバイス（刻印なしで使い続けるケース）の起点規則は onboarding 設計 doc §6-1 の未決のまま — #8 で「初回課金チェック時に刻む」等を決める。

## 6. 意図的にゲートしないもの

- **第1層読み取り**は active プラン（Standard以上）で常に可 — Pro ゲートではない（2026-07-30 決定: Gmail read via Composio は Standard）。
- **会議ノート UI / 検知 / ASR / Recap** — 全プラン（トライアル中に価値体験させる決定）。Memory API 経由参照（FR-MT-22）だけが Memory API ゲートに乗る。
- **ローカルキャプチャ・ローカル検索** — 期限切れでも生存(§4)。
- **3開示 consent / draft-stop / L3 承認キュー** — プラン直交。エンタイトルメントはこれらの手前・背後に合成されるだけで、一切緩めない（regression: `crates/shogun-mcp/tests/invariant4.rs::no_send_path_bypasses_entitlement_or_l3_gates`）。
- **オンボーディングの plan 選択** — 意思表明のみ（BYOKキーを訊くかの分岐）。権利は与えない。

## 7. テスト（すべて Linux で走る）

- `shogun-agents entitlement`: 7日境界（`start+7d-1ms` 可 / `start+7d` 失効）、プラン別マトリクス、期限切れ姿勢（ローカルのみ生存）、未開始既定、billing override
- `shogun-agents engine`: Standard/期限切れは L1/L2/L3 とも effector 到達ゼロで `PlanNotEntitled`、active trial は L1 実行
- `shogun-mcp dispatch/rest/mcp/server`: 3面それぞれの deny（valid token + locked plan → PlanNotEntitled / 403 / -32003）、401 が 403 より先、status/metrics 開放、Pro 通過
- `shogun-mcp service_gate`: Standard = read 可・write/send 不可、期限切れ = read も不可、draft-stop 優先順
- `shogun-mcp composio`: consent + draft-stop OFF でもプラン無しなら capability 不在
- `shogun-mcp plan_source`: desktop 形式（unix秒→ms）、legacy/garbage は未開始、ファイル起点の失効
- **横断 regression** `tests/invariant4.rs::no_send_path_bypasses_entitlement_or_l3_gates`: 全送信面（engine / dispatch / service_gate 全サービス×全 send op / composio）が locked plan で送信前拒否、かつ entitled でも旧ゲート（draft-stop・L3）が生きている
