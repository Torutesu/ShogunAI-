# Phase 0 開発指示書 — ノッチUIスパイク実装

- 対象読者: Phase 0 を実装するAIエージェント（Claude Opus 4.8想定）および人間のレビュアー
- ステータス: 確定（Phase 0 着手基準）
- 本書の位置付け: **何を作るか**は `docs/notch-ui-prototype-spec.md`（以下「仕様書」）が正。本書は**どう進めるか**（作業順序・完了定義・コミット規律・エスカレーション規則）だけを定める。両者が矛盾したら仕様書が勝つ。矛盾を見つけたら実装せず報告する

## 0. 読む順序（実装開始前に必読）

1. `CLAUDE.md` — 絶対不変条件・SLO・コード規約。全作業でこれに違反するコードを書かない
2. `docs/notch-ui-prototype-spec.md` — スパイクの完全仕様。パラメータ・状態遷移・計測点はすべてここの数値に従う
3. 本書
4. `docs/requirements-v1.0.md` — 参照のみ。**Phase 0 では要件v1.0の機能を実装しない**（スコープは仕様書§2が全て）
5. `docs/phase0-on-device-runbook.md` — **実機（ノッチ搭載Mac）での仕上げ・実測に入る段階で必読**。Linux+CI で完了できない挙動配線・計測タスク（D-01〜D-08）と Go/No-Go 手順を、既存コードとの接続点付きで手順化

## 1. ゴールと非ゴール

- ゴール: 仕様書の「4つの問い」（Q1常駐安定性 / Q2展開100ms / Q3 cache300ms+CPU5% / Q4ホバー誤発火）に**数値で答えるレポート** `docs/phase0-report-<date>.md` を確定させること
- 非ゴール: 製品コードを書くこと。スパイクは使い捨て（`crates/spike-harness` の計測コアのみ持ち越し）。磨き込み・リファクタ・汎用化に時間を使わない
- Go/No-Go の**判定はユーザー（人間）が行う**。エージェントの完了条件は「レポート確定＋判定に必要な材料が揃った状態」まで

## 2. 環境前提と制約

| 項目 | 内容 |
|---|---|
| 実行環境 | Apple Silicon (arm64) の macOS 14+ **実機**。ノッチ搭載機（MacBook Pro 14"/16" 等）+ 外部ディスプレイ1枚が検証構成 |
| 必須ツール | Xcode Command Line Tools / Rust stable（rustup）/ pnpm / Tauri v2 CLI |
| 権限 | Accessibility 権限（AX・CGEventTap用）。Input Monitoring が必要になった場合は仕様書付録B-3として記録 |
| 計測の有効条件 | **releaseビルドのみ**。debugビルドの計測値は無効（仕様書§2.2）。リモートLinux環境ではビルド確認・lint・単体テストまでしかできない — 計測系タスクは実機セッションで行う |
| CI | GitHub Actions: `cargo clippy --all-targets -- -D warnings` / `cargo test` / `pnpm -r typecheck` を必須化。macOSビルドは `macos-14`（arm64）ランナーでビルド成功確認のみ（計測はCIでやらない） |

## 3. ブランチ・コミット規律

- 作業ブランチ: `spike/notch-ui` を main から切る。タスクごとの細分ブランチは不要（スパイクのため）
- コミット: Conventional Commits。1タスク（T-xx）= 1〜数コミット。メッセージに `T-xx` を含める（例: `feat: NSPanel swap and window attributes (T-05)`）
- 計測結果に影響する変更のPR/コミットには計測値を本文に貼る（CLAUDE.mdコミット規約）
- `pnpm-lock.yaml` / `Cargo.lock` はコミットする。website 側のコードには一切触れない

## 4. 作業順序と完了定義（DoD）

仕様書§8のT-01〜T-16に従う。各タスクのDoDを以下に定める。**DoDを満たさないまま次に進まない**（依存タスクを除き並行は可）。

### Stage A: 調査（T-01, T-02）— 最初にやる。結果次第で以降の実装方針が変わる

| タスク | DoD |
|---|---|
| T-01 tauri-nspanel調査 | 案A/案Bの採否決定と根拠を `docs/phase0-findings.md` に記録（検証コード断片・確認したバージョン番号付き）。タイムボックス1日、超過したら案Bへ即切替 |
| T-02 グローバル監視調査 | mouseMoved配信範囲の実測結果、CGEventTap要否、keyDownモニタの必要権限を findings に記録。仕様書付録Bの該当項目に結論を書き戻す |

`docs/phase0-findings.md` は「実装時に要検証」15項目（仕様書付録B）の回答台帳とする。**項目に触れるタスクを完了する際、必ず該当項目へ結論を書き戻すこと。** 全項目が「検証済み or 意図的にペンディング（理由付き）」になっていることがT-16の前提条件。

### Stage B: 骨組み（T-03, T-04）

| タスク | DoD |
|---|---|
| T-03 ワークスペース骨組み | ルート `Cargo.toml`（workspace）新設、`apps/desktop` のTauri v2化、`crates/spike-harness` 空実装。`cargo build --release`（arm64）と `pnpm tauri dev` が通る。clippy deny設定（`[workspace.lints]`）を最初から入れる |
| T-04 ハーネスコア | クロック校正・リングバッファ・JSONL writer・`slo.rs`・cpu_sample が単体テスト付きで動く。JSONLスキーマが仕様書§4.4と一致。**キャプチャ本文・タイトル本文をpayloadに書けないことをテストで保証**（本文フィールドを持たない型設計にする） |

### Stage C: パネルと入力（T-05〜T-09）

| タスク | DoD |
|---|---|
| T-05 NSPanel化 | 仕様書§3.1.2の全属性が設定され、起動時に属性値をログでダンプして目視確認できる。透過・クリック透過（Idle時）が成立 |
| T-06 ノッチ検出+擬似ノッチ | 実ノッチ機/擬似モードの両方でIdle表示が仕様書§3.2の寸法通り。`event.notch_geometry` が記録される |
| T-07 ホバー監視 | 早期リジェクト・コアレス・速度算出が実装され、**座標正規化の単体テスト（マルチディスプレイ配置3パターン以上）が通る**（仕様書§3.4.7） |
| T-08 状態機械 | 全遷移T1〜T6がJSONLに記録される。状態・タイマーはRust側のみ（webviewにタイマー・状態分岐がないことをコードレビュー観点に）。Expanded DOMは常時マウント |
| T-09 誤発火対策一式 | dwell/速度延長/メニュー抑制/ドラッグ抑制/⌃⌥⌘F手動マークが動作。パラメータは仕様書付録Aの値をハードコードせず設定構造体に集約（リトライ時に変更するため） |

### Stage D: 計測とキャッシュ(T-10〜T-14)

| タスク | DoD |
|---|---|
| T-10 expand_latency計測 | rAF×2方式が実装され、スロー撮影較正の手順がfindings に記録される（較正自体は実機セッションで実施） |
| T-11 context cache | 仕様書§3.10の全上限値(深さ8/300要素/32KB/250ms/MessagingTimeout 100ms)が実装される。**AX呼び出しが `axcache.rs` の外から到達できないこと**（§3.10.3）。AXSecureTextField除外のテスト |
| T-12 表示継続系 | display_change/スリープ復帰/ヘルスチェック/自己修復/擬似ノッチFS挙動が実装され、該当イベントがJSONLに記録される |
| T-13 自動化スクリプト | `scripts/spike-soak.sh` / `spike-expand-test.sh` / S-13用osascriptが動く。CGEventPost注入がモニタに届くかの結果をfindingsへ |
| T-14 レポート生成 | `cargo run -p spike-harness --bin report` が仕様書§4.6の全項目を含むMarkdownを出す。手集計禁止 |

### Stage E: 実施と判定材料(T-15, T-16)

| タスク | DoD |
|---|---|
| T-15 シナリオ実施 | S-1〜S-14の全記録がJSONLに存在し、各シナリオの実施メモ（日時・構成・目視所見）が `docs/phase0-report-<date>.md` の草稿に入る |
| T-16 判定材料確定 | レポート確定版 + findings全項目クローズ + リトライ実施記録（あれば）。**Go/No-Goの結論は書かず「判定材料」として提出**し、ユーザーの判定を仰ぐ |

## 5. 実装上の絶対規律（仕様書から抜粋・違反しやすい順）

1. 状態機械・タイマー・キャッシュ・SLO計測はRust側が単独所有。webviewの責務は「class切替・描画完了通知・操作イベント転送」の3つのみ（IPC契約は仕様書§3.11.2に列挙されたメッセージ以外を追加しない）
2. JSONL・デバッグログ・テレメトリにキャプチャ本文・ウィンドウタイトル本文を書かない。長さ(bytes)とxxhash64のみ
3. `NSEvent.mouseLocation` のポーリング禁止（イベント駆動のみ）。イベントハンドラ内でのメモリ確保・ログI/O禁止
4. `unwrap()` はテスト以外禁止。clippy warnings deny。パネル・監視系のエラーで**プロセスを落とさない**（記録して継続）
5. Expanded表示をトリガとするAX呼び出しをコード上不可能に保つ（「押してから収集」禁止の実証条件）
6. SLO数値・判定閾値は `slo.rs` と設定構造体に一元化。マジックナンバー散在禁止
7. スコープ逸脱禁止: DB・設定画面・LLM呼び出し・ネットワーク・署名/公証はPhase 0では作らない（仕様書§2.2）。「ついでに製品コードの基礎を作る」ことも禁止（spike-harness以外は使い捨て前提）

## 6. エスカレーション規則（実装エージェント向け）

- **仕様の欠落・矛盾を見つけた場合**: 勝手に補完して進めない。findings に「仕様ギャップ」として記録し、影響が局所的（パラメータ1つの解釈など）なら暫定値を明記して進行、構造に関わる場合（状態機械の遷移追加が必要等）は停止してユーザーに選択肢付きで確認
- **「実装時に要検証」項目が想定と逆の結果になった場合**: 仕様書に書かれたフォールバック（例: 案A→案B、level 25→101、グローバルモニタ→CGEventTap）に従い、findings に記録。フォールバックが仕様書にない場合はエスカレーション
- **タイムボックス超過**: タスク単位の目安（仕様書§8）を2倍超過した時点で、削れる範囲（仕様書§2.2「作らないもの」を増やす方向）の提案を添えて報告
- **SLO未達が見えた場合**: 即座に諦めず仕様書§6の該当Qのリトライ規定を消化。それでも未達なら「未達の数値＋試したこと」をレポートに残す（No-Goも有効な成果である。数値を粉飾しない）

## 7. 完了チェックリスト（提出前セルフチェック)

- [ ] `cargo clippy --all-targets -- -D warnings` / `cargo test` / `pnpm -r typecheck` が全て通る
- [ ] releaseビルドでS-12(n≥200)・S-13(n≥100)・S-11(24hソーク)のJSONLが存在する
- [ ] レポートが層別（notch/pseudo × fullscreen）の p50/p95/p99 を含む
- [ ] `docs/phase0-findings.md` の15項目全てに結論または理由付きペンディングが書かれている
- [ ] 誤発火の分母（top_band_entry）が記録されている
- [ ] webview側にタイマー・状態分岐・キャッシュ・AX呼び出しが存在しない
- [ ] JSONLにユーザーテキスト本文が1件も含まれない（grepで確認し、確認コマンドをレポートに記載）
- [ ] website / packages 配下に差分がない

## 8. Phase 0 完了後の接続

- Go判定時: Phase 1 着手。`crates/spike-harness` を `crates/` 配下の正式クレートとして残し、`apps/desktop/src*` の扱い（破棄 or legacy/ 移動）を判定と同時に決定（仕様書§2.3）。Phase 1 の実装順序は `docs/requirements-v1.0.md` の開発フェーズ定義に従う
- No-Go判定時: `docs/palette-ui-spec.md` を新規作成（仕様書§7の転換パス）。本仕様書・レポート・findingsは判断記録として凍結

### 8.1 `crates/spike-core` の扱い（確定: 残す）

- **決定**: `crates/spike-core`（geometry / hover判定 / 状態機械 / AX walk policy / NotchEngine統合）を、`spike-harness` と同じく **Phase 0 から持ち越す資産**として残す。`apps/desktop/src*`（使い捨て対象）とは扱いを分ける。
- **根拠**: (1) Q2（展開）・Q4（誤発火）の正しさは状態機械とヒット領域計算の正しさに依存し、それを67の単体テストで担保済み。破棄するとオンデバイスで全ロジックを再実装＋無テストになり、担保が消える。(2) macOSアダプタを「OSイベントを流し込み Effect を適用するだけ」の薄い層に保つ設計の要。(3) Go後のPhase 1本実装で、状態機械・geometry・walk policy はそのまま製品ロジックとして流用可能。
- **dev-instructions §5.7 との関係**: 同§「spike-harness以外は使い捨て・製品基礎の作り込み禁止」からの**意図的な逸脱を正式に承認**する。逸脱を許容する範囲は spike-core に限る（純ロジック＋テストのみ。macOS依存・DB・LLM・ネットワークを持ち込まない）。`apps/desktop/src-tauri`（FFIアダプタ・グルー）は引き続き使い捨て前提。
- **Go/No-Go 別の後始末**:
  - Go: `crates/spike-core` を Phase 1 の該当クレート（`shogun-fusion` 等）へ吸収 or リネームして継続。テストは維持。
  - No-Go（パレット方式へ転換）: geometry の一部（h_mb 取得）と Recorder/slo は流用、状態機械は HoverIntent を除いた縮退版へ改変（仕様書§7.2 の流用表に準拠）。engine/hover のホバー判定は廃棄。
