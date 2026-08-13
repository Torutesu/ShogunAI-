# 5層ギャップの完成設計（L1会議検知拡張 / L2の穴 / L3ループ / L4 / L5）

**Status**: Draft v1（2026-08-09）
**前提**: `docs/feature-status.csv`（2026-08-05）とコード実態調査に基づく。現状評価の要約は `docs/positioning-category-messaging.md` の議論から: L1-L2は実在、L3の体験・実接続、L4の見え方、L5がギャップ。
**対象ブランチ規約**: 純ロジックはLinux green必須、実機検証は `docs/phase1-ondevice-runbook.md` の手順に追記。

---

## 1. L1 — 会議検知の対応拡大（工数見積もり）

### 現状の構造（拡張に有利）

`crates/shogun-core/src/meeting/detect.rs` は**テーブル駆動**:

- `MEETING_BUNDLE_IDS: &[&str] = &["us.zoom.xos"]`（ネイティブアプリ）
- `MEETING_HOSTS: &[&str] = &["meet.google.com"]`（ブラウザタブのパース済みホスト。substring攻撃対策済み）
- 判定ロジック（3シグナル相関・mic sustained・corroboration≥2・終了条件）は**アプリ非依存**で流用できる

### 追加候補と工数

| 対象 | 変更 | 工数 | 注意点 |
|---|---|---|---|
| Zoom Webクライアント | `MEETING_HOSTS` に `app.zoom.us`（`/wc` パスは見ない。ホストのみ） | 数時間 | ホスト単位なのでZoomマーケページと区別するためcorroboration（mic/コントロール可視）が効く既存設計のままでよい |
| Teams（ネイティブ） | `MEETING_BUNDLE_IDS` に `com.microsoft.teams2`（新Teams）, `com.microsoft.teams`（旧） | 数時間＋実機確認半日 | Teamsは常駐チャットアプリなので**frontmostだけでは会議ではない**。`has_zoom_bundle` 相当の「strong opener」には入れず、mic sustained との2シグナル要求側に置く（`OfferPolicy` の既存分岐を流用） |
| Teams（Web） | `MEETING_HOSTS` に `teams.microsoft.com`, `teams.live.com` | 数時間 | 同上。ホスト一致をstrongにせずcorroborating扱いにする |
| Webex（ネイティブ+Web） | bundle `Cisco-Systems.Spark` / host `*.webex.com`（サフィックス一致は `is_media_host` と同型の末尾一致で） | 半日 | `*.webex.com` は管理画面も含むためTeamsと同じ弱シグナル側 |
| Whereby / Around 等 | host追加のみ | 各30分 | 会議専用ホストなのでstrong側でよい |
| **Slack ハドル** | bundle IDが通常Slackと同一のため**テーブル追加では不可**。mic-in-use（既存 `MicWatch`）＋AXウィンドウタイトル/要素のハドル手掛かり（"Huddle" 文字列・通話コントロール）の組合せを新シグナルとして追加 | **1〜2日＋実機チューニング** | 誤検知リスクが最も高い。v1では「候補として弱くオファー」（自動開始しない）に留める |

**結論: ネイティブ/Web系の主要追加（Zoom Web・Teams・Webex）は合計2〜3日（うち大半は実機での誤検知確認）。コード変更自体は定数追加＋テスト。Slackハドルだけ別枠で1〜2日。** 900行の判定ロジックは書き直し不要。

追加時の規律: 各追加に対し (a) strong opener か corroborating か を明示的に選ぶ、(b) `detect.rs` の既存テスト群と同型のテストを足す、(c) 実機で「そのアプリを開いただけ」で発火しないことを確認してから `phase1-findings.md` に記録。

---

## 2. L2 — 穴の検討と設計

### 2.1 Cold層が検索から読まれない（E-09）— 最重要

**現状**: `cold.rs::load_partition` に呼び出し元がなく、30日超のイベントはFTS（語彙）のみでヒットする。「メモリは年単位で生きる」という設計思想に対して、セマンティック検索が30日で切れている。

**設計: クエリ時パーティションスキャン**

```
search_hybrid(query, range):
  warm: 既存どおり sqlite-vec knn + FTS → RRF
  cold: range が30日カットオフより古い期間を含む場合のみ:
    for p in partitions_in(range) (月次パーティション, 新しい順):
      rows = load_partition(p)              # int8 + scale
      score = dot_i8(query_vec_i8, row) * scale  # 逆量子化せず int8 内積 → スケール補正
      top-k を BinaryHeap で維持
    cold候補を RRF に第3ソースとして合流
```

- **性能見積もり**: 1パーティション=1ヶ月分。10万イベント/月 × 384次元のint8内積 ≈ 38M mul。SIMDなしでも1パーティション50ms級。既定は**直近6パーティションまで**（=7ヶ月分）を上限とし、`depth: all` 指定時のみ全走査（CLI/REST/MCPにパラメータ追加）。NFR（ローカル検索500ms）は「warm既定」の現行定義を維持し、cold込みは別枠のSLOにする（沈黙を成功と読まない原則で `measured` フラグを分ける）
- **FTSは現状でもcoldを跨いで効いている**（テキストは `event_log` に残り、埋め込みだけ降格される）ため、この変更は「古い記憶の意味検索」だけを足す差分で済む
- 実装位置: `shogun-memory/src/search.rs` に `search_cold_partitions` を追加し `search_hybrid_since` から合流。**全てLinuxでテスト可能（純ロジック）**
- **工数: 2〜3日**（int8内積＋ヒープ選抜＋RRF合流＋パーティション上限とテスト）

### 2.2 イベントバスが死んでいる（E-49）

**現状**: `bus.rs`（175行）は実装済みだが購読者ゼロ。connector文書の「item D（IntegrationSynced を Fusion/Notch が購読）」が前提としているのに配線がない。

**判断: 削除ではなく最小配線。** 理由: L3の「同期→パネル鮮度」体験（後述）とL4のBrief更新が同じ通知経路を必要とし、ポーリングで代替すると context cache の300ms SLOに悪影響。

- 配線v1: `ingest_integration` 完了時に `IntegrationSynced{service, n_items}` をpublish → daemonの購読タスクが context cache を無効化（次回アセンブルを新鮮に）＋Notchインジケータ更新
- **工数: 1日**。これ以上の用途（CaptureBurst等）は必要が生まれるまで足さない

### 2.3 Identity統合の確認API（E-21）

**現状**: exact一致のみ自動統合、確認付きmerge/splitのAPIとUIが無い。**v1据え置きで妥当**（誤統合はメモリ破壊で、年単位データの後方互換を壊すリスクが高い）。ただし `people` ペインに「同一人物として統合」ボタンの**枠だけ**先に置き、押下時は「v1では手動編集」で逃す。設計負債として明記。

### 2.4 記録事項

- Cold検索を入れるまで、外部向けに「全履歴を意味検索」とは言わない（現状は「全履歴をテキスト検索、30日を意味検索」が正確）。メッセージング文書と整合させる。

---

## 3. L3 — 体験ループを閉じる設計（最優先領域)

**現状の本質**: バックエンド（notch_actions / 承認キュー / send_exec / Composio Gmail送信）は揃っているのに、**ユーザーが到達できる入口と、キューに餌をやる生産者がいない**。新規開発ではなく「配線」が主。

### 順番（依存順・全部で実働2週間規模）

| # | WP | 内容 | 工数 |
|---|---|---|---|
| 1 | **notch actions UI** | `ContextCache` の≤4候補をパネルに描画 → `run_notch_action` → L2はインライン確認、L3は承認キューへ。SLO-02（提示150ms）の計測をこの画面に同梱 | 2〜4日 |
| 2 | **ローカル効果の実体化** | `ShowNotification`→UserNotifications, `CopyToClipboard`→NSPasteboard, `OpenApp`/`RevealFile`→NSWorkspace。`eprintln!` スタブ撲滅 | 1日 |
| 3 | **承認キュー統合（E-08）** | 3つの孤立キュー（mcp.rs / shogun_api.rs / lib.rs）を daemon 所有の単一 `ApprovalQueue` に集約。UIは既存 `ApprovalsSection` が唯一のドレイン。API経由投入も同じ画面に出る＝不変条件6（UI/API対称）の実体化 | 1〜2日 |
| 4 | **OAuth実配線＋Gmailライブ検証** | `connect_service` の `mark_connected()` 偽装をやめ、書き済みの `oauth_flow` に接続。`connector-summary-and-live-checklist.md` §4 を初めて実施（人間のクレデンシャル準備込み） | 2〜3日 |
| 5 | **エージェント生産者 v1 = Reply Drafter一本** | notchアクション「Draft reply」→ 既存 `draft_reply`（BYOK LLM呼び出し実装済み）→ 承認キュー → Composio送信。**7プリセットの残りはこのループが閉じるまで作らない** | 2〜3日 |
| 6 | **検索UI** | パネルに検索ボックス1つ（`search_hybrid` は実装済み）。SLO-04が初めて計測可能になる | 1〜2日 |

**ゲート**: #1〜#5 完走で「画面を見る→ボタンを押す→承認→実際にGmailが送信される」が実機で成立する。これがGTMプロトタイプゲートであり、デモの最小単位。#6は並行可。

**やらないことの明記**: プリセット7種の実行体、Wave 2/3コネクタの解放、ストリーミング表示（E-13）は、上のループが実機で閉じた後。順番を守らないと「浅く広い未完」が再生産される。

---

## 4. L4 — 設計（Briefの非劣化・Batch relay）

### 4.1 Morning Brief を本来の姿にする（費用対効果最大）

**現状**: `assemble_brief`（フル版: カレンダー行・生成文・提案アクション）は実装済みなのに、`fullui.rs:318` が `local_morning_brief(Vec::new(), now)`（カレンダー空・`generated:false` 固定）を呼んでいる。Dream Cycleの `MorningBrief` ジョブは意図的no-op。

**設計**:

1. **Dreamジョブを「夜間生成＋永続化」に昇格**: 新テーブル `briefs(date PRIMARY KEY, payload JSON, generated INTEGER, built_at)`（V15の一部）。ジョブは (a) state tables から材料集約 (b) Batch lane 利用可なら生成文を付与 (c) 不可なら extractive（既存の honest degradation パターン踏襲）で `generated:false` のまま保存
2. **カレンダー行の現実解**: Google Calendar 連携が生きるまでは「検知済み会議（meeting_recaps/次回言及）＋期限当日の commitments」をカレンダー相当行として渡す。連携解放後に差し替え
3. **朝の表示**: `fullui.rs` は当日 `briefs` 行を読むだけ（即表示・オフライン安定）。行が無ければ現行の劣化組み立てにフォールバック
4. `updated` フラグ（E-19b）はこの永続化で自然に実装できる（前夜payloadとの差分）

**工数: 2〜3日**（テーブル＋ジョブ実効化＋表示切替。生成文はrelay完成までextractive）

### 4.2 Batch relay（`docs/batch-relay-design.md` の実装着手）

設計は確定済み・未着手。**apps/api（現在空）を実装地とする**。

- **v1スコープ最小**: `POST /v1/batch`（license JWT検証 → プラン上限チェック → Anthropic Batches へ委譲 → 従量計上テーブル）と `GET /v1/batch/:id`。管理画面・請求連動はv2
- クライアント側: `dream.rs` の「開発用生キー直叩き（E-38, must-never-ship）」を relay 呼び出しに差し替え、開発時のみ環境変数でダイレクト経路（ガードスクリプトでリリースビルド禁止を強制）
- あわせて `run_dream_now` の同期ブロッキング（E-38）を非同期化
- **工数: サーバ3〜5日＋クライアント差し替え1日＋デプロイ/監視1日**。この完成が「Standard はSelectキーだけで動く」の前提であり、**課金開始のブロッカー**なのでL3ループの次に優先

### 4.3 通知経路

期限超過 commitments → `ShowNotification`（L3 WP2で実体化済みの効果）を hourly recompute から発火。**通知はL1（自動）に許される非送信アクション**であることをテストで固定。工数: 1日。

---

## 5. L5 — 自己改善（Lessons / Patterns）v1設計

**現状: コード・スキーマ・管理表ともゼロ。** 一方でメッセージングの中核主張（「使うほど賢くなる」「一度の修正が全実行に効く」）であり、乖離が最も大きい。以下は既存構造（extract→state→recompute→fusion注入、Shougun.md注入点、Dream Cycleジョブ枠）に素直に乗せる設計。

### 5.1 データモデル（V15マイグレーション）

```sql
-- 生の学習シグナル（ローカルDB内。テレメトリには一切出さない）
CREATE TABLE feedback_events (
  id INTEGER PRIMARY KEY,
  ts_ms INTEGER NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('edit_before_approve','reject','approve_unchanged','state_resolve','undo')),
  action_kind TEXT,            -- 例: 'draft_reply'
  scope TEXT NOT NULL CHECK(scope IN ('global','app','person','project')),
  scope_ref TEXT,              -- bundle id / person id / project id
  before_text TEXT,            -- 提案本文（ローカル保存は可。egress禁止）
  after_text TEXT              -- 確定本文
);

-- 蒸留された教訓
CREATE TABLE lessons (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL CHECK(kind IN ('style','preference','correction','pattern')),
  scope TEXT NOT NULL CHECK(scope IN ('global','app','person','project')),
  scope_ref TEXT,
  instruction TEXT NOT NULL,   -- プロンプト注入可能な1文（英語）
  confidence REAL NOT NULL CHECK(confidence >= 0 AND confidence <= 1),
  evidence_count INTEGER NOT NULL DEFAULT 1,
  active INTEGER NOT NULL DEFAULT 1,   -- ユーザーが個別にOFF可
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  last_evidence_at INTEGER NOT NULL
);
CREATE TABLE lesson_provenance (      -- state_provenance と同型
  lesson_id INTEGER NOT NULL,
  feedback_event_id INTEGER NOT NULL,
  PRIMARY KEY (lesson_id, feedback_event_id)
);
```

state tables と同じ規律を適用: **provenance必須・confidence必須・低confidenceは注入しない**（Fusionの既存band gateを流用）。

### 5.2 シグナルの取得点（既存UIに3フック）

1. **承認前編集**: `ApprovalsSection` で提案本文と確定本文が違うまま `confirm_send` された → `edit_before_approve`（before/after付き）
2. **却下**: `reject_send` → `reject`
3. **無修正承認**: 成功の corroboration として `approve_unchanged`（confidenceを上げる側の証拠）

いずれも既存コマンドの内側に1行フックを足すだけ。キャプチャ層には触れない。

### 5.3 蒸留（Dream Cycle 第7ジョブ `LessonDistillation`）

- 夜間に未処理 `feedback_events` を集約し、教訓候補を生成:
  - **ローカル既定（honest degradation）**: ルールベース——同一scopeで編集が3回以上同方向（例: 署名の削除、敬体→常体）なら定型 instruction を生成。挨拶・署名・長さ・言語選択など機械的に検出可能なパターンに限定
  - **Batch lane 利用可時**: before/after ペア群を分類プロンプトで蒸留（relay経由・Select KKキー。不変条件5どおりBatch laneの仕事）
- 同義 lessons はマージして `evidence_count` を加算、confidenceは `recompute.rs` と同じ corroboration/decay 規則（反証＝lessonに反する編集が続いたら減衰→`active=0`）
- 上限: active lessons **50件**（超過は最弱confidenceから休眠）。プロンプト予算を壊さない

### 5.4 注入と対称性

- **注入点は Shougun.md パイプラインに相乗り**: `user_config` の既存注入経路に「auto-learned」セクションとして合流。Fusion `assemble` はscope一致（相手・アプリ・プロジェクト）でtop-kを選ぶ
- **UIで全件可視・編集可能**: Personalization 画面に「Learned」リスト（instruction・根拠件数・ON/OFF・削除）。**学習内容がブラックボックスにならないこと**が信頼の要件。ユーザーが文言を編集したら手動 directive に昇格（Shougun.md側へ移動）
- **MCP/CLIにも対称に公開**（不変条件6）: `lessons list/disable` を Memory API に追加
- **絶対規則**: lessons は**生成内容にのみ**影響し、権限判定（L1/L2/L3）には一切影響しない。テストで固定

### 5.5 効果測定（「使うほど賢くなる」を主張から数値へ）

- 指標: (a) 承認前編集の編集距離の週次推移（縮むはず）(b) 無修正承認率 (c) lesson hit rate。`shogun metrics` に `measured:false` 起点で追加
- この数値がそのまま投資家向けの「複利ループが回っている」証拠になる

### 5.6 工数

| 段階 | 内容 | 工数 |
|---|---|---|
| v0 | V15＋フック3点＋Learnedリスト表示（蒸留なし・記録と可視化のみ） | 2〜3日 |
| v1 | ローカルルール蒸留ジョブ＋Fusion/ドラフト注入＋メトリクス | 3〜4日 |
| v1.5 | Batch lane蒸留（relay完成後に差し替え） | 1〜2日 |

v0はL3ループ（承認UI）が生きていれば即着手可能。**ピッチとの整合上、最低v0までは投資家面談前に入れるのが望ましい**（「記録は始まっている。蒸留はDream Cycleの第7ジョブとして設計済み」と言える状態）。

---

## 6. 推奨の全体順序

```
1. L3ループ閉鎖（WP1〜5, ~2週間）      ← すべての土台。デモとGTMゲート
2. L4.1 Brief非劣化（2〜3日）           ← デモの顔。L3と並行可
3. L4.2 Batch relay（~1週間）           ← 課金開始のブロッカー
4. L5 v0→v1（~1週間）                   ← ピッチ整合＋複利の計測開始
5. L2.1 Cold検索（2〜3日）              ← 純ロジック。隙間で並行可
6. L1 会議検知拡張（2〜3日＋Slack別枠）  ← 実機QAとまとめて
```

実働合計 ≈ 5〜6週間（1人）。うち「投資家に見せる最小」は 1＋2 の約2.5週間。
