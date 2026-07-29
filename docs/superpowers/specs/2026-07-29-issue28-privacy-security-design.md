# Issue #28 — Privacy & Security（LLM Key 保護 / 学習非利用 / ローカル優先 / データ削除）設計仕様

- Issue: [#28 ユーザーのLLM Keyが絶対に漏れないようにする。セキュリティ安全](https://github.com/Torutesu/ShogunAI-/issues/28)
- 隣接 Issue: #19（プライバシー）/ #23（設定UI一本化）/ #62（PostHog計測）
- 作成日: 2026-07-29
- ステータス: 設計承認済み（実装計画は writing-plans で別途作成）

---

## 1. 背景とゴール

ShogunAI は BYOK の LLM API Key と、画面・会議・連携由来の高感度ワークコンテキストを扱う。漏洩・悪用は「高額請求」「機密流出」「アカウント停止」を招く。さらに「送ったデータが学習に使われないか」「ローカルに閉じたい」という懸念がコアユーザーほど強い。

本仕様のゴールは、既存の絶対不変条件（CLAUDE.md）を**ユーザーに見える形で露出・拡張・文書化**し、以下を設計レベルで担保すること:

1. LLM Key が漏れない（既に Keychain 限定。UI で明示 + 誤露出防止）
2. ワークコンテキストが勝手に学習に使われない（**事実ベースで**文書化 + 匿名統計の同意制御）
3. 可能な限りローカル完結（境界定義 + 将来のローカル限定モードの切り出し余地）
4. ユーザーがいつでも削除できる（1h / 24h / 全削除 + アカウント削除）

### Non-Goal（本仕様の範囲外）

- ベンダー（Anthropic 等）の内部学習プロセスの変更。提供される設定は最大限利用する。
- 完全ローカル限定モード（クラウド通信ゼロ）の実装。境界定義まで。
- SOC2 / ISO 27001 等のフォーマル認証。
- 製品分析基盤（PostHog）本体の導入 → #62。本仕様は**同意トグルという契約点**のみ用意する。

---

## 2. 現状（コードベース実態）とギャップ

| 柱 | 既存実装 | 主要ファイル | ギャップ |
|---|---|---|---|
| Key 保管 | Keychain BYOK、平文非ログ、`ByokKey`/`SelectKkKey` 型分離 | `apps/desktop/src-tauri/src/inline_source.rs`、`crates/shogun-core/src/llm/anthropic.rs` | UI での「暗号化保存」明示、last-4 表示 |
| 削除 | `delete_all()`（Tx + `DeleteReport`、**未コマンド公開**）、`clear_memory`（state のみ） | `crates/shogun-memory/src/maintenance.rs`、`inline_source.rs` | **時間範囲削除なし**、`delete_all` 未露出、アカウント削除なし |
| DB マスキング | 書き込み前 redaction（issuer prefix + label） | `crates/shogun-memory/src/redact.rs`、`event_log.rs` | ログ/エラー経路の redaction なし |
| 学習オプトアウト | — | — | ポリシー未明示、同意トグルなし |
| Traceability | digest-only 送信ログ完備 | `migrations/V1__init.sql`（`traceability_log`） | （充足） |
| 設定 UI | BYOK 入力、テーマ、clear memory | `apps/desktop/src/App.tsx:1353-1652` | **Privacy & Security セクションなし** |
| 分析 | SLO メトリクスのみ（外部送信ゼロ） | `crates/shogun-core/src/metrics.rs` | 匿名統計トグルなし |

---

## 3. 重要な設計判断（正直さ優先）

イシューは「できない部分は正直に説明」を明示的に要求している。以下 3 点はその方針の中核。

### 判断① 「学習オプトアウト」はヘッダ実装ではなく文書化する

- Anthropic API（BYOK・Select KK Batch とも）は**商用規約上デフォルトで学習に使われない**。「学習させない」ための per-request ヘッダは存在しない。
- Zero Data Retention（ZDR）は**エンタープライズ契約レベル**の設定で、リクエストヘッダでは切り替えられない。
- 従って本仕様では:
  - Anthropic 向けは「**デフォルトで学習非利用**／保持は悪用監視目的で約30日／ZDR は将来のエンタープライズ課題」を**正確に文書 + 設定内カードに表示**する。
  - イシューの「匿名統計トグル」は Anthropic への送信ではなく **ShogunAI 自身の製品分析（#62）への同意**として実装する（両者を明確に分離）。
- 将来 v1 以降で他プロバイダ（OpenAI 等）を実装する際、そのプロバイダに学習利用設定 API がある場合は**デフォルト有効化**する（プロバイダ抽象化層に opt-out フックを用意）。

### 判断② マスキングは「DB」と「ログ」で逆方向

- `redact.rs` は **DB 書き込み経路**。ここで email / 氏名をマスクすると `people.emails` 等の**正当なメモリが壊れる**（メモリは email/氏名を意図的に保持する）。
- イシューが求める email / full URL マスクは **ログ・エラーレポート・テレメトリ経路専用**。
- 従って共有 redactor を切り出し、**ログ境界にのみ** email / URL / API キー形状のマスクを追加適用する。DB 経路の挙動は変えない。

### 判断③ 時間範囲削除は「孤児 state 削除 + provenance 剪定」

- `event_log.ts`（occurrence time, unix ms, indexed）で範囲削除可能。
- 派生 state（people / projects / commitments / open_loops）は集約。**対象イベントに紐づく `state_provenance` を削除し、根拠を全て失った（孤児化した）state 行のみ削除**する。他の生存イベントに支えられた state は残す。
- **既知の限界（正直に明記）**: 派生要約テキスト（`relationship_summary` 等）は、根拠イベント削除後も次回 Dream Cycle の再導出まで文言に影響が残り得る。UI・文書で「反映に最大◯時間」と説明する。将来、削除時の再導出トリガを追加検討（本仕様外）。

---

## 4. アーキテクチャ方針

- **データの重心は Rust コア**（不変条件1）。削除・マスキング・ポリシー判定は Rust 側に置く。webview はコマンド呼び出しと表示のみ。
- 既存 Tauri command パターン（`#[tauri::command]` → `lib.rs` の `invoke_handler![]` → React `invoke()`）を踏襲。
- 縦スライス5本。各スライスは UI から Rust まで貫通し、独立してマージ可能。
- UI 文言は英語（v1）+ i18n-ready。ブランドルール準拠（競合名・技術スタック名を出さない、絵文字は ⚔ のみ、"AI-powered/second brain" 等禁止）。

---

## 5. スライス別 詳細設計

### スライス A — 設定 UI「Privacy & Security」セクション

**目的**: 散在するセキュリティ設定を 1 セクションに集約し、約束を可視化する。

- `App.tsx` に `<PrivacySecuritySection />` を新設（既存 `ApprovalsSection` 等と同じ並び、`strings.ts` に文言追加）。
- 構成:
  - **LLM API Key カード**: 既存 BYOK 入力を移設。非表示型入力 + 保存/削除。設定済みは **last-4 のみ**表示（`get_llm_settings` を last-4 返却に拡張、平文は返さない）。「This key is encrypted in the macOS Keychain. No one — including the ShogunAI team — can read it in plaintext.」の短い説明。
  - **Data policy カード**: バッジ表示「Not used for model training」「Local-first」「AES-256 / TLS 1.3」。判断①の正確な文言。詳細ポリシー（スライスE の doc / Web）へのリンク。
  - **Data deletion カード**: 1h / 24h / All の 3 ボタン（スライスB へ接続）。
  - **Anonymous usage カード**: 匿名統計トグル（スライスD へ接続）。
- **依存**: B（削除コマンド）、D（トグル・ポリシー文言）が接続先。UI 枠は先行可能。

### スライス B — 時間範囲データ削除 + アカウント削除

**目的**: 「◯時間分だけ削除」「全削除 & アカウント削除」を提供。

- **Rust (`maintenance.rs`)**:
  - `delete_since(conn: &mut Connection, cutoff_ts: i64) -> Result<DeleteReport>` を追加。
    - `event_log WHERE ts >= cutoff` を削除（AD トリガで `event_fts` 同期、`event_vec`/`cold_embeddings` を対象 id で削除）。
    - `traceability_log WHERE ts >= cutoff`、`sessions.started_at >= cutoff` とその `session_notes` を削除。
    - `state_provenance WHERE event_id IN (削除対象)` を削除 → 剪定後に provenance を1つも持たない state 行（孤児）を削除。
    - 単一トランザクション、`DeleteReport` 返却。`delete_all` と実装対称。
  - テスト: 範囲内イベント削除 / 範囲外の生存 / 孤児 state 削除 / 生存 state 保持 / FTS 同期 / セッション・ノート削除。
- **Tauri command (`inline_source.rs` + `lib.rs`)**:
  - `delete_data_since(range: "1h" | "24h") -> Result<DeleteReport, String>`（現在時刻から cutoff 算出は Rust 側）。
  - `delete_all_and_account() -> Result<DeleteReport, String>`: `maintenance::delete_all` を呼び、**全 BYOK Keychain エントリ + OAuth トークンをクリア**（`clear_byok_key` 全 provider + integrations の Keychain）。アカウント削除の外部 API 連携が必要な部分はスライスE で文書化。
- **UI**: 3 ボタン + 確認ダイアログ（All は既存 clear memory 同様の二段確認 / タイプ確認）。実行後トースト「Deletion request accepted. This may take up to ◯.」削除対象データ種別を明示。
- **依存**: A（UI 枠）。

### スライス C — ログ / PII redactor

**目的**: ログ・エラーレポートから高感度情報を強制マスク。

- **共有関数化**: `redact.rs` の issuer prefix / label 正規表現を、DB 用途を壊さない形で共有ヘルパに整理。
- **ログ専用 redactor** を新設（DB 用とは別関数）: 既存の secret パターンに加え **email / full URL（クエリ含む）/ API キー形状（`sk-…` `sk-ant-…` `ghp_…` 等の正規表現強制マスク）** を追加。
- **適用箇所**:
  - Rust: tracing サブスクライバ層 or ログ出力ヘルパで redactor を通す。既存 `eprintln!` はヘルパ経由に順次移行（少なくとも高感度が通る経路）。
  - エラーレポート/パニック経路: メッセージを redactor に通す。
  - JS: console 出力ラッパ（開発時ログにキー・トークンが出ないこと）。
- **不変条件の保守**: 「テレメトリ・ログにキャプチャ内容を含めない」（CLAUDE.md）を破らないこと。redactor はあくまで多層防御の最後の砦。
- テスト: 「API キー形状は必ずマスク」「email/URL マスク」「DB 経路の people.emails は非破壊（DB redactor 未変更）」を保証。
- **依存**: なし（独立実装可）。

### スライス D — ベンダーポリシー明示 + 匿名統計トグル

**目的**: 判断①を UI・データに落とす。#62 の契約点を用意。

- **匿名統計トグル**: ローカル preference として保存（既存 `get_llm_settings`/`set_llm_settings` と同じ設定ストア機構を利用、または専用 `get/set_privacy_prefs`）。既定値はイシュー方針に従い決定（下記 Open Question）。
  - **ゲート関数** `analytics_enabled() -> bool` を Rust 側に用意。#62 の PostHog 送信は必ずこのゲートを通す。
  - Off 時: 課金・安定運用に必須なイベントのみ許可（許可リストを定義）。
- **ベンダーポリシー表示**: 設定内 Data policy カード（スライスA）に、判断①の正確な文言 + プロバイダ別データ保持ポリシー表への導線。
- **プロバイダ抽象化の opt-out フック**: `AgentClient` 抽象に「学習 opt-out 設定を要求する」インターフェース余地を用意（v1 は Anthropic のみ = no-op、将来 OpenAI 等で有効化）。
- **依存**: A（表示先）。#62 とは疎結合（ゲート関数の契約のみ）。

### スライス E — Privacy & Security 文書 + ローカル/クラウド境界

**目的**: 「何を・どこに・何に使い・何に使わないか」を明文化し、1 クリック導線を作る。

- `docs/privacy-security.md`（アプリ内・LP から参照可能な正典）:
  - データ種別（画面テキスト / 会議文字起こし / メタデータ / state）ごとに **保存先（ローカル DB / Keychain / クラウド送信の有無）**。
  - **ローカル vs クラウド境界の比較表**（ローカル完結処理 / クラウド送信処理）+ 「ローカル優先」「標準」「将来のローカル限定モード（構想）」の3モード比較。
  - **削除ポリシー**: 論理削除でなく物理削除であること、バックアップ/スナップショットの扱い、ベンダー側データの削除可否（できない部分は正直に）。
  - **学習非利用**（判断①）とベンダー別保持ポリシー表。
  - **ローカル限定モードの境界定義**（将来切り出しの設計余地。要件定義レベル）。
- **導線**: 設定 Data policy カード → 本 doc、LP / FAQ に「Your data, your control」相当セクション、オンボーディングに 3 行のプライバシーステップ（「Key は Keychain に暗号化」「データは学習に使われない」「いつでも削除可能」+ 詳細リンク）。オンボーディング実装は進行中の issue-24 作業と調整。
- **依存**: 文言は A/D と整合させる。

---

## 6. データ / スキーマ影響

- **スキーマ変更なし**（削除は既存テーブルに対する DELETE、preference は既存設定ストア）。マイグレーション不要。
- 将来 preference を DB に持つ場合は追加マイグレーション + ロールバック手順を添付（CLAUDE.md 規約）。本仕様は既存設定ストア利用で回避。

## 7. テスト戦略

- Rust: `delete_since` / redactor / ゲート関数のユニットテスト（`maintenance.rs`・`redact.rs` の既存テスト様式に合わせる）。`unwrap()` はテスト以外禁止。
- 削除は in-memory DB で FK 順・Tx 原子性・孤児判定を検証。
- UI: 確認ダイアログの二段確認、last-4 表示（平文非露出）、トグル永続化。
- 回帰: DB redactor（`redact.rs`）の既存挙動を壊さないこと（people.emails 非破壊）。

## 8. リスクと不変条件チェック

- 不変条件7（secrets は Keychain 以外に保存しない）: last-4 表示は Keychain 由来のメタのみ、平文は webview に渡さない。
- 不変条件3（生データはデバイス外に出さない）: 削除・トグルは全てローカル処理。
- 「削除は最大◯時間」の正直な表示（判断③の派生 state 限界）。
- ログ redactor が既存の「ログにキャプチャ内容を含めない」方針を弱めないこと（多層防御であって代替ではない）。

## 9. 確定事項（旧 Open Questions、2026-07-29 承認）

1. **匿名統計トグルの既定値**: **OFF（オプトイン方式）**。プライバシー強調のため既定で送信しない。ユーザーが明示的に ON にした場合のみ匿名統計を送信。
2. **削除の表示文言**: ローカルデータは**即時（数秒）**削除。UI は「Deleted from this device.」等、ローカル即時を明示。ベンダー側に送信済みのデータは判断①のとおり保持ポリシーに従う旨を別途注記。
3. **git フロー**: **main 起点の新ブランチ `feat/issue-28-privacy-security`** で進める（統合ブランチは main、今後の PR は main 起点。作業ツリーの issue-24 未コミット変更とは分離）。スライス単位で PR を分けることも可。

## 10. 実装順序（推奨）

C（独立・低リスク）→ B（削除コア + コマンド）→ A（UI 枠）→ D（トグル + ポリシー）→ E（文書 + オンボーディング導線）。A は B/D と並行で枠だけ先行可。
