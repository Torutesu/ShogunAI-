# コンテキスト・アーキテクチャ設計 — 取得・評価・蓄積・文脈化の再設計

対象: capture / API / MCP から入るあらゆるコンテキストの「取得 → 評価 → 蓄積 → 文脈化」の全体設計と、
UI での見せ方。上位文書は `docs/requirements-v1.0.md`（正本）と `CLAUDE.md`（不変条件）。
本書は「別チャットで実装する」ための設計・判断記録であって、実装の完了報告ではない。

---

## 0. 結論（先に要点）

1. **既存アーキテクチャは既に「ソース対称」**。AX キャプチャは `source=capture` という一ソースにすぎず、
   Gmail/Slack 等の同期は `Db::ingest_integration`（`daemon.rs:176`）で**同じ event_log に source タグ付きで入り、
   同じ抽出（`extract`）を通って同じ state tables に落ちる**。つまり「API/MCP でも取れるようにする」は
   *再設計ではなく、実トランスポートと UI と抽出品質の3つを埋める作業*。
2. **足りているもの**: 権限スコープ表(`scope.rs`)・Composio ゲート(`composio.rs`)・接続FSM(`connection.rs`)・
   同期合成(`sync.rs`)・MCPサーバ面(`mcp.rs`)・REST(`shogun_api.rs`)・トレーサビリティ(`traceability.rs`)は
   **実装済み＆テスト済み**。型・ゲート・プロトコルは揃っている。
3. **足りていないもの（＝次チャットの仕事）**:
   - **実トランスポート**: `IntegrationTransport` / `SendTransport` の実装が **テスト用 Fake しか無い**
     （`sync.rs:151`, `send_exec.rs:115`）。公式リモートMCPクライアント（OAuth→Keychain）が未実装。
   - **UI**: Sources（接続管理）画面・Traceability ビューア・コンテキスト根拠(provenance)表示が無い。
   - **抽出の質**: 現状は **cue 一致のローカル規則のみ**、confidence 上限 0.4（`extract.rs:27`）。
     モデル分類（Dream Cycle / Batch API）が capture 経路に**未配線**なので、state は「〜かも」以上に上がれない。
4. **最適化の芯**は §4。「二段抽出（ローカル即時＋Batch精緻化）」「AXObserver への一本化」
   「埋め込みベースの関連度」の3つが、ユーザー体験（ドラフトの的中率・レイテンシ・CPU）を最も動かす。

---

## 1. 現状マップ（grounded）

### 1.1 キャプチャ — **2経路が並走している**

| 経路 | 実体 | 周期 | 行き先 |
|---|---|---|---|
| **メモリ経路**（永続） | `capture_source::spawn_capture_poller`(`capture_source.rs:91`) → `Db::ingest_capture`(`daemon.rs:129`) | 2s(`DEFAULT_POLL_MS`) | event_log + 抽出 |
| **ライブ文脈経路**（一時・表示専用） | `integrate::spawn_focus_watcher`(`integrate.rs:542`) → `axcache::snapshot` → `context` イベント | 400ms 監視 / ~2s 再walk | webview 表示のみ（**DBに書かない**） |

- AX 読取は `axcache.rs` に集約。`AxElement::value_text`(`axcache.rs:172`)が **AXValue → AXTitle → AXDescription** の順。
  収集ロール: `AXStaticText/AXTextArea/AXTextField/AXHeading/AXLink/AXCell`。`AXSecureTextField` は subtree ごと除外。
  BFS・深さ≤8 / ≤300要素 / ≤32KB、250msタイムボックス、AXメッセージ100msタイムアウト。**画像は一切取らない**（不変条件2）。
- 除外は capture の**前段**（`pipeline.rs:51`）: パスワードマネージャ8種・SecurityAgent・**ターミナル8種**（スクロールバックが
  ゴミ commitment を生むため）。プライベートブラウズは既知ブラウザ＋タイトル語のヒューリスティック。ユーザー追加可・既定は削除不可。
- **AXObserver（push型フォーカスイベント）は未実装**。両経路とも「確実なフォールバック」としてポーリング（`capture_source.rs:9-11`）。

### 1.2 評価 / 抽出 — **ローカル規則のみ**

- `extract(text)`(`extract.rs:136`)は **モデルなし・正規表現なし**。`\n .!?` で分節→小文字化→cue表照合。
  cue: `MINE_CUES`("i'll "/"i will "…) / `THEIRS_CUES` / `WAITING_CUES` / `REVIEW_CUES` / `DECISION_CUES` …。
- confidence はルール毎の固定値（Mine=0.35, Waiting=0.35, …）で、**上限 `LOCAL_RULE_MAX_CONFIDENCE=0.4`**。
  これは Medium 帯の閾値 0.5 を意図的に下回らせ、ヒューリスティック候補を**事実として断定させない**ため（FR-ST-20）。
- 永続時に各候補は根拠イベントへ `Provenance::new(event_id)` でリンク（`extract.rs:190`）。
  **provenance 空の state 行は挿入時に拒否**（`state.rs:76` `EmptyProvenance`）——不変「根拠なき状態を作らない」。
- **モデル分類（第2段）は Dream Cycle(`dreamcycle/`) に先送りで、capture 経路に未配線**（`extract.rs:4-6`）。
  confidence 減衰・overdue 再計算は実装済み（`recompute.rs`）。

### 1.3 蓄積 — event log と state を物理分離、3層メモリ

- refinery マイグレーション（現在 v4）。WAL / `foreign_keys=ON` / state は STRICT。
- `event_log`: 追記のみ。`source ∈ capture/gmail/gcal/slack/notion/github/linear/agent/user`。
  **空間対応カラム**（`display_id/window_bounds/window_pose/gaze_target`）を V1 から確保（v1 は NULL、後付け禁止）。
- `people/projects/commitments/open_loops`: 各行に `confidence`（CHECK 0..1）+ `last_evidence_at`。
- `state_provenance`(多対多) / `traceability_log`(本文カラム無し)。
- **FTS5 trigram**（content+title, bm25）。**sqlite-vec `event_vec float[384]`（e5-small, Warm 総当り）は実装済みだが埋め込み投入は write 経路外**。
- 3層: **Hot**(24h/RAM, 200MB上限, `hot.rs`) / **Warm**(SQLite本体) / **Cold**(30日超, int8量子化, `cold.rs`)。
- dedup: `insert_or_touch`（`content_hash+source`）で再出現は `last_seen_at`更新＋`dwell_ms`加算。
  近似重複は Sørensen–Dice(bigram, 閾値0.98, 日本語対応)。**dedup touch は抽出をスキップ**（重複が候補を増やさない）。

### 1.4 文脈化 / Fusion — `f(state, screen_ctx, intent) → cache`

- `assemble`(`assemble.rs:126`): ランク = `relevance × band_weight(confidence)` + intent ヒント 0.5。actions は重複除去し **最大4**。
- **confidence ゲートが唯一の絞り**(`confidence.rs`): High≥0.8 / Medium≥0.5 / Low<0.5。Low は**破棄・本文も持たせない**、
  Medium は `possibly:` 接頭、High は素通し。**Low は action を提案できない**。
- action マップ(`assemble.rs:96`): v1 は **LocalAction(L1/L2)のみ・send は絶対に出さない**。空パネル禁止の
  fallback（Save note / Search memory / Extract tasks）。
- `Db::context_actions`(`daemon.rs:406`)が state を StateCandidate 化し、`screen_relevance`（**タイトル語の部分一致**, `daemon.rs:742`）で
  関連度を付けて assemble。
- インライン下書き/チャット: `compose_inline`(`inline.rs:115`) / `shogun_chat`(`inline_source.rs:514`)。
  memory は `Db::inline_memory`(`daemon.rs:238`)が集め、**confidence ゲートを通してからモデルへ**（低確度は届かない）。
  BYOK レーン・鍵は Keychain のみ・egress は1回だけトレース。

### 1.5 MCP / Composio の現状（型は本物・実トランスポートは Fake）

- **本物**: `scope.rs`(6サービス3波の権限表, `ExternalSend`は必ずL3 or Composio) / `composio.rs`(コンパイル時ゲート,
  3開示で consent, draft-stop OFF で初めて送信能力, 常にL3+第三者バッジ) / `mcp.rs`(JSON-RPC 2.0, 13ツール, send はL3 pending) /
  `shogun_api.rs`(axum localhost:7464, bearer) / `sync.rs`(`collect_sync`: authorize→read_sync→normalize→source付きIngestItem) /
  `connection.rs`(connect/sync/amber/disconnect FSM, サービス間隔離)。
- **Fake のみ（未配線）**: `IntegrationTransport`（実装は test `Fake`, `sync.rs:151`。**実リモートMCP/OAuth クライアント無し**）/
  `SendTransport`（`send_exec.rs:115`）。抽象は本物、具体コネクタが不在。

### 1.6 トレーサビリティ

- 外部に出る全チャンクが**ちょうど1件** `TraceRecord`（`route/purpose/destination/chunk_bytes/chunk_xxh64/third_party`のみ、
  **本文は保存せず digest だけ**）。route: `batch_api`(Select KK) / `messages_api`(BYOK) / `mcp` / `composio`(常に第三者) / `billing`。
- 保存側 `traceability_log` は**本文カラムを持たない**。送信は**成功時のみ**行を書く（失敗は何も残さない）。

---

## 2. 設計①: API/MCP でのコンテキスト取り込み（つなぎ込み）

### 2.1 統一ソースモデル（既にある設計を言語化）

> **原則**: 「コンテキストを生むものはすべて Source」。AX は `source=capture` の1ソース。新ソース追加＝
> ①`IntegrationTransport` 実装 ②`connection.rs` FSM に載せる ③`ingest_integration` に流す、の3点だけ。
> event_log・抽出・state・Fusion・confidence ゲートは**一切変えない**（対称性が壊れない）。

取り込みフロー（読み取り。egress しないのでトレース行は書かない）:

```
[remote MCP / API]  --read_sync-->  IntegrationTransport::read_sync
   -> normalize() -> Vec<IngestItem>{source, external_id, ts, title, body, ...}
   -> Db::ingest_integration(daemon.rs:176)   // source タグ付きで event_log へ、dedup + extract
   -> state tables（capture と全く同じ経路）
```

### 2.2 第1層 = 公式リモートMCP直結（OAuth はユーザー→サービス直接）

- `IntegrationTransport` の本実装 = **リモートMCPクライアント（Category C, OAuth 必要）**。
  OAuth トークンは **Keychain のみ**（BYOK と同じく per-service account、例 `gmail-oauth` / `gcal-oauth`）。平文ファイル/DB/ログ禁止（不変条件7）。
- 波の順（`scope.rs` の定義どおり）: **Wave1: Gmail + Google Calendar → Wave2: Slack → Wave3: Notion + GitHub + Linear**。
- **鍵レーンの分離を厳守**（不変条件5）: インデックス・分類・Dream・Brief = **Select KK（Batch API）**。
  エージェント推論・チャット・ドラフト = **ユーザー BYOK**。同期の読み取り自体は egress しないので鍵レーンに載らない。
- **バックグラウンド同期スケジューラ**: capture poller と同型の、ソース毎の周期スレッド。
  `connection.rs` の FSM（sync/amber(reauth)/disconnect）に接続し、失敗は1サービスに隔離（他を止めない）。freshness を保持。
- Slack: WS 管理者承認で接続不可なら **ドラフト→クリップボード** にフォールバック（要件どおり）。

### 2.3 第2層 = Composio（オプトイン・v1 は Gmail 送信のみ）

- `composio.rs` のゲートは本物。**足りないのは実 HTTP クライアントと consent UI**。
- 不変: 送信は**必ず L3**、`Route::ViaComposio`、**第三者バッジ**必須。`prepare_send` は SendAction+Preview を作るだけで、
  実 egress は confirmed 後。失敗は `FailedDraftSaved`（勝手に「送った」にしない）。
- 「ドラフト止まりモード」設定を必ず用意（既定 ON）。

---

## 3. 設計②: UI での見せ方

### 3.1 Sources（接続管理）画面 — 設定内の新セクション

各サービスをカードで:

- **状態ドット**: connected / syncing / **amber(再認証要)** / disconnected（`connection.rs` FSM の状態を直に反映）。
- **鮮度**: `last synced 3m ago`（freshness）。
- **スコープバッジ**: Wave1 は「read-only」明示。送信可能なのは Composio 経由の Gmail のみ。
- **第三者バッジ**: Composio 経由サービスは「第三者経由」を明示（不変・要件のトレーサビリティUI）。
- 操作: connect（OAuth 起動）/ disconnect / reauth / **draft-stop トグル**（Gmail）。

### 3.2 Traceability ビューア — 「生データが出た箇所」を一覧

- `traceability_log` を route/destination で絞り込み（most-recent-first）。**本文は無く digest と bytes のみ**なのでそのまま安全に見せられる。
- `third_party=true`（Composio）行を強調。これが「クラウドに何がどれだけ出たか」のユーザー可視化。
- REST は既に `/v1/...` があるので、まず**既存バックエンド上で UI フレームだけ**先行実装できる（新トランスポート不要）。

### 3.3 コンテキスト根拠（provenance）の提示 — 信頼の要

- SHOGUN がドラフト/アクションを出すとき、「**なぜ？**」アフォーダンスで **根拠イベント（provenance）＋ confidence 帯**を表示。
- 既にクリック可能にした `state__row` を延長: 行を押すと resolve、長押し/副操作で「根拠を見る」。
- confidence 帯を可視化（High=断定 / Medium=「possibly」/ Low=そもそも出さない）。ユーザーが**なぜこの提案かを追える**ことが体験の芯。

### 3.4 ライブソース表示

- ノッチの `reading {App}` を「今どのソースから文脈を取っているか」に一般化。同期が新規コンテキストを引いた瞬間を
  インジケータ色で軽く知らせる（作業を中断させない・不変「エラーは色で通知」）。

---

## 4. 設計③: AX キャプチャの「評価・蓄積・文脈化」の最適化 ★核心

現状の弱点（§1）に対する、体験インパクト順の再設計。

### 4.1 【最重要】二段抽出 — ローカル即時 ＋ Batch 精緻化

**問題**: 抽出が cue 一致のみで上限 0.4。**何も 0.5 を超えられない＝パネルは永遠に「possibly」しか言えない**。
モデル分類（Dream Cycle）が未配線。

**提案**: 抽出を2段に分ける。

- **Tier 0（現状維持・write 時・低コスト）**: `extract` のローカル規則を**候補ゲート**として残す。
  capture のレイテンシをゼロに保つ（write 経路でモデルを呼ばない）。
- **Tier 1（Batch API・Select KK・アイドル/夜間）**: Dream Cycle が直近イベントを再読し、
  **構造化された** commitment/open_loop を生成 — **due date のパース**、subject/**people・projects への entity linking**、
  **較正された confidence**（0.5 を超えて「事実」になれる）。provenance は複数根拠で weight 加算。
  鍵は **Select KK（Batch）**、BYOK ではない（不変条件5）。egress トレースは `batch_api` route。

**効果**: state が「事実」帯に上がれるようになり、Morning Brief・Fusion・ドラフトの断定度と的中率が跳ね上がる。
**これが単独で最大の UX レバー**。

### 4.2 キャプチャ経路を AXObserver に一本化

**問題**: メモリ経路(2s poll)とライブ経路(400ms poll)が二重にポーリング。AXObserver 未実装で、アイドル CPU（SLO 5%）と
フォーカス切替レイテンシ（cache 更新 SLO 300ms）に不利。

**提案**: `AXObserver`（`AXFocusedWindowChanged` / `AXTitleChanged`）による **push 型**へ移行、ポーリングはフォールバックに降格。
**1回の walk で** ライブ表示（一時）と永続 ingest の両方を賄い、digest で dedup。AX 真実は既に `axcache.rs` に集約済みなので
差し替え点は限定的。**効果**: アイドル CPU 低下（不要な walk 消滅）、フォーカス切替→cache 更新が push で 300ms SLO に寄る。

### 4.3 関連度を埋め込みベースへ（screen_ctx ⇄ state の意味一致）

**問題**: `screen_relevance`（`daemon.rs:742`）が**タイトル語の部分一致**。表記ゆれ・言い換え・多言語に弱い。

**提案**: 既にある `event_vec`(384-dim e5-small, Warm)を使う。現在の screen_ctx を**ローカル ONNX で埋め込み**、
state 候補（の根拠イベント埋め込み）と**コサイン類似**でランク。`assemble.rs` の `relevance × band_weight` の枠はそのまま、
`relevance` の質だけ上げる。**クラウド embedding は使わない・オフライン・追加限界費用ゼロ**（不変条件・技術スタック）。
併せて `dwell_ms`（既に加算済み）+ recency を salience の事前分布に使い、「見ていた画面」を優先。

### 4.4 Entity linking（people/projects を実際に埋める）

**問題**: people/projects テーブルがほぼ空。「Alice に送る」の Alice が person 行に結びつかない。

**提案**: 4.1 の Batch 段で commitment/open_loop を people/projects に解決してリンク。
ドラフトが**正しい相手**に宛てられ、Fusion 関連度も鋭くなる。

### 4.5 蓄積の微調整

- 埋め込み投入（`embed_job.rs`）を capture 直後に確実に走らせ、Warm 検索が撮ってすぐ効くように。
- salient span（本文全体でなく要点スパン）も保存し、Fusion 組み立てを高速化（150ms ボタン提示 SLO 余裕確保）。
- Cold 降格(30日)・confidence 減衰は現状維持。**後方互換を破るマイグレーションは書かない**（メモリは年単位で生きる）。

### 4.6 context cache は「フォーカスで先組み・押してから集めない」を厳守

- 4.2 の AXObserver フォーカスイベントに assemble を紐付け、`ContextCache` を RAM に保持。
  ⌥タップ/チャット時は **cache 読取 + `inline_memory` のみ**（walk しない）。SLO（ボタン150ms / cache300ms / 初トークン1s）に整合。

---

## 5. 段階計画（各段は独立に出荷・計測可能）

| 段 | 内容 | 依存 | 先行できるか |
|---|---|---|---|
| **P1** | Sources 画面 + Traceability ビューア + provenance 表示（**既存 REST/backend の上に UI だけ**） | 無し | ✅ 実トランスポート不要で今すぐ |
| **P2** | 実トランスポート第1弾（Gmail+GCal リモートMCP + OAuth→Keychain）+ 背景同期スケジューラ + FSM 稼働 | Keychain OAuth アダプタ | サービス毎に段階投入 |
| **P3** | Dream Cycle Batch 分類（Select KK）→ confidence>0.5・entity linking・due パース | P2 のデータ量 | 抽出の芯 |
| **P4** | Fusion 関連度を埋め込みへ / AX を AXObserver に一本化 | ONNX embed 投入 | レイテンシ・CPU |
| **P5** | Composio Gmail 送信（L3・consent UI・第三者バッジ・draft-stop） | P2 | 第2層 |

**計測**: レイテンシに触る P4 は p50/p95 を計測してからマージ（SLO ゲート）。スキーマ変更はマイグレーション＋ロールバック必須。

---

## 6. 不変条件チェック（この設計が守るもの）

- ✅ データ重心は Rust コア（webview は表示のみ / UI は既存 backend の投影）。
- ✅ 画像保存なし・AX テキストのみ。
- ✅ 生データはデバイス外に出さない（同期は読み取りで egress せず、出るのは処理チャンクのみ＋必ずトレース）。
- ✅ L1 に外部送信を含めない（送信は Composio 経由 Gmail のみ・常に L3）。
- ✅ 鍵分離: 分類/Dream/Brief=Select KK Batch、エージェント/チャット/ドラフト=BYOK。逆転させない。
- ✅ 人間UIとAI API（MCP/CLI/REST）対称: Sources/Traceability も同じ backend 面に載る。
- ✅ secrets（OAuth/BYOK）は Keychain のみ。

---

*本書は Linux セッションで、現状コードの精査（capture_source/axcache/extract/state/assemble/confidence/scope/composio/
connection/sync/traceability）に基づいて作成。実装は別チャットで段階計画 P1→ に沿って進める。*
