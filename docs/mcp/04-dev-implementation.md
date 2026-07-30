# MCP 連携 — 開発者向け実装ガイド（たたき台）

> Issue #59 のアウトプット第4弾。残作業を実装する開発者のための入口。
> 前提知識：`01-architecture.md`（3層モデルと実コード対応）。設計の正本は `docs/requirements-v1.0.md` §6.9〜6.10 と **CLAUDE.md「連携実装ルール」の 2026-07 決定（Gmail 全面 Composio 化）**。実装経緯は `docs/connector-adapter-plan.md`（WP-A〜G）と `docs/connector-summary-and-live-checklist.md`。
> ⚠️ **Gmail 経路のコードは旧設計（読み取り＝第1層、Composio＝送信のみ）のまま**。本書の残作業マップは「決定へのコード移行（§2-A0）」を最優先に置く。

## 1. コードの地図（どこに何があるか）

```
crates/shogun-mcp/            純ロジック（Linux でテスト可、ネットワーク依存なし）
  scope.rs                    権限表（Service × Wave × 操作）。権限の唯一の中心
                              （権限表は経路非依存 — Gmail の Composio 化で表は変わらない）
  service_gate.rs             認可の合成判断（Wave × 接続状態 × draft-stop）
                              ※Gmail の読み取りに Composio 同意条件を合成する拡張が必要（§2-A0）
  connection.rs               接続状態機械（connected / amber / disconnected）
  sync.rs                     read-sync 合成（gate → transport 継ぎ目 → 正規化）
                              ※Gmail の read-sync を第2層 transport へ向ける変更が必要（§2-A0）
  composio.rs                 第2層同意ゲート（3開示 = Disclosures、型でガード。draft-stop 既定 ON）
                              ※現状は送信ガードのみ。読み取りを含む全 Composio 操作の前提条件へ拡張（§2-A0）
  mcp.rs / dispatch.rs / rest.rs   Memory API の3面（MCP / CLI / REST 対称）

crates/shogun-integrations/   実 I/O アダプタ層（mcp の trait を実装。依存方向: integrations → mcp）
  endpoints.rs                Service → 公式 MCP URL + OAuth scopes
                              ⚠️ Gmail の第1層エントリ（gmailmcp.googleapis.com +
                              gmail.readonly / gmail.compose）は旧設計の残置 — 2026-07 決定により
                              Gmail は第1層エンドポイントを持たない。削除/無効化が必要（§2-A0）
  toolmap.rs                  ハブ操作名 → 実ツール名（第1層分は暫定。Gmail 分は Composio の
                              ツール slug への張り替えが必要）
  result.rs                   MCP / Composio レスポンス → FetchedItem 正規化（tolerant）
  rpc.rs / transport.rs       McpRpc trait / RemoteMcpTransport + WriteExecutor
  oauth.rs / oauth_flow.rs    OAuth 2.1+PKCE 純ロジック / ループバックフロー（feature `live`）
                              — 対象は Calendar / Drive（+Wave 2/3）。Gmail は OAuth を使わない
  token.rs / keychain.rs      TokenManager（自動リフレッシュ）/ macOS Keychain
                              （Composio API キーも Keychain。user id のみ非秘匿の設定 JSON）
  runtime.rs                  ConnectorRuntime（15分ポーリング）
                              ※Gmail は同意なしでは同期をスケジュールしない条件が必要（§2-A0）
  send_bridge.rs              経路振り分け
                              ⚠️ 現行は「email 送信 → 第2層、他 → 第1層」の操作種別ルーティング。
                              決定後は「Gmail の全操作 → 第2層」のサービス単位ルーティングへ（§2-A0）

crates/shogun-core/src/llm/   LLM クライアント
  anthropic.rs                request builder + parser（Batch / Messages）
  transport.rs                HttpTransport seam（Mock / Reqwest）
  traceability.rs             送信ダイジェスト記録（本文なし）
                              ※Composio 経由の読み取り egress も記録対象に加える（§2-A0）

crates/shogun-agents/         L1/L2 実行エンジン（L3 は M4 で解放予定）

apps/desktop/src-tauri/src/
  connectors.rs               connect/disconnect/list コマンド + 15分ポーラー（macOS-only, rough）
  approvals.rs                L3 キュー操作 + Composio policy
```

**変更時の注意**：crates 側の API を変えたら `cargo check -p shogun-desktop-spike` も必ず通すこと（core/memory のテストだけでは desktop 側の破壊を検知できない）。

## 2. 残作業マップ（後続 Issue の中身）

### A0. Gmail 経路のコード移行（2026-07 決定への追従）— ライブ検証の前提

決定：Gmail は読み取り・ドラフト・送信のすべてを Composio 経由（CLAUDE.md「連携実装ルール」）。現行コードは旧設計のままなので、以下を先に片付けないと Gmail のライブ検証（A）が旧経路を検証してしまう。

- [ ] `endpoints.rs` — Gmail の第1層エントリ（URL + `gmail.readonly` / `gmail.compose` スコープ）を削除 or 無効化（GA 時に戻せるよう、削除理由と決定参照をコメントで残す）
- [ ] `send_bridge.rs` — 振り分けを操作種別（email 送信のみ第2層）からサービス単位（Gmail の全操作 → 第2層）へ
- [ ] `composio.rs` / `service_gate.rs` — 同意ゲート（3開示）を**読み取り同期を含む全 Composio 操作**の前提条件に拡張。同意なし = Gmail の同期スケジュール自体が走らない（外部通信ゼロ）ことをテストで保証
- [ ] `sync.rs` / `runtime.rs` — Gmail の read-sync を第2層 transport（`HttpComposioApi`）へ
- [ ] `toolmap.rs` — Gmail のハブ操作名を Composio ツール slug へ張り替え（ハブ操作名 IF は不変）
- [ ] `traceability.rs` — **読み取り egress の記録**を追加（第三者境界。内容は残さずダイジェスト/フラグ + 経路 = Composio のみ）
- [ ] draft-stop 既定 ON（FR-C2-03、実装済み）の回帰テスト維持。OFF へ変更できるのは同意後のみ、を UI 層まで貫通
- [ ] 資格情報の置き場を確認：Composio API キー = Keychain、user id = 設定 JSON（非秘匿）。ログ・設定への API キー書き出しが無いこと

### A. ライブ接続検証（WP-G）— A0 完了後に最優先

実クレデンシャル + 実サービス + macOS 実機の3点が揃って初めて実行可能。手順は `docs/connector-summary-and-live-checklist.md` に既存（Gmail 節は A0 に合わせた改訂が必要）。

- [ ] GCP OAuth クライアント作成（`docs/oauth-client-setup.md`、Testing mode）— **対象は Calendar / Drive のみ**。Gmail は Google Cloud OAuth 不要
- [ ] Calendar 1本で end-to-end：OAuth → Keychain 保存 → `tools/list` → 同期1周 → event_log 着地
- [ ] 実サーバーの `tools/list` と `toolmap.rs` の突合 → ツール名確定（Calendar / Drive）
- [ ] 実レスポンスと `result.rs` のフィールド突合（tolerant 実装の検証)
- [ ] Drive で同様に
- [ ] **Gmail は Composio API で end-to-end**：API キー（Keychain）+ user id → 3開示同意 → 読み取り同期1周 → event_log 着地 → **読み取り egress のトレーサビリティ記録を確認**
- [ ] Composio Gmail のドラフト・送信の実エンドポイント検証（同意 + draft-stop 既定 ON + L3 経路ごと。draft-stop ON で送信が下書き止まりになること、同意なしで読み取りすら走らないことを実機で確認）
- [ ] トークン/資格情報失効 → amber 遷移 → 再認可 → 復帰の実機確認（OAuth 系と Composio 系の両方）

### B. LLM 結線（ツール定義 + 会話ループ）— 新規実装

`01-architecture.md` §5 が設計。実装ポイント：

- [ ] 接続状態 → `tools` 配列生成（操作名 + JSON Schema）。`anthropic.rs` の request builder 拡張
- [ ] 「Connected services」システムプロンプトブロックの生成関数（接続状態から機械生成。Gmail は Composio 同意完了 = connected の場合のみ掲載）
- [ ] tool_use → `service_gate` → `toolmap` → transport → tool_result のループ（`shogun-agents` に会話ループ新設）。Gmail 系操作は第2層 transport + 読み取り egress 記録
- [ ] 読み取り以外の tool_use を L1/L2/L3 エンジンに流す（L3 送信経路は M4 スケジュールに従う）
- [ ] `traceability.rs` の `Route` に MCP 経路（第1層）と Composio 経路（第2層、読み取り含む）を追加
- [ ] ツール呼び出しイベントを UI に流す（`02-user-guide.md` §4 のマイクロコピー用）

### C. オンボーディング / 設定 UI 仕上げ

`02-user-guide.md` §2〜§3 が仕様。実装ポイント：

- [ ] オンボーディング「Connect your work」：Drive 行追加・推奨順（Calendar → Mail → Drive）・改訂 lead 文（「すべて直結」と読める旧文言の撤去）・Mail 行の "via Composio" サブラベル・スキップ文言
- [ ] **Gmail 3開示同意シート**（`02-user-guide.md` §2-3'）：3開示の個別承諾 → `grant_consent` → Composio 接続確立。同意なしでは接続も同期も発生しないこと
- [ ] Connections パネル拡充：アクセス範囲バッジ / 最終同期時刻 / Disconnect 確認ダイアログ / amber の Reconnect 導線
- [ ] Mail 行の Composio 表示（別枠ではなく同一リスト内で視覚的に区別。"via Composio, a third-party service" 常設 + 詳細に同意状態 / draft-stop トグル / 同意取り消し）
- [ ] トレーサビリティ画面の「第三者経由」バッジ（Mail の読み取り記録を含む）
- [ ] 接続直後のサンプル質問提示（`03-product-design.md` §3 のアハ・モーメント）
- [ ] 会話中マイクロコピーの表示（B のイベントを購読）

## 3. 新サービス追加のレシピ（Wave 2 以降）

1. `scope.rs` — `Service` enum に追加し、`wave()` / `source_str()` / 権限表を更新（**表に書いた操作しか動かない**）
2. `endpoints.rs` — 公式 MCP URL + OAuth scopes
3. `toolmap.rs` — ハブ操作名 → 実ツール名（実サーバーの `tools/list` で確認してから）
4. `result.rs` — レスポンス正規化の対応
5. UI — Connections パネルの行は `Service` enum から自動導出されるのが理想（ハードコードしない）
6. テストは純ロジック層（shogun-mcp）で Linux 完結、実接続は checklist に1節追加

※ Wave 2 以降は第1層（公式 MCP 直結）が原則。第2層（Composio）に新サービスを載せるのは Gmail 同様の「公式に動く経路が無い」場合のオーナー判断のみで、その際も opt-in 3開示同意 + egress トレーサビリティ + UI 明示が必須（CLAUDE.md）。

## 4. 実装時の不変条件（壊してはいけないもの）

1. **表に無い操作は拒否** — 権限追加は必ず `scope.rs` 経由。コード内の特例分岐を作らない
2. **外部送信は必ず L3 + Composio 同意ゲート** — 承認を通らない送信経路を作らない。draft-stop は既定 ON、OFF へ変更できるのは同意後のユーザー操作のみ
3. **Composio 同意なしに Gmail の egress を発生させない** — 読み取り同期を含む。同意前は Composio への外部通信ゼロ（ゲートは型で保証、テストで回帰を防ぐ）
4. **Composio 経由の egress は読み取りも含めてトレーサビリティ記録** — 内容は残さない（ダイジェスト/フラグのみ）。UI には「第三者経由」を明示
5. **秘匿情報は Keychain のみ** — トークン・Composio API キーを JSON 設定やログに置かない（`Secret` newtype が Debug/Display を redact する設計を尊重）。user id のみ非秘匿として設定 JSON 可
6. **依存方向は integrations → mcp** — 逆流させない。HTTP 実体は transport seam の外側だけ
7. **トレーサビリティは本文なし** — ダイジェスト + byte 数のみ。記録に生データを混ぜない
8. **`now_ms` は引数、clock 読みしない** — 既存の決定論テスト方針に従う
