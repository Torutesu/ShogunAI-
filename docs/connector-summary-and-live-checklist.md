# コネクタ実装まとめ & macOSライブ検証チェックリスト

このドキュメントは、`claude/connector-design-composition-8aawqo` ブランチで実装した
コネクタ層（第1層＝公式リモートMCP、第2層＝Composio）の総括と、実機（macOS）で
「実際に接続して動かす」ための手順チェックリスト。

- 作成日: 2026-07-23
- ベース: `claude/shogunai-requirements-prep-nm2tf4`（プロダクトブランチ）の上に14コミット
- 関連: 設計 `docs/connector-adapter-plan.md` / OAuth登録 `docs/oauth-client-setup.md`

---

## 1. コミット要約（14コミット、41ファイル、~4,100行）

| コミット | 内容 |
|---|---|
| `7ff170a` docs | 実I/Oアダプタ層の実装計画 |
| `b352af2` | 第1層アダプタ土台（Google Workspace公式MCP: endpoints/toolmap/result正規化/rpcシーム/transport） |
| `e0829ba` | OAuth 2.1+PKCE（純ロジック）+ daemon配線骨格（runtime） |
| `69e9f63` | トークンライフサイクル（自動リフレッシュ）+ 接続状態一覧 + Keychain store + 接続/切断コマンド + 接続管理UI |
| `00b1d2c` refactor | KeychainストアをmacOSクロスチェック可能に分離、desktop配線の堅牢化 |
| `d603bc6` fix | CIツールチェーン(1.97)整合 + HTTPクライアントをshogun-coreに集約（FR-TR-03） |
| `6abc3da` fix | 既存clippy-1.97ブロッカー解消 → connectors.rsがmacOSでコンパイル |
| `69a27a4` | connect の async 化 + 設定ウィンドウにConnections UIをマウント |
| `3b600c3` | Wave 2 Slack（OPEN-03解決：公式MCP実在）+ OAuth登録手順doc |
| `3e599de` | 確認済み送信の実行ブリッジ（WP-F: send_bridge + FirstLayerSendTransport）+ per-vendor OAuth |
| `a8fd19f` | Composio Gmail送信の実行（WP-D: ComposioApi/HttpComposioApi/RoutedSendTransport + FR-C2-05ドラフト退避） |
| `422ca4a` | 承認キュー→送信実行の実アプリ結線（item B: exec feature/ApprovalQueue/L3確認UI） |
| `c81d8f7` | オンデマンド読み取り（item C: read_on_demand経路） |
| `aafea20` | Wave 3コネクタ定義（Notion/GitHub/Linear公式MCP endpoints/toolmap） |

## 2. 何が完成しているか（コード＋テスト）

**第1層（公式リモートMCP直結）— 6サービス全部の定義完了**
- Gmail / Google Calendar / Google Drive（Wave 1）/ Slack（Wave 2）/ Notion / GitHub / Linear（Wave 3）
- 各サービス: endpoint(URL+scope) / op→tool マッピング / スコープ許可表（read/write/L1-L3ゲート）
- 読み取り: 背景同期（15分ポーリング）+ オンデマンド取得（read_on_demand, Gmail/Drive）
- 書き込み: 承認済み(L2/L3)アクションを二重ゲートで実行（FirstLayerSendTransport）

**第2層（Composio）— v1のGmail送信のみ**
- 同意ゲート（型で強制）+ 実行体（HttpComposioApi: `POST /api/v3/tools/execute/GMAIL_SEND_EMAIL`）
- 失敗時FR-C2-05ドラフト退避（RoutedSendTransport）

**基盤**
- OAuth 2.1+PKCE ループバックフロー / トークン自動リフレッシュ / Keychain保存（invariant 7）
- 接続状態機械（connected/amber/disconnected, FR-INT-06/07）
- L3承認キュー → 実行 → トレーサビリティ記録（invariant 3 / FR-TR-03）
- 全HTTPクライアントは shogun-core に集約（egress不変条件をガードスクリプトで強制）

**アーキテクチャの分離（守られている不変条件）**
- 純ロジック（Linuxテスト可能）= shogun-mcp / shogun-integrations
- 実I/O = shogun-core（`net`）+ desktop アダプタ（macOS）
- データ重心はRustコア（invariant 1）/ 生データはトレース無しに出さない（invariant 3）

## 3. 未完了（コードだけでは進められないもの）

1. **ライブ検証**（本チェックリスト §4）— 実クレデンシャルでの実接続。未実施
2. **暫定ツール名の確定** — Slack/Notion/GitHub/Linear の toolmap は各サーバーの
   `tools/list` で確定が必要（Google分は確認済み）。`result.rs` のフィールドマッピングも実レスポンスで確認
3. **送信プロデューサ** — エージェント（Reply Drafter等）がsendを提案してキューへ。エージェント層の別機能
4. **Wave 2/3の解放** — FR-INT-03の安定性ゲート（製品判断）+ 各社OAuthアプリ登録
5. **バス購読 + item D** — Fusion/NotchがバスのIntegrationSyncedを購読する実装とセット

---

## 4. macOSライブ検証チェックリスト

実機（Apple Silicon macOS 14+）で「接続→同期→送信」を実際に動かす手順。
コードは結線済みなので、以下は主に**人間側の準備と確認**。

### 4-1. 事前準備（クレデンシャル）

- [ ] `docs/oauth-client-setup.md` に従い Google OAuth「Desktop app」クライアント作成
      （Developer Preview登録、同意画面はTestingモードで審査回避）
- [ ] 環境変数: `SHOGUN_GOOGLE_CLIENT_ID` / `SHOGUN_GOOGLE_CLIENT_SECRET`
- [ ] （Composio送信を試す場合）Composioアカウント + APIキーを Keychain へ:
      `security add-generic-password -s com.selectkk.shogun -a composio-api-key -w '<KEY>'`
- [ ] （同上）`SHOGUN_COMPOSIO_USER_ID`（Composioのconnected accountユーザーID）
- [ ] テスト用Googleアカウント（同意画面のテストユーザーに追加済み）
- [ ] （Calendar / Drive の第1層読み取りをライブ検証する場合）起動シェルに `SHOGUN_ENABLE_WAVE1_READ=calendar,drive` を設定（受理トークン: `calendar`/`gcal`, `drive`/`gdrive`。未設定の既定は Gmail のみ＝Calendar/Drive は UI 上 Coming soon のまま。リビルド不要でフラグだけで解放される）

### 4-2. ビルド & 起動

- [ ] `pnpm install`
- [ ] `pnpm --filter @shogun-ai/desktop build:vite`（frontend dist生成）
- [ ] `cargo tauri dev`（または `cargo build -p shogun-desktop-spike` 後に起動）
- [ ] 起動ログに `[spike] connector runtime started (read-sync poller live)` が出る

### 4-3. 接続（第1層 読み取り）

- [ ] 設定ウィンドウを開く: **⌘⇧,（カンマ）** グローバルショートカット（`open_settings` コマンドも同等）
- [ ] Connections で **Gmail** の Connect → ブラウザ認可 →「SHOGUN is connected」表示
- [ ] Keychainにトークンが入ったか:
      `security find-generic-password -s com.selectkk.shogun -a gmail-tokenset`
- [ ] Connectionsで Gmail が **Connected** 表示、last sync が更新される
- [ ] **初回同期時にログを確認**: `[connectors] gmail synced (+N new)` が出るか
- [ ] **⚠️ フィールドマッピング確認**: 同期されたイベントの title/body が正しく入っているか。
      ずれていたら `crates/shogun-integrations/src/result.rs` の候補キー（ID/TITLE/BODY/TS_KEYS）に
      実レスポンスのフィールド名を追記
- [ ] Google Calendar / Drive でも同様に接続 → 同期確認

### 4-4. 書き込み（第1層 L3、例: カレンダー作成）

- [ ] 手動で送信をキューに投入（開発用エントリ）: `submit_send` コマンドを
      kind=`calendar`, destination=`<タイトル>`, body=`<本文>` で呼ぶ
- [ ] 設定ウィンドウの Approvals に承認待ちが**全文表示**される（FR-AG-03）
- [ ] 「Confirm & send」→ カレンダーに実際にイベントが作成される
- [ ] トレーサビリティに記録が残る（route=direct）
- [ ] **⚠️ ツール名確認**: 実行が失敗する場合、`toolmap.rs` の該当ツール名を
      各サーバーの `tools/list` と突合（例: calendar `create_event`）

### 4-5. Composio Gmail送信（第2層、オプトイン）

- [ ] Composio側でGmailのconnected accountを作成
- [ ] ドラフト止まりモードをOFF（同意フロー経由）
- [ ] `submit_send` kind=`email` で投入 → Approvalsで確認 → Confirm
- [ ] 実際に送信される / 失敗時はGmailドラフトが保存される（FR-C2-05）
- [ ] トレーサビリティに「第三者経由（Composio）」バッジ付きで記録される（FR-C2-04）

### 4-6. 異常系の確認

- [ ] トークン失効時: 同期が amber になり、再認証導線が出る（FR-INT-06）
- [ ] 未解放Wave（例: Wave 1状態でSlack）: 接続/実行が拒否される
- [ ] 切断: Keychainトークン削除 + 同期停止（FR-INT-07）

### 4-7. フィードバックループ

ライブ検証で判明した実フィールド名・ツール名を、以下に反映してコミット:
- `crates/shogun-integrations/src/result.rs`（レスポンス正規化の候補キー）
- `crates/shogun-integrations/src/toolmap.rs`（op→tool の暫定名）
- `crates/shogun-integrations/src/endpoints.rs`（scope の最終セット）

これらが確定すれば、コネクタ層は「定義」から「実証済み」へ移行する。
