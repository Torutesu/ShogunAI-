# 実装計画書 — 残ワークストリーム詳細（エージェント投入用）

| 項目 | 内容 |
|---|---|
| 作成日 | 2026-07-31 |
| 目的 | v1.1決定済み要件の残実装を、**ゴールモードのサブエージェントに1件ずつ委任できる粒度**で計画化する |
| 上位文書 | `/CLAUDE.md`（絶対不変条件）、`docs/requirements-v1.0.md`（v1.1改版済み） |
| 使い方 | 各WSの「エージェント投入プロンプト」をコピーしてMac上のエージェントに渡す。エージェントは本書の該当WS節＋参照要件を読んでから着手する |
| ⚠️ 着手前に | **各WSの「✅ 実装済み」欄を必ず読むこと。** WS1/2/3/5はコア（純ロジック・Db層）が実装・テスト済みで、残りはmacOSネイティブ配線とUIのみ。既存実装を作り直さない |

## 0. オーケストレーション（先に読む）

### 0.1 依存関係と実行順

```
即時着手可（Mac・並列OK）:
  WS1 Evening Wrap UI ──┐
  WS2 ⌥ダブルタップ ────┼─ 相互独立。ただし WS2完了後に WS3 が⌥の記録点を足す
  WS3 action_feedback配線┘
  WS5 mic検知バグ修正 ──── 独立
  WS4 会議ノートUX ─────── Phase A(MTUX-01/03)は独立。enhance生成はWS8のKKキー待ち

ゲート待ち:
  WS6 視覚キャプチャ本実装 ← visspike の Go 判定（docs/vis-capture-spike-runbook.md）
  WS7 コネクタライブ検証   ← Google OAuth / Composio / KK APIキーの準備
  WS8 Dream Cycle Batch分類 ← Select KK APIキー
  WS9 会議音声のローカル暗号化保存 ← MT3レーン。WS4 Phase Aの後が自然
```

### 0.2 並列実行の規律（エージェント全員に適用）

- **1 WS = 1ブランチ = 1エージェント**。ブランチ名は `claude/ws<N>-<slug>`（例: `claude/ws1-evening-wrap-ui`）。ベースは `claude/shogunai-core-features-bo304s`（v1.1要件・バックエンドが載っている）
- 同時に走らせるのは**最大3つ**まで。特に `apps/desktop/src/App.tsx` と `strings.ts` は複数WSが触るため、WS1/WS2/WS4を同時に走らせるとコンフリクトする — **WS1→WS2→WS4の順を推奨**（WS3/WS5は他とファイルが重ならないので並列可）
- 全エージェント共通の掟: CLAUDE.mdの絶対不変条件7項を実装前に読む / clippy warnings deny / `unwrap()`はテスト以外禁止 / スキーマ変更はadditiveマイグレーション＋`docs/migrations/V*-rollback.md` / SLOに触る変更はp50/p95計測をPR本文に貼る / UI文言は`strings.ts`に分離（英語・ブランドルール準拠）
- 完了条件は各WSの「Done定義」。**テストが通らない状態でDoneと報告しない**

### 0.3 完了後のロードマップ（それ以降）

1. WS1-5完了 → **v1.1機能面の完成**。PR化してマージ、Phase 1のM3相当を消化
2. WS6-9完了 → M4-M6相当（連携ライブ・Dream Cycle稼働・会議ノート完全体・視覚キャプチャ）
3. その後のキュー（要件定義から）: 課金＋トライアル（§6.12、M5）→ オンボーディング完成（§6.13）→ 配布（notarization＋updater、§7.6）→ ライブ検証を経てWave 2/3コネクタ既定有効化
4. v1.5: Patterns学習・適用（FR-PAT-02。action_feedbackの蓄積が前提=WS3が今動く理由）
5. v2: チーム共有レイヤー、ナレッジグラフ、同期（要件から再設計）

---

## WS1: Evening Wrap UI配線（desktop）

**ゴール**: 夕方、Notchインジケータがゴールドになり、開くと今日のoutcome/未解決/明日の先頭/今日のloose endsが見える（FR-EB-01〜03）。

**✅ 実装済み（2026-07-31、テスト付き）**:
- `Db::evening_wrap(calendar_tomorrow, day_start_ms, now_ms, tomorrow_end_ms)`（`crates/shogun-core/src/daemon.rs`）— 日窓集計とセクション構成
- `shogun_fusion::wrap`（`EveningWrap`/`WrapOutcome`/`assemble_wrap`）— 順序・上限・confidenceゲート
- **`shogun_fusion::wrap::local_day_bounds(now_ms, utc_offset_seconds) -> (day_start_ms, tomorrow_end_ms)`** — 現地深夜の算出。**シェル側で日境界を自前計算しないこと**、OSからUTCオフセット秒だけ取ってこの関数に渡す（DST・UTC日跨ぎ・エポック前のテスト済み）

**残り**: Tauriコマンド化、生成トリガのスケジューラ、インジケータ/既読、UI表示、設定、strings.ts。

**対象ファイル**: `apps/desktop/src-tauri/src/`（コマンド追加・スケジューラ）、`apps/desktop/src/App.tsx`（表示）、`apps/desktop/src/strings.ts`（文言）。Morning Briefの既存配線（`morning_brief`系コマンドとその表示）を先にgrepして**同じ形**で作ること。

**実装ステップ**:
1. Tauriコマンド `shogun_evening_wrap` を追加。日境界は **`local_day_bounds` に現在のUTCオフセット秒を渡すだけ**（OSからのオフセット取得のみ実装。自前で深夜計算しない）。カレンダー行は現状コネクタ未接続なら空Vecで渡す（接続後にGCal同期結果から供給する拡張点をコメントで明示）
2. 生成トリガ: 常駐スレッドで毎分チェック、既定17:00〜21:00の間に「直近15分ユーザー入力なし」（既存のアイドル検知があれば再利用。無ければ最終capture eventのtsで代用）または設定時刻で1日1回生成。生成済みフラグは日付キーでRAM保持
3. 提示（FR-EB-03）: 生成完了→インジケータをゴールドに（Morning Brief既読管理のコードを共通化して再利用）。**自動ポップアップ禁止**。既読で白へ
4. UI: Expanded内の新セクションまたはBriefと同じビューの切替タブ。4セクション表示、各項目に`possibly`マーカーと根拠（provenance_event_id）チップ — Briefの既存コンポーネントを再利用
5. strings.ts: `eveningWrap*`キー群（英語。「⚔以外の絵文字禁止」等ブランドルール）
6. 設定: 生成ウィンドウの時刻設定（§6.15のGeneral/Dream Cycle欄に準じた場所）

**テスト/受け入れ**: 深夜境界計算のユニットテスト（タイムゾーン跨ぎ・DST）/ 生成パスにHTTPクライアント参照が無いこと（egressゼロ、既存のcheck-http-egress.pyが効く配置）/ 空データで空画面を出さない（各セクション "Nothing yet" 表示）/ 手動: 夕方トリガ→ゴールド→開いて4セクション。

**Done定義**: 上記テスト全通過＋実機で1サイクル動作確認＋`pnpm build`/`cargo clippy`クリーン＋コミット・プッシュ。

**エージェント投入プロンプト**:
> ShogunAI-リポジトリで `docs/impl-plans-2026-07-31.md` の **WS1** を実装して。ベースブランチ `claude/shogunai-core-features-bo304s` から `claude/ws1-evening-wrap-ui` を切る。要件は requirements-v1.0.md §6.17。バックエンド `Db::evening_wrap` は実装済みなのでUI/シェル配線のみ。Morning Briefの既存実装と同型に作り、CLAUDE.mdの不変条件（特にwebviewにロジックを置かない・egressゼロ・割り込まない）を守る。テスト込みでDone定義まで完走したらコミット・プッシュ。

---

## WS2: ⌥ダブルタップ・ワンアクション（FR-NU-10）

**ゴール**: ⌥キー2回タップ（300ms以内、間に他キーなし）で最上位アクションが即時起動する。L1即実行/L2選択済みExpanded/L3確認フロー直行。

**✅ 実装済み（2026-07-31、テスト14件）**: `shogun_core::notch::optiontap`（`OptionDoubleTap` / `Input::{Flags,Key,Pointer}` / `TAP_MAX_MS=250` / `GAP_MAX_MS=300` / `with_timings` / `reset`）。ホールド・⌥コード・⌥ドラッグ・間隔超過・flagsChanged重複・時計巻き戻しの拒否がすべてテスト済み。**検出器を作り直さないこと。**

**対象ファイル**: `apps/desktop/src-tauri/src/`（NSEventグローバルモニタ。既存のグローバルショートカット登録は `tauri_plugin_global_shortcut` なので、素の⌥は別途 `NSEvent::addGlobalMonitorForEventsMatchingMask` の `flagsChanged`/`keyDown`/`mouseDown` が必要）、notch状態機械、設定。

**実装ステップ**:
1. ~~検出器~~ → **実装済み**。アダプタは NSEvent を `optiontap::Input` に変換して `observe(input, now_ms)` を呼ぶだけ（`flagsChanged`→`Flags{option_down, other_modifiers}`、`keyDown`→`Key`、`mouseDown/dragged`→`Pointer`）。**キーコードや文字を読まないこと**（NFR-PRV-03。`Input`は型としてそれを持てない）
2. NSEventグローバルモニタに配線（macOS側。`#[cfg(target_os = "macos")]`）。発火→context cacheの最上位アクションを取得（`Db::context_actions`既存経路。**押下時に組み立てない** — cache読取のみ、AR-08）
3. レベル分岐: L1=実行して事後表示 / L2=Expandedを開き該当アクションを選択状態に（1操作で確定できるフォーカス）/ L3=既存のL3確認UIへ直行。**⌥経由でもL3省略禁止**（既存の許可テーブルが強制するが、UIショートカットで迂回しないこと）
4. 誤発火計測（FR-NU-10受け入れ条件）: 発火回数・その後3秒以内にESC/外側クリックで閉じた率（=誤発火プロキシ）をspike-harness系メトリクスに記録し、`shogun metrics`/Advanced画面に出す
5. 設定: 有効/無効・発火方式（ダブルタップ/長押し — 長押しは将来枠でUIのみ用意可）・Notch展開SLO（100ms）計測は既存NFR-SLO-01計測に乗せる
6. 確定履歴をaction_feedbackへ（WS3が先に終わっていれば `surface: OptionKey` で記録。未了ならTODOコメントで接続点を明示）

**テスト/受け入れ**: 検出器のユニットテスト（正常2タップ/間にキー挟み/301ms/⌥+ドラッグ中は不発）/ L3が確認なしで実行されないこと（既存invariant4テスト群がカバーすることを確認）/ 実機で誤発火率を1日計測しランブック様式で記録。

**Done定義**: テスト通過＋実機1日ドッグフードの誤発火メモ＋clippy/buildクリーン＋プッシュ。

**エージェント投入プロンプト**:
> ShogunAI-リポジトリで `docs/impl-plans-2026-07-31.md` の **WS2** を実装して。ブランチ `claude/ws2-option-double-tap`（ベース同上）。要件は requirements-v1.0.md FR-NU-10（決定: 既定ダブルタップ・変更可）。検出器は純ロジックでテスト可能に分離し、押下時はcache読取のみ（AR-08）。L3の明示確認は絶対に省略しない（不変条件4）。誤発火計測コード同梱まで含めてDone。

---

## WS3: action_feedback 記録の配線（FR-PAT-01の書き手）

**ゴール**: ユーザーの採択・修正・却下・[Track]確定が実際に `action_feedback` テーブルに落ちる（現状: テーブルとAPIのみ存在、書き手ゼロ）。

**✅ 実装済み（2026-07-31、テスト付き）**:
- `Db::record_action_feedback(action_kind, surface, outcome, context_app, rank, latency_ms) -> bool`（`daemon.rs`）
- `Db::action_acceptance_by_kind(since_ts)`（FR-CF-03のランキング入力）
- `Surface::from_wire` / `Outcome::from_wire`（未知値は既定値に落とさず`None`。コマンド境界のパース用）

**残り**: Tauriコマンド化と4決定点からの呼び出し。

**対象ファイル**: `apps/desktop/src-tauri/src/`（Tauriコマンド）、`apps/desktop/src/App.tsx`（決定点からの呼び出し）。

**実装ステップ**:
1. Tauriコマンド `shogun_action_feedback`（引数: action_kind, surface, outcome, rank, latency_ms を文字列/数値で受け、`from_wire`でパースしてから`Db::record_action_feedback`へ。**内容テキストの引数を作らない** — Db側もシグネチャで拒否している）
2. 決定点に呼び出しを追加（この4箇所をgrepで特定）:
   - Notchアクションボタンの実行時（accepted, surface=notch, rank=ボタン位置）
   - 提案の明示却下/パネルを開いて3秒以上見てから閉じた場合（dismissed）— 「見ずに閉じた」はノイズなので記録しない
   - インラインドラフト: そのまま挿入=accepted / 編集後挿入=edited / 破棄=dismissed（`inline_source.rs`の挿入経路）
   - Recap候補の[Track]=tracked / 破棄=discarded（meeting recap UI）
3. latency_ms = 提案表示ts→決定tsをUI側で計測して渡す
4. 削除との整合: FR-SET-07の全削除に含まれること（実装済み・テスト済み）を確認するのみ

**テスト/受け入れ**: ラッパのユニットテスト / コマンド層に content/text 引数が存在しないこと / 手動: 各決定点を操作→ `sqlite3` で行が入りoutcome/surfaceが正しいこと確認（確認後にテスト行を削除）。

**Done定義**: 4決定点すべて配線＋テスト＋クリーン＋プッシュ。

**エージェント投入プロンプト**:
> ShogunAI-リポジトリで `docs/impl-plans-2026-07-31.md` の **WS3** を実装して。ブランチ `claude/ws3-feedback-wiring`。要件は FR-PAT-01 / FR-CF-06。記録はメタデータのみで、コマンドのシグネチャにテキストを受ける引数を作らないこと（プライバシーを型で担保）。デバイス外送信は一切なし。

---

## WS4: 会議ノートUX（FR-MTUX群）

**ゴール**: 会議ノートが「仕組み」から「使いたくなるノートプロダクト」になる。実装順は requirements §9.6（本書では§6.22）どおり4フェーズ。

**Phase A — 専用ウィンドウ + co-writing（MTUX-01/03）**

**✅ 実装済み（2026-07-31）**: 清書の器。V13マイグレーション `session_notes_enhanced` ＋ロールバックdoc、`session_notes::{save_enhanced, get_enhanced}`、`delete_all` への追加、`LATEST_SCHEMA_VERSION=13`。**原文を上書きできない構造**（別テーブル・別関数）をテストで固定済み。

1. Recordingピルから開く専用 `WebviewWindow`（Tauri multiwindow。ノッチパネルとは別ウィンドウ・通常のキーフォーカス可）。ユーザーメモ欄（既存 `session_notes` のupsert経路を再利用）＋ `Listening · N participants` 静的1行
2. ~~清書の器~~ → **実装済み**。UIは `get_enhanced` が `None` のとき原文のみ表示（Batchレーン未整備時の恒常状態。待ち状態にしない）
3. 清書ジョブはRecap生成（FR-MT-16のBatchレーン）と同一ジョブに載せる — **KKキー未整備の間は縮退**（enhanced無し・原文のみ表示）。UIは enhanced があれば2層トグル（Original / Polished）
4. Notch Expandedの簡易メモ（FR-MT-10）はこのウィンドウの縮小ビューであることをコメントに明記し、保存経路を共通化

**Phase B — 会議ライブラリ（MTUX-02）**
1. Full UIに「Meetings」タブ: `sessions` テーブル一覧（日付降順）、検索は既存FTS（transcript_segments/session_notesが対象に入っているか確認、無ければFTS対象に追加）
2. シリーズビュー: タイトル正規化＋app_bundle_idで繰り返し会議をグルーピングし、前回の決定/[Track]済みcommitmentsを次回セッションのヘッダに表示
3. 各行 → Recap / 文字起こし / ノート / （WS9後）音声への導線

**Phase C — ライブ表示トグル + リアルタイム翻訳（MTUX-05/06。OPEN-18スパイク含む）**
1. FR-MT-10改定: ライブ文字起こしペインを既定OFFのトグルで（transcript_segmentsのライブ購読イベントを追加）
2. **翻訳スパイク（先行・別ブランチ可）**: macOS Translation framework（macOS 15+ / swift呼び出しが必要なら小さなヘルパ検討）で ASRテキスト→設定言語のストリーミング翻訳を検証。品質・レイテンシ・対応OSをランブック様式で記録 → Go なら本実装、No-Go なら BYOKテキスト翻訳（Messages APIストリーミング・音声は出さない・トレーサビリティ必須）をオプトインで
3. CPUは会議中15%枠（FR-MT-21）に含め、超過時は翻訳を間引く（OPEN-19決定）

**Phase D — チップ・テンプレート・編集・エクスポート（MTUX-04/07/08/09）**
1. 決定/宿題らしき発言の候補チップ（既存extract cueをtranscript_segmentsにインライン適用、静かに追加・削除可能・音なし）
2. テンプレート（1on1/商談/スタンドアップ/インタビュー）: session開始時にタイトル・シリーズ履歴から候補提示、選ぶとノート欄に骨組み挿入
3. Recap編集可能化（[Track]チップとWhy?根拠ジャンプは編集後も維持）
4. エクスポート: Markdownコピー/ローカル書き出し=L2。外部共有は実装しない（L3・スコープ外）

**テスト/受け入れ**: §6.22受け入れ基準4項（清書が原文を書き換えない/トグル既定OFF/翻訳経路に音声が乗らない/翻訳ON時CPU15%枠内計測）＋V13ロールバックdoc。

**Done定義**: フェーズごとにコミット。Phase A/BがマージラインでPhase C/Dは続行タスク可。

**エージェント投入プロンプト**:
> ShogunAI-リポジトリで `docs/impl-plans-2026-07-31.md` の **WS4 Phase A** から実装して。ブランチ `claude/ws4-meeting-notes-ux`。要件は requirements-v1.0.md §6.22 と FR-MT群。原文ノートを上書きしない2層設計、既定OFFの規律（FR-MT-01/02）、Batchレーン縮退（KKキー未整備）を守る。Phase Aを完了・プッシュしたらPhase Bへ。CとDは着手前に翻訳スパイクの結果を報告して指示を待つ。

---

## WS5: 会議検知のmic信号バグ修正

**症状（2026-07-31実機ログで確認）**: `[meeting] saw <あらゆるアプリ> state=recording mic=true` — Finder/loginwindowですらmic=true。常駐アプリ（VoiceOS等）がマイクを掴み続けているため、**システム全体の「入力デバイス稼働中」を読む現行実装では信号②が常時真**になり、FR-MT-04のconfidenceが常時底上げされている。

**✅ 実装済み（2026-07-31、テスト32件）**:
- `detect::MicSource::{Holder{bundle_id}, SystemWide}` と `detect::MicObservation` — 帰属の有無を型で区別
- `MicWatch::observe(&MicObservation, now)` — Holder経路（既知会議アプリ or 前面アプリのみ信頼、自プロセス除外）と SystemWide経路（stuck判定: 会議コンテキスト無しで3アプリ跨ぎ＋2分下限。会議出現/デバイス解放で回復）
- desktop `meeting.rs` の呼び出し側を新APIへ更新（前面アプリと会議コンテキストを同梱）
- `mic.rs` モジュールdocにプロセス帰属の実装手順を記録

**残り（macOS実機作業）**:
1. **本命修正**: macOS 14.4+の CoreAudio プロセスAPI（`kAudioHardwarePropertyProcessObjectList` → 各processの `kAudioProcessPropertyIsRunningInput` と `kAudioProcessPropertyPID` → bundle id解決）を `mic.rs` に実装し、`MicSource::Holder{bundle_id}` を報告する。取得は真偽のみ・音声ストリームに触れない（既存docコメント参照）
2. 14.0〜14.3または取得失敗時は現行の `SystemWide` にフォールバック（**分岐だけ書けばよい。判定ロジックは実装済み**）
3. 実機での前後比較: 修正前はFinder/loginwindowで `mic=true`、修正後はそれらで会議判定が出ないこと

**テスト/受け入れ**: stuckヒューリスティックのユニットテスト（連続true×アプリ切替→無効化、会議アプリ前面での新規true→有効）/ 実機: VoiceOS常駐状態でFinder前面時に `mic=true` が出ないこと / 会議アプリで実際にマイクONにしたときは検知すること。

**Done定義**: テスト＋実機ログでの前後比較（修正前のログは本書§WS5の症状欄が基準）＋プッシュ。

**エージェント投入プロンプト**:
> ShogunAI-リポジトリで `docs/impl-plans-2026-07-31.md` の **WS5** を修正して。ブランチ `claude/ws5-mic-signal-fix`。症状と方針は同節に記載。マイクからは「使用中か」の真偽のみ読み、音声ストリームには絶対に触れない（FR-MT-04）。純ロジック（stuck検知）はLinuxでもテストが通る形に分離する。

---

## WS6: 視覚キャプチャ本実装（FR-VIS群）〔ゲート: visspike Go判定〕

**ゴール**: イベント駆動キーフレーム＋オンデバイスOCRを製品キャプチャレーンに載せる（requirements §6.21）。**スパイクGoまで着手禁止**。Goしたらスパイク実測のパラメータ（ダウンスケール幅・JPEG品質・dHash閾値・内容変化フロア）を初期値に採用する。

**マイルストーン**:
- **M-VIS1 取得**: `crates/shogun-core/src/vis/` 新設（feature `vis`）。`objc2-screen-capture-kit` でSCScreenshotManagerキーフレーム取得。トリガは既存focus watcher＋AXテキスト差分（spikeと同じ）。pHash(dHash)重複破棄は純ロジックで分離（spikeのSwift実装 `spikes/vis-capture/Sources/visspike/main.swift` のdhash関数を移植）
- **M-VIS2 抽出**: `objc2-vision` VNRecognizeTextRequest → 抽出テキストを `ingest_capture` 既存経路へ（`kind="ocr_text"` をevent_log kindに追加。additive）。NFR-SEC-07リダクション適用
- **M-VIS3 保持**: V14マイグレーション `keyframes(id, ts, app_bundle_id, rel_path, bytes, phash, created_at)` ＋フレーム本体は `Application Support/.../keyframes/` に暗号化して保存。
  - **✅ 保持ポリシーは実装済み**: `shogun_memory::retention`（`Policy::keyframes()` = 7日/5GB、`sweep(items, now) -> Sweep{expired, over_budget}`）。期限切れ→予算超過の順、最古から退避、境界包含、時計巻き戻し耐性をテスト済み（13件）。**削除ルールを再実装しないこと** — 行を`retention::Item`に写して`sweep`を呼び、返ったidを消すだけ
  - **鍵管理の先例**: `shogun_memory::DbKey`（SQLCipher用）と同型にする — 鍵はデスクトップ層がKeychainから読んで**注入**し、crate側はKeychainを知らない（Linuxテスト可能性の維持・不変条件7）。WS9の音声と鍵管理モジュールを共通化する
- **M-VIS4 権限とガバナンス**: 画面収録TCC要求をオンボーディングへ（FR-VIS-07。拒否時はAXのみへ縮退・1時間ごと再検出）。除外リスト/一時停止/SecureTextField前面時はフレーム取得自体をスキップ（FR-VIS-04。**キャプチャ前段で**）。設定UI（ON/OFF・保持モード・期間・使用量）
- **M-VIS5 計測**: CPU/メモリ/ストレージの別枠メトリクス（spike-harness系）。バッテリーをContext Healthへ

**受け入れ（§6.21）**: 連続録画API不使用のアーキテクチャテスト / 除外・停止中フレームゼロ / 期限自動削除 / 画像が共通HTTP出口に到達しない依存検査 / 権限拒否縮退。

**エージェント投入プロンプト**:
> （スパイクGo判定後に使用）ShogunAI-リポジトリで `docs/impl-plans-2026-07-31.md` の **WS6 M-VIS1〜2** から実装して。ブランチ `claude/ws6-vis-capture`。要件は requirements-v1.0.md §6.21。不変条件: 連続録画禁止・画像はいかなる経路でもデバイス外へ出さない（クラウドVLM禁止）・除外リスト同一適用。スパイク実測値: <ここにsummary.mdの数値を貼る>。M-VIS1完了ごとにコミットし計測値を残す。

---

## WS7: コネクタ・ライブ検証〔ゲート: クレデンシャル〕

チェックリストの正本は `docs/connector-summary-and-live-checklist.md` §4。エージェントの仕事はチェックリスト消化＋発見した不具合の修正（特に: Slack/Notion/GitHub/Linearの実`tools/list`でのtoolmap確定、`result.rs`のフィールドマッピング実レスポンス照合）。

**エージェント投入プロンプト**:
> （OAuthクライアント/Composioキー/KKキー準備後）ShogunAI-リポジトリで `docs/connector-summary-and-live-checklist.md` §4 のライブ検証をWave 1から消化して。ブランチ `claude/ws7-connector-live`。トークンはKeychain以外に書かない（不変条件7）。チェック結果は同docに追記し、修正はコミット分離（fix: ごと）。

---

## WS8: Dream Cycle Batch分類の稼働〔ゲート: Select KK APIキー〕

**ゴール**: 「唯一の未着手」だったBatch分類を配線し、ローカル抽出の低confidence候補が夜間に昇格・破棄されるようにする（監査doc `context-layer-audit-and-plan.md` Phase M/Q 残項目）。

**実装ステップ**:
1. KKキーのKeychain格納（アカウント名は既存BYOK実装の命名規約に合わせる）と、`llm/`のBatch APIクライアント（`route=batch_api`のトレーサビリティは実装済み — 実HTTP+ポーリングを`net` feature下に）
2. スケジューラ（FR-DC-01）: 02:00-06:00ウィンドウ＋アイドル/電源条件。条件未達は縮退版（実装済みの`run_local_maintenance`）
3. 分類ジョブ: 当日イベント＋低confidence候補をチャンク化→Batchで構造化出力（due dateパース・people/projectsへのentity linking・較正confidence）→ upsert（冪等性は`job_runs`既存基盤）。**LOCAL_RULE_MAX_CONFIDENCE(0.4)を超える昇格はこの経路のみ**が正
4. Morning Brief生成ジョブを最終段に接続（FR-MB-01。縮退Briefは実装済み）
5. 送信チャンクは全件トレーサビリティ（実装済みの共通出口経由を確認）

**受け入れ**: FR-DC群の受け入れ基準（途中kill再開・二重実行冪等・Batch全滅時にローカル無影響）＋実機で1晩回して昇格が起きること。

**エージェント投入プロンプト**:
> （KKキー投入後）ShogunAI-リポジトリで `docs/impl-plans-2026-07-31.md` の **WS8** を実装して。ブランチ `claude/ws8-dream-cycle-batch`。鍵レーン厳守: このジョブはSelect KKキーのみ・BYOK禁止（不変条件5）。送信は処理用チャンクのみ＋全件トレーサビリティ（不変条件3）。

---

## WS9: 会議音声のローカル暗号化保存（FR-MT-12改定の実装）

**ゴール**: Recording中の音声をセッション単位のローカル暗号化ファイルに保存し、Recapから聞き直せる。30日自動削除。

**実装ステップ**:
1. V15マイグレーション `session_audio(session_id UNIQUE, rel_path, bytes, created_at)` ＋ロールバックdoc（`expires_at`は持たない — 保持期間は設定値であり、行に焼き付けると期間変更が既存行に効かなくなる。期限は`retention::Policy`が実行時に判定する）
2. 録音ライタ: 既存の取得バッファ（cpal/Core Audio tap）から分岐して暗号化ストリームで追記書き。**Recording状態の間のみ**（FR-MT-12）。鍵は`DbKey`と同型に注入（WS6 M-VIS3参照）
3. 保持: **✅ ポリシーは実装済み** — `retention::Policy::audio()`（30日/5GB）＋`sweep`。残りは`maintenance`から呼ぶ削除ジョブ、セッション単位即時削除、設定UI（期間変更・使用量・全削除）— §6.15 Meeting notes欄は要件反映済み
4. 再生導線: Recap/ライブラリ（WS4 Phase B）から該当セグメントへのシーク再生（復号ストリーム→再生。transcript_segmentsのtsでリンク）
5. 開示文言の切替（FR-MT-03改定文言 + `meetingDisclosure` のコメントに記載済みの手順どおり、**この変更と同一コミットで**）
6. ASR接続: ファイルが残るようになったため、将来の再文字起こしAPI（`retranscribe(session_id)`）の受け口だけ用意（実装はモデル改善時）

**受け入れ（§6.16改定済み基準）**: 音声ファイルが暗号化されている / Recording外で音声デバイスにアクセスしない / 期限超過自動削除 / 即時削除がファイル+参照を消す / 音声が外部送信経路に到達しない。

**エージェント投入プロンプト**:
> ShogunAI-リポジトリで `docs/impl-plans-2026-07-31.md` の **WS9** を実装して。ブランチ `claude/ws9-audio-retention`。要件は FR-MT-12改定（2026-07-30）と§6.16受け入れ基準。音声はいかなる経路でもデバイス外へ出さない（不変条件3・クラウドASR禁止）。開示文言の切替を実装と同一コミットで行う（FR-MT-03の一致原則）。

---

---

## 付録: 実装ログ

| 日付 | 内容 | コミット |
|---|---|---|
| 2026-07-31 | WS5コア: mic信号のstuck判定＋帰属型（テスト32件） | `d0cc24d` |
| 2026-07-31 | WS2コア: ⌥ダブルタップ検出器（14件）／WS3コア: Db記録API・wireパース | `f61cd06` |
| 2026-07-31 | WS1コア: `local_day_bounds`（日境界の純ロジック、DST/エポック前テスト） | `5c6cdd5` |
| 2026-07-31 | WS4 Phase Aコア: V13 `session_notes_enhanced`（2層ノートの器） | `4314562` |
| 2026-07-31 | WS6/WS9共通: `retention` 保持ポリシー（期限＋予算、13件） | 本コミット |

**なぜコアだけ先に入っているか**: これらはLinuxセッションで**テストまで検証できる**部分だから。macOSネイティブ配線（NSEventモニタ・CoreAudioプロセスAPI・Tauriコマンド・UI）はビルドできない環境なので、意図的にMac側エージェントに残してある。逆に言えば、残作業は「アダプタを書いて既存の型に渡す」だけに縮んでいる — 判定ロジックを再発明しないこと。

---

*計画は生き物: 各エージェントは着手時に対象ファイルの現状をgrepで確認し、本書と実装が食い違う場合は「実装が正・本書に追記」で進めること（ただしCLAUDE.mdの不変条件と要件のMUSTには従う）。*
