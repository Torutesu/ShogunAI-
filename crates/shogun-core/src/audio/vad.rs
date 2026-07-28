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

/// Milliseconds per frame, used to convert the ms-based knobs into whole frames.
const FRAME_MS: u32 = 1000 / 50;

/// The tunable knobs of the energy VAD, in human units (ms and an RMS floor). Kept separate from
/// the internal frame/sample counts so a caller (settings, tests) reasons in ms and the ms→frames
/// conversion lives in one place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadParams {
    /// RMS threshold above which a frame counts as speech. ~0.01 ≈ -40 dBFS.
    pub rms_floor: f32,
    /// Trailing silence that ends an utterance, in ms.
    pub hangover_ms: u32,
    /// Minimum speech length to keep an utterance, in ms (silence tail excluded).
    pub min_ms: u32,
    /// Hard cap on an utterance's length, in ms (matches the ring's 30 s wall).
    pub max_ms: u32,
}

impl Default for VadParams {
    /// Defaults tuned for meeting speech: ~ -40 dBFS floor, 500 ms hangover, 300 ms min, 30 s max.
    fn default() -> Self {
        VadParams { rms_floor: 0.01, hangover_ms: 500, min_ms: 300, max_ms: 30_000 }
    }
}

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
    /// A VAD with the default parameters (see [`VadParams::default`]).
    pub fn new() -> Self {
        Self::with_params(VadParams::default())
    }

    /// A VAD with explicit parameters. The ms-based knobs are converted to whole frames/samples
    /// here — the one place that conversion happens — so the rest of the state machine counts in
    /// frames and samples only.
    pub fn with_params(p: VadParams) -> Self {
        Vad {
            rms_floor: p.rms_floor,
            hangover_frames: (p.hangover_ms / FRAME_MS) as usize,
            max_samples: (p.max_ms as usize) * SAMPLE_RATE as usize / 1000,
            min_samples: (p.min_ms as usize) * SAMPLE_RATE as usize / 1000,
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
    fn defaults_match_the_documented_values() {
        // The ms→frames/samples conversion must reproduce the original hardcoded constants exactly,
        // so switching new() onto with_params(default) changes nothing.
        let v = Vad::new();
        assert_eq!(v.hangover_frames, 25);
        assert_eq!(v.max_samples, 30 * SAMPLE_RATE as usize);
        assert_eq!(v.min_samples, SAMPLE_RATE as usize * 300 / 1000);
        assert_eq!(v.rms_floor, 0.01);
    }

    #[test]
    fn a_stricter_floor_rejects_speech_the_default_would_keep() {
        // A quiet tone (amp 0.02) sits above the default -40 dBFS floor but below a stricter one.
        // Same audio, different params → different segmentation, proving the knob is wired through.
        let quiet = tone(SAMPLE_RATE as usize, 0.02);
        let silence = vec![0.0_f32; SAMPLE_RATE as usize];

        let mut lenient = Vad::new();
        let mut cuts = lenient.push(&quiet);
        cuts.extend(lenient.push(&silence));
        assert_eq!(cuts.len(), 1, "the default floor should treat the quiet tone as speech");

        let mut strict = Vad::with_params(VadParams { rms_floor: 0.1, ..VadParams::default() });
        let mut strict_cuts = strict.push(&quiet);
        strict_cuts.extend(strict.push(&silence));
        assert!(strict_cuts.is_empty(), "a stricter floor should reject the same quiet tone");
    }

    #[test]
    fn force_flush_at_max_length() {
        let mut v = Vad::new();
        let cuts = v.push(&tone(31 * SAMPLE_RATE as usize, 0.3)); // 31s continuous
        assert!(!cuts.is_empty(), "a 31s run must be force-flushed at 30s");
        assert!(cuts[0].len() <= 30 * SAMPLE_RATE as usize + FRAME);
    }
}
