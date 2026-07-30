# ShogunAI 「Done」項目監査レポート

対象: origin/main (HEAD = b9c1f23)。作業ツリーは origin/main と完全一致（`git diff origin/main HEAD` 差分なし）を確認済み。読み取り専用で実施。

前提として重要な事実: このリポジトリの履歴は 2026-07-28 の **70d0ac7 でスカッシュ・インポート**されており（1コミットに全ツリー同梱）、それ以前の PR の実体は個別コミットとして main 履歴に存在しない。よって「PR がマージ済み」の検証はコミットメッセージではなく **main 上のコード実体**と GitHub API（PR の base ブランチ・merged 状態）で行った。

---

## 1. Issue #47/#69 ウェイトリストの Supabase 保存（apps/website）

### (a) 判定: **done-with-gaps**（コードは main に存在。ただしセキュリティ上の実穴が1件、運用前提の抜けが複数）

- #69 は #47 の重複で `not_planned` クローズ。#47 が `completed`（2026-07-30 クローズ）。
- 実装は Supabase クライアント SDK ではなく **drizzle + postgres 直結**（`DATABASE_URL` = Supabase Session Pooler 想定）。Cloudflare 側シークレット設定・本番動作確認はリポジトリ外のためコードからは検証不能。

### (b) 証拠

- `apps/website/src/db/index.ts:5-11` — `DATABASE_URL` はサーバ側のみ。`NEXT_PUBLIC_` 系にシークレットなし（`.env.example` 参照）。anon/service role キーは**そもそも不使用**（クライアントサイド露出なし）。
- `apps/website/src/app/api/waitlist/signup/route.ts` — origin allowlist(18) → IP レート制限 5/min(21) → 8KB ボディ上限付き JSON パース(`lib/http.ts:32,55-63`) → honeypot(34) → 厳格 email 正規表現+254字上限(`lib/referral.ts:65-69`)。
- `apps/website/src/lib/rate-limit.ts:21-32` — DB 固定窓レートリミッタ（原子的 upsert、サーバレス横断で有効）。
- `apps/website/src/db/schema.ts` — refCode(公開)/statusToken(秘匿) の2トークン分離、IP は salted hash のみ保存(24)。
- リーダーボード/invite-context は nickname or マスク済み email のみ返す（`leaderboard/route.ts`, `invite-context/route.ts`）。

### (c) CLAUDE.md 不変条件との関係

website は CLAUDE.md の対象外（明記）だが、Issue #47 自身の Goal「接続文字列・管理トークンをブラウザ/Git に露出させない」は充足。

### (d) フォローアップ

| 深刻度 | 内容 |
|---|---|
| **High** | **既存メールで signup すると他人の private statusToken/statusUrl がそのまま返る**。`signup/route.ts:40-41` + `lib/service.ts:30-37`（`duplicate: true` でも既存行を返し、route が `row.statusToken` から statusUrl を組み立てて返却）。メールアドレスさえ知っていれば第三者が被害者のステータス閲覧・profile 書き込み（nickname/回答の改竄）が可能＝実質アカウント乗っ取り。重複時は「メールに確認リンクを送った」等のニュートラル応答にし、トークンはメール経由でのみ渡すべき |
| **Med** | `waitlist-auth.ts:27` — `WAITLIST_ALLOWED_ORIGINS` 未設定時に origin チェックが**フェイルオープン**（全許可）。本番で環境変数を入れ忘れると防御が消える。本番ビルドでは未設定を起動エラーにすべき |
| **Med** | Supabase RLS は不使用（Postgres 直結・単一クレデンシャル）。サーバ専用接続なら許容だが、`DATABASE_URL` 漏洩＝全件アクセス。最小権限ロール（participants/rate_limits のみ CRUD）を切ることを推奨 |
| **Low** | `WAITLIST_IP_SALT` 未設定時 `'dev-salt'` にフォールバック（`waitlist-auth.ts:49`）— レインボー可能な弱ハッシュ化 |
| **Low** | `signup/route.ts:43` `console.error('signup error:', e)` — PG の unique violation エラーは**違反値（メールアドレス）を含む**ためサーバログに PII が乗り得る。findByEmail→insert の TOCTOU 競合時は 500 にもなる（`onConflictDoNothing` + 再取得で解消） |
| **Low** | レートリミッタはフェイルオープン（設計判断としてコメント明記あり）＋ `rate_limits` テーブルの掃除ジョブなし（無限成長） |

---

## 2. Issue #61 / PR #91 PostHog DAU/MAU トラッキング

### (a) 判定: **done-with-gaps**（コードは main に完全に存在し設計は良質。ただし本番ビルドでは事実上動かない構成＋オプトアウト到達性の欠陥）

### (b) 証拠

- main のコミット列 f0575b5〜6a3fa24、マージ b9c1f23（PR #91、base=main）。実体確認済み:
  - `crates/shogun-core/src/analytics/mod.rs`（純ロジック+worker）、`analytics/reqwest_transport.rs`（`net` feature、TLS 検証無効化なし）
  - `apps/desktop/src-tauri/src/analytics.rs`（distinct_id 採番・永続化・opt-out コマンド）
  - 発火点: `lib.rs:472-480`（app_opened）、`notch_exec.rs:139-149`（shogun_query_executed）、`connectors.rs:120-126`（context_updated）
- **opt-out のゲート**: `AnalyticsHandle::capture`（mod.rs:139）と worker 受信側（mod.rs:78）の二重チェック、`Arc<AtomicBool>` 共有で即時反映、`analytics.json` に永続化・起動時読込（analytics.rs:146）。概ね健全。
- **キャプチャ内容の非混入**: 全イベントの props は `query_type`/`permission_level`/`outcome`/`source`/`newly_inserted`/`cold_start`/`os`/`app_version`/`plan` のみ。ユーザーテキスト・画面内容は一切なし。**CLAUDE.md テレメトリ規約に適合**。
- **distinct_id**: OS CSPRNG による UUIDv4（analytics.rs:25-36）、`analytics.json` 保存。ハードウェア ID 非使用の匿名 ID で良い。
- **ネットワーク feature ゲート**: `net` feature（shogun-core/Cargo.toml:26、既定 OFF）、desktop 側は features=["net",…] で有効。HTTP クライアントは shogun-core 集約（FR-TR-03 ガードスクリプトも main に存在: `scripts/check-http-egress.py`）。
- **unwrap()**: テストモジュールのみ（`#[allow(clippy::unwrap_used, clippy::expect_used)]` 付き、6a3fa24）。本体コードは `unwrap_or_default`/`ok()` で処理。規約適合。
- **API キー**: PostHog write key（phc_、公開安全）を実行時 env `SHOGUN_POSTHOG_KEY` から取得。シークレットではないので Keychain 不要の判断は妥当（不変条件7非該当）。

### (c) 不変条件違反

- 明確な違反はなし。ただし `AnalyticsToggle.tsx:34-36` の **UI 文言が日本語ハードコード**は「UI文言は英語(v1)・コード分離 i18n-ready」規約違反（他の onboarding 文言は strings.ts 経由なのにここだけ直書き）。

### (d) フォローアップ

| 深刻度 | 内容 |
|---|---|
| **High**(機能) | `SHOGUN_POSTHOG_KEY` は**実行時 env のみ**（analytics.rs:127）。Finder から起動する配布ビルドには env が無く、**本番で analytics は無条件サイレント無効**＝DAU/MAU 計測という Issue の目的が達成されない。`option_env!`（ビルド時埋め込み）か設定ファイル経由に変更要 |
| **Med** | オプトアウト UI（`AnalyticsToggle`）は **Onboarding.tsx にしか置かれていない**。AX 権限付与済みマシンでは onboarding 自体が出ない（`should_show_onboarding`）ため、**トグルに到達する UI が存在しないユーザーが生じる**。設定画面（App.tsx の settings ビュー）にも露出させるべき |
| **Med**(判断確認) | 既定 opt_out=false（**送信ON デフォルト**）。ローカルファースト/プライバシーを看板とする製品として、オーナーの明示サインオフを記録すべき |
| **Low** | worker のタイムアウトフラッシュ（mod.rs:85-88）は opt_out を再確認しない — opt-out 直前 3 秒以内にバッファ済みのイベントは送信される |
| **Low** | analytics egress にはトレーサビリティ記録がない。ユーザーデータを含まないため不変条件3の直接違反ではないが、「送信箇所には必ずトレーサビリティ」の原則に対する例外として docs に明記推奨 |

---

## 3. Issue #56 CI/CD 最適化

### (a) 判定: **fully done**（クローズコメントの主張はすべて main 上で実証確認。軽微な改善余地のみ）

### (b) 証拠（`.github/workflows/ci.yml`、7425bac ほか）

- push を統合ブランチ限定・PR は pull_request イベントのみ → 二重実行解消（ci.yml:13-29）✓
- `concurrency` + `cancel-in-progress: true`（33-35）✓
- desktop ビルド前の `pnpm --filter @shogun-ai/tokens build`（108,130）✓、`packages/**` トリガーパス追加（21,28）✓
- キャッシュ: `Swatinem/rust-cache@v2` + setup-node の pnpm キャッシュ ✓
- 不変条件ガード 3 種（egress/secret/migrations、self-test 付き）が実在し CI で実行（83-90、scripts/ に実体あり）✓
- **secrets 参照ゼロ**（両 workflow とも `secrets.` 不使用）— 誤用なし ✓
- phase0-smoke.yml も concurrency あり、workflow_dispatch + 限定ブランチのみ。

### (c) 不変条件違反: なし

### (d) フォローアップ

| 深刻度 | 内容 |
|---|---|
| **Med** | **apps/website の CI が一切ない**。waitlist（P0 の公開エンドポイント）の typecheck/tests（`apps/website/tests/` は存在する）が push でも PR でも走らない |
| **Low** | ci.yml:17 のレガシー branch `claude/shogunai-ui-lp-lisvsd`、phase0-smoke の trigger branch `claude/shogunai-requirements-prep-nm2tf4` が残存（main 昇格済みなので掃除可） |
| **Low** | GitHub Actions をタグ参照（@v4 等）。サプライチェーン強化には SHA ピン留め推奨 |

---

## 4. Issue #46 / PR #76 Accessibility 権限オンボーディング

### (a) 判定: **fully done**（main 上に完全に存在。品質良好）

### (b) 証拠

- コミット 0bcd410（PR #76、base=main、merged 確認済み）。
- `apps/desktop/src-tauri/src/onboarding.rs`:
  - `ax_trusted_silent()`（axcache.rs:195、非プロンプト）ポーリング + false→true エッジで `accessibility-changed` emit（161-185）。ウィンドウ消滅で watcher 停止、`AtomicBool` で多重起動防止。
  - `open_accessibility_settings`（117-127）: プロンプト版 `ax_trusted()` の副作用で AX リストに登録してから設定ペインを開く — UX 上正しい。
  - disposition は `onboarding.json`（app_data_dir）に completed/skipped のみ永続化（44-92）。**キャプチャ内容ゼロ**。
  - ファネルイベントはローカル `eprintln!` のみ（149-155）— ネットワーク送信なし、不変条件3適合。
  - Accessory アプリでの前面表示レシピ（canJoinAllSpaces|fullScreenAuxiliary + NSFloatingWindowLevel + orderFrontRegardless、229-262）— ノッチパネルと同一の実績ある手法、class swap 回避の理由もコメント化。
  - `SHOGUN_FORCE_ONBOARDING` QA ハッチ（271）は保存済み disposition を汚さない。
- コマンド 5 種は lib.rs:186-192 で登録、起動分岐は lib.rs:387-388。
- **unwrap() ゼロ**（本体）。エラーは全て eprintln + 継続（キャプチャデーモンを落とさない規約適合）。
- フロント `Onboarding.tsx` は `t()`（strings.ts）経由の英語コピー — i18n 規約適合。

### (c) 不変条件違反: なし

### (d) フォローアップ

| 深刻度 | 内容 |
|---|---|
| **Low** | skip 後にゲート機能を触った際の再表示（PR 本文でも follow-up と明記）が未実装 — 離脱ユーザーの回収経路がない |
| **Low** | `onboarding_event(name)` は webview からの任意文字列をそのままログへ（ログ注入。実害は軽微だが enum 化推奨） |
| **Info** | onboarding.rs:6 コメントに中国語混入（「离脱」）— 表記ゆれのみ |

---

## 5. Issue #20 / PR #71「Castle Position」と Issue #21 / PR #66「閉じたピルのドラッグ移動」

### (a) 判定

- **#20 / PR #71: fully done（main に存在）**。PR の head sha 自体は main の祖先ではないが、同内容が 609b4dd で design/product-visual-polish に再適用され、d59a043 で main に統合済み。コード実体を確認。
- **#21 / PR #66: not-actually-on-main（重要）**。PR #66 は **base = `design-system/documentation-node`** にマージされており、そのブランチは main に未統合。PR の中核 `beginPillDrag` は **main の全ツリーに存在しない**（grep 0件、head sha 6b705dc は `git merge-base --is-ancestor` で main の祖先でないことを確認）。main の App.tsx でドラッグできるのは展開時ヘッダーのみ（App.tsx:638 `onMouseDown={beginDrag}`）。閉じたハンドル（App.tsx:582-584）にドラッグハンドラなし。**Issue #21 は completed でクローズされているが、製品(main)にはこの機能がない。**

### (b) 証拠（#20）

- `crates/shogun-core/src/notch/geometry.rs:128-224` — `CastlePosition` enum（6配置、from_key/from_u8 は不正値を Notch にフォールバック）、`castle_origin()` は純関数で **画面外クランプ + 縮退 visible frame ガード**あり（220-222）。単体テストあり。
- `apps/desktop/src-tauri/src/lib.rs:70,74` — lock-free `AtomicU8 CASTLE`（配置パスはメインスレッドで非ブロッキング読取 → **Notch展開100ms SLO への影響は原理的に無視できる設計**。ただし p50/p95 の計測結果は PR に添付されていない）。
- 永続化: `castle.json`（lib.rs:1707-1766、app_data_dir、非シークレットの設定値なので Keychain 不要で適切）。`get/set_castle_position` コマンドで UI/API 対称（不変条件6適合）、set 時に即 re-dock。
- マルチディスプレイ: 全配置パス（`reposition_to_cursor_screen`/`pin_top_centre`/`set_panel_size`/redock、lib.rs:518,662,1098,1780）が**対象スクリーンの visible frame** に対して `castle_origin` を計算 — カーソルのいるディスプレイ基準で一貫。

### (c) 不変条件違反: なし（#20）。#21 は「Done 管理の虚偽状態」が問題。

### (d) フォローアップ

| 深刻度 | 内容 |
|---|---|
| **High** | **Issue #21 を再オープンするか、PR #66 の内容を main に移植する**。現状 main のユーザーは閉じたピルを動かせない。移植時は 4 コミット（6b705dc まで）の cherry-pick で足りるはずだが、base ブランチが design-system 系のため衝突確認要 |
| **Med** | 移植時の設計衝突: PR #66 のドラッグ位置は**永続化されず**、かつ main 側の Castle Position の redock パス（summon・castle 変更・リサイズ）がドラッグ位置を **castle_origin に強制的に戻す**。「ドラッグ位置 vs Castle 位置」の優先規則を決めてから統合すべき |
| **Low** | #20 は SLO 計測結果（p50/p95）が PR に貼られていない（CLAUDE.md「レイテンシに影響する変更は計測してからマージ」）。純関数+atomic なので実害は薄いが規約上は計測添付要 |

---

## 6. Issue #63 コンテキスト圧縮（PR #73 / #75 / #77 / #83）

### (a) 判定: **done-with-gaps**（4 PR とも main にマージ済みでコード実体・主張内容とも検証OK。ただし機能は既定 OFF の休眠状態で、Issue が open のままなのは正しい）

### (b) 証拠

- マージコミット: 85c8fe5(#73) / 1a41f6a(#75) / efca5d0(#77) / 469bbae(#83)。すべて main 祖先。
- **予算ガード（#83 の主張どおり）**: `crates/shogun-core/src/daemon.rs:88` `COMPRESS_BUDGET_MS = 50`。ガードは**支配的コストである evidence 検索直後**（949-951）と compress 直前（983-985）の 2 箇所、超過時は `raw_fallback`（1025-）で raw 側にも対称に計測記録（AB 対称化、bb82d4e の主張とも整合）。
- **N+1 解消（#83）**: セッション毎の個別クエリを `session_summaries_for(&session_ids)` の 1 バッチに置換（daemon.rs:966, 1278）。thread_key を同時返却し、**同一会話の thread summary と session summary の重複投入を dedup**（テストあり）。`assemble_evidence` 抽出で state クエリ二重実行も解消（912）。
- **citation 復元（#77 = e592f76）**: `event_id → &Evidence` の HashMap で `BlockRef::Event(id)` の ts/source/title を復元（daemon.rs:990-1003）。テスト `compressed_evidence_preserves_source_and_title` あり。fact ブロックへの実 state id/table 付与（128d3e2）も main に存在。
- **本番配線（#75）**: チャット経路が `db.compression_config()` で分岐（`apps/desktop/src-tauri/src/inline_source.rs:880-892`）、`Db::with_compression_config` builder（daemon.rs:279）。**ただし有効化は `SHOGUN_COMPRESSION=1` 環境変数のみ**（lib.rs:1897-1906、コメントに「既定 off・設定UIは次周」と正直に明記）。「配線済みだが休眠」が正確な状態。
- **キー分離（不変条件5）**: 適合。要約の実体は v1 では `LocalExtractiveSummarizer`（**LLM 非使用・ローカル抽出**、dreamcycle/jobs.rs:59-66）。`Summarizer`/`Classifier` seam に「Batch/Select-KK 側のみが差し込まれる invariant-5 境界」と明記（jobs.rs:6-10,47-50）。チャット側（BYOK）は圧縮済みコンテキストを消費するだけで、圧縮のために BYOK を要約に使う逆転はない。
- **計測のプライバシー**: `compression_metrics`（V11 マイグレーション実在: `crates/shogun-memory/src/migrations/V11__compression_metrics.sql`）は `query_hash` のみ保存・本文なし（daemon.rs:1082-1097、テスト `record_compression_metric_persists_hash_not_text`）。「テレメトリにユーザーテキストを含めない」規約適合。

### (c) 不変条件違反: なし

### (d) フォローアップ

| 深刻度 | 内容 |
|---|---|
| **Med** | 有効化手段が env 変数のみで一般ユーザーには実質未提供。設定 UI + 段階ロールアウト（AB）が終わるまで Issue #63 を閉じないこと（現状 open は正しい）。Done 報告時は「main にあるがデフォルト無効」と明記すべき |
| **Low** | `SHOGUN_COMPRESSION_BUDGET` で予算を上書き可能だが下限/上限バリデーションなし（極端値で SLO 逸脱の余地） |
| **Low** | `compression_metrics` の保持期間/掃除ポリシー未定義（ts インデックスはあり） |

---

## 総括

| # | 項目 | 判定 | 最重要指摘 |
|---|---|---|---|
| 1 | #47 waitlist→Supabase | done-with-gaps | **High**: 既存メール再signupで他人のstatusToken漏洩 |
| 2 | #61/#91 PostHog | done-with-gaps | **High(機能)**: 本番ビルドでキー未注入→計測が動かない / **Med**: opt-out到達不能ユーザー |
| 3 | #56 CI/CD | fully done | Med: website の CI 不在 |
| 4 | #46/#76 AXオンボーディング | fully done | 軽微のみ |
| 5 | #20/#71 Castle Position | fully done | Low: SLO計測未添付 |
| 5 | #21/#66 ピルドラッグ | **not-actually-on-main** | **High**: 間違ったブランチにマージされ main 不在のまま Issue クローズ |
| 6 | #63 圧縮 #73/75/77/83 | done-with-gaps | Med: 既定OFFの休眠（Issue open 継続は正しい） |

CLAUDE.md 不変条件の**明確な違反は検出されず**（キー分離・unwrap・テレメトリ内容・secrets 保存はいずれも適合）。規約レベルでは AnalyticsToggle の日本語直書き（UI英語/i18n 規約）と、レイテンシ関連 PR への計測未添付が該当。プロセス上の最大の問題は **#21 の「Done 判定と main の実体の乖離」**で、スカッシュ・インポート以前の design-system 系ブランチへのマージが main に届いていない同種の取りこぼしが他にもないか、`git branch -r --no-merged origin/main` の棚卸しを推奨する。
