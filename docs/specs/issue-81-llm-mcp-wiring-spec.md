# 仕様たたき: LLM 結線 — 接続済み MCP を Claude のツールとして使えるようにする

> Issue #81 / P0。設計正本: `docs/mcp/01-architecture.md` §5, `docs/mcp/04-dev-implementation.md` §2-B
> ステータス: たたき台（レビュー前提のドラフト）

## 1. 背景 / Why

接続レイヤー（shogun-mcp / shogun-integrations）と LLM クライアント（shogun-core/src/llm）は双方完成しているが、**間の結線（ツール定義生成・会話ループ）が存在しない**。これが無いと「カレンダーを理解した回答」というプロダクトの核が動かない。#80 でツール名確定後が望ましいが、ハブ操作名を安定 IF とする設計（01-architecture §5-2）により**設計・実装・モックテストは並行可能**。

## 2. ゴール / Done の定義

1. Calendar 接続済みの状態で「What's on my calendar tomorrow?」に実データで答えられる
2. **承認なしの外部送信経路が存在しないことをテストで保証**（`tests/invariant4.rs` 拡張）
3. ツール呼び出しが traceability に記録され、UI にイベントが流れる（#82 のマイクロコピーが購読できる）

## 3. スコープ

**In:** ツール定義生成 / システムプロンプトブロック生成 / 会話ループ / L1-L3 ルーティング / traceability / UI イベント発火
**Out:** UI 側のマイクロコピー表示（#82）、L3 送信 UI 自体（既存 Approvals）、Wave 2/3 サービス、ストリーミング最適化以外のレイテンシ改善

## 4. 機能仕様

### 4-1. `tools` 配列の生成（`anthropic.rs` request builder 拡張）

- 単位は**ハブ操作名**（`toolmap.rs` 左側: `list_calendar_events` / `search_mail` / `search_drive_files` 等）。実 MCP ツール名は絶対に LLM に見せない
- 生成条件: `connection.rs` FSM が `connected` かつ `scope.rs` 権限表に載っている操作のみ。amber / disconnected / 未リリース Wave の操作は**ツール定義自体を載せない**（呼べない = 最初の防線）
- 各ツールに JSON Schema を付与。Schema は `scope.rs` 近傍に静的定義し、操作追加時に権限表とセットで書く（表に無い操作は Schema も存在しない構造にする）
- 接続状態変化（connect / disconnect / amber 遷移）で次ターンから再生成。ターン中の変化は次ターン反映で良い（ターン内の再生成はしない）

### 4-2. 「Connected services」システムプロンプトブロック

- `connection.rs` の接続状態から機械生成する小関数を新設（場所: ハブ層。文字列テンプレートは `01-architecture.md` §5-1 の英語テンプレを正とする）
- 接続済みサービスのみ掲載。役割ベースの説明（"calendar = 予定・空き時間"）とし、ツール名は列挙しない
- 制約文（"sending always requires the user's explicit approval"）を必ず含める。ただし**防御の本体はプロンプトでなく gate**（プロンプトは期待値合わせ）
- 未接続サービスへの誘導1行（"say so briefly instead of guessing"）を含める

### 4-3. 会話ループ（`shogun-agents` に新設）

```
user message
  → request builder（tools + Connected services block）
  → Claude 応答をストリーミング処理
  → tool_use 検出:
      read 系 → service_gate（Wave 解放? connected? 権限表にある?）
                → toolmap → transport → result.rs 正規化
                → tool_result として返却しループ継続
                ※ Calendar/Drive = RemoteMcpTransport（公式MCP直結）。
                  Gmail = Composio transport（全面Composio化決定）。mail 系操作は
                  3開示同意ゲートを通過済みの場合のみツール定義に載せる（未同意 = 未接続扱い）
      write/send 系 → 即実行しない。L1/L2/L3 エンジン（engine.rs）へ提案として流す
                → tool_result には「queued for approval」等の状態を返す（結果を捏造しない）
  → 最終応答
```

- **ループ上限**: 1ターンあたり tool_use 最大 N 回（初期値 8）。超過時は「ここまでで分かったこと」を返させる（無限ループ・トークン浪費の防止）
- **タイムアウト**: 個々のツール呼び出しに上限（初期値 10s）。超過は tool_result にエラーを返しループ継続（会話全体を殺さない）
- **並列 tool_use**: v1 は逐次実行で開始（実装単純化）。SLO 上問題が出たら並列化を別Issueで
- **キー分離（不変条件5）**: 会話ループは**ユーザー BYOK のみ**。Select KK キー（Batch）経路にツール定義を混ぜない。request builder の型レベルで分離できるとなお良い

### 4-4. L1/L2/L3 ルーティング

- 読み取り以外の tool_use はすべて `shogun-agents/src/engine.rs` の既存承認フローへ
- **L1 に外部送信系を絶対に含めない**（不変条件4）。ルーティング表は `scope.rs` の権限表から導出し、コード内の特例分岐を作らない
- L3 送信経路は M4 スケジュールに従い、解放前は「キュー投入まで」で止まる

### 4-5. トレーサビリティ / UI イベント

- `traceability.rs` の `Route` に `Mcp { service, operation }` を追加。記録は**ダイジェスト + byte 数のみ、本文なし**
- UI 通知イベント（開始/終了/失敗、service と operation 種別のみ）をイベントバスに発火。#82 が購読
- テレメトリに乗せる場合も内容ゼロ・イベント種別のみ（CLAUDE.md テレメトリ規約）

## 5. テスト計画

| テスト | 層 | 内容 |
|---|---|---|
| ツール定義生成 | unit (Linux) | 接続状態×権限表の組み合わせで、載るべき操作だけが載る／amber で消える |
| プロンプトブロック生成 | unit | 接続0/1/3 での出力スナップショット |
| 会話ループ | unit + mock transport | tool_use → gate 拒否 / 成功 / タイムアウト / ループ上限 |
| **invariant4 拡張** | unit | 「write/send 系 tool_use が承認キューを経ずに transport へ到達する経路が型上存在しない」ことを網羅（最重要） |
| キー分離 | unit | Batch 経路の request に tools が載らないこと |
| E2E | 実機（#80 後） | Calendar 実接続で「明日の予定は？」→ 実データ回答、traceability 記録確認 |

## 6. SLO / 受け入れ計測

- アクション実行→初トークン 1s（ストリーミング必須）は**ツール呼び出し前の初トークン**に適用。ツール実行中はマイクロコピー（#82）で体感を埋める
- ツール1回あたりの p50/p95 レイテンシ計測コードを同梱し、PR 本文に計測結果を貼る（CLAUDE.md 規約）

## 7. リスク

| リスク | 構え |
|---|---|
| ツール結果が大きく context を圧迫 | result.rs 正規化後に上限バイトで truncate + 「続きは検索で」の設計。#63（コンテキスト圧縮）と接続 |
| モデルが未接続サービスを幻覚呼び出し | 定義に無いツールは API レベルで invalid → tool_result にエラーを返す。頻発するならプロンプト調整 |
| #80 遅延 | mock transport で全ユニットテストまで完了させ、実機 E2E のみ #80 待ちにする |

## 8. 実装ステップ（PR 分割案）

1. `feat(llm): tools 配列生成 + Connected services ブロック生成`（純ロジック、Linux テスト） — **済 2026-08-15**（`shogun-mcp/src/tool_catalog.rs`）

   実装時に確定した設計判断:
   - **権限表は変更しない**。ハブ操作名 → `(Service, scope op)` の束縛で足り、calendar のライブ読み取りは既存の `read_sync` 行（`toolmap.rs` で `list_events` に解決済み）が担う。表に無い操作を新設せずに済んだ
   - **フィルタは `service_gate::authorize_op` を再利用**する（条件を書き写さない）。定義集合が「ゲートが許す集合の部分集合」であることが構造的に保証され、両者がドリフトしない
   - **本 PR は read 系のみ**を載せる。write/send をルーティング（step 3）より先に定義へ載せることは不変条件4が禁じる事故そのもの。`tests/invariant4.rs` に「wave × plan × conn の全組み合わせで、載るツールは必ず `OpClass::Read`」を追加
   - Gmail の3開示同意は呼び出し側が `ConnState::Disconnected` に畳んで渡す（未同意 = 未接続扱い）。同意判定を読み手ごとに書き直さないための単一決定点
2. `feat(agents): 会話ループ（read 系のみ、mock transport）` — **済 2026-08-15**（`shogun-mcp/src/tool_loop.rs`）

   実装時に確定した設計判断:
   - **配置は `shogun-agents` ではなく `shogun-mcp`**。依存が `shogun-mcp → shogun-agents` の一方向であり、ループはゲート・カタログ・権限モデルを同時に見る必要があるため、`shogun-agents` に置くと循環する。`tests/invariant4.rs` を shogun-mcp に置いてあるのと同じ理由
   - **同期 + シーム駆動**。`ModelTurnSource` と `ReadToolRunner` の2トレイトだけに依存する純粋なオーケストレーションなので、予算・拒否・タイムアウト・停止性のすべてが Linux でランタイム無しにテストできる。async 境界は Dream Cycle と同じくシェル側（`rt.block_on`）に置く
   - **停止性はループの性質**として保証する。ツール予算だけでは停止しない（予算切れ後もモデルがツールを要求し続けられる）ため、モデルターン数の backstop（`MAX_TOOL_USES + 3`）を別に持つ
   - **拒否は予算を消費しない**。拒否はサービス往復を発生させていないので、混乱したモデルがデータに到達する前にターンを使い切ることを防ぐ
   - **失敗結果を捏造しない**。タイムアウト・トランスポート失敗はいずれも `is_error: true` の tool_result として返す（モデルがそれを土台に回答を組み立てるのを防ぐ）
   - 未記述サービスは `Disconnected` 扱い（absent ≠ permissive）
3. `feat(agents): write/send 系の L1-L3 ルーティング + invariant4 テスト拡張`
4. `feat(core): traceability Route::Mcp + UI イベント発火`
5. `test(e2e): 実機 Calendar 結線検証`（#80 完了後）
