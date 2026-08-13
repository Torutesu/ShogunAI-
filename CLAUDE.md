# CLAUDE.md — ShogunAI

SHOGUNのモノレポ。本ファイルはmacOSアプリ本体(`crates/` + `apps/desktop`)の実装時に常に守る運用ルールを記す。要件定義の全文は `docs/requirements-v1.0.md`、ノッチUIスパイク仕様は `docs/notch-ui-prototype-spec.md`、Phase 0の実装指示は `docs/phase0-dev-instructions.md` を参照。詳細で迷ったら docs/ を読むこと。マーケサイト(`apps/website`)は本ファイルの対象外(website配下のREADME/規約に従う)。

## プロダクト一言定義

ユーザーの仕事のワールドモデルを構築し、Macのノッチから「ボタンを押して仕事が終わる」体験を提供するローカルファーストAI OS。記録ツールではない。**状態の推定と実行**のプロダクト。

## 絶対不変条件（違反するコードは書かない）

1. **データの重心はRustコアに置く。** DB・キャプチャ・context cache・SLO責務はRustプロセスが単独所有。webview側にデータ層のロジックを置かない
2. **画像・音声データを一切保存しない。** 画面キャプチャはAccessibility API経由のテキストのみ。会議の音声は原則オンデバイス処理とし、**SHOGUN 自身は波形をディスク・一時ファイルに書かない**。永続化するのは文字起こしテキストとそのprovenanceのみ。スクリーンショット・録画・音声ファイルを生成するコードを書かない  
   **【2026-08-02 明示的例外】Visual recall が On のとき、OCR 用に取得した画面は圧縮 JPEG として暗号化済みメモリ DB に最大 72 時間だけ保持し、期限切れは自動削除する（`screen_frames` テーブル）。クラウド送信なし・音声対象外・永続タイムラインではない。Visual recall Off では画像も保存しない。**  
   **【2026-08-05 明示的例外｜会議 ASR】Meeting notes の既定 ASR は Deepgram Nova-3 Multilingual（クラウド live STT）。音声はライブ文字起こしのためにのみ外部へ送る（process-only）。常に `mip_opt_out=true`（学習・モデル改善への利用なし）。波形は SHOGUN がディスクへ書かない。会社キーはデスクトップバイナリ／共有 Keychain 秘密に埋め込まず、SHOGUN/Select バックエンドが保持するか短命 JWT（Deepgram `/auth/grant`）を発行する。UI に開示必須。FR-MT-13（オンデバイス ASR）への明示的例外。**
3. **生データはデバイス外に出さない。** クラウドに出るのは処理用チャンクのみ。送信箇所には必ずトレーサビリティログを実装（明示的例外＝Gmail の Composio 全面経由、および 2026-08-05 の会議 ASR Deepgram ライブ STT。詳細と必須条件は各例外参照）
4. **L1（自動実行）に外部送信系アクションを絶対に含めない。** 送信・投稿・カレンダー作成は必ずL3（明示確認）
5. **キーの分離**: インデックス・分類・Dream Cycle・Morning Brief = Select KKキー（Batch API）／エージェント推論・チャット・ドラフト = ユーザー資格情報（BYOK **または** サブスク委譲。Issue #110）。逆転させない。**サブスク委譲をBatch laneに使わない**（委譲先が使うのは月次の有限クレジット。バッチ量はそれを最速で溶かす作業であり、焼き切るとAgent lane自体が月替わりまで死ぬ）
6. **人間UIとAI API（MCP/CLI）は完全対称。** 新機能はUIとAPI両方から呼べる形で設計する。AI経由の操作にも同じL1/L2/L3を適用
7. **secrets（OAuthトークン・BYOKキー）はKeychain以外に保存しない。** 平文ファイル・DB・ログへの書き出し禁止  
   **【2026-08-13 明示的例外｜ライセンストークン】** FR-BIL-08 の署名済みライセンストークン（`v1.<payload>.<sig>`）は `billing.json`（app-data、平文）に置く。これは**秘密ではない**: Ed25519署名済みで改竄不能、payload内のdevice idに束縛されるため他Macでは無価値、約24時間で失効する。CLI/MCP/RESTの3面がKeychainに触れずプラン状態を読めることが設計上の要件（`shogun_mcp::plan_source`）。**ライセンスキー本体**（`shogun-XXXX-…`、APIのbearer）は引き続きKeychainのみ。Batch relay と ASR mint はこのトークンをbearerとして提示する（`license_client::cached_license_token`）

## 技術スタック（確定。勝手に変更しない）

- **アプリ**: Tauri v2 / Rust backend / React + TypeScript frontend
- **対応環境**: macOS 14 Sonoma以上 / Apple Silicon (arm64) のみ。ノッチ非搭載Mac・外部ディスプレイはメニューバー中央の擬似ノッチパネルで対応
- **Notchパネル**: NSPanel（objc2）、`.nonactivatingPanel` + `.canJoinAllSpaces` + `.fullScreenAuxiliary`
- **DB**: SQLite（rusqlite）+ sqlite-vec、WALモード、FTS5 trigram。マイグレーションはバージョン管理必須（refinery）
- **埋め込み**: ローカルONNX多言語embeddingモデル同梱（クラウドembedding API不使用。オフライン動作・追加限界費用ゼロ）
- **macOSネイティブ**: AXUIElement / NSWorkspace / NSEventグローバルモニタ / security-framework（Keychain）
- **MCP**: Rust MCP SDK。クライアント（公式リモートMCPへ直接OAuth）とサーバー（Memory API）の両方
- **第2層連携**: Composio（オプトイン。Gmail送信含む）
- **Agent lane の資格情報**: 第一選択は**サブスク委譲**（Issue #110。ユーザーが既にログイン済みのベンダー公式CLI `claude` / `codex` / `gemini` をローカルサブプロセスとして起動し、そのプランの枠で推論する）。BYOK（APIキー）はフォールバック。**他アプリの資格情報ファイル・Keychainエントリを読むコードを書かない。ベンダーのコンシューマ向けOAuthを自前実装しない**（規約違反・BAN対象）
- **BYOK**: v1はAnthropic + OpenAI互換（プロバイダ抽象化層あり）
- **課金**: Stripe / **配布**: Developer ID + notarization（App Store不使用）/ **更新**: Tauri updater
- 判断記録: ストレージにPGLiteを使わない理由は docs/requirements-v1.0.md 付録B

## リポジトリ構成（pnpm workspace + Cargo workspace 同居）

```
crates/
  shogun-core/      # デーモン: キャプチャ、DB、context cache、イベントバス
  shogun-memory/    # スキーマ、マイグレーション、3層メモリ、state tables、検索
  shogun-fusion/    # Context Fusion: f(state, screen_ctx, intent) → action
  shogun-agents/    # L1/L2/L3実行エンジン、プリセットエージェント
  shogun-mcp/       # MCPクライアント＋サーバー、REST API
  shogun-cli/       # shogunコマンド
apps/
  desktop/          # Tauriアプリ（Notchパネル、Full UI、設定）
  website/          # マーケサイト（本ファイルの対象外）
  api/              # スタンドアロンAPI予定地（本ファイルの対象外）
packages/           # TS共有パッケージ（website系）
docs/               # 要件・仕様・判断記録
```

## プラン構成（要件詳細は docs/requirements-v1.0.md §課金）

- Freeプランなし。7日間フルトライアル（Pro相当）→ Standard / Pro
- **Standard**: キャプチャ＋メモリ＋検索＋Notch UI＋第1層連携（読み取り）＋Dream Cycle＋Morning Brief。Select KKキーのみで動作（BYOK不要）
- **Pro**: ＋エージェント実行（L1/L2/L3）＋Memory API（MCP/CLI/REST）＋Composio第2層。**サブスク委譲 または BYOK が必要**（Issue #110。APIキー必須をやめ、既に契約済みのClaude/ChatGPT/Geminiプランで動くことを既定にする。サブスク経路は明示的opt-in同意が前提）
- **【2026-07-30 決定】Gmail 読み取りは Composio 経由になっても Standard に含める**（Gmail 全面 Composio 化で「第1層読み取り=Standard」の文字通り解釈だと Gmail 読み取りが Pro 落ちし Wave 1 の Standard 価値が崩れるため）。Pro の「Composio第2層」ゲートが意味するのは**送信の解放（draft-stop OFF での実送信）のみ**。読み取りの 3開示 opt-in 同意は全プラン共通で必須
- プラン判定はRustコア側で行う。webview側のゲーティングだけに頼らない
- **会議ノート（§6.16 FR-MT群）はトライアル含む全プランで使える。** 使ってみて価値が分かる機能で、トライアル中に体験できなければ課金判断の材料にならない（Memory API経由の参照=FR-MT-22 のみPro）
- **【解消済み 2026-08-08】** design-system ブランチとの「Free / $0 プラン」分岐は、オーナー判断（**Free廃止・全員課金**、2026-07-26）どおり本ファイルを正としてマージ済み。LP（`apps/website` の pricing）も Standard $49 / Pro $99（年額、7日フルトライアル）へ全ロケール更新済み。Freeプランを再導入する変更を書かない

## データモデルの原則

- **event log と state tables を分離する。** state tables = `people` / `projects` / `commitments` / `open_loops`
- 全stateレコードに必須: 根拠イベントへの参照（provenance）＋ **confidence**。低confidenceの状態をContext Fusionが事実として生成物に混ぜてはならない（「〜の可能性」として弱く渡す）
- スキーマはspatial-ready: `window_pose` / `gaze_target` / `dwell_ms` / `display_id` / `window_bounds` のカラム余地を最初から確保
- 3層メモリ: Hot（24h、RAM）/ Warm（30日、DB）/ Cold（全履歴、int8量子化＋期間パーティション）。**通常のベクトル検索はWarm層のみを対象にする**（sqlite-vecは総当たりのため）
- 後方互換を破るマイグレーションを書かない。メモリは年単位で生きるデータ

## SLO（実装のアクセプタンス基準。計測コードを同梱する）

| 項目 | 上限 |
|---|---|
| Notch展開（Idle→Expanded） | 100ms |
| コンテキストアクションボタン提示 | 150ms |
| アクション実行→初トークン | 1s（ストリーミング必須） |
| ローカル検索 | 500ms |
| context cache更新（フォーカス切替） | 300ms |
| アイドル時CPU（SHOGUN自身） | 5%（1分平均） |

- レイテンシに影響する変更はp50/p95を計測してからマージする
- context cacheは「押してから収集」禁止。常時プリアセンブル

## 開発フェーズ

- **Phase 0（今ここ）**: ノッチUIスパイク。`docs/notch-ui-prototype-spec.md` に完全準拠、実装手順は `docs/phase0-dev-instructions.md`。4つの問い（常駐安定性・展開100ms・cache300ms+CPU5%・ホバー誤発火）に答えるまで本実装を始めない。No-Go時はメニューバー＋パレット方式へ転換
- **Phase 1（v1）**: Notch UI本実装 → キャプチャ＋メモリ＋state tables → Context Fusion＋L1/L2エージェント → 第1層MCP連携（Wave 1: Gmail+Google Calendar → Wave 2: Slack → Wave 3: Notion+GitHub+Linear）→ 課金＋トライアル
- 会議ノート（検知＋オンデバイスASR＋Recap）はv1へ前倒し（Issue #7 / `docs/meeting-notes-ui-design.md`）。ただし**音声ファイル・録音の保存は恒久的に対象外**（不変条件2）。実装順はM1〜M5（同書§7）
- v1に含めない: ナレッジグラフ/同期/メタメモリ（v2）、Computer Use/visionOS（Phase 3）。頼まれてもv1スコープに足さず、docs/requirements-v1.0.md のスコープ表を根拠に確認を取る

## 連携実装ルール

- 第1層 = 公式リモートMCP直結。OAuthはユーザー→サービス直接、トークンはKeychain
- **【2026-07 決定】Gmail は読み取り・ドラフト・送信のすべてを Composio 経由にする。** 当初は「読み取り/ドラフト＝公式MCP直結、送信のみComposio」だったが、公式リモートMCPが Developer Preview で実接続できない可能性が高く、ユーザー判断で Gmail を全面 Composio に寄せた（認証情報は Composio APIキー＋user id の1組のみ、Google Cloud OAuth 不要）。**代償として受信箱の内容が第三者(Composio)を経由する**ことを明示的に受容した決定であり、以下を必須とする:
  - Composio の使用（読み取りを含む）に **opt-in 同意（3開示）を必須**にする。同意なしでは同期も送信も行わない
  - 送信は従来どおり L3、かつ draft-stop（既定ON、同意後のみOFF可）
  - **読み取り egress にもトレーサビリティを記録**（第三者境界。内容は残さずダイジェスト/フラグのみ）
  - 認証情報のうち APIキーは Keychain、user id は非秘匿として設定JSON
- 上記は不変条件3（生データをデバイス外に出さない／第三者露出の最小化）の原則に対する**明示的・記録済みの例外**。将来 Gmail 公式MCPが GA になれば読み取り/ドラフトを直結へ戻す余地を残す
- Slack: WS管理者承認で接続不可の場合、ドラフト生成→クリップボードコピーにフォールバック
- Composio経由の連携はトレーサビリティ画面に「第三者経由」を明示するUIを伴うこと

## コード規約

- Rust: clippy warnings deny。`unwrap()` はテスト以外禁止（キャプチャデーモンは絶対に落とさない）
- クラッシュ耐性優先: DB書き込みはWAL＋トランザクション。電源断でメモリを壊さない
- エラーはユーザーの作業を中断させない形で処理（Notchインジケータの色で通知）
- テレメトリ・ログにキャプチャ内容（ユーザーのテキスト）を含めない。デバッグログも同様
- **匿名プロダクト分析は単一系統（2026-08-08 統合決定）**: PostHog オプトアウト方式（既定ON）。永続状態は `analytics.json` の `opt_out` のみで、オンボーディングのトグルも設定画面の Privacy & Security カードも**同じ状態**を読み書きする（`analytics_get_opt_out` / `analytics_set_opt_out`）。旧 `privacy.json`（opt-in方式, #28 Slice D）は廃止。分析イベントに機能カウント以外（キャプチャ内容・個人データ・キー）を載せない
- UI文言は英語（v1）。文言はコードから分離しi18n-readyに保つ
- UI文言・外部向けコピーを書く場合はSHOGUNブランドルール準拠: 競合名を出さない、技術スタック名を出さない、絵文字は⚔のみ、"AI-powered/revolutionary/second brain"禁止

## コミット・PR

- Conventional Commits（feat: / fix: / perf: / docs:）
- SLO関連の変更は計測結果をPR本文に貼る
- スキーマ変更はマイグレーションファイル＋ロールバック手順を必須添付
