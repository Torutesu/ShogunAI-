# Phase 1 実装計画書（v1本実装）

作成: 2026-07-20（Phase 0 Go判定後）。
実行者向け: 本書は**実装を担当するエージェント（Sonnet想定）への作業指示**である。要件の正本は `docs/requirements-v1.0.md`（以下「要件書」、FR/NFR/AR番号で参照）と `CLAUDE.md`。本書は「何を・どの順で・どう検証して」作るかを定める。要件の再定義はしない — 番号参照先を必ず読んでから着手すること。

---

## 0. 現在地と前提

- Phase 0（ノッチUIスパイク）は物理ノッチ実機で Q2/Q3-A/Q3-B/Q4 の SLO 合格を確認し、**ノッチ方式で Go**（`docs/phase0-findings.md` 末尾の総括参照）。
- Q1（常駐）の長時間ソークは未実施のまま持ち越し。**M2完了条件の24h連続稼働で回収する**。
- 持ち越し資産: `crates/spike-core`（純ロジック: geometry/hover/statemachine/engine/axcache、71+テスト）と `crates/spike-harness`（計測: clock/slo/record/recorder/cpu/stats/report）。`apps/desktop/src-tauri` は使い捨てスパイクシェルであり、**M1で製品シェルに置き換える**。
- 実行環境の制約: このリポジトリの開発セッションは Linux コンテナで走ることがある。**macOSコードはローカルでコンパイルできない**前提で、(a) 純ロジックはLinuxでテスト可能なクレートに置く、(b) macOSアダプタは `.github/workflows/phase0-ci.yml` 方式（macos-14ランナーでcompile+clippy）で検証する、(c) 実測はユーザーの実機で行い結果を貼ってもらう——というPhase 0の開発ループを踏襲する。

### 0.1 実行者への行動規約（Phase 0で確立した運用）

1. **コミット規約**: Conventional Commits。SLOに影響する変更はp50/p95計測結果をPR/コミット本文に貼る。スキーマ変更はマイグレーションファイル＋ロールバック手順必須。
2. **停止して人間に確認するトリガ**: 外部送信を伴う機能の既定値変更 / 価格・課金ロジック / secrets の取り扱い方式変更 / 要件書に無いスコープ追加（スコープ表 §3 を根拠に確認）/ 破壊的マイグレーション。それ以外は自律で進める。
3. **検証優先**: 「コンパイルが通る」ではなく「テストが通る・計測が出る」を完了条件にする。沈黙を成功と読まない（レポートの NO-DATA/MISSING ガードを製品メトリクスにも踏襲）。
4. **プライバシー保証はテストで担保**: spike-harness の `no_text_body_fields` テストパターン（レコード型にテキスト本文フィールドが存在し得ないことの型レベル+テスト担保）を、製品のログ・テレメトリ・トレーサビリティ記録の全シンクに適用する。
5. 文言は英語・`strings.ts`型カタログ分離・絵文字は⚔のみ・競合名/技術スタック名を出さない（CLAUDE.mdブランドルール）。

---

## 1. マイルストーン全体像

要件書 §5.6 の M1〜M5 に従う。各Mは前段の完了条件充足がゲート。**M内のWP（Work Package）は依存が許す限り並行可**。

| M | 内容 | ゲート（完了条件） |
|---|---|---|
| M1 | Notch UI本実装（スパイクの製品化） | 状態機械全遷移 + NFR-SLO-01/02 実機計測合格 + **Q1ソーク（≥24h）** |
| M2 | キャプチャ + メモリ + state tables | 24h連続稼働・検索SLO（10万イベントp95≤500ms）・マイグレーションCI合格 |
| M3 | Context Fusion + L1/L2 + Dream Cycle / Morning Brief | US-01/02/03 E2E合格（要件書§4.2） |
| M4 | 第1層MCP（Wave 1→順次）+ L3 + Memory API + 第2層 | 許可範囲表・トレーサビリティのテスト合格 |
| M5 | 課金 + トライアル + オンボーディング + 配布 | トライアル→課金E2E・notarization済みビルド |

---

## 2. M1 — Notch UI本実装

目的: スパイクを捨て、製品リポジトリ構成（CLAUDE.md）の上に常駐アプリの骨格を作る。要件書 §6.1、AR-03〜05。

### WP1.1 ワークスペース再編と製品シェル

- `crates/shogun-core` を新設（ライブラリ。プロセス境界を前提としない設計 = AR-03）。
- `spike-core` の内容を `shogun-core` 配下モジュールへ **git mv で吸収**（`shogun_core::notch::{geometry, hover, statemachine, engine}`、`shogun_core::capture::walk_policy`（旧axcache））。テストごと移す。`crates/spike-core` はワークスペースから削除。
- `crates/spike-harness` は**開発計測ツールとして残置**（名称そのまま。製品内メトリクスは WP1.4 の別実装）。
- `apps/desktop/src-tauri` を製品シェルとして書き直す: identifier `com.selectkk.shogun`、productName `SHOGUN`、Login Item 自動起動、Dock非表示（`LSUIElement`）+メニューバー常駐、ウィンドウ全閉でもプロセス継続（AR-03）。スパイクの実証済み配線（NSPanel swap / tauri-nspanel v2.1 / CGEventTap / 位置固定 / SPIKE_NO_PANEL相当の診断フラグ / **wry evalプローブ禁止コメント**）を移植。
- CI: `phase0-ci.yml` を `ci.yml` に改名・対象を新構成に更新（pure crates job + macos-14 shell job + frontend job の3本立てを維持）。

**受け入れ**: 新構成で3ジョブgreen。実機で起動→ノッチ直下に常駐→展開/折りたたみが Phase 0 同等に動作。

### WP1.2 状態機械の完全化と表示要件

- 要件書 §6.1.1 の全遷移を engine に反映（スパイク実装との差分を洗い出して埋める。ホットキー展開・クリック展開の追加入力を含む）。
- §6.1.3 インジケータ: Idle シェルに状態色（正常/同期中/エラー/一時停止グレー）。エラーは作業を中断させない（NFR-REL-04）。
- §6.1.2 擬似ノッチ: ノッチ非搭載/外部ディスプレイでメニューバー中央パネル。**Phase 0既知課題「外部モニタで出ない」はここで解消**: ディスプレイ構成変更（`didChangeScreenParametersNotification` 500msデバウンス）→ 対象スクリーン再選定（内蔵優先 §3.7.1 相当）→ 再配置 → ヘルスチェック、の spike display.rs に書かれた設計を実装する。
- §6.1.4 フルスクリーン時挙動。
- webview: strings カタログ拡張、CSS遷移は spike の transform/opacity 方式を維持（NSWindowリサイズ禁止）。

**受け入れ**: 遷移網羅の純ロジックテスト（Linux）。実機で外部ディスプレイ接続/切断・フルスクリーンの手動チェックリスト合格。

### WP1.3 イベント駆動化（ポーリング廃止）

- スパイクの「400msポーリング+2s再walk」を、NSWorkspace `didActivateApplication` 通知 + AXObserver（`kAXFocusedWindowChangedNotification` / `kAXTitleChangedNotification` / `kAXValueChangedNotification` / `kAXFocusedUIElementChangedNotification`）購読 + 500msデバウンス（FR-CAP-02）に置換。
- ブラウザのAXツリー遅延構築対策（空成功→500ms後1回再試行）は移植。タブ切替は `kAXFocusedUIElementChanged`/`kAXTitleChanged` で捕捉されることを実機確認。
- 通知が取れないアプリ向けフォールバックポーリングは下限2s（FR-CAP-02）。

**受け入れ**: アプリ切替・タブ切替・ウィンドウ内編集の3操作でcache更新が発火する実機ログ。アイドル時CPUがスパイク時（0.9%）から悪化しないこと。

### WP1.4 製品内SLOメトリクス（NFR-SLO-00）

- `shogun_core::metrics`: 軽量ヒストグラム（固定バケット）で NFR-SLO-01〜06 を常時計測。ローカル保存のみ。spike-harness のJSONLは開発時の詳細計測用として併存。
- 計測点は要件書 §7.1 の表の定義に厳密に従う（例: SLO-01はイベント受信→最終フレーム描画完了）。
- `shogun metrics`（CLI、M4のshogun-cliで公開）と Full UI Advanced から読める形の内部APIだけ先に用意。

**受け入れ**: ヒストグラムのユニットテスト。実機でSLO-01/02のp50/p95が出力され、Phase 0実測（展開p95 18ms）と整合。

### WP1.5 Q1ソーク（持ち越し回収・第1回）

- 実機で15〜20分＋可能なら一晩の常駐ソークを実施依頼（ユーザー操作）。heartbeat・blackout・anim_timeout原因・self-heal回数を確認。anim_timeout が定常発生するなら原因修正をM1内で行う。

**受け入れ**: heartbeat連続・blackoutなし・クラッシュ0のレポート。

---

## 3. M2 — キャプチャ + メモリ + state tables

目的: 年単位で生きるデータ基盤。書き込みを絶対に失わない。要件書 §6.2〜6.4、§7.4〜7.5。

### WP2.1 `crates/shogun-memory` — スキーマとマイグレーション

- rusqlite + WAL（`synchronous=NORMAL`以上）+ refinery。DBパス `~/Library/Application Support/com.selectkk.shogun/`（0600、NFR-SEC-05）。
- **V1マイグレーション**（初版で全部入り。spatial-readyカラムを後付けにしない = FR-MEM-12）:
  - `event_log`（FR-MEM-11の全カラム。`content_hash`、`last_seen_at`、`dwell_ms`、`display_id`、`window_bounds`、`window_pose`、`gaze_target`）
  - FTS5 trigram 仮想テーブル + 同期トリガ
  - sqlite-vec 仮想テーブル（Warm層 float32）+ Cold層パーティション表（月単位、int8）
  - `people` / `projects` / `commitments` / `open_loops`（§6.4.2〜6.4.5の定義通り）+ `state_provenance(state_table, state_id, event_id, weight)`
  - `traceability_log`（M4で使用。**送信チャンクの本文は保存しない**設計: 目的・宛先・チャンクのdigest・バイト数・関連event_id）
- 起動時 `quick_check`、日次バックアップ（SQLite backup API、3世代、NFR-REL-03）。
- provenance空のstate INSERTを防ぐDB制約（トリガ）+リポジトリ層アサーション（FR-ST-02）。
- ロールバック手順書 `docs/migrations/V1-rollback.md`。

**受け入れ**: 空DB/ダミーデータDB両方へのマイグレーションCIテスト（FR-MEM-30受け入れ基準）。kill -9 →再起動でintegrity check通過・欠損なしのテスト。

### WP2.2 キャプチャパイプライン（`shogun_core::capture`）

- WP1.3のAX購読を入力に、FR-CAP-01〜09を実装:
  - 除外リスト（既定値はFR-CAP-05の列挙をそのままコード化。パスワードマネージャ+SecureTextFieldは削除不可）
  - プライベートブラウジング判定（既知ブラウザのタイトル/AX属性パターン）
  - 重複抑制（正規化後類似度98% → `last_seen_at`/`dwell_ms` 更新のみ。FR-CAP-03）
  - 一時停止ホットキー `⌃⌥⇧P` + 時限再開（FR-CAP-07）
  - 権限なしグレースフルデグラデーション + 1時間毎再検出（FR-CAP-08）
  - パニック隔離: キャプチャスレッドのみ再起動（FR-CAP-09、`catch_unwind` + supervisor）
- **除外判定はevent生成の前段**に置く（該当中はイベント自体を作らない）。

**受け入れ**: AXモックで「除外リスト全既定アプリでイベント0件」の自動テスト（Linuxで走る形に抽象化）。権限なし起動の統合テスト。実機24h稼働でクラッシュ0・CPU≤5%（= M2ゲート兼Q1回収）。

### WP2.3 内部イベントバス（AR-06/07）

- `shogun_core::bus`: `capture.text` / `focus.changed` / `cache.updated` / `state.updated` / `action.proposed` / `action.executed` / `integration.synced` / `error.raised` の型付きイベント。tokio broadcast + 有界キュー、満杯時は古いイベントをドロップしドロップ数をメトリクスへ。
- 発行者（キャプチャ）を購読者が絶対にブロックしない構造をテストで担保。

### WP2.4 3層メモリ（FR-MEM-01〜04）

- Hot層: RAM 200MB上限、超過時は古い順に要約へ畳み込み。**Warm層に先に書いてからHotに載せる**順序を型で強制。再起動時はWarmから10s以内にバックグラウンド再構築。
- Warm→Cold移動はDream Cycle（M3）に委ねるが、パーティション表とint8量子化関数はここで実装・テスト。

**受け入れ**: kill→再起動でHot再構築・欠損なし。RAM上限の畳み込みテスト。

### WP2.5 ローカルembedding（FR-MEM-21〜22、ADR-001）

- **着手時ベンチで選定を確定**（§9未決事項）: 候補 multilingual-e5-small 等を `ort`（ONNX Runtime, CoreML/CPU EP）で日英混在テキスト・512トークン・M1基準50ms/件（NFR-RES-05）で比較。結果を `docs/adr/embedding-model-selection.md` に記録。
- 書き込み非ブロッキングの非同期embedジョブ（遅延許容5分、未embed行はFTS5のみ）。Low Power Mode中は周期2倍（NFR-RES-04）。
- モデルはアプリ同梱。クラウドembedding禁止。

### WP2.6 ハイブリッド検索（FR-MEM-20、NFR-SLO-04）

- FTS5（全期間）+ sqlite-vec（Warmのみ）→ Reciprocal Rank Fusion。出典表示用DTO（FR-MEM-23）。
- 10万イベント合成データでの p95 計測ベンチをCIに置く（Linuxで実行可能。SLO合格はmacOS実機値で確認）。

### WP2.7 State tablesリポジトリ層 + インライン抽出（第1段）

- CRUD + provenance必須の型安全リポジトリ。confidence規則（§6.4.6）。
- インライン抽出の第1段は**ローカルルールのみ**（正規表現/ヒューリスティクスで commitments/open_loops 候補、低confidence）。Batch API分類はM3のDream Cycleと同時に導入（キー分離不変条件5に触れるため、モデル呼び出し基盤をM3で一元化してから）。

---

## 4. M3 — Context Fusion + エージェント + Dream Cycle / Morning Brief

目的: 「状態の推定と実行」の中核。要件書 §6.5〜6.8。**キー分離（不変条件5）とL1外部送信禁止（不変条件4）はこのMの中心的ガードレール。**

### WP3.1 モデル呼び出し基盤（`shogun_core::llm`）

- プロバイダ抽象trait + Anthropic実装1つ（ADR-002）。**2系統を型で分離**: `BatchClient`（Select KKキー、Dream Cycle/Morning Brief/分類専用）と `AgentClient`(ユーザーBYOK、推論/チャット/ドラフト専用)。逆用をコンパイルエラーにする（キー型を別型に）。
- BYOK管理: Tauri command → 即Keychain（security-framework）、UI読み戻しは末尾4桁のみ（NFR-SEC-01/02）。
- 全外部送信に traceability 記録（AR-11/12: 処理用チャンクのみ、生ログ全体送信禁止）。ストリーミング対応（SLO-03）。

### WP3.2 `crates/shogun-fusion` — Context Fusion

- `f(state, screen_ctx, intent) → action候補` （§6.5）。AR-09のcache内容（screen_ctx + 関連state上位 + Hot要約 + L1/L2/L3タグ付きアクション候補）を組み立てる。
- 低confidence stateは事実として混ぜず「〜の可能性」として弱く渡す（データモデル原則）。
- SLO-05（focus.changed→300ms）とSLO-02（アクション4件提示150ms）の計測を同梱。cacheはRAMのみ・起動時即構築（AR-10）。

### WP3.3 `crates/shogun-agents` — L1/L2実行エンジン + プリセット7種

- 権限モデル（§6.6.1）: L1=自動実行（**外部送信系を型レベルで排除** — L1アクション型に送信系バリアントを定義しない）、L2=ワンタップ、L3=明示確認（M4で解放）。
- 実行エンジン（§6.6.2）: キュー・キャンセル・タイムアウト・`action.executed`イベント+event log記録。
- プリセットエージェント7種（§6.6.3の定義通り。v1はコード定義でよい）。
- ストリーミング初トークンSLO-03計測。

### WP3.4 Dream Cycle（§6.7）

- 夜間バッチ（電源条件FR-DC-01）: バックアップ（NFR-REL-03）→ Warm→Cold移動+int8量子化（FR-MEM-04、トランザクション内）→ Batch APIでstate統合・再計算 → `state.updated`発行。
- 全送信チャンクにトレーサビリティ。失敗時は元状態維持。

### WP3.5 Morning Brief（§6.8）+ US-01/02/03 E2E

- Brief生成（Batch API）とNotch/Full UI表示。
- **M3ゲート**: 要件書§4.2のUS-01/02/03を実機E2Eで通す（受け入れシナリオをそのままテスト手順書化）。

---

## 5. M4 — 連携 + L3 + Memory API

目的: 外部世界との接続。**人間UIとAI APIの完全対称（不変条件6）をここで成立させる。** 要件書 §6.9〜6.11、§6.14。

### WP4.1 `crates/shogun-mcp` 基盤 + トレーサビリティUI

- Rust MCP SDKクライアント。OAuthはユーザー→サービス直接、トークンはKeychain。リフレッシュ・失効時の再認証導線。
- トレーサビリティ画面（§6.14）: 全外部送信の目的/宛先/時刻/チャンクdigest表示。Composio経由は「第三者経由」明示。

### WP4.2 Wave 1: Gmail + Google Calendar

- §6.9.2 許可範囲表に厳密準拠。Gmail: 読み取り・ドラフト=公式MCP、**送信はここでは作らない**（第2層WP4.6）。「ドラフト止まりモード」設定を最初から用意。
- カレンダー作成はL3のみ（不変条件4）。
- 同期は `integration.synced` → event log（source列で区別）→ 検索/Fusionに合流。

### WP4.3 L3実行フロー

- 明示確認UI（モーダル許可ケース、NFR-REL-04）。承認→実行→`action.executed`→トレーサビリティの一連。
- **AI経由（MCP/CLI/REST）にも同一のL1/L2/L3ゲートを適用**（不変条件6。承認はUIに出る）。

### WP4.4 Memory API（Pro）: MCPサーバー / CLI / REST

- `crates/shogun-mcp`（サーバー側）+ `crates/shogun-cli`。REST: 127.0.0.1:**7464**、トークン認証、CORS無効（NFR-SEC-03、FR-API群）。
- 新機能はUIとAPI両方から呼べる形（対称性）をレビュー観点に固定化。

### WP4.5 Wave 2: Slack → Wave 3: Notion + GitHub + Linear

- Slack: WS管理者承認不可の場合はドラフト→クリップボードフォールバック。
- Wave間は独立WP。各Waveの完了条件 = 許可範囲表テスト + トレーサビリティ記録テスト。

### WP4.6 第2層（Composio、オプトイン）

- Gmail送信のみ（v1）。オプトインフロー + 「第三者経由」UI + L3必須。

---

## 6. M5 — 課金・オンボーディング・配布

要件書 §6.12〜6.13、§7.6。

- WP5.1 Stripe + ライセンス検証 + 7日フルトライアル。**プラン判定はRustコア側**（webviewゲーティングだけに頼らない）。Standard/Pro機能境界はキー境界と一致（ADR-003）。
- WP5.2 オンボーディング（§6.13: 権限取得フロー・キャプチャ説明・除外リスト初期設定）。
- WP5.3 設定画面（§6.15）+ 削除/エクスポート（FR-SET-07）。
- WP5.4 配布: Developer ID + notarization + Tauri updater。**署名証明書・Stripe鍵の取得はユーザー作業** — 必要になった時点で停止して依頼する。
- **M5ゲート**: トライアル→課金E2E、notarization済み.dmgが実機で起動。

---

## 7. 横断ガードレール（全WP共通・レビュー観点）

| # | 項目 | 担保方法 |
|---|---|---|
| G1 | 不変条件1: データ重心はRust | webviewからのSQL/検索ロジック/secrets保持をコードレビュー+アーキテクチャテストで禁止（AR-04/05） |
| G2 | 不変条件2: 画像を保存しない | スクリーンショットAPI呼び出しの不存在をCIでgrep検査 |
| G3 | 不変条件3: 生データ外送禁止 | 外部通信は5経路のみ（AR-11）。HTTPクライアント生成を`llm`/`mcp`/課金モジュールに閉じ込め、それ以外での`reqwest`等の使用をCI検査 |
| G4 | 不変条件4: L1に外部送信なし | L1アクション型に送信系バリアントを持たせない（型レベル）+テスト |
| G5 | 不変条件5: キー分離 | BatchClient/AgentClientの別型化（WP3.1）+「BatchキーでMessages APIを叩くコードが書けない」ことの型テスト |
| G6 | 不変条件6: UI/API対称 | 新機能PRのチェックリスト項目化 |
| G7 | 不変条件7: secretsはKeychainのみ | 平文書き出しのgrep CI + `no_text_body_fields`型のシンクテスト |
| G8 | ログ/テレメトリにユーザーテキストを含めない | 全シンクの型にテキスト本文フィールドを作らない（spike-harness方式を踏襲） |
| G9 | clippy deny / unwrap禁止 | 既存workspace lints（`unwrap_used=deny`）を全新クレートに適用 |
| G10 | SLO計測の同梱 | レイテンシ系WPは計測コードを同一PRに含める（NFR-SLO-00） |

---

## 8. 実行順序とセッション運用（Sonnetへの具体指示)

1. **開始点はWP1.1**。ワークスペース再編は他の全WPの前提。git mvで履歴を保つこと。
2. 各WPは「純ロジック（Linuxテスト可）→ macOSアダプタ（CI compile）→ 実機検証依頼」の3段で進める。実機検証はユーザーに手順を提示し、結果ログ/レポートを貼ってもらう（Phase 0の運用と同一）。
3. M1完了時とM2完了時に、SLO実測値を `docs/phase1-findings.md`（新設）に記録する。
4. 未決事項（要件書§9.1）のうち実装前決定が必要なもの（embeddingモデル選定はWP2.5冒頭のベンチで確定、他は該当WP冒頭で確認）に当たったら、選択肢+推奨を添えてユーザーに確認する。
5. ブランチ運用: 引き続き `claude/shogunai-requirements-prep-nm2tf4` で作業（変更指示があるまで）。push前にCI 3ジョブ green を確認。
6. コンテキストが長くなったら、進行状態を `docs/phase1-findings.md` の「進行中WPと次の一手」節に書き出してからセッションを跨ぐこと。

---

## 9. リスクと監視項目（Phase 0からの持ち越し含む）

| リスク | 監視/対策 |
|---|---|
| Q1未検証（常駐長時間） | WP1.5で第1回ソーク、M2ゲートの24hで確定。anim_timeout散発の原因もここで特定 |
| wry 0.55.1 evalパニック | Rust側webviewプローブ禁止を維持。wry更新時に上流Issueを再確認 |
| 外部ディスプレイ対応 | WP1.2で解消（Phase 0では意図的スコープ外だった） |
| Q2コールドスタート外れ値 | WP1.4のヒストグラムで起動直後サンプルを層別し、暖機起因かを確定 |
| sqlite-vec性能（10万件総当たり） | WP2.6のCIベンチで早期検出。超過時はWarm層の件数上限/IVF検討をユーザー相談 |
| AXObserverの信頼性（アプリ毎の差） | WP1.3でフォールバックポーリング(≥2s)を残す |
| Batch APIコスト | トレーサビリティにトークン数記録、Dream Cycle1回あたりの上限をconfig化 |

---

*本計画はFable 5が立案（2026-07-20）。実装はSonnetが担当し、本書と要件書の番号参照に従う。判断に迷ったら要件書→CLAUDE.md→本書の優先順で解釈し、それでも曖昧なら停止してユーザーに確認すること。*
