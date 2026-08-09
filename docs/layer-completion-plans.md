# 詳細実装計画: 会議検知拡張 / L3ループ / L4 / L5

**Status**: Plan v1（2026-08-09）
**親文書**: `docs/layer-completion-designs.md`（設計判断）。本書はWP単位の実装計画（変更ファイル・テスト・受け入れ基準・日割り）。
**規約**: 純ロジックはLinux green（`cargo test --workspace --exclude shogun-desktop-spike` + clippy warnings deny）。実機結果は `docs/phase1-findings.md` に追記。スキーマ変更はマイグレーション＋ロールバック手順必須。

---

# Plan A — 会議検知拡張（Zoom Web / Teams / Slackハドル）

## A-0 前提リファクタ: 強/弱シグナルの型を分ける（半日）

現状 `detect.rs` は `MEETING_BUNDLE_IDS`（strong）1本で、Teams系を足すと「開いただけで会議」になる。先に強弱を型で分ける。

**変更**: `crates/shogun-core/src/meeting/detect.rs`

```rust
pub enum MeetingHint { Strong, Weak }

const STRONG_MEETING_BUNDLES: &[&str] = &["us.zoom.xos"];
const WEAK_MEETING_BUNDLES:   &[&str] = &[];        // A-2で埋める
const STRONG_MEETING_HOSTS:   &[&str] = &["meet.google.com"];
const WEAK_MEETING_HOSTS:     &[&str] = &[];        // A-1/A-2で埋める

pub fn bundle_hint(bundle_id: &str) -> Option<MeetingHint>;
pub fn host_hint(url: &str) -> Option<MeetingHint>;   // 既存 host_of() 経由（substring攻撃対策維持）
```

- `DetectionCtx` に `has_weak_meeting_signal: bool` を追加。`has_zoom_bundle` は `has_strong_bundle` にリネーム（呼び出し側 `apps/desktop/src-tauri/src/meeting.rs:413-436` を同時修正）
- `has_strong_opener()` はStrongのみ真。Weakは `corroborating_count()` の1票（mic sustained等ともう1票必要）
- 既存 `is_meeting_app` / `is_meeting_url` は `bundle_hint(..).is_some()` 系の薄いラッパとして残す（他呼び出し互換）

**テスト**（既存テスト群と同型で追加）:
- `a_weak_bundle_alone_does_not_offer`
- `a_weak_bundle_with_sustained_mic_offers`
- `strong_hosts_still_offer_alone`
- 既存全テストがリネーム後もgreen

## A-1 Zoom Web（数時間）

- `STRONG_MEETING_HOSTS` に `app.zoom.us` を追加（Zoom Webクライアントのホスト。参加ページも同ホストだが、参加ページ→即会議遷移のためStrongで実害なし。誤検知が実機で出たらWeakへ降格）
- テスト: `zoom_web_client_host_offers` / `zoom_marketing_site_does_not`（`zoom.us` ≠ `app.zoom.us` を確認）

## A-2 Teams / Webex（1日）

- `WEAK_MEETING_BUNDLES` += `com.microsoft.teams2`, `com.microsoft.teams`, `Cisco-Systems.Spark`
- `WEAK_MEETING_HOSTS` += `teams.microsoft.com`, `teams.live.com`
- Webexサブドメイン（`*.webex.com`）: `is_media_host` と同型の末尾一致ヘルパ `host_matches_suffix` を流用してWeak判定
- **終了条件の確認**: `end_condition` の「アプリ消滅で終了」はWeak bundleにも適用されるが、Teamsは会議後もアプリが残るため「mic停止（既存 silence limit）」が主終了条件になることをテストで固定: `teams_meeting_ends_on_silence_not_on_app_presence`
- テスト: `teams_frontmost_alone_never_offers` / `teams_with_mic_sustained_offers` / `webex_subdomain_matches` / `webex_admin_pages_do_not_auto_start`

## A-3 実機QA（1日・人間）

`docs/phase1-ondevice-runbook.md` に追記する確認手順:

- [ ] Teamsを開いてチャットだけ10分 → オファー0件
- [ ] Teams会議参加 → mic sustained後にオファー、終了60s後に自動終了
- [ ] app.zoom.us で参加 → オファー、退出でgrace後終了
- [ ] Webex参加/管理画面それぞれ
- 結果を `phase1-findings.md` 実機ログテンプレへ

## A-4 Slackハドル（別枠1.5〜2日）

bundle同一（`com.tinyspeck.slackmacgap`）のためテーブルでは不可。**新シグナル `huddle_hint`** を追加。

**変更**:
1. `detect.rs` に純関数:
   ```rust
   /// Slackのウィンドウタイトル/AXテキストからハドル中の手掛かりを検出。
   /// "Huddle" / "ハドル" + 通話コントロール語彙（mute/leave等）の共起で真。
   pub fn huddle_hint(window_title: Option<&str>, ax_snippets: &[&str]) -> bool;
   ```
2. `DetectionCtx` に `has_huddle_hint: bool`。**ポリシー: huddle_hint単独では何もしない。mic sustained ≥10s との2票でWeakオファー（自動開始なし・確認付きのみ）**
3. `apps/desktop/src-tauri/src/meeting.rs`: frontmostがSlackのとき、既存タイトル読み取り＋capture済みAXテキスト先頭N件を `huddle_hint` に渡す
4. 終了: mic停止60s または hint消失grace（既存 `meeting_url_left_past_grace` と同型の `huddle_hint_lost_past_grace`）

**テスト**: `huddle_title_plus_mic_offers` / `huddle_title_alone_does_nothing` / `slack_normal_call_words_in_chat_do_not_trigger`（"huddle"がメッセージ本文に出るだけのケース）/ 終了grace系

**受け入れ**: 実機でSlack通常利用1時間・誤オファー0件、実ハドルでオファー→録音→recap生成。

**Plan A 合計: 3〜4日（うちLinuxで完結: A-0/A-1/A-2/A-4のロジック）**

---

# Plan B — L3: 体験ループを閉じる（~2週間）

ゴール: **「パネルを見る → ボタンを押す → 承認 → 実際にGmailが送信される」を実機で成立させる。**

## B-1 notch actions UI（2〜4日）

バックエンドは完成済み: `notch_actions::notch_actions() -> Vec<ActionView>`、`notch_exec::run_notch_action`、`confirm_notch_action`（登録済み・フロント未呼び出し＝E-10）。

**変更**: `apps/desktop/src/App.tsx`
1. 展開パネルに `ActionsRow` コンポーネント新設（chat入力の上）: 展開イベントで `invoke('notch_actions')` → ≤4ボタン描画。空なら汎用アクション（Fusionの never-empty 保証があるので常に何か出る）
2. クリック → `invoke('run_notch_action', {id})` → 戻り値のdispositionで分岐:
   - L1実行済み → 完了トースト（結果1行）
   - L2 → インライン確認チップ（Confirm/Cancel）→ `confirm_notch_action(id)`
   - L3送信系 → 「承認キューに追加」表示＋Approvalsバッジ加算
3. **SLO-02計測を同梱**: 展開時刻→ボタン描画完了を `performance.now()` で計測し `record_slo('actions_present', ms)` コマンド（新設・spike-harness の記録系を流用）へ。`shogun metrics` で `measured:true` になる
4. キャッシュ鮮度: L2.2のバス配線（`IntegrationSynced`→cache無効化）が入り次第、展開中の再fetchを購読で駆動（それまでは展開時fetchのみ）

**テスト**: Rust側はdisposition列挙の網羅テスト（既存engine testsに追加）。TS側はActionsRowの分岐スナップショット。実機で ⌘⇧J self-test と同一結果になることを確認。

**受け入れ**: 実機でアクション提示p95を計測し150ms以下（SLO-02）を `phase1-findings.md` に記録。

## B-2 ローカル効果の実体化（1日）

**変更**: `apps/desktop/src-tauri/src/notch_exec.rs` の `LocalEffector::run`
- `ShowNotification` → `UNUserNotificationCenter`（objc2）。権限リクエストはonboardingの既存許可フローに追記
- `CopyToClipboard` → `NSPasteboard.generalPasteboard`（`inline_source.rs` に既存のpasteboard利用があれば流用）
- `OpenApp` → `NSWorkspace.openApplication`
- `RevealFile` → `NSWorkspace.activateFileViewerSelecting`
- `eprintln!` フォールバックは削除し、未対応バリアントはコンパイルエラーになるようmatchを網羅化

**テスト**: 効果はmacOS実機のみ。Linux側は「全バリアントがEffector traitで網羅されている」ことをexhaustive matchで担保。

## B-3 承認キュー統合（E-08解消, 1〜2日）

**現状**: `mcp.rs:35` / `shogun_api.rs:71` / `lib.rs:463` が各自プライベートキューを持ち、UIは1つしかドレインしない。

**変更**:
1. `crates/shogun-core/src/approval.rs` の `ApprovalQueue` を `Arc<Mutex<_>>` でdaemon状態に1個だけ生成し、3箇所へ注入（コンストラクタ引数化）
2. `ApprovalItem` に `origin: Origin { Ui, Api, Mcp }` を追加。`list_approvals` が全originを返し、UI（`App.tsx` ApprovalsSection）にoriginタグ表示
3. 期限切れ（10分）ロジックは既存のまま単一キューに適用

**テスト**: `invariant4.rs` に「API経由で投入したL3が `list_approvals` に現れ、`confirm_send` で送信実行される」統合テストを追加。孤立キュー3箇所の削除をもってE-08クローズ。

## B-4 OAuth実配線＋Gmailライブ検証（2〜3日＋人間の準備）

**変更**: `apps/desktop/src-tauri/src/connectors.rs`
1. `connect_service` の `mark_connected()` 偽装（E-26）を撤去し、書き済みの `oauth.rs`/`oauth_flow.rs`（PKCEループバック）→ `keychain_store.rs` 保存 → 成功時のみ `mark_connected` の順に配線
2. `transport_serves` はGmailのままでよい（スコープ拡大しない）。Calendar以降は `ConnUi::ComingSoon` 維持
3. 失敗パス: ブラウザ不発火・タイムアウト・拒否をamber状態＋再試行導線に落とす（`FR-INT-06/07` の状態機械は実装済み）

**ライブ検証**（`connector-summary-and-live-checklist.md` §4 を初実施・人間タスク）:
- [ ] Google OAuthクライアント作成（`docs/oauth-client-setup.md`）、env設定
- [ ] Composio APIキーをKeychainへ、`SHOGUN_COMPOSIO_USER_ID` 設定
- [ ] 接続→15分同期→`ingest_integration` がevent logに落ちる→検索でヒット、まで確認
- [ ] 結果と実レスポンスでの `result.rs` フィールドずれを記録・修正

**受け入れ**: 実アカウントで connected 状態が再起動を跨いで維持（トークン自動リフレッシュ動作）。

## B-5 Reply Drafter v1（2〜3日）

7プリセットの実行体は作らない。**「返信ドラフト」1本だけ**を、既存部品の直列で成立させる。

**経路**: notch action候補に `DraftReply`（Fusion `assemble` は宛先/スレッドをscreen_ctx＋stateから既に持てる）→ `run_notch_action` → 既存 `approvals::mac::draft_reply`（BYOK/委譲でLLM呼び出し実装済み）→ 本文プレビュー付きで承認キューへ（B-3の単一キュー）→ Approve → `send_exec::execute_send` → Composio `GMAIL_SEND_EMAIL` → トレーサビリティ記録（digestのみ・実装済み）

**変更点は接続のみ**:
1. `shogun-fusion/assemble.rs`: 返信文脈（未返信メール由来のopen_loop等）があるとき `DraftReply` 候補を出す採点を追加
2. `notch_exec.rs`: `DraftReply` disposition → `draft_reply` 呼び出し → キュー投入
3. `App.tsx`: 承認カードに本文プレビュー＋編集フィールド（**編集はL5のfeedback_events取得点になるので、v1から編集可能にしておく**）

**E2E受け入れ（=GTMプロトタイプゲート）**: 実機で自分宛メールに対し、パネル提示→Draft reply→編集→承認→受信箱に実メール到着→トレーサビリティ画面に記録、を通しで実演可能。

## B-6 検索UI（1〜2日・並行可）

- `App.tsx` パネルに検索ボックス（`/` ショートカット）。バックエンドは `search_hybrid` 実装済み——desktop側コマンドが無ければ `search_memory(query, limit)` を新設（`shogun-mcp` のMemoryBackend呼び出しを流用）
- 結果行: excerpt＋出典アプリ＋時刻、クリックでコピー/該当stateへ
- **SLO-04計測を同梱**（B-1と同じ `record_slo` 経路）

**Plan B 日割り**: D1-3 B-1 → D4 B-2 → D5-6 B-3 → D7-8 B-4（並行して人間がクレデンシャル準備）→ D9-10 B-5 → D11 E2E＋SLO記録。B-6は隙間で。

---

# Plan C — L4: Brief非劣化＋Batch relay（~1.5週間）

## C-1 Morning Brief 非劣化（2〜3日）

**マイグレーション V15**（ロールバック: `DROP TABLE briefs;`）:
```sql
CREATE TABLE briefs (
  date TEXT PRIMARY KEY,          -- 'YYYY-MM-DD' ローカル日付
  payload TEXT NOT NULL,          -- BriefView JSON
  generated INTEGER NOT NULL,     -- 生成文が付いたか
  built_at INTEGER NOT NULL,
  prev_digest TEXT                -- FR-MB-06 updated判定用
);
```

**変更**:
1. `dreamcycle/jobs.rs` の `JobKind::MorningBrief => Ok(())` を実装に置換:
   - 材料: state tables＋当日 `meeting_recaps`＋期限当日 `commitments`（Calendar連携が生きるまでのカレンダー代替。B-4完了後に実カレンダー行へ差し替え）
   - `fusion::brief::assemble_brief` で組み立て。Summarizer seamはBatch可なら生成文、不可なら `LocalExtractiveSummarizer`（既存honest degradationパターン）
   - `briefs` にUPSERT、`prev_digest` 比較で `updated` フラグ（E-19b解消）
2. `apps/desktop/src-tauri/src/fullui.rs:315-335`: 当日 `briefs` 行を読むだけに変更。行なし時のみ現行の劣化組み立てにフォールバック。`generated` はpayloadの値をそのまま表示（固定false廃止）
3. 朝イチ表示は読み取りのみ＝即時・オフライン安定

**テスト**: ジョブの冪等性（`job_runs` 再開でUPSERT二重なし）/ updatedフラグ差分 / フォールバック経路。全てLinux green可能。

**受け入れ**: 実機で夜間Dream後の朝、カレンダー相当行＋提案アクション付きBriefが即表示。

## C-2 Batch relay 実装（サーバ3〜5日＋クライアント1日）

`docs/batch-relay-design.md` 確定済み設計の実装。**実装地: `apps/api`（現在空・package.json あり→TypeScript/Hono、Cloudflare Workers or Fly）**

**v1エンドポイント**:
- `POST /v1/batch` — `Authorization: Bearer <license JWT(ES256)>` 検証 → プラン/日次上限チェック → Anthropic `POST /v1/messages/batches` へ委譲（custom_id素通し）→ `usage(batch_id, license_id, chunks, created_at)` 計上
- `GET /v1/batch/:id` — status/results 中継（resultsは署名付き一時URLでなくストリーム中継。中継サーバはbodyを保存しない＝不変条件3をサーバ側にも延長）
- 秘密: AnthropicキーはサーバSecretのみ。ログにbody・キーを出さないミドルウェアを最初に書く

**クライアント**: `crates/shogun-core/src/llm/anthropic.rs` の `AnthropicBatchClient` と同traitで `RelayBatchClient` を追加し、`dream.rs` の選択を差し替え。**開発用直叩き経路（E-38）は `cfg(debug_assertions)`＋env必須にし、`check-secret-exposure.py` にリリースバイナリへの直Anthropic URL混入チェックを追加**

**テスト**: サーバはJWT検証/上限/計上のユニット＋Anthropicモック。クライアントはtrait差し替えのみなので既存Dreamテストがそのまま効く。

**受け入れ**: 実機Dream Cycleがrelay経由で完走し、端末側にAnthropicキーが存在しないこと（Keychainダンプ確認）。

## C-3 小物（1日）

- `run_dream_now` の同期ブロッキング解消（tauri asyncコマンド化＋進捗イベント）
- 期限超過 commitments → `ShowNotification`（B-2の効果を流用）。**通知はL1可の非送信アクション**であることをテストで固定

---

# Plan D — L5: Lessons / Patterns（v0: 2〜3日 → v1: +3〜4日）

設計は `layer-completion-designs.md` §5。実装順:

## D-1 スキーマ V16（ロールバック: 3テーブルDROP）

設計書どおり `feedback_events` / `lessons` / `lesson_provenance`。`crates/shogun-memory/src/lessons.rs` 新設:
- `record_feedback(kind, action_kind, scope, scope_ref, before, after)`
- `upsert_lesson(candidate) -> merge or insert`（同scope×同instructionの正規化一致でマージ、`evidence_count+=1`、corroborationでconfidence上昇）
- `decay_and_deactivate(now)`（反証カウント→`active=0`）、`active_lessons(scope_filter, top_k)`
- 全てLinux green。`recompute.rs` の減衰式を流用

## D-2 取得フック（半日）

`apps/desktop/src-tauri/src/approvals` の3コマンド内側に1行ずつ:
- `confirm_send`: 提案本文≠確定本文なら `edit_before_approve`（before/after保存。**ローカルDBのみ・egress禁止はテーブルに触るegressが存在しないことで担保**）、一致なら `approve_unchanged`
- `reject_send`: `reject`
- state解決UI（既存one-tap resolve）: `state_resolve`

## D-3 Learned UI（1日）— ここまでが v0

`App.tsx` Personalization セクションに「Learned」リスト: instruction / 根拠件数 / confidence / ON-OFFトグル / 削除。編集したら手動directiveへ昇格（Shougun.md側へ移す＝`user_config` の既存書き込み経路）。
**v0出荷ライン**: 「修正が記録され、学習候補が見える」。蒸留は未稼働でも、面談で実物を見せられる。

## D-4 蒸留ジョブ（1.5日）

`dreamcycle/plan.rs` の `JobKind` に第7ジョブ `LessonDistillation` 追加（`job_runs` 再開性は既存機構に乗る）。`jobs.rs` ハンドラ:
- **ローカルルール蒸留（既定）**: 未処理 `feedback_events` をscope×action_kindでグルーピングし、機械検出可能なパターンのみ教訓化——(a) 署名/挨拶の一貫削除・追加 (b) 長さの一貫短縮（>30%×3回） (c) 言語切替（返信言語の一貫修正） (d) 敬体/常体。各ルールは「3回以上同方向」で発火、instruction は定型文テンプレから生成
- **Batch蒸留（relay完成後）**: before/afterペア群を分類プロンプトへ（`Classifier` seamと同じ差し替え式）
- 生成→`upsert_lesson`→`lesson_provenance` 記録

**テスト**: 「同方向3回で教訓が立ち、逆方向修正が続くと減衰してactive=0」「2回では立たない」「上限50でLRU休眠」。

## D-5 注入（1日）

- `user_config/directives.rs` の `render_directives` 出力に `## Learned (auto)` セクションを合流（**lessonsはinstructionのみ、confidence Low帯は除外**——Fusionの既存band gateと同じ閾値定数を共有）
- `fusion/assemble.rs`: scope一致（相手person/アプリ/プロジェクト）でtop-k（既定5件）を選びプロンプト予算 `budget.rs` に載せる
- **不変条件テスト**: `lessons_never_change_permission_level`（L3送信がlessonsでL1/L2に降格しないことを型/テストで固定）
- MCP/CLI対称（不変条件6）: Memory APIに `lessons.list` / `lessons.set_active` を追加（`mcp.rs` 13ツール→15）

## D-6 計測（半日）

`shogun metrics` に追加（初期 `measured:false`）: 週次の承認前編集距離中央値 / 無修正承認率 / lesson hit rate（注入されたlessonが編集されなかった率）。**「使うほど賢くなる」の実証データ源**。

---

# 全体スケジュール（1人・直列時）

| 週 | 内容 |
|---|---|
| W1 | B-1〜B-3（actions UI・効果・キュー統合）＋A-0〜A-2をLinux側で並行 |
| W2 | B-4〜B-5（OAuth・Reply Drafter・E2Eゲート）＋A-3実機QA |
| W3前半 | C-1（Brief非劣化）＋D-1〜D-3（L5 v0） |
| W3後半〜W4 | C-2（relay）＋C-3 |
| W4〜W5 | D-4〜D-6（L5 v1）＋A-4（Slackハドル）＋L2 Cold検索（設計書2.1） |

**マイルストーン**:
- **M-a（W2末）**: E2Eデモ成立（見る→押す→承認→実送信）＝投資家デモ最小
- **M-b（W3末）**: Brief本来形＋L5 v0＝「複利ループの記録が始まっている」と言える
- **M-c（W5末）**: relay稼働（課金前提充足）＋L5 v1＋検知拡張

**今日からLinuxで着手可能なもの**: A-0/A-1/A-2/A-4純ロジック、B-3（キュー統合の大半）、C-1のジョブ/テーブル、D-1/D-4/D-5の全部、Cold検索。macOS必須はB-1/B-2/B-4実配線/B-5実機E2E/A-3。
