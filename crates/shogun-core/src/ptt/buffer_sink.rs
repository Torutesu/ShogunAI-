//! 一発の発話を組み立てる [`SegmentSink`]（Issue #44）。
//!
//! 会議レーンの sink は文字起こしをDBに追記するが、push-to-talk はそれをしない。1回の
//! 発話は1回のプロンプトになって消える寿命のもので、`sessions` に残す interval も無い。
//! ここが不変条件2の最後の関門でもある: 波形は `Worker` のバッファにしか無く、この型が
//! 受け取るのは既にテキストになったものだけで、それもディスクには落ちない。

use crate::audio::worker::SegmentSink;
use crate::audio::Utterance;

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
