# SHOGUN ユーザーストーリー全集 v1.0 — コード準拠版

- 作成日: 2026-08-05
- 目的: **実装コードに基づいて**全機能のユーザーストーリー(受け入れシナリオ)を定義し、テスト・修正・再テストの基準にする
- ステータス追跡は `docs/feature-status.csv` (正本スプレッドシート) を参照。本書のストーリーIDと1:1対応
- 形式: Given / When / Then + 根拠コード。要件定義書(`docs/requirements-v1.0.md`)のFR IDを併記
- ステータス凡例: ✅ implemented / 🟡 partial / ⚠️ implemented-but-unwired(dead) / ❌ missing

> 各ストーリーの「Then」は**現行コードが実際に保証する動作**を書いている。要件との乖離・疑わしい挙動は `docs/feature-status.csv` の Errors 列と Issue 一覧(§99)に記録する。

---

## 1. Notch UI (FR-NU群)

### US-NU-01: ノッチにマウスを置くとプレビューが出る ✅
- Given: SHOGUN常駐中、画面上端のノッチ(または擬似ノッチ)にIdleピルが表示されている
- When: マウスをノッチ領域(R_enter)に載せ、120ms滞留する(高速進入 >1200pt/s の場合は250ms)
- Then: Hoverプレビュー(300px幅)が表示される。自動でExpandedにはならない。水平に通過しただけ(fly-by)では何も出ない
- 根拠: `crates/shogun-core/src/notch/hover.rs` (velocity判定), `statemachine.rs` (dwell 120/250ms), `tests/hover_to_state.rs`
- 関連: FR-NU-01, SLO-01

### US-NU-02: クリックで100ms以内に展開する ✅
- Given: Idle または Hover状態
- When: ノッチピルをクリックする(またはグローバルホットキー ⌘⇧Space)
- Then: Expandedパネル(560×300)が表示され、ExpandCommit計測点が記録される(SLO-01: 100ms p95)。20秒無操作で自動的にCollapsing→Idleへ戻る。Escキー/パネル外クリックでも閉じる
- 根拠: `statemachine.rs` (Expanded + MarkExpandCommit + 20s idle timer), `metrics.rs` (Slo::Expand budget 100ms)
- 関連: FR-NU-02, SLO-01

### US-NU-03: メニューバー操作でノッチが誤発火しない ✅
- Given: ユーザーがメニューバーのメニューを開こうとしている
- When: R_enter外でmouseDownし、メニューを操作する
- Then: mouseUp後300msまでhover判定が抑制され、パネルは開かない。ドラッグ中(ボタン押下中)の進入も無視される
- 根拠: `hover.rs` (menubar suppress + drag suppress、テスト7件)
- 関連: FR-NU-01, Phase 0 Q4

### US-NU-04: フルスクリーン作業を邪魔しない ✅
- Given: ユーザーがアプリをフルスクリーンで使用中
- When: SHOGUNが提案を生成した/マウスが上端に触れた
- Then: パネルは自動表示されない(全状態からHiddenへ遷移)。ホットキーによる明示展開のみフルスクリーン上でも可能
- 根拠: `statemachine.rs` (Hidden from every state, hotkey over fullscreen)
- 関連: FR-NU-08, US-11(要件書)

### US-NU-05: パネルの位置を6箇所から選べる ✅
- Given: 設定でCastlePosition(ノッチ+四辺/角)を変更した
- When: パネルを表示する
- Then: 選択位置に画面内へクランプされて表示される
- 根拠: `notch/geometry.rs` (CastlePosition, clamp テスト)
- 関連: FR-NU-05(擬似ノッチ)

### US-NU-06: 上端フリック(y=max)でも入域と判定される ✅
- Given: マウスを勢いよく画面最上端へ投げた(カーソルがy=maxに張り付く)
- When: カーソルがノッチ幅内の上端に到達する
- Then: R_enterに1ptのオーバーシュートが確保されているため入域と判定される
- 根拠: `geometry.rs` (TOP_EDGE_OVERSHOOT = 1pt)

---

## 2. キャプチャ (FR-CAP群)

### US-CAP-01: フォーカスを切り替えると画面のテキストだけが記録される ✅
- Given: Accessibility権限付与済み、SHOGUN常駐中
- When: 別アプリのウィンドウへフォーカスを移す
- Then: AXツリーの有界走査(深さ8 / 300要素 / 32KB / 250msタイムボックス)でテキストのみが収集され、event_logに保存される。画像・スクリーンショットは生成されない（Visual recall を On にした場合のみ、テキストが取れなかったウィンドウで、選択した保持期間（既定3日）で自動削除されるフレームが例外的に生成される）。ツールバー等のchromeは後回しにされ、本文が予算内で優先される
- 根拠: `capture/walk_policy.rs` (Limits, DeferredChrome), 不変条件2
- 関連: FR-CAP-01..03

### US-CAP-02: パスワードマネージャは絶対に記録されない ✅
- Given: 既定の除外リスト有効(パスワードマネージャ8種、SecurityAgent、ターミナル8種)
- When: 1Password等を開いて操作する
- Then: キャプチャイベントは一切生成されない(走査自体が除外ゲートで短絡され、AX読み取りも発生しない)。既定除外はUIから解除できない(remove_appがfalseを返す)
- 根拠: `capture/exclusion.rs` + `pipeline.rs` (Boomテスト: 除外時はツリーに一切触れない)
- 関連: FR-CAP-05..07, US-05(要件書)

### US-CAP-03: プライベートブラウジングは記録されない ✅
- Given: Safari/Chrome/Edge/Firefox/Brave/Arcのいずれかでプライベートウィンドウを開いている
- When: そのウィンドウにフォーカスがある
- Then: タイトルのプライベートマーカーを検出しキャプチャしない。未知のブラウザは通常どおりキャプチャされる(FR-CAP-05の仕様どおり)
- 根拠: `exclusion.rs` (6ブラウザ×3マーカー)

### US-CAP-04: セキュア入力欄は走査からスキップされる ✅
- Given: 任意アプリにパスワード入力欄(SecureTextField)がある
- When: そのウィンドウをキャプチャする
- Then: SecureTextFieldノードとそのサブツリー全体が読み取られない
- 根拠: `walk_policy.rs` (SecureTextField skip テスト)

### US-CAP-05: 同じ画面の再読み込みはイベントを増やさない ✅
- Given: 直近にキャプチャした本文と98%以上類似の本文が再度現れた
- When: キャプチャが走る
- Then: 新規イベントは作られず既存行がtouchされ、last_seen_atとdwell_msが加算される。抽出(commitment候補等)もtouch時は再実行されない
- 根拠: `capture/dedup.rs` (Sørensen–Dice 0.98), `daemon.rs:378 ingest_capture`
- 関連: FR-CAP-03

### US-CAP-06: ユーザーが除外アプリ・除外タイトルを追加できる ✅
- Given: 設定画面のキャプチャ除外設定
- When: アプリまたはタイトル部分文字列(大文字小文字無視)を追加する
- Then: 以後そのアプリ/タイトルはキャプチャされない。設定画面の表示とデーモンの実挙動が一致する(同一ポリシーオブジェクト)
- 根拠: `exclusion.rs` (user_apps, title_patterns, drift防止テスト)

---

## 3. メモリ・検索 (FR-MEM群)

### US-MEM-01: 「あれどこだっけ」が500msで返る ✅
- Given: 過去に読んだ資料の断片(日本語可)だけ覚えている
- When: 検索窓に断片を入力する
- Then: FTS5 trigram + Warm層(30日)ベクトル検索のRRFハイブリッド結果が返る。結果にはアプリ名・ウィンドウタイトル・出典(source)が付く。Warm窓で結果が薄い場合は全履歴へ自動拡大する。埋め込みモデル未ロードでも語彙検索で動作する
- 根拠: `shogun-memory/src/search.rs` (fts_query, RRF k=60, search_warm_first), `daemon.rs:1571`
- 関連: FR-MEM-20..23, NFR-SLO-04, US-06(要件書)
- ⚠️ 既知の限界: 30日より古い記憶はベクトル検索対象外(cold層は検索経路に未接続)、2文字の日本語語(会議・資料等)は語彙検索でヒットしない

### US-MEM-02: 書いた瞬間に検索できる(埋め込みは後追い) ✅
- Given: 新しいイベントがevent_logに書かれた直後
- When: そのキーワードで検索する
- Then: FTSで即ヒットする。ベクトル化は書き込み経路の外(embed backlog)で後から行われ、5分以内に意味検索にも乗る
- 根拠: `embed_job.rs` (off-write-path), FR-MEM-22

### US-MEM-03: APIキーやトークンはメモリに残らない ✅
- Given: 画面にAnthropicキーやghp_トークン等が表示されていた
- When: キャプチャ/文字起こし/要約が保存される
- Then: 22種のissuerプレフィックス・20種のラベルパターンにマッチする12文字以上の値は`[redacted]`にマスクされてから行が書かれる。会議中の自分のメモ(session_notes)だけは本人の言葉なのでマスクされない(意図的)
- 根拠: `redact.rs`、適用箇所: event_log/thread summary/session summary/recap/transcript
- 関連: NFR-SEC, 不変条件7周辺

### US-MEM-04: 30日を過ぎた記憶はCold層へ縮む ✅
- Given: Dream CycleのColdDemotionジョブが夜間に走る
- When: 30日より古いイベントのWarmベクトルが処理される
- Then: f32ベクトルはint8量子化されcold_embeddingsへ移動(cosine≥0.999維持)、event_vecから削除される。イベント本文とFTS検索性は残る
- 根拠: `cold.rs` (demote_older_than, 1トランザクション冪等)
- 関連: FR-MEM-10..12

### US-MEM-05: DBは暗号化でき、鍵はKeychainにしか無い ✅
- Given: 暗号化が有効
- When: DBファイルをファイルシステムから直接読む
- Then: 平文は読めない(SQLCipher)。鍵のDebug表示は`***redacted***`。既存平文DBは無損失で暗号化版へ移行できる
- 根拠: `lib.rs` (DbKey, encrypt_existing, テスト4件)
- 関連: NFR-SEC-01

### US-MEM-06: 全データをエクスポート/全削除できる ✅(2026-08-05修正)
- Given: 設定画面のデータ管理
- When: 「Export」/「Delete everything」を実行する
- Then: エクスポートはローカルJSON(ネットワーク送信なし)。全削除はスキーマを残してユーザーデータを1トランザクションで消す
- 根拠: `maintenance.rs` (export_json, delete_all)
- 関連: FR-SET-07
- ✅ **修正済み(2026-08-05)**: delete_allがtranscript_segments/meeting_recaps/compression_metricsを削除順に含み、会議ありでも全削除が成功する(回帰テスト追加)。export_jsonはthreads/sessions/notes/transcripts/recaps/provenance/traceabilityを含む完全版に

### US-MEM-07: AIコーディングセッションが記憶になる ✅
- Given: AI sessions取り込みがOn(オプトイン)
- When: Claude CodeのJSONLセッションログが取り込まれる
- Then: 会話ターンのみ(ツール呼び出し・思考ブロック・画像は除外)が1スレッドとして保存され検索可能になる。再取り込みは重複しない。シークレットはマスクされる
- 根拠: `ai_session.rs` (pure parser), `daemon.rs:704 ingest_ai_session`
- 関連: Phase R4

---

## 4. State tables・抽出・confidence (FR-ST群)

### US-ST-01: 根拠のないstateレコードは存在できない ✅
- Given: 任意のstate書き込み経路
- When: provenance(根拠イベント参照)なしで人物/プロジェクト/commitment/open loopを挿入しようとする
- Then: 書き込み前に拒否される(EmptyProvenance)。挿入時はstate_provenance行が同一トランザクションで作られ、失敗時は全ロールバック
- 根拠: `state.rs` (insert_with_provenance)
- 関連: FR-ST-02

### US-ST-02: 「明日返します」と書いたらSHOGUNが覚えている ✅
- Given: キャプチャまたはメール同期で "I'll send it tomorrow" / 「明日返します」等の文が入った
- When: ローカル抽出(モデル不使用)が走る
- Then: commitment/open loop候補がconfidence≤0.4(Low)で保存され、根拠イベントにリンクされる。Lowなので生成物には混ざらず、複数回の傍証(corroboration)か夜間のモデル分類で昇格するまで表に出ない。日本語キューは全てCJK文字を含み英語文には誤発火しない
- 根拠: `extract.rs` (EN/JAキュー、LOCAL_RULE_MAX_CONFIDENCE=0.4、テスト18件)
- 関連: FR-ST-12, US-07(要件書)

### US-ST-03: confidenceは時間で減衰し、傍証で回復する ✅(2026-08-05修正)
- Given: state行が30日間新しい根拠を得ていない
- When: 毎時のローカルメンテナンスが走る
- Then: confidence = base × 0.5^(経過/半減期) で再計算される(累積でなく導出、冪等)。独立した複数イベントの傍証があればbase confidenceが最大0.75まで上がる(Highには決して届かない — 断定はモデル分類のみ)
- 根拠: `recompute.rs` (decay_confidence, corroborate, ceiling 0.75)
- 関連: FR-ST-21, §6.4.6
- ✅ **修正済み(2026-08-05)**: corroborateが最新傍証イベントのtsへlast_evidence_atを前進させ、昇格が次のdecayを生き残る(連続実行テスト追加)

### US-ST-04: Lowは事実として語られない ✅
- Given: confidence 0.49のcommitmentと0.85のcommitmentがある
- When: Context Fusionが文脈を組み立てる/Memory APIが読み出す/Briefが生成される
- Then: High(≥0.8)は事実として、Medium[0.5,0.8)は`possibly:`接頭辞付きで、Low(<0.5)は完全に除外されて扱われる。この判定は単一実装(shogun-fusion::confidence)をUI/API両方が共有する
- 根拠: `confidence.rs` (band boundaries テスト), `memory_api.rs:12` (同一実装をimport)
- 関連: FR-ST-20, FR-CF-03

### US-ST-05: 別チャネルの同名人物は勝手に統合されない 🟡
- Given: Slackの`alice`とGitHubの`alice`、メールのAlice Smith
- When: 同期・キャプチャで人物が観測される
- Then: 完全一致のチャネルID(同チャネル+同値)のみ0.95で自動統合。名前だけの一致は0.35のPossibleとして新規人物を作り、後の確認に回す(誤統合しない)。ハンドルは`channel:value`形式でチャネル名前空間が分かれる
- 根拠: `identity.rs` (EXACT 0.95 / NAME_ONLY 0.35 / MERGE_THRESHOLD 0.5)
- 関連: FR-ST-10
- 🟡 未完: Possibleを確定統合するAPI・誤統合を分割するAPIが存在しない(docには記載)。また本番からobserve_identityを呼ぶ経路が未接続

### US-ST-06: 期限切れは自動でoverdueになる ✅
- Given: due_atを過ぎたopenなcommitmentがある
- When: 毎時メンテナンス/Dream Cycleが走る
- Then: statusがoverdueになり、open loopのstaleness_daysが経過日数に更新される(冪等)
- 根拠: `recompute.rs` (recompute_overdue_and_staleness)
- 関連: FR-ST-21

### US-ST-07: パネルからワンタップで解決できる ✅
- Given: Notchパネルのstateリストにcommitment/open loopが並んでいる
- When: 行をクリックする
- Then: done/closedになり、以後のドラフト・カウント・Brief・アクション候補から消える
- 根拠: `daemon.rs:1540 resolve_commitment / :1550 resolve_open_loop`(読み出し側でdone/closed除外)
- ✅ 2026-08-05修正: context_actionsもdone/closedを除外(回帰テスト追加)

---

## 5. Context Fusion・アクション提案 (FR-CF群)

### US-CF-01: 画面に関係あるアクションが150ms以内に並ぶ ✅
- Given: context cacheが常時プリアセンブルされている(押してから収集は禁止)
- When: Notchを展開する
- Then: 事前計算済みの≤4アクションが即座に表示される。スコア = 画面関連度 × confidence帯重み(High 1.0 / Medium 0.6)。intent(ユーザー入力)が一致する候補は+0.5ブースト。同一アクションは最高スコアのみ残る
- 根拠: `assemble.rs` (MAX_ACTIONS=4, scoring, dedup), `daemon.rs:1429 context_actions`
- 関連: FR-CF-01..03, SLO-02

### US-CF-02: 知らない相手の画面でもパネルは空にならない ✅
- Given: state tablesに関連レコードが1つもない画面
- When: パネルを開く
- Then: 汎用フォールバック(Save note / Search memory / Extract tasks — 全てローカルL1)が表示される。検索は画面の顕著語をシードにする
- 根拠: `assemble.rs` (generic_actions)
- 関連: FR-CF-04

### US-CF-03: 自動実行アクションに外部送信は構造的に存在しない ✅
- Given: L1として提案されるあらゆるアクション
- When: Fusionが候補にレベルを付与する
- Then: レベルは型(`Action::required_level()`)から導出され、Send系は必ずL3になる。LocalAction型には送信ヴァリアントが存在しないため、L1送信はコンパイル時に表現不能
- 根拠: `shogun-agents/src/permission.rs` + `assemble.rs` + cross-crate `tests/invariant4.rs`
- 関連: 不変条件4, FR-AG-01

---

## 6. エージェント実行 (FR-AG群)

### US-AG-01: L1は即実行、L2はワンタップ、L3は専用ボタン ✅
- Given: エージェントがアクションを提出した
- When: 実行エンジンがレベルで振り分ける
- Then: L1(通知・検索・アプリ起動・クリップボード・ドラフト保存)は即実行。L2(state更新・ドラフト生成)は確認待ちになり、タイムアウト内のワンタップで実行/超過でExpired(実行されない)。L3はエンジンでは拒否され、承認キューのみが受け付ける
- 根拠: `engine.rs` (submit/confirm/expire_due), `approval.rs`
- 関連: FR-AG-01..03

### US-AG-02: 送信は必ず全文プレビュー+専用ボタン ✅
- Given: Send reply等のL3アクションが承認キューに入った
- When: 承認UIを開く
- Then: 宛先・種別・**全文**(要約でない)・経路(直結/Composio経由)・使用キー種別が表示される。Enterキーでは確定できない(RequiresDedicatedButton)。10分放置でTimedOut(送信されない)
- 根拠: `approval.rs` (Preview::for_send, ConfirmIntent::EnterKey拒否, 10min timeout)
- 関連: FR-AG-03, US-04(要件書)

### US-AG-03: 承認済み送信は成功時のみ痕跡が残る ✅
- Given: L3承認が完了した送信
- When: 実行される
- Then: 成功時のみtraceability行(ダイジェスト+バイト数のみ、本文なし)が1行書かれる。失敗時は何も出ていないので記録されない。Composio経由はthird_party=trueで記録される。送信直前にサービスゲートが再適用される(二重ゲート)
- 根拠: `send_exec.rs` (execute_send, テスト9件)
- 関連: FR-TR-01, 不変条件3

### US-AG-04: 7種のプリセットエージェントが定義されている 🟡
- Given: Reply Drafter / Meeting Prep / Task Extractor / Follow-up Sentinel / Calendar Scheduler / Issue Triage / Note Capture
- When: 各プリセットの操作を参照する
- Then: 各操作にレベルが静的に定義され、送信系は全てL3。ローカル専用プリセットに送信操作は存在しない
- 根拠: `presets.rs` (PRESETS表, テスト6件)
- 関連: FR-AG-10..16
- 🟡 未完: 定義テーブルのみで、プリセットを実際に走らせるランタイム(プロンプト・LLM呼び出し・操作ディスパッチャ)は本クレート群に存在しない

### US-AG-05: Composio送信失敗時はドラフトに落ちる、成功と偽らない ✅
- Given: Gmail送信がComposio側で失敗した
- When: 実行結果が返る
- Then: FailedDraftSavedとなり(Sentとは決して報告されない)、ドラフトがGmailに保存される
- 根拠: `composio.rs` (on_composio_failure), `send_exec.rs`
- 関連: FR-C2-05
- ⚠️ 既知の問題: このフォールバックのドラフト作成自体が第三者egressだがtraceされない(#2)

---

## 7. Dream Cycle・Morning Brief (FR-DC/MB群)

### US-DC-01: 夜、条件が揃った時だけ夢を見る ✅
- Given: アイドル15分または画面ロック、かつ電源接続またはバッテリー30%以上、02:00–06:00窓内
- When: Dream Cycleのtickが走る
- Then: 条件成立でFull(6ジョブ: Consolidation→Compression→StateUpdate→ConfidenceRecalc→ColdDemotion→MorningBrief)、窓を逃したらDegraded(StateUpdate+ConfidenceRecalcのみ、モデル呼び出しなし)、同日再実行はしない
- 根拠: `dreamcycle/gate.rs`, `plan.rs`, `schedule.rs`
- 関連: FR-DC-01..03

### US-DC-02: 途中で落ちても翌晩は続きから ✅
- Given: 前夜のcycleが途中で失敗した(ledgerにdone/failedが残っている)
- When: 次のcycleが走る
- Then: doneのジョブはスキップされ、失敗地点から再開する。Consolidationは既存stateと重複照合するのでcrash-resumeでも二重追加しない。処理高水位(last_consolidated_to)により同じイベントを二度分類しない
- 根拠: `dreamcycle/run.rs` (resume), `jobs.rs` (idempotent consolidation), `shogun-memory/jobs.rs` (high-water mark)
- 関連: FR-DC-04

### US-DC-03: 夜間バッチはSelect KKキー、失敗してもローカル機能は止まらない ✅
- Given: Batch分類(Anthropic Batch API)が失敗し続けている
- When: 連続失敗日数が閾値を超える
- Then: インジケータがNormal→Amber→Redと変わるが、ローカル機能(検索・キャプチャ・Notch)は決してブロックされない。分類キーはSelect KK(型レベルでBYOKと分離、取り違えはコンパイルエラー)
- 根拠: `dreamcycle/health.rs` (local_features_blocked常にfalse), `llm/mod.rs` (SelectKkKey/ByokKey型分離)
- 関連: FR-DC-05, 不変条件5

### US-MB-01: 朝、今日やるべきことが既に並んでいる ✅
- Given: 前夜にDream Cycleが完了し、Calendar接続済み
- When: Morning Briefを開く
- Then: 今日の予定(時刻順)・期限が近いcommitments(overdue最優先→期日順)・open loops(staleness降順トップ5)・提案アクション(≤3)が表示される。Lowは除外、Mediumはpossibly付き、各項目に根拠イベントIDが付く
- 根拠: `brief.rs` (assemble_brief, キャップ5/3/5, provenanceテスト)
- 関連: FR-MB-01..05, US-02(要件書)

### US-MB-02: 生成に失敗した朝も空画面にならない ✅
- Given: 夜間のBrief生成が失敗した
- When: 朝Briefを開く
- Then: 縮退Brief(カレンダー+overdue commitmentsのみ、LLM文なし、degraded=true)が表示される
- 根拠: `brief.rs` (assemble_degraded), `daemon.rs:1624 local_morning_brief`
- 関連: FR-MB-04

---

## 8. 会議ノート (FR-MT群)

### US-MT-01: 会議ノートは出荷時オフ、同意なしに聴かない ✅
- Given: 初期状態(または設定ファイルが壊れている/機能追加前のファイル)
- When: 設定が読み込まれる
- Then: 必ずenabled=falseとして読まれる(fail-closed)。オンにしない限り検知もofferも一切走らない
- 根拠: `meeting/settings.rs` (serde(default), 破損ファイルテスト)
- 関連: FR-MT-01

### US-MT-02: 会議が始まると10秒のofferが出る ✅
- Given: 会議ノートOn。マイクが10秒以上使用中+会議アプリ(Zoom)/会議URL(meet.google.com)を検出
- When: 複合スコア(mic .40 / app .30 / controls .15 / calendar .10、最大0.95)が閾値を超える
- Then: 「taking notes in 8s」カウントダウン付きofferがNotchに出る。無視すれば開始、Not nowで今回だけスキップ(10分クールダウン)。カレンダー予定だけでは絶対にofferされない。検知段階で音声は1サンプルも読まれない(デバイス使用中フラグのみ)
- 根拠: `meeting/detect.rs` (重み、lookalikeホスト拒否テスト), `gate.rs` (10min cooldown)
- 関連: FR-MT-02..04

### US-MT-03: Offerを経ずに録音状態には絶対に入らない ✅
- Given: Idle状態の会議ステートマシン
- When: あらゆる入力を与える
- Then: Idle→Recordingの直接エッジは存在しない(全入力がno-effect)。必ずOffered経由。終了時は必ずStopAudioが最初に発行されてから区間クローズ・Recap生成に進む
- 根拠: `meeting/statemachine.rs` (recording_is_unreachable_without_passing_through_offered)
- 関連: FR-MT-07/08

### US-MT-04: 音声はRAMから出ない ✅
- Given: 録音中
- When: 音声パイプラインが動く
- Then: PCMは30秒上限のRAMリングのみ。VADで発話単位に切られ、whisper.cpp(オンデバイス)で文字起こし後、波形は即破棄。Transcriberは`&[f32]`しか受けない(ファイルパスを渡す口が型として存在しない)。永続化されるのはテキスト+provenance+ASR confidenceのみ
- 根拠: `audio/ring.rs` (MAX_SECONDS=30), `worker.rs`, `asr/mod.rs`
- 関連: 不変条件2, FR-MT-13..15

### US-MT-05: システム音声(相手の声)はmacOS 14.4+でボット無しに取れる ✅
- Given: macOS 14.4+、システム音声キャプチャ許可済み
- When: 会議録音を開始する
- Then: Core Audioプロセスタップで相手側音声を取得し、マイクと別VADで区切って文字起こしする。14.0–14.3や許可拒否時はマイクのみへ縮退(クラッシュしない)
- 根拠: `audio/capture/system_tap.rs` (Ok(None) degrade)
- 関連: FR-MT-14

### US-MT-06: 会議後すぐRecapが出る、モデルが無くても空にならない ✅
- Given: 会議が終了した(Stop押下または沈黙15分/アプリ終了)
- When: Recapが生成される
- Then: モデルありならSummary/Decisions/Picked up(Track押下でconfidence 1.0のstateに確定)。モデル無し/失敗時もタイトル・分数・メモ・イベント数の縮退Recapが必ず出る。負の経過時間は「-4分」と表示せずNoneにする
- 根拠: `meeting/recap.rs` (degraded), `minutes.rs` (tolerant parse)
- 関連: FR-MT-19, wireframe C4

### US-MT-07: 会議中のパネルは「メモを打つ場所」 ✅
- Given: 録音中にパネルを展開した
- When: メモを打つ
- Then: session_notesに1文書として保存される(上書き型)。ユーザー自身の言葉なのでマスクされない。ライブ文字起こしは表示しない(仕様)
- 根拠: `session_notes.rs` (upsert, 非redactの意図コメント), meeting-notes-ui-design §3.4

### US-MT-08: 録音の見落としがない ✅
- Given: 録音を止め忘れて蓋を閉じた/クラッシュした
- When: 次回起動時
- Then: 開きっぱなしの区間はstarted_at時点で閉じられる(ダウンタイムを跨ぐ架空の長さを発明しない)
- 根拠: `daemon.rs:602 close_abandoned_meetings`
- ✅ 2026-08-05修正: 起動時刻カットオフを追加し、現行runが開いたセッションは巻き込まれない(テスト追加)

---

## 9. 連携 (FR-INT/C2群)

### US-INT-01: Gmailを読むにはComposio同意(3開示)が必要 🟡
- Given: Gmail連携を有効にしようとしている
- When: オプトイン同意フローを完了する(第三者経由/データ種別/取り消し可能性の3開示すべて)
- Then: 同意後のみ15分間隔の同期が走り、メールがevent_logに入る(source=gmail)。同意なしでは同期も送信も行われない
- 根拠: `composio.rs` (Disclosures 3項目), `connectors.rs` (poller内consent確認)
- 関連: FR-C2-01, 連携実装ルール
- ✅ 2026-08-05修正: on-demand読み取り(fetch_on_demand)にも同意チェックを追加。読み取りegressトレースはポーラー・on-demand両経路で記録済みであることを確認(型レベルの読み取り同意ゲートは今後の強化課題)

### US-INT-02: Gmail送信はdraft-stopが既定ON ✅
- Given: Composio同意済み、draft-stop設定が既定(ON)
- When: 返信ドラフトを作った
- Then: Sendアクション自体が提示されない(グレーアウトでなく非表示)。draft-stopをOFFにして初めてL3送信が可能になる。この状態ではprepare_sendが型的に呼べない
- 根拠: `composio.rs` (SendCapability, offered_actions)
- 関連: FR-C2-02..04

### US-INT-03: 接続が切れても他のサービスは無事 ✅
- Given: Gmail/Calendar等の複数サービス接続中
- When: 1つのサービスのトークンが失効する
- Then: そのサービスだけamber(要再認証)になり、読み取りのみ許可・書き込み拒否。他サービスは影響なし。再認証でlast_syncを保ったまま復帰。切断時はKeychainトークンが必ず削除され、取り込み済みイベントは既定で残る(選択可)
- 根拠: `connection.rs` (state machine, isolation テスト)
- 関連: FR-INT-06/07

### US-INT-04: トークンは自動更新され、Keychain以外に置かれない ✅
- Given: アクセストークンの期限が60秒以内
- When: API呼び出しが必要になる
- Then: リフレッシュトークンで透過的に更新・Keychainに保存される。リフレッシュ不能ならNeedsReauth。ベンダー混線防止(SlackトークンがGoogleエンドポイントに行かない)
- 根拠: `token.rs` (TokenManager, ConfigSelector), `keychain.rs`
- 関連: 不変条件7

### US-INT-05: OAuthはPKCE+stateで、シークレットはURLに出ない ✅
- Given: サービス接続フロー
- When: ブラウザ認可→loopbackリダイレクト→コード交換が走る
- Then: S256 code_challenge、anti-CSRF state検証、verifierはブラウザURLに決して含まれない。ユーザーが拒否したらエラーとして扱う
- 根拠: `oauth.rs` (PKCE, parse_redirect), `oauth_flow.rs`
- ⚠️ loopbackソケット・CSRF検証パスは自動テストなし(featureゲート)

### US-INT-06: Slack投稿不可ならクリップボードに落ちる ✅(Wave1では到達不能)
- Given: Slack接続がWS管理者承認で不可
- When: Slack投稿アクションを実行しようとする
- Then: L2のクリップボードドラフトに縮退する(送信ではない)
- 根拠: `slack.rs` (resolve_post)
- 関連: FR-INT-30
- ⚠️ WaveがWave::Oneにハードコードされており、Slack自体が現状「Coming soon」(#17)

### US-INT-07: Wave 1はGmail+Calendar+Driveが同期される 🟡
- Given: Wave 1リリース状態で各サービス接続済み
- When: 15分ポーラーが走る
- Then: 読み取り同期がsourceタグ付きでevent_logに入る
- 根拠: `runtime.rs` (services_due, sync_service)
- ✅ 2026-08-05修正: transportが対応しないサービス(Calendar/Drive)はComing soon表示となり接続・同期対象から外れる(誤amber解消)。実transport実装までGmailのみ

---

## 10. Memory API — MCP/CLI/REST (FR-API群)

### US-API-01: 外部AIから自分のメモリを検索できる ✅
- Given: MCPクライアント(Claude Code等)にSHOGUNのMCPサーバーを登録済み
- When: `memory.search`ツールを呼ぶ
- Then: 人間UIの検索と同一のハイブリッド検索・同一のconfidenceゲート(Low除外/Medium possibly)で結果が返る。13ツール(検索/コンテキスト/state 4種のlist・get/ノート追記/提案/アクション実行)が公開される
- 根拠: `mcp.rs` (13 tools), `rest.rs` (render_reads共有), `db_backend.rs`
- 関連: FR-API-01..03, US-08(要件書)

### US-API-02: 外部AIが送信しようとしても勝手には送られない ✅(UI連携は❌)
- Given: 外部エージェントが`actions.execute`で`send_email`を呼んだ
- When: APIが処理する
- Then: 送信は実行されず202+approval_idでL3承認待ちになる。ローカル系(検索・通知・クリップボード)は即実行される
- 根拠: `rest.rs` (act), `shogun_api.rs` e2eテスト
- 関連: FR-API-04
- 🔴 **既知バグ**: その承認はプロセス内のプライベートキューに入り、Notch UIから確認する経路が存在しない(デスクトップ・shogun-api・shogun-mcpが3つの別キューを持つ)。fail-safe(何も送られない)だが承認フローが端から端まで繋がっていない(#1)

### US-API-03: トークンなしでは読み取りすらできない ✅
- Given: REST API(127.0.0.1:7464)
- When: Bearerトークンなしで/v1/memory/searchを叩く
- Then: 401。/v1/status と /v1/metrics(集計値のみ)だけが無認証。listenは127.0.0.1限定、ボディ256KiB上限
- 根拠: `server.rs` (bind_local, 認可), `memory_api.rs` (TokenRegistry)
- 関連: FR-API-06

### US-CLI-01: ターミナルから`shogun search`できる ✅
- Given: shogun-apiデーモン起動中、SHOGUN_API_TOKEN設定済み
- When: `shogun search 契約 更新` / `shogun commitments` / `shogun note "..."` / `shogun run '{"kind":"send_email",...}'` 等を実行
- Then: 対応するREST呼び出しが行われ、結果が表示される。exit code: 0成功/1 HTTP≥400/2構文エラー/3デーモン未起動
- 根拠: `shogun-cli` (parse/command/wire/http)
- 関連: FR-API-05
- ⚠️ 既知の問題: `--json`と`--no-screen`はパースされるが無効(no-op)。ヘルプの「Keychainから読む」記載は虚偽(env varのみ)

---

## 11. LLMレーン・課金境界 (不変条件5)

### US-LLM-01: 夜間分類とチャットでキーが混ざらない ✅
- Given: Select KKキー(Batch)とユーザーBYOKキー(Agent)が設定されている
- When: 任意のLLM呼び出しが行われる
- Then: BatchClient(Select KK)とAgentClient(BYOK)は型で分離されており、取り違えはコンパイルエラー。キーはDebug/Displayで常にマスクされる。拒否されたキー(401/403)はプロバイダ障害と区別されユーザーに別の対処が示せる
- 根拠: `llm/mod.rs` (型分離, redact_secrets)
- 関連: 不変条件5, ADR-002/003

### US-LLM-02: 全ての送信は https のみ・事前トレース ✅
- Given: あらゆるLLM/連携egress
- When: リクエストが構築される
- Then: http:// は拒否される(型で)。トレース行(ダイジェスト+長さのみ)はリクエスト送信**前**に記録されるので、401で終わったプロンプトも痕跡が残る
- 根拠: `llm/transport.rs`, `anthropic.rs` (trace before request)
- 関連: NFR-SEC-04, FR-TR-03

### US-LLM-03: BYOKはAnthropic+OpenAI互換3種に対応 ✅
- Given: 設定でprovider(Anthropic/OpenRouter/OpenAI/Gemini)とmodel idを選んだ
- When: エージェント推論を実行する
- Then: 選択プロバイダのAgent laneで補完が実行される。Batch laneはAnthropicのみ
- 根拠: `openai_compat.rs`, `anthropic.rs`
- ⚠️ ストリーミング未実装: complete()は全文を待って返すため「初トークン1s」SLOは現状測定不能(#11)

---

## 12. SLO・計測 (NFR-SLO群)

### US-SLO-01: 無計測は成功と見なされない ✅
- Given: 6つのSLO(展開100ms/アクション提示150ms/初トークン1s/検索500ms/cache更新300ms/アイドルCPU5%)
- When: /v1/metrics やFull UIのSLOカードを見る
- Then: 各SLOのp50/p95と予算内割合が返る。1件も計測が無い項目はmeasured:false ⇒ pass:false として表示される(沈黙≠成功)
- 根拠: `metrics.rs` (SloRegistry, measured:false⇒pass:false), `bin/shogun_api.rs`
- 関連: NFR-SLO-00..06
- ⚠️ FirstTokenは記録箇所がなく恒久的にmeasured:false(#11)

---

## 99. 疑わしい問題の索引

テスト・修正フェーズの対象。詳細と行番号は `docs/feature-status.csv` の ERR- 列を参照。

| # | 重大度 | 概要 | 対応 (2026-08-05) |
|---|---|---|---|
| ERR-01 | 🔴 Critical | delete_all が会議データのFK制約で全体失敗 | ✅ 修正+回帰テスト |
| ERR-02 | 🔴 High | 「Not now」が効かず10秒後に自動録音再開(decline未記録/一瞬のフォーカス切替で拒否消滅/時計逆行で即録音) | ✅ 修正+回帰テスト |
| ERR-03 | 🔴 High | Wave1のCalendar/Drive同期が常時失敗→amber | ✅ 修正(未対応サービスはComing soon表示・接続不可に) |
| ERR-04 | 🟠 High | Composio読み取りegressのトレース | ✅ 配線層で対応済みと確認(ポーラー+on-demandコマンド両方で記録) |
| ERR-05 | 🟠 High | corroboration昇格が次のdecayで即無効化 | ✅ 修正+連続実行テスト |
| ERR-06 | 🟠 Med | 送信失敗時ドラフトフォールバックの未トレース | ✅ 配線層で対応済みと確認(save_gmail_draftが記録) |
| ERR-07 | 🟠 Med | on-demand読み取りに同意ゲートなし | ✅ 修正(consent必須化) |
| ERR-08 | 🟠 Med | API発L3承認が3分裂キューで確認不能 | 📝 文書化(共有キュー設計が必要。現状fail-safe) |
| ERR-09 | 🟠 Med | Cold層が検索経路から到達不能 | 📝 文書化(設計課題) |
| ERR-10 | 🟠 Med | 検索エラーの吞み込み+trace route誤ラベル | 📝 文書化(routeフォールバックはDB CHECKで到達不能、.ok()はクラッシュ耐性設計) |
| ERR-11 | 🟡 Med | context_actionsに解決済み項目が混入 | ✅ 修正+回帰テスト |
| ERR-12 | 🟡 Med | 圧縮パスで要約がfact扱い | ✅ 修正(summary (unverified): ラベル) |
| ERR-13 | 🟡 Med | 会議declineクールダウンの脆弱性 | ✅ 修正(60秒の連続離脱でのみ解除) |
| ERR-14 | 🟡 Med | export_json不完全 | ✅ 修正(threads/sessions/notes/transcripts/recaps/provenance/traceability追加+テスト) |
| ERR-15 | 🟡 Med | Gmail直結MCP設定の残存 | 📝 文書化(GA復帰用の意図的余地 — CLAUDE.md準拠) |
| ERR-16 | 🟡 Med | CommitmentTheirs到達不能 | 📝 文書化(direction列の追加はスキーマ課題) |
| ERR-17 | 🟡 Med | dedup touchの横断誤touch/スレッド未更新 | ✅ 修正(アプリ単位スコープ+thread活動延長+テスト) |
| ERR-18 | 🟡 Low | CLI no-opフラグ・虚偽ヘルプ | ✅ 修正(usage文を実態に一致) |
| ERR-19 | 🟡 Low | Brief provenance欠落・updatedスタブ | 📝 文書化 |
| ERR-20 | 🟡 Low | preset()未知IDフォールバック | ✅ 修正(Option返却) |
| ERR-21 | 🟡 Low | dead code群・doc誤り | 📝 文書化(クリーンアップバックログ) |

### 追加修正(デスクトップUI)

- ✅ fullui/meetingウィンドウのTauri capability欠落(listen()/close()死亡) → 付与
- ✅ 展開レイテンシSLOが永久無計測(painted未呼び出し) → webviewから配線(interact/collapse_request含む)
- ✅ Full UI: 初期ペイン不一致、無反応ボタン(Fix導線→ペイン遷移、Run now→実行、死にボタン撤去)
- ✅ Full UIタイムスタンプのUTC表示 → OSローカルオフセット適用
- ✅ Onboardingのappearance JSONパースバグ(ライトモード不能)
- ✅ チャットの無限「考え中」→ 90秒タイムアウト
- ✅ key_rejectedの永続ラッチ → 成功時に解除
- ✅ 会議メモのblur時のみ保存 → 800msデバウンス自動保存+直前セッションへのフォールバック
- ✅ 沈黙15分自動終了が発火不能 → 音声レーンのlast_voiceを配線
- ✅ close_abandoned_meetingsが進行中会議を巻き込み得る → 起動時刻カットオフ
- ✅ analyticsトグルが日本語ハードコード+設定画面に不在 → strings.ts経由EN化+Settingsに追加
- ✅ ホバー領域400×180 < パネル560×360(カーソル下で勝手に閉じる) → 実寸に一致
- ✅ セキュア欄スキップの予算未計上 / 抽出open loopのopened_at / ledger書込失敗の黙殺 ほか

再テスト結果: 全スイート緑(shogun-core 431 / shogun-memory 216 / shogun-mcp 102 / shogun-integrations 54 / shogun-agents 33 / shogun-cli 30 / shogun-fusion 37、clippy警告0、tsc --noEmitクリーン)。macOS専用コード(apps/desktop/src-tauri)はLinux CI上でコンパイル検証不能のため、実機確認項目としてfeature-status.csvに明記。

---

## 13. デスクトップアプリ — Notchパネル (apps/desktop)

### US-DT-01: 起動するとノッチにピルが常駐する 🟡
- Given: SHOGUN.appを起動した(Dockアイコンなし=Accessory)
- When: 起動が完了する
- Then: NSPanel(nonactivating / canJoinAllSpaces / fullScreenAuxiliary / floating)としてノッチ直下にピルが表示される。Space切替・アプリ切替・スリープ復帰後も自己修復ウォッチャで常駐が維持される。トレイに⚔メニュー(Show/Hide, Quit)が出る
- 根拠: `lib.rs:993 adopt_native_panel`, `lib.rs:1197` (space watchers), `lib.rs:1236` (1Hz self-heal logger)
- 関連: FR-NU-01, Phase 0 Q1
- ⚠️ 既知の問題: 起動時に`open=true`ハードコードのため実際は展開状態で起動し、Idleピルに初回から到達できない(App.tsx:212)。640×300→560×360のリサイズフラッシュあり

### US-DT-02: ピルをクリック/ホバー滞留で展開する 🟡
- Given: Idleピル表示中
- When: クリックする、または250ms滞留する
- Then: チャットパネル(560×360)が展開する。ヘッダに現在読んでいるアプリ(● reading Mail)とdueカウント、ピン/履歴/Full UI/設定/最小化/終了ボタンが並ぶ
- 根拠: `App.tsx:580-618` (handle), `App.tsx:490` (HOVER_DWELL_MS 250)
- ⚠️ 既知の問題: RustホバーエンジンとWebview双方が独立にdwell判定(120/250ms vs 250ms)する二重実装。仕様のHoverプレビュー状態(A2)はUI未実装でhover=expanded扱い。展開レイテンシSLOの計測点`painted`をwebviewが一度も呼ばず、SLO-01は永久に無計測

### US-DT-03: パネルで自分の仕事について質問できる ✅
- Given: BYOKキー設定済み、パネル展開中
- When: 「Ask ShogunAI…」に質問を入力してEnter
- Then: referent解決(「あれ」の曖昧時は質問で返す)→メモリ検索→根拠付き回答が表示され、下部に≤4個の出典チップが付く。キー未設定なら送信せず設定への案内を返す
- 根拠: `inline_source.rs:836 shogun_chat`, `App.tsx:747-829`
- 関連: FR-CF群, US-06(要件書)
- ✅ 2026-08-05修正: 90秒タイムアウトを追加(失敗が可視化される)。⚠️ 残: ストリーミング未実装(SLO-03計測不能)、履歴のlocalStorage平文は文書化

### US-DT-04: どのアプリでも⌥タップでその場に下書き ✅
- Given: 任意アプリのテキスト欄にカーソルがある
- When: ⌥キーを単独で500ms以内にタップする
- Then: カーソル周辺の文脈+メモリからドラフトが生成され、AX APIでキャレット位置に挿入される(AX不可ならクリップボード保存・復元付き⌘Vフォールバック)。ピルに Drafting…→Drafted の状態が出て2.2秒で消える。キー拒否/文脈なし/失敗は区別して表示
- 根拠: `lib.rs:1392 watch_option_tap`, `inline_source.rs:601 run_inline_at_cursor`
- 関連: FR-AG(ドラフト), 不変条件2(AXテキストのみ)

### US-DT-05: 追跡中のcommitments/open loopsをパネルで解決できる ✅
- Given: ヘッダのカウントチップ(2 due · 1 waiting)
- When: チップをクリック→リスト行をクリック
- Then: リストが開閉し、行クリックで楽観的に消えてresolve_state_itemが走る。overdueは強調表示
- 根拠: `App.tsx:396-418, 725-745`, `inline_source.rs:715`
- ⚠️ 既知の問題: Undoなし。3秒ポーリングが折りたたみ中も走る

### US-DT-06: 提示されたコンテキストアクションを実行できる ❌
- Given: 画面文脈に応じたアクション(≤4)がRust側で組み立てられている
- When: パネルを開く
- Then: (期待)アクションボタンが並び、L1即実行/L2ワンタップ確認(8s)/L3承認へ回る
- 根拠: `notch_actions.rs`, `notch_exec.rs` (バックエンド完成)
- ❌ **UI未実装**: フロントエンドからnotch_actions/run_notch_action/confirm_notch_actionへの参照がゼロ。実行経路は⌘⇧Jのself-test(デバッグ用)のみ
- 関連: FR-CF-01, SLO-02

### US-DT-07: パネルから検索できる ❌
- Given: Full UIまたはパネル
- When: 検索窓に入力する
- Then: (期待)500ms以内にハイブリッド検索結果
- ❌ **検索UIがどの画面にも存在しない**。検索バックエンド(F-DB-SEARCH)は完成しているがUIから呼ぶ手段がなく、local_search SLOも永久に無計測
- 関連: FR-MEM-20, US-06(要件書)

### US-DT-08: パネルの設定で全機能を制御できる 🟡
- Given: ⚙︎で設定ビュー(560×460)を開く
- When: 各セクションを操作する
- Then: Approvals / Meeting notes / Connections / Composio / AI sessions / Nightly review / Appearance / Castle Position / Shortcuts / Model / Your key / Memory の12セクションが機能する
- 根拠: `App.tsx:1873-2077`
- ⚠️ 既知の問題: Connections の Connect は実OAuthを走らせずmark_connectedのみ(F-20)。needs_reauth状態でRe-authボタンがない。会議の「このアプリでは録らない」追加UIがない。キャプチャ除外の設定UIがどこにもない(exclusions.json を書くコードが存在しない)

### US-DT-09: BYOKキーはKeychainに保存され、拒否は明示される ✅
- Given: Model/Your keyセクション
- When: プロバイダ(Anthropic/OpenRouter/OpenAI/Gemini)を選びキーを保存
- Then: プロバイダごとにKeychainへ保存(last-4のみ表示)。401/403でkey_rejectedが立ち、設定とピルの両方に表示される
- 根拠: `inline_source.rs:40-203`
- ✅ 2026-08-05修正: 成功補完時にkey_rejectedラッチを自動解除。⚠️ 残: キーのテスト送信機能なし

### US-DT-10: メモリを消すには CLEAR とタイプする ✅
- Given: Memoryセクション
- When: 削除ボタンを押す
- Then: 件数付き確認が出て、`CLEAR`をタイプしないと実行されない。Escでキャンセル
- 根拠: `App.tsx:2018-2074`

### US-DT-11: パネル位置を6箇所から選べる(Castle Position) 🟡
- Given: 設定のCastle Positionミニ画面ピッカー
- When: notch/左右端/下辺3箇所のいずれかを選ぶ
- Then: 即座に移動し、castle.jsonに永続化。失敗時はロールバック
- 根拠: `App.tsx:1440-1502`, `lib.rs:1709`
- ⚠️ 既知の問題: パネルのCSS形状(上辺フラット・下角丸・上からのunfurl)がnotch位置専用で、他5箇所では視覚的に不正。ホバー判定領域は常にnotch位置のため、他位置ではクリック/ホットキーでしか開けない

## 14. デスクトップアプリ — 会議・Full UI・オンボーディング

### US-DT-12: 会議が始まると右上にofferバーが出る 🟡
- Given: 会議ノートOn、Zoom/Meet等を検出
- When: 検知スコアが閾値を超える
- Then: 400×88のオーバーレイに「Meeting detected + 会議名 + [Take notes(残り秒)] [Not now]」が表示され、放置で録音開始、録音中は経過時間+メモ入力+Stopが出る
- 根拠: `meeting.rs:508`, `MeetingOverlay.tsx`
- ✅ **修正済み(2026-08-05)**: NotNow/Stopでdeclineを記録(10分クールダウン)、他ウィンドウへの一瞬のフォーカス移動では拒否が消えない(60秒連続離脱でのみ解除)、時計逆行はカウントダウン再アンカー(録音開始しない)。回帰テスト追加
- ✅ 2026-08-05修正: capability付与(meeting/fullui)。イベント購読が復活し、モデル議事録がRecapに届く(実機確認は要デバイス)

### US-DT-13: 会議後のRecapで議事録を確認できる 🟡
- Given: 会議が終了した
- When: Recapカード(400×280)が出る
- Then: タイトル・分数・自分のメモ、モデル生成のSummary/Decisions/Next actionsが表示されDoneで閉じる
- 根拠: `meeting_recap.rs`, `MeetingOverlay.tsx`
- ✅ 2026-08-05修正: capability付与でmeeting_recapイベント購読が有効化。メモは800msデバウンスで自動保存され、区間クローズ直後は直前セッションへフォールバック保存

### US-DT-14: Full UIで文脈の健全性を確認できる 🟡
- Given: ⤢でFull UI(1200×820)を開く
- When: サイドバーの Today / Context Health / Sources / Memory / Activity / Traceability を切り替える
- Then: Coverage・Yield・Egress等の実数カード、traceability一覧(digest+bytesのみ)が表示される
- 根拠: `fullui.rs`, `FullUi.tsx`
- ✅ 2026-08-05修正: Fix導線はペイン遷移として動作、Run nowは実行可能、機能未実装のWhy?/Reviewボタンは撤去(Reviewはパネルへの案内文に)。初期ペインはToday。タイムスタンプはローカル時刻。Escクローズはcapability付与で復活。⚠️ 残: Today actions/schedule・Activity runs・confidence mixの空データ(バックエンド未配線)、開後のデータ更新なし、plan表示ハードコード

### US-DT-15: 初回起動でAccessibility権限を案内される ✅
- Given: 初回起動(権限未付与)
- When: オンボーディングウィンドウ(720×640)が出る
- Then: 「何が有効になるか/何を絶対にしないか」の2カラム+System Settings 4手順+[Do this later][Open Accessibility Settings]。権限付与を800msウォッチャが検知して成功画面(You're set + analyticsトグル + Open SHOGUN)へ。拒否してもクラッシュしない
- 根拠: `onboarding.rs`, `Onboarding.tsx`
- 関連: FR-OB-04, US-10(要件書)
- ✅ 2026-08-05修正: appearanceをJSONパースしライトモード有効化。analyticsトグルをパネルSettingsにも追加(いつでも変更可能)。文言はstrings.ts経由の英語に統一

### US-DT-16: 使用状況の収集はオプトアウトできる 🟡
- Given: オンボーディング成功画面のトグル
- When: オフにする
- Then: PostHogへのイベント送信が即時停止する(匿名ID、内容データなし)
- 根拠: `analytics.rs`, `AnalyticsToggle.tsx`
- ✅ 2026-08-05修正: パネルSettingsに「Usage analytics」セクションを追加

### US-DT-17: SLO計測が常時記録される 🟡
- Given: アプリ稼働中
- When: 展開・cache更新・CPUサンプル等が発生する
- Then: JSONL(metrics/)とSLOレジスタに記録され、Full UIとAPIで参照できる
- 根拠: `metrics.rs`, `integrate.rs`
- ✅ 2026-08-05修正: webviewからpainted(ダブルrAF)・interact(click/key/scroll)・collapse_request(esc)を配線。展開レイテンシSLOと誤発火判定が計測可能に(実機計測は要デバイス)

---

## 15. カバレッジ一覧(ストーリー→機能マッピング)

全ストーリー数: 52。対応機能インベントリ: shogun-core 51機能 / memory+fusion 40機能 / agents+mcp+integrations+cli 33機能 / desktop 38機能(重複統合後)。個別機能とストーリーの対応、実装状態・テスト結果は `docs/feature-status.csv` を正とする。
