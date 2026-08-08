# 仕様たたき: オンボーディング & Connections UI 仕上げ — 接続体験の実装

> Issue #82 / P0。設計正本: `docs/mcp/02-user-guide.md`, `docs/mcp/03-product-design.md` §3-4
> ステータス: たたき台（レビュー前提のドラフト）

## 1. 背景 / Why

Connections UI は骨格実装済みだがラフ。`02-user-guide.md` で項目・マイクロコピーが定義されたので実装に落とす。#81 依存はマイクロコピー表示のみで、**他はすべて並行着手可能**。

## 2. ゴール / Done の定義

1. 新規ユーザーがオンボーディングから Calendar + Mail を接続できる
2. Settings → Connections で状態確認・Reconnect・Disconnect まで完結できる
3. 計測イベント（接続率・接続深度・接続→初回利用）が発火している（内容ゼロ、イベント種別のみ）

## 3. スコープ

**In:** オンボーディング「Connect your work」改訂 / Connections パネル拡充 / Composio 別枠化 / サンプル質問提示 / マイクロコピー表示（#81 後）/ 計測イベント
**Out:** 新しいオンボーディング画面の追加（**画面は増やさない**）、Wave 2/3 の接続実装、Composio オプトインのオンボーディング組み込み（意図的に初回に出さない）

## 4. 画面仕様

### 4-1. オンボーディング「Connect your work」（既存4番目ステップの改訂）

- lead 文は現行を維持: *"Give SHOGUN read access to the tools you already use. It connects directly to each service — nothing is routed through anyone else."*（直結の信頼訴求は必ず残す）
- 行構成・順序: **Calendar → Mail → Drive**（現行ワイヤーに無い Drive 行を追加）→ 区切り → Slack 等 *"Not available yet"*（disabled, opacity 落とし）
- 推奨バッジは Calendar / Mail の2つのみ（3つ等価に並べない — 選択負荷対策、`03-product-design.md` §3）
- Calendar / Drive の Connect → ブラウザ OAuth → 復帰で行が Connected に変化（`oauth_flow.rs` ループバック）
- **Mail の Connect は Google OAuth ではなく Composio 3開示同意シート**（第三者経由 / データ種別 / 取消可能性。Gmail 全面 Composio 化決定に伴い読み取り同意を接続時に取る。`docs/mcp/02-user-guide.md` §2 改訂版）。同意で Connected 化、拒否は未接続のまま先へ
- スキップ可能。添え文: *"You can connect these anytime in Settings."*
- **送信の解放（draft-stop OFF）はこのステップに入れない**（初回は読み取り価値に集中。送信解放の同意は Approvals 文脈で）

### 4-2. Settings → Connections パネル（拡充）

各サービス行:

| 項目 | 仕様 | データ源 |
|---|---|---|
| 名前+アイコン+説明 | 静的文言は `02-user-guide.md` §1 の表と一致させる | `Service` enum（行はenumから自動導出、ハードコードしない） |
| アクセス範囲バッジ | Read / Read & Draft / Read & Write | `scope.rs` 権限表から導出 |
| 状態バッジ | Connected（緑）/ Reconnect（amber）/ Not connected / Coming soon | `connection.rs` FSM + `service_gate` |
| 接続日時・最終同期 | *"Connected Jul 12 · Last sync 5 min ago"* | 接続記録 + `ConnectorRuntime` |
| アクション | Connect / Disconnect / Reconnect | 既存コマンド |

状態別ルール:
- **amber は赤エラーにしない**: *"Session expired — reconnect to resume sync"* + Reconnect ボタン
- **Disconnect は確認ダイアログ必須**: *"Disconnect Calendar? SHOGUN will stop syncing and forget its access."* 実行でトークンを Keychain から削除
- **Coming soon** は押せないが行として見せる

### 4-3. Mail 行の Composio 表示（別枠化から変更）

Gmail 全面 Composio 化に伴い、「第1層の Gmail 行 + 別枠の Composio セクション」構成は廃止し、**Mail 行そのものが Composio 経由の行**になる:

- Mail 行は Calendar / Drive と同じリストに置くが、**視覚的に区別**し常時 *"via Composio, a third-party service"* ラベルを表示（正直表示。直結と誤認させない）
- 行の詳細に: 同意状態 / draft-stop トグル（既定 ON、同意後のみ OFF 可）/ 同意の取消（取消で同期停止・egress ゼロへ）
- 未同意の間、Gmail の同期・送信系機能はすべて同意フロー（3開示）へ誘導

### 4-4. アハ・モーメント（接続直後のサンプル質問）

接続完了画面 or 初回チャットで、接続状態に応じて出し分け:

| 接続状態 | サンプル質問 |
|---|---|
| Calendar | *"What's on my calendar tomorrow?"* |
| Calendar + Mail | *"Help me prep for my next meeting."* |
| Mail のみ | *"Anything I need to reply to?"* |

実装は接続状態を見た文言出し分けのみ（軽量）。クリックでそのままチャット送信。

### 4-5. 会話中マイクロコピー（#81 のイベント購読）

- 表示: `02-user-guide.md` §4 の文言表に従う（*Checking your calendar…* 等。サービス名を主語に）
- **読み取りにのみ表示**。送信・書き込みは Approvals パネルが主役なのでステータス行で流さない
- 未接続案内 / amber 案内 / L3 待ち（*Waiting for your approval to send.*）も同表に従う

## 5. 計測イベント（`03-product-design.md` §4）

発火するイベント（**内容は一切取らない。接続状態とイベント種別のみ**。Issue #28 のプライバシー設計と整合させる):

| イベント | タイミング |
|---|---|
| `onboarding_connect_shown` / `_skipped` | ステップ表示 / スキップ |
| `service_connected` / `service_disconnected` | 接続/切断（service 名のみ） |
| `connection_depth` | オンボーディング完了時の接続数（0-3） |
| `mcp_first_use` | 接続後初の MCP 由来回答（接続→初回利用の24h判定はサーバ側/分析側で） |
| `amber_entered` / `amber_recovered` | amber 遷移 / 再認可復帰 |
| `composio_optin_shown` / `_accepted` / `_declined` | 送信同意フロー |

既存 PostHog 基盤（PR #91）の opt-out ゲートを必ず通す。

## 6. UI 文言規約チェック

- 全文言は英語・i18n-ready（コードから分離）
- 競合名・技術スタック名（"MCP" を UI に出すかは要判断 → 推奨: ユーザー向けは "connections" で通し、MCP という語は Settings 詳細と docs のみ）
- 絵文字は ⚔ のみ / "AI-powered" 等の禁止ワードなし

## 7. テスト / 受け入れ

- [ ] 新規プロファイルでオンボーディング → Calendar+Mail 接続 → サンプル質問表示 → 回答（E2E、#80/#81 完了後）
- [ ] 接続ゼロスキップでもオンボーディング完了できる
- [ ] amber 状態の行表示と Reconnect 導線（トークン revoke で再現）
- [ ] Disconnect 確認 → Keychain からトークン消滅を確認
- [ ] プラン判定: Connections 自体は Standard 以上で利用可。ゲーティングは Rust コア側判定を通す（webview だけに頼らない）
- [ ] 計測イベントに本文・タイトル等のユーザーコンテンツが一切含まれないことをコードレビューで確認

## 8. 実装ステップ（PR 分割案）

1. `feat(desktop): Connections パネル拡充`（バッジ・同期時刻・Disconnect 確認・Coming soon）— 依存なし、即着手可
2. `feat(desktop): オンボーディング Connect your work 改訂`（Drive 行・推奨順・スキップ文言）— 依存なし
3. `feat(desktop): Composio 別枠セクション + 3開示同意フロー導線`
4. `feat(desktop): アハ・モーメント（サンプル質問出し分け）`
5. `feat(desktop): 計測イベント発火`（PostHog opt-out ゲート経由）
6. `feat(desktop): 会話中マイクロコピー`（#81 のイベント定義確定後）
