# 仕様と実装の乖離監査（2026-08-20）

**目的**: `CLAUDE.md` / `docs/requirements-v1.0.md` / `README.md` の記述と、実際に入っているコードの差分を洗い、**どちらを正にしたか**を記録する。以後、同種のズレはこの表に追記して解消する。

**調査方法**: 記述側は上記3文書、実装側は `crates/` / `apps/desktop` / `apps/api` / `apps/website` のコードと `docs/feature-status.csv`・`docs/connector-adapter-plan.md`・`todo.md`。判定は「コードに存在するもの」を事実とした。

**凡例** — 対応: `docs更新`＝実装に合わせて文書を直した／`要判断`＝オーナーの決定が要る（文書は現状を明示するに留めた）／`一致`＝差分なし。

---

## 1. 乖離一覧

| # | 項目 | 文書の記述 | 実装の事実（根拠） | 対応 |
|---|---|---|---|---|
| 1 | **開発フェーズ** | CLAUDE.md「Phase 0（今ここ）…4つの問いに答えるまで本実装を始めない」／requirements ステータス「Draft for Phase 1（Phase 0 Go判定待ち）」「Phase 0の間、本書の要件を先行実装してはならない」 | Phase 1 の純ロジックは端から端まで実装・テスト済み（`crates/` 全体、`docs/feature-status.csv`、`docs/phase1-findings.md`）。未了は**実機（Apple Silicon）検証**とUI周りのみ | docs更新 |
| 2 | **crate構成** | CLAUDE.md／requirements §5.1 とも6 crate（core / memory / fusion / agents / mcp / cli） | 実際は10。追加は `shogun-integrations`（第1層アダプタ＋Composio）／`shogun-license`（ライセンス検証）／`shogun-redact`（秘匿値マスク）／`spike-harness`（Phase 0スパイク） | docs更新 |
| 3 | **`apps/api`** | CLAUDE.md「スタンドアロンAPI予定地（本ファイルの対象外）」／README「scaffold」 | Batch relay が実装済み（`POST /v1/batch`・`GET /v1/batch/:id`、`src/auth.ts` のライセンス検証、rate limit、usage 計上）。会議ASRの短命トークン発行の契約も同パッケージに置かれている（`README-asr-proxy.md`、デバイス側は `shogun_core::audio::asr::deepgram::EphemeralTokenAuth`）。**不変条件5（キー分離）と2026-08-05のASR例外の実体がここにある**ため「対象外」にできない | docs更新 |
| 4 | **Gmail の経路** | CLAUDE.md【2026-07 決定】「Gmail は読み取り・ドラフト・送信の**すべて**を Composio 経由にする」 | 実装は**当初モデルのまま**: 読み取り／ドラフトは Google 公式リモートMCP直結（`gmailmcp.googleapis.com`、scope は `gmail.readonly` + `gmail.compose`。`crates/shogun-integrations/src/endpoints.rs`）、Composio は**送信のみ**（`crates/shogun-mcp/src/scope.rs` の `RequiresComposio`、`crates/shogun-integrations/src/composio.rs`）。requirements §6.9/§6.10 と `docs/connector-adapter-plan.md` も当初モデルで書かれている | **要判断**（§2-A） |
| 5 | **Wave 1 の範囲** | requirements FR-INT-03「Wave 1 = Gmail + Google Calendar」／CLAUDE.md も同様 | Wave 1 に **Google Drive** を含む（2026-07-23 のプロダクト判断。`crates/shogun-mcp/src/scope.rs`、`docs/feature-status.csv` US-INT-07）。Google Docs/Sheets は独立サービスにせず Drive 経由で読む | docs更新 |
| 6 | **テレメトリ既定** | requirements NFR-TEL-01「既定OFF（オプトイン）」 | CLAUDE.md の 2026-08-08 統合決定どおり **PostHog オプトアウト方式（既定ON）**。永続状態は `analytics.json` の `opt_out` のみで既定 `false`（`apps/desktop/src-tauri/src/analytics.rs`、`crates/shogun-core/src/analytics/`） | docs更新（CLAUDE.mdが上位文書） |
| 7 | **ライセンストークンの保存先** | requirements FR-BIL-08「署名付きライセンストークンをKeychainに保存」／NFR-SEC-01 も Keychain 限定に列挙 | CLAUDE.md の 2026-08-13 例外どおり `billing.json`（app-data、平文）。読み出しは `shogun_mcp::plan_source` / `shogun_core::license_client::cached_license_token`。**ライセンスキー本体は引き続きKeychainのみ** | docs更新 |
| 8 | **BYOKプロバイダ** | requirements ADR-002「v1はAnthropicのみ。実装は1つ」 | Anthropic に加え **OpenAI互換**が実装済み（`crates/shogun-core/src/llm/openai_compat.rs`）。さらに **サブスク委譲**（`llm/subscription.rs`、Issue #110）が Agent lane の第一選択 | docs更新 |
| 9 | **Proの資格情報要件** | requirements ADR-003「Proの推論はBYOK必須」 | 現行は **サブスク委譲 または BYOK**（CLAUDE.md プラン構成と一致、実装も両経路あり） | docs更新 |
| 10 | **Memory API のツール数** | requirements FR-API-02「v1の公開ツール（最小セット）」9行 | 実装は **28ツール**（`crates/shogun-mcp/src/memory_api.rs`）。表に無いもの: `memory.get_context_pack` / `memory.get_wrap` / `actions.status` / `device.onboarding.get` / `lessons.list` / `lessons.set_active` / `visual_recall.*`（7）/ `profile.whoami` / `profile.set` | docs更新 |
| 11 | **Memory API のトランスポート** | requirements FR-API-01「MCPサーバー（stdio + Streamable HTTP/localhost）」 | 実装・運用は **stdio のみ**。Streamable HTTP は保留（`todo.md`「Streamable HTTP MCP on 127.0.0.1 — park」）。REST（127.0.0.1:7464）は実装済み | docs更新 |
| 12 | **Memory API の有効化ゲート** | requirements §6.11 に記載なし（Pro機能とだけ） | `memory_api.json` の Enable トグルで fail-closed、現状は**ソフトPro gate**（トライアル中も有効化可）。ハードゲートは Stripe WP5.1 待ち（`docs/memory-api-mcp.md`、`todo.md`） | docs更新 |
| 13 | **DB暗号化** | CLAUDE.md 技術スタックに暗号化の記載なし（SQLite/rusqlite/sqlite-vec/WAL/FTS5/refinery のみ） | **SQLCipher** で暗号化済み（`rusqlite` の `bundled-sqlcipher-vendored-openssl`、`crates/shogun-memory/src/lib.rs` の `PRAGMA key`、`docs/feature-status.csv` US-MEM-05） | docs更新 |
| 14 | **website の紹介プログラム** | README「portable referral/waitlist engine (Drizzle + Postgres)」 | 紹介系APIは**全て404化して撤去**（`/api/waitlist/{rank,status,profile,leaderboard,invite-context}`）。現行のウェイトリストは**メール1フィールドのみ**（D1 の `waitlist_email_capture`）。`src/lib/referral.ts` のロジックと i18n 辞書の `hero.invitedBy` / `invitedTier` が残骸として残っている | docs更新（コード側の残骸整理は別タスク） |
| 15 | **desktop アプリの完成度** | README「desktop / api — scaffolds」 | `apps/desktop` は Notch パネル・Full UI・オンボーディング・会議オーバーレイ・音声・Visual recall まで実装済み（`apps/desktop/src`、`src-tauri`） | docs更新 |
| 16 | Visual recall（2026-08-02 例外） | CLAUDE.md・requirements とも記載あり | 実装あり（`screen_frames` テーブル、`crates/shogun-core/src/capture/visual_recall.rs`） | 一致 |
| 17 | 会議ASR（2026-08-05 例外） | CLAUDE.md・requirements FR-MT-13 とも Deepgram 既定に更新済み | 実装あり（`crates/shogun-core/src/audio/asr/deepgram.rs`） | 一致 |


## 1.5 追加監査（2026-08-20 第2巡・外向けコピー作成中に発覚）

外向けコピーを書くために「いま何が保存され、何が外に出るか」を洗い直したところ、**要件定義が実装に追いついていない領域**がさらに見つかった。18〜21 は外部への説明文に直結するため、優先度が高い。

| # | 項目 | 文書の記述 | 実装の事実（根拠） | 対応 |
|---|---|---|---|---|
| 18 | **Visual recall の保持期間** | CLAUDE.md 不変条件2の例外「圧縮 JPEG として暗号化済みメモリ DB に**最大 72 時間**だけ保持」 | 既定は**3日（=72時間）**だが、**ユーザーが 1〜3,650 日（最長10年）まで設定できる**（`crates/shogun-core/src/capture/visual_recall.rs`: `DEFAULT_RETENTION_DAYS = 3` / `PRESET_RETENTION_DAYS = [1..7]` / `MAX_CUSTOM_RETENTION_DAYS = 3_650`）。「最大72時間」は**もはや上限ではない** | **要判断**（§2-C） |
| 19 | **音声のクラウド送信範囲** | CLAUDE.md 2026-08-05 例外は「**Meeting notes の既定 ASR**」に限定して Deepgram を許可 | **音声入力（hold-to-talk, Issue #44）も Deepgram live WS を第一経路にしている**（`apps/desktop/src-tauri/src/voice_lane.rs`。Whisper はフォールバック）。ディスクには書かないが、**会議以外の音声も外部へ出ている**。例外の文言がこの経路を含んでいない | **要判断**（§2-D） |
| 20 | **音声入力（push-to-talk）** | requirements v1.0 に**記述ゼロ**（"PTT" / "音声入力" とも 0 件） | 実装済み。ショートカット長押し中だけマイクを開き、離した瞬間に文字起こし＋文脈結合＋応答（`voice_lane.rs` / `voice_shortcut.rs` / `voice_session.rs` / `VoiceOverlay.tsx`）。設計は `docs/push-to-talk-voice-design.md` | docs更新（要件化） |
| 21 | **Scribe（その場書き換え）** | requirements v1.0 に**記述ゼロ** | 実装済み。任意アプリの編集可能フィールドを AX で掴み、指示に沿って**その場で書き換える**（`apps/desktop/src-tauri/src/scribe.rs` / `ScribeOverlay.tsx`）。保護スパンの保存検証つき | docs更新（要件化） |
| 22 | **feature-status.csv の網羅性** | 「機能単位の状況はこれを正とする」と本監査 §1 で位置付けた | 音声入力・Scribe・Visual recall の**行が存在しない**（Meeting のエラー行のみ）。最終更新は 2026-08-05 | docs更新（行を追加するまで「正」と呼べない） |
| 23 | **設計だけで未着手のもの** | — | Evening Wrap / 朝夜サマリー配達（`docs/daily-summaries-design.md`「設計確定・実装未着手」）、Skills（`docs/skills-concept-and-ui-design.md`「設計提案・未着手」）。**外向けコピーで語ってはいけない範囲** | 記録のみ |

**結論**: 「一日をテキストで記憶する。画像は保存しない」という説明は、**2026-08-02 の Visual recall と 2026-07-30 の音声入力が入った時点で古い**。外向けの説明はこの監査の内容に合わせて書き直した（`docs/product-hunt-launch.md` §1）。

---

## 2. オーナー判断が要る項目

### A. Gmail を全面 Composio にするのか（#4）

**現状**: 文書（CLAUDE.md）と実装が正面から食い違っている唯一の項目。

- CLAUDE.md【2026-07 決定】: 公式リモートMCPが Developer Preview で**実接続できない可能性が高い**ことを理由に、Gmail を読み取りからすべて Composio に寄せる。代償（受信箱が第三者を経由する）を明示的に受容。
- 実装: Gmail 読み取り／ドラフトは Google 公式MCP直結のまま。Composio は送信のみ。`docs/connector-adapter-plan.md` も同モデルで、公式MCPへの**実HTTP接続はライブキーで未検証**。

つまり「決定の前提（公式MCPが繋がらない）が未検証のまま、決定も未実施」という状態。**先に検証すべきで、文書側で勝手に片付けない。**

**次の一手（この順で）**:
1. 実機で `gmailmcp.googleapis.com` への OAuth＋接続を試す（`docs/phase1-ondevice-runbook.md` の連携手順）。
2. 繋がる → CLAUDE.md の 2026-07 決定を「不要になったため撤回」として記録し、実装・requirements の当初モデルを正にする（**受信箱が第三者を経由しない**ので privacy 面でも上位）。
3. 繋がらない → 実装を全面 Composio へ寄せる作業を起票し、requirements §6.9/§6.10 と CLAUDE.md 2026-07-30 のプラン注記（読み取り3開示opt-inを全プラン必須／Proゲートは送信解放のみ）を実装に反映する。

**この監査では**: CLAUDE.md の該当箇所に「**未実施。実装は当初モデル**」と明記し、決定文そのものは消していない（撤回はオーナーの判断）。

### B. requirements の未実装マーキング

`docs/requirements-v1.0.md` は「これから作るもの」として書かれた文書だが、実態は大半が実装済み。**要件の文言（MUST/SHOULD）は受け入れ基準として有効なので書き換えない**方針を取り、実装状況の追跡は `docs/feature-status.csv` に一本化した。requirements 側には位置付けの注記のみ入れている。


### C. Visual recall の保持上限（#18）

不変条件2の例外は「最大72時間」と書かれているが、実装はユーザー設定で最長10年まで伸ばせる。**どちらかに寄せる必要がある。**

- **文書を実装に合わせる場合**: 「保持期間はユーザーが選ぶ（既定3日、上限10年）。期限切れは自動削除」と書き換える。プライバシー訴求の強度は下がるので、外向けには「**既定オフ**・端末内・暗号化・期間はあなたが決める・自動削除」の順で言う（そう書いてある）。
- **実装を文書に合わせる場合**: プリセットを 1〜7 日に限定し、カスタム上限を撤廃する（実装は `MAX_CUSTOM_RETENTION_DAYS` の変更のみで済む）。

**どちらでもよいが、決めるまで外向けに「72時間で消えます」と書かない。**現に消えない設定が存在する。

### D. 音声入力のクラウド ASR（#19）

2026-08-05 の例外は会議 ASR に限定して書かれている。実装では**音声入力（hold-to-talk）も Deepgram を第一経路**にしており、例外の外側にある経路が動いている状態。ディスクへ書かない点と `mip_opt_out` は共通なので、**実務上の危険度は低いが、記録がない**。

**次の一手**: CLAUDE.md 不変条件2の例外文を「Meeting notes の ASR」から「**会議 ASR および音声入力（push-to-talk）の live STT**」へ広げ、UI 開示（設定 → 音声）に同じ文言を出す。この監査では文書を勝手に広げず、判断待ちとして記録に留めた。

---

## 3. この監査で行った文書修正

- `CLAUDE.md`: 開発フェーズの現在地、crate構成（10 crate）、`apps/api` の位置付け、DB暗号化（SQLCipher）、Gmail 決定の未実施注記、Wave 1 への Drive 追加。
- `docs/requirements-v1.0.md`: ステータス／最終更新、本書の位置付け（Phase 0ゲートの記述）、§5.1 crate構成、FR-INT-03（Wave 1）、FR-API-01/02/03、FR-BIL-08、NFR-SEC-01、NFR-TEL-01、ADR-002、ADR-003。各所に `【2026-08-20 実装反映】` の印を付けた。
- `README.md`: desktop / api / website の現況。

## 4. 残っている整理タスク（コード側。この監査では触っていない）

- [ ] i18n 辞書の紹介プログラム残骸（`hero.invitedBy` / `invitedTier`、4言語）と `src/lib/referral.ts` の扱いを決める（撤去 or 再開）
- [ ] LP の架空テスティモニアル・未実証の「4h saved」・未取得の Product Hunt バッジ文字列（`docs/product-hunt-launch.md` §10）
- [ ] Memory API の Streamable HTTP と ハードPro gate（`todo.md`）
- [ ] Gmail 公式MCPの実接続検証（§2-A）
