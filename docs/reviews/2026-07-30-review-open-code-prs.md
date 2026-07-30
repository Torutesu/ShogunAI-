# ShogunAI オープンPRレビュー（2026-07-30）

対象: origin/main = `b9c1f23`（#91 PostHog DAU/MAU マージ済み）。
各PRは実 diff（`git diff origin/main...origin/<branch>` / `git merge-tree`）で確認した。

## 総括

| PR | ブランチ | 判定 | 一言 |
|---|---|---|---|
| #92 push-to-talk | `feat/issue-44-push-to-talk` | **approve-with-nits**（要リベース） | 不変条件2は厳密に遵守。品質高い。main の #91 と `analytics.rs` が add/add 衝突 |
| #90 onboarding | `claude/ui-commerce-onboarding-za0ns8` | **request-changes（ブロッカー）** | main と **共通祖先なし**。マージすると会議ノート・音声・analytics 等 約2.6万行を巻き戻す。オンボーディング実装自体は良質 → main 上へ移植し直すこと |
| #78 Privacy & Security | `feat/issue-28-privacy-security` | **request-changes** | Keychain/削除/マスキングの実装は概ね良いが、マージ済み #91 と**アナリティクス二重実装・デフォルト矛盾**。opt-in ゲートが dead code |
| #74 Shougun.md | `feat/issue-41-shougun-md` | **approve-with-nits**（要リベース） | 設計は堅実。chat 側の directives 配置・サイズ上限なし・UI文言ハードコードを直したい |

推奨マージ順: **#92 → #74 → #78 → #90（作り直し）**（理由は末尾）。

---

## PR #92 「feat: push-to-talk voice interaction (#44)」

**判定: approve-with-nits（現 main へのリベース必須）**

### 不変条件チェック（すべて合格）

- **不変条件2（音声を保存しない）: 合格。** 音声経路を全行確認した。
  - `apps/desktop/src-tauri/src/ptt_lane.rs` — マイクのみを開き（system tap なし）、波形は `Worker` の RAM バッファのみ。DB 書き込みなし。
  - `crates/shogun-core/src/ptt/buffer_sink.rs` — sink が受け取るのは文字起こし後テキストのみ（`text: String` フィールドだけ）。`take()`/`discard()` でセッション毎に消える。
  - diff 全体に音声のファイル書き出し・録音生成コードは存在しない。ディスクに書くのは `ptt.json`（enabled/hold_key のみ、秘密なし）だけ。
  - `Info.plist` の `NSMicrophoneUsageDescription` が「audio is never saved」を明言 — 良い。
- **不変条件5（キー分離）: 合格。** ストリーミングは `AnthropicAgentClient`（`ByokKey`）経由。`crates/shogun-core/src/llm/anthropic.rs` の `complete_streaming` は送信**前**に digest-only の TraceRecord を記録（不変条件3も合格）。
- **不変条件7 / ログ規約: 合格。** `transport.rs` の `HttpRequest::Debug` が `x-api-key`/`authorization` をredact。発話・文字起こし・応答はどのログにも出ない（`analytics.rs` は時間と enum コードのみ、「文字数すら送らない」と明記）。`SHOGUN_PTT_DEBUG` の診断出力も flagsChanged（修飾キー）の keyCode/flags のみで文字キー内容は出ない。
- **unwrap 禁止: 合格。** diff 中の `unwrap()` 12件は全てテスト/ドキュメント内。本体は `unwrap_or`/`map_err`/lock毒化時の早期 return で統一。
- **UI文言: 合格。** 英語、`strings.ts` 集約（設定画面）。絵文字なし。

### 指摘（重要度順）

1. **[High/マージブロッカー] 現 main と衝突 — リベース必須。**
   `git merge-tree origin/main origin/feat/issue-44-push-to-talk` で確認:
   - `apps/desktop/src-tauri/src/analytics.rs` — **add/add 衝突**。main には #91 の PostHog アダプタ（`analytics.json` の distinct_id + opt_out）が既に同パスで存在し、本PRは同パスに PTT 計測モジュール（eprintln のみ）を新規追加している。本PRの analytics.rs 冒頭コメント「PostHog は PR #91 にあり main 未マージ」「opt-out の仕組み自体が無い」は**既に事実と異なる**。リベース時は PTT 計測を main の analytics.rs に統合し、実送信化する際は #91 の opt_out ゲートを必ず通すこと。
   - `apps/desktop/src-tauri/src/lib.rs` — content 衝突（コマンド登録部）。
2. **[Low] エラー文言が Rust 側 `ptt::fail_message` にハードコード。** 英語かつ1関数集約なので実害は小さいが、`strings.ts` と二重管理になる。i18n 時の移設先をコメントで明示済みなので nit 扱い。
3. **[Low] `docs/superpowers/plans/2026-07-30-push-to-talk.md`（3,369行）をリポジトリにコミット。** 履歴ノイズ。残すなら意図的に。
4. **[Low] `save_settings` の `std::fs::write` が非アトミック。** 電源断で ptt.json が壊れても `load_settings` が enabled=false へフェイルクローズするので安全側だが、tmp+rename が望ましい。
5. **[Info] SLO 対応は良好。** パネルは起動時生成（押下時生成を回避）、hold→パネル実測を `SloRegister.record_expand_ms` に記録、初トークンは `record_first_token_ms`（SLO-03）で計測、SSE は真の逐次デコード（`sse.rs` + UTF-8 境界キャリーは `transport.rs` 側）。アイドル CPU は NSEvent モニタ（イベント駆動）のみで、20ms ポーリングスレッドはマイクが開いている間だけ。**計測コード同梱の規約も満たす。**
6. **[Info] 状態機械の設計が堅い。** 「マイクを開く入力は HoldStart のみ / Recording から出る全辺が Stop か Discard を伴う」をテストで固定。左右⌘の device-dependent ビット判定（`hold_monitor.rs` の NX_DEVICERCMDKEYMASK）、poison→完全リリースまで再武装しない設計、ENABLED=false でも HoldEnd/Cancel は通す（マイク閉じ漏れ防止）— いずれも実機の落とし穴を正しく塞いでいる。

---

## PR #90 「feat(onboarding): first-run flow (issue #6)」

**判定: request-changes（このままのマージは絶対不可）**

### [Blocker] ブランチが main と無関係な履歴を持つ

- `git merge-base origin/main origin/claude/ui-commerce-onboarding-za0ns8` → **no merge base**。469bbae も b9c1f23 もこのブランチの履歴に含まれない（`git branch --contains` で確認）。ブランチは LP コミットを root とする独立した259コミットの履歴で、`git diff main...branch`（three-dot）自体が不可能 = **PRとしてレビュー可能な差分が定義できない**。
- main との tree 差分は **159ファイル / +4,964 / −28,121**。このままマージ（あるいは force 的に樹を採用）すると、main にマージ済みの以下を**全部削除・巻き戻す**:
  - 会議ノート一式（`meeting.rs` / `meeting_recap.rs` / `mic.rs` / `crates/shogun-core/src/audio/*` / migrations **V7〜V11**（sessions, session_notes, transcript_segments, meeting_recaps, compression_metrics）/ `session.rs` ほか） — Issue #7 の成果物
  - #61/#91 の analytics（`analytics.rs`, `crates/shogun-core/src/analytics/*`, `AnalyticsToggle.tsx`）
  - Full UI（`fullui.rs`, `src/fullui/*`）、`metrics.rs`、`model_fetch.rs`、main 側 onboarding（#46）
  - **CLAUDE.md 自体が旧版へ後退**: 不変条件2から音声条項が消え、「Gmail 全面 Composio（2026-07 決定）」の記録が消えて旧「読み取り=公式MCP直結」に戻り、「会議ノートは全プラン」の項も消える。**プロジェクトの記録済み意思決定の逆行**であり、それ単体でブロッカー。

### オンボーディング実装そのものの評価（移植する価値はある）

末尾の数コミット（`22cc779` design/frontend, `fc40b3b` shortcuts, `814b1a8` Rust IPC）は品質が高い:

- **プラン判定**: `onboarding.json`（Rust所有、invariant 1 準拠）に `plan` を**意図として記録するだけ**で、権利ゲーティング自体は未実装（コメントで「gating lives in the Rust core, not here」と明示）。webview 側だけのゲーティングという違反はない。ただし**実際のプラン強制は依然どこにも無い**ので、課金実装時のフォローアップとして明記しておくこと。
- トライアル開始 = オンボーディング完了時に一度だけ刻印、再実行で再スタートしない（`trial_started_at` の `prev.or(...)` 実装＋テスト）— 仕様どおり。
- 権限フロー: `ax_trusted_silent`（非プロンプト）を1.5sポーリング、プロンプトはボタンからの一回のみ — システムダイアログ連発を正しく回避。
- draft-stop: 既定ON・曖昧入力はONへフェイルセーフ（不変条件4整合）。
- UI: プランは Standard/Pro のみ（Free なし — CLAUDE.md の2026-07-26 オーナー判断に整合）、絵文字は ⚔ のみ、文言は strings カタログ経由。exclusion カテゴリは live policy から取得しハードコードしない — 良い。
- MCP/CLI 対称（invariant 6）: `Tool::DeviceOnboardingGet` を MCP/REST/CLI に追加（contract only）。

### 要求

1. 現 main から新ブランチを切り、オンボーディング関連コミットを**チェリーピック/再適用**して PR を作り直す（main 既存の `onboarding.rs`(#46) との統合方針も明記）。本ブランチ・本PRはクローズ。
2. CLAUDE.md・既存機能への一切の削除・巻き戻しを diff に含めない。

---

## PR #78 「feat: Privacy & Security (#28)」

**判定: request-changes**（主因は main 済み #91 との意味的衝突。それ以外は良い実装）

### 良い点（チェック項目の結果）

- **BYOK は Keychain のみ: 合格。** 鍵の平文が config/DB/ログに出る経路は diff に無い。`byok_key_last4()` は末尾4文字だけを webview へ渡し、4文字未満の鍵は全マスク（`inline_source.rs`）。`Secret::last4` は main 既存の実装を利用。
- **削除の実装は誠実。** `crates/shogun-memory/src/maintenance.rs`:
  - `delete_since`: state_provenance → event_vec → cold_embeddings → event_log → sessions 子テーブル（session_notes/transcript_segments/meeting_recaps）→ dangling `event_log.session_id` の NULL 化 → sessions → traceability → 孤児 state 掃除、を**単一トランザクション**で。FK 失敗でロールバックする穴（録音済み会議があると削除不能になる）をテストで塞いでいる。境界 `ts >= cutoff` もテスト固定。FTS は AD trigger で同期（テストあり）。
  - `delete_all_and_account`: DB 全行 + 全 BYOK プロバイダ鍵 + 全 OAuth tokenset + Composio 鍵を Keychain から削除。`memory-db-key` を残す判断（DB を開けなくしない）も妥当で文書化済み。
  - ベクタテーブル（event_vec / cold_embeddings）: **カバー済み**。
- **マスキング基盤**: `crates/shogun-redact`（純粋・依存ゼロ・Cow 返しでノーマッチ時ゼロアロケーション）、issuer prefix + ラベル方式で誤爆回避方針も明確。`elog!` マクロ（`log_redact.rs`）。
- UI: 全量削除は「Type DELETE」の二段確認、二重発火ガードあり。文言は strings.ts 集約・英語。

### 指摘（重要度順）

1. **[High] マージ済み #91 とのアナリティクス二重実装・方針矛盾。** テキスト上は clean merge（merge-tree 確認済み）だが意味的に衝突する:
   - main(#91): `analytics.json` の `opt_out`（**既定 false = 送信ON**）＋ PostHog 実送信 ＋ `AnalyticsToggle.tsx`。
   - 本PR: `privacy.json` の `analytics_enabled`（**既定 OFF = opt-in**）＋独自トグル＋「全送信が必ず通るゲート」を謳う `analytics_enabled()` — しかし **`#[allow(dead_code)]` で、main の PostHog 送信経路はこのゲートを一切参照しない**。
   - このままマージすると: トグルが2つ並び、既定が互いに矛盾し、本PRの UI 文言「Off by default. When on, ...」は**実際の PostHog 送信について嘘**になる（PostHog は opt_out=false のまま送信し続ける）。
   - 要求: 単一の設定ソースに統合し（#28 の要件は opt-in。#91 の既定 opt_out=false とどちらを正とするかは**オーナー判断を明示的に取る**）、PostHog の送信ゲートをその1点に接続すること。
2. **[Medium] `delete_all_and_account` が #91 の `analytics.json`（distinct_id）を消さない。**「Delete everything & account」後も匿名IDが残り、以後のテレメトリが同一IDで継続する。アカウント削除の意味論として distinct_id 再生成（または削除）と送信停止を含めるべき。
3. **[Medium] 削除後の物理残渣: `PRAGMA wal_checkpoint(TRUNCATE)` / VACUUM が無い。** 削除行が WAL とフリーページに残る。SQLCipher（bundled-sqlcipher）でファイルは暗号化されているため生平文流出ではないが、`memory-db-key` は Keychain に**残す設計**なので鍵保持者にはページ残渣が復元可能。delete_since / delete_all 後に checkpoint + （incremental_）vacuum を推奨。
4. **[Low] ログマスキングの適用範囲が1箇所のみ。** `elog!` の実使用は `model_asset.rs` の1行だけ。基盤としては良いが、PR タイトルの「log masking」は既存の eprintln 網羅ではない点を PR 本文で明確化し、段階適用の tracking issue を切ること。
5. **[Low] コメントの自己矛盾。** `strings.ts` の「no last-4: the backend deliberately hands out no key material」が、最終コミット dd2c586 で追加した `byok_key_last4` と矛盾。コメント更新を。
6. **[Low] `delete_since` は `threads` の派生サマリを次回 Dream Cycle まで残す**（`threads: 0` と文書化済み）。挙動としては許容だが、削除UIの文言で「派生サマリは夜間に再構成される」旨を伝えるのが誠実。
7. **[Info] Anthropic データ利用ノート**（anthropic.rs の docコメント）: 「no-train はベンダー既定であり当方のオプトアウトではない」と正確に記述 — UI 文言（policyNotTrained）も虚偽にならない範囲。良い。

ベース鮮度: merge-base は `1a41f6a`（古い）が、途中で origin/main をマージ済みで現 main とはテキスト衝突なし。ただし上記1の理由で**論理的なリベース（#91 との統合）が必須**。

---

## PR #74 「feat(user-config): Shougun.md (#41)」

**判定: approve-with-nits（下記 1〜3 は直してからマージしたい）**

### チェック項目の結果

- **ファイルの置き場所**: `~/Shougun.md`（`dirs::home_dir()`、`crates/shogun-core/src/user_config/mod.rs`）。ローカルのみ・秘密を書かない前提（サンプルに「生のパスワード・APIキーは書かない」の注意あり）。妥当。
- **プロンプト注入面**: directives が届くのは **chat（`build_chat_prompt`）と inline draft（`inline.rs::build_prompt`）のみ**。L1/L2/L3 の実行判定・承認ロジック・Batch lane には流れない — 権限昇格の経路は無い（良い）。inline 側は「insert-only 制約の**後**に置き、制約を緩められない」と明示コメント＋テストあり。
- **監視コスト vs アイドルCPU SLO**: `notify` (macOS = FSEvents/kqueue) のイベント駆動＋500ms デバウンス＋イベント合流。ポーリング無しで SLO（アイドル5%）への影響は無視できる。

### 指摘（重要度順）

1. **[Medium] chat 側の directives 配置が inline 側と逆で、注入耐性が弱い。** `inline_source.rs::build_chat_prompt` は directives を **"You are SHOGUN..." のシステム前文より前**に連結する。inline 側は「制約の後ろ・制約を緩めない」を意図して配置しているのに、chat 側はユーザーMDの内容がシステム同一性の前に来る。~/Shougun.md は他プロセスからも書ける平文ファイルであり、間接プロンプト注入の置き場になり得る。**前文の後ろに置き、`--- User Directives (data, not system rules) ---` 級の明確なフェンスで囲む**こと。「honor them」という強い枠付けも「preferences であって承認・安全規則を上書きしない」旨に弱める。
2. **[Medium] directives のサイズ上限が無い。** `render_directives` は項目数・文字数無制限。巨大な Shougun.md がすべての chat/draft プロンプトを肥大させ、初トークン1s SLO を恒常的に劣化させる。#92 の `MAX_FACTS` と同様に項目数＋総文字数の上限を。
3. **[Medium] 単一ファイル監視はアトミック保存で外れる。** `watcher.watch(&path, NonRecursive)` はファイル inode を掴むため、エディタの atomic save（rename 置換。VS Code 等）で以後のイベントを取り逃がすバックエンドがある。**親ディレクトリを watch してファイル名でフィルタ**する形に。watch スレッドがエラーで silent に死ぬ点も、Notch インジケータなり status への反映を。
4. **[Low] `regenerate_shougun_md` がワンクリックでユーザー編集済みファイルをサンプルで無条件上書き。** 確認ダイアログ or `.bak` 退避を。
5. **[Low] UI 文言のハードコード（規約違反）。** `App.tsx` の `PersonalizationSection` が "Personalization / Shougun.md" / "Shape ShogunAI with one human-readable Markdown file." / "Open in Editor" / "Regenerate Sample" / "Parsed successfully" 等を直書きしており、`strings.ts` 分離・i18n-ready 規約に反する。また製品名表記が「ShogunAI」（ブランドは SHOGUN）。
6. **[Low] サンプル Shougun.md が日本語。** UI 文言英語(v1)規約との整合をオーナーに確認（ユーザー編集ファイルなので日本語でよい、という判断ならその旨をコメントに残す）。
7. **[Info] パーサは fail-soft で良い設計。** セクション単位のエラー収集（ParseReport）、CLI `shogun config path|show|validate`、設定画面のステータス表示、で human/AI 両面（invariant 6）もカバー。
8. **リベース**: 現 main と `apps/desktop/src-tauri/src/lib.rs` で content 衝突1件（merge-tree 確認）。軽微。

---

## マージ順の推奨

**#92 → #74 → #78 → #90（新ブランチで再構成）**

1. **#92 を最初に。** ブロッカーが「機械的なリベース＋analytics.rs 統合」のみで、設計判断は不要。inline_source.rs / strings.ts / App.tsx（設定セクション追加）に触る他PRの基準面を早く確定させる。
2. **#74 を次に。** 修正量が小さい（配置・上限・watch 方式・文言）。#92 マージ後に inline_source.rs / lib.rs / App.tsx をリベース（#92 の `complete_streaming_blocking` と #74 の `build_prompt(…, directives)` は同居可能で衝突は表面的）。なお #92 の PTT プロンプト（`ptt/prompt.rs`）にも将来的に directives を配線するかは別 issue に。
3. **#78 はオーナー判断待ちがあるため3番目。** アナリティクス opt-in/opt-out の一本化は #91 の実装に対する設計決定が要る。遅らせることで #92 の PTT 計測（実送信化するなら同じゲートを通す）とも一括で整合させられる。
4. **#90 は最後、かつ現ブランチのままではマージしない。** 現 main（上記3本マージ後）から新ブランチを切り、オンボーディングのコミットのみ移植して新PRに。プラン権利の実ゲーティング（Rustコア）は別 issue として明示的に残す。

## CLAUDE.md 不変条件違反の明示ラベル（総覧）

- **#92: 違反なし**（不変条件2・3・5・7、unwrap禁止、テレメトリ規約、UI文言規約すべて適合を diff で確認）。
- **#90: 「違反コード追加」ではなく「マージ行為が違反を引き起こす」型のブロッカー** — マージすると不変条件2の音声条項・Gmail Composio 決定・会議ノート全プラン提供の記録を CLAUDE.md ごと巻き戻し、マージ済み実装（V7〜V11 マイグレーション等 = 「後方互換を破るマイグレーション禁止」にも抵触する削除）を破壊する。
- **#78: 違反なし（鍵は Keychain のみ、ログに内容なし）** — ただし High-1 は「UI 文言が実挙動と乖離する」品質問題で、放置すればプライバシー開示の虚偽になる。
- **#74: 軽微な規約違反1件** — UI 文言のコード直書き（文言分離・i18n-ready 規約違反、App.tsx PersonalizationSection）。不変条件レベルの違反はなし。
