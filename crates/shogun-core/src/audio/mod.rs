//! The meeting audio lane (MT3, FR-MT-13): microphone + system audio → ASR → text.
//!
//! Default ASR is Deepgram Nova-3 (cloud live STT, CLAUDE.md 2026-08-05 exception). Whisper remains
//! an optional offline/dev fallback. SHOGUN never writes the waveform to disk; Deepgram egress is
//! process-only with `mip_opt_out=true` and a third-party traceability row (duration digest only).
//!
//! The pure-logic pieces (`ring`, `resample`, `vad`, the `Transcriber`/`AudioSource` traits, and
//! `worker`) are dependency-light and unit-tested on Linux CI with fakes. The real FFI backends
//! (`capture::mic`, `capture::system_tap`, `asr::whisper`) live behind the `audio` feature and
//! `#[cfg(target_os = "macos")]`. `asr::deepgram` needs the `net` feature.

pub mod asr;
pub mod capture;
pub mod resample;
pub mod ring;
pub mod vad;
pub mod worker;

/// Who is speaking, decided by capture source: mic = `Me`, system tap = `Other`. Re-exported as
/// the same idea `shogun-memory` persists, but kept as its own type so core does not depend on the
/// memory crate for pure-logic tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    Me,
    Other,
}

/// 16 kHz mono is the rate every backend and the VAD agree on, fixed here so the number lives in
/// one place.
pub const SAMPLE_RATE: u32 = 16_000;

/// A span of speech cut out by the VAD, ready for ASR. `pcm` is 16 kHz mono f32 and owned so the
/// capture thread can reuse its own buffers immediately.
#[derive(Debug, Clone, PartialEq)]
pub struct Utterance {
    pub speaker: Speaker,
    /// epoch ms at the first sample.
    pub started_at: i64,
    pub pcm: Vec<f32>,
}

/// One line back from a `Transcriber`.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub text: String,
    /// Model certainty, already normalised to [0,1].
    pub confidence: f64,
}
