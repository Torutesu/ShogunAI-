# Push-to-Talk 音声対話 実装計画（Issue #44）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ショートカットを長押ししている間だけマイクを開き、離すと文字起こし → 現在コンテキスト結合 → エージェント応答のストリーミング表示までを一息で走らせる入口を作る。

**Architecture:** 純ロジック（状態機械・テキスト連結・プロンプト構築・SSEデコード）を `shogun-core` に置き、副作用（NSEventフック・マイク・NSPanel・HTTP）は `apps/desktop/src-tauri` 側の実行層が Effect として処理する。会議ノート（`crates/shogun-core/src/meeting/statemachine.rs`）と同じ流儀で、コア側はLinux CIでもテストできる状態に保つ。

**Tech Stack:** Rust / Tauri v2 / objc2（NSEvent, NSPanel）/ whisper-rs（既存の `shogun_core::audio`）/ reqwest（SSEストリーミング）/ React + TypeScript

**設計書:** `docs/push-to-talk-voice-design.md`

---

## 前提知識（このリポジトリを知らない人向け）

**ワークスペース構成**

- `crates/shogun-core` — 純ロジック＋デーモン。feature gate: `db` / `net` / `exec` / `audio`。デフォルトは全部オフで、Linux上でも純ロジックのテストが走る
- `apps/desktop/src-tauri` — Tauriアプリ本体。**Cargoパッケージ名は `shogun-desktop-spike`**（ディレクトリ名と違うので注意）。`shogun-core` を `features = ["db","net","exec","audio"]` で使う
- `apps/desktop/src` — React + TypeScript のフロントエンド

**テストの走らせ方**

```bash
cargo test -p shogun-core                    # 純ロジック（audio backendなし）
cargo test -p shogun-core --features audio   # 実ASRバックエンド込み（macOSのみ）
cargo check -p shogun-desktop-spike          # デスクトップ側のビルド確認。必須
```

⚠️ `cargo test -p shogun-core` だけではデスクトップ側の破損を検知できない。crates側のAPIを触ったら**必ず** `cargo check -p shogun-desktop-spike` を通すこと。

**アプリの起動**

```bash
cd apps/desktop && pnpm dev     # tauri dev
```

**守るべき規約（CLAUDE.md より）**

- `unwrap()` はテスト以外で禁止。キャプチャデーモンを絶対に落とさない
- clippy warnings deny
- UI文言は英語。i18n-readyに保つ
- ログ・テレメトリにユーザーのキャプチャ内容や発話内容を含めない
- Conventional Commits（`feat:` / `fix:` / `docs:` / `perf:`）

**既存コードで模倣すべきパターン**

- 状態機械: `crates/shogun-core/src/meeting/statemachine.rs` — `State` / `Input` / `Effect` の3 enum、`Machine::step(&mut self, input) -> Vec<Effect>`、テストは同一ファイル内の `#[cfg(test)] mod tests`
- 音声レーン: `apps/desktop/src-tauri/src/audio_lane.rs` — モデル解決 → Whisperロード → Mic → Worker → poll-and-parkスレッド
- NSEventフック: `apps/desktop/src-tauri/src/lib.rs:1380` の `watch_option_tap` — flagsChanged の状態機械、poison方式
- パネル生成: `apps/desktop/src-tauri/src/meeting.rs:523` の `build_overlay` + `lib.rs:1317` の `float_on_all_spaces`

---

## ファイル構成

**新規作成**

| パス | 責務 |
|---|---|
| `crates/shogun-core/src/ptt/mod.rs` | PTTモジュールのルート。サブモジュール宣言のみ |
| `crates/shogun-core/src/ptt/statemachine.rs` | 5状態の純ロジック状態機械。マイクを開く唯一の経路を型で縛る |
| `crates/shogun-core/src/ptt/buffer_sink.rs` | `SegmentSink` 実装。文字起こしをRAM上で連結し、DBに書かない |
| `crates/shogun-core/src/ptt/prompt.rs` | 発話テキスト＋コンテキストからプロンプトを組む純関数 |
| `crates/shogun-core/src/llm/sse.rs` | チャンク境界をまたぐ増分SSEデコーダ |
| `apps/desktop/src-tauri/src/hold_monitor.rs` | NSEvent flagsChanged による素修飾キーの長押し検知 |
| `apps/desktop/src-tauri/src/ptt_lane.rs` | マイクのみの一発ASRレーン |
| `apps/desktop/src-tauri/src/ptt.rs` | Effect実行層。状態機械と全副作用の配線、Tauri command |
| `apps/desktop/src/PttOverlay.tsx` | 録音中／解析中／応答／エラーのパネルUI |

**変更**

| パス | 内容 |
|---|---|
| `crates/shogun-core/src/lib.rs` | `pub mod ptt;` を追加 |
| `crates/shogun-core/src/llm/mod.rs` | `pub mod sse;` を追加 |
| `crates/shogun-core/src/llm/transport.rs` | `StreamingTransport` trait と `ReqwestTransport` の実装を追加 |
| `crates/shogun-core/src/llm/anthropic.rs` | `AnthropicAgentClient::complete_streaming` を追加 |
| `apps/desktop/src-tauri/src/lib.rs` | モジュール宣言、setup配線、Tauri command登録 |
| `apps/desktop/src-tauri/src/audio_lane.rs` | モデル解決関数を `pub(crate)` にして共有 |
| `apps/desktop/src-tauri/src/analytics.rs` | 呼び出しのみ（変更不要の見込み） |
| `apps/desktop/src-tauri/Info.plist` | `NSMicrophoneUsageDescription` を追加 |
| `apps/desktop/src/App.tsx` | `ptt` ウィンドウで `PttOverlay` を描画する分岐 |
| `apps/desktop/src/fullui/FullUi.tsx` | Push-to-Talk 設定セクション |

---

## Task 1: ホットキー機構のスパイク（最優先・手戻り最大）

`tauri-plugin-global-shortcut` の `ShortcutState::Released` が、キーを押しっぱなしにしたときに確実に届くかを実測する。ここの結論で設定UIに出せる選択肢が変わるため、他のどのコードより先に潰す。

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs:1488-1511`（`register_expand_shortcut` に一時ログを足す）
- Create: `docs/ptt-hotkey-spike-findings.md`

- [ ] **Step 1: Released を含む全イベントをログに出す**

`apps/desktop/src-tauri/src/lib.rs` の `register_expand_shortcut` 内、既存の `on_shortcut` クロージャを次に置き換える。既存は `Pressed` のときだけ処理していて `Released` を捨てている。

```rust
    let res = app.global_shortcut().on_shortcut(expand, move |app, _sc, event| {
        // SPIKE (Issue #44): Pressed だけでなく Released も、到達時刻つきで観測する。
        // 押しっぱなしでの keyrepeat / フォーカス喪失で Released が落ちないかを見る。
        eprintln!(
            "[ptt-spike] shortcut {:?} at {:?}",
            event.state(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        if event.state() == ShortcutState::Pressed {
```

- [ ] **Step 2: 起動して観測する**

```bash
cd apps/desktop && pnpm dev
```

⌘⇧J を次の5パターンで操作し、`[ptt-spike]` 行をすべて記録する。

1. 短く押して離す
2. 3秒間押しっぱなしにして離す
3. 押しっぱなしのまま別アプリをクリックしてから離す
4. 押しっぱなしのまま ⌘ を先に離し、次に J を離す
5. 押しっぱなしのまま Mission Control を開いてから離す

期待: 各操作で `Pressed` が1回、`Released` が1回。

観測すべき失敗モード:
- `Released` が来ない
- `Pressed` がキーリピートで複数回来る
- 3・5 で `Released` が落ちる

- [ ] **Step 3: 結果を記録する**

`docs/ptt-hotkey-spike-findings.md` に、5パターンそれぞれの実測ログと結論を書く。結論は次の二択のどちらかを明記すること。

- **A: `Released` は信頼できる** → 設定UIで通常コンボの長押しを選択肢に含める
- **B: `Released` は信頼できない** → 設定UIは素修飾キー（右⌘ / 右⌥ / Fn）のみを提示する。Task 12 の設定UIをそれに合わせる

どちらの結論でも Task 4 の `hold_monitor`（素修飾キー用）は必要なので、後続タスクはブロックされない。

- [ ] **Step 4: スパイクのログを消してコミット**

Step 1 で足した `eprintln!("[ptt-spike] ...")` ブロックを削除し、`register_expand_shortcut` を元の形に戻す。

```bash
cargo check -p shogun-desktop-spike
git add docs/ptt-hotkey-spike-findings.md apps/desktop/src-tauri/src/lib.rs
git commit -m "docs: record push-to-talk hotkey release-event spike findings (#44)"
```

---

## Task 2: PTT状態機械（純ロジック）

マイクを開く経路を状態機械の中に閉じ込める。`StartCapture` は `HoldStart` からしか出ず、`Recording` から出る全経路が `StopCapture` か `DiscardCapture` を必ず伴う — これをテストで固定する。会議ノートの `FR-MT-12` テストと同じ考え方。

**Files:**
- Create: `crates/shogun-core/src/ptt/mod.rs`
- Create: `crates/shogun-core/src/ptt/statemachine.rs`
- Modify: `crates/shogun-core/src/lib.rs`

- [ ] **Step 1: モジュールを宣言する**

`crates/shogun-core/src/ptt/mod.rs` を作る。

```rust
//! Push-to-talk 音声対話（Issue #44）: ショートカットを長押ししている間だけマイクを開き、
//! 離した瞬間に文字起こし → コンテキスト結合 → エージェント応答までを一息で走らせる入口。
//!
//! ここにあるのは全て純ロジックで、マイクにもネットワークにも触らない。実際の副作用は
//! `apps/desktop/src-tauri/src/ptt.rs` の実行層が [`statemachine::Effect`] を解釈して行う。
//! 不変条件2の担保もここが要: 波形は `shogun_core::audio` のRAMバッファにしか存在せず、
//! [`buffer_sink::BufferSink`] は文字起こしテキストだけを受けてDBにも書かない。

pub mod buffer_sink;
pub mod prompt;
pub mod statemachine;
```

`crates/shogun-core/src/lib.rs` の `pub mod notch;` の直後に次の行を足す（アルファベット順の位置。`meeting` と `notch` の間ではなく `notch` の後で良い — 既存の並びも厳密な五十音順ではない）。

```rust
/// Push-to-talk 音声対話の純ロジック（Issue #44）。マイクを開く唯一の経路を状態機械が持つ。
pub mod ptt;
```

- [ ] **Step 2: 失敗するテストを書く**

`crates/shogun-core/src/ptt/statemachine.rs` を作り、テストだけ先に書く。

```rust
//! Push-to-talk のセッション機械（Issue #44）。
//!
//! ```text
//!         HoldStart                 HoldEnd (>= min_hold)
//!   Idle ───────────► Recording ─────────────────► Transcribing
//!    ▲                   │                              │
//!    │  Cancel / 短すぎるHold / MaxHold                  │ Transcribed
//!    │                   ▼                              ▼
//!    │◄──────────────(破棄)                        Responding
//!    │                                                  │
//!    └──────────── Dismiss / ResponseDone ──────────────┘
//! ```
//!
//! **マイクを開く入力は `HoldStart` ただ一つ。** そして `Recording` から出る全ての辺が
//! `StopCapture` か `DiscardCapture` を伴う。「録音が始まっていたのに気づかなかった」も
//! 「離したのにマイクが開きっぱなし」も、この機械では表現できない。テストが線を守る。

#[cfg(test)]
mod tests {
    use super::*;

    fn machine() -> Machine {
        Machine::new(Params::default())
    }

    /// 押した瞬間にマイクが開き、パネルが出て、上限タイマーが走る。
    #[test]
    fn hold_start_opens_the_microphone_and_shows_the_panel() {
        let mut m = machine();
        let fx = m.step(Input::HoldStart { at_ms: 1_000 });

        assert_eq!(m.state(), State::Recording);
        assert!(fx.contains(&Effect::StartCapture));
        assert!(fx.contains(&Effect::ShowPanel(Panel::Listening)));
        assert!(fx.contains(&Effect::PlaySound(Sound::Start)));
        assert!(fx.contains(&Effect::StartTimer { timer: Timer::MaxHold, ms: 30_000 }));
    }

    /// 誤爆（一瞬押しただけ）は録音を捨てて黙って戻る。エージェントには何も届かない。
    #[test]
    fn a_hold_shorter_than_the_minimum_is_discarded_silently() {
        let mut m = machine();
        m.step(Input::HoldStart { at_ms: 1_000 });
        let fx = m.step(Input::HoldEnd { at_ms: 1_100 }); // 100ms — 誤爆

        assert_eq!(m.state(), State::Idle);
        assert!(fx.contains(&Effect::DiscardCapture));
        assert!(fx.contains(&Effect::HidePanel));
        assert!(!fx.contains(&Effect::StopCapture), "破棄なので通常停止は出さない");
        assert!(
            !fx.iter().any(|e| matches!(e, Effect::PlaySound(_))),
            "誤爆で音を鳴らすと、押していないつもりの操作が音を立てる"
        );
    }

    /// 十分な長さのholdは文字起こしへ進む。
    #[test]
    fn a_real_hold_moves_to_transcribing() {
        let mut m = machine();
        m.step(Input::HoldStart { at_ms: 1_000 });
        let fx = m.step(Input::HoldEnd { at_ms: 3_000 });

        assert_eq!(m.state(), State::Transcribing);
        assert!(fx.contains(&Effect::StopCapture));
        assert!(fx.contains(&Effect::PlaySound(Sound::End)));
        assert!(fx.contains(&Effect::ShowPanel(Panel::Transcribing)));
        assert!(fx.contains(&Effect::CancelTimer(Timer::MaxHold)));
    }

    /// 押しっぱなしで放置してもマイクは閉じる。手を離す入力が永遠に来ない場合の保険。
    #[test]
    fn the_max_hold_timer_closes_the_microphone_on_its_own() {
        let mut m = machine();
        m.step(Input::HoldStart { at_ms: 0 });
        let fx = m.step(Input::MaxHoldExpired { at_ms: 30_000 });

        assert_eq!(m.state(), State::Transcribing);
        assert!(fx.contains(&Effect::StopCapture), "上限に達したらマイクは必ず閉じる");
    }

    /// Escでのキャンセルは録音を捨てる。送信はしない。
    #[test]
    fn cancel_discards_the_recording_without_submitting() {
        let mut m = machine();
        m.step(Input::HoldStart { at_ms: 1_000 });
        let fx = m.step(Input::Cancel);

        assert_eq!(m.state(), State::Idle);
        assert!(fx.contains(&Effect::DiscardCapture));
        assert!(fx.contains(&Effect::HidePanel));
        assert!(!fx.iter().any(|e| matches!(e, Effect::SubmitToAgent(_))));
    }

    /// 文字起こしが返ればエージェントへ送る。
    #[test]
    fn a_transcript_is_submitted_to_the_agent() {
        let mut m = machine();
        m.step(Input::HoldStart { at_ms: 1_000 });
        m.step(Input::HoldEnd { at_ms: 3_000 });
        let fx = m.step(Input::Transcribed("make a task for the review".into()));

        assert_eq!(m.state(), State::Responding);
        assert!(fx.contains(&Effect::SubmitToAgent("make a task for the review".into())));
        assert!(fx.contains(&Effect::ShowPanel(Panel::Responding)));
    }

    /// 無音を投げ込まない。空の文字起こしはエラーとして扱い、エージェントには渡さない。
    #[test]
    fn an_empty_transcript_never_reaches_the_agent() {
        let mut m = machine();
        m.step(Input::HoldStart { at_ms: 1_000 });
        m.step(Input::HoldEnd { at_ms: 3_000 });
        let fx = m.step(Input::Transcribed("   ".into()));

        assert_eq!(m.state(), State::Idle);
        assert!(!fx.iter().any(|e| matches!(e, Effect::SubmitToAgent(_))));
        assert!(fx.contains(&Effect::ShowPanel(Panel::Error(Fail::NothingHeard))));
    }

    /// 失敗は黙って消えない。理由を出してIdleへ戻る。
    #[test]
    fn a_failure_shows_a_reason_rather_than_vanishing() {
        let mut m = machine();
        m.step(Input::HoldStart { at_ms: 1_000 });
        m.step(Input::HoldEnd { at_ms: 3_000 });
        let fx = m.step(Input::Failed(Fail::Network));

        assert_eq!(m.state(), State::Idle);
        assert!(fx.contains(&Effect::ShowPanel(Panel::Error(Fail::Network))));
    }

    /// 応答表示中に押し直したら、前のセッションを捨てて新しい録音を始める。
    #[test]
    fn holding_again_while_responding_starts_a_fresh_session() {
        let mut m = machine();
        m.step(Input::HoldStart { at_ms: 1_000 });
        m.step(Input::HoldEnd { at_ms: 3_000 });
        m.step(Input::Transcribed("first".into()));
        let fx = m.step(Input::HoldStart { at_ms: 9_000 });

        assert_eq!(m.state(), State::Recording);
        assert!(fx.contains(&Effect::StartCapture));
    }

    /// 録音中の再Holdは無視する。多重セッションを作らない。
    #[test]
    fn a_second_hold_while_recording_is_ignored() {
        let mut m = machine();
        m.step(Input::HoldStart { at_ms: 1_000 });
        let fx = m.step(Input::HoldStart { at_ms: 1_500 });

        assert_eq!(m.state(), State::Recording);
        assert!(fx.is_empty(), "多重の StartCapture はマイクを二重に開く");
    }

    /// マイクを開く入力は HoldStart ただ一つ。他のどの入力からも StartCapture は出ない。
    #[test]
    fn only_hold_start_can_open_the_microphone() {
        let others = [
            Input::HoldEnd { at_ms: 5_000 },
            Input::MaxHoldExpired { at_ms: 5_000 },
            Input::Cancel,
            Input::Transcribed("x".into()),
            Input::Failed(Fail::Network),
            Input::ResponseDone,
            Input::Dismiss,
        ];
        for start in [State::Idle, State::Recording, State::Transcribing, State::Responding] {
            for input in others.iter() {
                let mut m = machine();
                m.force_state_for_test(start);
                let fx = m.step(input.clone());
                assert!(
                    !fx.contains(&Effect::StartCapture),
                    "{start:?} + {input:?} からマイクが開いた"
                );
            }
        }
    }

    /// Recording から出る全ての辺で、マイクは必ず閉じる。
    #[test]
    fn every_exit_from_recording_closes_the_microphone() {
        let exits = [
            Input::HoldEnd { at_ms: 9_000 },
            Input::MaxHoldExpired { at_ms: 30_000 },
            Input::Cancel,
            Input::Failed(Fail::MicUnavailable),
        ];
        for input in exits.iter() {
            let mut m = machine();
            m.step(Input::HoldStart { at_ms: 1_000 });
            let fx = m.step(input.clone());

            assert_ne!(m.state(), State::Recording, "{input:?} で Recording に留まった");
            assert!(
                fx.contains(&Effect::StopCapture) || fx.contains(&Effect::DiscardCapture),
                "{input:?} がマイクを開いたまま Recording を抜けた"
            );
        }
    }

    /// 想定外の入力でパニックしない。デーモンは落とせない（CLAUDE.md）。
    #[test]
    fn unexpected_inputs_are_no_ops() {
        let mut m = machine();
        let fx = m.step(Input::ResponseDone); // Idle で応答完了が来る道理はない
        assert_eq!(m.state(), State::Idle);
        assert!(fx.is_empty());
    }
}
```

- [ ] **Step 2b: テストが「コンパイルできずに」失敗することを確認する**

```bash
cargo test -p shogun-core ptt::statemachine
```

期待: `cannot find type 'Machine' in this scope` 等のコンパイルエラー。まだ本体を書いていないので当然。

- [ ] **Step 3: 状態機械を実装する**

`crates/shogun-core/src/ptt/statemachine.rs` の、先ほど書いたモジュールdocコメントと `#[cfg(test)] mod tests` の**間**に次を挿入する。

```rust
/// 実行層がスケジュールするタイマー。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timer {
    /// 1回の発話の上限。押しっぱなしで放置されてもマイクを閉じるための保険。
    MaxHold,
}

/// 開始・終了の音。macOSの短いシステムサウンドを想定し、鳴らす判断だけをここで持つ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Start,
    End,
}

/// セッションが立ち行かなくなった理由。ユーザーに見せる文言は実行層が決めるが、
/// 「何が起きたか」の語彙はここで閉じる。計測のエラーコードもこれと同じ粒度。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fail {
    /// マイクが使えない（権限拒否 / デバイスなし）。
    MicUnavailable,
    /// ASRモデルが無い、または読み込めない。
    NoAsrModel,
    /// 音は録れたが、文字起こしが空だった。
    NothingHeard,
    /// 文字起こし自体が失敗した。
    AsrFailed,
    /// ネットワークに出られない。
    Network,
    /// BYOKキーが拒否された、または設定されていない。
    KeyRejected,
}

impl Fail {
    /// 計測用の安定した文字列。**発話内容は絶対に含めない**（CLAUDE.md: テレメトリに
    /// キャプチャ内容を含めない）。
    pub fn code(self) -> &'static str {
        match self {
            Fail::MicUnavailable => "mic_unavailable",
            Fail::NoAsrModel => "no_asr_model",
            Fail::NothingHeard => "nothing_heard",
            Fail::AsrFailed => "asr_failed",
            Fail::Network => "network",
            Fail::KeyRejected => "key_rejected",
        }
    }
}

/// パネルが見せている中身。状態機械の `State` とは意図的に別。失敗表示は `Idle` に戻った
/// あとも画面に残るので、可視性を `State` に持たせると「エラーを出したまま次の録音ができない」
/// という嘘の制約が生まれる。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Listening,
    Transcribing,
    Responding,
    Error(Fail),
}

/// セッションの状態。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Recording,
    Transcribing,
    Responding,
}

impl State {
    pub fn tag(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Recording => "recording",
            State::Transcribing => "transcribing",
            State::Responding => "responding",
        }
    }
}

/// 機械への入力。`at_ms` は単調増加のミリ秒（実行層が `Instant` から作る）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// 設定されたキーが押し下がった。**マイクを開く唯一の入力。**
    HoldStart { at_ms: i64 },
    /// キーが離された。
    HoldEnd { at_ms: i64 },
    /// 上限に達した。手を離す入力が来ないまま放置された場合の保険。
    MaxHoldExpired { at_ms: i64 },
    /// Esc、またはパネルのキャンセル操作。
    Cancel,
    /// 文字起こしが返った。空文字は `Fail::NothingHeard` として扱われる。
    Transcribed(String),
    /// どこかで失敗した。
    Failed(Fail),
    /// 応答が最後まで届いた。
    ResponseDone,
    /// ユーザーがパネルを閉じた。
    Dismiss,
}

/// 実行層が順番どおりに実行する副作用。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Transition(State),
    /// マイクを開いてASRレーンを起動する。
    StartCapture,
    /// 録音を止めて、溜まった文字起こしを取り出す。
    StopCapture,
    /// 録音を止めて、溜まったものを**捨てる**。誤爆とキャンセルの道。
    DiscardCapture,
    PlaySound(Sound),
    ShowPanel(Panel),
    HidePanel,
    /// 文字起こしテキストをコンテキストと合わせてエージェントへ。
    SubmitToAgent(String),
    StartTimer { timer: Timer, ms: u64 },
    CancelTimer(Timer),
}

/// タイミング定数。仕様値をコードからgrepできるよう、インライン化せず名前を付ける。
#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// これより短いholdは誤爆として捨てる。
    pub min_hold_ms: i64,
    /// 1回の発話の上限。設計書の「最大30秒」。
    pub max_hold_ms: u64,
}

impl Default for Params {
    fn default() -> Self {
        Self { min_hold_ms: 300, max_hold_ms: 30_000 }
    }
}

/// セッション機械。現在状態と、進行中のholdの開始時刻だけを持つ。
#[derive(Debug)]
pub struct Machine {
    state: State,
    params: Params,
    /// 進行中のholdが始まった時刻。`Recording` のときだけ `Some`。
    hold_started_at: Option<i64>,
}

impl Machine {
    pub fn new(params: Params) -> Self {
        Self { state: State::Idle, params, hold_started_at: None }
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// テスト専用: 任意の状態から始める。プロダクションコードから呼ばない。
    #[cfg(test)]
    fn force_state_for_test(&mut self, state: State) {
        self.state = state;
        self.hold_started_at = (state == State::Recording).then_some(0);
    }

    /// 入力を適用し、実行すべき副作用を順番に返す。
    pub fn step(&mut self, input: Input) -> Vec<Effect> {
        use Input as I;
        use State as S;

        match (self.state, input) {
            // ── 録音開始 ────────────────────────────────────────────────────────────────
            // Idle からも Responding からも押せる。応答を読んでいる途中で次を思いついたら、
            // パネルを閉じる操作を挟ませない。ただし Recording 中の再Holdは無視（下の腕）。
            (S::Idle | S::Transcribing | S::Responding, I::HoldStart { at_ms }) => {
                self.state = S::Recording;
                self.hold_started_at = Some(at_ms);
                vec![
                    Effect::Transition(S::Recording),
                    Effect::StartCapture,
                    Effect::PlaySound(Sound::Start),
                    Effect::ShowPanel(Panel::Listening),
                    Effect::StartTimer { timer: Timer::MaxHold, ms: self.params.max_hold_ms },
                ]
            }

            // ── 録音終了 ────────────────────────────────────────────────────────────────
            // 手を離すのと上限に達するのは同じ遷移。上限は「離す入力が来ない」場合の保険で
            // あって、別の終わり方ではない。
            (S::Recording, I::HoldEnd { at_ms } | I::MaxHoldExpired { at_ms }) => {
                let held = at_ms - self.hold_started_at.unwrap_or(at_ms);
                self.hold_started_at = None;
                if held < self.params.min_hold_ms {
                    // 誤爆。音も鳴らさずに消える。押したつもりのない操作が痕跡を残さない。
                    self.state = S::Idle;
                    vec![
                        Effect::CancelTimer(Timer::MaxHold),
                        Effect::Transition(S::Idle),
                        Effect::DiscardCapture,
                        Effect::HidePanel,
                    ]
                } else {
                    self.state = S::Transcribing;
                    vec![
                        Effect::CancelTimer(Timer::MaxHold),
                        Effect::Transition(S::Transcribing),
                        Effect::StopCapture,
                        Effect::PlaySound(Sound::End),
                        Effect::ShowPanel(Panel::Transcribing),
                    ]
                }
            }

            // ── キャンセル ──────────────────────────────────────────────────────────────
            (S::Recording, I::Cancel) => {
                self.hold_started_at = None;
                self.state = S::Idle;
                vec![
                    Effect::CancelTimer(Timer::MaxHold),
                    Effect::Transition(S::Idle),
                    Effect::DiscardCapture,
                    Effect::HidePanel,
                ]
            }
            (S::Transcribing | S::Responding, I::Cancel | I::Dismiss) => {
                self.state = S::Idle;
                vec![Effect::Transition(S::Idle), Effect::HidePanel]
            }
            // Idle での Dismiss は、失敗表示を閉じる操作。
            (S::Idle, I::Dismiss) => vec![Effect::HidePanel],

            // ── 文字起こしが返った ──────────────────────────────────────────────────────
            (S::Transcribing, I::Transcribed(text)) => {
                if text.trim().is_empty() {
                    // 無音や雑音だけの録音。空プロンプトをエージェントに投げても金と時間を
                    // 捨てるだけなので、ここで止める。
                    self.state = S::Idle;
                    vec![
                        Effect::Transition(S::Idle),
                        Effect::ShowPanel(Panel::Error(Fail::NothingHeard)),
                    ]
                } else {
                    self.state = S::Responding;
                    vec![
                        Effect::Transition(S::Responding),
                        Effect::ShowPanel(Panel::Responding),
                        Effect::SubmitToAgent(text),
                    ]
                }
            }

            // ── 失敗 ────────────────────────────────────────────────────────────────────
            // Recording 中の失敗（マイクが落ちた等）も、必ず録音を捨ててから戻る。
            (S::Recording, I::Failed(why)) => {
                self.hold_started_at = None;
                self.state = S::Idle;
                vec![
                    Effect::CancelTimer(Timer::MaxHold),
                    Effect::Transition(S::Idle),
                    Effect::DiscardCapture,
                    Effect::ShowPanel(Panel::Error(why)),
                ]
            }
            (S::Transcribing | S::Responding, I::Failed(why)) => {
                self.state = S::Idle;
                vec![Effect::Transition(S::Idle), Effect::ShowPanel(Panel::Error(why))]
            }
            // マイクを開こうとした時点で失敗するケース（権限拒否）。Idle のまま理由を出す。
            (S::Idle, I::Failed(why)) => vec![Effect::ShowPanel(Panel::Error(why))],

            // ── 応答完了 ────────────────────────────────────────────────────────────────
            // パネルは開いたまま。読み終わったユーザーが閉じるか、次のholdが上書きする。
            (S::Responding, I::ResponseDone) => {
                self.state = S::Idle;
                vec![Effect::Transition(S::Idle)]
            }

            // それ以外は無操作。遅れて届いたタイマーや二重のHoldStartで機械が動いてはならず、
            // 想定外の入力でパニックする余裕もない（CLAUDE.md: デーモンは落とさない）。
            _ => Vec::new(),
        }
    }
}
```

- [ ] **Step 4: テストが通ることを確認する**

```bash
cargo test -p shogun-core ptt::statemachine
```

期待: 13テスト全部 PASS。

```bash
cargo clippy -p shogun-core --all-targets -- -D warnings
```

期待: warning なし。

- [ ] **Step 5: コミット**

```bash
git add crates/shogun-core/src/ptt/mod.rs crates/shogun-core/src/ptt/statemachine.rs crates/shogun-core/src/lib.rs
git commit -m "feat(core): push-to-talk session machine (#44)

The microphone can only be opened by HoldStart, and every exit from
Recording carries StopCapture or DiscardCapture. Both are held by tests."
```

---

## Task 3: BufferSink（文字起こしをRAMで受ける）

会議レーンの `DbSink`（`apps/desktop/src-tauri/src/audio_lane.rs:43-60`）はDBに書く。PTTはDBに書かない — 文字起こしを連結してその場でエージェントに渡し、あとは捨てる。

**Files:**
- Create: `crates/shogun-core/src/ptt/buffer_sink.rs`
- Modify: `crates/shogun-core/src/ptt/mod.rs`（宣言済みなので変更不要）

- [ ] **Step 1: 失敗するテストを書く**

`crates/shogun-core/src/ptt/buffer_sink.rs` を作る。

```rust
//! 一発の発話を組み立てる [`SegmentSink`]（Issue #44）。
//!
//! 会議レーンの sink は文字起こしをDBに追記するが、push-to-talk はそれをしない。1回の
//! 発話は1回のプロンプトになって消える寿命のもので、`sessions` に残す interval も無い。
//! ここが不変条件2の最後の関門でもある: 波形は `Worker` のバッファにしか無く、この型が
//! 受け取るのは既にテキストになったものだけで、それもディスクには落ちない。

use crate::audio::worker::SegmentSink;
use crate::audio::Utterance;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Speaker;

    fn utterance(at: i64) -> Utterance {
        // pcm は sink が見ないので空で良い。sink がテキストしか触らないことの裏返し。
        Utterance { speaker: Speaker::Me, started_at: at, pcm: Vec::new() }
    }

    #[test]
    fn segments_are_joined_in_arrival_order() {
        let mut sink = BufferSink::new();
        sink.emit(&utterance(0), "make a task", 0.9);
        sink.emit(&utterance(1_000), "for the review", 0.8);

        assert_eq!(sink.take(), "make a task for the review");
    }

    /// whisperは無音区間に空文字や空白だけのセグメントを返すことがある。連結時に
    /// 二重スペースを作らせない。
    #[test]
    fn blank_segments_do_not_leave_gaps() {
        let mut sink = BufferSink::new();
        sink.emit(&utterance(0), "hello", 0.9);
        sink.emit(&utterance(500), "   ", 0.1);
        sink.emit(&utterance(1_000), "", 0.0);
        sink.emit(&utterance(1_500), "world", 0.9);

        assert_eq!(sink.take(), "hello world");
    }

    /// 何も聞き取れなかった録音は空を返す。状態機械側の `NothingHeard` の入口。
    #[test]
    fn a_silent_recording_yields_nothing() {
        let mut sink = BufferSink::new();
        sink.emit(&utterance(0), "  ", 0.0);

        assert_eq!(sink.take(), "");
    }

    /// take はバッファを空にする。次のセッションに前回の発話が混ざらない。
    #[test]
    fn take_empties_the_buffer() {
        let mut sink = BufferSink::new();
        sink.emit(&utterance(0), "first", 0.9);
        assert_eq!(sink.take(), "first");

        sink.emit(&utterance(1_000), "second", 0.9);
        assert_eq!(sink.take(), "second", "前回の発話が残っていた");
    }

    /// discard は take と同様に空にするが、中身を返さない。誤爆・キャンセルの道。
    #[test]
    fn discard_drops_everything() {
        let mut sink = BufferSink::new();
        sink.emit(&utterance(0), "never mind", 0.9);
        sink.discard();

        assert_eq!(sink.take(), "");
    }
}
```

- [ ] **Step 2: テストが失敗することを確認する**

```bash
cargo test -p shogun-core ptt::buffer_sink
```

期待: `cannot find type 'BufferSink' in this scope` でコンパイルエラー。

- [ ] **Step 3: 実装する**

`crates/shogun-core/src/ptt/buffer_sink.rs` の `use` 行と `#[cfg(test)] mod tests` の間に挿入する。

```rust
/// 1セッション分の文字起こしを溜める sink。`Worker` のポーリングスレッドが `emit` を呼び、
/// セッションを閉じる側が [`take`](Self::take) か [`discard`](Self::discard) を呼ぶ。
#[derive(Debug, Default)]
pub struct BufferSink {
    text: String,
}

impl BufferSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// 溜まった発話を取り出し、バッファを空にする。次のセッションに持ち越さない。
    pub fn take(&mut self) -> String {
        std::mem::take(&mut self.text)
    }

    /// 溜まったものを捨てる。誤爆とキャンセルはここを通る。
    pub fn discard(&mut self) {
        self.text.clear();
    }
}

impl SegmentSink for BufferSink {
    fn emit(&mut self, _u: &Utterance, text: &str, _confidence: f64) {
        // 話者は見ない: push-to-talk はマイクだけを開くので、全て `Speaker::Me` になる。
        // confidence も見ない — 低確度でも「聞き間違えたテキスト」を見せる方が、黙って
        // 落として無反応になるよりましで、間違いはユーザーが読めば分かる。
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if !self.text.is_empty() {
            self.text.push(' ');
        }
        self.text.push_str(text);
    }
}
```

- [ ] **Step 4: テストが通ることを確認する**

```bash
cargo test -p shogun-core ptt::buffer_sink
cargo clippy -p shogun-core --all-targets -- -D warnings
```

期待: 5テスト PASS、warning なし。

- [ ] **Step 5: コミット**

```bash
git add crates/shogun-core/src/ptt/buffer_sink.rs
git commit -m "feat(core): buffer transcript segments in RAM for push-to-talk (#44)"
```

---

## Task 4: プロンプト構築（純関数）

発話テキストに、いま画面にあるものと確度ゲート済みの事実を添えてプロンプトにする。`ReplyContext` は `db` feature の裏にあるので、この関数は**受け取る形を平文のデータに絞って** feature gate なしに保つ。呼び出し側（デスクトップ）が `ReplyContext` からこの形に詰め替える。

**Files:**
- Create: `crates/shogun-core/src/ptt/prompt.rs`

- [ ] **Step 1: 失敗するテストを書く**

`crates/shogun-core/src/ptt/prompt.rs` を作る。

```rust
//! 発話 + いまの画面 → プロンプト（Issue #44）。
//!
//! `ReplyContext` を直接受けない。あれは `db` feature の裏にいて、この変換自体はDBを必要と
//! しないから — 詰め替えは呼び出し側の仕事にして、ここはLinuxでもテストできる純関数に保つ。
//!
//! 添えるのは**すでに確度ゲートを通った事実**だけ。低確度の推測をここで事実として混ぜない
//! （CLAUDE.md: 低confidenceの状態を生成物に混ぜてはならない）。

/// プロンプトに添える、いまの状況。全て省略可能で、何も無ければ発話だけを投げる。
#[derive(Debug, Default, Clone)]
pub struct Spoken<'a> {
    /// 前面アプリの表示名。
    pub app: Option<&'a str>,
    /// 前面ウィンドウのタイトル。
    pub window_title: Option<&'a str>,
    /// 確度ゲート済みの事実。`ReplyContext::facts` がそのまま入る。
    pub facts: &'a [String],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spoken_words_are_always_present() {
        let out = build_prompt("summarise this page", &Spoken::default());
        assert!(out.contains("summarise this page"));
    }

    /// コンテキストが何も無くても成立する。キャッシュが冷えているのを待たない、という
    /// 設計判断がここに出る。
    #[test]
    fn no_context_still_produces_a_usable_prompt() {
        let out = build_prompt("what time is the standup", &Spoken::default());

        assert!(out.contains("what time is the standup"));
        assert!(!out.contains("On screen"), "空のコンテキスト見出しを出さない");
        assert!(!out.contains("Known"), "空の事実見出しを出さない");
    }

    #[test]
    fn the_foreground_window_is_included_when_known() {
        let ctx = Spoken { app: Some("Safari"), window_title: Some("Q3 plan"), ..Spoken::default() };
        let out = build_prompt("summarise this", &ctx);

        assert!(out.contains("Safari"));
        assert!(out.contains("Q3 plan"));
    }

    #[test]
    fn confidence_gated_facts_are_included() {
        let facts = vec!["You owe Aya a reply on the Q3 plan".to_string()];
        let ctx = Spoken { facts: &facts, ..Spoken::default() };
        let out = build_prompt("what do I owe", &ctx);

        assert!(out.contains("You owe Aya a reply on the Q3 plan"));
    }

    /// 事実が多すぎるとプロンプトが膨らんで初トークンが遅れる。上限で切る。
    #[test]
    fn the_fact_list_is_bounded() {
        let facts: Vec<String> = (0..50).map(|i| format!("fact number {i}")).collect();
        let ctx = Spoken { facts: &facts, ..Spoken::default() };
        let out = build_prompt("go", &ctx);

        assert!(out.contains("fact number 0"));
        assert!(!out.contains("fact number 20"), "上限を超えた事実が入っている");
    }

    /// 応答は読み上げられる可能性があり、パネルも小さい。短く答えるよう明示する。
    #[test]
    fn the_instruction_asks_for_a_short_spoken_style_answer() {
        let out = build_prompt("what is this", &Spoken::default());
        assert!(out.to_lowercase().contains("brief"));
    }
}
```

- [ ] **Step 2: テストが失敗することを確認する**

```bash
cargo test -p shogun-core ptt::prompt
```

期待: `cannot find function 'build_prompt' in this scope`。

- [ ] **Step 3: 実装する**

`Spoken` 構造体の定義と `#[cfg(test)] mod tests` の間に挿入する。

```rust
/// プロンプトに載せる事実の上限。多いほど賢くなるわけではなく、初トークンまでの時間だけが
/// 確実に伸びる（SLO: 初トークン1s）。
const MAX_FACTS: usize = 12;

/// 発話と状況からプロンプトを組む。
///
/// 見出しは中身があるときだけ出す。空の "On screen:" が並ぶプロンプトは、モデルに
/// 「情報が無い」ではなく「情報を探せ」と読ませてしまう。
pub fn build_prompt(spoken: &str, ctx: &Spoken<'_>) -> String {
    let mut out = String::with_capacity(spoken.len() + 512);

    out.push_str(
        "You are SHOGUN, answering a question the user just spoke aloud while working. \
         Keep the answer brief and plain — it is shown in a small panel next to their work, \
         and may be read aloud. No preamble, no restating the question.\n\n",
    );

    match (ctx.app, ctx.window_title) {
        (Some(app), Some(title)) => {
            out.push_str(&format!("On screen: {app} — {title}\n"));
        }
        (Some(app), None) => out.push_str(&format!("On screen: {app}\n")),
        (None, Some(title)) => out.push_str(&format!("On screen: {title}\n")),
        (None, None) => {}
    }

    let facts: Vec<&String> = ctx.facts.iter().take(MAX_FACTS).collect();
    if !facts.is_empty() {
        out.push_str("Known about their work:\n");
        for f in facts {
            out.push_str("- ");
            out.push_str(f);
            out.push('\n');
        }
    }

    out.push_str("\nThey said: ");
    out.push_str(spoken.trim());
    out.push('\n');
    out
}
```

- [ ] **Step 4: テストが通ることを確認する**

```bash
cargo test -p shogun-core ptt::
cargo clippy -p shogun-core --all-targets -- -D warnings
```

期待: ptt配下の全テスト（状態機械13 + sink 5 + prompt 6）が PASS。

- [ ] **Step 5: コミット**

```bash
git add crates/shogun-core/src/ptt/prompt.rs
git commit -m "feat(core): build the push-to-talk prompt from speech plus screen context (#44)"
```

---

## Task 5: 増分SSEデコーダ

`crates/shogun-core/src/llm/anthropic.rs:313` の `parse_sse_text` は**ボディ全体**を受け取ってから解析する。初トークンを1秒以内に出すには、届いたチャンクを片端から解析する必要がある。チャンクは行の途中で切れるので、持ち越しバッファを持つデコーダを作る。

**Files:**
- Create: `crates/shogun-core/src/llm/sse.rs`
- Modify: `crates/shogun-core/src/llm/mod.rs`

- [ ] **Step 1: モジュールを宣言する**

`crates/shogun-core/src/llm/mod.rs` の既存の `pub mod` 群の末尾に足す。

```rust
/// チャンク境界をまたぐ増分SSEデコーダ（SLO-03の初トークン計測に必要）。
pub mod sse;
```

- [ ] **Step 2: 失敗するテストを書く**

`crates/shogun-core/src/llm/sse.rs` を作る。

```rust
//! 届いた端からテキストを取り出すSSEデコーダ。
//!
//! [`super::anthropic::parse_sse_text`] はボディが揃ってから解析するので、最初の一文字が
//! 出るのは応答が終わったあと — 「初トークン1s」というSLOはそれでは測りようがない。この型は
//! 同じイベント形（`content_block_delta` の `text_delta`）を、チャンクが行の途中で切れても
//! 落とさずに読む。
//!
//! ネットワークには触らない純ロジックなので、feature gate も要らずLinuxでテストできる。

#[cfg(test)]
mod tests {
    use super::*;

    const DELTA_A: &str =
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n";
    const DELTA_B: &str =
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n";

    #[test]
    fn a_complete_event_yields_its_text() {
        let mut d = SseDecoder::new();
        assert_eq!(d.push(DELTA_A), vec!["Hel".to_string()]);
    }

    /// 一度の push に複数イベントが入っていても全部返す。
    #[test]
    fn several_events_in_one_chunk_all_come_out() {
        let mut d = SseDecoder::new();
        let both = format!("{DELTA_A}{DELTA_B}");
        assert_eq!(d.push(&both), vec!["Hel".to_string(), "lo".to_string()]);
    }

    /// これがこの型の存在理由: 行の途中で切れたチャンクを落とさない。
    #[test]
    fn an_event_split_across_chunks_is_not_lost() {
        let mut d = SseDecoder::new();
        let (head, tail) = DELTA_A.split_at(30);

        assert!(d.push(head).is_empty(), "行が完成する前に何か返している");
        assert_eq!(d.push(tail), vec!["Hel".to_string()]);
    }

    /// text_delta 以外のイベント（ping, message_start, content_block_stop …）は無視する。
    #[test]
    fn non_text_events_are_ignored() {
        let mut d = SseDecoder::new();
        let noise = "event: ping\ndata: {\"type\":\"ping\"}\n\n\
                     data: {\"type\":\"message_start\",\"message\":{}}\n\n";
        assert!(d.push(noise).is_empty());
    }

    #[test]
    fn the_done_sentinel_is_ignored() {
        let mut d = SseDecoder::new();
        assert!(d.push("data: [DONE]\n\n").is_empty());
    }

    /// 壊れたJSONで止まらない。1行落として次へ進む。
    #[test]
    fn malformed_json_does_not_stop_the_stream() {
        let mut d = SseDecoder::new();
        assert!(d.push("data: {not json\n\n").is_empty());
        assert_eq!(d.push(DELTA_A), vec!["Hel".to_string()]);
    }

    /// 既存の parse_sse_text と同じ結果に落ち着く。二つの実装が食い違わないことの確認。
    #[test]
    fn the_incremental_result_matches_the_whole_body_parser() {
        let body = format!("{DELTA_A}{DELTA_B}data: [DONE]\n\n");
        let mut d = SseDecoder::new();
        let incremental: String = d.push(&body).concat();

        assert_eq!(incremental, super::super::anthropic::parse_sse_text(&body));
    }
}
```

- [ ] **Step 3: テストが失敗することを確認する**

```bash
cargo test -p shogun-core llm::sse
```

期待: `cannot find type 'SseDecoder' in this scope`。

- [ ] **Step 4: 実装する**

`crates/shogun-core/src/llm/sse.rs` のモジュールdocコメントと `#[cfg(test)] mod tests` の間に挿入する。

```rust
use serde_json::Value;

/// 到着順にSSEを読み、テキストデルタだけを吐くデコーダ。1本のストリームにつき1つ作る。
#[derive(Debug, Default)]
pub struct SseDecoder {
    /// まだ行として完成していない末尾。次のチャンクの頭と繋がる。
    pending: String,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// ボディのチャンクを1つ食わせ、この時点で確定したテキストデルタを順番に返す。
    ///
    /// 行が完成していない部分は内部に持ち越す。チャンクが `"data: {\"ty"` で切れても、次の
    /// push で残りと連結してから解釈されるので、デルタは失われない。
    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.pending.push_str(chunk);
        let mut out = Vec::new();

        // 最後の改行までを行として処理し、その先は次回に持ち越す。改行が無ければ何もしない。
        let Some(cut) = self.pending.rfind('\n') else { return out };
        let complete: String = self.pending.drain(..=cut).collect();

        for line in complete.lines() {
            let line = line.trim_start();
            let Some(data) = line.strip_prefix("data:") else { continue };
            let data = data.trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            // 壊れた1行でストリーム全体を落とさない。落とすのはその行だけ。
            let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
            if v.get("type").and_then(Value::as_str) != Some("content_block_delta") {
                continue;
            }
            let Some(delta) = v.get("delta") else { continue };
            if delta.get("type").and_then(Value::as_str) != Some("text_delta") {
                continue;
            }
            if let Some(t) = delta.get("text").and_then(Value::as_str) {
                if !t.is_empty() {
                    out.push(t.to_string());
                }
            }
        }
        out
    }
}
```

- [ ] **Step 5: テストが通ることを確認する**

```bash
cargo test -p shogun-core llm::sse
cargo clippy -p shogun-core --all-targets -- -D warnings
```

期待: 7テスト PASS、warning なし。

- [ ] **Step 6: コミット**

```bash
git add crates/shogun-core/src/llm/sse.rs crates/shogun-core/src/llm/mod.rs
git commit -m "feat(core): incremental SSE decoder for first-token latency (#44)"
```

---

## Task 6: ストリーミングトランスポート

`HttpTransport::send`（`crates/shogun-core/src/llm/transport.rs:109`）はボディを最後まで読んでから返す。ここに増分の経路を足す。既存traitは触らない — `MockTransport` を含む全実装が壊れるし、ストリーミングが必要なのは Agent lane だけで Batch lane には要らないから。

**Files:**
- Modify: `crates/shogun-core/src/llm/transport.rs`

- [ ] **Step 1: 失敗するテストを書く**

`crates/shogun-core/src/llm/transport.rs` の `#[cfg(test)] mod tests` の中、`debug_redacts_api_key_header` テストの直後に足す。

```rust
    /// ストリーミング用のモックが、渡された順にチャンクを送り出すこと。
    #[tokio::test]
    async fn mock_streaming_transport_delivers_chunks_in_order() {
        let t = MockStreamingTransport::new(200, vec!["one ".into(), "two".into()]);
        let (tx, rx) = std::sync::mpsc::channel();
        let req =
            HttpRequest::new(Method::Post, "https://api.anthropic.com/v1/messages", vec![], None)
                .unwrap();

        let status = t.send_streaming(req, tx).await.unwrap();

        assert_eq!(status, 200);
        let got: Vec<String> = rx.into_iter().collect();
        assert_eq!(got, vec!["one ".to_string(), "two".to_string()]);
    }

    /// 受け手が先に消えた（ユーザーがパネルを閉じた）場合、送信側はエラーにせず静かに終わる。
    /// 応答の途中で閉じるのは正常な操作で、失敗として記録するものではない。
    #[tokio::test]
    async fn a_dropped_receiver_ends_the_stream_without_an_error() {
        let t = MockStreamingTransport::new(200, vec!["one".into(), "two".into()]);
        let (tx, rx) = std::sync::mpsc::channel();
        drop(rx);
        let req =
            HttpRequest::new(Method::Post, "https://api.anthropic.com/v1/messages", vec![], None)
                .unwrap();

        assert!(t.send_streaming(req, tx).await.is_ok());
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

```bash
cargo test -p shogun-core llm::transport
```

期待: `cannot find type 'MockStreamingTransport' in this scope`。

- [ ] **Step 3: trait とモックを実装する**

`crates/shogun-core/src/llm/transport.rs` の `pub trait HttpTransport { ... }` ブロック（行109-114）の直後に挿入する。

```rust
/// 増分でボディを受け取るトランスポート。
///
/// [`HttpTransport`] と分けてある。全実装に streaming を強いるとモックもBatch lane側も
/// 巻き添えになるが、増分が要るのはAgent laneのSSEだけで、しかもそこは「最初の一文字までの
/// 時間」がSLOになっている唯一の経路だから。
///
/// チャンクは `std::sync::mpsc` で渡す。受け手（パネルへ流す側）はtokioの外にいる同期スレッド
/// なので、非同期チャネルにすると受け手側にランタイムを持ち込むことになる。
pub trait StreamingTransport: Send + Sync {
    /// `req` を送り、ボディを届いた順に `chunks` へ流す。返るのはHTTPステータス。
    ///
    /// 受け手が先に落ちた場合はエラーにせず、送信を打ち切って `Ok` で戻る — 応答の途中で
    /// パネルを閉じるのは正常な操作であって、失敗ではない。
    fn send_streaming(
        &self,
        req: HttpRequest,
        chunks: std::sync::mpsc::Sender<String>,
    ) -> impl Future<Output = Result<u16, TransportError>> + Send;
}

/// 決められたチャンクを順に流すだけのテスト用トランスポート。ネットワーク無しで
/// ストリーミング経路を検証するための土台。
pub struct MockStreamingTransport {
    status: u16,
    chunks: std::sync::Mutex<std::collections::VecDeque<String>>,
}

impl MockStreamingTransport {
    pub fn new(status: u16, chunks: Vec<String>) -> Self {
        Self { status, chunks: std::sync::Mutex::new(chunks.into()) }
    }
}

impl StreamingTransport for MockStreamingTransport {
    fn send_streaming(
        &self,
        _req: HttpRequest,
        chunks: std::sync::mpsc::Sender<String>,
    ) -> impl Future<Output = Result<u16, TransportError>> + Send {
        let queued: Vec<String> = self
            .chunks
            .lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default();
        let status = self.status;
        async move {
            for c in queued {
                // 受け手が消えていたら打ち切る。閉じたパネルに向かって流し続けない。
                if chunks.send(c).is_err() {
                    break;
                }
            }
            Ok(status)
        }
    }
}
```

- [ ] **Step 4: ReqwestTransport に実装する**

`impl HttpTransport for ReqwestTransport { ... }` ブロックの直後に挿入する。

```rust
#[cfg(feature = "net")]
impl StreamingTransport for ReqwestTransport {
    fn send_streaming(
        &self,
        req: HttpRequest,
        chunks: std::sync::mpsc::Sender<String>,
    ) -> impl Future<Output = Result<u16, TransportError>> + Send {
        let client = self.client.clone();
        async move {
            let method = match req.method {
                Method::Get => reqwest::Method::GET,
                Method::Post => reqwest::Method::POST,
            };
            let mut rb = client.request(method, &req.url);
            for (k, v) in &req.headers {
                rb = rb.header(k, v);
            }
            if let Some(body) = req.body {
                rb = rb.body(body);
            }
            let mut resp = rb.send().await.map_err(|e| TransportError::Io(e.to_string()))?;
            let status = resp.status().as_u16();
            while let Some(bytes) =
                resp.chunk().await.map_err(|e| TransportError::Io(e.to_string()))?
            {
                // 不正なUTF-8で切らない: SSEのテキストは常にUTF-8で、マルチバイト文字が
                // チャンク境界にかかった分は from_utf8_lossy が置換文字にする。デコーダ側の
                // 持ち越しバッファと合わせて、実害が出るのは境界にかかった1文字だけ。
                let s = String::from_utf8_lossy(&bytes).into_owned();
                if chunks.send(s).is_err() {
                    break;
                }
            }
            Ok(status)
        }
    }
}
```

- [ ] **Step 5: テストが通ることを確認する**

```bash
cargo test -p shogun-core llm::transport
cargo test -p shogun-core --features net llm::
cargo clippy -p shogun-core --all-targets --features net -- -D warnings
```

期待: 全 PASS、warning なし。

- [ ] **Step 6: コミット**

```bash
git add crates/shogun-core/src/llm/transport.rs
git commit -m "feat(core): streaming transport seam for incremental SSE bodies (#44)"
```

---

## Task 7: Anthropic のストリーミング応答

`AnthropicAgentClient::complete`（`crates/shogun-core/src/llm/anthropic.rs:366`）は既に `stream: true` でリクエストしているが、ボディが揃うまで待って `parse_sse_text` に渡している。Task 5・6 で作った部品を繋いで、届いた端から返す経路を足す。既存の `complete` は残す — 非ストリーミングで良い呼び出し元（inline draft）がそのまま動く。

**Files:**
- Modify: `crates/shogun-core/src/llm/anthropic.rs`

- [ ] **Step 1: 失敗するテストを書く**

`crates/shogun-core/src/llm/anthropic.rs` の `#[cfg(test)] mod tests` の末尾に足す。

```rust
    /// ストリーミング経路が、届いた端からテキストを流すこと。
    #[tokio::test]
    async fn streaming_completion_emits_deltas_as_they_arrive() {
        use crate::llm::transport::MockStreamingTransport;

        let sse = vec![
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n".to_string(),
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n".to_string(),
            "data: [DONE]\n\n".to_string(),
        ];
        let client = AnthropicAgentClient::new(
            MockStreamingTransport::new(200, sse),
            NoopSink,
            ByokKey::new(Secret::new("sk-test")),
            cfg(),
        );

        let (tx, rx) = std::sync::mpsc::channel();
        client.complete_streaming("hi", tx).await.unwrap();

        let got: Vec<String> = rx.into_iter().collect();
        assert_eq!(got, vec!["Hel".to_string(), "lo".to_string()]);
    }

    /// 401はネットワーク不調と区別してユーザーに返す。直せる唯一のエラーなので。
    #[tokio::test]
    async fn a_rejected_key_is_reported_as_such() {
        use crate::llm::transport::MockStreamingTransport;

        let client = AnthropicAgentClient::new(
            MockStreamingTransport::new(401, vec!["{\"error\":\"bad key\"}".to_string()]),
            NoopSink,
            ByokKey::new(Secret::new("sk-bad")),
            cfg(),
        );

        let (tx, _rx) = std::sync::mpsc::channel();
        let err = client.complete_streaming("hi", tx).await.unwrap_err();

        assert!(
            matches!(err, LlmError::KeyRejected { .. }),
            "401 が KeyRejected 以外になった: {err:?}"
        );
    }

    /// 送信前にトレーサビリティを記録する。ダイジェストのみで、本文は残さない（不変条件3）。
    #[tokio::test]
    async fn streaming_records_egress_before_sending() {
        use crate::llm::transport::MockStreamingTransport;

        let sink = CountingSink::default();
        let seen = sink.count.clone();
        let client = AnthropicAgentClient::new(
            MockStreamingTransport::new(200, vec![]),
            sink,
            ByokKey::new(Secret::new("sk-test")),
            cfg(),
        );

        let (tx, _rx) = std::sync::mpsc::channel();
        let _ = client.complete_streaming("hi", tx).await;

        assert_eq!(seen.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
```

⚠️ このテストは `NoopSink` / `CountingSink` / `cfg()` というヘルパを使う。既存の `mod tests` に `cfg()` は既にある（`anthropic.rs:518` で使われている）。`NoopSink` と `CountingSink` が無ければ、`mod tests` の先頭に次を足す。

```rust
    /// 何も記録しないトレーサビリティsink。ストリーミング経路の検証で、記録の有無が
    /// 論点でないときに使う。
    struct NoopSink;
    impl TraceabilitySink for NoopSink {
        fn record(&self, _r: TraceRecord) {}
    }

    /// 記録された回数だけ数えるsink。中身は見ない — ダイジェストの検証は既存テストの担当。
    #[derive(Default)]
    struct CountingSink {
        count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }
    impl TraceabilitySink for CountingSink {
        fn record(&self, _r: TraceRecord) {
            self.count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

```bash
cargo test -p shogun-core llm::anthropic
```

期待: `no method named 'complete_streaming' found`。

- [ ] **Step 3: complete_streaming を実装する**

`impl` ブロック内、既存の `pub async fn complete`（行366-380）の直後に挿入する。

⚠️ `AnthropicAgentClient` は `T: HttpTransport` でジェネリック。`complete_streaming` は `T: StreamingTransport` を要求するので、**別の `impl` ブロック**として書く。既存の `impl<T: HttpTransport, S: TraceabilitySink> AnthropicAgentClient<T, S>` ブロックの**閉じ括弧の後**に、次を丸ごと足す。

```rust
/// ストリーミング経路。`T: StreamingTransport` を要求するので、非ストリーミングの
/// `complete` とは別の impl ブロックに置く — 片方しか実装していないトランスポートでも
/// 使える方だけがコンパイルできる。
impl<T: crate::llm::transport::StreamingTransport, S: TraceabilitySink> AnthropicAgentClient<T, S> {
    /// プロンプトを送り、テキストデルタを届いた順に `out` へ流す。
    ///
    /// 返るのはストリームが終わったときで、テキストそのものは返さない。呼び出し側は
    /// `out` を読みながら画面に出す — 完成した文字列を返り値で待つと、まさにこのメソッドが
    /// 存在する理由（初トークン1s）が消える。
    pub async fn complete_streaming(
        &self,
        prompt: &str,
        out: std::sync::mpsc::Sender<String>,
    ) -> Result<(), LlmError> {
        let req = build_messages_request(&self.cfg, self.key.secret(), prompt, true)?;
        // 送信前に記録する（不変条件3）。ダイジェストのみで本文は残さない。
        self.sink.record(TraceRecord::for_chunk(
            Route::MessagesApi,
            "agent",
            self.cfg.destination(),
            prompt,
            false,
        ));

        // トランスポートからの生チャンクをここで受け、SSEを解いてからデルタだけを `out` に流す。
        let (raw_tx, raw_rx) = std::sync::mpsc::channel::<String>();
        let send = self.transport.send_streaming(req, raw_tx);

        // ステータスが分かるのは送信が終わってから。エラーボディはデルタを含まないので、
        // 非2xxのときは `out` に何も流れないまま、ここでエラーになる。
        let status = send.await?;

        let mut decoder = crate::llm::sse::SseDecoder::new();
        let mut body_for_error = String::new();
        for chunk in raw_rx {
            if !(200..300).contains(&status) {
                // 失敗時はデルタを流さず、エラー本文を組み立てるためだけに読む。
                body_for_error.push_str(&chunk);
                continue;
            }
            for delta in decoder.push(&chunk) {
                // 受け手が消えた = パネルが閉じられた。正常終了として抜ける。
                if out.send(delta).is_err() {
                    return Ok(());
                }
            }
        }

        if !(200..300).contains(&status) {
            return Err(crate::llm::status_error("messages", status, &body_for_error));
        }
        Ok(())
    }
}
```

⚠️ `self.transport` / `self.sink` / `self.key` / `self.cfg` が private フィールドの場合、同一モジュール内なのでアクセスできる。別モジュールに置かないこと。

- [ ] **Step 4: テストが通ることを確認する**

```bash
cargo test -p shogun-core llm::anthropic
cargo clippy -p shogun-core --all-targets -- -D warnings
```

期待: 新規3テストを含め全 PASS。

- [ ] **Step 5: コミット**

```bash
git add crates/shogun-core/src/llm/anthropic.rs
git commit -m "feat(core): stream Anthropic completions delta-by-delta (#44)

complete() already asked for stream:true but buffered the whole body
before parsing, so first-token latency could not be measured. This adds
the incremental path without changing the existing callers."
```

---

## Task 8: 長押し検知（NSEvent）

`apps/desktop/src-tauri/src/lib.rs:1380` の `watch_option_tap` は「⌥を単独で500ms以内にタップ」を検知する。必要な部品（down/upエッジ、他入力によるpoison、`Instant` ベースの計時）は全部そこにある。それを**素の修飾キーの長押し**へ一般化して切り出す。

既定は**右⌘**。左右は `flagsChanged` の `keyCode` で判別できる（左⌘=55 / 右⌘=54 / 右⌥=61 / Fn=63）。右⌘単独押しはmacOSが何にも割り当てていないので衝突がない。⌥単独は既存のdraftトリガが占有済みなので使わない。

**Files:**
- Create: `apps/desktop/src-tauri/src/hold_monitor.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: キー種別と純ロジックのテストを書く**

`apps/desktop/src-tauri/src/hold_monitor.rs` を作る。

```rust
//! 素の修飾キーの長押し検知（Issue #44）。
//!
//! `watch_option_tap` と同じ NSEvent グローバルモニタの上に立つが、見ているものが逆:
//! あちらは「短く単独で叩いた」を、こちらは「押している間ずっと」を取る。
//!
//! 素の修飾キーである理由は、tauriのグローバルショートカットが素の修飾キーを登録できない
//! から（`watch_option_tap` の冒頭コメントと同じ制約）。そして素の修飾キーを選ぶ理由は、
//! 長押しに文字キーを混ぜると押している間ずっとキーリピートが走り、前面アプリに文字が
//! 流れ込むから。
//!
//! 既定は右⌘。macOSが右⌘単独に何も割り当てておらず、⌘Space（Spotlight）とも衝突しない。
//! ⌥単独は既存のdraftトリガ（`watch_option_tap`）が使っているので選べない。

/// 長押しに使える素の修飾キー。左側のキーは意図的に含めない — 通常のショートカットの
/// 起点として最も使われるので、長押し判定と取り合いになる。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldKey {
    RightCommand,
    RightOption,
    Fn,
}

impl HoldKey {
    /// 設定ファイルに書く安定した文字列。
    pub fn key(self) -> &'static str {
        match self {
            HoldKey::RightCommand => "right_command",
            HoldKey::RightOption => "right_option",
            HoldKey::Fn => "fn",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        match s {
            "right_command" => Some(HoldKey::RightCommand),
            "right_option" => Some(HoldKey::RightOption),
            "fn" => Some(HoldKey::Fn),
            _ => None,
        }
    }

    /// このキーの `NSEvent.keyCode`。左右の判別はこれでしかできない — `modifierFlags`
    /// は左右を区別しない。
    fn key_code(self) -> u16 {
        match self {
            HoldKey::RightCommand => 54,
            HoldKey::RightOption => 61,
            HoldKey::Fn => 63,
        }
    }

    /// このキーが押されているときに立つ `NSEventModifierFlags` のビット。
    fn flag(self) -> usize {
        match self {
            HoldKey::RightCommand => 1 << 20, // NSEventModifierFlagCommand
            HoldKey::RightOption => 1 << 19,  // NSEventModifierFlagOption
            HoldKey::Fn => 1 << 23,           // NSEventModifierFlagFunction
        }
    }
}

impl Default for HoldKey {
    fn default() -> Self {
        HoldKey::RightCommand
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_hold_key_is_right_command() {
        assert_eq!(HoldKey::default(), HoldKey::RightCommand);
    }

    /// 設定ファイルとの往復で値が変わらない。
    #[test]
    fn hold_keys_round_trip_through_their_wire_key() {
        for k in [HoldKey::RightCommand, HoldKey::RightOption, HoldKey::Fn] {
            assert_eq!(HoldKey::from_key(k.key()), Some(k));
        }
    }

    #[test]
    fn an_unknown_wire_key_is_rejected_rather_than_guessed() {
        assert_eq!(HoldKey::from_key("left_command"), None);
        assert_eq!(HoldKey::from_key(""), None);
    }

    /// 左⌘(55)を拾わない。通常のショートカットの起点と取り合いになる。
    #[test]
    fn the_right_command_key_code_is_not_the_left_one() {
        assert_eq!(HoldKey::RightCommand.key_code(), 54);
        assert_ne!(HoldKey::RightCommand.key_code(), 55);
    }
}
```

- [ ] **Step 2: テストが失敗することを確認する**

`apps/desktop/src-tauri/src/lib.rs` の既存の `mod` 宣言群（ファイル冒頭付近、`mod audio_lane;` などが並んでいる箇所）に足す。

```rust
/// 素の修飾キーの長押し検知（push-to-talk, Issue #44）。
#[cfg(target_os = "macos")]
mod hold_monitor;
```

```bash
cargo test -p shogun-desktop-spike hold_monitor
```

期待: 4テスト PASS（この段階では純ロジックだけなので通る）。

- [ ] **Step 3: NSEventモニタを実装する**

`apps/desktop/src-tauri/src/hold_monitor.rs` の `impl Default for HoldKey` の後、`#[cfg(test)] mod tests` の前に挿入する。

```rust
/// 長押しの監視を開始する。アプリのライフタイム中ずっと動き続ける（モニタは意図的にleakする、
/// `watch_option_tap` と同じ）。
///
/// 押し下がったら `on_start`、離れたら `on_end` を呼ぶ。**`on_end` は `on_start` を呼んだ
/// 場合にのみ呼ばれる** — 押していないキーが離れたことにして、開いていないマイクを閉じに
/// 行かせない。
///
/// 他のキーやマウスが割り込んだholdは無効化する（poison）。⌘クリックや⌘Tabを
/// 「長押し」と読み違えると、ユーザーが普通の操作をしただけで録音が始まる。
#[cfg(target_os = "macos")]
pub fn watch<S, E>(key: HoldKey, on_start: S, on_end: E)
where
    S: Fn() + Send + Sync + 'static,
    E: Fn() + Send + Sync + 'static,
{
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use std::sync::atomic::{AtomicBool, Ordering};

    /// このholdでマイクを開いたか。`on_end` を呼んで良いかの唯一の判断材料。
    static HOLDING: AtomicBool = AtomicBool::new(false);
    /// 他の入力が割り込んだ。キーが完全に離れるまで再武装しない。
    static POISONED: AtomicBool = AtomicBool::new(false);
    /// 前回の flagsChanged 時点でこのキーが押されていたか。真の押し下げエッジだけを取るため。
    static WAS_DOWN: AtomicBool = AtomicBool::new(false);

    const MASK_KEY_DOWN: usize = 1 << 10; // NSEventMaskKeyDown
    const MASK_FLAGS_CHANGED: usize = 1 << 12; // NSEventMaskFlagsChanged
    // ⌘クリック・⌘ドラッグ・⌘スクロールを長押しと読まないための、マウス系の全マスク。
    // `watch_option_tap` の MASK_MOUSE と同じ集合。
    const MASK_MOUSE: usize = (1 << 1)
        | (1 << 2)
        | (1 << 3)
        | (1 << 4)
        | (1 << 5)
        | (1 << 6)
        | (1 << 22)
        | (1 << 25)
        | (1 << 26)
        | (1 << 27)
        | (1 << 29)
        | (1 << 30)
        | (1 << 31);

    let target_code = key.key_code();
    let target_flag = key.flag();
    // 対象キー以外の修飾キー。長押し中にこれらが加わったら和音であって長押しではない。
    const ALL_MODIFIERS: usize = (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 23);
    let other_modifiers = ALL_MODIFIERS & !target_flag;

    let on_start = std::sync::Arc::new(on_start);
    let on_end = std::sync::Arc::new(on_end);

    // 割り込みでholdを無効化する。すでに録音が始まっていたなら、開いたマイクは閉じる。
    let end_for_poison = on_end.clone();
    let poison = move || {
        POISONED.store(true, Ordering::Relaxed);
        if HOLDING.swap(false, Ordering::Relaxed) {
            end_for_poison();
        }
    };

    // SAFETY: setup（メインスレッド）から呼ぶ。モニタとブロックはアプリのライフタイム分
    // 意図的にleakする（`watch_option_tap` と同じ扱い）。
    unsafe {
        let poison_for_block = poison.clone();
        let disarm_block = block2::RcBlock::new(move |_ev: *mut AnyObject| poison_for_block());
        let key_mon: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_KEY_DOWN,
            handler: &*disarm_block
        ];
        let mouse_mon: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_MOUSE,
            handler: &*disarm_block
        ];
        std::mem::forget(disarm_block);
        let _ = (key_mon, mouse_mon);

        let flags_block = block2::RcBlock::new(move |ev: *mut AnyObject| {
            if ev.is_null() {
                return;
            }
            let code: u16 = msg_send![ev, keyCode];
            let flags: usize = msg_send![ev, modifierFlags];

            // 対象キー以外の修飾キーが動いた場合: それが押し下げなら和音なので無効化する。
            if code != target_code {
                if flags & other_modifiers != 0 {
                    poison();
                }
                return;
            }

            let down = flags & target_flag != 0;
            let was_down = WAS_DOWN.swap(down, Ordering::Relaxed);

            if down && !was_down {
                // 真の押し下げエッジ。他の修飾キーが既に押されているなら和音なので始めない。
                if flags & other_modifiers != 0 {
                    POISONED.store(true, Ordering::Relaxed);
                    return;
                }
                POISONED.store(false, Ordering::Relaxed);
                if !HOLDING.swap(true, Ordering::Relaxed) {
                    on_start();
                }
            } else if !down && was_down {
                // 完全に離れた。ここが唯一の再武装ポイント。
                POISONED.store(false, Ordering::Relaxed);
                if HOLDING.swap(false, Ordering::Relaxed) {
                    on_end();
                }
            }
        });
        let flags_mon: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_FLAGS_CHANGED,
            handler: &*flags_block
        ];
        std::mem::forget(flags_block);
        let _ = flags_mon;
    }

    eprintln!("[ptt] hold monitor watching {}", key.key());
}
```

⚠️ `poison` クロージャを `Clone` するには `on_end` が `Arc` である必要があり、クロージャ自体も `Clone` を導出できる形（キャプチャが全部 `Clone`）でなければならない。上のコードは `end_for_poison: Arc<E>` だけをキャプチャしているので `Clone` が付く。コンパイルが通らない場合は `let poison = std::sync::Arc::new(move || {...});` にして、各所で `poison.clone()` を呼ぶ形へ変える。

- [ ] **Step 4: ビルドが通ることを確認する**

```bash
cargo check -p shogun-desktop-spike
cargo clippy -p shogun-desktop-spike --all-targets -- -D warnings
cargo test -p shogun-desktop-spike hold_monitor
```

期待: ビルド成功、warning なし、4テスト PASS。

- [ ] **Step 5: コミット**

```bash
git add apps/desktop/src-tauri/src/hold_monitor.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): detect bare-modifier hold for push-to-talk (#44)

Right Command by default: macOS assigns nothing to it alone, it does not
collide with Spotlight's Cmd+Space, and bare Option is already taken by
the draft trigger."
```

---

## Task 9: 一発ASRレーン

`apps/desktop/src-tauri/src/audio_lane.rs` は会議専用（`session_id` 必須、`DbSink` でDB書き込み、system tapで相手の声も拾う）。PTTは自分の発話だけを、DBに書かずに一度取る。モデル解決は共有し、レーンは分ける。

**Files:**
- Create: `apps/desktop/src-tauri/src/ptt_lane.rs`
- Modify: `apps/desktop/src-tauri/src/audio_lane.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: モデル解決関数を共有する**

`apps/desktop/src-tauri/src/audio_lane.rs:79` の `select_model_path` を `pub(crate)` にする。

```rust
pub(crate) fn select_model_path(app: &tauri::AppHandle, model: AsrModel) -> Option<std::path::PathBuf> {
```

同じく `whisper_model_path`（行66）も `pub(crate)` にする。

```rust
pub(crate) fn whisper_model_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
```

- [ ] **Step 2: PTTレーンを実装する**

`apps/desktop/src-tauri/src/ptt_lane.rs` を作る。

```rust
//! push-to-talk の一発ASRレーン（Issue #44）。
//!
//! [`crate::audio_lane`] との違いは3つあり、どれも意図的:
//!
//! 1. **マイクだけを開く。** system tap を開かない。押して話しているのは目の前の一人で、
//!    その人の質問に答えるのが仕事だから、部屋の他の音を拾う理由がない。
//! 2. **DBに書かない。** 1回の発話は1回のプロンプトになって消える。`sessions` に対応する
//!    interval も無い。
//! 3. **劣化しない。** 会議はマイクが死んでもノートは録れるので notes-only に落ちるが、
//!    こちらは音声が全てなので、始められないなら理由を出して止まる。黙って無反応になるのが
//!    最悪の結果。
//!
//! 不変条件2は [`crate::audio_lane`] と同じ理屈で守られる: 波形は `Worker` のバッファにしか
//! 存在せず、ここが受け取るのは文字起こし後のテキストだけ。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use shogun_core::audio::capture::AudioSource;
use shogun_core::audio::worker::Worker;
use shogun_core::meeting::settings::{AsrModel, MeetingLanguage};
use shogun_core::ptt::buffer_sink::BufferSink;
use shogun_core::ptt::statemachine::Fail;

/// 動作中の一発レーン。`stop` か `discard` で必ず回収する。
pub struct Handle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    /// ポーリングスレッドと、停止側の両方から触るので `Mutex`。中身はテキストだけ。
    sink: Arc<Mutex<BufferSink>>,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// `Mutex` 越しに `SegmentSink` を満たすためのアダプタ。`Worker::poll` は
/// `&mut dyn SegmentSink` を取るので、ロックを取った状態のものを渡す。
struct Locked<'a>(std::sync::MutexGuard<'a, BufferSink>);

impl shogun_core::audio::worker::SegmentSink for Locked<'_> {
    fn emit(&mut self, u: &shogun_core::audio::Utterance, text: &str, confidence: f64) {
        self.0.emit(u, text, confidence);
    }
}

/// マイクを開いて文字起こしを始める。
///
/// 失敗は [`Fail`] で返す — 呼び出し側はそれをそのまま状態機械の `Input::Failed` に渡せる。
pub fn start(
    app: &tauri::AppHandle,
    model: AsrModel,
    language: MeetingLanguage,
) -> Result<Handle, Fail> {
    let Some(model_path) = crate::audio_lane::select_model_path(app, model) else {
        eprintln!("[ptt] no whisper model available");
        return Err(Fail::NoAsrModel);
    };
    let asr = shogun_core::audio::asr::whisper::Whisper::load_with_language(
        &model_path.to_string_lossy(),
        language.whisper_code(),
    )
    .map_err(|e| {
        eprintln!("[ptt] whisper load failed ({e})");
        Fail::NoAsrModel
    })?;

    let mic = shogun_core::audio::capture::mic::Mic::open().map_err(|e| {
        eprintln!("[ptt] microphone unavailable ({e})");
        Fail::MicUnavailable
    })?;

    let sink = Arc::new(Mutex::new(BufferSink::new()));
    let sink_for_thread = sink.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();

    let mut worker = Worker::new(SingleSource(Box::new(mic)), asr);
    let join = std::thread::spawn(move || {
        while !stop_flag.load(Ordering::Relaxed) {
            let consumed = match sink_for_thread.lock() {
                Ok(g) => worker.poll(now_ms(), &mut Locked(g)),
                // ロックが毒された = 別スレッドがpanicした。マイクを開いたまま回り続けない。
                Err(_) => break,
            };
            if consumed == 0 {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        // 最後の発話を吐き出してデバイスを解放する。
        if let Ok(g) = sink_for_thread.lock() {
            worker.stop(now_ms(), &mut Locked(g));
        }
    });

    eprintln!("[ptt] audio lane started");
    Ok(Handle { stop, join: Some(join), sink })
}

/// マイクを閉じ、溜まった文字起こしを取り出す。
pub fn stop(handle: Handle) -> String {
    let mut handle = handle;
    join_lane(&mut handle);
    handle.sink.lock().map(|mut s| s.take()).unwrap_or_default()
}

/// マイクを閉じ、溜まったものを捨てる。誤爆とキャンセルの道。
pub fn discard(handle: Handle) {
    let mut handle = handle;
    join_lane(&mut handle);
    if let Ok(mut s) = handle.sink.lock() {
        s.discard();
    }
    eprintln!("[ptt] audio lane discarded");
}

fn join_lane(handle: &mut Handle) {
    handle.stop.store(true, Ordering::Relaxed);
    if let Some(join) = handle.join.take() {
        // panicしたキャプチャスレッドは無視する。どのみち畳んでいる最中で、ここでpanicを
        // 伝播させるとセッション機械ごと落ちる。
        let _ = join.join();
    }
}

/// 単一ソース用の薄いラッパ。`MultiSource` は複数を回すためのもので、1本しか無いときに
/// ラウンドロビンの剰余計算を通す意味がない。
struct SingleSource(Box<dyn AudioSource>);

impl AudioSource for SingleSource {
    fn try_recv(&mut self) -> Option<shogun_core::audio::capture::Frame> {
        self.0.try_recv()
    }
    fn stop(&mut self) {
        self.0.stop();
    }
}
```

- [ ] **Step 3: モジュールを宣言してビルドする**

`apps/desktop/src-tauri/src/lib.rs` の `mod hold_monitor;` の隣に足す。

```rust
/// push-to-talk の一発ASRレーン（Issue #44）。
#[cfg(target_os = "macos")]
mod ptt_lane;
```

```bash
cargo check -p shogun-desktop-spike
cargo clippy -p shogun-desktop-spike --all-targets -- -D warnings
```

期待: ビルド成功、warning なし。

⚠️ `shogun_core::audio::capture::Frame` が `pub` でない場合は `capture/mod.rs` で `pub struct Frame` になっているか確認する。`MultiSource` が `Frame` を返しているので `pub` のはず。

- [ ] **Step 4: コミット**

```bash
git add apps/desktop/src-tauri/src/ptt_lane.rs apps/desktop/src-tauri/src/audio_lane.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): one-shot mic-only ASR lane for push-to-talk (#44)

Unlike the meeting lane this opens no system tap, writes no transcript to
the database, and refuses to start rather than degrading — for a spoken
question there is nothing left to do without audio."
```

---

## Task 10: PTTパネル（Rust側）

`apps/desktop/src-tauri/src/meeting.rs:523` の `build_overlay` と同じ形で、PTT専用のウィンドウを作る。`float_on_all_spaces`（`lib.rs:1317`）を通せば全Space・フルスクリーンアプリの上に出る。

**Files:**
- Create: `apps/desktop/src-tauri/src/ptt.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: ウィンドウ生成と表示制御を書く**

`apps/desktop/src-tauri/src/ptt.rs` を作る。この時点では状態機械の配線はまだで、パネルだけ。

```rust
//! push-to-talk の実行層（Issue #44）。
//!
//! [`shogun_core::ptt::statemachine`] が何をすべきかを決め、ここがそれを実際に行う。
//! マイク・パネル・音・エージェント呼び出しの全てがこのファイルを通るので、
//! 「マイクを開くコードはどこか」の答えが1箇所に収まる。

use shogun_core::ptt::statemachine::{Fail, Panel, Sound, State};
use tauri::{Emitter, Manager};

/// PTTパネルのウィンドウラベル。notch / meeting とは別の窓。
const WINDOW_LABEL: &str = "ptt";

/// パネルのサイズ。録音中は小さく、応答が出たら縦に伸びる。
const LISTENING_SIZE: (f64, f64) = (320.0, 96.0);
const RESPONDING_SIZE: (f64, f64) = (420.0, 260.0);

/// フロントエンドに送る、パネルが見せるべき中身。状態機械の [`Panel`] をそのまま
/// シリアライズせず、UIが読める形に写して渡す。
#[derive(Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PanelView {
    Listening,
    Transcribing,
    Responding,
    Error { code: &'static str },
}

impl From<Panel> for PanelView {
    fn from(p: Panel) -> Self {
        match p {
            Panel::Listening => PanelView::Listening,
            Panel::Transcribing => PanelView::Transcribing,
            Panel::Responding => PanelView::Responding,
            Panel::Error(why) => PanelView::Error { code: why.code() },
        }
    }
}

/// パネルウィンドウを（無ければ）作る。起動時に一度呼び、以降は使い回す。
///
/// meeting overlay と同じ理由で `WebviewUrl::default()` を使う: `App("index.html")` は
/// devサーバーが配らないURLに解決されて、JavaScriptが一度も走らない空の窓になる。
pub fn build_panel(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
        return Some(win);
    }
    let win = tauri::WebviewWindowBuilder::new(app, WINDOW_LABEL, tauri::WebviewUrl::default())
        .title("SHOGUN — voice")
        .transparent(true)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .shadow(false)
        .skip_taskbar(true)
        .inner_size(LISTENING_SIZE.0, LISTENING_SIZE.1)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|e| eprintln!("[ptt] panel window build failed: {e}"))
        .ok()?;
    crate::float_on_all_spaces(&win);
    eprintln!("[ptt] panel url = {:?}", win.url().map(|u| u.to_string()));
    Some(win)
}

/// パネルに中身を出す。位置は castle 設定に合わせ、録音中も応答も同じ場所に出す
/// （設計書§7: 視線を動かさせない）。
pub fn show_panel(app: &tauri::AppHandle, view: PanelView) {
    let Some(win) = app.get_webview_window(WINDOW_LABEL) else { return };
    let size = match view {
        PanelView::Responding => RESPONDING_SIZE,
        _ => LISTENING_SIZE,
    };
    let _ = win.set_size(tauri::LogicalSize::new(size.0, size.1));
    // 状態が変わるたびに送る。webview側はこれだけを見て描き分ける。
    let _ = win.emit("ptt:panel", view);
    let _ = win.show();
    let _ = win.set_always_on_top(true);
    crate::redock_to_castle(app);
}

pub fn hide_panel(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
        let _ = win.hide();
    }
}

/// 開始・終了の合図。macOS標準のシステムサウンドを使うので、ユーザーのシステム音量と
/// 「サウンドエフェクトを再生」設定にそのまま従う。独自の音源ファイルは持たない。
pub fn play_sound(sound: Sound) {
    let name = match sound {
        Sound::Start => "Tink",
        Sound::End => "Pop",
    };
    // SAFETY: メインスレッドである必要はない（NSSound は任意のスレッドから鳴らせる）。
    // 失敗しても無視する — 音が出ないことでセッションを止める理由がない。
    unsafe {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        use objc2_foundation::NSString;
        let ns_name = NSString::from_str(name);
        let sound: *mut AnyObject = msg_send![class!(NSSound), soundNamed: &*ns_name];
        if !sound.is_null() {
            let _: bool = msg_send![sound, play];
        }
    }
}

/// 現在の状態タグ。ログとデバッグ用。
pub fn state_tag(state: State) -> &'static str {
    state.tag()
}

/// 失敗理由からユーザーに見せる一文を作る。**英語**（v1規約）。i18n-readyに保つため、
/// 文言はこの関数だけに集める。
pub fn fail_message(why: Fail) -> &'static str {
    match why {
        Fail::MicUnavailable => "SHOGUN cannot reach the microphone. Open Privacy & Security settings to allow it.",
        Fail::NoAsrModel => "The speech model is not available yet.",
        Fail::NothingHeard => "Nothing was heard. Hold the key and speak, then let go.",
        Fail::AsrFailed => "That could not be transcribed. Try once more.",
        Fail::Network => "SHOGUN could not reach the network.",
        Fail::KeyRejected => "The API key was rejected. Check it in Settings.",
    }
}
```

- [ ] **Step 2: モジュールを宣言し、起動時にウィンドウを作る**

`apps/desktop/src-tauri/src/lib.rs` の `mod ptt_lane;` の隣に足す。

```rust
/// push-to-talk の実行層（Issue #44）。
#[cfg(target_os = "macos")]
mod ptt;
```

`redock_to_castle` は現在 `fn`（private）。`ptt.rs` から呼ぶので `pub(crate)` にする。`lib.rs:1761`:

```rust
pub(crate) fn redock_to_castle(handle: &tauri::AppHandle) {
```

`float_on_all_spaces` は既に `pub(crate)`（`lib.rs:1317`）なので変更不要。

setup 関数の中、meeting overlay を作っている箇所の近くに足す。

```rust
    // PTTパネルは起動時に作る。押した瞬間にAppKitのウィンドウを作りに行くと、
    // 「押してからパネルが出るまで100ms」というSLOを窓の生成コストで落とす。
    let _ = ptt::build_panel(app.handle());
```

- [ ] **Step 3: ビルドが通ることを確認する**

```bash
cargo check -p shogun-desktop-spike
cargo clippy -p shogun-desktop-spike --all-targets -- -D warnings
```

期待: ビルド成功、warning なし。

⚠️ `objc2_foundation::NSString` が `apps/desktop/src-tauri/Cargo.toml` の依存に無い場合は追加する。`lib.rs` が既に `objc2_foundation::{NSPoint, NSRect}` を使っているので入っているはず。

- [ ] **Step 4: コミット**

```bash
git add apps/desktop/src-tauri/src/ptt.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): push-to-talk panel window (#44)"
```

---

## Task 11: パネルUI（React）

**Files:**
- Create: `apps/desktop/src/PttOverlay.tsx`
- Modify: `apps/desktop/src/App.tsx`

- [ ] **Step 1: PttOverlay を書く**

`apps/desktop/src/PttOverlay.tsx` を作る。

```tsx
// push-to-talk のパネル（Issue #44）。
//
// 録音中・解析中・応答・エラーの4つを同じ位置・同じ枠で描き分ける。位置を変えないのは、
// 話し終えたユーザーの視線が既にそこにあるから。
//
// データは全て Rust から来る（不変条件1: webview にデータ層のロジックを置かない）。
// このファイルが持っているのは描画と、閉じる・コピーの操作だけ。

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

type PanelView =
  | { kind: "listening" }
  | { kind: "transcribing" }
  | { kind: "responding" }
  | { kind: "error"; code: string };

// 失敗理由の文言は Rust 側（ptt::fail_message）が持つ。ここでは受け取って出すだけ。
type ErrorPayload = { code: string; message: string };

export function PttOverlay() {
  const [view, setView] = useState<PanelView | null>(null);
  const [answer, setAnswer] = useState("");
  const [errorText, setErrorText] = useState("");

  useEffect(() => {
    const unlisten = [
      listen<PanelView>("ptt:panel", (e) => {
        setView(e.payload);
        // 新しいセッションが始まったら前の応答を消す。前の答えの上に次の答えが
        // 積み上がると、どこまでが今の返事か分からなくなる。
        if (e.payload.kind === "listening") {
          setAnswer("");
          setErrorText("");
        }
      }),
      // 応答は届いた端から追記する。完成を待って一度に出すと、ストリーミングにした
      // 意味が消える。
      listen<string>("ptt:delta", (e) => setAnswer((a) => a + e.payload)),
      listen<ErrorPayload>("ptt:error", (e) => setErrorText(e.payload.message)),
    ];
    return () => {
      unlisten.forEach((p) => p.then((f) => f()));
    };
  }, []);

  // Esc で閉じる。録音中なら録音ごと捨てる（Rust 側が state を見て判断する）。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") void invoke("ptt_cancel");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!view) return null;

  return (
    <div className="ptt-panel">
      {view.kind === "listening" && (
        <div className="ptt-row">
          <span className="ptt-mic ptt-mic--live" aria-hidden />
          <span className="ptt-label">Listening…</span>
          <span className="ptt-hint">Esc to cancel</span>
        </div>
      )}

      {view.kind === "transcribing" && (
        <div className="ptt-row">
          <span className="ptt-mic" aria-hidden />
          <span className="ptt-label">Working…</span>
        </div>
      )}

      {view.kind === "responding" && (
        <div className="ptt-answer">
          <p className="ptt-text">{answer}</p>
          <div className="ptt-actions">
            <button onClick={() => void navigator.clipboard.writeText(answer)}>Copy</button>
            <button onClick={() => void invoke("ptt_open_full_ui")}>Open in SHOGUN</button>
            <button onClick={() => void invoke("ptt_dismiss")}>Close</button>
          </div>
        </div>
      )}

      {view.kind === "error" && (
        <div className="ptt-answer ptt-answer--error">
          <p className="ptt-text">{errorText}</p>
          <div className="ptt-actions">
            {view.code === "mic_unavailable" && (
              <button onClick={() => void invoke("ptt_open_privacy_settings")}>
                Open Settings
              </button>
            )}
            <button onClick={() => void invoke("ptt_dismiss")}>Close</button>
          </div>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: App.tsx で ptt ウィンドウのときに描画する**

`apps/desktop/src/App.tsx` の先頭付近、既存のウィンドウラベル分岐（`meeting` を出し分けている箇所）に倣って足す。既存が `getCurrentWindow().label` で分岐しているならその形に合わせる。

```tsx
import { PttOverlay } from "./PttOverlay";

// ...既存の分岐に追加
if (label === "ptt") return <PttOverlay />;
```

⚠️ 既存の分岐の書き方（`MeetingOverlay` の出し分け）を先に読んで、同じ形に合わせること。

- [ ] **Step 3: スタイルを足す**

`apps/desktop/src` の既存CSSファイル（`App.css` 等、`MeetingOverlay` のスタイルがある場所）に足す。ダーク/ライト両対応のため、既存パネルが使っている色トークンをそのまま使う。

```css
/* push-to-talk パネル（Issue #44）。位置は Rust 側が castle 設定で決めるので、
   ここでは中身の並びだけを持つ。 */
.ptt-panel {
  display: flex;
  align-items: center;
  height: 100%;
  padding: 12px 16px;
  border-radius: 14px;
  background: var(--panel-bg);
  color: var(--panel-fg);
  font: inherit;
}

.ptt-row {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
}

.ptt-mic {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--panel-fg);
  opacity: 0.4;
}

/* 録音中であることは、OSのマイクインジケータだけに頼らずパネル自身も示す（プライバシー要件）。 */
.ptt-mic--live {
  opacity: 1;
  background: #e5484d;
  animation: ptt-pulse 1.2s ease-in-out infinite;
}

@keyframes ptt-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.45; }
}

.ptt-hint { margin-left: auto; opacity: 0.5; font-size: 11px; }
.ptt-label { font-size: 13px; }

.ptt-answer { display: flex; flex-direction: column; gap: 10px; width: 100%; height: 100%; }
.ptt-text { margin: 0; overflow-y: auto; flex: 1; font-size: 13px; line-height: 1.5; }
.ptt-actions { display: flex; gap: 8px; justify-content: flex-end; }
```

⚠️ `--panel-bg` / `--panel-fg` は仮の名前。既存のパネルCSSが使っている変数名を確認して、それに合わせること。

- [ ] **Step 4: 型チェックが通ることを確認する**

```bash
cd apps/desktop && pnpm typecheck
```

期待: エラーなし。

⚠️ `ptt_cancel` / `ptt_dismiss` / `ptt_open_full_ui` / `ptt_open_privacy_settings` はまだ存在しないコマンド。`invoke` は文字列を取るので型チェックは通る。実体は Task 12 で作る。

- [ ] **Step 5: コミット**

```bash
git add apps/desktop/src/PttOverlay.tsx apps/desktop/src/App.tsx apps/desktop/src/App.css
git commit -m "feat(desktop): push-to-talk panel UI (#44)"
```

---

## Task 12: 実行層の配線

ここで全部が繋がる。状態機械の Effect を実際の副作用に落とし、hold monitor から入力を流し込む。

**Files:**
- Modify: `apps/desktop/src-tauri/src/ptt.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: セッションランナーを実装する**

`apps/desktop/src-tauri/src/ptt.rs` の末尾に足す。

```rust
use shogun_core::ptt::statemachine::{Effect, Input, Machine, Params, Timer};
use std::sync::{Arc, Mutex};

/// 実行層の全状態。Tauri state として1つだけ持つ。
pub struct Session {
    machine: Mutex<Machine>,
    /// 動作中のASRレーン。`Recording` のときだけ `Some`。
    lane: Mutex<Option<crate::ptt_lane::Handle>>,
    /// 上限タイマーの世代。キャンセルは「世代を進める」ことで行う — 起動済みのスリープを
    /// 止める術がないので、目覚めたスレッドに自分が古いことを気づかせる。
    max_hold_epoch: Arc<std::sync::atomic::AtomicU64>,
    /// このセッションでマイクが開いた時刻。計測用。
    started_at: Mutex<Option<std::time::Instant>>,
}

impl Session {
    pub fn new() -> Self {
        Self {
            machine: Mutex::new(Machine::new(Params::default())),
            lane: Mutex::new(None),
            max_hold_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            started_at: Mutex::new(None),
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// 単調増加のミリ秒。壁時計の飛びに影響されない（`watch_option_tap` が `Instant` を使うのと
/// 同じ理由）。
fn mono_ms() -> i64 {
    use std::time::Instant;
    static ORIGIN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed().as_millis() as i64
}

/// 入力を機械に食わせ、返ってきた Effect を順に実行する。
///
/// **PTTの全ての入力はここを通る。** マイクを開くコードが1箇所にあることの実効的な保証で、
/// 状態機械のテストがそのまま実装の性質になる。
pub fn feed(app: &tauri::AppHandle, input: Input) {
    let effects = {
        let session = app.state::<Session>();
        let Ok(mut m) = session.machine.lock() else {
            eprintln!("[ptt] session machine poisoned; input dropped");
            return;
        };
        m.step(input)
    };
    for e in effects {
        run_effect(app, e);
    }
}

fn run_effect(app: &tauri::AppHandle, effect: Effect) {
    let session = app.state::<Session>();
    match effect {
        Effect::Transition(state) => eprintln!("[ptt] → {}", state.tag()),

        Effect::StartCapture => {
            let settings = shogun_core::meeting::settings::Settings::default();
            match crate::ptt_lane::start(app, settings.asr_model, settings.language) {
                Ok(handle) => {
                    if let Ok(mut g) = session.lane.lock() {
                        *g = Some(handle);
                    }
                    if let Ok(mut g) = session.started_at.lock() {
                        *g = Some(std::time::Instant::now());
                    }
                    crate::analytics::capture_ptt_started(app);
                }
                Err(why) => {
                    // マイクが開けないなら、機械にそう伝えて畳ませる。ここで勝手に
                    // 状態を戻さない — 遷移を決めるのは機械の仕事。
                    eprintln!("[ptt] capture failed: {}", why.code());
                    feed(app, Input::Failed(why));
                }
            }
        }

        Effect::StopCapture => {
            let handle = session.lane.lock().ok().and_then(|mut g| g.take());
            let spoke_ms = session
                .started_at
                .lock()
                .ok()
                .and_then(|mut g| g.take())
                .map(|t| t.elapsed().as_millis() as u64)
                .unwrap_or(0);
            let app = app.clone();
            // 文字起こしはスレッドを跨ぐ: whisperは数百ms〜秒かかるので、ここで待つと
            // イベントハンドラのスレッドが止まる。
            std::thread::spawn(move || {
                let text = handle.map(crate::ptt_lane::stop).unwrap_or_default();
                eprintln!("[ptt] transcribed {} chars in {spoke_ms}ms of speech", text.len());
                feed(&app, Input::Transcribed(text));
            });
        }

        Effect::DiscardCapture => {
            let handle = session.lane.lock().ok().and_then(|mut g| g.take());
            if let Ok(mut g) = session.started_at.lock() {
                *g = None;
            }
            if let Some(h) = handle {
                std::thread::spawn(move || crate::ptt_lane::discard(h));
            }
        }

        Effect::PlaySound(sound) => play_sound(sound),

        Effect::ShowPanel(panel) => {
            if let Panel::Error(why) = panel {
                if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
                    let _ = win.emit(
                        "ptt:error",
                        serde_json::json!({ "code": why.code(), "message": fail_message(why) }),
                    );
                }
                crate::analytics::capture_ptt_failed(app, why.code());
            }
            show_panel(app, panel.into());
        }

        Effect::HidePanel => hide_panel(app),

        Effect::SubmitToAgent(text) => submit(app, text),

        Effect::StartTimer { timer: Timer::MaxHold, ms } => {
            let epoch = session.max_hold_epoch.clone();
            let mine = epoch.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(ms));
                // 目覚めたときに世代が進んでいたら、このタイマーは既にキャンセルされている。
                if epoch.load(std::sync::atomic::Ordering::SeqCst) == mine {
                    feed(&app, Input::MaxHoldExpired { at_ms: mono_ms() });
                }
            });
        }

        Effect::CancelTimer(Timer::MaxHold) => {
            session.max_hold_epoch.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// 発話テキストにいまの画面を添えてエージェントへ投げ、応答を届いた端からパネルに流す。
fn submit(app: &tauri::AppHandle, spoken: String) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(db) = app.try_state::<shogun_core::daemon::Db>().map(|s| s.inner().clone()) else {
            feed(&app, Input::Failed(Fail::Network));
            return;
        };

        // コンテキストは**読むだけ**。押してから収集しない（SLO: context cacheは常時
        // プリアセンブル）。キャッシュが冷えていたら発話だけで投げる — 温まるのを待つと、
        // 待った分だけ初トークンが遅れる。
        let ctx = app
            .try_state::<shogun_core::daemon::ReplyContextCache>()
            .and_then(|c| c.current());
        let facts: Vec<String> = ctx.as_ref().map(|c| c.facts.clone()).unwrap_or_default();
        let (fg_app, fg_title) = crate::capture_source::foreground_app_and_title();

        let prompt = shogun_core::ptt::prompt::build_prompt(
            &spoken,
            &shogun_core::ptt::prompt::Spoken {
                app: fg_app.as_deref(),
                window_title: fg_title.as_deref(),
                facts: &facts,
            },
        );

        let Some(agent) = crate::inline_source::mac::build_agent(&db) else {
            feed(&app, Input::Failed(Fail::KeyRejected));
            return;
        };

        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let win = app.get_webview_window(WINDOW_LABEL);
        let started = std::time::Instant::now();
        let mut first_token_ms: Option<f64> = None;

        // 送信は別スレッド、受信はこのスレッド。届いた端から画面に出す。
        let app_for_send = app.clone();
        let send = std::thread::spawn(move || agent.complete_streaming_blocking(&prompt, tx));

        for delta in rx {
            if first_token_ms.is_none() {
                let ms = started.elapsed().as_secs_f64() * 1000.0;
                first_token_ms = Some(ms);
                app_for_send.state::<crate::metrics::SloRegister>().record_first_token_ms(ms);
            }
            if let Some(w) = &win {
                let _ = w.emit("ptt:delta", delta);
            }
        }

        match send.join() {
            Ok(Ok(())) => {
                crate::analytics::capture_ptt_completed(
                    &app,
                    first_token_ms.unwrap_or(0.0) as u64,
                    started.elapsed().as_millis() as u64,
                );
                feed(&app, Input::ResponseDone);
            }
            Ok(Err(why)) => feed(&app, Input::Failed(why)),
            // 送信スレッドがpanicした。理由は分からないが、セッションは畳む。
            Err(_) => feed(&app, Input::Failed(Fail::Network)),
        }
    });
}
```

- [ ] **Step 2: InlineAgent にブロッキングのストリーミング入口を足す**

`apps/desktop/src-tauri/src/inline_source.rs` の `impl AgentClient for InlineAgent` ブロックの**後**に、次の inherent impl を足す。

```rust
    impl InlineAgent {
        /// ストリーミングで応答を受け取る。tokioの外にいる同期スレッドから呼ぶための入口で、
        /// `complete` が `block_on` を使っているのと同じ理由・同じ安全性（呼び出し元は
        /// 専用のstdスレッドで、このスレッドを回しているランタイムは無い）。
        ///
        /// Anthropic以外のプロバイダはストリーミング未対応なので、完成した応答を1つの
        /// チャンクとして流す。応答は出るが初トークンは速くならない — SLOを満たすのは
        /// 既定のAnthropicだけ、という現状をそのまま表す。
        pub(crate) fn complete_streaming_blocking(
            &self,
            prompt: &str,
            out: std::sync::mpsc::Sender<String>,
        ) -> Result<(), shogun_core::ptt::statemachine::Fail> {
            use shogun_core::ptt::statemachine::Fail;

            let whole = |text: Result<String, LlmError>| -> Result<(), Fail> {
                let text = text.map_err(map_fail)?;
                let _ = out.send(text);
                Ok(())
            };

            match self {
                InlineAgent::Anthropic { rt, client } => {
                    rt.block_on(client.complete_streaming(prompt, out)).map_err(map_fail)
                }
                InlineAgent::Mock(m) => whole(m.complete(prompt)),
                InlineAgent::OpenAiCompat { rt, client } => whole(rt.block_on(client.complete(prompt))),
            }
        }
    }

    /// LLMのエラーを、ユーザーに見せる語彙へ落とす。直せるもの（キー）と直せないもの
    /// （ネットワーク）を混ぜない。
    fn map_fail(e: LlmError) -> shogun_core::ptt::statemachine::Fail {
        use shogun_core::ptt::statemachine::Fail;
        match e {
            LlmError::KeyRejected { .. } => Fail::KeyRejected,
            _ => Fail::Network,
        }
    }
```

⚠️ `LlmError` のバリアント名は `crates/shogun-core/src/llm/mod.rs` で確認すること。`KeyRejected` が別名（`Unauthorized` 等）なら合わせる。

⚠️ `build_agent` は `pub(crate)` で `mod mac` の中にある。`ptt.rs` からは `crate::inline_source::mac::build_agent` で呼ぶ。`mod mac` が `pub(crate)` でない場合は `pub(crate) mod mac;` にする。

- [ ] **Step 3: 前面アプリ名の取得を用意する**

必要な部品は既にある。`crate::display::frontmost_app()` が `FrontApp { pid, bundle_id, name }` を返し（`display.rs:50`）、`crate::axcache::focused_window(pid)` の `.title()` がウィンドウタイトルを返す（`axcache.rs:213` と `axcache.rs:166`）。`capture_source.rs:151` の `focused_thread_key_and_title` がこの2つを既に組み合わせている。

`apps/desktop/src-tauri/src/ptt.rs` の末尾に足す。

```rust
/// いま前面にあるアプリの表示名とウィンドウタイトル。
///
/// どちらも取れなくてよい。プロンプトに**添える**情報であって、無いことが発話を投げない
/// 理由にはならない（`submit` は両方 `None` でも進む）。
///
/// AXの読み取りをここで新しく書かない。`display::frontmost_app` と
/// `axcache::focused_window` は既にキャプチャ経路が使っているもので、同じことを2通りに
/// 書くとズレる。
fn foreground_app_and_title() -> (Option<String>, Option<String>) {
    let Some(front) = crate::display::frontmost_app() else { return (None, None) };
    let name = (!front.name.is_empty()).then(|| front.name.clone());
    let title = crate::axcache::focused_window(front.pid).and_then(|w| w.title());
    (name, title)
}
```

Step 1 で書いた `submit` の中の `crate::capture_source::foreground_app_and_title()` を、同一モジュールの `foreground_app_and_title()` に直す。

⚠️ `display` / `axcache` の各 `mod` 宣言が `lib.rs` で `pub(crate)` になっているか確認する。private なら `pub(crate) mod display;` / `pub(crate) mod axcache;` にする。`frontmost_app` と `focused_window` は macOS 用の内部モジュール（`display::mac` 等）に入っている場合があるので、`capture_source.rs:152` の呼び出し形（`use crate::display::frontmost_app;`）に合わせること。

- [ ] **Step 4: Tauri command を足す**

`apps/desktop/src-tauri/src/ptt.rs` の末尾に足す。

```rust
/// Escまたはパネルのキャンセル。録音中なら録音ごと捨てる。
#[tauri::command]
pub fn ptt_cancel(app: tauri::AppHandle) {
    feed(&app, Input::Cancel);
}

/// パネルを閉じる。応答を読み終えたユーザーの操作。
#[tauri::command]
pub fn ptt_dismiss(app: tauri::AppHandle) {
    feed(&app, Input::Dismiss);
}

/// 本体ウィンドウを開く。応答から先の作業へ移りたいときの逃げ道。
#[tauri::command]
pub fn ptt_open_full_ui(app: tauri::AppHandle) {
    crate::open_full_ui(&app);
}

/// マイク権限の設定画面を開く。拒否したユーザーに「どこで直すか」を示す唯一の手段。
#[tauri::command]
pub fn ptt_open_privacy_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        .spawn();
}
```

⚠️ `crate::open_full_ui` は既存の関数名に合わせること。`lib.rs` で Full UI を開いている箇所（`fullui` 関連のcommand）を確認する。

- [ ] **Step 5: setup で配線する**

`apps/desktop/src-tauri/src/lib.rs` の `invoke_handler![...]` に足す。

```rust
        ptt::ptt_cancel,
        ptt::ptt_dismiss,
        ptt::ptt_open_full_ui,
        ptt::ptt_open_privacy_settings,
```

setup 関数、`ptt::build_panel` を呼んでいる箇所の直後に足す。

```rust
    app.manage(ptt::Session::new());
    // 長押し監視。キー選択は設定から読む（Task 13）。この時点では既定の右⌘で固定。
    {
        let handle = app.handle().clone();
        let start_handle = handle.clone();
        hold_monitor::watch(
            hold_monitor::HoldKey::default(),
            move || {
                ptt::feed(&start_handle, shogun_core::ptt::statemachine::Input::HoldStart {
                    at_ms: ptt::mono_ms_pub(),
                })
            },
            move || {
                ptt::feed(&handle, shogun_core::ptt::statemachine::Input::HoldEnd {
                    at_ms: ptt::mono_ms_pub(),
                })
            },
        );
    }
```

`mono_ms` を `lib.rs` から呼べるよう、`ptt.rs` の `fn mono_ms()` を公開する。

```rust
pub fn mono_ms_pub() -> i64 {
    mono_ms()
}
```

- [ ] **Step 6: ビルドが通ることを確認する**

```bash
cargo check -p shogun-desktop-spike
cargo clippy -p shogun-desktop-spike --all-targets -- -D warnings
cargo test -p shogun-core
```

期待: ビルド成功、warning なし、コアのテスト全 PASS。

- [ ] **Step 7: 実機で一周させる**

```bash
cd apps/desktop && pnpm dev
```

右⌘を2秒押しながら "what is on my screen" と話し、離す。

期待:
1. 押した瞬間にパネルが出て「Listening…」＋赤い点が脈打つ
2. 開始音が鳴る
3. 離すと終了音が鳴り「Working…」になる
4. 数秒後に応答が届いた端から流れ出す
5. Copy / Open in SHOGUN / Close が出る

うまく行かない場合は `[ptt]` で始まるログを読む。各 Effect が実行されたところと、失敗理由がそこに出る。

- [ ] **Step 8: コミット**

```bash
git add apps/desktop/src-tauri/src/ptt.rs apps/desktop/src-tauri/src/inline_source.rs apps/desktop/src-tauri/src/capture_source.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): wire push-to-talk end to end (#44)

Hold the key, speak, let go, read the streamed answer. Every input goes
through ptt::feed, so the machine's tests describe the real behaviour."
```

---

## Task 13: 設定 UI と永続化

**Files:**
- Modify: `apps/desktop/src-tauri/src/ptt.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src/fullui/FullUi.tsx`
- Modify: `apps/desktop/src/fullui/types.ts`

- [ ] **Step 1: 設定の読み書きを実装する**

`apps/desktop/src-tauri/src/ptt.rs` の末尾に足す。`castle.rs` の `init` / `save` と同じ形。

```rust
/// PTTの設定。`app_data/ptt.json` に置く。秘密は含まないのでKeychainは不要。
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct PttSettings {
    /// β機能なので既定はオフ。設定から明示的に有効化する。
    #[serde(default)]
    pub enabled: bool,
    /// 長押しに使うキーの安定文字列（`HoldKey::key()`）。
    #[serde(default = "default_hold_key")]
    pub hold_key: String,
    /// 応答の読み上げ。初期はオフ（Issue: 将来的にオプション）。
    #[serde(default)]
    pub speak_response: bool,
}

fn default_hold_key() -> String {
    crate::hold_monitor::HoldKey::default().key().to_string()
}

impl Default for PttSettings {
    fn default() -> Self {
        Self { enabled: false, hold_key: default_hold_key(), speak_response: false }
    }
}

fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("ptt.json"))
}

/// 保存済み設定を読む。壊れたファイルは既定（無効）に落ちる — 読めない設定で
/// 勝手にマイクを有効化しない。
pub fn load_settings(app: &tauri::AppHandle) -> PttSettings {
    settings_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<PttSettings>(&t).ok())
        .unwrap_or_default()
}

fn save_settings(app: &tauri::AppHandle, s: &PttSettings) -> Result<(), String> {
    let Some(p) = settings_path(app) else { return Ok(()) };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(&p, json).map_err(|e| format!("save failed: {e}"))
}

#[tauri::command]
pub fn get_ptt_settings(app: tauri::AppHandle) -> PttSettings {
    load_settings(&app)
}

/// 設定を更新する。キーの変更は次回起動から効く — NSEventのグローバルモニタは
/// 登録解除の口を持たないので、動作中に張り替えると古い監視が残る。
#[tauri::command]
pub fn set_ptt_settings(app: tauri::AppHandle, settings: PttSettings) -> Result<(), String> {
    if crate::hold_monitor::HoldKey::from_key(&settings.hold_key).is_none() {
        return Err(format!("unknown hold key: {}", settings.hold_key));
    }
    save_settings(&app, &settings)?;
    ENABLED.store(settings.enabled, std::sync::atomic::Ordering::Relaxed);
    eprintln!("[ptt] settings: enabled={} key={}", settings.enabled, settings.hold_key);
    Ok(())
}

/// βフラグ。長押し監視は常に張るが、無効なら入力を捨てる — キー選択の変更を
/// 再起動なしで反映できない代わりに、有効/無効の切り替えは即座に効く。
pub static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
```

- [ ] **Step 2: feed でβフラグを見る**

`ptt.rs` の `pub fn feed` の先頭に足す。

```rust
pub fn feed(app: &tauri::AppHandle, input: Input) {
    // 無効なら押し下げを無視する。ただし進行中のセッションを畳む入力は通す —
    // 録音中に機能をオフにしてマイクが開きっぱなしになる、が起きてはならない。
    if !ENABLED.load(std::sync::atomic::Ordering::Relaxed)
        && matches!(input, Input::HoldStart { .. })
    {
        return;
    }
```

- [ ] **Step 3: setup で設定を読み込む**

`lib.rs` の setup、`app.manage(ptt::Session::new());` の直前に足す。

```rust
    let ptt_settings = ptt::load_settings(app.handle());
    ptt::ENABLED.store(ptt_settings.enabled, std::sync::atomic::Ordering::Relaxed);
    let hold_key = hold_monitor::HoldKey::from_key(&ptt_settings.hold_key).unwrap_or_default();
```

`hold_monitor::watch` の呼び出しで `HoldKey::default()` を `hold_key` に置き換える。

`invoke_handler![...]` に足す。

```rust
        ptt::get_ptt_settings,
        ptt::set_ptt_settings,
```

- [ ] **Step 4: 設定UIを足す**

`apps/desktop/src/fullui/FullUi.tsx` の設定ペイン（castle位置やLLMプロバイダの選択がある箇所）に、同じ形でセクションを足す。

```tsx
// Push-to-Talk（Issue #44）。β機能なので既定はオフ。
function PushToTalkSection() {
  const [settings, setSettings] = useState<PttSettings | null>(null);

  useEffect(() => {
    void invoke<PttSettings>("get_ptt_settings").then(setSettings);
  }, []);

  if (!settings) return null;

  const update = (patch: Partial<PttSettings>) => {
    const next = { ...settings, ...patch };
    setSettings(next);
    void invoke("set_ptt_settings", { settings: next });
  };

  return (
    <section className="fullui-section">
      <h3>Push-to-Talk <span className="fullui-badge">Beta</span></h3>
      <p className="fullui-hint">
        Hold a key and speak. Let go and SHOGUN answers, with what is on your screen in mind.
      </p>

      <label>
        <input
          type="checkbox"
          checked={settings.enabled}
          onChange={(e) => update({ enabled: e.target.checked })}
        />
        Enable push-to-talk
      </label>

      <label>
        Hold key
        <select
          value={settings.hold_key}
          onChange={(e) => update({ hold_key: e.target.value })}
        >
          <option value="right_command">Right ⌘</option>
          <option value="right_option">Right ⌥</option>
          <option value="fn">Globe / fn</option>
        </select>
      </label>
      <p className="fullui-hint">Takes effect after restarting SHOGUN.</p>

      <label>
        <input
          type="checkbox"
          checked={settings.speak_response}
          onChange={(e) => update({ speak_response: e.target.checked })}
        />
        Read the answer aloud
      </label>
    </section>
  );
}
```

`apps/desktop/src/fullui/types.ts` に型を足す。

```ts
export type PttSettings = {
  enabled: boolean;
  hold_key: string;
  speak_response: boolean;
};
```

⚠️ Task 1 のスパイクで「`Released` は信頼できる」という結論が出た場合は、`<select>` に通常コンボの選択肢を足す。信頼できない結論なら上の3択のまま。

⚠️ `speak_response` はUIだけ用意して、読み上げの実装は行わない（Issue: 初期はオフ設定、将来的にオプション）。オンにしても何も起きないのが分かるよう、`disabled` にして "Coming soon" を添えるか、実装するまでこのラベルを出さないかを選ぶ。**実装しないなら出さない**方を選ぶこと — 押しても何も起きないトグルは壊れているのと区別がつかない。

- [ ] **Step 5: 確認してコミット**

```bash
cargo check -p shogun-desktop-spike
cargo clippy -p shogun-desktop-spike --all-targets -- -D warnings
cd apps/desktop && pnpm typecheck
```

```bash
git add apps/desktop/src-tauri/src/ptt.rs apps/desktop/src-tauri/src/lib.rs apps/desktop/src/fullui/
git commit -m "feat(desktop): push-to-talk settings and beta flag (#44)"
```

---

## Task 14: マイク権限

**Files:**
- Modify: `apps/desktop/src-tauri/Info.plist`

- [ ] **Step 1: 用途説明を足す**

`apps/desktop/src-tauri/Info.plist` の `<dict>` 内、`LSUIElement` の後に足す。

```xml
    <!-- マイクの用途。この文字列がそのまま macOS の許可ダイアログに出るので、
         「何のために使うか」と「保存しないこと」の両方を書く。不変条件2を、
         コードだけでなくユーザーに見える場所でも約束する。 -->
    <key>NSMicrophoneUsageDescription</key>
    <string>SHOGUN listens while you hold the push-to-talk key, and while you record a meeting. Speech is transcribed on your Mac and the audio is never saved.</string>
```

- [ ] **Step 2: 実機で確認する**

```bash
cd apps/desktop && pnpm dev
```

マイク権限を一度リセットしてから試す。

```bash
tccutil reset Microphone dev.shogun.spike
```

期待:
1. 初回の長押しでmacOSの許可ダイアログが出て、上の文言が表示される
2. 「許可しない」を選ぶとパネルにエラーが出て、"Open Settings" ボタンが現れる
3. そのボタンでプライバシー設定のマイク欄が開く
4. 許可して再度長押しすると録音が始まる

- [ ] **Step 3: コミット**

```bash
git add apps/desktop/src-tauri/Info.plist
git commit -m "feat(desktop): declare the microphone usage description (#44)"
```

---

## Task 15: 計測

Issue の定量目標（週1回以上の利用率30%、ワーク完了50%、平均応答時間）を出せるだけのイベントを送る。**発話内容は絶対に含めない**（CLAUDE.md: テレメトリにキャプチャ内容を含めない）。

**Files:**
- Modify: `apps/desktop/src-tauri/src/analytics.rs`

- [ ] **Step 1: イベント関数を足す**

`apps/desktop/src-tauri/src/analytics.rs` の末尾に足す。既存の `capture` の呼び出し方に合わせること。

```rust
/// push-to-talk のセッションが始まった（マイクが開いた）。
///
/// ここから先の3つの関数が、Issue #44 の定量目標の材料になる。**発話も文字起こしも
/// 送らない** — 送るのは時間と結果コードだけで、それで「速いか」「失敗していないか」は
/// 十分に分かる。
pub fn capture_ptt_started(app: &tauri::AppHandle) {
    let Some(a) = app.try_state::<Analytics>() else { return };
    a.capture("ptt_session_started", Props::new());
}

/// 応答が最後まで届いた。
pub fn capture_ptt_completed(app: &tauri::AppHandle, first_token_ms: u64, total_ms: u64) {
    let Some(a) = app.try_state::<Analytics>() else { return };
    let mut props = Props::new();
    props.insert("first_token_ms", first_token_ms);
    props.insert("total_ms", total_ms);
    a.capture("ptt_session_completed", props);
}

/// セッションが失敗して終わった。`code` は `Fail::code()` の安定文字列。
pub fn capture_ptt_failed(app: &tauri::AppHandle, code: &str) {
    let Some(a) = app.try_state::<Analytics>() else { return };
    let mut props = Props::new();
    props.insert("reason", code);
    a.capture("ptt_session_failed", props);
}
```

⚠️ `Props::new()` / `insert` / `Analytics::capture` の正確なシグネチャは `analytics.rs` の既存の呼び出し（`context_updated` イベント）を読んで合わせること。`try_state` で取れない場合の扱いも既存に合わせる。

- [ ] **Step 2: 確認してコミット**

```bash
cargo check -p shogun-desktop-spike
cargo clippy -p shogun-desktop-spike --all-targets -- -D warnings
```

```bash
git add apps/desktop/src-tauri/src/analytics.rs
git commit -m "feat(desktop): measure push-to-talk sessions (#44)

Timings and outcome codes only — never the speech or the transcript."
```

---

## Task 16: 実機検証と SLO 計測

コードは書き終わり。ここは「本当に動くか」を確かめる作業で、結果をPRに貼るまでが1タスク。

**Files:**
- Create: `docs/ptt-verification.md`

- [ ] **Step 1: 全テストとビルドを通す**

```bash
cargo test -p shogun-core
cargo test -p shogun-core --features audio
cargo check -p shogun-desktop-spike
cargo clippy --workspace --all-targets -- -D warnings
cd apps/desktop && pnpm typecheck
```

期待: 全て成功。**失敗したものがあれば、その出力ごと記録して直す。** 通ったことにして進まない。

- [ ] **Step 2: 手動検証を実行して記録する**

`docs/ptt-verification.md` を作り、各項目の結果（実際に何が起きたか）を書く。

```markdown
# Push-to-Talk 実機検証（Issue #44）

環境: macOS __ / __ Mac / ビルド __

## 表示

- [ ] 他アプリが最前面のときにパネルが出る
- [ ] フルスクリーンアプリの上にパネルが出る
- [ ] 別Spaceに切り替えてもパネルが出る
- [ ] ダークモードで読める
- [ ] ライトモードで読める

## 長押しの取りこぼし

- [ ] 3秒押して離す → 正常に一周する
- [ ] 押しながら別アプリをクリック → 録音が止まり、マイクが閉じる
- [ ] 押しながら Mission Control を開いて離す → マイクが閉じる
- [ ] 右⌘を押しながら他キーを押す（⌘C等）→ 録音が始まらない
- [ ] 一瞬だけ叩く → 何も起きない（誤爆判定）
- [ ] 押しっぱなしで30秒放置 → 自動で録音が閉じる

## 失敗の見え方

- [ ] マイク権限を拒否 → 理由と "Open Settings" が出る
- [ ] 機内モード → ネットワークエラーが出る
- [ ] 無効なBYOKキー → キー拒否のメッセージが出る
- [ ] 無言で3秒押して離す → "Nothing was heard"

## キャンセル

- [ ] 録音中に Esc → 録音が捨てられ、エージェントに何も届かない
- [ ] 応答表示中に Close → パネルが閉じる
- [ ] 応答表示中に再度長押し → 新しいセッションが始まる

## 不変条件

- [ ] 録音中・録音後に音声ファイルが生成されていないことを確認
      `find ~/Library/Application\ Support/dev.shogun.spike -newermt '-5 minutes'`
- [ ] `sessions` / `transcript_segments` テーブルにPTTの行が増えていないことを確認

## SLO

| 項目 | 上限 | p50 | p95 |
|---|---|---|---|
| Hold開始 → パネル表示 | 100ms | | |
| ASR完了 → 初トークン | 1s | | |
| Hold終了 → ASR完了 | （実測を記録） | | |
| アイドル時CPU増分 | ほぼ0 | | |
```

- [ ] **Step 3: SLO を計測する**

20回セッションを回し、`[ptt]` のログと `SloRegister` の値から p50 / p95 を出す。Health ペイン（D2）に `first_token_ms` が出ているのでそこを読む。

アイドル時CPUは、PTTを有効にして30分放置し、アクティビティモニタでSHOGUNのCPU使用率（1分平均）を無効時と比べる。hold monitor が常駐する分の増分を見る。

計測値を `docs/ptt-verification.md` の表に埋める。**上限を超えた項目があれば、超えたまま記録し、原因の見当を書く。** 通ったことにしない。

- [ ] **Step 4: コミット**

```bash
git add docs/ptt-verification.md
git commit -m "docs: push-to-talk verification results and SLO measurements (#44)"
```

- [ ] **Step 5: PR を出す**

```bash
git push -u origin feat/issue-44-push-to-talk
gh pr create --base main --title "feat: push-to-talk voice interaction (#44)" --body "$(cat <<'EOF'
## What

ショートカットを長押ししている間だけマイクを開き、離すと文字起こし → コンテキスト結合 → エージェント応答のストリーミング表示までを一息で走らせる入口。Issue #44。

β experimental flag の裏、既定オフ。設定から有効化する。

## 設計

`docs/push-to-talk-voice-design.md`

## 主な判断

- **既定キーは右⌘の長押し。** ⌘Space は Spotlight と衝突し、⌥単独は既存のdraftトリガが占有済み。右⌘単独はmacOSが未割り当てで、`flagsChanged` の keyCode で左右を判別できる
- **マイクを開く経路は状態機械の `HoldStart` ただ一つ**で、`Recording` から出る全ての辺が `StopCapture` か `DiscardCapture` を伴う。どちらもテストが守っている
- **Anthropicにストリーミングを実装した。** 既存の `complete()` は `stream: true` でリクエストしながらボディが揃うまで待っていたので、初トークンのSLOが測れなかった
- **PTTは劣化しない。** 会議レーンはマイクが死んでも notes-only で続くが、こちらは音声が全てなので理由を出して止まる

## SLO

`docs/ptt-verification.md` に計測結果。

## 不変条件

- 不変2: 波形は `Worker` のRAMバッファのみ。`BufferSink` はテキストしか受けず、DBにも書かない
- 不変3: 外部に出るのは文字起こし後のテキストのみ。egressはdigest-onlyで記録
- 不変4: PTTの出力は提示のみ。送信系アクションは既存のL3承認フローを通る
EOF
)"
```

---

## 自己レビュー結果

**設計書の各節と、それを実装するタスクの対応**

| 設計書 | タスク |
|---|---|
| §3 状態機械 | Task 2 |
| §4.1 経路A（素修飾キー） | Task 8 |
| §4.2 経路B（コンボ） | Task 1（スパイクで採否を決定）、Task 13（選択肢に反映） |
| §4.3 フェイルセーフ | Task 2（`MaxHoldExpired`）、Task 12（タイマー実行） |
| §5 録音＋ASRレーン | Task 3、Task 9 |
| §6 コンテキスト結合 | Task 4、Task 12 |
| §6.1 ストリーミング | Task 5、6、7、12 |
| §7 UI | Task 10、11 |
| §8 設定 | Task 13 |
| §8 権限 | Task 14 |
| §8 計測 | Task 15 |
| §8 リリース（βフラグ） | Task 13 |
| §10 テスト・SLO | Task 16 |

**既知の残作業（意図的に範囲外）**

- 応答の音声読み上げ（TTS）: Issue が「初期はオフ、将来的にオプション」としているため、UIトグルも実装しないなら出さない（Task 13 Step 4 の注記）
- ホットキーのライブ再バインド: NSEventのグローバルモニタが登録解除の口を持たないため、キー変更は再起動後に効く。設定UIにその旨を明記済み
- **音声レベルバーは作らない。** 設計書§7 が挙げていたが、`Worker` は音量を外に出す口を持たず、そのためだけにASRパイプラインへRMSの経路を通すのは、録音中であることを伝えるという目的に対して割に合わない。脈打つ赤いドット（`.ptt-mic--live`）が同じ役目を果たす。レベルの可視化が要ると分かった時点で、`Worker` にRMS通知を足す別Issueにする — 動かないバーを置いておくくらいなら、無い方がいい
