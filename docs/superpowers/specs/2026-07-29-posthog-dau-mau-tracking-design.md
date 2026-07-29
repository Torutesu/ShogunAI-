# DAU/MAU KPI トラッキング（PostHog）設計

- Issue: [Torutesu/ShogunAI-#61](https://github.com/Torutesu/ShogunAI-/issues/61) — DAU/MAU KPIトラッキング分析
- 対象: Phase 1（イベント実装 → PostHog送信 → DAU/MAU/スティッキネスの基本Insight → ダッシュボードv1）
- ステータス: 設計合意済み（実装計画は writing-plans で作成）

## 背景と目的

ShogunAI のプロダクト成長を継続的に定量モニタリングするための単一の真実の KPI（DAU / MAU / DAU/MAU比）を、PostHog 上で自動集計・可視化する。本Issueは「アクティブ定義に必要な最小限のイベント」に絞り、ログ収集 → 集計 → ダッシュボード可視化までを PostHog 単体で完結させる。

**アクティブユーザー定義（v1）**: 対象日に以下3イベントのいずれかを1回以上発火したユーザー（OR条件）を「その日のアクティブユーザー」とする。日境界は JST 0時。

## 前提（コードベース調査結果）

- `apps/desktop` は Tauri v2 + React 18 / TypeScript。CLAUDE.md 不変条件により **データ重力は Rust バックエンド、フロントは薄い View**、**シークレットは Keychain のみで Webview を越えない**、**外部送信は Rust から単一経路**。→ PostHog 送信は **Rust バックエンドから**行う。
- 既存の外部 analytics/telemetry は無し（内部 tokio broadcast バスと SQLite event_log は外部送信していない）。
- **アカウント/ユーザーID がまだ存在しない**（Phase 0 スパイク、BYOK=Anthropicキーを Keychain 保存のみ）。→ `distinct_id` は匿名マシンUUIDで代替。
- plan の実態はコード上 `trial`(7日) / `standard` / `pro`（`onboarding.json`）。Issue の free/paid/beta 呼称ではなく実態に合わせる。

## 決定事項（ブレインストーミングでの合意）

| 論点 | 決定 |
|---|---|
| 同意モデル | **オプトアウト（既定ON＋明示開示）** |
| distinct_id | **匿名マシンUUID（今すぐ計測開始）**。v1 認証時に `$identify` でアカウントIDへマージする差し込み口のみ用意 |
| PostHog環境 | **未定・後で決める** → host/key を設定注入する環境非依存設計。ダッシュボードは環境確定後に構築 |
| スコープ | 設計spec → 実装計画まで。コード実装は計画承認後 |
| タイムゾーン | PostHog プロジェクトTZ = **Asia/Tokyo (JST)**。イベントはUTCタイムスタンプ送信、PostHog側でJST集計 |

## アーキテクチャ（アプリ側）

`shogun-core` に `analytics` モジュールを新設し、非ブロッキング送信ワーカー経由で PostHog に送る。

- **公開API**: `Analytics::capture(event: &str, props: Props)` / `Analytics::identify(...)`（identify は差し込み口のみ、マージ本体は将来）。
- **送信ワーカー**: mpsc channel + 専用スレッド/タスク。呼び出し元は即 return（UXをブロックしない）。
- **クライアント**: `posthog-rs` は使わず、**capture のみの薄い自前クライアント**。reqwest（既存 `crates/shogun-core/src/mcp_http.rs` と同じ blocking client を別スレッドで）で PostHog `/batch` エンドポイントに送信。数秒 または N件でフラッシュ（fire-and-forget）。
- **イベント供給**: 既存内部バスに乗る `IntegrationSynced` は購読して `context_updated` に変換。バスに無いもの（`app_opened` / query）は発火点で直接 `capture`。
- **no-op ガード**: `SHOGUN_POSTHOG_KEY` 未設定時、または `opt_out=true` 時は capture を破棄（開発・OSSビルドで無害）。

**却下案**: (B) 各呼び出し箇所で reqwest 直叩き — 送信失敗がUXをブロックし重複しやすい。(C) フロント posthog-js — Keychain越境不可・フロント非ネットワーク原則に反する。

## アイデンティティ（distinct_id）

- 初回起動時に **匿名 UUID v4** を生成し `app_data/analytics.json`（非シークレット）に永続化、`distinct_id` に使用。PII なし。
- `Analytics::identify()` の口だけ用意。v1 でアカウント基盤ができたら「匿名UUID → アカウントID」を `$identify` でエイリアスする（**マージ本体は本Issue対象外**）。

## イベントカタログ

### 共通プロパティ（全イベント）

| プロパティ | 内容 |
|---|---|
| `app_version` | Tauri `package_info().version` |
| `os` | プラットフォーム文字列（v1は `macos`）。精緻な OS バージョン（例 `macOS-14.5`）は Phase 2 |
| `plan` | v1 は `trial` 固定（`fullui.rs` の実態に合わせる）。`standard`/`pro`/`unknown` への分岐は課金基盤（v1認証）到来時 |
| `trial_days_remaining` | 課金基盤到来後に追加（v1スコープ外） |

`country` は v1 では送らない（PostHog GeoIP も privacy 配慮でオフ想定。将来必要なら追加）。

### イベント3種

| event | 発火点 | 固有プロパティ |
|---|---|---|
| `app_opened` | `apps/desktop/src-tauri/src/lib.rs` の setup 末尾（起動1回/launch にガード） | `cold_start`（bool） |
| `shogun_query_executed` | `apps/desktop/src-tauri/src/notch_exec.rs::run_notch_action()` の `submit()` 後（submit した時のみ発火）、および inline/full draft 経路 | `query_type`（サーフェス単位：`notch_action`/`draft_inline`/`draft_full`。細かな Action 種別は Phase 2）, `permission_level`（`L1`/`L2`/`L3`）, `outcome`（`ok`/`awaiting_confirm`/`rejected`） |
| `context_updated` | コネクタ read-sync 完了時（`shogun_core::bus::BusEvent::IntegrationSynced` 購読、または `connectors.rs` の sync 完了点） | `source`（`gmail`/`google_calendar`/`google_drive`/`slack`）, `newly_inserted`（件数） |

**タイムスタンプ**: v1 はイベントを即時送信し PostHog サーバ受信時刻を採用（クライアント側 ISO8601 タイムスタンプ付与は chrono 依存を避けるため Phase 2）。ワーカーのフラッシュ間隔は数秒なので日境界のズレは許容範囲。

DAU のアクティブ定義 = この3イベントの **OR**。

## 同意（オプトアウト＝既定ON）

- `app_data/analytics.json` に `opt_out: false` を初期化。
- 設定画面に「匿名の利用状況を送信」トグルを追加。`opt_out=true` で送信ワーカーを停止・キュー破棄。変更は即時反映。
- 初回オンボーディング/設定に明示開示文（「機能改善のため匿名の利用状況を送信します。個人データ・画面キャプチャ内容・APIキーは一切送りません」）を表示。

## 設定・シークレット

- PostHog の project write key（`phc_...`）は **公開安全な書き込み専用キー**のため **Keychain 不要**。ビルド時 env `SHOGUN_POSTHOG_KEY` / `SHOGUN_POSTHOG_HOST` で注入し、既存の env パターン（`SHOGUN_GOOGLE_*` 等）に揃える。未設定なら no-op。
- **タイムゾーン**: PostHog プロジェクトTZ = Asia/Tokyo (JST)。DAU 日境界=JST 0時。イベントは UTC タイムスタンプで送信し PostHog 側で JST 集計。ダッシュボードに TZ 明記。

## 信頼性・データ品質

- 送信ワーカー: 失敗時は最大N回リトライ → 諦めてローカルログにカウンタ記録。キュー無限肥大を防ぐ**上限リングバッファ**。
- 二重カウント対策: `app_opened` は launch あたり1回にガード。distinct_id は永続UUIDで安定。
- PostHog 側で「`app_opened` が閾値を下回ったら通知」の簡易アラート Insight（Slack連携は Phase 2）。
- データ仕様（アクティブ定義・イベント名・プロパティ・タイムゾーン・v1定義）を本specとして明文化し、定義変更時は v2 として追記し過去との比較可能性を保つ。

## PostHog側ダッシュボード v1

環境確定後に構築。ダッシュボード名 `ShogunAI – Core KPI (DAU/MAU)`。

カード:
1. Yesterday DAU（日次 unique users, 最新値）
2. This Month MAU（ローリング30日 unique users）
3. DAU/MAU %（Formula: DAU ÷ MAU）
4. 日次DAU推移（直近30–90日 折れ線）
5. 月次MAU推移（データ蓄積次第）

セグメント: `plan`, `app_version`（`country` は保留）。

アクセス: 全メンバー閲覧可、編集は PM/ファウンダー/データ担当のみ。ダッシュボードURLを定例アジェンダに固定。

## テスト

- ユニット: opt_out で送信されない / property 整形 / batch flush / `SHOGUN_POSTHOG_KEY` 未設定で no-op。
- 結合: ローカルのダミー受信 host に対し3イベントが正しい形（event名・共通/固有プロパティ・distinct_id）で飛ぶことを確認。

## スコープ境界

- **本Issue (Phase 1)**: 上記アプリ側実装 + ダッシュボード v1。
- **対象外 (Phase 2 / 別Issue)**: コホート/リテンション分析、Feature flags/Experiments（A/B）、`$identify` マージ本体、Slack/メール通知、LTV/CAC等の収益KPI、フル機能コホートUI・複雑なファネル、PostHog外の自動レポーティング。
