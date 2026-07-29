# Issue #59 からの後続 Issue ドラフト（3本）

> このファイルは GitHub Issue 化するためのドラフト。作成したら各 Issue へのリンクをここに追記し、本文はこのファイルではなく Issue を正とする。

---

## Issue A: Wave 1 ライブ接続検証 — Calendar / Gmail / Drive の実 MCP 接続を通す

**Why**: 接続レイヤーは実装済みだが、実クレデンシャルでの検証が未実施（WP-G）。ツール名・レスポンス形式が暫定のままでは、この上に乗る全機能（LLM 結線・UI）が確定できない。

**What**:
- GCP OAuth クライアント作成（`docs/oauth-client-setup.md`、Testing mode）
- Calendar → Gmail → Drive の順で end-to-end 検証（OAuth → Keychain → `tools/list` → 同期 → event_log 着地）
- 実サーバーの `tools/list` で `toolmap.rs` のツール名を確定、実レスポンスで `result.rs` を検証
- Composio Gmail 送信の実エンドポイント検証（オプトイン + draft-stop + L3 経路）
- amber 遷移 → 再認可 → 復帰の実機確認

**参照**: `docs/mcp/04-dev-implementation.md` §2-A、`docs/connector-summary-and-live-checklist.md`

**Done の定義**: Wave 1 の3サービスが実機で接続・同期でき、toolmap/result の暫定マークが外れている。

---

## Issue B: LLM 結線 — 接続済み MCP を Claude のツールとして使えるようにする

**Why**: 接続レイヤーと LLM クライアントは両方あるが、間の結線（ツール定義生成・会話ループ）が無い。これが無いと「カレンダーを理解した回答」というプロダクトの核が動かない。

**What**:
- 接続状態 → `tools` 配列生成（ハブ操作名を安定 IF に。`anthropic.rs` request builder 拡張）
- 「Connected services」システムプロンプトブロックの機械生成（接続済みのみ掲載）
- tool_use → `service_gate` → `toolmap` → transport → tool_result の会話ループ（`shogun-agents`）
- 読み取り以外を L1/L2/L3 エンジンへルーティング
- `traceability.rs` に MCP 経路を追加、ツール呼び出しイベントの UI 通知

**参照**: `docs/mcp/01-architecture.md` §5、`docs/mcp/04-dev-implementation.md` §2-B

**依存**: Issue A（ツール名確定後が望ましい。設計・モック実装は並行可）

**Done の定義**: Calendar 接続済みの状態で「明日の予定は？」に実データで答えられる。承認なしの外部送信経路が存在しないことをテストで保証。

---

## Issue C: オンボーディング & Connections UI 仕上げ — 接続体験の実装

**Why**: UI は骨格実装済みだがラフ。オンボーディングの MCP 提案・設定画面の項目・マイクロコピーが `docs/mcp/02-user-guide.md` で定義されたので、実装に落とす。

**What**:
- オンボーディング「Connect your work」改訂：Drive 行追加、推奨順（Calendar → Mail → Drive）、スキップ文言
- Connections パネル拡充：アクセス範囲バッジ / 最終同期時刻 / Disconnect 確認 / amber の Reconnect 導線
- Composio 設定の別枠化（第1層の行と混ぜない）
- 接続直後のサンプル質問提示（アハ・モーメント、`03-product-design.md` §3）
- 会話中マイクロコピー表示（Issue B のイベントを購読）
- 計測イベントの発火（接続率・接続深度・接続→初回利用。`03-product-design.md` §4。内容は取らず接続状態とイベント種別のみ）

**参照**: `docs/mcp/02-user-guide.md`、`docs/mcp/03-product-design.md`

**依存**: マイクロコピー表示のみ Issue B に依存。他は並行可。

**Done の定義**: 新規ユーザーがオンボーディングから Calendar + Mail を接続し、Settings で状態確認・解除までできる。
