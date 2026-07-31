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
    /// 進行中の hold に他のキーやマウスが割り込んだ。Recording を潰す点は `Cancel` と
    /// 同じだが、**Recording でなければ何もしない** — MaxHold 満了後や文字起こし中に届いた
    /// 割込みが、確定済みのセッションを壊してはならない。
    HoldInterrupted,
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
                // Recording へ入る唯一の腕が hold_started_at を必ず設定するので、ここは常に Some。
                // 破れたときに黙って誤爆判定に落ちると原因が追えないので、デバッグビルドで露見させる。
                debug_assert!(self.hold_started_at.is_some(), "Recording 中は hold_started_at が Some");
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
            // モニタ由来の割込み（`HoldInterrupted`）も Esc/パネル操作の `Cancel` と、
            // Recording を潰す効果は同じ。両者の違いは Recording 以外での扱い —
            // `HoldInterrupted` は下の catch-all で no-op に落ち、確定済みセッションを
            // 壊さない（MaxHold 満了後にキーを握ったまま別キーが入っても文字起こしを守る）。
            (S::Recording, I::Cancel | I::HoldInterrupted) => {
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

    /// 録音中の割込み（他キー/マウス）は Esc と同じく録音を捨てる。送信はしない。
    #[test]
    fn a_hold_interrupt_while_recording_discards_without_submitting() {
        let mut m = machine();
        m.step(Input::HoldStart { at_ms: 1_000 });
        let fx = m.step(Input::HoldInterrupted);

        assert_eq!(m.state(), State::Idle);
        assert!(fx.contains(&Effect::DiscardCapture));
        assert!(fx.contains(&Effect::HidePanel));
        assert!(!fx.iter().any(|e| matches!(e, Effect::SubmitToAgent(_))));
    }

    /// MaxHold 満了で Transcribing に進んだあと、まだキーを握ったまま別キーが入っても
    /// 確定済み録音を壊さない。`HoldInterrupted` は Recording 以外では効果ゼロ。
    #[test]
    fn a_hold_interrupt_after_max_hold_does_not_disturb_transcribing() {
        let mut m = machine();
        m.step(Input::HoldStart { at_ms: 0 });
        m.step(Input::MaxHoldExpired { at_ms: 30_000 }); // → Transcribing
        assert_eq!(m.state(), State::Transcribing);

        let fx = m.step(Input::HoldInterrupted);
        assert_eq!(m.state(), State::Transcribing, "割込みで確定済みセッションが壊れた");
        assert!(fx.is_empty(), "Transcribing 中の HoldInterrupted は何も出さない");
    }

    /// 応答中の割込みも効果ゼロ。読んでいる応答をモニタ由来の割込みで畳まない。
    #[test]
    fn a_hold_interrupt_while_responding_is_a_no_op() {
        let mut m = machine();
        m.step(Input::HoldStart { at_ms: 1_000 });
        m.step(Input::HoldEnd { at_ms: 3_000 });
        m.step(Input::Transcribed("hi".into())); // → Responding
        assert_eq!(m.state(), State::Responding);

        let fx = m.step(Input::HoldInterrupted);
        assert_eq!(m.state(), State::Responding);
        assert!(fx.is_empty(), "Responding 中の HoldInterrupted は何も出さない");
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
            Input::HoldInterrupted,
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
            Input::HoldInterrupted,
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

    /// 文字起こし中に押し直しても新しい録音が始まり、遅れて届いた前回の文字起こしは
    /// 何も動かさない。飛行中のASRがあることが、この経路を他の HoldStart と分けている。
    #[test]
    fn holding_during_transcribing_starts_a_fresh_recording() {
        let mut m = machine();
        m.step(Input::HoldStart { at_ms: 1_000 });
        m.step(Input::HoldEnd { at_ms: 3_000 }); // → Transcribing
        let fx = m.step(Input::HoldStart { at_ms: 4_000 });

        assert_eq!(m.state(), State::Recording);
        assert!(fx.contains(&Effect::StartCapture));

        let stale = m.step(Input::Transcribed("stale".into()));
        assert!(stale.is_empty(), "録音中に届いた古い Transcribed は無視される");
    }

    /// 失敗表示を閉じる操作。Idle のままパネルだけを消す。
    #[test]
    fn dismissing_from_idle_only_hides_the_panel() {
        let mut m = machine();
        m.step(Input::Failed(Fail::MicUnavailable));
        let fx = m.step(Input::Dismiss);

        assert_eq!(m.state(), State::Idle);
        assert_eq!(fx, vec![Effect::HidePanel]);
    }
}
