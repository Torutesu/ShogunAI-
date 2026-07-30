# MCP 接続レイヤー — アーキテクチャ設計

> Issue #59 のアウトプット第1弾（`00-overview.md` とセット）。
> Issue が求めた「MCP ハブ」の概念設計を、**実在するコード**と対応づけて記述する。
> 設計の正本は `docs/requirements-v1.0.md` §6.9〜6.10 と **CLAUDE.md「連携実装ルール」の 2026-07 決定（Gmail 全面 Composio 化）**。実装計画は `docs/connector-adapter-plan.md`。
> ⚠️ Gmail 経路のコードは旧設計（読み取り＝第1層）のまま。本書は決定後の設計を正として記述し、コード未追従の箇所は都度注記する（残作業は §6 / `04-dev-implementation.md` §2）。

## 1. 3層モデル

Issue #59 が定義した「ShogunAI クライアント / MCP ハブ / MCP サーバー群」の3層は、現行設計では次のとおり実現する。

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
│    - 認可: service_gate.rs（Wave × 接続状態 × 同意 ×      │
│            draft-stop）                                  │
└───────────┬──────────────────────────┬─────────────────┘
            │ 第1層（公式 MCP 直結）      │ 第2層（Composio）
┌───────────▼──────────────┐  ┌────────▼─────────────────┐
│ ③ 公式リモート MCP サーバー群 │  │ Composio API             │
│    Calendar / Drive        │  │ Gmail 読み取り・ドラフト・   │
│    （Wave 2/3: Slack 等）   │  │ 送信のすべて（2026-07 決定。 │
│                            │  │ opt-in 3開示同意必須、     │
│                            │  │ 同意なしでは同期もしない）    │
└──────────────────────────┘  └──────────────────────────┘
```

「MCP ハブ」という独立プロセス・サーバーは**存在しない**。ハブの3責務はローカルアプリ内の2クレートに分散実装されている。これは意図的な設計（FR-INT-01: 中間サーバー無し）で、ユーザーデータが**自社サーバー**を経由しないことを保証する。

**Gmail だけは第三者（Composio）を経由する**（2026-07 決定）。理由は Gmail 公式リモート MCP が Developer Preview で実接続確度が低いこと。受信箱の内容が第三者を経由する代償を明示的に受容した記録済み決定であり、opt-in 3開示同意・L3＋draft-stop（既定 ON）・読み取り egress トレーサビリティを必須条件とする。Gmail 公式 MCP が GA になれば読み取り/ドラフトを第1層へ戻す余地を残す（`00-overview.md` 前提 2）。

## 2. MCP ハブの3責務と実装の対応

Issue #59 が挙げたハブの責務は、いずれも対応する実装がある（Gmail 経路のみ旧設計からの移行が残作業）。

### 2-1. レジストリ（どの MCP が利用可能か）

| 役割 | 実装 |
|---|---|
| サービスの一覧と Wave 割り当て | `crates/shogun-mcp/src/scope.rs` — `Service` enum（7サービス）+ `Service::wave()` |
| 各サービスの MCP URL と OAuth スコープ | `crates/shogun-integrations/src/endpoints.rs`。⚠️ **Gmail の第1層エントリ（`gmailmcp.googleapis.com` + `gmail.readonly`/`gmail.compose`）は旧設計の残置**。2026-07 決定により Gmail は第1層エンドポイントを持たず（Google Cloud OAuth 不要）、削除/無効化が移行作業に含まれる |
| 操作名 → 各 MCP のツール名 | `crates/shogun-integrations/src/toolmap.rs`（**ツール名は暫定**、第1層は実サーバーの `tools/list` で突合が必要。Gmail の操作は Composio のツール slug へのマッピングに移行） |
| 権限表（サービス × 操作 × 承認レベル） | `scope.rs` のスコープ表。表にない操作は拒否。外部送信は必ず L3。**権限表は経路（第1層/第2層）に依存しない** — Gmail が Composio 経由になっても「Gmail で何ができるか」の権限は同じ表で管理する |

新サービス追加の拡張ポイント：`Service` enum → `endpoints.rs` → `toolmap.rs` → `result.rs`（正規化）の4点セット。

### 2-2. ユーザー単位の接続状態管理

| 役割 | 実装 |
|---|---|
| 接続状態機械（connected / amber / disconnected） | `crates/shogun-mcp/src/connection.rs` |
| OAuth 2.1 + PKCE（純ロジック） | `crates/shogun-integrations/src/oauth.rs`、ループバックフローは `oauth_flow.rs`（feature `live`）。**対象は Calendar / Drive（および Wave 2/3）。Gmail は OAuth を使わない** |
| トークン保管・自動リフレッシュ | `token.rs`（`TokenStore` / `TokenManager`）+ `keychain.rs`（macOS Keychain のみ、FR-INT-02） |
| 同期スケジューラ（15分ポーリング + オンデマンド） | `runtime.rs`（`ConnectorRuntime`、FR-INT-04）。**Gmail の同期は Composio 同意が無い限り一切スケジュールしない** |
| Composio のオプトイン状態 | `composio.rs` の同意ゲート（3開示 = `Disclosures`、型でガード）。consent_acknowledged / draft_stop / user_id（JSON 設定）+ API キー（Keychain）。⚠️ 現行の同意ゲートは送信文脈で設計されており、**読み取り同期を含む全 Composio 操作の前提条件へ拡張する**のが移行作業 |

状態遷移：`disconnected → (認可) → connected → (資格情報失効等) → amber → (再認可) → connected`。amber は「壊れた」ではなく「再認可すれば戻る」状態としてユーザーに提示する。Gmail の「接続」は OAuth ではなく **3開示同意 → Composio 接続確立**であり、FSM 上は同じ3状態で扱う。

### 2-3. ルーティング（LLM の呼び出しを実 MCP サーバーへ）

| 役割 | 実装 |
|---|---|
| 経路振り分け（Gmail の全操作 → 第2層、その他 → 第1層） | `crates/shogun-integrations/src/send_bridge.rs`。⚠️ **現行コードは「email 送信のみ第2層」の旧設計**。2026-07 決定により Gmail は読み取り同期・ドラフトも第2層に振り分けるよう移行する（振り分け単位が「操作種別」から「サービス」へ変わる） |
| 第1層トランスポート | `transport.rs` の `RemoteMcpTransport`（JSON-RPC 2.0、**ライブ接続未検証**）。対象は Calendar / Drive |
| 第2層トランスポート | `shogun-core` の `HttpComposioApi`（`api/v3/tools/execute/{tool}`）。Gmail の読み取り・ドラフト・送信すべてがここを通る |
| 認可の合成判断 | `service_gate.rs` — Wave 解放 × 接続状態 × Composio 同意 × draft-stop を合成。未リリース Wave は自動拒否。**Gmail は同意が無ければ読み取りも拒否** |
| レスポンス正規化 | `result.rs` — MCP / Composio レスポンス → `FetchedItem`（tolerant パース、実レスポンスでの確認が必要） |
| MCP サーバー（Memory API 提供側） | `mcp.rs` — JSON-RPC 2.0、13 ツール。MCP / CLI / REST の3面で対称（`dispatch.rs`） |

## 3. データフロー

### 3-1. 第1層の読み取り同期（例：Google Calendar）

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

### 3-2. 第2層の読み取り同期（Gmail、2026-07 決定後の経路）

```
ConnectorRuntime（15分周期）
  → service_gate: Wave 解放済み？ Composio 同意（3開示）済み？ → 同意なしなら即スキップ（外部通信ゼロ）
  → HttpComposioApi → Composio → Gmail
  → 読み取り egress をトレーサビリティに記録
     （第三者境界。内容は残さず、ダイジェスト/フラグ + 経路 = Composio のみ）
  → result.rs で FetchedItem に正規化
  → event_log に source="gmail" タグ付きで保存
```

第1層との違いは2点だけ：**(1) 同意ゲートが同期の前提条件**（同意前は同期そのものが存在しない）、**(2) 読み取りでも egress をトレーサビリティに記録**（第1層の読み取りは公式サーバーとの直接通信なので第三者境界の記録対象ではない）。event_log 以降の扱い（検索・Context Fusion）は第1層と完全に対称。

### 3-3. MCP が2〜3個の場合（Wave 1 フル接続）

- 各サービスは**独立した接続状態**を持つ。Calendar が connected で Gmail が amber、は普通に起こる
- 同期は ConnectorRuntime がサービスごとにスケジュール。1つの失敗が他を止めない
- LLM から見える差分は「利用可能ツールが増える」こと。組み合わせ判断（例：会議準備 → Calendar で予定 → Drive で資料 → Gmail で関連スレッド）はモデル側の呼び出し優先度設計で扱う（→ 本書 §5、詳細はリリース3で拡充）。**経路の違い（第1層/第2層）はモデルからは見えない** — 同意・認可はゲートが保証する

### 3-4. 書き込み・送信（全サービス共通の安全経路)

```
LLM が送信/書き込みを提案
  → scope.rs: この操作は表にあるか？ 承認レベルは？（外部送信は必ず L3）
  → L3 承認キューへ（approvals.rs）
  → ユーザーが設定ウィンドウの Approvals パネルで全文確認 → 確認 or 拒否
  → 確認後: send_bridge がルーティング
      - Gmail（ドラフト・送信）→ 第2層（Composio。オプトイン同意必須 +
        draft-stop 既定 ON = 下書き作成で停止。OFF は同意後のみ）
      - その他 → 第1層（公式 MCP）
  → トレーサビリティ記録（本文なし、ダイジェスト + byte 数。第三者経由はバッジ明示）
```

**人間の承認を通らない外部送信は構造上存在しない。**これが本アーキテクチャの最重要不変条件。

## 4. 第1層 / 第2層をなぜ分けたまま保つか

| | 第1層（公式 MCP 直結） | 第2層（Composio） |
|---|---|---|
| 経路 | ローカルアプリ → 公式サーバー | ローカルアプリ → Composio → サービス |
| 第三者へのデータ通過 | なし | **あり**（だからオプトイン + UI 明示が必須） |
| 用途 | Calendar / Drive（Wave 2/3 も）の読み取り同期・書き込み | **Gmail の読み取り・ドラフト・送信のすべて**（2026-07 決定。公式 MCP が Developer Preview で実接続確度が低いため） |
| 同意 | OAuth 同意（ユーザー → サービス直接） | **opt-in 3開示同意**（①第三者経由 ②データ種別 ③取り消し可能）。同意なしでは同期も送信もしない |
| トレーサビリティ | 送信・書き込みを記録 | **読み取りを含む全 egress を記録**（内容なし、ダイジェスト/フラグのみ） |
| 認可資格情報 | ユーザー直接 OAuth、トークンは Keychain | Composio API キー（Keychain）+ user id（非秘匿、設定 JSON）+ 同意フラグ |

第2層は「便利だから広げる」対象ではなく、「公式に**動く**能力が無い間の橋」。Gmail を第2層に置いたのは接続確度と回答品質を優先した明示的なトレードオフであり、**Gmail 公式 MCP が GA になれば読み取り/ドラフトを第1層へ戻す**のが方針。ドキュメント・UI では常に両者を区別して見せ、第2層であることを隠さない。

## 5. モデル（Claude）側から見た MCP 利用

> 現状、LLM へのツール定義・システムプロンプトへの MCP 一覧の結線は**未実装**（`shogun-core/src/llm/` はクライアントまで、`shogun-agents` は L1/L2 実行エンジンまで）。本節は「結線実装時にそのまま組み込めるテキスト設計」。

### 5-1. システムプロンプトに渡す「接続済みサービス」ブロック

原則：**接続済み（connected）のサービスだけを載せる**。未接続・amber・未リリース Wave のサービスは一覧に出さない — モデルに「繋がっていないものを呼ばせない」が唯一のシンプルな防線。接続状態が変わったら次ターンからブロックを再生成する。**Gmail が「connected」になるのは Composio の 3開示同意が完了した後だけ**なので、mail がこの一覧に載る = 同意済み、が常に成り立つ（同意状態の判定はプロンプトではなくゲートが行う）。

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
- **経路（第1層/第2層）はモデルに説明しない**。経路の開示はユーザーに対して UI が行う責務（`02-user-guide.md` §3・§5）であり、モデルの判断材料ではない
- **書き込み・送信の制約はプロンプトにも書く**（"sending always requires approval"）。ただし防御の本体はプロンプトではなく `scope.rs` + L3 ゲート + Composio 同意ゲート。プロンプトは期待値合わせ、ゲートが保証
- **未接続への言及ルール**を1行入れる：「繋げばもっとできる」をモデルが自然に案内できるようにする（`02-user-guide.md` §4 のマイクロコピーと対応）

### 5-2. LLM ツール定義との対応

Anthropic API の `tools` 配列に載せる単位は、**MCP サーバーの生ツールではなくハブ層の操作名**（`toolmap.rs` の左側、例：`list_calendar_events` / `search_mail` / `search_drive_files`）。理由：

1. 実 MCP のツール名は暫定で、サーバー側の都合で変わりうる（`tools/list` 突合待ち）。ハブの操作名を安定インターフェースにする。**Gmail が第1層 → Composio に経路変更されても、操作名インターフェースは不変** — この設計が経路変更のコストを toolmap の張り替えだけに抑えた
2. `scope.rs` の権限表が操作名単位なので、ツール呼び出し → 認可判定が1対1で素直につながる
3. モデルに見せる面を絞れる（表に無い操作はそもそもツール定義に載らない = 呼べない）

呼び出しの流れ（結線後）：

```
Claude が tool_use（例: list_calendar_events / search_mail）
  → service_gate: Wave 解放済み？ connected？ 権限表にある操作？
    （Gmail 系操作は Composio 同意済み？）
  → OK なら toolmap で実ツール名に変換
      - Calendar / Drive → RemoteMcpTransport → 公式 MCP
      - Gmail → HttpComposioApi → Composio（読み取り egress をトレーサビリティ記録）
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
| ツール呼び出しのトレーサビリティ記録 | `shogun-core/src/llm/traceability.rs` の Route に MCP 経路を追加。**Composio 経由の読み取りは必ず記録対象に含める** |

## 6. 未検証事項・移行残作業（区別して読むこと）

| 項目 | 状態 | 後続 Issue 候補 |
|---|---|---|
| **Gmail 経路のコード移行（第1層 → 全面 Composio）** — `endpoints.rs` の Gmail 第1層エントリ削除、`send_bridge.rs` のサービス単位ルーティング化、同意ゲートの読み取りへの拡張、読み取り egress トレーサビリティ | **未実装（2026-07 決定へのコード未追従）** | Gmail 移行 Issue（ライブ検証 Issue の前提） |
| 第1層ライブ接続（実クレデンシャル + 実 MCP + macOS 実機、Calendar / Drive） | 未検証（WP-G） | ライブ検証 Issue |
| `toolmap.rs` のツール名 | 第1層分は暫定（実サーバーの `tools/list` で確定待ち）。Gmail 分は Composio ツール slug への張り替えが必要 | ライブ検証 Issue に含める |
| `result.rs` のフィールド名 | tolerant 実装済みだが実レスポンス未確認（公式 MCP・Composio 双方） | 同上 |
| Composio Gmail の実エンドポイント（読み取り・ドラフト・送信） | 型は完成、実呼び出し未検証 | 同上 |
| Wave 2/3（Slack / Notion / GitHub / Linear） | コードあり・未テスト。安定性ゲート（人間判断）待ち | Wave 2 解放 Issue |
| Connections / Approvals パネルのスタイル・最終文言（3開示同意画面を含む） | 骨格実装済み・ラフ。同意画面は未実装 | UI 仕上げ Issue |
| オンボーディングでの MCP 提案フロー | 未実装（`02-user-guide.md` / `03-product-design.md` で定義 → 実装 Issue へ） | オンボーディング Issue |

## 7. Figma に起こす図（骨格メモ)

1. **アーキ図** — 本書 §1 の3層図。ボックス：クライアント（設定ウィンドウ / マイクロコピー）、ハブ相当（レジストリ / 接続状態 / ルーティング / 認可）、第1層サーバー群（Calendar / Drive）、第2層 Composio（Gmail 全面）。矢印に「OAuth 直接」「opt-in 3開示同意 + 第三者経由の明示」のラベル
2. **接続画面ワイヤー** — Connections パネル：サービス名 / できること / アクセス範囲（読み・書き）/ 経路（direct / via Composio）/ 状態バッジ（Connected / Amber / Coming soon）/ Connect・Disconnect ボタン
3. **3開示同意画面ワイヤー** — Gmail の Connect から遷移。3開示（第三者経由・データ種別・取り消し可能）の提示と個別の承諾、draft-stop の説明（既定 ON）
4. **オンボーディング簡易フロー** — 説明 → 推奨1〜2個提示 → 認可（Calendar/Drive = OAuth、Gmail = 3開示同意）→ 完了、の4ステップ（`02-user-guide.md` で定義）
