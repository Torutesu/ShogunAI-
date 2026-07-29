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

## 5. モデル（Claude）側から見た MCP 利用【骨格 — リリース3で拡充】

- システムプロンプトには「接続済みサービスの一覧 + 各用途 + 呼び出し優先度」をテキストで渡す
- 優先度ルールの例：会議・予定系の質問 → まず Calendar / ファイル・資料系 → Drive / 連絡・スレッド系 → Gmail
- 未接続サービスは一覧に載せない（モデルに「繋がっていないものを呼ばせない」）
- LLM クライアント実装は `crates/shogun-core/src/llm/`（anthropic.rs、キー分離、traceability.rs）

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
