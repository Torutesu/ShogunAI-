# MT3 音声レーン設計 — オンデバイス ASR と会話文字起こし

- **Issue**: #7 バックグラウンド音声取得（Granola型自動検知）
- **段**: MT3（`docs/meeting-notes-ui-design.md` §7 の採番）
- **前提**: MT1（検知・ライフサイクル・フローティングUI）/ MT2（会議中ノート・自動終了・縮退Recap）は実装済み・到達可能
- **日付**: 2026-07-28
- **オーナー決定**: ASR=whisper.cpp small（turboを高精度オプション）/ システム音声=macOS 14.4+ で process tap・旧OSはマイクのみ縮退 / スコープ=MT3フル（端から端）

## 0. ゴールと非ゴール

### ゴール
- 会議 `Recording` 状態のときにマイクとシステム音声を取得し、**オンデバイスで文字起こし**して `transcript_segments` に蓄積する。
- 話者は「自分 / それ以外」の2値（マイク入力か system 出力かで判別）。
- statemachine が既に発行している `StartAudio` / `StopAudio` Effect を**実際に消費するランタイム**を作る（現状は誰も消費しておらず、音声レーンは空）。

### 非ゴール（このスコープでやらない）
- ライブ文字起こしの画面表示（設計書§3.4：意図的に出さない）。
- Recap 本体（要約・候補抽出・`[Track]` 確定）= MT4。本設計は transcript を**書くところまで**。
- 参加者名への話者割り当て（`calendar_occurrences.attendees` 突合）= 次段。
- 音声ファイル・録音の保存（**不変条件2により恒久的に対象外**）。
- Apple SpeechAnalyzer 実装（trait 差込口だけ用意し、実装は将来 macOS 26+ 対応時）。

## 1. 不変条件との整合（最重要・違反したら設計失敗）

CLAUDE.md 不変条件2：「画像・音声データを一切保存しない。会議の音声はオンデバイスでのみ処理し、波形はRAMから出さない（ディスク・一時ファイル・クラウドのいずれにも書かない）。永続化するのは文字起こしテキストとその provenance のみ」。

本設計の担保：
- PCM は **RAM のリングバッファのみ**に存在する。ファイル・一時ファイルを一切生成しない。
- ASR エンジン（whisper-rs）は**メモリ上の `&[f32]` を直接受ける** API を使う。ファイル入力を要求する経路は選ばない（設計書§1.1「一時ファイルなら良いを許さない」に準拠）。
- ASR 完了後、その発話区間の PCM を破棄する。
- 永続化するのは `transcript_segments`（テキスト＋provenance）のみ。クラウドに音声は出ない。
- モデルファイル（gguf）は**静的アプリアセット**であり、ユーザー音声ではない。同梱・ダウンロードは不変条件の対象外。
- テレメトリ・ログに文字起こし内容を含めない（コード規約）。

## 2. アーキテクチャ

```
mic ─────────┐                         ┌─ speaker='me'
             ├→ resample 16kHz mono f32 ┤
system tap ──┘   (system tap は 14.4+)  └─ speaker='other'
                        │
             [RAM リングバッファ 30s / 音源別 / drop-oldest]
                        │
                     VAD（無音境界で発話区間を切り出し・最大30s）
                        │
             Transcriber（trait）→ WhisperCpp（whisper-rs, Metal, small）
                        │
             transcript_segments（テキストのみ）  ← PCM は即破棄
```

- **リングバッファ 30s / drop-oldest**：ASR が追いつかない場合は古い音声から捨てる。貯めれば「実質的な録音」になるため、上限は設計上の防壁として固定値で持つ（設計書§2）。
- **ライブ文字起こしは出さない**ため、低遅延ストリーミングは不要。VAD で発話を切り出し無音境界でまとめて ASR に流す方式で足りる。これが whisper.cpp（非ストリーミング）で十分な理由であり、Parakeet の低遅延優位が効かない理由。

## 3. コンポーネント（各ファイル1責務）

| モジュール | 責務 | 主依存 | 単体テスト可否 |
|---|---|---|---|
| `shogun-memory/src/migrations/V9__transcript_segments.sql` | 設計書§2.1のテーブル新設＋インデックス | refinery | マイグレーション適用テスト |
| `shogun-memory/src/transcript_segments.rs` | `transcript_segments` の挿入・取得 API | rusqlite | ○（in-memory DB） |
| `shogun-core/src/audio/ring.rs` | 音源別 RAM リングバッファ（30s 上限・drop-oldest） | — | ○（純ロジック） |
| `shogun-core/src/audio/vad.rs` | 発話区間の切り出し（無音境界・最大30s・最小発話長） | webrtc-vad 系 or 自前エネルギー判定 | ○（合成PCMで境界検証） |
| `shogun-core/src/audio/resample.rs` | 入力サンプルレート→16kHz mono f32 | rubato 等 | ○ |
| `shogun-core/src/audio/capture/mic.rs` | マイク取得スレッド。speaker='me' | cpal | △（デバイス依存、trait裏でfake） |
| `shogun-core/src/audio/capture/system_tap.rs` | Core Audio process tap（14.4+）。speaker='other'。非対応OSは `None` | objc2 / core-audio-sys | △（同上） |
| `shogun-core/src/audio/capture/mod.rs` | `trait AudioSource`（capture 抽象。fake 実装をテストで使う） | — | — |
| `shogun-core/src/audio/asr/mod.rs` | `trait Transcriber`。`WhisperCpp` 実装 | whisper-rs | fake で○ |
| `shogun-core/src/audio/asr/whisper.rs` | whisper-rs ラッパ（Metal・small/turbo 切替・言語自動判定） | whisper-rs | ゴールデン（`#[ignore]`） |
| `shogun-core/src/audio/worker.rs` | capture+VAD+ASR を束ね、発話 flush 毎に ASR をワーカスレッドで実行し DB 書込。実時間スレッドを塞がない | 上記全部 | fake Transcriber で○ |
| `shogun-core/src/audio/mod.rs` + meeting 配線 | `StartAudio`→worker 起動(session_id)、`StopAudio`→停止・drain | — | statemachine 連携テスト |

## 4. データモデル（V9 マイグレーション）

設計書§2.1のスキーマをそのまま採用（additive・後方互換）：

```sql
CREATE TABLE transcript_segments (
  id INTEGER PRIMARY KEY,
  session_id INTEGER NOT NULL REFERENCES sessions(id),
  ts INTEGER NOT NULL,               -- epoch ms（セグメント開始）
  speaker TEXT,                      -- 'me' | 'other' | NULL（不明はNULLで持つ。推測で埋めない）
  text TEXT NOT NULL,
  origin TEXT NOT NULL,              -- 'asr' | 'caption'（キャプション由来かを provenance に残す）
  confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_transcript_session ON transcript_segments (session_id, ts);
```

- `origin='asr'` を本設計で書く。`'caption'`（会議UIの字幕由来）は将来の別経路。
- `confidence` は whisper のセグメント平均対数確率を [0,1] に正規化して格納。
- `speaker` はキャプチャ音源で決定（mic=me / system=other）。不明は NULL、推測で埋めない。
- ロールバック手順（`docs/migrations/V9-rollback.md`）を必須添付（コミット規約）。

## 5. モデルと言語

- **既定 whisper small（量子化 gguf 同梱）**。設定で **large-v3-turbo**（高精度モード）に切替。切替は `meeting/settings.rs` に `asr_model` を追加。
- モデルは trait `Transcriber` の裏。将来 macOS 26+ の Apple SpeechAnalyzer を `AppleSpeech` として差し込める。
- 言語は whisper の**発話ごと自動判定**（言語ヒントは渡さない）。言語方針（`context-layer-audit-and-plan.md` §8）に従い、**英語を精度の主指標・日本語を回帰チェック**として評価。日本語対応が英語精度を落とさないこと。
- turbo は同梱で肥大するため、既定は small を同梱、turbo は初回オンデバイス取得（設定でオプトイン）とする。取得先・検証（ハッシュ）は実装計画で詰める。

## 6. ライフサイクル配線

statemachine（`meeting/statemachine.rs`）は変更しない。既存の Effect を消費する：

- `Effect::StartAudio` 受領 → 現在の `session_id` で `audio::worker` を起動。マイクを開き、14.4+ なら system tap も開く。
- `Effect::StopAudio` 受領 → worker に停止を指示し、リングバッファに残った最後の発話を drain して ASR→DB、その後 PCM を破棄。
- worker は `Recording` 状態からしか起動されない（FR-MT-12：音声レーンは Recording 以外から開けない）。この不変を配線側の型で守る（worker 起動関数を Recording 遷移ハンドラからのみ呼ぶ）。

## 7. エラー処理（キャプチャデーモンは絶対に落とさない）

| 事象 | 挙動 |
|---|---|
| マイク権限拒否 | Notch インジケータ色で通知。会議ノートは MT2 の「ノートだけ」に縮退。クラッシュしない |
| process tap 非対応（macOS 14.0〜14.3） | マイクのみに縮退。初回だけログ（内容は含めない）。機能は出す |
| system tap 権限（TCC）拒否 | マイクのみに縮退。UI で「相手音声は取得していない」ことが分かる表示 |
| ASR が処理落ち | リングバッファが古い音声から破棄（設計通り）。キャプチャスレッドは決してブロックしない |
| モデル欠損 / 破損 | 音声機能のみ無効化。セッションのノート記録・自動終了は継続 |
| whisper 実行時エラー | その発話をスキップし confidence 記録なし。セッションは継続 |

- `unwrap()` はテスト以外で使わない（clippy warnings deny）。
- DB 書込は WAL＋トランザクション。電源断で `transcript_segments` を壊さない。

## 8. テスト戦略

- **単体**
  - `ring.rs`：drop-oldest の 30s 上限、境界（ちょうど30s、超過）。
  - `vad.rs`：合成PCM（無音→発話→無音）で区間境界・最小発話長・最大30sクランプ。
  - `resample.rs`：既知サンプルレート→16k mono の長さ・値域。
  - `transcript_segments.rs`：in-memory DB で挿入・取得・CHECK制約違反。
- **パイプライン（モデル非依存）**：fake `Transcriber`（決定論的に固定テキストを返す）で `worker` の flush→DB 書込、speaker 付与、PCM 破棄を検証。
- **statemachine 連携**：`StartAudio` で worker 起動、`StopAudio` で停止＋drain（fake Transcriber）。Recording 以外から worker が起動しないことを型・テストで担保。
- **ゴールデン（`#[ignore]`／CIでモデルキャッシュ）**：英語＋日本語の極小 PCM フィクスチャ→whisper small→transcript 非空・期待語を含む。テストフィクスチャは合成またはライセンス明確な短尺（ユーザー音声ではない）。
- **実機**：実 macOS + 実 Zoom / Google Meet での取得・話者判別・14.4未満の縮退は手動。既存の AX プローブに倣い音声プローブ（`crates/shogun-core/tests` 配下）を追加。

## 9. 依存追加

- `whisper-rs`（whisper.cpp バインディング、Metal feature 有効）
- `cpal`（マイク取得）
- `objc2` / `core-audio-sys`（Core Audio process tap。既存の objc2 利用に合わせる）
- `rubato` 等（リサンプル）
- VAD：`webrtc-vad` クレート or 自前エネルギー判定（実装計画で確定）

いずれも Apple Silicon / macOS 14+ でビルド可能なこと、ライセンス（同梱可否）を実装計画で確認する。

## 10. SLO / 性能の観点

- キャプチャ・VAD・DB 書込は実時間スレッドをブロックしない（リングバッファで分離）。
- ASR はワーカスレッドで実行。アイドル時 CPU 5% の SLO は「会議中でない時」を指すが、会議中の CPU も計測しPR本文に p50/p95 を記載（SLO関連変更の規約）。
- Notch 展開100ms 等の既存 SLO に影響しないこと（音声レーンは別スレッド）。

## 11. オープン事項（実装計画で決める）

1. VAD の実装（クレート vs 自前）と閾値・最小発話長・ハングオーバ。
2. turbo モデルの取得方式（初回ダウンロード先・ハッシュ検証・保存場所＝Keychain対象外の静的アセット領域）。
3. whisper confidence の [0,1] 正規化式。→ **決着（2026-07-28）:** セグメントのトークン `token_probability()`（whisper-rs 0.16 で既に [0,1]）の平均を [0,1] にクランプして採用。special/timestamp トークンは `&WhisperSegment` から context の special-token id が参照できず（`get_state` が `pub(super)`）除外できないため、mean token probability を as-is で採用する。
4. system tap の TCC 権限の見え方（設計書§8-2）と 14.4未満縮退の UI 文言。
5. Recap 言語（設計書§8-4）は MT4 スコープだが、transcript の言語混在をどう持つか（segment 単位で言語を持たないか）を確認。
