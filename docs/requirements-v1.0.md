# SHOGUN 要件定義書 v1.0

| 項目 | 内容 |
|---|---|
| 文書ID | requirements-v1.0 |
| 対象プロダクト | SHOGUN (ShogunAI) — macOSアプリ |
| ステータス | **Phase 1 実装中**（純ロジックは実装・テスト済み、実機検証が残る。2026-08-20 実装反映） |
| 最終更新 | 2026-08-20（実装との突き合わせ反映。差分の記録は `docs/spec-implementation-drift-audit.md`） |
| 上位文書 | `/CLAUDE.md`（運用ルール。本書と矛盾する場合はCLAUDE.mdの絶対不変条件が優先） |
| 関連文書 | `docs/notch-ui-prototype-spec.md`（ノッチUIスパイク仕様）、`docs/phase0-dev-instructions.md`（Phase 0 開発指示書）、`docs/spec-implementation-drift-audit.md`（仕様と実装の乖離監査）、`docs/feature-status.csv`（機能単位の実装・テスト状況） |

## 本書の位置付け（必読）

- **【2026-08-20 実装反映】** 現在の開発フェーズは **Phase 1（v1）実装中**である。本書のFR/NFRの大半は既に実装されており、「先行実装してはならない」というPhase 0期の制約は**失効している**。**要件の文言（MUST/SHOULD）は受け入れ基準として引き続き有効**であり、実装済みだからといって緩めない。
- **実装状況の追跡は本書では行わない。** 機能単位の実装・テスト状況は `docs/feature-status.csv`、残タスクは `docs/phase1-findings.md` と `todo.md` を正とする。本書と実装が食い違って見えた場合は `docs/spec-implementation-drift-audit.md` を先に参照する。
- Phase 0（ノッチUIスパイク）の詳細は `docs/notch-ui-prototype-spec.md` / `docs/phase0-findings.md`。**4つの問いのうち実機実測を要する項目は物理ノッチMacで未決**であり、実機検証で閉じる。
- Phase 0の詳細仕様（常駐安定性・展開100ms・cache300ms+CPU5%・ホバー誤発火の4つの問い、およびNo-Go時のメニューバー＋パレット方式への転換基準）は `docs/notch-ui-prototype-spec.md` に完全準拠する。本書ではノッチUIスパイクの内容を重複記述しない。
- 本書の各要件（FR-xx / NFR-xx）は、実装者（人間またはAIエージェント）が**受け入れ基準**として参照できる粒度で書かれている。要件の変更はConventional Commits（`docs:`）でバージョン管理し、CLAUDE.mdの絶対不変条件7項に抵触する変更は行わない。
- 記法: **MUST** = 必須（違反は受け入れ不可）、**SHOULD** = 強い推奨（逸脱には判断記録が必要）、**MAY** = 任意。
- FR/NFR番号は安定識別子である。要件の統合・削除で欠番が生じても再採番しない（既存参照を壊さないため）。

---

## 1. プロダクト定義とビジョン

### 1.1 一言定義

ユーザーの仕事の**ワールドモデル**を構築し、Macのノッチから「ボタンを押して仕事が終わる」体験を提供する、ローカルファーストのAI OS。

SHOGUNは記録ツールではない。**状態の推定と実行**のプロダクトである。

### 1.2 ワールドモデルとは何か

SHOGUNにおけるワールドモデルとは、「ユーザーの仕事世界の現在状態の構造化表現」である。具体的には次の4つの問いに常時答えられるデータ構造を指す:

1. **誰と**仕事をしているか（people）
2. **何の**プロジェクトが動いているか（projects）
3. **何を約束**しているか（commitments）
4. **何が未完了**のまま開いているか（open_loops）

生のイベントログ（画面で何が起きたか）と、そこから推定された状態（state tables）を分離し、状態には必ず根拠（provenance）と確度（confidence）を付与する。これが「記録」と「ワールドモデル」の違いである。

### 1.3 「ボタンを押して仕事が終わる」

SHOGUNの中核体験は次のループである:

```
キャプチャ（Accessibility API・テキストのみ）
  → メモリ（event log + 3層メモリ）
  → 状態推定（state tables + confidence）
  → Context Fusion: f(state, screen_ctx, intent) → action
  → Notch UIに文脈アクションを提示（150ms以内）
  → ユーザーが押す（またはL1なら自動実行）
  → エージェントが実行（L1/L2/L3の権限モデル下で）
  → 結果とトレーサビリティログ
```

ユーザーが「AIに文脈を説明する」工程をゼロにする。文脈は常にSHOGUN側がプリアセンブル済みであり（context cache）、「押してから収集」は禁止である。

### 1.4 プロダクト原則（設計判断の優先順位）

| # | 原則 | 意味 |
|---|---|---|
| P1 | ローカルファースト | 生データはデバイス外に出ない。クラウドに出るのは処理用チャンクのみ |
| P2 | 状態の推定 > 記録 | 検索できるログではなく、行動可能な状態を作る |
| P3 | 実行 > 提案 | 最終成果は「終わった仕事」。ただし外部不可逆操作は必ずL3 |
| P4 | 人間UIとAI APIの完全対称 | 新機能はUI・MCP/CLI/RESTの両方から同一権限モデルで呼べる |
| P5 | 割り込まない | エラーも通知も、ユーザーの作業を中断させない形で伝える |
| P6 | メモリは年単位で生きる | 後方互換を破らない。スキーマはspatial-ready |

### 1.5 v1のゴール（成功条件）

- v1のユーザーが1日1回以上、Notchのコンテキストアクションから仕事を完了させる（実行系アクションの日次利用率をテレメトリのイベント数のみで計測。内容は送らない。§7.7参照）。
- 7日間トライアル→有料転換のファネルが計測可能な状態でリリースされている。
- CLAUDE.mdのSLO表（§7.1で詳細化）を全項目満たしている。

### 1.6 CLAUDE.md絶対不変条件との対応表

CLAUDE.mdの絶対不変条件7項と、本書でそれを具体化した要件の対応。実装レビュー時のチェックリストとして使う。

| 不変条件 | 内容 | 本書での具体化 |
|---|---|---|
| 1 | データの重心はRustコア | AR-03〜05（プロセス構成・webview禁止事項）、NFR-SLO-00（計測はRust側） |
| 2 | 画像・音声ファイルを保存しない（音声はRAM処理・テキストのみ永続化） | FR-CAP-01、NFR-PRV-01、FR-MT群 |
| 3 | 生データはデバイス外に出さない＋トレーサビリティ | AR-11/12、NFR-PRV-02、FR-TR-01〜04 |
| 4 | L1に外部送信を含めない。送信・投稿・カレンダー作成はL3 | FR-AG-01/02、FR-AG-14（カレンダー作成=L3）、§6.9サービス別表 |
| 5 | キーの分離（Select KK=バッチ系 / BYOK=推論系） | FR-DC-02、FR-MB-01、FR-AG-06、FR-BIL-02/04 |
| 6 | 人間UIとAI APIの完全対称＋同一L1/L2/L3 | FR-AG-04、FR-API-01〜06 |
| 7 | secretsはKeychainのみ | FR-INT-02、FR-C2-05、NFR-SEC-01/02 |

---

## 2. 用語集

| 用語 | 定義 |
|---|---|
| **Notch UI** | MacBookのノッチ領域（および擬似ノッチ）を常駐UIとして使うSHOGUNのメインサーフェス。NSPanelで実装。状態機械は§6.1 |
| **擬似ノッチ** | ノッチ非搭載Mac・外部ディスプレイで、メニューバー中央に表示する同型のフローティングパネル。挙動・状態機械はノッチと同一 |
| **3層メモリ** | Hot（直近24h、RAM常駐）/ Warm（30日、SQLite）/ Cold（全履歴、int8量子化＋期間パーティション）の3層構造。§6.3 |
| **event log** | キャプチャ・連携から得た生イベントの追記型ログ。不変（immutable）。状態はここから推定される |
| **state tables** | `people` / `projects` / `commitments` / `open_loops` の4テーブル。ワールドモデルの本体。§6.4 |
| **provenance** | stateレコードから根拠イベント（event log行）への参照。全stateレコードに必須 |
| **confidence** | stateレコードの確度。0.0〜1.0。閾値と生成物への渡し方の規則は§6.4.6 |
| **open_loops** | 「開いたまま」の事項。返信待ち、未読の重要スレッド、放置中のPRなど。ユーザーの明示的タスクとは限らない |
| **commitments** | ユーザーが他者に（または他者がユーザーに）明示的に約束した事項。期日を持ちうる |
| **Context Fusion** | `f(state, screen_ctx, intent) → action`。現在の状態・画面文脈・推定意図から次のアクションを合成する層。crates/shogun-fusion |
| **context cache** | 現在フォーカス中の文脈に対して常時プリアセンブルされるコンテキストパッケージ。フォーカス切替から300ms以内に更新 |
| **screen_ctx** | 現在フォーカス中のアプリ・ウィンドウ・テキストから抽出した即時文脈 |
| **intent** | ユーザーの直近の行動系列から推定される目的（例: 「この人に返信しようとしている」） |
| **L1（自動実行）** | 通知のみで自動実行されるアクション。**ローカル完結・可逆な操作のみ**。外部送信は絶対に含めない |
| **L2（ワンタップ承認）** | Notchに提案が出て1クリックで実行。デバイス外への影響が読み取り系または下書き系に限られるもの |
| **L3（明示確認）** | 実行内容のプレビュー（送信先・全文）を提示し、明示的な確認操作を要求。送信・投稿・カレンダー作成・削除など外部に不可逆な影響を持つ操作すべて |
| **Dream Cycle** | 夜間バッチ処理。統合・圧縮・state更新・confidence再計算・Cold層への降格。Select KKキー（Batch API）で実行。§6.7 |
| **Morning Brief** | Dream Cycleの成果物として毎朝生成される、その日の状態サマリと推奨アクション。§6.8 |
| **Select KKキー** | 運営会社（Select KK）が保有するAnthropic APIキー。インデックス・分類・Dream Cycle・Morning BriefのBatch API処理**のみ**に使う |
| **BYOK** | Bring Your Own Key。ユーザー自身のAnthropic APIキー。エージェント推論・チャット・ドラフト生成**のみ**に使う。v1はAnthropicのみ対応 |
| **第1層連携** | 各サービスの**公式リモートMCP**への直接接続。OAuthはユーザー→サービス直接、トークンはKeychain。v1はGmail / Google Calendar / Slack / Notion / GitHub / Linearの6つ |
| **第2層連携** | Composio経由の連携（オプトイン）。v1ではGmail送信のみ。トレーサビリティ画面に「第三者経由」を明示 |
| **Memory API** | SHOGUNのメモリ・状態への外部AIアクセス面。MCPサーバー / CLI / ローカルREST。人間UIと完全対称。§6.11 |
| **トレーサビリティログ** | デバイス外への全送信の記録。送信先・送信内容の要約・経路（直接/第三者経由）・承認レベルを含む。§6.14 |
| **ドラフト止まりモード** | 外部送信系エージェントの出力をドラフト作成までに制限する設定。Gmail送信（Composio）に必須で用意 |
| **spatial-ready** | 将来の空間コンピューティング対応（Phase 3: visionOS）に備え、`window_pose` / `gaze_target` / `dwell_ms` / `display_id` / `window_bounds` のカラム余地をスキーマに最初から確保する方針 |
| **Wave** | 第1層連携の段階導入単位。Wave 1 = Gmail + Google Calendar、Wave 2 = Slack、Wave 3 = Notion + GitHub + Linear |
| **FTS5 trigram** | SQLite FTS5のtrigramトークナイザによる全文検索。日本語等の分かち書き不要言語を含む多言語対応のため採用 |
| **refinery** | Rustのマイグレーション管理ライブラリ。スキーマ変更は必ずバージョン管理されたマイグレーションで行う |

---

## 3. スコープ定義

本表は「v1に足すかどうか」の判断根拠である。CLAUDE.mdはスコープ外要求を断る根拠として本表を参照する。**本表にないものをv1に足す場合は、本書の改版を伴わなければならない。**

### 3.1 v1に含むもの

| 領域 | v1に含む |
|---|---|
| キャプチャ | Accessibility API経由のテキストキャプチャ、除外リスト、一時停止 |
| メモリ | 3層メモリ、event log、FTS5 trigram検索、Warm層ベクトル検索（ローカルONNX embedding） |
| 状態 | state tables 4種（people/projects/commitments/open_loops）、provenance + confidence |
| UI | Notch UI（実ノッチ＋擬似ノッチ）、Full UI（検索・トレーサビリティ・設定）、Morning Brief表示 |
| Fusion | context cache常時プリアセンブル、コンテキストアクション提示 |
| 会議ノート（全プラン。トライアル中から使える） | 会議検知（session区間）、Notch Offered/Recordingピル、会議中ノート、オンデバイスASR（音声はRAMのみ・テキスト永続化）、Recap（要約＋決定事項＋約束候補）、三段オフ＋オプトイン（FR-MT群、`docs/meeting-notes-ui-design.md`） |
| エージェント | L1/L2/L3実行エンジン、プリセットエージェント7種、チャット、ドラフト生成（Pro） |
| バッチ | Dream Cycle、Morning Brief |
| 連携（第1層） | Gmail / Google Calendar / Slack / Notion / GitHub / Linear（公式リモートMCP、Wave 1→3） |
| 連携（第2層） | Composio: Gmail送信のみ（オプトイン、Pro） |
| API | Memory API: MCPサーバー / CLI / ローカルREST（Pro） |
| 課金 | Stripe、7日間フルトライアル、Standard / Pro、ライセンス検証、オフライン猶予 |
| 配布 | Developer ID + notarization、Tauri updater |
| 言語 | UI英語のみ（i18n-ready構造）、生成物の出力言語はユーザー設定 |

### 3.2 v1に含まないもの（時期を含めて明記）

| 項目 | 予定 | v1で断る根拠 |
|---|---|---|
| 録音ファイル・録画ファイルの保存 | **恒久的に対象外** | 不変条件2。回避策も設計しない。会議音声は聞いてテキストのみ残す（下記FR-MT群） |
| 会議URL/カレンダー経由のbot会議参加 | **恒久的に対象外** | 取得するのはMac出力音声のみ。会議への自動送信・投稿もしない（Issue #7 Non Goal） |
| ナレッジグラフ（エンティティ間グラフ構造） | **v2** | v1のワールドモデルはstate tables 4種で表現する |
| マルチデバイス同期・クラウドバックアップ | **v2** | ローカルファースト優先。同期はE2E暗号化設計が前提のためv2 |
| メタメモリ（記憶についての記憶・自己評価） | **v2** | Dream Cycleのconfidence再計算までがv1 |
| Computer Use（画面操作の自動実行） | **Phase 3** | v1の実行はMCP経由の構造化操作のみ |
| visionOS対応 | **Phase 3** | v1はspatial-readyなスキーマ確保に留める |
| Intel Mac対応 | 将来判断（未決事項 §9） | Apple Siliconのみ公式サポート（付録A ADR-005） |
| Windows / Linux対応 | 予定なし | macOSネイティブAPI（AXUIElement等）に依存 |
| iOSコンパニオンアプリ | 予定なし（v2以降で再検討） | — |
| Freeプラン | 予定なし | 課金方針（§6.12）。トライアルで代替 |
| チームプラン・組織管理 | 未決事項（§9） | v1は個人ワーカー向け |
| Slack以外のチャット（Discord, Teams等） | 予定なし（要望次第でv2検討） | 第1層は6サービスに固定 |
| クラウドembedding API | 予定なし | ローカルONNX固定（付録A ADR-001） |
| Anthropic以外のBYOKプロバイダ | 将来判断 | v1はAnthropicのみ。trait抽象化のみ確保（付録A ADR-002） |
| App Store配布 | 予定なし | Developer ID直接配布（CLAUDE.md確定） |
| モバイル通知・リモート通知 | 予定なし | 通知面はNotch UIとmacOS通知のみ |

### 3.3 スコープ判断の運用

- 実装中にスコープ外の要望・アイデアが出た場合、実装者は本表の該当行を引用して確認を取る。**確認なしにv1スコープへ追加しない。**
- 本表に該当行がない新規要望は「v1に含まない」をデフォルトとし、§9（未決事項）に追記して判断を待つ。

---

## 4. 想定ユーザーと主要ユーザーストーリー

### 4.1 想定ユーザー

**AIネイティブな個人ワーカー。** 具体的には:

- ファウンダー / インディーハッカー: 複数プロジェクト・複数の相手と並行して動き、返信漏れ・フォローアップ漏れが直接損失になる
- ビルダー / エンジニア: GitHub / Linear / Slackを横断し、コンテキストスイッチのコストが最大の敵
- リサーチャー / ナレッジワーカー: 大量の読み物・スレッド・ドキュメントから「自分が何を追っていたか」を失いやすい

共通特性: 既にAIチャットを日常的に使っており、「毎回文脈を説明し直す」ことに不満を持つ。BYOKに抵抗がない層を含む。macOSユーザー。仕事言語は英語または英語＋母語（生成物の出力言語設定が効く）。

### 4.2 ユーザーストーリー（受け入れシナリオ形式）

各ストーリーは Given / When / Then 形式。関連FRを併記する。

**US-01: 文脈を説明しない返信ドラフト**
- Given: ユーザーがGmail（第1層接続済み）で取引先からのメールスレッドを画面に表示している。SHOGUNのstate tablesにはこの相手（people）と関連プロジェクト（projects）のレコードがある
- When: Notchにホバーする
- Then: 150ms以内に「Draft reply」アクションが表示され、押すと1s以内に最初のトークンがストリーミング表示される。ドラフトはスレッド内容とstate（過去の約束・プロジェクト状況）を反映している。confidenceが閾値未満の状態は断定でなく可能性として表現されている
- 関連: FR-CF-01〜03, FR-AG-10, FR-ST-20

**US-02: 朝、今日やるべきことが既に並んでいる**
- Given: 前夜にDream Cycleが完了している。Google Calendar接続済み
- When: 朝、Macを開いてNotchのMorning Briefインジケータを開く
- Then: 今日の予定・期限が近いcommitments・放置中のopen_loops・推奨アクション（それぞれL2/L3ボタン付き）が1画面で表示される。生成言語はユーザー設定の出力言語である
- 関連: FR-MB-01〜05, FR-DC-01

**US-03: 会議前の自動準備**
- Given: 15分後にCalendar上の会議がある。参加者はstate tablesのpeopleに存在する
- When: 会議の15分前になる
- Then: L1として「Meeting Prep準備完了」の通知がNotchに出る（自動実行はローカルでの資料集約のみ）。開くと、参加者ごとの直近のやり取り・未解決のcommitments・関連ドキュメントへの参照が表示される。外部への送信は一切発生していない
- 関連: FR-AG-11, FR-CF-02

**US-04: 送信は必ず自分が最終確認する**
- Given: Pro + Composio（第2層）オプトイン済み。ドラフト止まりモードはOFF
- When: 「Send reply」アクションを実行する
- Then: 送信先アドレス・件名・本文全文のプレビューがL3確認ダイアログで表示され、明示的な確認操作をするまで送信されない。送信後、トレーサビリティ画面に「第三者経由（Composio）」の記録が残る
- 関連: FR-AG-03, FR-C2-02〜04, FR-TR-01

**US-05: パスワードマネージャは絶対に記録されない**
- Given: 既定の除外リストが有効
- When: パスワードマネージャアプリを開いて操作する
- Then: その間のキャプチャイベントは一切生成されない（保存もされない）。Notchのインジケータがキャプチャ一時停止状態の色になる。アプリを閉じるとキャプチャが再開する
- 関連: FR-CAP-05〜07

**US-06: 「あれどこだっけ」が500msで返る**
- Given: 過去2週間に読んだ資料の内容の断片（日本語）だけ覚えている
- When: Full UIの検索に断片を入力する
- Then: 500ms以内にFTS5 trigram＋Warm層ベクトル検索のハイブリッド結果が返り、当時のアプリ名・ウィンドウタイトル・前後の文脈が表示される
- 関連: FR-MEM-20〜22, NFR-SLO-04

**US-07: 返信し忘れをSHOGUNが覚えている**
- Given: 3日前にSlackで「明日返します」と書いたスレッドが未返信
- When: Dream Cycleがcommitmentsの期限超過を検出する
- Then: Morning Briefと Notchに「Follow-up」提案（L2）が出る。押すとドラフトが生成され、Slack投稿はL3確認を経る。Slack接続がWS管理者承認で不可の場合は、ドラフトのクリップボードコピーにフォールバックする
- 関連: FR-AG-13, FR-INT-30, FR-ST-12

**US-08: 外部AIから自分のメモリを使う**
- Given: Proユーザー。外部のAIコーディングエージェントにSHOGUNのMCPサーバーを登録済み
- When: 外部エージェントが `memory.search` ツールを呼ぶ
- Then: 人間UIの検索と同一の結果が返る。外部エージェントが書き込み系ツールを呼んだ場合、人間UIと同一のL1/L2/L3判定が適用され、L3操作はNotchでの明示確認なしに完了しない
- 関連: FR-API-01〜06

**US-09: トライアルから納得して課金する**
- Given: 初回起動から7日間、Pro相当の全機能を使った
- When: 8日目に起動する
- Then: 機能がロックされ、Standard / Proの選択画面（年払い/月払い価格併記）が表示される。選択→Stripe決済完了で1分以内に機能が解放される。ローカルのメモリデータは消えていない
- 関連: FR-BIL-02〜05

**US-10: 権限を拒否しても壊れない**
- Given: 初回起動でAccessibility権限を拒否した
- When: オンボーディングを続行する
- Then: アプリはクラッシュせず、キャプチャ以外の機能（連携の読み取り統合、Morning Brief、検索は連携データのみ）が動作する。Notchインジケータが権限不足の状態色を示し、設定画面から権限付与への導線が常設される
- 関連: FR-CAP-08, FR-OB-04

**US-11: フルスクリーン作業を邪魔しない**
- Given: ユーザーが動画編集アプリをフルスクリーンで使用中
- When: SHOGUNがL2提案を生成した
- Then: Notchパネルは自動表示されず、提案はキューに保持される。フルスクリーン解除後、Notchインジケータの色で提案の存在が示される
- 関連: FR-NU-08

---

## 5. システムアーキテクチャ

### 5.1 リポジトリ構成

既存のpnpmモノレポ（`apps/website` 等を含む）のルートに **Cargo workspaceを追加統合**する。JS側（pnpm/turbo）とRust側（cargo）は同一リポジトリで共存し、`apps/desktop`（Tauri）が両者の接合点となる。

```
（リポジトリルート）
├── package.json / pnpm-workspace.yaml / turbo.json   # 既存JSモノレポ
├── Cargo.toml                                        # [workspace] 追加
├── crates/
│   ├── shogun-core/         # デーモン: キャプチャ、DB所有、context cache、イベントバス、LLMレーン、音声/ASR
│   ├── shogun-memory/       # スキーマ、refineryマイグレーション、3層メモリ、state tables、検索
│   ├── shogun-fusion/       # Context Fusion: f(state, screen_ctx, intent) → action
│   ├── shogun-agents/       # L1/L2/L3実行エンジン、プリセットエージェント、LLMプロバイダtrait
│   ├── shogun-mcp/          # MCPクライアント（第1層）＋MCPサーバー/REST（Memory API）＋スコープ表
│   ├── shogun-integrations/ # 第1層コネクタアダプタ（公式リモートMCP）＋Composio第2層  ★2026-08-20 追記
│   ├── shogun-license/      # ライセンストークン検証                                    ★同上
│   ├── shogun-redact/       # 秘匿値マスク（書き込み前・ログ前）                        ★同上
│   ├── shogun-cli/          # shogunコマンド
│   └── spike-harness/       # Phase 0スパイク用ハーネス（製品コードではない）           ★同上
├── apps/
│   ├── desktop/          # Tauri v2アプリ（Notchパネル、Full UI、設定）React + TS
│   ├── api/              # Select KKバックエンド: Batch relay（/v1/batch）＋会議ASRの短命トークン発行 ★同上
│   └── website/          # 既存
└── docs/
```

**AR-01（MUST）**: 上記のcrate分割・責務配置に従う（**【2026-08-20 実装反映】** `shogun-integrations` / `shogun-license` / `shogun-redact` / `spike-harness` および `apps/api` を実装に合わせて追記）。crate間の依存方向は `core → memory/fusion/agents/mcp` を集約点とし、循環依存を作らない。

**AR-02（MUST）**: JSワークスペースとCargoワークスペースのビルドは独立して成立する（`pnpm build` がRustツールチェーン無しで、`cargo build` がNode無しで通る。`apps/desktop` のフルビルドのみ両方を要求してよい）。

### 5.2 プロセス構成

**AR-03（MUST）**: v1は「**同居構成**」を採用する。shogun-coreは独立OSデーモン（launchd常駐）ではなく、Tauriアプリプロセス内のRustバックエンドとしてホストされる。ただし:

- shogun-coreはライブラリとして**プロセス境界を前提としない設計**（内部イベントバス経由の疎結合）にし、将来のデーモン分離（launchd化）をアーキテクチャ変更なしに行えること。
- Tauriアプリはログイン項目（Login Item）として自動起動し、ウィンドウを全て閉じてもプロセスは常駐する（メニューバー/Notch常駐アプリとして振る舞う）。
- 判断理由: v1では配布・権限（Accessibility権限はアプリ単位）・更新（Tauri updater）の単純さがプロセス分離の利益を上回る。分離は将来のCLI/外部API常時稼働要件が強まった時点で再判断する。

**AR-04（MUST）**: **データの重心はRust側に置く**（CLAUDE.md不変条件1）。DB・キャプチャ・context cache・SLO計測はRust（shogun-core以下）が単独所有する。webview（React）側は表示とユーザー入力のみを担い、以下を**禁止**する:
- webviewからのSQL発行・DBファイルアクセス
- webviewでの検索ランキング・状態推定・キャッシュ組み立てロジック
- webviewでのsecrets保持（トークン・APIキーをJSに渡さない。§7.5）

**AR-05（MUST）**: UIとRust間の通信はTauri command（要求/応答）とTauri event（Rust→UIのプッシュ）に限定する。UIに渡すデータは表示用に整形済みのDTOとし、event logの生レコードをそのまま渡さない。

### 5.3 内部イベントバス

**AR-06（MUST)**: shogun-core内に単一のイベントバス（Rust、mpsc/broadcastベース）を置き、以下のイベント種別を流す:

| イベント | 発行元 | 主な購読者 |
|---|---|---|
| `capture.text` | キャプチャ | memory（event log書き込み） |
| `focus.changed` | キャプチャ（NSWorkspace） | fusion（context cache再構築） |
| `cache.updated` | fusion | UI（Notchアクション更新） |
| `state.updated` | memory / Dream Cycle | fusion、UI |
| `action.proposed` | fusion / agents | UI（L2/L3提示） |
| `action.executed` | agents | memory（event log）、トレーサビリティ |
| `integration.synced` | mcp（第1層） | memory |
| `error.raised` | 全コンポーネント | UI（インジケータ色） |

**AR-07（MUST）**: イベントバスの購読者が遅延・失敗しても発行者（特にキャプチャ）をブロックしない（バックプレッシャは有界キュー＋古いイベントのドロップで処理し、ドロップ数をメトリクスに記録する）。

### 5.4 context cacheの常時プリアセンブル

**AR-08（MUST）**: context cacheは「押してから収集」を禁止し、以下のトリガで**常時先行更新**する:
- `focus.changed`（アプリ/ウィンドウ切替）→ 300ms以内に再構築完了（NFR-SLO-05）
- `capture.text`（同一ウィンドウ内の内容変化）→ デバウンス500msで増分更新
- `state.updated` → 影響範囲のみ差分更新

**AR-09（MUST）**: context cacheの内容は最低限、次を含む: screen_ctx（フォーカス中アプリ・ウィンドウタイトル・可視テキストの抽出）、関連stateレコード（people/projects/commitments/open_loopsのうち関連度上位、各confidence付き）、直近のHot層イベント要約、実行可能アクション候補（L1/L2/L3タグ付き）。

**AR-10（MUST）**: context cacheはRAM上に保持し、ディスクへ永続化しない（クラッシュ後は再構築する。再構築はフォーカスイベントを待たず起動時に即実行）。

### 5.5 データフロー上の境界（プライバシー境界）

**AR-11（MUST）**: デバイス外への通信は以下の5経路のみとし、それ以外の外部通信をコードに書かない:

| 経路 | 内容 | キー/認証 | トレーサビリティ |
|---|---|---|---|
| Anthropic Batch API | Dream Cycle・Morning Brief・インデックス/分類の処理用チャンク | Select KKキー | 必須 |
| Anthropic Messages API | エージェント推論・チャット・ドラフト | ユーザーBYOK | 必須 |
| 第1層公式リモートMCP | 各サービスの読み書き | ユーザー→サービスOAuth | 必須 |
| Composio（第2層） | Gmail送信のみ（v1） | オプトイン接続 | 必須（「第三者経由」明示） |
| Stripe / ライセンスAPI / updater | 課金・ライセンス検証・更新確認 | アカウント認証 | 課金系は対象外（キャプチャ内容を含まないため）。ただし通信自体は§7.7の方針に従う |

**AR-12（MUST）**: 上記いずれの経路でも、event logの生レコード全体・スクリーン全文の無差別送信を行わない。送信されるのは目的に必要な**処理用チャンク**（抽出・整形済みの断片）のみである。

### 5.6 Phase 1実装順序（マイルストーン）

CLAUDE.mdのPhase 1定義に従い、以下の順序で実装する。各マイルストーンは前段の受け入れ基準充足を開始条件とする。

| M | 内容 | 主な対応要件 | 完了条件 |
|---|---|---|---|
| M1 | Notch UI本実装（Phase 0スパイクの製品化） | §6.1 | 状態機械全遷移＋NFR-SLO-01/02の計測合格 |
| M2 | キャプチャ＋メモリ＋state tables | §6.2〜6.4 | 24h連続稼働・検索SLO・マイグレーションCI合格 |
| M3 | Context Fusion＋L1/L2エージェント＋Dream Cycle/Morning Brief | §6.5〜6.8 | US-01/02/03のE2E合格 |
| M4 | 第1層MCP連携（Wave 1→順次）＋L3実行＋第2層 | §6.9〜6.11 | 許可範囲表・トレーサビリティのテスト合格 |
| M5 | 課金＋トライアル＋オンボーディング＋配布 | §6.12〜6.13、§7.6 | トライアル→課金E2E・notarization済みビルド |
| M6 | 会議ノート（MT1〜MT5、下記） | §6.16 | §6.16受け入れ基準7項目の合格（特に「音声がディスクに書かれない」検査） |

**M6の内訳（MT採番。Phase 1のM1〜M5とは別系統）** — 詳細は `docs/meeting-notes-ui-design.md` §7:

| MT | 内容 | 音声 | 依存 |
|---|---|---|---|
| MT1 | `sessions` + 会議検知 + Notch の Offered/Recording ピル | **なし** | M1（Notch）、M2（DB） |
| MT2 | 会議中ノート（`session_notes`）+ 終了検知 + 縮退Recap | **なし** | MT1 |
| MT3 | オンデバイスASR + `transcript_segments` + リングバッファ上限 | あり | MT2 + OPEN-07/08 |
| MT4 | Recap本体（Batch要約 + 候補抽出 + `[Track]`確定） | — | MT3 + Select KKキー |
| MT5 | 三段オフ・除外リスト・オンボーディングのオプトイン | — | MT1と並行可 |

**MT1/MT2は音声なしで出荷できる。** 検知・器・UIが先に立ち上がっていれば、ASRは「解像度を上げる差し替え」で済む。**逆順にしない。**

---

## 6. 機能要件（FR群）

各FRは「目的 / 詳細要件 / プラン / SLO / エラー時挙動 / 受け入れ基準」の観点を含む。プラン列の意味: **Std** = Standard以上、**Pro** = Proのみ、**All** = トライアル含む全プラン。

### 6.1 Notch UI

**目的**: SHOGUNの主要サーフェス。常駐しつつ作業を邪魔せず、文脈アクション・状態・エラーを最小の視覚コストで提示する。

#### 6.1.1 状態機械

**FR-NU-01（MUST, All）**: Notch UIは以下の状態機械に従う。図中の遷移以外を実装しない。

```
                 hover 120ms持続
   ┌─────────┐ ───────────────→ ┌─────────┐  click / ⌘⇧Space
   │  Idle   │                  │  Hover  │ ───────────────→ ┌──────────┐
   │(常駐表示)│ ←─────────────── │(プレビュー)│                 │ Expanded │
   └─────────┘   マウス離脱200ms └─────────┘ ←─────────────── └──────────┘
        ↑                                      ESC / 外側クリック      │
        │                                     / 20s無操作            │ "Open Full UI"
        │ フルスクリーン解除                                          ↓
   ┌─────────┐                                            ┌──────────────┐
   │ Hidden  │ ←── フォーカスアプリがフルスクリーン化          │ Full UI(別窓) │
   └─────────┘                                            └──────────────┘
```

- **Idle**: ノッチ形状に沿った最小表示。インジケータ（FR-NU-06）のみ描画
- **Hover**: ホバー120ms持続で遷移（誤発火防止。Phase 0の計測結果で60〜200msの範囲で調整可）。次のアクション候補1件と状態サマリ1行をプレビュー
- **Expanded**: クリックまたはグローバルホットキー（既定 `⌘⇧Space`、変更可）で遷移。Idle→Expanded直接遷移も可（ホットキー時）。コンテキストアクション最大4件、Morning Brief導線、チャット入力欄（Pro）を表示
- **Hidden**: フルスクリーン時（FR-NU-08）

**FR-NU-02（MUST, All）**: Idle→Expandedの描画完了は**100ms以内**（NFR-SLO-01）。Expandedの初回表示時点でアクション候補が確定していること（cacheプリアセンブルの帰結。プレースホルダやスピナーで代替しない）。

**FR-NU-03（MUST, All）**: NSPanel実装は `.nonactivatingPanel` + `.canJoinAllSpaces` + `.fullScreenAuxiliary` を使用し、Notch操作でフォーカス中アプリのキーフォーカスを奪わない（チャット入力欄クリック時のみ例外的にキー入力を受ける）。

#### 6.1.2 擬似ノッチとマルチディスプレイ

**FR-NU-04（MUST, All）**: ノッチ非搭載Mac、および外部ディスプレイでは、**メニューバー中央**に実ノッチと同型・同状態機械のフローティングパネル（擬似ノッチ）を表示する。実ノッチと擬似ノッチでUI・挙動・SLOに差を設けない。

**FR-NU-05（MUST, All）**: マルチディスプレイ時の表示規則:
- パネルは**キーウィンドウのあるディスプレイ**（＝ユーザーの作業中ディスプレイ）に1つだけ表示する
- フォーカスがディスプレイ間を移動したら500ms以内にパネルが追従する
- 内蔵ディスプレイ（実ノッチ）と外部ディスプレイ（擬似ノッチ）の間を移動しても状態（Expanded中の内容等）を維持する
- 設定で「内蔵ディスプレイ固定」を選択可能

#### 6.1.3 インジケータとエラー通知

**FR-NU-06（MUST, All）**: Idle状態のインジケータは色で状態を伝える。モーダルやバナーでエラーを出さない（CLAUDE.md: エラーはユーザーの作業を中断させない）。

| 色 | 状態 |
|---|---|
| 白（微光） | 正常・キャプチャ稼働中 |
| ゴールド | 新しい提案あり（L2/L3待ち）または Morning Brief未読 |
| グレー | キャプチャ一時停止中（手動 / 除外アプリ表示中 / 権限なし） |
| アンバー | 劣化動作（連携の一部失敗、Batch API失敗、ライセンス猶予中など。機能は継続） |
| 赤 | 要対応エラー（DB書き込み失敗、ライセンス失効等）。Hoverで内容と対処導線を表示 |

**FR-NU-07（MUST, All）**: アンバー/赤の全状態はHoverプレビューで1行説明＋対処アクションに到達できること。エラー詳細はFull UIのログ画面へ導線を張る。

#### 6.1.4 フルスクリーン時挙動

**FR-NU-08（MUST, All）**: フォーカス中アプリがフルスクリーンの間、パネルはHiddenに遷移し自動表示しない。生成された提案はキューに保持し、フルスクリーン解除後にインジケータ色（ゴールド）で存在を示す。グローバルホットキーによる明示呼び出し時のみ、フルスクリーン上にも `.fullScreenAuxiliary` でExpanded表示してよい。

**受け入れ基準（6.1）**: 状態機械の全遷移がUIテストで検証されている。実ノッチ機・非ノッチ機・外部ディスプレイ接続の3構成で FR-NU-04/05 が手動テストされている。NFR-SLO-01の計測コードが同梱されている。

### 6.2 キャプチャ

**目的**: ユーザーの作業文脈を、プライバシー不変条件の範囲内で受動的に取得する。

**FR-CAP-01（MUST, All）**: キャプチャは **Accessibility API（AXUIElement）経由のテキストのみ**。スクリーンショット・画像・動画・OCRを取得・保存するコードを書かない（CLAUDE.md不変条件2）。**【2026-08-02 明示的例外｜Visual recall】** Visual recall が On のときにかぎり、AX からテキストが取得できなかったウィンドウについて圧縮 JPEG を暗号化済みメモリ DB（`screen_frames`）に最大 72 時間保持し、期限切れは自動削除する。既定は Off、クラウド送信なし、音声は対象外、永続タイムラインではない（CLAUDE.md 不変条件2の例外）。会議音声は本節のキャプチャレーンとは**別レーン**であり、FR-MT群（§6.16）の条件下（Recording中のみ・RAM内処理・テキストのみ永続化）でのみ扱う。**常時キャプチャの一部として音声を取得しない。**取得対象: フォーカス中ウィンドウのアプリ名（bundle id）・ウィンドウタイトル・可視テキスト（AX階層から抽出）・フォーカス要素のロール、およびNSWorkspaceからのアプリ切替イベント。

**FR-CAP-02（MUST, All）**: 取得タイミング: (a) フォーカス切替時、(b) 同一ウィンドウでのAX通知（値変更・タイトル変更）を500msデバウンスで。ポーリングを行う場合は2s間隔を下限とし、アイドル時CPU 5%制約（NFR-SLO-06）に収める。

**FR-CAP-03（MUST, All）**: 重複抑制: 直前キャプチャとのテキスト差分が閾値未満（正規化後の類似度98%以上）の場合はevent logに新規行を作らず、既存行の `last_seen_at` と `dwell_ms` を更新する。

**FR-CAP-04（MUST, All）**: `AXSecureTextField`（パスワード入力欄）の値は、いかなるアプリでも**読み取り自体を行わない**。

**FR-CAP-05（MUST, All）**: **既定の除外リスト**（該当アプリ/ウィンドウがフォーカスの間、キャプチャイベントを一切生成しない）:
- パスワードマネージャ: 1Password, Bitwarden, KeePassXC, Dashlane, Enpass, Keychain Access（キーチェーンアクセス）
- プライベートブラウジングウィンドウ: Safari / Chrome / Edge / Brave / Arc / Firefox（ウィンドウタイトルおよびAX属性でプライベートモードを判定。判定不能なブラウザは通常キャプチャとし、既知ブラウザのみ対応）
- macOSの認証ダイアログ（SecurityAgent）
- SHOGUN自身の設定画面のうちBYOKキー入力画面

**FR-CAP-06（MUST, All）**: ユーザーは除外リストにアプリ単位・ドメイン単位（ブラウザのURL/タイトルパターン）で追加・削除できる。既定項目のうちパスワードマネージャとSecureTextFieldは**削除不可**（UI上グレーアウト）。

**FR-CAP-07（MUST, All）**: 一時停止:
- グローバルホットキー（既定 `⌃⌥⇧P`、変更可）で即時トグル
- 停止中はインジケータがグレー、Notchに「Paused」表示
- 再開方法: 同ホットキー / Notchから / 時限再開（15分・1時間・今日中）を選択可
- 停止中はevent log書き込み・context cache更新への入力が発生しない（連携同期は継続する）

**FR-CAP-08（MUST, All）**: Accessibility権限のグレースフルデグラデーション:
- 権限なしでもクラッシュ・エラーダイアログ連発をしない。キャプチャ系のみ停止し、連携統合・検索（連携データ）・Morning Briefは動作する
- インジケータはグレー、Hoverで「Enable capture」導線（システム設定の該当ペインを直接開く）
- 権限付与を1時間ごとに再検出し、付与され次第自動でキャプチャ開始する

**FR-CAP-09（MUST, All）**: キャプチャデーモンは絶対に落とさない: AX API呼び出しの失敗・タイムアウト（個別呼び出し500ms上限）は該当イベントのスキップとして処理し、`unwrap()` を使わない。パニックが起きた場合もキャプチャスレッドのみ再起動し、アプリ全体を巻き込まない。

**受け入れ基準（6.2）**: 除外リスト既定値の全アプリで「イベントが1件も生成されない」ことの自動テスト（AXモック）。権限なし起動の統合テスト。24時間連続稼働でクラッシュ0・CPU 5%以下（1分平均）の計測ログ。

### 6.3 メモリ（3層メモリ + event log）

**目的**: 年単位で生きるメモリ基盤。書き込みは絶対に失わず、検索は500msで返す。

#### 6.3.1 3層構造

**FR-MEM-01（MUST, All）**: 3層メモリを実装する:

| 層 | 範囲 | 置き場所 | 内容 |
|---|---|---|---|
| **Hot** | 直近24h | RAM（shogun-core内） | 直近イベントの構造化バッファ＋要約。context cacheの主材料 |
| **Warm** | 直近30日 | SQLite | 全イベント行＋float32相当のembedding（sqlite-vec）＋FTS5索引。**ベクトル検索の対象はこの層のみ** |
| **Cold** | 全履歴 | SQLite（期間パーティション） | 圧縮済みイベント＋int8量子化embedding。月単位パーティション |

**FR-MEM-02（MUST, All）**: Hot層はRAM専用だが、**元データは常にWarm層（ディスク）に先に書かれている**こと。Hot層はキャッシュであり、プロセス再起動時はWarm層から直近24h分を再構築する（起動から10s以内にバックグラウンド完了）。Hot層のRAM使用は200MBを上限とし、超過時は古い順に要約へ畳み込む。

**FR-MEM-03（MUST, All）**: 通常のベクトル検索はWarm層のみを対象にする（sqlite-vecは総当たりのため。CLAUDE.mdデータモデル原則）。Cold層の検索は (a) FTS5 trigram全文検索は全期間対象、(b) ベクトル検索はユーザーが明示的に期間指定した場合のみ該当パーティションに対して実行、の2形態に限る。

**FR-MEM-04（MUST, All）**: 層間移動はDream Cycle（§6.7）が行う: Warm層で30日を超えた行は、Dream Cycleで要約・統合された上でCold層パーティションへ移動し、embeddingはint8量子化される。移動はトランザクション内で行い、失敗時は元の状態を維持する。

#### 6.3.2 event log

**FR-MEM-10（MUST, All）**: event logは**追記型（immutable）**とする。UPDATEは `last_seen_at` / `dwell_ms` の重複抑制更新（FR-CAP-03）のみ許可。DELETEはユーザー明示の削除操作（FR-SET-07）とCold層への移動時のみ。

**FR-MEM-11（MUST, All）**: event logの主要カラム（スキーマ概要。詳細はマイグレーションが正）:

| カラム | 型 | 説明 |
|---|---|---|
| `id` | INTEGER PK | — |
| `ts` | INTEGER (unix ms) | 発生時刻 |
| `source` | TEXT | `capture` / `gmail` / `gcal` / `slack` / `notion` / `github` / `linear` / `agent` / `user` |
| `kind` | TEXT | `text` / `focus` / `message` / `event` / `action_executed` など列挙 |
| `app_bundle_id` | TEXT NULL | キャプチャ時のアプリ |
| `window_title` | TEXT NULL | — |
| `content` | TEXT | 抽出テキスト本体 |
| `content_hash` | TEXT | 重複抑制用 |
| `last_seen_at` | INTEGER | 重複抑制更新 |
| `dwell_ms` | INTEGER | 滞在時間（spatial-ready） |
| `display_id` | INTEGER NULL | spatial-ready |
| `window_bounds` | TEXT NULL (JSON) | spatial-ready |
| `window_pose` | TEXT NULL (JSON) | spatial-ready（v1では常にNULL可） |
| `gaze_target` | TEXT NULL | spatial-ready（v1では常にNULL可） |

**FR-MEM-12（MUST, All）**: spatial-readyカラム（`window_pose` / `gaze_target` / `dwell_ms` / `display_id` / `window_bounds`）は**v1初回マイグレーションから存在**させる。v1で値を書かないカラムはNULLでよいが、後付けマイグレーションにしない。

#### 6.3.3 検索

**FR-MEM-20（MUST, Std）**: ローカル検索はハイブリッド: FTS5（trigramトークナイザ）＋Warm層ベクトル検索（sqlite-vec）の結果をスコア統合（Reciprocal Rank Fusion）して返す。p95 500ms以内（NFR-SLO-04）。

**FR-MEM-21（MUST, Std）**: embeddingは**同梱のローカルONNX多言語モデル**で生成する（付録A ADR-001）。要件として固定する性質: ローカル実行・多言語（最低限、英語と日本語を含むCJK）・オフライン動作・推論の追加限界費用ゼロ・出力がsqlite-vecに格納可能。モデルの最終選定（候補: multilingual-e5-small等）はPhase 1着手時のベンチで確定する（§9）。クラウドembedding APIは使わない。

**FR-MEM-22（MUST, Std）**: embedding生成は書き込みパスをブロックしない（event log書き込み後の非同期ジョブ。遅延許容は5分。未embedding行はFTS5のみで検索対象になる）。

**FR-MEM-23（MUST, All）**: 検索結果の各行は出典（source・アプリ・時刻）を表示し、キャプチャ由来と連携由来を視覚的に区別する。

#### 6.3.4 マイグレーションと互換性

**FR-MEM-30（MUST）**: スキーマ変更はrefineryによるバージョン管理マイグレーションのみで行う。手書きの `ALTER TABLE` をアプリコードに埋め込まない。各マイグレーションにはロールバック手順（ドキュメント）を必須添付する。

**FR-MEM-31（MUST）**: **後方互換を破るマイグレーションを書かない**: カラム削除・意味変更・型の非互換変更を禁止。必要なら新カラム追加＋書き込み二重化＋読み取り移行の3段階で行う。アプリのダウングレード時に旧バージョンがDBを開けなくなる変更は、メジャーバージョンアップ＋明示の告知なしに行わない。

**受け入れ基準（6.3）**: マイグレーションを空DBと「v1初版スキーマのダミーデータ入りDB」の両方に適用するCIテスト。10万イベント投入時の検索p95計測。プロセスkill→再起動でHot層が再構築されevent logに欠損がないこと。

### 6.4 State tables

**目的**: ワールドモデルの本体。event logから推定された「現在の状態」を、根拠と確度付きで保持する。

#### 6.4.1 共通規則

**FR-ST-01（MUST, Std）**: state tablesは `people` / `projects` / `commitments` / `open_loops` の4テーブルとし、event logと**物理的に分離**する（CLAUDE.mdデータモデル原則）。

**FR-ST-02（MUST, Std）**: 全stateレコードに以下を必須とする:
- **provenance**: 根拠となるevent log行への参照（1レコードにつき1件以上。中間テーブル `state_provenance(state_table, state_id, event_id, weight)` で多対多）
- **confidence**: REAL、0.0〜1.0
- `created_at` / `updated_at` / `last_evidence_at`（最後に根拠イベントを得た時刻）

provenanceが空のstateレコードをINSERTするコードパスを作らない（DB制約またはリポジトリ層のアサーションで担保）。

**FR-ST-03（MUST, Std）**: stateの更新経路は次の3つのみ:

| 経路 | タイミング | キー |
|---|---|---|
| インライン抽出（軽量分類） | 連携同期・キャプチャ後の非同期ジョブ | Select KKキー（Batch API）またはローカルルール |
| Dream Cycle（統合・再計算） | 夜間バッチ | Select KKキー（Batch API） |
| ユーザー明示編集 | Full UIから | なし（confidence=1.0、provenanceは編集イベント） |

エージェント推論（BYOK）の出力からstateを直接書き換えない（エージェントの提案→ユーザー承認（L2）→ユーザー編集扱い、の経路は可）。

#### 6.4.2 people

**目的**: ユーザーが仕事で関わる人物の現在状態。

| フィールド | 説明 |
|---|---|
| `id`, `display_name`, `aliases` (JSON) | 名寄せ用。メールアドレス・Slackハンドル等をaliasesに保持 |
| `emails` / `handles` (JSON) | 連携アカウント識別子 |
| `relationship_summary` | この人物との関係・進行中の文脈の要約（Dream Cycleが更新） |
| `last_interaction_at` / `interaction_channel` | 最終接触 |
| `pending_from_me` / `pending_from_them` (JSON) | 相互の返信待ち事項（open_loops参照のキャッシュ） |
| provenance / confidence / 各種タイムスタンプ | 共通規則 |

**FR-ST-10（MUST, Std）**: 名寄せ: 同一人物が複数チャネル（Gmailアドレス・Slackハンドル・GitHubアカウント）に現れる場合の統合はDream Cycleで行い、統合の確度もconfidenceに反映する。誤統合のユーザー修正（分割）をFull UIから可能にする。

#### 6.4.3 projects

**目的**: 進行中のプロジェクト・仕事のまとまりの現在状態。

| フィールド | 説明 |
|---|---|
| `id`, `name`, `status` | status: `active` / `waiting` / `paused` / `done`（列挙） |
| `summary` | 現況要約（Dream Cycleが更新） |
| `participants` (JSON) | people参照 |
| `sources` (JSON) | 関連するGitHubリポジトリ・Linearプロジェクト・Notionページ・Slackチャンネル等の識別子 |
| `last_activity_at` | — |
| provenance / confidence | 共通規則 |

#### 6.4.4 commitments

**目的**: 明示的な約束。「誰が・誰に・何を・いつまでに」。

| フィールド | 説明 |
|---|---|
| `id`, `direction` | `mine`（自分が約束した）/ `theirs`（相手が約束した） |
| `counterparty_id` | people参照 |
| `description` | 約束内容 |
| `due_at` (NULL可) | 期日 |
| `status` | `open` / `done` / `overdue` / `cancelled`。`overdue` はDream Cycleが `due_at` 超過で遷移させる |
| `project_id` (NULL可) | projects参照 |
| provenance / confidence | 共通規則 |

**FR-ST-11（MUST, Std）**: commitmentsの生成は「明示的な約束表現」（"I'll send it by Friday" 等）を根拠に持つ場合のみ。推測ベースの義務はcommitmentsでなくopen_loopsに入れる。

**FR-ST-12（MUST, Std）**: `overdue` への遷移はDream Cycleで判定し、Morning Briefとフォローアップ提案（FR-AG-13）の入力になる。

#### 6.4.5 open_loops

**目的**: 開いたままの事項。返信待ち・読みかけ・放置中のレビュー・宙に浮いた決定など。

| フィールド | 説明 |
|---|---|
| `id`, `kind` | `reply_needed` / `waiting_on_them` / `review_pending` / `decision_pending` / `follow_up` / `other`（列挙） |
| `description` | — |
| `counterparty_id` / `project_id` (NULL可) | 参照 |
| `opened_at` / `staleness_days` | 放置日数（Dream Cycleが更新） |
| `status` | `open` / `closed`。closeの根拠（返信を検出した等）もprovenanceに残す |
| provenance / confidence | 共通規則 |

**FR-ST-13（MUST, Std）**: open_loopsの自動クローズ: 対応する完了イベント（返信送信・PRマージ等）を検出したらDream Cycleまたはインライン抽出でclosedへ遷移させる。自動クローズのconfidenceが0.8未満の場合はclosedにせず、Morning Briefで「解決した可能性」として提示する。

#### 6.4.6 confidenceの規則

**FR-ST-20（MUST, Std）**: confidenceの解釈と生成物への渡し方を全コンポーネントで統一する:

| confidence | 解釈 | Context Fusion / 生成物での扱い |
|---|---|---|
| 0.8 〜 1.0 | 高確度 | 事実として使用可 |
| 0.5 〜 0.8未満 | 中確度 | **「〜の可能性」として弱く渡す**。プロンプト内で `possibly:` プレフィクス等の明示マーク必須。生成物内でも断定表現にしない |
| 0.5未満 | 低確度 | 生成物・アクション判断に**使用しない**。検索結果・Full UIの状態閲覧でのみ表示（低確度マーク付き） |

低confidenceの状態をContext Fusionが事実として生成物に混ぜてはならない（CLAUDE.md不変条件）。この規則の適用はshogun-fusionのプロンプト組み立て層で一元実装し、各エージェントに個別実装させない。

**FR-ST-21（MUST, Std）**: confidenceの初期値は抽出器が決め、Dream Cycleが再計算する。再計算の入力: 根拠イベント数・新しさ（`last_evidence_at` からの経過）・矛盾イベントの有無・ユーザー修正履歴。ユーザーが明示編集したレコードは1.0とし、以後の自動更新はユーザー値を上書きしない（新しい根拠で変化を検出した場合は「変更提案」として提示）。

**受け入れ基準（6.4）**: provenance空のINSERTが失敗するテスト。confidence帯域ごとのプロンプト出力（事実/possibly/除外）のスナップショットテスト。名寄せ分割操作の統合テスト。

### 6.5 Context Fusion

**目的**: `f(state, screen_ctx, intent) → action`。「今この瞬間に押せる正しいボタン」を常時計算しておく。

**FR-CF-01（MUST, Std）**: Context Fusionは純粋に**ローカルで**アクション候補を決定する（LLM呼び出しをアクション提示の同期経路に入れない）。入力: context cache（state関連レコード＋screen_ctx＋Hot層要約）、intentヒューリスティック（直近の行動系列: 例「メールスレッドを30秒以上閲覧」→ reply系intent）。出力: アクション候補リスト（各候補にL1/L2/L3タグ・対象エージェント・優先度スコア）。

**FR-CF-02（MUST, Std）**: コンテキストアクションボタンの提示は、Notch Expanded表示時点から**150ms以内**（NFR-SLO-02）。実体はcache更新時（フォーカス切替から300ms以内、NFR-SLO-05）に計算済みであること。

**FR-CF-03（MUST, Std）**: アクション候補の上限は4件。優先度スコアは（intent一致度、state緊急度（overdue等）、直近の同種アクション採択率）の加重で決め、決定ロジックはユニットテスト可能な純関数として実装する。

**FR-CF-04（MUST, Std）**: screen_ctxに関連stateが見つからない場合（未知の相手・新規文脈）でも、汎用アクション（Save note / Search memory / Extract tasks）を提示し、空のパネルを出さない。

**FR-CF-05（MUST, Std）**: FusionはStandardでも動作する（提示まで）。ただし**実行**がBYOK必須のアクション（ドラフト生成等）はStandardではロック表示（押すとProアップグレード導線）とする。ロック表示はアクション候補4件のうち最大1件までとし、Standardユーザーの体験を広告面にしない。

**エラー時挙動**: cache構築失敗時は前回の有効cacheを使い、インジケータをアンバーにする。300msを超過した場合も提示は行い、超過をメトリクスに記録する。

**受け入れ基準（6.5）**: 代表シナリオ10件（US-01〜US-11から抽出）でのアクション候補のスナップショットテスト。150ms/300msの計測コード同梱。LLM呼び出しがFR-CF-01の同期経路に存在しないことのアーキテクチャテスト（依存グラフ検査）。

### 6.6 エージェント実行

**目的**: 提示されたアクションを、L1/L2/L3の権限モデル下で安全に実行する。

#### 6.6.1 権限モデル

**FR-AG-01（MUST）**: 全アクションは実行前に必ずL1/L2/L3のいずれかに分類される。未分類のアクションは実行できない（enumで型的に強制）。定義:

| レベル | 承認 | 許可される影響範囲 |
|---|---|---|
| **L1 自動実行** | なし（通知のみ） | ローカル完結・可逆な操作のみ。外部送信を**絶対に**含めない（CLAUDE.md不変条件4） |
| **L2 ワンタップ承認** | Notchで1クリック | デバイス外への影響が**読み取り系または下書き系**に限られるもの（例: Gmailドラフト作成、外部サービスの読み取り取得） |
| **L3 明示確認** | 実行内容プレビュー（送信先・全文）＋明示的確認操作 | 外部に**不可逆**な影響を持つ操作すべて: 送信・投稿・カレンダーイベント作成・削除・イシュー作成・ページ編集など |

**FR-AG-02（MUST）**: レベル判定は「操作の種類」に対して静的に定義する（`shogun-agents` 内の単一の許可テーブル）。実行時の動的引き下げ（L3→L2）を行うコードパスを作らない。引き上げ（L2→L3）は設定で可能（FR-SET-05）。

**FR-AG-03（MUST）**: L3確認UIの必須表示項目: 操作種別、送信先（アドレス/チャンネル/リポジトリ等の完全表記）、送信内容の**全文**（スクロール可の全文であり要約ではない）、経路（直接MCP / 第三者経由Composio）、使用キー種別（BYOK）。確認操作は専用ボタンのクリックとし、Enterキー単独では確定しない。

**FR-AG-04（MUST）**: AI API（MCP/CLI/REST）経由の操作にも**同一のレベル判定・同一の承認UI**を適用する（CLAUDE.md不変条件6）。外部AIがL3操作を要求した場合、Notch/Full UIに承認要求が表示され、ユーザーが確認するまでAPI呼び出しはpendingを返す（タイムアウト10分でrejected）。

**FR-AG-05（MUST）**: L1実行は事後通知をNotchインジケータ＋実行履歴に残す。L1の全操作は「SHOGUNローカルDB・ファイルへの書き込み等、SHOGUN内で取り消し可能」なものに限り、実行履歴からワンクリックで取り消せる（undo可能期間: 7日）。

#### 6.6.2 実行エンジン

**FR-AG-06（MUST, Pro）**: エージェント推論・チャット・ドラフト生成は**ユーザー自身の資格情報**で実行する。Select KKキーをこの用途に使わない（CLAUDE.md不変条件5）。資格情報の経路は2つあり、**サブスク委譲を第一選択、BYOKをフォールバック**とする（Issue #110）:

- **サブスク委譲（既定）**: ユーザーが既にインストール・ログイン済みのベンダー公式CLI（`claude` / `codex` / `gemini`）をローカルサブプロセスとして起動し、そのプランの枠で推論する。SHOGUNは資格情報を保持も読み取りもしない。**他アプリの資格情報ファイル／Keychainエントリの読み取り、およびベンダーのコンシューマ向けOAuthの自前実装は恒久的に禁止**（規約違反・BAN対象であり、notarize配布する商用プロダクトが依存してよい基盤ではない）
- **BYOK（フォールバック）**: ユーザーのAPIキー。Keychain保存（NFR-SEC-01）

**FR-AG-06a（MUST）**: サブスク委譲は Agent lane 専用とする。Batch lane（インデックス・分類・Dream Cycle・Morning Brief）には使わない。委譲先が消費するのは**月次の有限クレジット**であり、バッチ量はそれを最速で溶かす作業であるため——焼き切ると、Select KK が既にバッチ単価でカバーしている作業と引き換えに、ユーザーが実際に体感する Agent lane が月替わりまで停止する（不変条件5の型レベル分離で担保: `SubscriptionAgentClient` は `AgentClient` のみ実装し `BatchClient` を実装しない）。

> **前提となるポリシー（変更前に必ず確認すること）**: Anthropic は 2026年2〜4月に第三者からのサブスクOAuth利用を禁止・サーバ側でブロックしたが、**2026-06-15 から方針を変更**し、Claudeプランに月次の Agent SDK クレジット（Pro $20 / Max 5x $100 / Max 20x $200 相当、API標準レート課金）を付与した上で、**Agent SDK・`claude -p`・およびユーザーのサブスクで認証する第三者アプリ**を対象に含めた。この枠は対話利用（Claude Code / Claude）の上限とは別勘定で、アカウント単位（プール・共有不可）。恒久的に禁止されたままなのは**サブスクOAuthトークンの自前発行・窃取による直接API呼び出し**であり、本節の委譲方式はそれに該当しない。OpenAI 側には第三者利用に関する同等の公式声明が確認できないため、`codex exec` はあくまで「ユーザー自身がインストール・ログインしたCLIをユーザー自身が動かす」位置づけとする。

**FR-AG-06b（MUST）**: サブスク委譲の利用開始には**明示的opt-in同意**を必須とする。コンシューマ向けプランのデータ取り扱いはメーターAPI経路と同一ではなく、その差はユーザーが引き受ける判断であるため、既定ONにしない。同意なしで委譲先が選択されている場合、Agent lane は生成を行わない。

**FR-AG-06c（MUST）**: 委譲経由の送信もegressであり、トレーサビリティに `route = local_agent` として記録する（AR-11。記録は従来どおりダイジェスト＋バイト長のみ）。トレーサビリティ画面は「ローカルCLI経由」と委譲先ベンダーを表示する。第三者バッジ（FR-C2-04）は付けない——ユーザー自身が契約したベンダーであり、Composioのような中継ではないため。

**FR-AG-06d（MUST）**: プロンプトは委譲先プロセスの**標準入力**で渡す。argvは同一マシンの全プロセスから `ps` で読めるため、キャプチャ由来テキストを置いてはならない。また子プロセスの環境からSHOGUN側のAPIキー（`ANTHROPIC_API_KEY` 等）を除去する——残すと委譲先がサブスクではなくメーター課金で認証し、ユーザーが避けたはずの従量課金が黙って発生する。

**FR-AG-06e（MUST）**: プラン枠の使い切りは、資格情報の問題（401）とも SHOGUN の障害とも区別して提示する。文言はどのプランの枠かを明示し、SHOGUN側の失敗と読めてはならない。**これは短い時間窓ではなく月次クレジットである**ため、「しばらく待って再試行」と案内してはならない（月替わりまで回復しない）。BYOKへのフォールバック導線を出す。

資格情報が未設定のProユーザーには、該当機能の起動時に設定導線を出す（検出済みのサブスク委譲先を第一選択として提示し、APIキー入力は副次選択に置く）。

**FR-AG-07（MUST, Pro）**: アクション実行→初トークン表示は**1s以内、ストリーミング必須**（NFR-SLO-03）。非ストリーミングの一括応答UIを作らない。

**FR-AG-08（MUST, Pro）**: LLMプロバイダは trait（`LlmProvider`: 補完・ストリーミング・ツール呼び出しの抽象）で分離する。v1の実装はAnthropicのみ（付録A ADR-002）。trait境界にAnthropic固有型を漏らさない。

**FR-AG-09（MUST, Pro）**: エージェントのプロンプトに渡すstateはFR-ST-20のconfidence規則を通す。エージェントがツール（第1層MCP等）を呼ぶ場合、各ツール呼び出しにもFR-AG-01のレベル判定を適用する（エージェント内部からのL3操作も個別に明示確認を要求）。

#### 6.6.3 プリセットエージェント（v1で7種）

各プリセットの「実行レベル」は操作ごとに記す。すべてPro機能（実行はBYOK）。提示（Fusion）はStandardでも行われる（FR-CF-05）。

| ID | 名称 | 内容 | レベル |
|---|---|---|---|
| **FR-AG-10** | Reply Drafter | フォーカス中のメール/Slackスレッドへの返信ドラフト生成。state（相手・約束・プロジェクト）を反映 | ドラフト生成=L2 / 送信=L3（送信はGmailのみ、Composio経由・オプトイン） |
| **FR-AG-11** | Meeting Prep | 次の会議の参加者・議題に関する状態集約ブリーフィングをローカル生成・表示 | 集約・表示=L1（外部送信なし。LLM整形を使う場合はL2起動） |
| **FR-AG-12** | Task Extractor | 画面中/スレッド中のテキストからタスク・約束を抽出し、open_loops / commitmentsへの追加を提案 | 抽出提案=L2（承認でstate書き込み） |
| **FR-AG-13** | Follow-up Sentinel | overdueなcommitments・staleなopen_loopsを検出し、フォローアップドラフトを提案 | 検出・提示=L1相当（Dream Cycle成果の表示） / ドラフト生成=L2 / 送信・投稿=L3 |
| **FR-AG-14** | Calendar Scheduler | スレッド文脈から会議候補を抽出し、Google Calendarイベント作成を提案 | 空き時間読み取り=L2 / イベント作成=**L3**（カレンダー作成は不可逆扱い、CLAUDE.md不変条件4） |
| **FR-AG-15** | Issue Triage | 画面文脈からGitHub/Linearのイシュー・コメントのドラフトを作成 | ドラフト表示=L2 / イシュー作成・コメント投稿=L3 |
| **FR-AG-16** | Note Capture | 現在の文脈をNotionページ/データベース行として保存する下書きを作成 | 下書き=L2 / Notionへの書き込み=L3 |

**FR-AG-17（MUST, Pro）**: 自由入力チャット: Notch Expanded / Full UIからBYOKでのチャットを提供する。チャットはcontext cacheを自動添付し（添付内容はユーザーが展開して確認可能）、チャット内からのツール実行にもFR-AG-01を適用する。

**FR-AG-18（MUST, Pro）**: 実行履歴: 全エージェント実行（L1含む）を実行履歴に記録し、Full UIで閲覧可能にする。記録項目: 時刻、エージェントID、レベル、承認方法、結果（成功/失敗/キャンセル）、外部送信の有無（有ならトレーサビリティログへのリンク）。

**エラー時挙動**: BYOKの401/403はインジケータ赤＋キー再設定導線。レート制限（429）は指数バックオフ（最大3回）後にL2/L3提案を「再試行」ボタン付きで保持。実行途中の失敗は部分実行の内容を明示する（「ドラフトは作成済み・送信は未実行」等）。

**受け入れ基準（6.6）**: 許可テーブルの網羅テスト（全操作種別にレベルが定義されている）。「L1に分類された操作が外部送信APIに到達しない」ことのアーキテクチャテスト。L3確認なしで外部送信が発生しないことのE2Eテスト（モックMCP）。初トークン1sの計測コード同梱。

### 6.7 Dream Cycle

**目的**: 夜間バッチで「今日の生データ」を「明日使えるワールドモデル」へ変換する。

**FR-DC-01（MUST, Std）**: Dream Cycleは1日1回、既定02:00〜06:00（ローカル時刻、変更可）のウィンドウで実行する。実行条件: (a) ユーザーがアイドル（入力なし15分以上）または画面ロック中、(b) 電源接続中またはバッテリー30%以上。条件を満たさないままウィンドウを過ぎた場合、次回アイドル時に縮退版（state更新のみ）を実行し、フルサイクルは翌夜に持ち越す。Macがスリープしていた場合はウェイク後の最初のアイドルで実行する。

**FR-DC-02（MUST, Std）**: LLM処理は**Select KKキーのAnthropic Batch API**で実行する（CLAUDE.md不変条件5）。BYOKをDream Cycleに使わない。送信するのは処理用チャンク（当日イベントの抽出・整形済み断片）のみで、送信内容はトレーサビリティログに記録する（AR-11）。

**FR-DC-03（MUST, Std）**: Dream Cycleのジョブ内容（実行順）:

| # | ジョブ | 内容 |
|---|---|---|
| 1 | 統合（consolidation） | 当日のevent logからstate tables更新候補を抽出（新規people/projects、commitments、open_loops開閉） |
| 2 | 圧縮（compression） | 当日イベントの日次要約を生成しHot層/検索用に格納。冗長イベントの要約統合 |
| 3 | state更新 | 候補の適用。既存レコードとの名寄せ・矛盾検出 |
| 4 | confidence再計算 | FR-ST-21の入力で全stateレコードを再計算。overdue/staleness更新 |
| 5 | Cold層への降格 | Warm層の30日超過分を要約・int8量子化してCold層パーティションへ移動（FR-MEM-04） |
| 6 | Morning Brief生成 | §6.8 |

**FR-DC-04（MUST, Std）**: 冪等性とクラッシュ耐性: 各ジョブはジョブ実行テーブル（job_runs: ジョブ種別・入力範囲・状態）で管理し、途中失敗時は完了済みジョブをスキップして再開できる。同一入力範囲への二重適用でstateが壊れないこと（upsert設計）。

**FR-DC-05（MUST, Std）**: Batch API失敗時（24h以内に結果が得られない場合を含む）: インジケータをアンバーにし、翌夜のサイクルで未処理分をまとめて処理する。3日連続失敗で赤＋Full UIに詳細表示。**ローカル機能（キャプチャ・検索・Fusion提示）はBatch API失敗の影響を受けない**こと。

**FR-DC-06（MUST, Std）**: Dream Cycleの実行結果サマリ（処理イベント数・state変更数・所要時間・送信チャンク数）をFull UIで閲覧可能にする。

**受け入れ基準（6.7）**: 途中killからの再開テスト。二重実行の冪等性テスト。Batch API全滅時にローカル機能が無影響であることの統合テスト。

### 6.8 Morning Brief

**目的**: 「Macを開いた瞬間、今日が整理されている」体験。

**FR-MB-01（MUST, Std）**: Morning BriefはDream Cycleの最終ジョブとして生成する（Select KKキー・Batch API）。生成内容:

| セクション | 内容 | 出典 |
|---|---|---|
| Today | 今日のカレンダー予定（時刻順）＋各予定へのMeeting Prep導線 | Google Calendar（接続時） |
| Commitments due | 今日期限・期限超過のcommitments | state tables |
| Open loops | staleness上位のopen_loops（最大5件） | state tables |
| What happened | 昨日の要約（3〜5行） | Dream Cycle圧縮ジョブ |
| Suggested actions | 推奨アクション最大3件（各L2/L3ボタン付き） | Fusion/Follow-up Sentinel |

**FR-MB-02（MUST, Std）**: 生成言語はユーザー設定の**出力言語**（FR-SET-04）に従う。UI枠組みは英語（v1）だが、Brief本文は設定言語で生成する。

**FR-MB-03（MUST, Std）**: 提示タイミング: 生成完了後、当日最初の「画面ロック解除またはアプリフォーカス」から60s以内にNotchインジケータをゴールドにし、Hoverプレビューに「Morning Brief ready」を表示する。自動でExpandedをポップアップさせない（割り込まない原則）。既読管理を行い、既読後はインジケータを白へ戻す。

**FR-MB-04（MUST, Std）**: Dream Cycle失敗等でBrief本文が生成できなかった朝は、ローカルデータのみの縮退Brief（カレンダー＋overdue commitments一覧。LLM文章なし）を表示する。空画面・エラー画面を朝一番に見せない。

**FR-MB-05（MUST, Std）**: Brief内の各項目は出典（state/イベント）へのリンクを持ち、低confidence項目（0.5〜0.8）は「possibly」表示で区別する（FR-ST-20）。

**FR-MB-06（SHOULD, Std）**: カレンダー統合の詳細: 当日の予定変更（Brief生成後の追加・削除）はBrief表示時にローカルで差分反映する（再生成はしない。差分は「Updated」マーク付き）。

**受け入れ基準（6.8）**: 生成〜提示のE2Eテスト（モックBatch API）。縮退Briefの表示テスト。出力言語設定（例: 日本語）でBrief本文が該当言語になるテスト。

### 6.9 第1層連携（公式リモートMCP直結）

**目的**: 外部サービスの文脈をワールドモデルに統合し、承認済みの書き込みを実行する。

#### 6.9.1 共通要件

**FR-INT-01（MUST, Std）**: 第1層連携は各サービスの**公式リモートMCPサーバー**へ、shogun-mcp（Rust MCP SDKクライアント）から直接接続する。中間サーバー（Select KK運営サーバー含む）を経由しない。

**FR-INT-02（MUST, Std）**: OAuthは**ユーザー→サービス直接**（MCP仕様のOAuth 2.1 Authorization Code + PKCEフロー、システムブラウザで認可）。アクセストークン・リフレッシュトークンは**Keychainのみ**に保存する（CLAUDE.md不変条件7）。トークンをDB・設定ファイル・ログ・webviewに書き出さない。

**FR-INT-03（MUST, Std）**: 段階導入（Wave）: Wave 1 = Gmail + Google Calendar + **Google Drive** → Wave 2 = Slack → Wave 3 = Notion + GitHub + Linear。**【2026-08-20 実装反映】** Google Drive は 2026-07-23 のプロダクト判断で Wave 1 に追加（公式リモートMCP `drivemcp.googleapis.com` が存在するため第1層の条件を満たす）。**Google Docs / Sheets は独立サービスにせず**、Drive の `read_file_content` 経由で読む。各Waveは前Waveの安定（接続成功率95%以上・クラッシュ増加なしを2週間）を確認してから既定で有効化する。未リリースWaveのサービスは設定画面に「Coming soon」として表示してよい。

**FR-INT-04（MUST, Std）**: 同期方式: 各サービスの読み取りは15分間隔のポーリング（サービス側のレート制限に応じて延長）＋フォーカス文脈に応じたオンデマンド取得（例: Gmailスレッド表示中の該当スレッド取得）。取得データはevent log（`source` = サービス名）へ正規化して格納し、キャプチャ由来イベントと同一の検索・Fusion経路に乗せる。

**FR-INT-05（MUST, Std）**: 読み取りスコープは各サービスで**必要最小限を要求**し、下表の「読み取り範囲」を超えるスコープを要求しない。書き込み操作は下表のレベル列に従う（外部送信系は必ずL3。CLAUDE.md不変条件4）。

**FR-INT-06（MUST, Std）**: 接続失敗・トークン失効時: インジケータをアンバーにし、該当サービスのみ再認証導線を出す。他サービス・ローカル機能に影響を波及させない。再認証まで該当サービスのデータは「最終同期時点」として扱い、鮮度をUIに表示する。

**FR-INT-07（MUST, Std）**: 各サービスはユーザーが個別に接続・切断できる。切断時、Keychainのトークンを削除し、以後の同期を停止する。切断時に既存の取り込み済みイベントを削除するかはユーザーに選択させる（既定: 保持）。

#### 6.9.2 サービス別許可範囲表

以下の表が各サービスの実装範囲の正である。表にない操作を実装しない。

**Gmail（Wave 1）**

| 操作 | 範囲 | レベル |
|---|---|---|
| 読み取り | 受信・送信済みメールのメタデータ＋本文、スレッド構造、ラベル | 同期=バックグラウンド / オンデマンド取得=L2（初回接続時に一括許可可） |
| ドラフト作成・更新 | ユーザーのDraftsフォルダへの下書き | L2 |
| ラベル付与・既読化 | ユーザー操作起点のみ | L2 |
| **送信** | **第1層では実装しない**。送信はComposio（第2層、§6.10）のみ | —（Composio側でL3） |
| 削除・アーカイブ | v1では実装しない | — |

**Google Calendar（Wave 1）**

| 操作 | 範囲 | レベル |
|---|---|---|
| 読み取り | ユーザーのカレンダー一覧・予定（参加者・時刻・場所・説明） | 同期=バックグラウンド |
| 空き時間照会 | free/busy | L2 |
| イベント作成 | 参加者への招待送信を伴うため不可逆 | **L3** |
| イベント更新・削除 | 同上 | **L3** |

**Slack（Wave 2）**

| 操作 | 範囲 | レベル |
|---|---|---|
| 読み取り | 参加チャンネル・DMのメッセージ、メンション、スレッド | 同期=バックグラウンド |
| ドラフト生成（ローカル） | SHOGUN内での下書き作成（Slackへは未送信） | L2 |
| メッセージ投稿・返信 | チャンネル・DMへの投稿 | **L3** |
| リアクション | 外部可視のため | **L3** |

**FR-INT-30（MUST, Std）**: Slackフォールバック: ワークスペース管理者承認が得られず公式リモートMCPに接続できない場合、Slack向けアクションは「ドラフト生成→クリップボードコピー」へフォールバックする（CLAUDE.md連携実装ルール）。フォールバック時のUIは投稿ボタンを出さず「Copy to clipboard」のみを出し、コピー実行はL2とする（デバイス外送信を伴わないため）。Slack公式リモートMCPの提供状況はWave 2着手時に再確認する（§9）。

**Notion（Wave 3）**

| 操作 | 範囲 | レベル |
|---|---|---|
| 読み取り | ユーザーが接続時に許可したページ・データベース | 同期=バックグラウンド |
| ページ/DB行の作成 | 許可済みスペース内 | **L3** |
| ページ更新（追記含む） | 同上 | **L3** |
| 削除 | v1では実装しない | — |

**GitHub（Wave 3）**

| 操作 | 範囲 | レベル |
|---|---|---|
| 読み取り | ユーザーがアクセス可能なIssue / PR / 通知 / コミットメタデータ（コード本文の一括取り込みはしない。参照時のみ） | 同期=バックグラウンド |
| Issue / PRコメントのドラフト | SHOGUN内下書き | L2 |
| Issue作成・コメント投稿 | — | **L3** |
| PRマージ・クローズ・ブランチ操作 | v1では実装しない | — |

**Linear（Wave 3）**

| 操作 | 範囲 | レベル |
|---|---|---|
| 読み取り | ユーザーのチームのIssue・プロジェクト・自分へのアサイン | 同期=バックグラウンド |
| Issueドラフト（ローカル） | SHOGUN内下書き | L2 |
| Issue作成・更新・コメント | — | **L3** |
| ステータス変更 | 外部チームに可視のため | **L3** |

**受け入れ基準（6.9）**: 各サービスにつき「表にない操作のMCPツール呼び出しが実行エンジンの許可テーブルで拒否される」テスト。トークンがKeychain以外（DB・ファイル・ログ）に現れないことの検査（結合テストでのファイルシステム/ログのgrep検査）。Slackフォールバックの動作テスト。

### 6.10 第2層連携（Composio）

**目的**: 公式MCPが提供しない操作（v1ではGmail送信のみ）を、明示的なオプトインの下で提供する。

**FR-C2-01（MUST, Pro）**: 第2層はComposio経由とし、v1の対象操作は**Gmail送信のみ**。他サービス・他操作を第2層に追加する場合は本書の改版を要する。

**FR-C2-02（MUST, Pro）**: 完全オプトイン: 既定は無効。有効化時に専用の同意画面を表示し、次を明示する: (a) 送信がComposio（第三者）のインフラを経由すること、(b) 経由するデータの種類（送信先・件名・本文）、(c) いつでも無効化できること。同意なしに接続フローを開始しない。

**FR-C2-03（MUST, Pro）**: **ドラフト止まりモード**: Composio有効時にも「送信は行わずGmailドラフト作成まで」に制限する設定を必ず用意する（既定: ON）。ONの間、送信系アクションはUIに現れない（グレーアウトではなく非表示。誤操作余地を残さない）。

**FR-C2-04（MUST, Pro）**: Composio経由の送信は**L3**（プレビュー: 宛先・件名・本文全文）。トレーサビリティ画面の該当エントリに「**第三者経由（via Composio）**」バッジを必ず表示する（CLAUDE.md連携実装ルール）。

**FR-C2-05（MUST, Pro）**: Composioの認証情報もKeychainのみに保存。Composio障害時は送信アクションを自動的にドラフト作成（第1層）へフォールバックし、その旨を実行結果に明示する（サイレントに経路を変えて送信はしない — 送信自体は必ず失敗として扱い、ドラフト保存＋通知）。

**受け入れ基準（6.10）**: オプトインなしで送信経路のコードに到達しないテスト。ドラフト止まりモードON時に送信UIが非表示になるテスト。トレーサビリティの「第三者経由」表示テスト。

### 6.11 Memory API（MCPサーバー / CLI / REST）

**目的**: SHOGUNのメモリとワールドモデルを、ユーザーが使う他のAIから利用可能にする。**人間UIとAI APIは完全対称**（CLAUDE.md不変条件6）。

**FR-API-01（MUST, Pro）**: 提供面は3つ:
- **MCPサーバー**（stdio）: 外部AIエージェント向け。**【2026-08-20 実装反映】** **Streamable HTTP/localhost は保留**（悪用面積を絞るため当面 stdio のみ。`todo.md`）。復活させる場合はNFR-SEC-03のバインド・認証要件をそのまま適用する
- **CLI**（`shogun` コマンド、shogun-cli）: スクリプト・ターミナル向け
- **REST**（`http://127.0.0.1:7464`、localhostバインドのみ。既定ポート7464は設定で変更可能、使用中の場合は起動時に自動フォールバックし実ポートをCLI `shogun api status` で取得可能にする）: 任意クライアント向け

3面は同一のRust内部API（shogun-core経由）の薄いラッパであり、機能差を作らない。

**FR-API-02（MUST, Pro）**: v1の公開ツール/エンドポイント。**【2026-08-20 実装反映】** 下表は**当初の最小セット**であり、実装済みの公開ツールはこれを含む28ツール（wire nameの正は `crates/shogun-mcp/src/memory_api.rs` の `Tool::wire_name`）。表に無い実装済みツール: `memory.get_context_pack` / `memory.get_wrap` / `actions.status` / `device.onboarding.get` / `lessons.list` / `lessons.set_active` / `visual_recall.{status,set_enabled,set_retention,search_frames,get_frame,rescan_frame,delete_frame}` / `profile.whoami` / `profile.set`。**追加ツールも同じ規律に従う**（読み取りはFR-API-06のconfidence規則、書き込みはL1/L2/L3、送信系はL3）:

| ツール | 内容 | レベル |
|---|---|---|
| `memory.search` | ハイブリッド検索（FR-MEM-20と同一実装） | 読み取り |
| `memory.get_context` | 現在のcontext cache（screen_ctx除外オプション付き） | 読み取り |
| `state.people.list/get` | people照会（confidence付き） | 読み取り |
| `state.projects.list/get` | projects照会 | 読み取り |
| `state.commitments.list/get` | commitments照会 | 読み取り |
| `state.open_loops.list/get` | open_loops照会 | 読み取り |
| `memory.append_note` | ユーザーメモとしてevent logへ追記 | **L1**（ローカル・可逆） |
| `state.propose_update` | state変更の提案（commitment追加等） | **L2**（Notchで承認） |
| `actions.execute` | プリセットエージェントの起動 | 対象操作のレベルに従う（L3操作はFR-AG-04の承認フロー） |

**FR-API-03（MUST, Pro）**: 認証: クライアントごとのAPIトークンを発行（Full UIで発行・失効管理）。トークンはKeychainに保存し、REST/HTTPはlocalhostのみにバインドする。トークンなしの呼び出しは読み取り含め全拒否。

**FR-API-03b（MUST, Pro）**: **【2026-08-20 実装反映】** Memory API は設定の Enable トグル（`memory_api.json`）で**既定オフ・fail-closed**とする（未有効化時は `shogun-mcp` / `shogun-api` が明示エラーで終了する）。**現状このトグルはソフトなProゲート**（トライアル中も有効化できる）であり、Stripe連携完了後にプラン判定と結合してハードゲート化する（`todo.md`）。

**FR-API-04（MUST, Pro）**: AI経由の操作にも同一のL1/L2/L3を適用する（FR-AG-04）。APIからの承認待ち操作は呼び出し元にpending状態と承認要求IDを返し、承認/拒否/タイムアウト（10分）を照会可能にする。

**FR-API-05（MUST, Pro）**: 対称性の維持プロセス: 新機能のFRは「UIからの操作」と「APIからの操作」の両方を受け入れ基準に含めなければならない。片方のみの新機能を追加する場合は判断記録を必須とする。

**FR-API-06（MUST, Pro）**: API経由の読み取りにもconfidence規則を適用する（レスポンスにconfidence値と `possibly` フラグを含め、0.5未満は既定で除外。明示パラメータ指定時のみ低確度を含めて返す）。

**受け入れ基準（6.11）**: UI検索とAPI検索の結果一致テスト。API経由L3操作がUI承認なしに完了しないE2Eテスト。CLI/REST/MCPの3面で同一操作のスモークテスト。

### 6.12 課金・ライセンス

**目的**: Freeプランなし・トライアル起点のシンプルな課金。オフラインでもユーザーの仕事を止めない。

#### 6.12.1 プランと価格

**FR-BIL-01（MUST）**: プラン構成と価格（USD）:

| プラン | 年払い | 月払い | 内容 |
|---|---|---|---|
| **Trial** | 無料・7日間 | — | Pro相当の全機能 |
| **Standard** | **$49/月**（年一括請求） | **$62/月** | キャプチャ、3層メモリ、state tables、ローカル検索、Notch UI、第1層連携（読み取りコンテキスト統合）、Dream Cycle、Morning Brief |
| **Pro** | **$99/月**（年一括請求） | **$124/月** | Standardの全て＋エージェント実行エンジン（L1/L2/L3すべて: コンテキストアクション実行、チャット、ドラフト生成）＋Memory API（MCP/CLI/REST）＋Composio第2層 |

**FR-BIL-02（MUST）**: キー構成とプランの整合（CLAUDE.md不変条件5と完全整合）:
- **Standard**はSelect KKキー（Batch API: インデックス・分類・Dream Cycle・Morning Brief）**のみ**で全機能が動作し、**BYOK不要**
- **Pro**のエージェント推論・チャット・ドラフト生成は**ユーザー自身の資格情報必須**（サブスク委譲 または BYOK。FR-AG-06）。Select KKキーをエージェント推論に使わない
- この対応により「プラン境界＝キー境界」となり、逆転（Standardでユーザー資格情報を要求、ProでSelect KKキーによる推論）を実装しない

**FR-BIL-03（MUST）**: Standardにおける第1層連携は「読み取りコンテキスト統合」（同期・検索・Fusion・Morning Briefへの反映）に限る。書き込み系操作（ドラフト作成・L3送信等）はエージェント実行エンジンの機能でありPro限定。Standard UIでは書き込み系アクションをロック表示（FR-CF-05の上限規則に従う）。

#### 6.12.2 トライアル

**FR-BIL-04（MUST）**: 初回起動から7日間、Pro相当の全機能を提供する。トライアル中の資格情報: エージェント機能を試すにはユーザー自身の資格情報が必要（Select KKキーで肩代わりしない。不変条件5維持）。ただし**サブスク委譲が第一選択**であるため、既にClaude/ChatGPT/Geminiに課金しているユーザーはAPIキーを取得せずトライアルでエージェント機能を体験できる（Issue #110。APIキー取得がトライアル→Pro転換の最大の摩擦であったため）。未設定のトライアルユーザーにはStandard相当機能＋検出済み委譲先の接続導線を提示する。

**FR-BIL-05（MUST）**: トライアル満了時: エージェント/API/Composioと新規キャプチャ・同期を停止し、プラン選択画面を表示する。**ローカルデータは削除しない**。閲覧・検索・エクスポート（FR-SET-08）は満了後も30日間可能とし、その後もデータは購読再開まで保持する。決済完了から60s以内に機能を解放する。

**FR-BIL-06（MUST）**: トライアル開始時のクレジットカード要否は**未決事項**（§9）。実装は「カードなし開始」「カード必須開始」の両方をフラグで切り替え可能な構造にする。

#### 6.12.3 Stripeとライセンス検証

**FR-BIL-07（MUST）**: 決済はStripe（Checkout + Billing）。アプリ内にカード情報を扱うUIを作らず、システムブラウザでCheckoutを開く。webhook→ライセンスAPI→アプリの反映で60s以内。

**FR-BIL-08（MUST）**: ライセンス検証: アプリはライセンスAPIに対し24時間ごと＋起動時に検証を行い、署名付きライセンストークン（プラン・有効期限を含む）を保存する。**【2026-08-20 実装反映】** 保存先は **`billing.json`（app-data、平文）**（CLAUDE.md 2026-08-13 例外）。トークンはEd25519署名済み・device idに束縛・約24時間で失効するため秘密ではなく、CLI/MCP/RESTの3面がKeychainに触れずプラン状態を読めることが設計要件（`shogun_mcp::plan_source`）。**ライセンスキー本体（APIのbearer）はKeychainのみ。**検証リクエストにキャプチャ内容・メモリ内容を一切含めない（送るのはライセンスID・アプリバージョン・匿名デバイスIDのみ）。

**FR-BIL-09（MUST）**: オフライン猶予: ライセンスAPIに到達できない場合、最後に取得した有効トークンから**14日間**は全機能を維持する（インジケータは7日目からアンバー）。14日超過でトライアル満了と同じ制限状態に移行し、オンライン復帰時の検証成功で即時復元する。決済失敗（Stripe側）の場合はStripeのリトライ設定に従い、最終失敗から7日間の猶予後に制限状態へ移行する。

**受け入れ基準（6.12）**: プラン×機能のアクセス制御マトリクスの網羅テスト。オフライン14日境界のテスト（時計モック）。トライアル満了→課金→復元のE2Eテスト（Stripeテストモード）。

### 6.13 オンボーディング

**目的**: 初回起動から「最初の価値」までを最短にする。権限拒否でも壊れない。

**FR-OB-01（MUST, All）**: 初回起動フロー（この順序。各ステップはスキップ可能で、スキップ時の状態を明示する）:

| # | ステップ | 内容 |
|---|---|---|
| 1 | Welcome | 一言定義の提示。トライアル開始（アカウント作成。クレカ要否は§9の決定に従う） |
| 2 | Accessibility権限 | 目的説明（テキストのみ・既定でスクショなし・ローカル保存・Visual recall は任意でOffが既定）→システム設定へ誘導→付与検出で次へ |
| 3 | キャプチャ開始 | 付与確認後、キャプチャを開始しNotchデモ（Hover→Expandedのガイド）を実施 |
| 4 | 連携接続 | Wave 1（Gmail / Google Calendar）の接続を提案。スキップ可 |
| 5 | （Pro/トライアル）AIの接続 | 検出済みのサブスク委譲先（`claude` / `codex` / `gemini`）を第一選択として提示し、opt-in同意の上で選ばせる。APIキー入力は副次選択に置く（Issue #110）。スキップ可（エージェント機能ロックのまま進む） |
| 6 | 会議ノート（オプトイン） | 会議を検知して聞き、**録音は保存せず文字起こしテキストのみ残す**こと、bot は会議に参加しないこと、いつでも1タップで止められることを説明し、**ON/OFFを選ばせる**。既定はOFF（FR-MT-01）。スキップ時はOFFのまま進む |
| 7 | 完了 | Morning Briefの予告（「明日の朝、ここに現れる」）と一時停止ホットキーの案内 |

**FR-OB-02（MUST, All）**: ステップ2の説明画面には次を明記する: キャプチャはAccessibility API経由のテキストのみ / 既定ではスクリーンショット・画像を保存しない（Visual recall を自分で On にした場合のみ、テキストが取れなかったウィンドウのフレームを最大72時間ローカル保持し自動削除することを併記する） / 生データはデバイス外に出ない / 除外リストと一時停止がいつでも使える。

**FR-OB-06（MUST, All）**: ステップ6（会議ノート）でONを選んだ場合のみ、マイク／音声取得のTCC権限を要求する。**OFFのまま権限を要求しない。** 説明にはFR-MT-03の開示テキストを用い、「ShogunAIはZoom / Google Meetにbot参加しません」「承認した会議だけを聞き、音声は保存しません」を明記する。

**FR-OB-03（MUST, All）**: 権限付与の検出は2s間隔でポーリングし、付与後に自動でステップ3へ進む（再起動を要求する場合は再起動後にフローの続きから再開する）。

**FR-OB-04（MUST, All）**: 権限拒否時の体験: ステップ2で「Skip for now」を選べる。スキップ時はFR-CAP-08の縮退状態（連携統合とMorning Briefは可能）でオンボーディングを続行し、完了画面とNotch Hoverに「Enable capture」導線を常設する。拒否を理由にアプリを終了・機能全ロックしない。

**FR-OB-05（SHOULD, All）**: オンボーディング完了直後の初回価値: 接続済み連携がある場合、初回同期完了時点（目標: 接続から5分以内）で「最初のstate候補」（検出したpeople/open_loops）をNotchから確認できるようにする。

**受け入れ基準（6.13）**: 全ステップスキップで完走できるテスト。権限拒否→後日付与→自動キャプチャ開始の統合テスト。

### 6.14 トレーサビリティ

**目的**: 「デバイスの外に何が出たか」を100%記録し、ユーザーが検証できるようにする（CLAUDE.md不変条件3）。

**FR-TR-01（MUST, All）**: **全外部送信箇所**にトレーサビリティログを実装する。対象はAR-11の全経路（Anthropic Batch/Messages、第1層MCP書き込み、Composio。第1層の読み取り要求も要求パラメータを記録対象とする）。記録項目:

| 項目 | 内容 |
|---|---|
| `ts` | 送信時刻 |
| `destination` | 送信先（API種別＋エンドポイント種別） |
| `route` | `direct` / `via_composio`（第三者経由） |
| `purpose` | `dream_cycle` / `morning_brief` / `indexing` / `agent` / `chat` / `integration_write` / `integration_read` 等の列挙 |
| `key_kind` | `select_kk` / `byok` / `oauth_user` / `composio` |
| `approval` | `l1` / `l2` / `l3` / `background`（同期等の非アクション送信） |
| `payload_digest` | 送信内容の要約（先頭抜粋＋サイズ＋チャンク数）。**全文はローカルに7日間保持**し、以後digestのみ保持 |
| `status` | 成功 / 失敗 |

**FR-TR-02（MUST, All）**: トレーサビリティ閲覧画面（Full UI）: 時系列一覧、送信先/purpose/route/キー種別でのフィルタ、各エントリの詳細（7日以内なら送信全文）。Composio経由エントリには「第三者経由」バッジを表示する（FR-C2-04）。

**FR-TR-03（MUST, All）**: 外部送信を行う関数はトレーサビリティ記録を強制する共通クライアント層（shogun-core内の単一HTTP出口）を必ず経由する。個別コンポーネントが素のHTTPクライアントで外部送信するコードをレビューで拒否できるよう、CIで依存検査（許可リスト外のHTTPクライアント利用検出）を行う。

**FR-TR-04（MUST, All）**: トレーサビリティログ自体はデバイス外に送信しない。ログのローカル保持期間: digest行は無期限（Cold層同様のパーティション）、全文は7日。

**受け入れ基準（6.14）**: 全送信経路（モック）でログ行が生成されるテスト。共通出口を経由しないHTTP呼び出しがCIで検出されるテスト。

### 6.15 設定画面

**FR-SET-01（MUST, All）**: 設定画面（Full UI内）は以下のセクション・項目を持つ:

| セクション | 項目 |
|---|---|
| **General** | ログイン時起動（既定ON）/ 表示ディスプレイ（自動追従・内蔵固定）/ グローバルホットキー2種（展開・一時停止）/ アップデートチャネルと「Check for updates」 |
| **Capture** | キャプチャON/OFF / 除外アプリリスト（既定項目表示・追加/削除、FR-CAP-06の削除不可項目はグレーアウト）/ 除外ドメイン・タイトルパターン / 一時停止の時限オプション既定値 |
| **Meeting notes** | 機能ON/OFF（**既定OFF**、FR-MT-01）/ 会議アプリ別の除外リスト（FR-MT-02(b)）/ 常に除外する予定（繰り返し1on1等）/ 開示テキストの常設表示（FR-MT-03）/ 直近セッション一覧とRecapへの導線 |
| **Memory** | Warm層期間（既定30日、14〜90日で変更可。短縮時はDream Cycleで降格）/ ストレージ使用量表示（層別）/ データ削除（期間指定・アプリ指定・全削除。全削除は確認2段階） |
| **Language** | 生成物の出力言語（既定: システム言語。Morning Brief・ドラフト・チャット応答に適用）。UI言語はv1では英語固定（表示のみ） |
| **Integrations** | 第1層6サービスの接続/切断・同期状態・最終同期時刻 / Composio（オプトイン、同意画面へ）/ ドラフト止まりモード（既定ON） |
| **Agents**（Pro） | プリセットの個別有効/無効 / L2→L3への引き上げ設定（FR-SET-05）/ L1自動実行の通知粒度 |
| **API**（Pro） | BYOKキーの設定・検証・削除（Keychain保存。表示は末尾4桁のみ）/ Memory APIのON/OFF / クライアントトークンの発行・失効一覧 |
| **Dream Cycle** | 実行ウィンドウ（既定02:00〜06:00）/ 直近の実行結果サマリ / 手動実行（縮退版） |
| **Account** | プラン表示・変更（Stripeポータルへ）/ ライセンス状態 / トライアル残日数 |
| **Privacy** | トレーサビリティ画面への導線 / テレメトリのON/OFF（既定OFF、§7.7）/ データエクスポート |
| **Advanced** | 診断ログ（キャプチャ内容を含まない）/ SLOメトリクス表示 / DBのintegrity check実行 |

**FR-SET-04（MUST, Std）**: 出力言語設定はMorning Brief（FR-MB-02）・ドラフト生成・チャット応答の生成プロンプトに適用される。UI文言はi18n-ready構造（文言カタログをコードから分離。v1カタログは英語のみ）とする。

**FR-SET-05（MUST, Pro）**: ユーザーは任意の操作種別をL2→L3へ引き上げられる（より慎重側のみ）。L3→L2、L2→L1への引き下げUIは提供しない。

**FR-SET-07（MUST, All）**: データ削除: 期間・アプリ単位の削除はevent log・派生embedding・FTS索引・関連state provenanceを整合的に削除する（provenanceが空になったstateレコードは「根拠喪失」フラグを立て、次回Dream Cycleで再評価または削除）。

**FR-SET-08（MUST, All）**: データエクスポート: event log・state tables・トレーサビリティdigestをJSON Lines形式でローカルにエクスポートできる。エクスポートはL1（ローカル完結）。

**受け入れ基準（6.15）**: 全設定項目の永続化・再起動後復元テスト。削除操作のリレーショナル整合性テスト。BYOKキーが設定UI経由でもログ・DBに残らないことの検査。

### 6.16 会議ノート（Meeting Notes）

**目的**: 会議を検知して開始し、終わるまで聞き続け、終わったら**そのまま仕事になる**ノートを返す。記録を残すことが目的ではなく、会議で生まれた**決定と約束をstate tablesに接地させる**ことが目的。設計の正本は `docs/meeting-notes-ui-design.md`、区間の器の設計は `docs/meeting-context-and-dashboard-design.md`。

> **本節を貫く原則**: 「録音を残す」のではなく「**聞いて、テキストだけ残す**」。音声はディスクに書かない（不変条件2、NFR-PRV-01）。

**プラン**: FR-MT-01〜21は**全プラン（トライアル含む）**。会議ノートは「使ってみて価値が分かる」タイプの機能で、トライアル中に体験できなければ課金判断の材料にならない。Memory API経由の参照（FR-MT-22）のみPro（API面自体がPro機能のため）。

#### 6.16.1 有効化・同意・オフの設計

**FR-MT-01（MUST, All）**: 会議ノート機能の既定は**OFF**とする。オンボーディング（FR-OB群）で一度だけ説明し、ユーザーが明示的にONにした場合のみ有効化する。「黙って有効になっていた」を構造的に作らない。**既定ONへの変更・アップデートによる自動有効化を行わない。**

**FR-MT-02（MUST, All）**: オフは常に以下**3つの粒度**で到達可能であること。

| 段 | 到達経路 | 効果 |
|---|---|---|
| (a) 全体オフ | 設定 → Meeting notes → Off | 検知もASRも走らない。**マイク・音声デバイスに一切アクセスしない** |
| (b) このアプリでは録らない | Offeredの副操作 / 設定の除外リスト | 指定した会議アプリでは以後Offeredを出さない |
| (c) この会議は録らない | Offeredの「Not now」/ Recordingの「Stop」 | 今回のセッションのみ。設定を変更しない |

全体オフ中はNotchにピル（FR-MT-09）を一切表示しない（「オフなのに何か出ている」を作らない）。除外は会議アプリ単位に加え、**特定の予定を常に除外**（繰り返し1on1等）を設定できること。

**FR-MT-03（MUST, All）**: 開示テキストをRecap画面と設定画面に常設する: 「この会議の音声はこのMacの中だけで処理され、録音は保存されません。残るのは文字起こしテキストです」。**事実と実装が一致していること**が受け入れ条件（実装がこの文言を裏切っていないことをテストで担保）。法域別の分岐は持たない（既定OFF＋明示オプトイン＋常時可視のピルで一律運用）。参加者への告知を**プロダクトが代行しない**（会議へ自動でメッセージ・チャット投稿をしない。CLAUDE.md不変条件4／Issue #7 Non Goal）。

#### 6.16.2 検知とセッション（区間）

**FR-MT-04（MUST, All）**: 会議検知は**単一信号に頼らず**、以下の信号の組み合わせで行い、結果をconfidence付きで扱う（断定しない）。

| 信号 | 取得元 | 単独の強さ |
|---|---|---|
| ① 予定がある | `calendar_occurrences`（FR-MT-06） | 強（ただし予定=出席ではない） |
| ② 会議アプリが前面 / マイクが使用中 | NSWorkspace + bundle id表 + マイク使用中フラグ | 中 |
| ③ 会議UIの痕跡 | AXに参加者リスト・Leave/Mute等のコントロールが見える | 中 |

②③が立った時点でセッション候補とし、①と一致すればconfidenceを上げて予定に紐付ける。**①のみ（会議アプリが前面に来ない）では区間を開かない**（「予定はあったが出席の証拠なし」）。
検知対象アプリはbundle id表で管理し、v1はGoogle Meet（ブラウザのURL）とZoomを必須対象とする。
**マイク使用中の検知は「使用中か」の真偽のみを読み、音声ストリームには触れない。** この境界は実装コード上にコメントで明示すること。

**FR-MT-05（MUST, All）**: 点イベントの`event_log`とは別に、**区間**を表す`sessions`を第一級の概念として持つ。

```
sessions   区間: [started_at, ended_at)  kind = meeting | call | focus
  ├─ calendar_occurrence_id?   予定に紐づくか（飛び込み会議はNULL）
  ├─ participants              people.id[]（identity.rsで名寄せ済み）
  ├─ thread_key?               関連する会話スレッド
  ├─ summary / decisions       Recapが書く
  └─ confidence + provenance   検知も推定である以上、断定しない（state tablesと同じ規律）

event_log.session_id  ← additive。区間中のキャプチャ・チャット・メールが自動的にぶら下がる
```

スキーマ変更は**additive**であること（後方互換を破らない。CLAUDE.md）。`kind=focus`により会議以外の区間にも同じ器を使えること。

**FR-MT-06（MUST, All）**: 未来の予定は`event_log`（append-only の過去ログ）ではなく、**state**として`calendar_occurrences`テーブルに保持する。RFC3339の開始/終了時刻を**epoch msとして解釈**し（現行の正規化はts=0に潰すため修正必須）、`title` / `location`（会議URLを含む。第三者に出さない）/ `attendees`（JSON）を構造化フィールドとして持つ。`attendees`はidentity.rs（名寄せ）の供給元となる。

#### 6.16.3 ライフサイクルとUI

**FR-MT-07（MUST, All）**: 会議ノートは以下の状態機械に従う。図中の遷移以外を実装しない。

```
  Idle
   │  会議検知（FR-MT-04）
   ▼
  Offered ──"Not now"───────────────────────────────────┐
   │  10秒のグレース（何もしなければ開始）                 │
   ▼                                                    │
  Recording ──"Stop"──→ Wrapping ──→ Recap ──確定──→ state tables
   │                                                    │
   └─ 会議アプリ終了 / 予定終了+10分 / 無音15分 → Wrapping │
                                                        ▼
                                                      Idle
```

**FR-MT-08（MUST, All）**: Offered状態では**10秒のカウントダウン**を提示し、無操作なら録音を開始する。「Not now」は**この会議のみ**に作用し設定を変更しない（FR-MT-02(c)）。長押し/副操作で「このアプリでは今後録らない」（同(b)）に到達できること。**自動で始まるが、始まる前に必ず一度姿を見せる**ことがこの要件の趣旨であり、Offeredを省略して直接Recordingへ遷移する実装を認めない。

**FR-MT-09（MUST, All）**: Recording中はNotch折りたたみ時にも**ピルを常時可視**とし、`● Notes · 12:04  Weekly sync  ■ Stop` の形で **(a) ライブドット (b) 経過時間 (c) セッション名 (d) 1タップのStop** を示す。
- **経過時間を必ず表示する**（「まだ動いているのか」が状態のトグルだけでは分からない）
- Stopは**確認ダイアログなしで即時停止**（止めることに摩擦を作らない）
- 録画を想起させる赤の録画ランプを使わず、ライブドットで表す（録画ではないため）
- **FR-NU-08（フルスクリーン時Hidden）の唯一の例外**がこのピルであり、フルスクリーン中も表示を維持する（常時可視の要請が優先）

**FR-MT-10（MUST, All）**: 会議中の展開パネルは**ユーザー自身がノートを打つ場所**とし、**ライブ文字起こしを表示しない**。代わりに `Listening · N participants` の静かな1行のみを出す。理由: (1)流れる字を目で追うと会議に集中できない (2)ASRの誤りがその場で見えると訂正したくなる＝仕事を増やす (3)誤りは文脈込みのRecapで直せる。
ユーザーのノートは`session_notes`に保存する。**ノートを打つことを前提にしない**（打たなければ文字起こしだけでRecapを組み立てる）。

**FR-MT-11（MUST, All）**: 以下のいずれかで**自動的にWrappingへ遷移**する。「止め忘れて延々と録り続けていた」を作らない。
- 会議アプリの終了、または対象タブ/ウィンドウの消失
- 紐づく予定の終了時刻 +10分
- 無音が15分継続

#### 6.16.4 音声パイプライン（RAMの内側と外側）

**FR-MT-12（MUST, All）**: 音声の経路は以下に限る。**リングバッファより先に音声を出さない。**

```
 マイク ────┐
            ├─→ [RAMリングバッファ 30s] ─→ オンデバイスASR ─→ transcript_segments（テキスト）
 システム音声┘        ↑ ここから先に音声は行かない          └→ sessionに紐づく
 (Core Audio tap)     └─ 破棄（ディスクにもクラウドにも書かない）
```

- リングバッファは**30秒固定上限**。ASRが追いつかない場合は**古い音声から捨てる**（貯めれば実質的な録音になるため、上限は設計上の防壁として固定値で持ちユーザー設定に露出しない）
- 音声の取得は**Recording状態の間のみ**。Idle/Offered/Wrapping中は音声デバイスにアクセスしない
- **ファイル入力を要求するASR実装を選ばない**（「一時ファイルなら良い」を許さない＝ストリーミングASRを選ぶ技術的根拠）
- Issue #7で言及された「押す前のN分を遡って含めるリングバッファ」は、上記30秒上限を超える保持となるため**採用しない**（FR-MT-08のOfferedグレースで代替する）

**FR-MT-13（MUST, All）**: ~~ASRはオンデバイス・ストリーミングとし、音声をクラウドに送信しない。~~ **【2026-08-05 オーナー例外】** 既定 ASR は **Deepgram Nova-3 Multilingual**（クラウド live STT）。常に `mip_opt_out=true`。SHOGUN は波形をディスクに書かない。会社キーはバイナリ埋め込み禁止（バックエンド保持 or 短命 JWT）。UI 開示必須。Whisper はオフライン/dev フォールバックのみ。macOS 14.4未満でCore Audio tapが使えない環境での縮退挙動（マイクのみに縮退するか機能を出さないか）を明示的に決めて実装すること（§9）。

**FR-MT-14（MUST, All）**: 文字起こしは`transcript_segments`（`session_id` / `ts` / `speaker` / `text` / `origin` / `confidence`）としてWarm層に保存し、既存の検索・抽出・Fusionがそのまま効くこと。`origin`は `asr`（音声由来）/ `caption`（画面のライブキャプション由来）を区別して持つ。**キャプション由来か音声由来かで説明責任が異なる**ため、provenanceに必ず残す。

**FR-MT-15（MUST, All）**: 話者分離はv1では「**自分 / それ以外**」の2値までとし（マイク入力かシステム出力かで判別）、不明な場合は`speaker = NULL`とする。**推測で埋めない。** 参加者名への割り当ては`calendar_occurrences.attendees`との突合として次段に持ち越す（§9）。

#### 6.16.5 Recap（会議後）

**FR-MT-16（MUST, All）**: セッション終了後、Recapとして **(1)要約3〜5行 (2)決定事項 (3)「誰が何を負ったか」の候補**（commitments / open_loops）を提示する。**候補は候補として提示する** — 低確度は`possibly:`帯のまま出し（FR-ST群のconfidenceゲートに従う）、断定しない。

**FR-MT-17（MUST, All）**: Recapの候補は`[Track]`の**1タップで確定**し、その瞬間のみ **confidence 1.0 + provenance=ユーザー明示編集** としてstate tablesに入る（§6.4「ユーザー明示編集」経路）。これが唯一の正直な昇格経路であり、**ユーザー確定なしにHigh帯へ昇格させる実装を認めない**。

**FR-MT-18（MUST, All）**: Recapから`Why?`で**根拠セグメントに降りられる**こと（provenanceの可視化。FR-TR群と同型）。

**FR-MT-19（MUST, All）**: Recapの初回表示は**会議終了から60秒以内**。間に合わない場合は「文字起こし・ノート・予定のみで構成した**縮退Recap**」を先に提示し、**空の画面を出さない**。

**FR-MT-20（MUST, All）**: Recapの要約生成は**Select KKキー（Batch API）**で行う（不変条件5の鍵レーン規律。エージェント推論・チャット・ドラフトのBYOKレーンと逆転させない）。**音声はクラウドに出さず**、送信するのは文字起こしから作った処理用チャンクのみで、全送信にトレーサビリティログを伴う（不変条件3、FR-TR-01）。議事録の外部共有・送信は**v1スコープ外**（L3扱い、§3.2）。

#### 6.16.6 API対称性・リソース

**FR-MT-21（MUST, All）**: 会議中のリソース上限は**アイドル時SLO（CPU 5%、NFR-SLO-06）とは別枠**とし、ASR稼働中のCPUは**15%を上限**として別途計測する（混ぜるとアイドル5%が意味を失う）。リングバッファ＋ASRモデルのメモリは3層メモリのHot上限（200MB）とは別勘定で計上する。会議中のバッテリー消費を計測しContext Healthに出すこと。

**FR-MT-22（MUST, Pro）**: 不変条件6（人間UIとAI APIの完全対称）に従い、セッション一覧・Recap・文字起こしの参照はMemory API（MCP / CLI / REST）からも同一のL1/L2/L3規律で到達できること。**webview側に会議データの集計ロジックを置かない**（不変条件1）。

**受け入れ基準（6.16）**:
1. **音声がディスクに書かれないことの検査**: Recording中の全ファイル書き込みを監視するテストで、音声データを含むファイルが生成されないことを検証する（FR-MT-12、NFR-PRV-01）。
2. 状態機械（FR-MT-07）の全遷移がテストで検証されている。Offeredを経ずにRecordingへ入る経路が存在しないことを含む。
3. 三段オフ（FR-MT-02）それぞれの効果と、全体オフ時に音声デバイスへアクセスしないことが検証されている。
4. 自動Wrapping条件3種（FR-MT-11）がテストされている。
5. `[Track]`未押下の候補がstate tablesに入らないこと（FR-MT-17）が検証されている。
6. 会議中CPU（FR-MT-21）の計測コードが同梱され、アイドルSLOと別系統で記録される。
7. 既定OFF（FR-MT-01）であること、およびアップデートで既定が変わらないことのテスト。

---

## 7. 非機能要件（NFR群）

### 7.1 SLO（計測定義付き）

CLAUDE.mdのSLO表を計測定義付きで詳細化する。**各SLOの計測コードを製品コードに同梱する**（NFR-SLO-00）。レイテンシに影響する変更はp50/p95を計測してからマージし、結果をPR本文に貼る。

**NFR-SLO-00（MUST）**: 計測基盤: shogun-core内に軽量メトリクス収集（ヒストグラム）を実装し、`shogun metrics`（CLI）とFull UIのAdvanced設定から現在値を確認できる。計測値はローカル保存のみ（テレメトリ送信はオプトイン時もSLO統計値のみ。§7.7）。

| ID | 項目 | 計測点（開始→終了） | p50目標 | p95上限（受け入れ基準） |
|---|---|---|---|---|
| NFR-SLO-01 | Notch展開 | ホットキー/クリックのイベント受信 → Expandedの最終フレーム描画完了（CVDisplayLink基準） | 50ms | **100ms** |
| NFR-SLO-02 | コンテキストアクション提示 | Expanded描画開始 → アクションボタン4件の描画完了 | 80ms | **150ms** |
| NFR-SLO-03 | アクション実行→初トークン | 承認操作のイベント受信 → ストリーミング初トークンのUI描画 | 600ms | **1s**（ストリーミング必須） |
| NFR-SLO-04 | ローカル検索 | クエリ確定（入力デバウンス後）→ 結果リスト描画完了 | 200ms | **500ms**（10万イベント時） |
| NFR-SLO-05 | context cache更新 | `focus.changed` 受信 → cache差し替え完了 | 150ms | **300ms** |
| NFR-SLO-06 | アイドル時CPU | SHOGUN全プロセス合算、1分平均（ユーザー入力なし・同期なしの状態） | 2% | **5%** |

**NFR-SLO-07（MUST）**: SLO超過の扱い: p95超過を検出したら該当操作をメトリクスに記録し、Advanced画面で確認可能にする。超過してもユーザー操作をブロック・キャンセルしない（提示は遅れても行う）。

### 7.2 セキュリティ

**NFR-SEC-01（MUST）**: secrets（OAuthトークン・BYOKキー・Composio認証情報・**ライセンスキー**・APIクライアントトークン）は**macOS Keychain（security-framework）以外に保存しない**。**【2026-08-20 実装反映】** 例外は**署名済みライセンストークン**のみで、これは `billing.json` に置く（FR-BIL-08の理由参照。秘密ではない）。平文ファイル・DB・環境変数・ログ・クラッシュレポートへの書き出しを禁止する（CLAUDE.md不変条件7）。

**NFR-SEC-02（MUST）**: secretsをwebview（JSコンテキスト）に渡さない。BYOK入力はTauri command経由で即Keychainへ書き込み、UIへの読み戻しは末尾4桁のみ。

**NFR-SEC-03（MUST）**: ローカルREST/MCPサーバーは127.0.0.1バインドのみ。全呼び出しにトークン認証（FR-API-03）。CORSは無効（ブラウザからの呼び出しを想定しない）。

**NFR-SEC-04（MUST）**: 外部通信は全てTLS。証明書検証の無効化オプションを作らない。

**NFR-SEC-05（SHOULD）**: SQLiteのDBファイル・モデルファイルはmacOSのユーザーホーム配下（`~/Library/Application Support/com.selectkk.shogun/`）に0600相当の権限で置く。DB全体の暗号化（SQLCipher等）はv1では採用しない（FileVault前提。判断は付録Bに付記）。

### 7.3 プライバシー原則

**NFR-PRV-01（MUST）**: スクリーンショット・画像・**音声ファイルを保存**するコードを書かない（不変条件2）。**【2026-08-02 明示的例外｜Visual recall】** Visual recall が On のときにかぎり、AX からテキストが取得できなかったウィンドウについて圧縮 JPEG を暗号化済みメモリ DB（`screen_frames`）に最大 72 時間保持し、期限切れは自動削除する。既定は Off、クラウド送信なし、音声は対象外、永続タイムラインではない（CLAUDE.md 不変条件2の例外）。音声ファイルについては例外なし。会議音声の取得は会議セッション中に限り、波形はRAM内リングバッファ（上限30秒）でのみ処理し、ディスク・一時ファイル・クラウドのいずれにも書かない。永続化するのは文字起こしテキストとそのprovenance（`origin`: asr / caption）のみ。詳細はFR-MT群（`docs/meeting-notes-ui-design.md`）。

**NFR-PRV-02（MUST）**: 生データ（event logの行・キャプチャ全文）はデバイス外に出さない。クラウドに出るのは目的別の処理用チャンクのみで、全てトレーサビリティログを伴う（不変条件3、FR-TR-01）。

**NFR-PRV-03（MUST）**: テレメトリ・診断ログ・クラッシュレポートにキャプチャ内容（ユーザーのテキスト）を含めない。例外なし（デバッグビルドも同様）。

**NFR-PRV-04（MUST）**: サーバー側（ライセンスAPI・Stripe）にメモリ内容・state内容を保存しない。サーバーが持つのはアカウント・課金・ライセンス状態のみ。

### 7.4 クラッシュ耐性・データ完全性

**NFR-REL-01（MUST）**: SQLiteはWALモード。書き込みは全てトランザクション。電源断・強制終了でDBが破損しないこと（`PRAGMA synchronous=NORMAL` 以上）。起動時にintegrity check（quick_check）を行い、破損検出時は直近バックアップ（NFR-REL-03）からの復旧フローを提示する。

**NFR-REL-02（MUST）**: キャプチャ・DB書き込み経路で `unwrap()` 禁止（テストコード以外）。パニックはスレッド単位で隔離・再起動し、アプリ全体を落とさない。クラッシュ（プロセス異常終了）はセッションあたり0を目標とし、発生時は次回起動時に診断（キャプチャ内容を含まない）を記録する。

**NFR-REL-03（MUST）**: ローカルバックアップ: DBのスナップショット（SQLite backup API）を日次（Dream Cycle前）に取得し、直近3世代を保持する。バックアップもデバイス内のみ。

**NFR-REL-04（MUST）**: エラーはユーザーの作業を中断させない形で処理する（Notchインジケータ色、FR-NU-06/07）。モーダルダイアログを出してよいのはL3確認・削除確認・復旧フローのみ。

### 7.5 リソース上限

| ID | 項目 | 上限 |
|---|---|---|
| NFR-RES-01 | アイドル時CPU | 5%（1分平均。NFR-SLO-06と同一） |
| NFR-RES-02 | 常駐メモリ（RSS、アプリ合算） | 通常時900MB以下、うちHot層200MB以下（FR-MEM-02）。ONNXモデルは常駐に含む |
| NFR-RES-03 | ディスク | Warm+Cold+索引の合計使用量を設定画面に表示。20GB到達で通知（アンバー）し、Cold層の圧縮・削除導線を提示 |
| NFR-RES-04 | バッテリー | Dream CycleはFR-DC-01の電源条件に従う。embedding生成等のバックグラウンドジョブはLow Power Mode中は間引く（周期2倍） |
| NFR-RES-05 | GPU/ANE | ONNX推論はCoreML/CPU実行。推論1件あたり50ms以内（e5-small級・512トークン・M1基準）を選定ベンチの基準にする |

### 7.6 配布・更新

**NFR-DST-01（MUST）**: 配布はDeveloper ID署名＋notarization済みDMG。App Storeは使わない。arm64（Apple Silicon）単一バイナリ、macOS 14 Sonoma以上（付録A ADR-005）。

**NFR-DST-02（MUST）**: 更新はTauri updater（署名検証付き）。更新チェックは日次＋手動。更新適用はユーザー確認後（作業中の強制再起動をしない）。updaterへのリクエストにユーザーデータを含めない（バージョン・アーキテクチャのみ）。

**NFR-DST-03（MUST）**: マイグレーション（refinery）はアプリ更新後の初回起動時に自動適用し、適用前にバックアップ（NFR-REL-03）を取得する。

### 7.7 テレメトリ方針

**NFR-TEL-01（MUST）**: **【2026-08-20 実装反映】** 匿名プロダクト分析は**オプトアウト方式（既定ON）**の単一系統（PostHog。CLAUDE.md 2026-08-08 統合決定）。永続状態は `analytics.json` の `opt_out` のみで、オンボーディングのトグルと設定画面のPrivacy & Securityカードは同じ状態を読み書きする。旧 `privacy.json` のオプトイン方式は廃止。送信してよいのは: 匿名デバイスID、アプリバージョン、機能イベント名とカウント（例: `action_executed{level=l2}`）、SLO統計値（p50/p95）、クラッシュ有無。**キャプチャ内容・state内容・検索クエリ・生成物・送信先情報は送らない**（NFR-PRV-03）。

**NFR-TEL-02（MUST）**: テレメトリ送信もトレーサビリティログの対象とする（purpose=`telemetry`）。送信項目の一覧を設定画面から閲覧できる。

### 7.8 アクセシビリティ・UI品質

**NFR-UI-01（SHOULD）**: Full UIはmacOSのキーボード操作・VoiceOverの基本操作に対応する。Notchパネルはホットキーで全機能に到達可能とする（マウス必須にしない）。

**NFR-UI-02（MUST）**: UI文言はSHOGUNブランドルール準拠: 競合プロダクト名を出さない、技術スタック名をUI文言に出さない、絵文字は⚔のみ許可、"AI-powered" / "revolutionary" / "second brain" を使わない。

---

## 8. 技術スタック

CLAUDE.mdの確定表を転記し、選定理由を付す。**このスタックを勝手に変更しない。**

| 領域 | 技術 | 選定理由（1行） |
|---|---|---|
| アプリ枠 | Tauri v2 / Rust backend / React + TS frontend | データ重心をRustに置きつつ、UI開発速度と既存JSモノレポ資産を活かすため |
| Notchパネル | NSPanel（objc2）、`.nonactivatingPanel` + `.canJoinAllSpaces` + `.fullScreenAuxiliary` | フォーカスを奪わず全Space・フルスクリーン補助表示に出せる唯一の組み合わせのため |
| DB | SQLite（rusqlite）+ sqlite-vec、WAL、FTS5 trigram | 単一プロセス埋め込み・実績・多言語FTS・ベクトル検索を1ファイルで満たすため（詳細は付録B） |
| マイグレーション | refinery | Rustネイティブのバージョン管理マイグレーションで後方互換規律を機械化するため |
| macOSネイティブ | AXUIElement / NSWorkspace / NSEventグローバルモニタ / security-framework | テキストのみキャプチャ（不変条件2）とKeychain限定secrets（不変条件7）の実現手段 |
| embedding | ローカルONNX多言語モデル（同梱） | オフライン動作・追加限界費用ゼロ・プライバシー境界維持のため（ADR-001） |
| MCP | Rust MCP SDK（クライアント＋サーバー） | 第1層直結（クライアント）とMemory API（サーバー）を同一SDKで実装するため |
| LLM | Anthropic（Batch API=Select KKキー / Messages API=BYOK） | キー分離（不変条件5）。プロバイダはtrait抽象、v1実装はAnthropicのみ（ADR-002） |
| 第2層連携 | Composio（オプトイン） | 公式MCP未提供の操作（Gmail送信）を第三者経由と明示した上で提供するため |
| 課金 | Stripe | Checkout/Billing/ポータルで課金UIを自前実装しないため |
| 配布 | Developer ID + notarization（App Store不使用） | Accessibility常駐アプリの審査リスク回避と更新速度のため |
| 更新 | Tauri updater | 署名検証付き自動更新の標準手段のため |

---

## 9. リスクと未決事項

### 9.1 未決事項（実装前に決定が必要なもの・時期）

| ID | 未決事項 | 決定時期 | 現時点の扱い |
|---|---|---|---|
| OPEN-01 | トライアル開始時のクレジットカード要否 | 課金実装着手前 | 両対応の実装構造（FR-BIL-06）。マーケ・CVR観点で判断 |
| OPEN-02 | embeddingモデルの最終選定（候補: multilingual-e5-small等） | Phase 1着手時のベンチ | 要件はFR-MEM-21で固定（ローカル・多言語・オフライン・限界費用ゼロ・sqlite-vec格納・NFR-RES-05の推論速度） |
| OPEN-03 | Slack公式リモートMCPの提供状況・WS管理者承認フローの実態確認 | Wave 2着手前 | 不可の場合はFR-INT-30のフォールバックが恒常運用になる |
| OPEN-04 | チームプラン（複数シート・組織請求） | v1リリース後の需要検証 | v1は個人のみ（§3.2） |
| OPEN-05 | 価格ローカライズ（JPY等の現地通貨表示・課税） | 課金実装着手前 | v1はUSD建て。Stripeの通貨対応範囲で判断 |
| OPEN-06 | Intel Mac対応 | v1リリース後（要望数と保守コストで判断） | v1はApple Siliconのみ（ADR-005） |
| OPEN-07 | オンデバイスASRモデルの選定（オンデバイス・ストリーミング・多言語） | MT3着手前のベンチ | 要件はFR-MT-13で固定。モデルサイズと会議中CPU（FR-MT-21の15%枠）のトレードオフで判断。**ファイル入力を要求する実装は選定対象外**（FR-MT-12） |
| OPEN-08 | システム音声の取得方式とmacOS 14.0〜14.3での縮退挙動 | MT3着手前 | Core Audio tap（`CATapDescription`、macOS 14.4+）を前提とし、非対応環境で「マイクのみに縮退」か「機能を出さない」かを決める（FR-MT-13） |
| OPEN-09 | 話者分離をどこまでやるか | MT3完了後 | v1は「自分 / それ以外」の2値まで（FR-MT-15）。参加者名への割り当ては`calendar_occurrences.attendees`との突合として次段 |

### 9.2 主要リスク

| リスク | 影響 | 緩和策 |
|---|---|---|
| Phase 0 No-Go（ノッチUI不成立） | メインサーフェス転換 | CLAUDE.md既定: メニューバー＋パレット方式へ転換。本書のFR-NU群を改版（状態機械・SLOは概ね移植可能） |
| Accessibility APIで十分なテキストが取れないアプリ（Electron・独自描画） | キャプチャ空白地帯 | 取得可否のアプリ別実測をPhase 1早期に実施。取れないアプリは連携（第1層）データで補完。常時キャプチャをスクショOCRへ転換することは**しない**（不変条件2）。テキストが取れないウィンドウに限っては、Off が既定の Visual recall（2026-08-02 例外、72時間で自動削除）が任意の補完手段になる |
| 公式リモートMCPの仕様変更・提供中止 | 第1層連携の停止 | サービスごとに接続層を分離し、個別に無効化・フォールバック可能にする（FR-INT-06） |
| sqlite-vecの性能限界（Warm層行数増大） | 検索SLO超過 | Warm層期間短縮設定（FR-SET-01 Memory）、量子化前倒し。v2でANNインデックス再評価 |
| Batch APIのコスト増大（Select KKキー負担） | 原価圧迫 | チャンク量の上限設計・Dream Cycle処理量のメトリクス監視。プラン原価モデルの月次見直し |
| BYOKハードルによるPro転換率低下 | 収益 | オンボーディングでのキー取得ガイド充実。OPEN-01と合わせてファネル計測 |
| ノッチ常駐アプリというUIパラダイムの誤発火・鬱陶しさ | 解約 | Phase 0の誤発火計測、FR-MB-03の「割り込まない」原則、全自動表示の抑制 |

---

## 10. 付録A: 主要判断記録（ADR）

各ADRは「状況 / 決定 / 理由 / 帰結」の形式。変更する場合は新ADRで上書きし、旧ADRは残す。

### ADR-001: embeddingはローカルONNXモデルを同梱し、クラウドembedding APIを使わない

- **状況**: Warm層のベクトル検索にembeddingが必要。クラウドAPI（高精度・容易）とローカル推論（プライバシー・費用）の選択。
- **決定**: 多言語ローカルONNXモデル（候補: multilingual-e5-small等、最終選定はPhase 1ベンチ = OPEN-02）を同梱する。クラウドembedding APIは使わない。
- **理由**: (1) キャプチャ全文をembedding目的でクラウドに送ると「生データはデバイス外に出さない」（不変条件3）の実質的な骨抜きになる。(2) キャプチャは高頻度でありAPI課金は限界費用が利用時間に比例して増える。ローカルなら限界費用ゼロ。(3) オフラインでも検索が完全動作する。(4) e5-small級はarm64のCPU/CoreML推論で実用速度（NFR-RES-05）。
- **帰結**: 精度は最新クラウドモデルに劣る可能性がある。FTS5とのハイブリッド（FR-MEM-20）で補い、モデル差し替え可能な形（モデルファイル＋次元数をメタデータ管理）で実装する。

### ADR-002: BYOKはv1でAnthropicのみ。プロバイダ抽象化層（trait）は設けるが実装は1つ

> **【2026-08-20 実装反映】** **本ADRは更新された。** 現行は **(1) Agent laneの第一選択＝サブスク委譲**（ユーザーが契約済みのベンダー公式CLIをローカルサブプロセスで起動。Issue #110、`crates/shogun-core/src/llm/subscription.rs`）、**(2) BYOKはフォールバックで Anthropic + OpenAI互換の2実装**（`llm/openai_compat.rs`）。trait境界を先に切っておく判断（下記）が正しく働いた結果であり、方針の否定ではない。Batch lane（Select KKキー）がAnthropicである点は不変（不変条件5）。

- **状況（当時）**: エージェント推論のBYOKを複数プロバイダ対応にするかどうか。
- **決定（当時）**: v1はAnthropicのみ。`LlmProvider` trait（FR-AG-08）で抽象化のみ行い、実装は1つ。
- **理由**: (1) ツール呼び出し・ストリーミング・プロンプトキャッシュの挙動差を吸収するコストが v1の価値に寄与しない。(2) Select KKキー側（Batch API）がAnthropicであり、プロンプト資産・評価基盤を1系統に集中できる。(3) trait境界だけ正しく切っておけば追加は後から可能。
- **帰結**: 他プロバイダ希望ユーザーを一部逃す。要望はOPEN項目として計測する。trait境界にAnthropic固有型を漏らさない規律をレビューで維持する。

### ADR-003: プラン差別化は「機能差」で行い、キー境界と一致させる

- **状況**: Standard/Proの差を使用量制限（クォータ）にするか機能差にするかの選択。
- **決定**: 機能差で差別化する。Standard = 観測系（キャプチャ・メモリ・検索・Notch UI・第1層読み取り統合・Dream Cycle・Morning Brief）、Pro = 実行系（エージェント実行・チャット・ドラフト・Memory API・Composio）。StandardはSelect KKキーのみで動作しBYOK不要、Proの推論はユーザー資格情報を要する（**【2026-08-20 実装反映】** **サブスク委譲 または BYOK**。当初の「BYOK必須」から更新。Issue #110）。
- **理由**: (1) プラン境界とキー境界（不変条件5）が完全一致し、実装・原価・説明が単純になる。(2) クォータ制は「使うほど罰される」体験になりプロダクト原則（実行 > 提案）と矛盾する。(3) BYOK必須層をProに限定することで、Standardのオンボーディングからキー取得の摩擦を排除できる。
- **帰結**: Standardユーザーは実行系に触れない。FR-CF-05のロック表示（最大1件）でアップグレード動機を作るが、広告的体験にはしない。

### ADR-004: ノッチ非搭載環境には「擬似ノッチ」で同型UIを提供する

- **状況**: ノッチはMacBook（2021以降）内蔵ディスプレイにしかない。外部ディスプレイ・ノッチ非搭載Macをどうするか。
- **決定**: メニューバー中央に同型・同状態機械のフローティングパネル（擬似ノッチ）を出す（FR-NU-04）。
- **理由**: (1) 「画面上端中央の固定アンカー」という空間的習慣が製品体験の核であり、環境で挙動が変わると習慣が形成されない。(2) 実装はNSPanelの配置座標の違いのみで、状態機械・SLO・描画を完全共有できる。(3) クラムシェル運用（外部ディスプレイのみ）のユーザーを切り捨てない。
- **帰結**: メニューバー中央のアイコン過密環境では他常駐アイコンと視覚干渉しうる。位置の微調整オプションは要望を見てから検討する。

### ADR-005: 公式サポートはApple Silicon（arm64）のみ、macOS 14以上

- **状況**: Intel Mac対応の要否。
- **決定**: v1はApple Siliconのみ公式サポート。macOS 14 Sonoma以上。Intel対応は将来判断（OPEN-06）。
- **理由**: (1) ノッチ搭載機は全てApple Siliconであり、コア体験の対象機はASに集中している。(2) ローカルONNX推論（embedding）の性能・電力効率がAS前提で設計できる（NFR-RES-05の基準もM1）。(3) 単一アーキテクチャのビルド・署名・QAで配布が簡素になり、v1の検証マトリクスを半減できる。
- **帰結**: Intel機ユーザー（減少中）を逃す。擬似ノッチ（ADR-004）はAS製Mac mini / Studio等のノッチ非搭載機のために引き続き必要。

### ADR-006: 既存pnpmモノレポにCargo workspaceを統合する（リポジトリ分離しない）

- **状況**: Rustコアを別リポジトリにするか、既存JSモノレポ（apps/website等）に統合するか。
- **決定**: 既存モノレポのルートにCargo workspaceを追加し、crates/ + apps/desktop を同居させる（§5.1）。
- **理由**: (1) apps/desktop（Tauri）はRustとReact/TSの両方に跨り、分離するとバージョン同期・CI連携のコストが恒常化する。(2) LP（website）・ドキュメント・アプリの文言/ブランド資産を単一リポジトリで共有できる。(3) 個人〜少人数開発ではリポジトリ分割の権限分離メリットがない。
- **帰結**: CIはJS/Rustのパスフィルタで分割実行する（AR-02の独立ビルド要件）。リポジトリサイズ増大はONNXモデル等のバイナリをGit LFSまたはビルド時取得にして緩和する。

---

## 11. 付録B: ストレージにPGLiteを使わない理由

**決定**: SHOGUNのローカルストレージはSQLite（rusqlite）+ sqlite-vec + FTS5（trigram）を採用し、PGLite（WASM版PostgreSQL + pgvector）は採用しない。

**背景**: 初期構想では「Postgres系の機能（pgvector等）をローカルで使う」選択肢としてPGLiteが検討対象だった。以下の理由で不採用とする。

1. **プロセスモデルの不一致**: PGLiteはWASMランタイム上で動くシングルコネクションのPostgresであり、Rustデーモン（shogun-core）から使うにはWASMランタイム（またはNode系ホスト）の同梱が必要になる。データの重心をRustに置く原則（不変条件1）に対し、データ層だけ異質なランタイムを常駐させるのは構成が倒錯する。SQLite+rusqliteは同一プロセス内の関数呼び出しであり、追加ランタイムがゼロである。

2. **常駐デーモンのメモリフットプリント**: SHOGUNは常駐アプリでありNFR-RES-02（RSS 900MB以下）を守る必要がある。WASM上のPostgresはヒープ・ページキャッシュを二重に抱え、アイドル時フットプリントがSQLite（数MB＋ページキャッシュ）より恒常的に大きい。アイドルCPU 5%制約（NFR-SLO-06）にもWASM層のオーバーヘッドは不利に働く。

3. **クラッシュ耐性の実績**: SQLiteのWAL＋トランザクションは電源断耐性の実績が桁違いに長い（NFR-REL-01の前提）。「メモリは年単位で生きる」データを、ブラウザ由来で比較的新しいPGLiteの永続化層に預けるリスクを取る理由がない。

4. **全文検索**: FTS5 trigramはSQLite組み込みで、日本語を含む多言語の部分一致全文検索が索引付きで動く（FR-MEM-20）。PGLiteでPostgresのFTS（tsvector）を使う場合、CJKの分かち書き問題に別途対処が必要で、拡張（pg_trgm等）のWASMビルド保守も自前になる。

5. **ベクトル検索の要件水準**: 本製品のベクトル検索は「Warm層（30日分）のみを対象にした総当たり」で足りる設計であり（FR-MEM-03）、pgvectorのANNインデックス（HNSW等）が必要になる規模をv1では扱わない。sqlite-vecで要件を満たせる以上、pgvectorのためにPostgresを持ち込む理由がない。

6. **Rustエコシステムの成熟度**: rusqlite・refinery・sqlite-vecはRustからの利用実績が厚く、静的リンクで単一バイナリに収まる。PGLiteの主戦場はTypeScript/ブラウザであり、Rustファーストの本構成ではバインディング・運用知見ともに薄い。

7. **配布の単純さ**: SQLiteは追加バイナリ・拡張のnotarization対象物を増やさない（sqlite-vecは静的リンク）。WASMランタイム同梱は署名・更新・サイズの全てで配布コストを増やす。

**帰結・再評価条件**: 将来、(a) マルチデバイス同期でサーバー側Postgresとのスキーマ共有が決定的に有利になる場合、または (b) Warm層を超えるANN検索が恒常要件になる場合（v2ナレッジグラフ）、ストレージ選定を再評価する。その場合もローカルはSQLiteを維持し、同期層で変換する案を第一候補とする。

---

*本書はCLAUDE.md（運用ルール）の下位文書である。矛盾を発見した場合は実装を止め、CLAUDE.mdの絶対不変条件を優先した上で本書の改版を提案すること。*

