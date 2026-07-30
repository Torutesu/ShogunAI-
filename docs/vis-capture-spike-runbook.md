# 視覚キャプチャ検証スパイク ランブック（FR-VIS 検証ゲート）

| 項目 | 内容 |
|---|---|
| 目的 | requirements-v1.0 §6.21 の検証ゲート3問に実測で答え、視覚キャプチャ本実装のGo/No-Goを判定する |
| 対象 | `spikes/vis-capture/`（Swift実行ファイル `visspike`） |
| 実行環境 | Apple Silicon Mac / macOS 14+ / Xcode Command Line Tools |
| 上位文書 | `docs/requirements-v1.0.md` §6.21、`docs/core-features-v1.1-proposal.md` §8 |

## 0. 判断記録: スパイクをSwiftで書く理由

製品実装はCLAUDE.mdの技術スタックどおり **Rust + objc2** で行う。本スパイクだけSwiftなのは、(a) ScreenCaptureKit / Vision の非同期API・delegate群をobjc2で書くコストが計測の目的に対して過大、(b) 実機で `swift run` 一発でビルド・実行でき、検証の往復が最速、(c) 使い捨ての計測コードであり製品に持ち込まない、の3点による。**このコードを製品経路に流用しない**（Phase 0のspike-harnessとは扱いが異なる）。

## 1. スパイクが答える3問（§6.21 検証ゲート）

| # | 問い | 計測 |
|---|---|---|
| 1 | イベント駆動取得＋OCRのCPU/バッテリーが別枠予算内か | プロセスCPU平均・1分平均p95、powermetrics併走 |
| 2 | OCRでAXの空白がどれだけ埋まるか（精度向上の実在） | フレームごとの「OCRのみトークン比率」（AXで取れなかった語の割合）をアプリ別に集計 |
| 3 | ストレージ増加率 | ダウンスケール(最大幅1600px)+JPEG(q0.5)+dHash重複破棄後の実測MB → MB/日換算 |

スパイクの挙動は本実装と同じ規律に揃えてある: イベント駆動キーフレームのみ（連続録画なし）、フォーカス切替＋内容変化（2秒フロア）トリガ、dHash重複破棄、パスワードマネージャ除外、AXSecureTextField非読取、プライベートブラウジングのタイトルヒューリスティック除外。

## 2. 準備

1. Xcode CLTを導入済みであること（`xcode-select --install`）
2. ターミナル（iTerm/Terminal）に**Accessibility権限**を付与: システム設定 → プライバシーとセキュリティ → アクセシビリティ
3. 初回実行時に**画面収録権限**のプロンプトが出るので許可（許可後にプロセス再起動が必要な場合あり）

```sh
cd spikes/vis-capture
swift build -c release   # 初回のみ。以後は swift run
```

## 3. 実行プロトコル

### 3.1 セッションA: 通常作業（2時間）

普段どおりの作業をしながら流す。CPU・ストレージの代表値はここから取る。

```sh
# ターミナル1: スパイク本体（120分で自動停止。Ctrl-Cで途中終了も可）
swift run -c release visspike --duration 120

# ターミナル2: システム側の消費（60秒間隔でログ）
sudo powermetrics --samplers cpu_power,tasks -i 60000 \
  | tee /tmp/visspike-powermetrics.log | grep -E "visspike|Combined Power"
```

### 3.2 セッションB: AX空白地帯の狙い撃ち（20分）

問2の証拠集め。**AXでテキストが取りにくいアプリを最低3つ**、各5分程度操作する。候補: Slack / Notion / Figma / Discord / Miro / YouTube（字幕ON）/ PDFビューア / 独自描画のエディタ。通常のブラウザ・ネイティブアプリ（AXが効く対照群）も数分混ぜる。

```sh
swift run -c release visspike --duration 20
```

### 3.3 出力

各実行ごとに `vis-spike-out/<timestamp>/` に `summary.md`（判定表）・`frames.json`（フレーム別記録）・`frames/*.jpg` が書かれる。5分ごとに自動フラッシュされるので途中クラッシュでもデータは残る。

## 4. Go/No-Go判定基準

| # | 指標 | Go基準 | 根拠 |
|---|---|---|---|
| 1a | プロセスCPU平均（セッションA） | **≤ 5%** | FR-VIS-06 別枠予算の提案値 |
| 1b | 1分平均CPUのp95 | **≤ 10%** | 瞬間スパイクの許容幅 |
| 1c | capture+OCR p95 | **≤ 1500ms** | 非同期経路（SLO非クリティカル）だが2秒ポーリングを塞がないこと |
| 2a | AX空白アプリ群のOCRのみトークン比率 | **≥ 30%**（3アプリ以上で） | 「AXで取れない情報が実在し、OCRが埋める」の直接証拠 |
| 2b | 全アプリ平均のOCRのみトークン比率 | **≥ 5%** | 全体でも上積みがあること（ノイズでないこと） |
| 3 | ストレージ projection | **≤ 200MB/日**（8時間換算） | 既定7日保持で約1.4GB、別枠上限5GB（NFR-RES-03）に対する余裕 |
| 4 | バッテリー | powermetricsのvisspike消費が作業を阻害しない水準（記録のみ・数値ゲートなし） | 参考値としてPRに添付 |

**判定**:
- **全Go** → FR-VIS本実装（Rust/objc2）へ進む。スパイクの実測値をPR本文に貼る（CLAUDE.md SLO規律と同じ運用）
- **1系がNo-Go** → トリガ間引き（内容変化フロアを2s→5s）・OCR levelを`.fast`に落として再計測。それでも超過なら本実装をIdleタイミングのOCR遅延実行に設計変更
- **2系がNo-Go** → 視覚キャプチャの価値仮説が崩れるため本実装を中止し、AXテキスト＋連携データの現行路線を維持（要件の縮退先どおり）
- **3がNo-Go** → JPEG品質/解像度を下げて再計測。下げても超過なら既定を「抽出後即破棄」モードに変更（FR-VIS-03の縮退先）

## 5. 結果記入テンプレート

```
実行日:            機種:             macOS:
セッションA: CPU平均 __% / 1分p95 __% / capture+OCR p95 __ms / storage __MB → __MB/日
セッションB: 対象アプリと OCRのみ比率:
  - __________ : __%
  - __________ : __%
  - __________ : __%
対照群平均: __%
powermetrics所見:
判定: Go / No-Go(該当項目: )
```

結果は本ランブック末尾に追記し、`docs:` でコミットする（phase0-findings.md と同じ運用）。

## 6. 後片付け（必須）

スパイクのフレームは**平文JPEG**でローカル保存される（計測目的の一時データ。製品では暗号化＋期限管理される: FR-VIS-03）。実行後は必ず削除する:

```sh
rm -rf spikes/vis-capture/vis-spike-out
```

`vis-spike-out/` と `.build/` は `.gitignore` 済み。**フレーム画像をコミット・共有しないこと。**

## 7. Go後の本実装への引き継ぎ

- 製品実装はRust/objc2（`objc2-screen-capture-kit` / Vision相当）で `shogun-core` のキャプチャレーンに載せる（FR-VIS-02: 専用経路を作らない）
- スパイクで確定したパラメータ（ダウンスケール幅・JPEG品質・dHash閾値・内容変化フロア）を初期値として持ち込む
- 精度検証の本命（state抽出・検索のrecall前後比較）は `crates/shogun-memory/tests/retrieval_eval.rs` の評価セットにOCR由来文書を加えて実施する（スパイクの問2はその前段のプロキシ）

---

## 実行結果ログ

（未実施。上のテンプレートで追記すること）
