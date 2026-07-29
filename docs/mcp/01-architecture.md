# MCP 接続レイヤー — アーキテクチャ設計

> Issue #59 のアウトプット第1弾（`00-overview.md` とセット）。
> Issue が求めた「MCP ハブ」の概念設計を、**実在するコード**と対応づけて記述する。
> 設計の正本は `docs/requirements-v1.0.md` §6.9〜6.10。実装計画は `docs/connector-adapter-plan.md`。

## 1. 3層モデル

Issue #59 が定義した「ShogunAI クライアント / MCP ハブ / MCP サーバー群」の3層は、現行コードでは次のとおり実現されている。

```
┌────────────────────────────────────────────────────────┐
│ ① ShogunAI クライアント（apps/desktop）                   │
│    - 設定ウィンドウ: Connections / Approvals パネル        │
│    - Tauri コマンド: connectors.rs / approvals.rs        │
│    - マイクロコピー（「カレンダーから予定を取得中」等）        │
└──────────────────────┬─────────────────────────────────┘
                       │
┌──────────────────────▼─────────────────────────────────┐
│ ② MCP ハブ相当（crates/shogun-mcp + shogun-integrations）│
│    - レジストリ: Service enum + endpoints.rs + scope.rs  │
│    - 接続状態管理: connection.rs FSM + keychain + runtime│
│    - ルーティング: send_bridge.rs + transport.rs         │
│    - 認可: service_gate.rs（Wave × 接続状態 × draft-stop）│
└───────────┬──────────────────────────┬─────────────────┘
            │ 第1層（公式 MCP 直結）      │ 第2層（Composio）
┌───────────▼──────────────┐  ┌────────▼─────────────────┐
│ ③ 公式リモート MCP サーバー群 │  │ Composio API             │
│    Gmail / Calendar / Drive│  │ （現状 Gmail 送信のみ、     │
│    （Wave 2/3: Slack 等）   │  │   オプトイン必須）          │
└──────────────────────────┘  └──────────────────────────┘
```

「MCP ハブ」という独立プロセス・サーバーは**存在しない**。ハブの3責務はローカルアプリ内の2クレートに分散実装されている。これは意図的な設計（FR-INT-01: 中間サーバー無し）で、ユーザーデータが自社サーバーを経由しないことを保証する。

## 2. MCP ハブの3責務と実装の対応

Issue #59 が挙げたハブの責務は、すべて実装済みの対応物がある。

### 2-1. レジストリ（どの MCP が利用可能か）

| 役割 | 実装 |
|---|---|
| サービスの一覧と Wave 割り当て | `crates/shogun-mcp/src/scope.rs` — `Service` enum（7サービス）+ `Service::wave()` |
| 各サービスの MCP URL と OAuth スコープ | `crates/shogun-integrations/src/endpoints.rs` |
| 操作名 → 各 MCP のツール名 | `crates/shogun-integrations/src/toolmap.rs`（**ツール名は暫定**、実サーバーの `tools/list` で突合が必要） |
| 権限表（サービス × 操作 × 承認レベル） | `scope.rs` のスコープ表。表にない操作は拒否。外部送信は必ず L3 |

新サービス追加の拡張ポイント：`Service` enum → `endpoints.rs` → `toolmap.rs` → `result.rs`（正規化）の4点セット。

### 2-2. ユーザー単位の接続状態管理

| 役割 | 実装 |
|---|---|
| 接続状態機械（connected / amber / disconnected） | `crates/shogun-mcp/src/connection.rs` |
| OAuth 2.1 + PKCE（純ロジック） | `crates/shogun-integrations/src/oauth.rs`、ループバックフローは `oauth_flow.rs`（feature `live`） |
| トークン保管・自動リフレッシュ | `token.rs`（`TokenStore` / `TokenManager`）+ `keychain.rs`（macOS Keychain のみ、FR-INT-02） |
| 同期スケジューラ（15分ポーリング + オンデマンド） | `runtime.rs`（`ConnectorRuntime`、FR-INT-04） |
| Composio のオプトイン状態 | consent_acknowledged / draft_stop / user_id（JSON 設定）+ API キー（Keychain） |

状態遷移：`disconnected → (OAuth 認可) → connected → (トークン失効等) → amber → (再認可) → connected`。amber は「壊れた」ではなく「再認可すれば戻る」状態としてユーザーに提示する。

### 2-3. ルーティング（LLM の呼び出しを実 MCP サーバーへ）

| 役割 | 実装 |
|---|---|
| 経路振り分け（email 送信 → 第2層、その他 → 第1層） | `crates/shogun-integrations/src/send_bridge.rs` |
| 第1層トランスポート | `transport.rs` の `RemoteMcpTransport`（JSON-RPC 2.0、**ライブ接続未検証**） |
| 第2層トランスポート | `shogun-core` の `HttpComposioApi`（`api/v3/tools/execute/{tool}`） |
| 認可の合成判断 | `service_gate.rs` — Wave 解放 × 接続状態 × draft-stop を合成。未リリース Wave は自動拒否 |
| レスポンス正規化 | `result.rs` — MCP レスポンス → `FetchedItem`（tolerant パース、実レスポンスでの確認が必要） |
| MCP サーバー（Memory API 提供側） | `mcp.rs` — JSON-RPC 2.0、13 ツール。MCP / CLI / REST の3面で対称（`dispatch.rs`） |

## 3. データフロー

### 3-1. MCP が1個の場合（例：Google Calendar のみ接続）

**読み取り同期（バックグラウンド）：**

```
ConnectorRuntime（15分周期）
  → service_gate: Wave 解放済み？ connected？ → OK
  → RemoteMcpTransport → 公式 Calendar MCP（tools/call）
  → result.rs で FetchedItem に正規化
  → event_log に source="gcal" タグ付きで保存
```

取り込まれたデータは AX キャプチャ等と同じ event_log に入る（ソース対称、FR-INT-05）。以後の検索・Context Fusion は出所を区別できる。

**会話中のオンデマンド取得：**

```
ユーザー「明日の予定は？」
  → LLM がカレンダー参照を判断（ツール呼び出し）
  → ハブ層が gate 判定 → 第1層で取得 → 正規化して LLM に返す
  → クライアントは「カレンダーから予定を取得中」を表示
```

### 3-2. MCP が2〜3個の場合（Wave 1 フル接続）

- 各サービスは**独立した接続状態**を持つ。Calendar が connected で Gmail が amber、は普通に起こる
- 同期は ConnectorRuntime がサービスごとにスケジュール。1つの失敗が他を止めない
- LLM から見える差分は「利用可能ツールが増える」こと。組み合わせ判断（例：会議準備 → Calendar で予定 → Drive で資料 → Gmail で関連スレッド）はモデル側の呼び出し優先度設計で扱う（→ 本書 §5、詳細はリリース3で拡充）

### 3-3. 書き込み・送信（全サービス共通の安全経路)

```
LLM が送信/書き込みを提案
  → scope.rs: この操作は表にあるか？ 承認レベルは？（外部送信は必ず L3）
  → L3 承認キューへ（approvals.rs）
  → ユーザーが設定ウィンドウの Approvals パネルで全文確認 → 確認 or 拒否
  → 確認後: send_bridge がルーティング
      - email 送信 → 第2層（Composio、オプトイン + draft-stop 考慮）
      - その他     → 第1層（公式 MCP）
  → トレーサビリティ記録（本文なし、ダイジェスト + byte 数。第三者経由はバッジ明示）
```

**人間の承認を通らない外部送信は構造上存在しない。**これが本アーキテクチャの最重要不変条件。

## 4. 第1層 / 第2層をなぜ分けたまま保つか

| | 第1層（公式 MCP 直結） | 第2層（Composio） |
|---|---|---|
| 経路 | ローカルアプリ → 公式サーバー | ローカルアプリ → Composio → サービス |
| 第三者へのデータ通過 | なし | **あり**（だからオプトイン + UI 明示が必須） |
| 用途 | 読み取り同期・書き込みの基本経路 | 公式 MCP に無い能力の補完（現状 Gmail 送信のみ） |
| 認可 | ユーザー直接 OAuth、トークンは Keychain | Composio API キー（Keychain）+ 同意フラグ |

第2層は「便利だから広げる」対象ではなく、「公式に能力が無い間の橋」。公式 MCP が送信をサポートしたら第1層へ寄せるのが方針。ドキュメント・UI では常に両者を区別して見せる。

## 5. モデル（Claude）側から見た MCP 利用

> 現状、LLM へのツール定義・システムプロンプトへの MCP 一覧の結線は**未実装**（`shogun-core/src/llm/` はクライアントまで、`shogun-agents` は L1/L2 実行エンジンまで）。本節は「結線実装時にそのまま組み込めるテキスト設計」。

### 5-1. システムプロンプトに渡す「接続済みサービス」ブロック

原則：**接続済み（connected）のサービスだけを載せる**。未接続・amber・未リリース Wave のサービスは一覧に出さない — モデルに「繋がっていないものを呼ばせない」が唯一のシンプルな防線。接続状態が変わったら次ターンからブロックを再生成する。

テンプレート（英語、実際の生成はハブ層の接続状態から機械生成）：

```
## Connected services
You can pull context from these connected services:
- calendar: the user's Google Calendar. Events, availability, upcoming meetings. Read-only.
- mail: the user's Gmail. Threads and messages. Read-only; you may draft replies, but sending always requires the user's explicit approval.
- drive: the user's Google Drive. Documents and files. Read-only.

Priorities:
- Questions about schedule, meetings, or availability → check calendar first.
- Questions about conversations, requests, or follow-ups → check mail first.
- Questions about documents or materials → check drive first.
- For meeting prep, combine: calendar (the event) → drive (related files) → mail (related threads).

If a task would clearly benefit from a service that is not listed here, say so briefly instead of guessing (the user can connect it in Settings).
```

設計上のポイント：

- **サービスは操作名でなく役割で説明する**（「calendar = 予定・空き時間」）。個々のツール名の羅列はツール定義側（5-2）の仕事
- **書き込み・送信の制約はプロンプトにも書く**（"sending always requires approval"）。ただし防御の本体はプロンプトではなく `scope.rs` + L3 ゲート。プロンプトは期待値合わせ、ゲートが保証
- **未接続への言及ルール**を1行入れる：「繋げばもっとできる」をモデルが自然に案内できるようにする（`02-user-guide.md` §4 のマイクロコピーと対応）

### 5-2. LLM ツール定義との対応

Anthropic API の `tools` 配列に載せる単位は、**MCP サーバーの生ツールではなくハブ層の操作名**（`toolmap.rs` の左側、例：`list_calendar_events` / `search_mail` / `search_drive_files`）。理由：

1. 実 MCP のツール名は暫定で、サーバー側の都合で変わりうる（`tools/list` 突合待ち）。ハブの操作名を安定インターフェースにする
2. `scope.rs` の権限表が操作名単位なので、ツール呼び出し → 認可判定が1対1で素直につながる
3. モデルに見せる面を絞れる（表に無い操作はそもそもツール定義に載らない = 呼べない）

呼び出しの流れ（結線後）：

```
Claude が tool_use（例: list_calendar_events）
  → service_gate: Wave 解放済み？ connected？ 権限表にある操作？
  → OK なら toolmap で実 MCP ツール名に変換 → RemoteMcpTransport → 公式 MCP
  → result.rs で正規化 → tool_result として Claude に返す
  → 読み取り以外（書き込み・送信）は即実行せず L1/L2/L3 エンジン（shogun-agents）に流す
```

### 5-3. 呼び出し優先度の設計方針

- 優先度は**プロンプトの指針**であって、コード側で強制しない（会議準備で Drive から始めても壊れない）。強制するのは認可（gate）だけ
- 「まず1サービスで答え、足りなければ広げる」を基本にする。3サービス全部を毎回叩くとレイテンシとトークンが無駄になる
- 組み合わせ（会議準備 = calendar → drive → mail）は代表パターンとしてプロンプトに1例だけ載せる。網羅しない — パターン列挙はモデルを硬直させる

### 5-4. 結線の実装ポイント（後続 Issue の中身）

| 作業 | 場所 |
|---|---|
| `tools` 配列の生成（接続状態 → 操作名リスト → JSON Schema） | `shogun-core/src/llm/anthropic.rs` の request builder 拡張 |
| 「Connected services」ブロックの生成 | ハブ層の接続状態（`connection.rs`）から文字列生成する小関数（新設） |
| tool_use → gate → transport → tool_result のループ | `shogun-agents` に会話ループを新設 or 既存 engine と接続 |
| 読み取り以外の tool_use を L1/L2/L3 に流す | `shogun-agents/src/engine.rs`（L3 送信経路は M4 で解放予定の現状に注意） |
| ツール呼び出しのトレーサビリティ記録 | `shogun-core/src/llm/traceability.rs` の Route に MCP 経路を追加 |

## 6. 未検証事項（「未実装」ではない）

| 項目 | 状態 | 後続 Issue 候補 |
|---|---|---|
| 第1層ライブ接続（実クレデンシャル + 実 MCP + macOS 実機） | 未検証（WP-G） | ライブ検証 Issue |
| `toolmap.rs` のツール名 | 暫定（実サーバーの `tools/list` で確定待ち） | ライブ検証 Issue に含める |
| `result.rs` のフィールド名 | tolerant 実装済みだが実レスポンス未確認 | 同上 |
| Composio Gmail 送信の実エンドポイント | 型は完成、実呼び出し未検証 | 同上 |
| Wave 2/3（Slack / Notion / GitHub / Linear） | コードあり・未テスト。安定性ゲート（人間判断）待ち | Wave 2 解放 Issue |
| Connections / Approvals パネルのスタイル・最終文言 | 骨格実装済み・ラフ | UI 仕上げ Issue |
| オンボーディングでの MCP 提案フロー | 未実装（`02-user-guide.md` / `03-product-design.md` で定義 → 実装 Issue へ） | オンボーディング Issue |

## 7. Figma に起こす図（骨格メモ)

1. **アーキ図** — 本書 §1 の3層図。ボックス：クライアント（設定ウィンドウ / マイクロコピー）、ハブ相当（レジストリ / 接続状態 / ルーティング / 認可）、第1層サーバー群、第2層 Composio。矢印に「OAuth 直接」「オプトイン + 明示」のラベル
2. **接続画面ワイヤー** — Connections パネル：サービス名 / できること / アクセス範囲（読み・書き）/ 状態バッジ（Connected / Amber / Coming soon）/ Connect・Disconnect ボタン
3. **オンボーディング簡易フロー** — 説明 → 推奨1〜2個提示 → OAuth → 完了、の4ステップ（`02-user-guide.md` で定義）
