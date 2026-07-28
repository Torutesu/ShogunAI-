//! Cut a continuous stream into utterances at silence boundaries (§2).
//!
//! Energy-based and stateful: frames above an RMS floor are speech; once speech has been seen and
//! silence persists for `hangover`, the utterance is emitted. A speech run that reaches
//! `max_samples` is force-flushed so nothing can grow past the ring's 30s wall. Deliberately
//! simple and dependency-free so the boundary logic is exhaustively testable; a spectral VAD can
//! replace it behind the same `push`/`flush` shape.

use super::SAMPLE_RATE;

/// 20 ms frames at 16 kHz.
const FRAME: usize = SAMPLE_RATE as usize / 50;

pub struct Vad {
    rms_floor: f32,
    hangover_frames: usize,
    max_samples: usize,
    min_samples: usize,
    cur: Vec<f32>,
    /// Samples in `cur` that were classified as speech (trailing hangover silence excluded), so the
    /// `min_samples` gate measures actual speech, not speech plus its silence tail.
    speech_samples: usize,
    in_speech: bool,
    silence_run: usize,
    pending_frame: Vec<f32>,
}

/// One completed utterance's samples, relative to the stream. The caller stamps the wall-clock
/// time and speaker.
pub type Cut = Vec<f32>;

impl Vad {
    /// Defaults tuned for meeting speech: ~ -40 dBFS floor, 500 ms hangover, 30 s max, 300 ms min.
    pub fn new() -> Self {
        Vad {
            rms_floor: 0.01,
            hangover_frames: 25, // 25 * 20ms = 500ms
            max_samples: 30 * SAMPLE_RATE as usize,
            min_samples: SAMPLE_RATE as usize * 300 / 1000,
            cur: Vec::new(),
            speech_samples: 0,
            in_speech: false,
            silence_run: 0,
            pending_frame: Vec::new(),
        }
    }

    fn is_speech(frame: &[f32], floor: f32) -> bool {
        let sum_sq: f32 = frame.iter().map(|s| s * s).sum();
        (sum_sq / frame.len() as f32).sqrt() > floor
    }

    /// Feed samples; returns any utterances that completed within this chunk.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Cut> {
        let mut out = Vec::new();
        self.pending_frame.extend_from_slice(samples);
        while self.pending_frame.len() >= FRAME {
            let frame: Vec<f32> = self.pending_frame.drain(..FRAME).collect();
            let speech = Self::is_speech(&frame, self.rms_floor);
            if speech {
                self.in_speech = true;
                self.silence_run = 0;
                self.cur.extend_from_slice(&frame);
                self.speech_samples += frame.len();
            } else if self.in_speech {
                self.cur.extend_from_slice(&frame);
                self.silence_run += 1;
                if self.silence_run >= self.hangover_frames {
                    if let Some(c) = self.take() {
                        out.push(c);
                    }
                }
            }
            if self.cur.len() >= self.max_samples {
                if let Some(c) = self.take() {
                    out.push(c);
                }
            }
        }
        out
    }

    /// Emit whatever speech is buffered (used at stop). `None` if nothing usable.
    pub fn flush(&mut self) -> Option<Cut> {
        self.take()
    }

    fn take(&mut self) -> Option<Cut> {
        self.in_speech = false;
        self.silence_run = 0;
        let enough = self.speech_samples >= self.min_samples;
        self.speech_samples = 0;
        if enough {
            Some(std::mem::take(&mut self.cur))
        } else {
            self.cur.clear();
            None
        }
    }
}

impl Default for Vad {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, amp: f32) -> Vec<f32> {
        (0..n).map(|i| amp * ((i as f32) * 0.3).sin()).collect()
    }

    #[test]
    fn silence_alone_yields_nothing() {
        let mut v = Vad::new();
        let cuts = v.push(&vec![0.0_f32; SAMPLE_RATE as usize]);
        assert!(cuts.is_empty());
        assert!(v.flush().is_none());
    }

    #[test]
    fn speech_then_silence_emits_one_utterance() {
        let mut v = Vad::new();
        let mut cuts = v.push(&tone(SAMPLE_RATE as usize, 0.3)); // 1s speech
        cuts.extend(v.push(&vec![0.0_f32; SAMPLE_RATE as usize])); // 1s silence > hangover
        assert_eq!(cuts.len(), 1, "expected exactly one utterance");
        assert!(cuts[0].len() >= SAMPLE_RATE as usize);
    }

    #[test]
    fn too_short_speech_is_dropped() {
        let mut v = Vad::new();
        let mut cuts = v.push(&tone(SAMPLE_RATE as usize / 20, 0.3)); // 50ms < 300ms min
        cuts.extend(v.push(&vec![0.0_f32; SAMPLE_RATE as usize]));
        assert!(cuts.is_empty());
    }

    #[test]
    fn force_flush_at_max_length() {
        let mut v = Vad::new();
        let cuts = v.push(&tone(31 * SAMPLE_RATE as usize, 0.3)); // 31s continuous
        assert!(!cuts.is_empty(), "a 31s run must be force-flushed at 30s");
        assert!(cuts[0].len() <= 30 * SAMPLE_RATE as usize + FRAME);
    }
}
