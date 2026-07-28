//! Ties capture → per-speaker VAD → ASR → sink into one pollable unit (§3).
//!
//! Kept sink-agnostic via `SegmentSink` so the pure pipeline is tested without a database, then
//! wired to `transcript_segments` at the call site. One `Vad` per speaker: mic and system audio
//! are independent streams and must not have their silence boundaries interleaved.

use super::asr::Transcriber;
use super::capture::{AudioSource, Frame};
use super::vad::Vad;
use super::{Speaker, Utterance};

/// Where finished lines go. The desktop implements this over `transcript_segments`; tests collect
/// into a Vec.
pub trait SegmentSink {
    fn emit(&mut self, u: &Utterance, text: &str, confidence: f64);
}

pub struct Worker<S: AudioSource, T: Transcriber> {
    source: S,
    asr: T,
    vad_me: Vad,
    vad_other: Vad,
    /// Wall-clock ms of the current step, set by the driver each poll.
    now: i64,
}

impl<S: AudioSource, T: Transcriber> Worker<S, T> {
    pub fn new(source: S, asr: T) -> Self {
        Worker { source, asr, vad_me: Vad::new(), vad_other: Vad::new(), now: 0 }
    }

    fn transcribe_cut(&mut self, speaker: Speaker, pcm: Vec<f32>, sink: &mut dyn SegmentSink) {
        let u = Utterance { speaker, started_at: self.now, pcm };
        for seg in self.asr.transcribe(&u.pcm) {
            let text = seg.text.trim();
            if !text.is_empty() {
                sink.emit(&u, text, seg.confidence);
            }
        }
        // u (and its pcm) is dropped here — the waveform never outlives transcription.
    }

    /// Drain all currently-available frames, transcribing any utterances they complete. Returns the
    /// number of frames consumed (0 means idle). Non-blocking.
    pub fn poll(&mut self, now: i64, sink: &mut dyn SegmentSink) -> usize {
        self.now = now;
        let mut consumed = 0;
        while let Some(Frame { speaker, samples }) = self.source.try_recv() {
            consumed += 1;
            let vad = match speaker {
                Speaker::Me => &mut self.vad_me,
                Speaker::Other => &mut self.vad_other,
            };
            for cut in vad.push(&samples) {
                self.transcribe_cut(speaker, cut, sink);
            }
        }
        consumed
    }

    /// Stop capture and flush the final utterance on each speaker before the buffers are dropped.
    pub fn stop(&mut self, now: i64, sink: &mut dyn SegmentSink) {
        self.now = now;
        self.source.stop();
        if let Some(cut) = self.vad_me.flush() {
            self.transcribe_cut(Speaker::Me, cut, sink);
        }
        if let Some(cut) = self.vad_other.flush() {
            self.transcribe_cut(Speaker::Other, cut, sink);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::asr::FakeTranscriber;
    use crate::audio::capture::FakeSource;
    use crate::audio::SAMPLE_RATE;

    #[derive(Default)]
    struct VecSink {
        lines: Vec<(Speaker, String, f64)>,
    }
    impl SegmentSink for VecSink {
        fn emit(&mut self, u: &Utterance, text: &str, confidence: f64) {
            self.lines.push((u.speaker, text.to_string(), confidence));
        }
    }

    fn tone(n: usize) -> Vec<f32> {
        (0..n).map(|i| 0.3 * ((i as f32) * 0.3).sin()).collect()
    }

    #[test]
    fn speech_then_silence_produces_a_line_via_asr() {
        let frames = vec![
            Frame { speaker: Speaker::Me, samples: tone(SAMPLE_RATE as usize) },
            Frame { speaker: Speaker::Me, samples: vec![0.0; SAMPLE_RATE as usize] },
        ];
        let mut w = Worker::new(FakeSource::new(frames), FakeTranscriber::default());
        let mut sink = VecSink::default();
        w.poll(1_000, &mut sink);
        assert_eq!(sink.lines.len(), 1);
        assert_eq!(sink.lines[0].0, Speaker::Me);
        assert!(sink.lines[0].1.starts_with("utterance-"));
    }

    #[test]
    fn mic_and_system_are_segmented_independently() {
        let frames = vec![
            Frame { speaker: Speaker::Me, samples: tone(SAMPLE_RATE as usize) },
            Frame { speaker: Speaker::Other, samples: tone(SAMPLE_RATE as usize) },
            Frame { speaker: Speaker::Me, samples: vec![0.0; SAMPLE_RATE as usize] },
            Frame { speaker: Speaker::Other, samples: vec![0.0; SAMPLE_RATE as usize] },
        ];
        let mut w = Worker::new(FakeSource::new(frames), FakeTranscriber::default());
        let mut sink = VecSink::default();
        w.poll(1_000, &mut sink);
        let speakers: Vec<Speaker> = sink.lines.iter().map(|l| l.0).collect();
        assert!(speakers.contains(&Speaker::Me) && speakers.contains(&Speaker::Other));
    }

    #[test]
    fn stop_flushes_trailing_speech() {
        let frames = vec![Frame { speaker: Speaker::Me, samples: tone(SAMPLE_RATE as usize) }];
        let mut w = Worker::new(FakeSource::new(frames), FakeTranscriber::default());
        let mut sink = VecSink::default();
        w.poll(1_000, &mut sink); // no trailing silence yet → nothing emitted
        assert_eq!(sink.lines.len(), 0);
        w.stop(2_000, &mut sink); // flush emits the buffered utterance
        assert_eq!(sink.lines.len(), 1);
    }
}
