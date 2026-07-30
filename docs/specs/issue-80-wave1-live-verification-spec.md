# 仕様たたき: Wave 1 ライブ接続検証 — Calendar / Gmail / Drive の実 MCP 接続を通す

> Issue #80 / P0。設計正本: `docs/mcp/01-architecture.md`, `docs/mcp/04-dev-implementation.md` §2-A, `docs/connector-summary-and-live-checklist.md`, `docs/oauth-client-setup.md`
> ステータス: たたき台（レビュー前提のドラフト）

## 1. 背景 / Why

コネクタ層（第1層＝公式リモートMCP直結、第2層＝Composio）はコード＋テストは完成しているが、**実クレデンシャル・実MCPサーバー・macOS実機の3点セットでの検証が未実施**（WP-G）。`toolmap.rs` のツール名と `result.rs` のレスポンス正規化は暫定マークのままであり、この上に乗る #81（LLM結線）・#82（UI仕上げ）の仕様が確定できない。**#80 は Wave 1 実装トリオ（#80→#81→#82）のクリティカルパス先頭**である。

## 2. ゴール / Done の定義

1. Wave 1 の3サービスが実機で接続・同期でき、`event_log` にデータが着地する（Calendar / Drive = 公式 MCP 直結 + Google OAuth、**Gmail = 全面 Composio 経由 + 3開示同意**）
2. `toolmap.rs` / `result.rs` の暫定マークが外れている（実 `tools/list`・実レスポンスと突合済み）
3. Composio Gmail 送信が実エンドポイントで L3 経路ごと検証済み
4. amber（トークン失効）→ 再認可 → 復帰が実機で確認済み
5. 検証結果が `docs/connector-summary-and-live-checklist.md` に追記され、乖離があれば修正コミットが入っている

## 3. スコープ

**In:**
- GCP OAuth クライアント作成（Testing mode、`docs/oauth-client-setup.md` 手順。**Calendar / Drive のみ** — Gmail は Google OAuth 不要）
- Calendar → Drive の順の第1層 end-to-end 検証
- **Gmail は全面 Composio 経由で検証**（2026-07 決定。読み取り同期・送信とも。3開示オプトイン → 同期 → L3 承認 → 送信 → トレーサビリティ記録）
- 検証で見つかった toolmap / result / oauth_flow / Gmail 経路移行（`docs/mcp/04-dev-implementation.md` §2-A0）の修正

**Out（別Issue）:**
- LLM へのツール定義結線（#81）
- オンボーディング/Connections UI の拡充（#82）
- Wave 2/3 サービス（Slack / Notion / GitHub / Linear）のライブ検証
- Google OAuth 本番審査（Testing mode で検証後、別途着手）

## 4. 検証手順仕様

### 4-1. 第1層（Calendar / Drive）共通シーケンス

各サービスで以下を順に確認し、結果を checklist に記録する:

| # | ステップ | 確認対象 | 合格条件 |
|---|---|---|---|
| 1 | `connect_service` → ブラウザ OAuth 同意 → ループバック復帰 | `oauth_flow.rs`（PKCE） | アプリに制御が戻り接続状態が `connected` |
| 2 | トークンの Keychain 保存 | `keychain.rs` / `token.rs` | 平文がJSON設定・ログに出ない（不変条件7） |
| 3 | `tools/list` 取得 | `transport.rs` / `toolmap.rs` | 全ハブ操作名に対応する実ツール名が存在。乖離は toolmap 修正 |
| 4 | 同期1周（15分ポーラー手動発火） | `runtime.rs` / `sync.rs` / `result.rs` | FetchedItem 正規化が実レスポンスで成立、`event_log` 着地 |
| 5 | トークン失効の強制（GCP側で revoke） | `connection.rs` FSM | `amber` 遷移 → UI に Reconnect 表示 |
| 6 | 再認可 | 同上 | `connected` 復帰、同期再開、データ欠損なし |

### 4-2. Gmail（全面 Composio 経由）検証シーケンス

前提: Gmail は読み取り・ドラフト・送信のすべてが Composio 経由（2026-07 決定）。Google OAuth は使わない。コード側の経路移行（`endpoints.rs` の第1層 Gmail エントリ撤去等、`docs/mcp/04-dev-implementation.md` §2-A0）が検証の前提作業。

1. **未同意状態で一切の egress が発生しない**ことを確認（同期・送信とも。ゼロ egress テスト）
2. オプトイン同意（3開示 UI: 第三者経由 / データ種別 / 取消可能性）→ APIキーは Keychain、user id は設定JSON に着地
3. 読み取り同期1周 → `event_log` 着地、**読み取り egress のトレーサビリティ**（「第三者経由」フラグ＋ダイジェストのみ、本文なし）を確認
4. draft-stop ON（既定）で送信提案 → 下書き作成で停止することを確認
5. draft-stop OFF + L3 承認 → `POST /api/v3/tools/execute/GMAIL_SEND_EMAIL` 実行成功
6. 失敗系: API エラー時に FR-C2-05 ドラフト退避（`RoutedSendTransport`）が動くこと
7. 同意取消 → 同期停止・以後 egress ゼロに戻ることを確認

## 5. 成果物

- [ ] `docs/connector-summary-and-live-checklist.md` §4 の各項目に ✅/❌ ＋実測メモ
- [ ] `toolmap.rs` の暫定コメント除去 PR（乖離修正含む。Google 3サービス分）
- [ ] `result.rs` のフィールドマッピング確定 PR（tolerant 実装で吸収できなかった差分）
- [ ] 発見バグはこの Issue にぶら下げず個別 Issue 化（1バグ1Issue）

## 6. 不変条件チェック（CLAUDE.md）

- 不変条件3: 読み取り egress のトレーサビリティが Composio 経由でも記録されること（検証項目に含む）
- 不変条件4: 検証中も送信は必ず L3。検証用ショートカットで承認を飛ばすコードを書かない・残さない
- 不変条件7: 検証ログ・スクリーン記録にトークン/APIキーを含めない（`Secret` newtype の redact を信頼しつつ目視確認）

## 7. リスクと構え

| リスク | 構え |
|---|---|
| Google 公式 MCP が Developer Preview で実接続不可（Gmail は既に Composio 全面切替済み） | Calendar/Drive で同事象が出た場合は即オーナーにエスカレーション。Composio 寄せの追加判断は #80 内で勝手にしない |
| `tools/list` が想定と大きく乖離 | ハブ操作名は安定 IF なので影響は toolmap 張り替えに閉じる（設計通り）。#81 は操作名ベースで並行可 |
| Testing mode のテストユーザー上限・トークン7日失効 | むしろ amber 検証に利用する。長期運用は本番審査後 |

## 8. 実装メモ

- 実機（macOS 14+, Apple Silicon）でのみ実行可能。CI には載せない（純ロジック層のテストは既に Linux で通っている）
- 検証は Calendar 1本を最初に完走させてから Gmail / Drive に展開（1本通れば残りは差分確認）
- 所要目安: GCP セットアップ 0.5d / Calendar E2E 1d / Gmail+Drive 1d / Composio 0.5d / 修正・記録 1d
