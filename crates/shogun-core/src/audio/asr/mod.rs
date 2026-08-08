//! The ASR seam. `worker` depends on this trait, not on a specific engine — so the pipeline is
//! tested with a deterministic fake, and backends (Deepgram Nova-3 default, optional whisper
//! fallback, future Apple SpeechAnalyzer) plug in behind the same shape.

#[cfg(feature = "net")]
pub mod deepgram;

#[cfg(all(feature = "audio", target_os = "macos"))]
pub mod whisper;

use super::Segment;

/// Turn 16 kHz mono f32 PCM into text. Given an in-memory slice — never a file path — so no caller
/// can be tempted to spill audio to disk (invariant 2 / SHOGUN-local waveform rule).
pub trait Transcriber: Send {
    /// Transcribe one utterance. Returns zero or more lines. An empty result is normal (silence,
    /// or audio the model could not read) and must not be an error the caller has to handle.
    fn transcribe(&mut self, pcm: &[f32]) -> Vec<Segment>;

    /// Translate speech to English (whisper `translate` task). Default: not supported — Deepgram
    /// path uses Select KK for JA→EN instead.
    fn translate_to_english(&mut self, pcm: &[f32]) -> Vec<Segment> {
        let _ = pcm;
        Vec::new()
    }
}

/// Deterministic stand-in for pipeline tests: emits one line whose text encodes the sample count,
/// so a test can assert the worker forwarded the right audio without a model.
#[derive(Default)]
pub struct FakeTranscriber {
    pub calls: usize,
}

impl Transcriber for FakeTranscriber {
    fn transcribe(&mut self, pcm: &[f32]) -> Vec<Segment> {
        self.calls += 1;
        if pcm.is_empty() {
            return Vec::new();
        }
        vec![Segment { text: format!("utterance-{}-samples", pcm.len()), confidence: 0.99 }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_emits_one_line_per_nonempty_utterance() {
        let mut t = FakeTranscriber::default();
        assert_eq!(t.transcribe(&[]).len(), 0);
        let out = t.transcribe(&[0.1, 0.2, 0.3]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "utterance-3-samples");
        assert_eq!(t.calls, 2);
    }
}
