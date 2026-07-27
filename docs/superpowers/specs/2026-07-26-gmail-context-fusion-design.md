# Gmail 文脈融合 + 送信ループ — 設計

- 日付: 2026-07-26
- ブランチ: design/product-visual-polish
- スコープ: Gmail 読み取り（同期）＋ 融合（画面＝セレクタ / 取得＝中身）＋ 送信（Composio, L3）の縦一本
- 明示的に後回し: Google Calendar、Slack、Wave 3 コネクタ

## 背景と問題

⌥ タップのドラフトが的外れになる。原因は文脈供給が薄いこと。ブラウザにフォーカスがあるとき、
AX ウォークは 300 要素の予算をツールバー/タブ帯で使い切り、本文に届く前に打ち切られる
（実測: Gmail スレッド全体で `bytes=63`）。AX ウォーク側は別途「本文優先」の並べ替えで一部緩和
済み（`Role::DeferredChrome`）だが、**根本解は画面スクレイプではなく API/MCP 由来の構造化データを
使い、それを画面コンテキストと融合すること**。

## 確定した設計判断

1. **MCP 非依存**: Google 公式リモート MCP は Developer Preview で実接続できない可能性がある。
   OAuth / Keychain / トークン自動更新の基盤（`shogun-integrations`）は MCP 非依存なので流用し、
   transport 継ぎ目のみ Gmail REST 直叩きに差し替える。
2. **スコープ**: Gmail 読み取り＋送信(Composio) の縦一本。Calendar は後回し。
3. **融合モデル**: 画面(AX) は「今どのスレッド/相手を見ているか」の**セレクタ**に使い、本文は
   Gmail から取得した**完全なスレッド**（ペイロード）を使う。AX の不完全さを API データで補う。

## 既存資産（実装済み・テスト済み。再利用する）

- OAuth 完全実装（PKCE・ループバック・トークン交換/更新）: `shogun-integrations/oauth.rs`,
  `oauth_flow.rs`
- Keychain トークン保存（不変条件7準拠）: `shogun-integrations/keychain.rs`,
  service = `com.selectkk.shogun`, account = `<source>-tokenset`
- transport 継ぎ目: `McpRpc::call_tool(service, tool, args) -> Value`
  (`shogun-integrations/rpc.rs`)、`RemoteMcpTransport<R: McpRpc>` が
  `IntegrationTransport`(`read_sync`/`fetch_on_demand`) と `WriteExecutor`(`execute`) を実装
- 正規化 `parse_items`（`shogun-integrations/result.rs`）: 許容キーに Gmail ネイティブ名
  （`threadId` / `messageId` / `snippet` / `internalDate`）を既に含む
- 融合の信頼度ゲート `shogun-fusion`（`assemble.rs` / `confidence.rs`）: 高=事実 / 中=possibly /
  低=除外。テスト済み
- 承認キュー + 送信ルーティング: `apps/desktop/src-tauri/src/approvals.rs`
  （`confirm_send` → `RoutedSendTransport` → `execute_send`、L3・専用ボタンのみ）
- Composio 送信: `shogun-core/composio_send.rs`（composio.dev 直 HTTP、APIキーは Keychain、
  MCP 非依存）、型安全な同意ゲート + draft-stop 既定ON（`shogun-mcp/composio.rs`,
  `shogun-integrations/composio.rs`）
- 接続コマンド + 15分同期ポーラー: `apps/desktop/src-tauri/src/connectors.rs`
- 文脈組み立て: `Db::build_reply_context(thread_key)`, `assemble_context(query,...)`,
  `inline_memory(limit)`（`shogun-core/daemon.rs`）
- thread_key 導出: `shogun_memory::thread::thread_key(source, native_id, app, title)`
  （`shogun-memory/thread.rs`）、`normalise_window_title`

## §1 全体アーキテクチャ

```
[Gmail REST API]
      │ OAuth token（Keychain・自動更新。既存 shogun-integrations を流用）
      ▼
GmailRestRpc （新規。McpRpc を実装。公式MCP HttpMcpRpc の代わり）
      ▼
RemoteMcpTransport<GmailRestRpc>（既存）→ parse_items 正規化（既存）
      ▼
ingest / event log / thread store（既存）
      ▼
┌──────────── 融合（新規の中核）────────────┐
│ AX画面 ──セレクタ(件名)──▶ リンカ ──▶ gmail:<threadId> │
│ 取得済みGmailスレッド ──ペイロード(完全本文)──────────  │
│ state facts ──信頼度ゲート（既存）───────────────────  │
└───────────────────┬──────────────────────┘
                    ▼
   build_reply_context / assemble_context を拡張
                    ▼
   ドラフト生成(BYOK Agent lane) → キャレット挿入
                    ▼
   送信: L3 承認キュー → Composio（draft-stop 既定ON）
```

新規に書くのは実質2つ: **`GmailRestRpc`** と **融合リンカ**。残りは配線。

## §2 接続層（MCP 非依存の Gmail REST）

継ぎ目は `McpRpc` trait 一点。`GmailRestRpc`（新規）が `McpRpc::call_tool` を実装する。

- read: `read_sync` が渡す tool 名（例 `list_recent_messages`）を Gmail REST にマップ
  - `GET /gmail/v1/users/me/messages?maxResults=N` → 各 id を `messages.get`
    （`format=metadata` で件名/日時、本文は必要時に取得）
- write（下書きフォールバック用。§4 で必須と判明）: 下書き作成 tool 名を
  `POST /gmail/v1/users/me/drafts`（`users.drafts.create`）にマップ
- レスポンスを `parse_items` が食える形 `{ "structuredContent": [ {threadId, subject,
  snippet, internalDate}, ... ] }` で返す。許容キーが Gmail 名なので生 JSON をほぼそのまま包むだけ
- OAuth トークンは既存 `ManagedTokenProvider`（Keychain＋自動更新）から取得。ここは新規コード無し
- HTTP egress は shogun-core に集約（不変条件3 / FR-TR-03）。`GmailRestRpc` は
  `shogun-core/src/` 配下に置き、egress 点にトレーサビリティログを実装

設定: client id/secret は環境変数 `SHOGUN_GOOGLE_CLIENT_ID` / `SHOGUN_GOOGLE_CLIENT_SECRET`
（既存 `connectors.rs` の期待どおり、Desktop app クライアント）。

代替案として `IntegrationTransport` を直接実装する独立 `GmailRestTransport` も検討したが、差分が
大きく正規化を重複させるため不採用。`McpRpc` 継ぎ目に入れる方が差分最小。

## §3 融合（セレクタ→ペイロードの橋渡し）

### 問題: 同じ会話が 2 つの thread_key を持つ

- 画面(AX)から見た Gmail スレッド: `capture:com.google.Chrome:<正規化した件名>`（native_id 無し）
- Gmail 同期したスレッド: `gmail:<threadId>`（native_id あり）

この 2 つを橋渡しするのが**リンカ**（新規の主要ロジック、純関数）。

### リンカ

照合キーは件名（subject）。ブラウザの window title を `normalise_window_title` で正規化すると
メール件名を含み、取得済み Gmail アイテムは `subject` を持つ。段階的に:

1. 正規化件名の完全一致
2. 外れたら包含（片方が他方を含む）
3. それでも外れたら AX フォールバック
4. 誤マッチ防止: 件名が短すぎる/空のときは包含照合を使わず即フォールバック
   （他人のスレッドを差し込む方が害が大きい）

### `build_reply_context` の拡張

```
build_reply_context(on_screen_selector):
  1. リンカで on_screen_selector → best gmail:<threadId> を解決
  2. 解決できた → Gmail スレッド完全本文を turns の主ソースにする（高信頼・provenance付き）
  3. 解決できない → 従来どおり capture の turns（断片・OnScreenOnly ラベル）
  4. state facts（commitments/open_loops）は従来どおり信頼度ゲート通過分を添える
```

### 信頼度と provenance

- 取得した Gmail 本文 = 高信頼の事実。ヒューリスティック抽出の state facts（≤0.4）とは別レーン。
  「可能性」ではなく事実としてプロンプトに入れる
- 各ペイロードに provenance（どの messageId 由来か）を持たせる。将来トレーサビリティ画面や
  チャット引用で「このメール由来」と示せる
- 融合出力は「事実(取得データ) / 弱い示唆(中信頼state) / 除外(低信頼)」の 3 層を維持
  （既存 `confidence.rs` の枠を使う）

### プロンプト反映

`ReplyContext` に `payload_source`（`Fetched{message_id}` / `OnScreenOnly`）を足し、
`build_prompt`（`shogun-core/inline.rs`）で本文の出所を意識した組み立てにする。ドラフト生成・
挿入・送信は既存経路。

## §4 送信ループ・MCP 非依存の一貫性・エラー処理

### 送信ループ（既存配線 + キー供給）

```
ドラフト生成 → 挿入 → ユーザーが「送る」
  → confirm_send（L3・専用ボタンのみ・Enter 単独では確定しない）
  → RoutedSendTransport
       ├ Email → ComposioSendTransport（composio.dev 直 HTTP、MCP 非依存・既存）
       └ その他 → FirstLayerSendTransport
  → 失敗時 save_gmail_draft（FR-C2-05: 下書き保存、「送信済み」と偽らない）
  → execute_send → traceability sink（egress 記録）
```

必要なのはキー供給のみ: Composio APIキー（Keychain、既存 `composio_api_key()`）、
`SHOGUN_COMPOSIO_USER_ID`（環境変数、既存）。

### MCP 非依存の一貫性（§2 への追加要件）

下書きフォールバック `save_gmail_draft` は `FirstLayerSendTransport`（= `RemoteMcpTransport`）を
通る。§2 で継ぎ目を `GmailRestRpc` に差し替えるので、下書き保存も自動的に Gmail REST 経由になる
（`WriteExecutor::execute` → `call_tool` → GmailRestRpc が `users.drafts.create` にマップ）。
したがって **`GmailRestRpc` は read だけでなく draft-create の write マッピングも含める**
（§2 に反映済み）。Composio 送信そのものはもともと MCP 非依存。

### 不変条件の維持

- 読み取り同期は L1（自動）でよいが、送信は必ず L3（不変条件4）。既存の承認キューを通す
- Composio は「第三者経由」バッジ必須（既存 `COMPOSIO_THIRD_PARTY`）
- draft-stop 既定 ON（設定で明示 OFF にするまで送信不可、型で保証済み）
- secrets は Keychain のみ（不変条件7）。ログに鍵・本文を出さない

### エラー処理（作業を止めない）

- Gmail 同期失敗 → 該当サービスだけ amber、他は継続（既存 FR-INT-06）
- リンカ不一致 → AX フォールバック（§3）
- Composio 送信失敗 → 下書き保存 + 通知（既存 FR-C2-05）
- 認証エラー(403 等) → 直近で実装したプロバイダ理由の表面化（redact 済み）を流用

## テスト

| 層 | テスト | 実行環境 |
|---|---|---|
| `GmailRestRpc` | tool名→RESTマッピング、REST JSON→parse_items が食える形、read/draft 両方 | Linux（HTTP モック） |
| リンカ | 完全一致 / 包含 / 衝突フォールバック / 短件名フォールバック | Linux 純関数 |
| `build_reply_context` | Fetched 解決 / OnScreenOnly / 信頼度 3 層維持 | Linux |
| 送信 | Email→Composio、失敗→下書き、L3 必須、第三者バッジ | 既存テスト拡張 |
| 実接続 | Gmail 同期→event log→融合→ドラフト→L3→Composio 送信 の縦一本 | 実機（キー投入後） |

## 受け入れ基準（縦一本）

1. Gmail 接続 → 最近のメールが event log に入る
2. Gmail のスレッドを画面で開いて ⌥ → AX の断片ではなく取得した完全なスレッドを根拠にドラフトが
   入る（`payload_source = Fetched`）
3. 送信ボタン → L3 確認 → Composio 送信、または失敗時に下書き保存
4. SLO 無回帰（cache 更新 ≤300ms、初トークン ≤1s、ローカル検索 ≤500ms）

## 新規コンポーネント一覧

- `GmailRestRpc`（`shogun-core/src/`）: `McpRpc` 実装。read（messages.list/get）+
  write（drafts.create）を Gmail REST にマップ。egress トレーサビリティ付き
- 融合リンカ（`shogun-core` または `shogun-fusion` の純関数）: 画面セレクタ(件名) →
  `gmail:<threadId>` の解決
- `ReplyContext.payload_source` フィールド + `build_reply_context` の解決ステップ
- 配線: `connectors.rs` の transport を `GmailRestRpc` ベースに、`confirm_send` の Composio キー供給

## 未解決の外部依存（実装前に用意が必要）

- Google OAuth Desktop クライアント（`SHOGUN_GOOGLE_CLIENT_ID` / `SHOGUN_GOOGLE_CLIENT_SECRET`）。
  Gemini で 403 になったプロジェクトとは別プロジェクトで作成推奨
- Composio APIキー（Keychain）+ `SHOGUN_COMPOSIO_USER_ID`。送信検証にのみ必要。読み取り＋融合は
  Composio 無しで検証可能

---

## 追記（2026-07-27）: Gmail を全面 Composio 経由に変更

当初この設計は「MCP 非依存で Gmail REST 直接読み取り、送信のみ Composio」だった。その後ユーザー判断で **Gmail の読み取り・下書き・送信すべてを Composio 経由**にした。

### 動機
- Google 公式リモート MCP は Developer Preview で実接続できない可能性が高い
- 認証情報を **Composio APIキー＋user id の1組**に集約し、Google Cloud の OAuth クライアント作成を不要にしたい

### 受容したトレードオフ
- **受信箱の内容が第三者(Composio)を経由する。** 不変条件3（第三者露出の最小化）の原則に対する明示的・記録済みの例外（CLAUDE.md「連携実装ルール」に明記）

### 実装差分（設計 §2 の置き換え）
- transport 継ぎ目 `McpRpc` の実装を `GmailRestRpc`（Gmail REST 直＋OAuth）から **`ComposioReadRpc`**（`HttpComposioApi` で Composio ツールを呼ぶ）に差し替え。`GmailRestRpc`/`gmail_rest.rs` は撤去
- Composio ツール: 読み取り `GMAIL_FETCH_EMAILS` / `GMAIL_FETCH_MESSAGE_BY_THREAD_ID`、下書き `GMAIL_CREATE_EMAIL_DRAFT`、送信 `GMAIL_SEND_EMAIL`（既存）
- **`gmail_shape`（完全本文抽出・base64urlデコード・MIME walk）はそのまま再利用**。Composio が Gmail ネイティブなメッセージ形（`messageId`/`threadId`/`subject`/`snippet`/`payload`/`internalDate`）を `{data, successful}` で返すため、正規化（`parse_items`）・ingest・融合は無改修
- 認証情報: APIキーは Keychain、user id は非秘匿として `composio.json`。キー/user id 保存時にコネクタランタイムを再構築
- **同意を読み取りにも適用**: 同期ポーラーは `consent_acknowledged` が false ならスキップ。送信は L3＋draft-stop 維持
- **読み取り egress のトレーサビリティ**: 成功時に `Route::Composio, third_party=true`、チャンクは空（第三者境界の記録のみ、内容は残さない）

### 未検証（要 Composio アカウント）
- Composio の `data` 直下の正確なネスト（`data.messages` 等）とフィールド名は、実接続で最終確認が必要。抽出は `composio_read.rs` の `extract_messages` に隔離済みで、初回ライブコールで直せる
