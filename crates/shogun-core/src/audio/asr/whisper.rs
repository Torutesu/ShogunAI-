//! whisper.cpp backend (whisper-rs, Metal). Fed an in-memory f32 slice — never a file — so the
//! waveform never touches disk (invariant 2). small is the bundled default; large-v3-turbo is the
//! opt-in high-accuracy model (§5).
//!
//! Language is *not* auto-detected per utterance anymore. Device testing found whisper's
//! per-utterance auto-detection misfires on short English lines (it read "Ask not what your country
//! can do for you" as Japanese katakana), and the policy is English-primary, Japanese alongside
//! (`docs/context-layer-audit-and-plan.md` §8). So the meeting's language is fixed for the whole
//! session:
//!   - English/Japanese: whisper is pinned to `"en"`/`"ja"` and never detects — best quality for
//!     the chosen language, and the katakana misfire is impossible.
//!   - Auto: the language is detected *once*, from the first utterance that carries speech, and then
//!     locked for the rest of the session. A `Whisper` instance lives exactly one meeting, so the
//!     lock lives one meeting. This keeps Auto stable within a call instead of flip-flopping line to
//!     line, while still guessing when the user genuinely does not want to choose.

use super::super::Segment;
use super::Transcriber;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Whisper {
    ctx: WhisperContext,
    /// The language to transcribe in, as a whisper code (`"en"`/`"ja"`), or `None` for auto.
    ///
    /// For a fixed language this is set at load and never changes. For Auto it starts `None` and is
    /// overwritten with the detected code the first time detection succeeds — the per-session lock.
    /// It is mutated only through `&mut self` in `transcribe` (which already takes `&mut self`), so
    /// no extra interior-mutability machinery is needed.
    language: Option<String>,
    /// Whether `language` is a *fixed* preference (English/Japanese) or an Auto slot that may still
    /// be filled by detection. Distinguishes "fixed to None" (there is no such thing) from "Auto,
    /// not yet locked": only when this is `false` and `language` is `None` do we run detection.
    fixed: bool,
}

impl Whisper {
    /// Load a gguf model from `model_path`, auto-detecting language (detect-once-then-lock). Kept as
    /// the thin default so the golden test and any auto caller need no code. Errors if the file is
    /// missing/corrupt — the caller degrades the audio lane to off rather than failing the meeting
    /// (see meeting.rs wiring).
    pub fn load(model_path: &str) -> Result<Self, String> {
        Self::load_with_language(model_path, None)
    }

    /// Load a gguf model and fix the transcription language. `lang` is a whisper code
    /// (`Some("en")`/`Some("ja")`) to pin the language, or `None` for Auto (detect once, then lock).
    /// Callers pass `MeetingLanguage::whisper_code()` straight through.
    pub fn load_with_language(model_path: &str, lang: Option<&str>) -> Result<Self, String> {
        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|e| format!("whisper load failed: {e}"))?;
        Ok(Whisper { ctx, language: lang.map(str::to_string), fixed: lang.is_some() })
    }
}

impl Transcriber for Whisper {
    fn transcribe(&mut self, pcm: &[f32]) -> Vec<Segment> {
        let Ok(mut state) = self.ctx.create_state() else {
            return Vec::new();
        };

        // Decide the language for this utterance. A fixed language (English/Japanese) is used as-is.
        // An unlocked Auto session detects from this utterance's audio and, on success, locks the
        // result on `self` so every later utterance reuses it — the per-session lock. On failure we
        // fall back to letting whisper auto-detect *this one* utterance (set_language(None)) without
        // locking, so a bad first utterance never pins the wrong language for the whole meeting.
        let lang: Option<String> = if self.fixed || self.language.is_some() {
            self.language.clone()
        } else {
            match detect_language(&mut state, pcm) {
                Some(code) => {
                    self.language = Some(code.clone()); // lock for the rest of the session
                    Some(code)
                }
                None => None, // detection failed; let whisper try this utterance, stay unlocked
            }
        };

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        // Some("en")/Some("ja") pins whisper (fixing the katakana misfire and giving best quality);
        // None lets whisper auto-detect this utterance — only reached for an Auto session whose
        // detection has not (yet) succeeded.
        params.set_language(lang.as_deref());
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true); // each utterance is independent; avoids cross-talk carryover

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

/// Detect the spoken language of `pcm` and return its whisper code (`"en"`, `"ja"`, …), or `None`
/// if detection cannot run or the id does not map to a known code.
///
/// whisper-rs 0.16 exposes detection as `WhisperState::lang_detect(offset_ms, n_threads)`, which
/// needs the mel spectrogram computed first (`pcm_to_mel`) and returns the detected language id plus
/// per-language probabilities. The id is turned into a code with `whisper_rs::get_lang_str(id)` (the
/// re-exported `whisper_lang_str`). Runs on the *same* state that will then transcribe, so the mel
/// is computed once. Every failure path returns `None` so the caller degrades to whisper's own
/// auto-detect for the one utterance — detection never panics and never takes the meeting down.
fn detect_language(state: &mut whisper_rs::WhisperState, pcm: &[f32]) -> Option<String> {
    // 1 thread is what the rest of this path assumes; lang_detect rejects < 1 anyway.
    if state.pcm_to_mel(pcm, 1).is_err() {
        return None;
    }
    // offset_ms = 0: detect from the start of the utterance's audio.
    let (lang_id, _probs) = state.lang_detect(0, 1).ok()?;
    whisper_rs::get_lang_str(lang_id).map(str::to_string)
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
