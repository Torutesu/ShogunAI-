# OAuthクライアント登録手順（コネクタのライブ接続前提）

第1層コネクタをライブで動かすために人間側で必要な登録作業。コード側は完成しており
（`docs/connector-adapter-plan.md` 実装状況表参照）、ここで発行するクレデンシャルを
環境変数に入れれば「Connect → ブラウザ認可 → Keychain保存 → 15分同期」が動く。

- 作成日: 2026-07-23
- 対象: Google Workspace（Wave 1: Gmail / Calendar / Drive）、Slack（Wave 2、参考）

---

## 1. Google Workspace（Wave 1）

### 1-1. Developer Preview 登録

Google Workspace の公式リモートMCPサーバー（`gmailmcp` / `calendarmcp` / `drivemcp`
.googleapis.com）は **Developer Preview Program** の機能。
https://developers.google.com/workspace/preview からプログラムに登録する
（Googleアカウントで申請 → 承認待ち。通常数日）。

### 1-2. GCPプロジェクトとAPIの有効化

1. https://console.cloud.google.com で新規プロジェクト作成（例: `shogun-dev`）
2. 「APIとサービス → ライブラリ」で以下を有効化:
   - Gmail API / Google Calendar API / Google Drive API
   - （MCPサーバー用のAPI有効化が別途必要な場合はDeveloper Previewのドキュメント指示に従う）

### 1-3. OAuth同意画面

「APIとサービス → OAuth同意画面」で構成:

| 項目 | 開発中の設定 |
|---|---|
| User Type | **External** + **Testing** モード（審査不要、テストユーザー100人まで） |
| アプリ名 | SHOGUN (dev) |
| テストユーザー | 自分のGoogleアカウントを追加 |
| スコープ | 下記 1-5 のスコープを追加 |

> ⚠️ **審査(CASA)について**: `gmail.readonly` / `gmail.compose` は restricted scope。
> **Testingモードのうちは審査不要**。一般公開（Production化）する段階で Google の
> 検証 + CASA Tier 2 セキュリティ評価が必要になる（費用・数週間〜）。開発・クローズドβは
> Testingモードで回避できる。この判断は要件 §9 / adapter-plan の隠れコスト節参照。

### 1-4. OAuthクライアント作成（Desktop app）

「APIとサービス → 認証情報 → 認証情報を作成 → OAuthクライアントID」:

- アプリケーションの種類: **デスクトップアプリ**
- 名前: `shogun-desktop-dev`

→ 発行された **クライアントID** と **クライアントシークレット** を控える。
（Desktopアプリのシークレットは機密扱いではないが、リポジトリにはコミットしない）

ループバックリダイレクト（`http://127.0.0.1:<random port>/callback`）はデスクトップ
アプリ種別では追加登録不要（Googleが `http://127.0.0.1` の任意ポートを許可する）。

### 1-5. 要求スコープ（コードと一致していること）

`crates/shogun-integrations/src/endpoints.rs` が要求する最小スコープ:

```
Gmail:    gmail.readonly, gmail.compose        # send は要求しない（第2層Composioのみ）
Calendar: calendar.calendarlist.readonly, calendar.events.readonly,
          calendar.events.freebusy, calendar.events
Drive:    drive.readonly, drive.file
```

同意画面のスコープ設定にはこれらを登録する。**`gmail.send` は絶対に追加しない**
（不変条件4: 第1層は送信経路を持たない）。

### 1-6. 環境変数の設定

macOSでアプリを起動するシェルに:

```bash
export SHOGUN_GOOGLE_CLIENT_ID="xxxx.apps.googleusercontent.com"
export SHOGUN_GOOGLE_CLIENT_SECRET="GOCSPX-..."
```

（インストーラ配布時はビルド時埋め込み or 初回設定画面に移行する。env は開発用）

### 1-7. 動作確認手順

1. macOSで `cargo tauri dev`（または dev ビルド起動）
2. 設定ウィンドウ（`open_settings`）→ Connections → Gmail の **Connect**
3. ブラウザでGoogle認可 → 「SHOGUN is connected.」表示 → タブを閉じる
4. Connections の Gmail が **Connected** になり、Keychain（`com.selectkk.shogun` /
   `gmail-tokenset`）にトークンが入っていることを確認:
   `security find-generic-password -s com.selectkk.shogun -a gmail-tokenset`
5. 15分後（または poll を手動トリガして）`[connectors] gmail synced (+N new)` ログと
   event log への `source=gmail` 行を確認
6. **最初のライブ同期時**: `crates/shogun-integrations/src/result.rs` のフィールド
   マッピング（tolerant実装）が実レスポンスのフィールド名と合っているかログで確認し、
   必要なら候補キーを追記する

---

## 2. Slack（Wave 2 — OPEN-03 解決済み）

要件 §9.1 OPEN-03「Slack公式リモートMCPの提供状況確認」は **解決**:
Slackは公式リモートMCPサーバーを提供している（2026年2月ローンチ）。

| 項目 | 値 |
|---|---|
| エンドポイント | `https://mcp.slack.com/mcp`（JSON-RPC 2.0 over Streamable HTTP） |
| 認可エンドポイント | `https://slack.com/oauth/v2_user/authorize` |
| トークンエンドポイント | `https://slack.com/api/oauth.v2.user.access` |
| クライアント登録 | **DCR非対応** — Slack App を作成し固定の client id/secret を使う |
| 管理者承認 | ワークスペース管理者がMCPクライアント接続を承認・管理（アプリ承認プロセス） |
| アプリ要件 | **ディレクトリ公開アプリ or 社内アプリのみ** MCP利用可 |

含意:
- FR-INT-30 のフォールバック（管理者未承認 → クリップボードドラフト）は**恒常運用ではなく
  例外パス**として維持（承認されないWSで発動）。実装済み（`shogun-mcp/src/slack.rs`）
- 「ディレクトリ公開 or 社内アプリのみ」の制約により、一般配布には **Slack App Directory
  公開申請**が必要（Wave 2 着手時の人間側タスク）。開発中は自分のWSの社内アプリで可

### Slack App 作成（開発用・Wave 2着手時）

1. https://api.slack.com/apps → Create New App（自分のワークスペース）
2. OAuth & Permissions で user token scopes を設定（`search:read.public`,
   `chat:write` 等 — ツール別スコープはSlack MCPドキュメント参照）
3. client id / secret を `SHOGUN_SLACK_CLIENT_ID` / `SHOGUN_SLACK_CLIENT_SECRET` に

> 注: Slackのトークンレスポンスはユーザートークンが `authed_user.access_token` に
> ネストされる形式。コード側は `parse_token_response` が両形式を受ける（実装済み）。

---

## 3. セキュリティ上の注意（全サービス共通）

- クレデンシャルは**リポジトリ・DB・ログに書かない**（invariant 7）。トークンは
  Keychainのみ、client id/secret も env / ビルド時注入のみ
- スコープは endpoints.rs の定義から増やさない（FR-INT-05: 必要最小限）
- 検証時のスクリーンショットにトークンやコードが写り込まないこと
