# Push-to-Talk 音声対話 設計書（Issue #44）

作成日: 2026-07-30 / 対象Issue: [#44 Heyclikey的なショートカット押した後の音声対話](https://github.com/Torutesu/ShogunAI-/issues/44)

## 1. これは何か

ショートカットを**長押ししている間だけ**マイクを開き、離した瞬間に文字起こし・コンテキスト結合・エージェント応答までを一息で走らせる入口。「押す→話す→離す→即レス」のリズムで、作業フローを一切崩さずにSHOGUNを呼べるようにする。

本機能はゼロからの新規実装ではない。必要な部品はすでに揃っており、**本設計の主眼は既存部品の配線と、その配線に必要な最小限の一般化**にある。

| 必要な能力 | 既存資産 | 本Issueでの扱い |
|---|---|---|
| グローバルショートカット | `lib.rs::shortcuts` / `watch_option_tap`（NSEvent flagsChanged 状態機械） | 長押し（hold）検知へ一般化 |
| 録音＋オンデバイスASR | `audio_lane.rs` / `shogun_core::audio`（Whisper.cpp, Mic, Worker） | 一発モードのレーンとして再利用 |
| 常時最前面パネル | NSPanel（`overlay_ptr`, `PANEL_BEHAVIOR`, castle位置） | PTT専用ウィンドウを追加 |
| 現在コンテキスト | `ReplyContextCache` / `build_reply_context_for_screen()` | プリアセンブル済みキャッシュを読むだけ |
| エージェント呼び出し | `inline_source.rs::build_agent()` / `shogun_core::llm` | ストリーミング対応を追加して利用 |
| 計測 | `analytics.rs`（PostHog） | イベント追加のみ |
| 権限ガイド | `onboarding.rs`（部分） | マイク権限フローを追加 |

## 2. 不変条件との関係

| 不変条件 | 本機能での担保方法 |
|---|---|
| 2. 音声データを保存しない | 波形は `Worker` のRAMバッファのみ。PTTのSinkはDBにも書かず、文字起こしテキストをメモリ上で受けて破棄する。一時ファイルを作らない |
| 3. 生データをデバイス外に出さない | 外部に出るのは文字起こし後のテキストとコンテキスト抜粋のみ。egressはdigest-onlyでトレーサビリティに記録 |
| 4. L1に外部送信系を含めない | PTTの出力は**提示のみ**。応答から送信・投稿・カレンダー作成へ進む場合は既存のL3承認キュー（`approvals.rs`）を必ず通す。長押しから自動実行される経路は作らない |
| 1. データの重心はRustコア | 状態機械・ASR・コンテキスト結合はすべてRust側。webviewは描画とユーザー操作の受け口のみ |

## 3. 状態機械

PTTの中核は5状態の機械。純ロジックを `shogun_core::ptt` に置き、副作用（マイク開閉・ウィンドウ表示・LLM呼び出し）は Effect として desktop 側が実行する。会議ノートの machine と同じ構造で、ユニットテスト可能にする。

```
        HoldStart                 HoldEnd
Idle ─────────────► Recording ─────────────► Transcribing
 ▲                    │                          │
 │                    │ Cancel(Esc) / 権限失敗    │ ASR完了
 │                    ▼                          ▼
 │◄──────────────── (破棄)                   Responding
 │                                               │
 └───────────────────────────────────────────────┘
              Dismiss / 次のHoldStart
```

- **入力**: `HoldStart`, `HoldEnd`, `Cancel`, `Transcribed(text)`, `AsrFailed(reason)`, `ResponseChunk`, `ResponseDone`, `ResponseFailed(reason)`, `Dismiss`
- **Effect**: `StartCapture`, `StopCapture`, `DiscardCapture`, `ShowPanel(state)`, `PlaySound(start|end)`, `SubmitToAgent{text}`, `HidePanel`
- **短すぎるhold**（< 300ms）は誤爆とみなし、録音破棄・パネル非表示で `Idle` へ戻す
- **最大録音長 30秒**でサーバー側から `HoldEnd` 相当を自走させ、押しっぱなし放置でマイクが開き続けないようにする
- **Recording中の再Hold**は無視（多重セッションを作らない）。`Responding` 中の新規Holdは前セッションを打ち切って新規開始

## 4. ショートカット機構（最大の技術リスク）

「離した瞬間」を確実に取ることが本機能の成否を決める。2経路を用意する。

### 4.1 経路A: 素の修飾キー長押し（既定）

`watch_option_tap` の NSEvent `flagsChanged` 状態機械を `hold_monitor` として一般化する。既存実装は「⌥を単独でタップ（500ms以内）」を検知しており、必要な要素（down/upエッジ検出、他キー・マウス混入時のpoison、Instantベースの計時）はすべて揃っている。長押しは同じ機械の判定条件を変えるだけで得られる。

**既定キー: 右⌘の長押し。** `flagsChanged` の keyCode で左右を判別できる（左⌘=55 / 右⌘=54、右⌥=61）。右⌘単独押しはmacOSが何にも割り当てていないため衝突がない。Issueが例示する ⌘+Space は Spotlight と正面衝突するため採用しない。Fn（Globe）はシステム設定の「Globeキーを押して」と競合するため既定にしない（選択肢としては残す）。

⌥単独は既存のdraftトリガ（`watch_option_tap`）が占有済みのため使わない。

### 4.2 経路B: 通常コンボの長押し（ユーザーが選んだ場合）

`tauri-plugin-global-shortcut` は `ShortcutState::Pressed` / `Released` を返す。現状のコードは `Pressed` しか見ていないため、`Released` の到達信頼性（キーリピート、フォーカス喪失時の取りこぼし、修飾キー先離しの挙動）が未検証。

**これをM2のスパイクで先に潰す。** `Released` が信頼できない場合、設定UIは経路Aの素修飾キー（右⌘ / 右⌥ / Fn）のみを選択肢として提示し、コンボ長押しは提供しない。この判断は実装着手前に確定させる。

### 4.3 取りこぼし時のフェイルセーフ

`HoldEnd` が来ないまま30秒経過、またはアプリがフォーカスを失って修飾キー状態が不明になった場合、機械は自動で `HoldEnd` を発火して録音を閉じる。マイクが開きっぱなしになる状態を作らない。

## 5. 録音＋ASRレーン

`audio_lane.rs` は会議専用（`session_id` 必須、`DbSink` でDB書き込み、system tapで相手音声も取る）なので、PTT用に**一発モードのレーン**を分ける。

- **ソースはマイクのみ。** system tap は開かない（PTTは自分の発話だけが対象。他人の音声を拾う理由がない）
- **Sinkはメモリ。** `SegmentSink` を実装した `BufferSink` が発話テキストを連結して保持。DBには書かない
- **モデル・言語**は会議設定と同じ解決ロジック（`select_model_path` / `MeetingLanguage`）を共有。日本語・英語に絞る（Issue Non-Goal準拠）
- `audio_lane::start` と共通化できる部分（モデル解決、Worker起動、poll-and-parkループ、stop時flush）は関数として切り出して両者から使う。会議側の振る舞いは変えない

**劣化方針**は会議と同じ思想を踏襲するが、結論が異なる。会議は「音声が無くてもノートは録る」ので notes-only へ落ちるが、PTTは音声が全てなので、マイク不可・モデル不可の場合は**セッションを開始せず**、理由を明示したパネルを出して `Idle` に戻る（黙って何も起きないのが最悪）。

## 6. コンテキスト結合とエージェント呼び出し

```
ASRテキスト
  + ReplyContextCache の現在値（プリアセンブル済み。押してから収集しない）
  + 前面アプリ / ウィンドウタイトル
  → prompt
  → build_agent()（BYOK: Anthropic既定）
  → ストリーミング応答 → パネルへemit
```

- コンテキストは `ReplyContextCache` を**読むだけ**。SLO「context cacheは押してから収集禁止」を守る。キャッシュが空なら音声テキスト単体で投げる（コンテキスト取得のために待たない）
- confidence gating は既存規約に従う（High=事実として記述 / Medium="possibly:" / Low=除外）
- 高度なコンテキスト融合ロジックはIssue本文どおり別Issueの管轄。ここでは最小結合に留める

### 6.1 ストリーミング（新規実装が必要な唯一の主要部分）

`AnthropicAgentClient::complete()` は既に `stream: true` でリクエストしており、SSEパーサ（`anthropic.rs:313` の `parse_sse_text`）も存在する。**足りないのは増分の経路だけ** — `HttpTransport::send` がボディを最後まで読んでから返すため、実質は「SSEを要求して全部バッファしてから解析」になっている。`inline_source.rs:948` にも「非ストリーミングなので初トークンではなく全体を計測している」と明記されている。

必要な追加は3つ: ①チャンク境界をまたぐ増分SSEデコーダ、②増分でボディを流すトランスポート、③それらを繋ぐ `complete_streaming`。既存の `complete()` は残す（inline draft がそのまま動く）。

「テキスト入力より速い」というIssueの定性ゴールは、この初トークン体感がすべてなので、これは削れない。**M5でAnthropicのSSEストリーミングを実装する。** 他プロバイダは非ストリーミングのままフォールバックさせる（応答完了時に一括表示）。

## 7. UI

Notch本体とは独立した専用ウィンドウ（label: `ptt`）を、`MeetingOverlay` と同じNSPanelパターンで作る。`PANEL_BEHAVIOR`（canJoinAllSpaces + fullScreenAuxiliary）と `OVERLAY_LEVEL` を適用し、全Space・フルスクリーンアプリの上に出す。表示位置は castle 設定に追従させ、録音中も応答も**同じ位置**に出す（Issue記載のUX）。

| 状態 | 表示 |
|---|---|
| Recording | マイクアイコン（脈打つ赤いドット）＋"Listening…"＋Escでキャンセルの示唆 |
| Transcribing | 解析中インジケータ |
| Responding | SHOGUNアイコン＋テキストバブル（ストリーミングで伸びる）＋ Copy / Close / Open in SHOGUN |
| Error | 理由と次の一手（設定を開く / 再試行） |

- **サウンド**: 開始「ピッ」/ 終了「ポン」。macOS標準のシステムサウンドを使い、システム音量と「サウンドエフェクトを再生」設定にそのまま従う
- **音声レベルバーは作らない。** `Worker` は音量を外に出す口を持たず、そのためだけにASRパイプラインへRMSの経路を通すのは、録音中を伝えるという目的に対して割に合わない。脈打つ赤いドットが同じ役目を果たす
- **録音中の明示**: OS標準のマイクインジケータに加え、パネル自身が録音中であることを常に示す（プライバシー要件）
- **UI文言は英語**（v1規約）。i18n-readyに保つ
- **ダーク/ライト両対応**。既存パネルのトークンに従う
- 応答パネルは自動で消さない。Escまたは Close、もしくは次のPTTセッション開始で閉じる

## 8. 設定・権限・計測

### 設定（Full UI）
- 「Push-to-Talk」セクションを追加: 有効/無効、キー選択、最大録音長、応答の読み上げ（**初期オフ**、将来オプション）
- キー選択は `shortcuts.json` に永続化。既存の `get_shortcuts` / `set_shortcut` の枠組みを拡張する（素修飾キーは既存のコンボ文字列形式に収まらないため、`ptt` は専用の値表現を持つ）

### 権限
- `Info.plist` に `NSMicrophoneUsageDescription` を追加。**確認済み: 現状は `LSUIElement` のみで未記載**
- 初回PTT実行時にマイク権限をガイド付きで要求。拒否時はパネルに理由とシステム設定へのリンクを出す
- アクセシビリティ権限はキャプチャ側で既に要求済みのため、PTTからは状態確認と案内のみ

### 計測（PostHog）
既存の `Analytics::capture()` にイベントを追加。**キャプチャ内容・発話内容は絶対に含めない**（規約）。

| イベント | プロパティ |
|---|---|
| `ptt_session_started` | trigger種別 |
| `ptt_session_completed` | 発話時間ms, ASR時間ms, 初トークンms, 応答完了ms |
| `ptt_session_cancelled` | 段階（recording/transcribing/responding） |
| `ptt_session_failed` | 理由コード（mic_denied / no_model / asr_failed / network / key_rejected） |

Issue記載の定量目標（週1回以上の利用率30%、ワーク完了50%）はこのイベント群から算出する。

⚠️ **訂正（実装中に判明）: PostHog はこのブランチの base である `main` に存在しない。** `analytics.rs` と PostHog クライアントは `feat/posthog-dau-mau-tracking`（Issue #61 / PR #91）にのみあり、本設計の初版はそのブランチ上で行った調査を根拠に「実装済み」と書いていた。誤り。

したがって本Issueのスコープでは、上記3イベントは**ローカルの構造化ログとして出すに留める**（`onboarding.rs::onboarding_event` と同じ既存の流儀）。ここでPostHogクライアントを作るとPR #91と重複・衝突するため作らない。PR #91 がマージされた時点で3関数の本体が実サンクへの送信に変わり、**呼び出し側は変わらない** — その継ぎ目になるよう `&AppHandle` を今から受け取っている。

PR #91 マージ時にあわせて設計が要るもの: **opt-out の尊重**。現状この repo には opt-out の仕組み自体が無い。

### リリース
β experimental flag の裏で提供。既定オフで、設定から有効化する。利用率とエラー率を見てから全体公開を判断する。

## 9. 実装単位

| # | 単位 | 内容 | 依存 |
|---|---|---|---|
| M1 | PTT状態機械 | `shogun_core::ptt` の純ロジック＋ユニットテスト | なし |
| M2 | ホットキー機構 | `Released` 信頼性スパイク → `hold_monitor` 実装 | なし（最優先） |
| M3 | 一発ASRレーン | `BufferSink` + mic-only レーン、audio_lane との共通化 | M1 |
| M4 | PTTパネル | NSPanelウィンドウ + React UI + サウンド | M1 |
| M5 | コンテキスト結合＋ストリーミング応答 | ReplyContextCache結合、Anthropic SSE、egressトレース | M1, M3 |
| M6 | 設定・権限・計測・βフラグ | Full UIセクション、Info.plist、権限ガイド、PostHogイベント | M1〜M5 |

M2を最初に潰す。ここの結論次第で設定UIが提示できる選択肢が変わるため、後段の手戻りが最も大きい。

## 10. テスト・受け入れ基準

### 自動テスト
- M1の状態機械: 全遷移、短すぎるhold、30秒上限、Recording中の再Hold、各失敗経路
- M3のレーン: モックASR + モックソースで BufferSink の連結・stop時flushを検証
- M5のプロンプト構築: コンテキスト有無の両方、confidence gatingの反映

### 手動検証（macOS実機）
- 他アプリ最前面、フルスクリーンアプリ上、別Space でのパネル表示
- 修飾キー先離し・キーリピート・アプリフォーカス喪失での `HoldEnd` 取りこぼし
- マイク拒否 → ガイド表示 → 許可後に復帰
- 機内モード（ネットワーク断）での失敗表示

### SLO
| 項目 | 上限 | 計測方法 |
|---|---|---|
| Hold開始 → パネル表示 | 100ms | 既存 `metrics.rs` に計測を同梱、p50/p95をPRに貼る |
| Hold終了 → ASR完了 | （実測値を記録し目標を後決め） | 同上 |
| ASR完了 → 初トークン | 1s | 同上 |
| アイドル時CPU（PTT未使用時） | 追加分ほぼゼロ | hold_monitorが常駐する分の増分を確認 |

### ビルド確認
`cargo check -p shogun-desktop-spike` を必ず通す。crates側のAPIを変えた場合、core/memory のテストだけでは desktop 側の破損を検知できない。

## 11. 未確定事項

- PostHog本番環境の確定（Issue #61 / PR #91 と共通の課題）。上記§8の訂正も参照
- **`.entitlements` ファイルが存在しない。** 会議ノートの Core Audio プロセスタップ（`CATapDescription` / `AudioHardwareCreateProcessTap`）は、署名・notarize されたビルドで entitlement を要求する可能性がある。PTT自身はタップを使わないためスコープ外だが、Task 16 の実機検証で確認する
- **`NSMicrophoneUsageDescription` の欠落は本Issue以前からの既存バグだった。** `audio_lane.rs` が会議ノートで既にマイクを開いており、キーが無いと macOS TCC がアプリを終了させうる。本Issueの変更が副次的に解消する
- ASRのウォームアップ戦略: Whisperモデルのロードに時間がかかる場合、初回PTTが遅くなる。M3の実測後に、βフラグ有効時はモデルを事前ロードしておく等の判断をする

## 12. スコープ外（Issue Non-Goal準拠）

- ASRエンジンの自前実装・精度最適化
- 日本語・英語以外の音声認識最適化
- macOS以外のプラットフォーム
- 音声対話のマルチターン管理・会話履歴の作り込み（1セッション1往復に留める）
- PTT応答からの外部送信アクションの自動化（L3承認は既存フローに委ねる）
