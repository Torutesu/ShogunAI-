# MCP 連携 — 開発者向け実装ガイド（たたき台）

> Issue #59 のアウトプット第4弾。残作業を実装する開発者のための入口。
> 前提知識：`01-architecture.md`（3層モデルと実コード対応）。設計の正本は `docs/requirements-v1.0.md` §6.9〜6.10。実装経緯は `docs/connector-adapter-plan.md`（WP-A〜G）と `docs/connector-summary-and-live-checklist.md`。

## 1. コードの地図（どこに何があるか）

```
crates/shogun-mcp/            純ロジック（Linux でテスト可、ネットワーク依存なし）
  scope.rs                    権限表（Service × Wave × 操作）。権限の唯一の中心
  service_gate.rs             認可の合成判断（Wave × 接続状態 × draft-stop）
  connection.rs               接続状態機械（connected / amber / disconnected）
  sync.rs                     read-sync 合成（gate → transport 継ぎ目 → 正規化）
  composio.rs                 第2層同意ゲート（型でガード）
  mcp.rs / dispatch.rs / rest.rs   Memory API の3面（MCP / CLI / REST 対称）

crates/shogun-integrations/   実 I/O アダプタ層（mcp の trait を実装。依存方向: integrations → mcp）
  endpoints.rs                Service → 公式 MCP URL + OAuth scopes
  toolmap.rs                  ハブ操作名 → 実 MCP ツール名（暫定）
  result.rs                   MCP レスポンス → FetchedItem 正規化（tolerant）
  rpc.rs / transport.rs       McpRpc trait / RemoteMcpTransport + WriteExecutor
  oauth.rs / oauth_flow.rs    OAuth 2.1+PKCE 純ロジック / ループバックフロー（feature `live`）
  token.rs / keychain.rs      TokenManager（自動リフレッシュ）/ macOS Keychain
  runtime.rs                  ConnectorRuntime（15分ポーリング）
  send_bridge.rs              経路振り分け（email → 第2層、他 → 第1層）

crates/shogun-core/src/llm/   LLM クライアント
  anthropic.rs                request builder + parser（Batch / Messages）
  transport.rs                HttpTransport seam（Mock / Reqwest）
  traceability.rs             送信ダイジェスト記録（本文なし）

crates/shogun-agents/         L1/L2 実行エンジン（L3 は M4 で解放予定）

apps/desktop/src-tauri/src/
  connectors.rs               connect/disconnect/list コマンド + 15分ポーラー（macOS-only, rough）
  approvals.rs                L3 キュー操作 + Composio policy
```

**変更時の注意**：crates 側の API を変えたら `cargo check -p shogun-desktop-spike` も必ず通すこと（core/memory のテストだけでは desktop 側の破壊を検知できない）。

## 2. 残作業マップ（後続 Issue の中身）

### A. ライブ接続検証（WP-G）— 最優先

実クレデンシャル + 実 MCP + macOS 実機の3点が揃って初めて実行可能。手順は `docs/connector-summary-and-live-checklist.md` に既存。

- [ ] GCP OAuth クライアント作成（`docs/oauth-client-setup.md`、Testing mode）
- [ ] Calendar 1本で end-to-end：OAuth → Keychain 保存 → `tools/list` → 同期1周 → event_log 着地
- [ ] 実サーバーの `tools/list` と `toolmap.rs` の突合 → ツール名確定
- [ ] 実レスポンスと `result.rs` のフィールド突合（tolerant 実装の検証)
- [ ] Gmail / Drive で同様に
- [ ] Composio Gmail 送信の実エンドポイント検証（オプトイン + draft-stop + L3 経路ごと）
- [ ] トークン失効 → amber 遷移 → 再認可 → 復帰の実機確認

### B. LLM 結線（ツール定義 + 会話ループ）— 新規実装

`01-architecture.md` §5 が設計。実装ポイント：

- [ ] 接続状態 → `tools` 配列生成（操作名 + JSON Schema）。`anthropic.rs` の request builder 拡張
- [ ] 「Connected services」システムプロンプトブロックの生成関数（接続状態から機械生成）
- [ ] tool_use → `service_gate` → `toolmap` → transport → tool_result のループ（`shogun-agents` に会話ループ新設）
- [ ] 読み取り以外の tool_use を L1/L2/L3 エンジンに流す（L3 送信経路は M4 スケジュールに従う）
- [ ] `traceability.rs` の `Route` に MCP 経路を追加
- [ ] ツール呼び出しイベントを UI に流す（`02-user-guide.md` §4 のマイクロコピー用）

### C. オンボーディング / 設定 UI 仕上げ

`02-user-guide.md` §2〜§3 が仕様。実装ポイント：

- [ ] オンボーディング「Connect your work」：Drive 行追加・推奨順（Calendar → Mail → Drive）・スキップ文言
- [ ] Connections パネル拡充：アクセス範囲バッジ / 最終同期時刻 / Disconnect 確認ダイアログ / amber の Reconnect 導線
- [ ] Composio 設定の別枠化（第1層の行と混ぜない）
- [ ] 接続直後のサンプル質問提示（`03-product-design.md` §3 のアハ・モーメント）
- [ ] 会話中マイクロコピーの表示（B のイベントを購読）

## 3. 新サービス追加のレシピ（Wave 2 以降）

1. `scope.rs` — `Service` enum に追加し、`wave()` / `source_str()` / 権限表を更新（**表に書いた操作しか動かない**）
2. `endpoints.rs` — 公式 MCP URL + OAuth scopes
3. `toolmap.rs` — ハブ操作名 → 実ツール名（実サーバーの `tools/list` で確認してから）
4. `result.rs` — レスポンス正規化の対応
5. UI — Connections パネルの行は `Service` enum から自動導出されるのが理想（ハードコードしない）
6. テストは純ロジック層（shogun-mcp）で Linux 完結、実接続は checklist に1節追加

## 4. 実装時の不変条件（壊してはいけないもの）

1. **表に無い操作は拒否** — 権限追加は必ず `scope.rs` 経由。コード内の特例分岐を作らない
2. **外部送信は必ず L3 or Composio 同意ゲート** — 承認を通らない送信経路を作らない
3. **秘匿情報は Keychain のみ** — トークン・API キーを JSON 設定やログに置かない（`Secret` newtype が Debug/Display を redact する設計を尊重）
4. **依存方向は integrations → mcp** — 逆流させない。HTTP 実体は transport seam の外側だけ
5. **トレーサビリティは本文なし** — ダイジェスト + byte 数のみ。記録に生データを混ぜない
6. **`now_ms` は引数、clock 読みしない** — 既存の決定論テスト方針に従う
