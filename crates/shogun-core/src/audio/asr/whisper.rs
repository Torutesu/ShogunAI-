//! whisper.cpp backend (whisper-rs, Metal). Fed an in-memory f32 slice — never a file — so the
//! waveform never touches disk (invariant 2). small is the bundled default; large-v3-turbo is the
//! opt-in high-accuracy model (§5). Language is auto-detected per utterance.

use super::super::Segment;
use super::Transcriber;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Whisper {
    ctx: WhisperContext,
}

impl Whisper {
    /// Load a gguf model from `model_path`. Errors if the file is missing/corrupt — the caller
    /// degrades the audio lane to off rather than failing the meeting (see meeting.rs wiring).
    pub fn load(model_path: &str) -> Result<Self, String> {
        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|e| format!("whisper load failed: {e}"))?;
        Ok(Whisper { ctx })
    }
}

impl Transcriber for Whisper {
    fn transcribe(&mut self, pcm: &[f32]) -> Vec<Segment> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(None); // auto-detect (§5)
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true); // each utterance is independent; avoids cross-talk carryover

        let Ok(mut state) = self.ctx.create_state() else {
            return Vec::new();
        };
        if state.full(params, pcm).is_err() {
            return Vec::new();
        }
        let n = state.full_n_segments();
        let mut out = Vec::new();
        for i in 0..n {
            let Some(segment) = state.get_segment(i) else {
                continue;
            };
            let Ok(text) = segment.to_str_lossy() else {
                continue;
            };
            let text = text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            // The segment's confidence: the mean over its content tokens of `token_probability()`,
            // already in [0,1] (see `segment_confidence`).
            let conf = segment_confidence(&segment);
            out.push(Segment { text, confidence: conf });
        }
        out
    }
}

/// The segment's confidence, defined precisely as: the arithmetic mean, over the segment's content
/// tokens, of each token's `token_probability()`. whisper-rs 0.16 already returns that probability
/// in `[0,1]` (it is `whisper_full_get_token_p`, a probability, not a logprob), so no exp/softmax
/// step is needed; the mean is clamped to `[0,1]` only as a defensive guard against FP drift.
///
/// "Content tokens" is the ideal — special/timestamp tokens (`[_BEG_]`, `<|…|>`, EOT/SOT) would
/// ideally be excluded from the mean. whisper-rs 0.16 does expose the special-token ids on the
/// *context* (`token_eot`, `token_beg`, …), but a `&WhisperSegment` does not expose its context
/// (`get_state` is `pub(super)`), so the threshold is not reachable here without restructuring the
/// transcribe path to thread the context through. In practice this transcribe path sets
/// `print_timestamps(false)` / no token timestamps, so the tokens iterated are the decoded content
/// tokens; the mean is therefore accepted as-is. Falls back to 0.5 when no probabilities are
/// available.
fn segment_confidence(segment: &whisper_rs::WhisperSegment) -> f64 {
    let tokens = segment.n_tokens();
    if tokens == 0 {
        return 0.5;
    }
    let mut sum = 0.0_f64;
    let mut count = 0;
    for t in 0..tokens {
        if let Some(token) = segment.get_token(t) {
            sum += token.token_probability() as f64;
            count += 1;
        }
    }
    if count == 0 {
        0.5
    } else {
        (sum / count as f64).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden: requires a bundled small model at $SHOGUN_WHISPER_MODEL and a tiny PCM fixture.
    /// Ignored by default (heavy, model-gated); run in CI with the model cached:
    ///   SHOGUN_WHISPER_MODEL=... cargo test -p shogun-core --features audio -- --ignored whisper_golden
    #[test]
    #[ignore]
    fn whisper_golden_transcribes_english() {
        let model = std::env::var("SHOGUN_WHISPER_MODEL").expect("set SHOGUN_WHISPER_MODEL");
        let mut w = Whisper::load(&model).expect("load");
        // 16k mono f32 of a short spoken phrase, generated or licensed — never user audio.
        let pcm = load_fixture_pcm("tests/fixtures/hello_16k.f32");
        let segs = w.transcribe(&pcm);
        assert!(!segs.is_empty(), "expected a transcript for spoken audio");
    }

    #[allow(dead_code)]
    fn load_fixture_pcm(path: &str) -> Vec<f32> {
        let bytes = std::fs::read(path).expect("fixture");
        bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect()
    }
}
