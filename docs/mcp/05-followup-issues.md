# Issue #59 からの後続 Issue ドラフト（3本）

> このファイルは GitHub Issue 化するためのドラフト。**Issue 化済み**：
> - Issue A → [#80](https://github.com/Torutesu/ShogunAI-/issues/80)
> - Issue B → [#81](https://github.com/Torutesu/ShogunAI-/issues/81)
> - Issue C → [#82](https://github.com/Torutesu/ShogunAI-/issues/82)
>
> ⚠️ **2026-07-30 注記**：#80〜#82 は Gmail 全面 Composio 化決定（CLAUDE.md「連携実装ルール」）の反映**前**の本文で作成されている。特に #80 の「GCP OAuth で Gmail end-to-end」は決定により無効な作業。**各 Issue の本文を本ドラフト（改訂版）に合わせて更新するまで、本ファイルの記述を正とする。**

---

## Issue A: Wave 1 ライブ接続検証 — Calendar / Drive（第1層）+ Gmail（Composio）の実接続を通す

**Why**: 接続レイヤーは実装済みだが、実クレデンシャルでの検証が未実施（WP-G）。ツール名・レスポンス形式が暫定のままでは、この上に乗る全機能（LLM 結線・UI）が確定できない。さらに **Gmail 経路のコードは旧設計（読み取り＝第1層）のまま**で、2026-07 決定（Gmail 全面 Composio 化）への移行が先に必要 — 移行せずに検証すると旧経路を検証してしまう。

**What**:
- **【前提】Gmail 経路のコード移行**（`docs/mcp/04-dev-implementation.md` §2-A0）：
  - `endpoints.rs` の Gmail 第1層エントリ（`gmail.readonly` / `gmail.compose`）削除、`send_bridge.rs` のサービス単位ルーティング化
  - 同意ゲート（3開示）を読み取り同期を含む全 Composio 操作の前提条件に拡張（同意なし = 外部通信ゼロをテストで保証）
  - 読み取り egress のトレーサビリティ記録（本文なし、ダイジェスト/フラグ + 第三者経由フラグ）
- GCP OAuth クライアント作成（`docs/oauth-client-setup.md`、Testing mode）— **Calendar / Drive のみ**（Gmail は Google Cloud OAuth 不要）
- Calendar → Drive の順で第1層 end-to-end 検証（OAuth → Keychain → `tools/list` → 同期 → event_log 着地）
- 実サーバーの `tools/list` で `toolmap.rs` のツール名を確定、実レスポンスで `result.rs` を検証
- **Gmail は Composio API で end-to-end 検証**：API キー（Keychain）+ user id → 3開示同意 → 読み取り同期 → event_log 着地 → 読み取り egress 記録の確認。続けてドラフト・送信（同意 + draft-stop 既定 ON + L3 経路ごと）
- amber 遷移 → 再認可 → 復帰の実機確認（OAuth 系・Composio 系の両方）

**参照**: `docs/mcp/04-dev-implementation.md` §2-A0/§2-A、`docs/connector-summary-and-live-checklist.md`（Gmail 節は要改訂）、CLAUDE.md「連携実装ルール」

**Done の定義**: Wave 1 の3サービスが実機で接続・同期でき（Calendar / Drive = 直結、Gmail = Composio）、toolmap/result の暫定マークが外れている。Gmail は同意なしで egress ゼロ・draft-stop 既定 ON・読み取り egress 記録あり、がテストと実機の両方で確認済み。

---

## Issue B: LLM 結線 — 接続済み MCP を Claude のツールとして使えるようにする

**Why**: 接続レイヤーと LLM クライアントは両方あるが、間の結線（ツール定義生成・会話ループ）が無い。これが無いと「カレンダーを理解した回答」というプロダクトの核が動かない。

**What**:
- 接続状態 → `tools` 配列生成（ハブ操作名を安定 IF に。`anthropic.rs` request builder 拡張）
- 「Connected services」システムプロンプトブロックの機械生成（接続済みのみ掲載。Gmail は Composio 同意完了 = connected の場合のみ載る）
- tool_use → `service_gate` → `toolmap` → transport → tool_result の会話ループ（`shogun-agents`）。Gmail 系操作は第2層 transport 経由 + 読み取り egress 記録
- 読み取り以外を L1/L2/L3 エンジンへルーティング
- `traceability.rs` に第1層 MCP 経路と Composio 経路（読み取り含む）を追加、ツール呼び出しイベントの UI 通知

**参照**: `docs/mcp/01-architecture.md` §5、`docs/mcp/04-dev-implementation.md` §2-B

**依存**: Issue A（ツール名確定後が望ましい。設計・モック実装は並行可）

**Done の定義**: Calendar 接続済みの状態で「明日の予定は？」に実データで答えられる。承認なしの外部送信経路が存在しないこと、および Composio 同意なしに Gmail 系 tool_use が実行されないことをテストで保証。

---

## Issue C: オンボーディング & Connections UI 仕上げ — 接続体験の実装

**Why**: UI は骨格実装済みだがラフ。オンボーディングの MCP 提案・設定画面の項目・マイクロコピーが `docs/mcp/02-user-guide.md` で定義されたので、実装に落とす。Gmail 全面 Composio 化により、**3開示同意シートと経路の明示表示が必須要件**になった（CLAUDE.md）。

**What**:
- オンボーディング「Connect your work」改訂：Drive 行追加、推奨順（Calendar → Mail → Drive）、**改訂 lead 文**（「すべて直結」と読める旧文言の撤去。Calendar/Drive = 直結、Mail = via Composio を正確に言う）、Mail 行の "via Composio, a third-party service" 常設サブラベル、スキップ文言
- **Gmail 3開示同意シート**（`02-user-guide.md` §2-3'）：3開示の個別承諾 → `grant_consent` → Composio 接続。同意なしでは接続も同期も発生しない
- Connections パネル拡充：アクセス範囲バッジ / 最終同期時刻 / Disconnect 確認 / amber の Reconnect 導線
- Mail 行の Composio 表示（**別枠化ではなく**同一リスト内で視覚的に区別。詳細に同意状態 / draft-stop トグル（既定 ON）/ 同意取り消し）
- トレーサビリティ画面の「第三者経由」バッジ（Mail の読み取り記録を含む）
- 接続直後のサンプル質問提示（アハ・モーメント、`03-product-design.md` §3）
- 会話中マイクロコピー表示（Issue B のイベントを購読）
- 計測イベントの発火（接続率・接続深度・**Gmail 読み取り同意率（同意シートの到達/承諾/離脱点）**・接続→初回利用。`03-product-design.md` §4。内容は取らず接続状態とイベント種別のみ）

**参照**: `docs/mcp/02-user-guide.md`、`docs/mcp/03-product-design.md`、CLAUDE.md「連携実装ルール」

**依存**: 同意シート〜同期開始の貫通は Issue A の Gmail 経路移行に依存。マイクロコピー表示のみ Issue B に依存。他は並行可。

**Done の定義**: 新規ユーザーがオンボーディングから Calendar（OAuth 直結）+ Mail（3開示同意 → Composio）を接続し、Settings で状態確認・draft-stop 操作・同意取り消しまでできる。Mail の経路（第三者経由）が接続前・接続後・トレーサビリティのすべての画面で明示されている。
