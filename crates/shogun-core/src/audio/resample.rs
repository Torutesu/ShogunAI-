//! Bring any capture stream to the one rate the VAD and ASR agree on: 16 kHz mono f32 (§3).
//!
//! Linear interpolation, deliberately dependency-free — ASR is robust to the mild aliasing this
//! introduces, and it keeps the pure-logic layer unit-testable on CI. Swap in a polyphase
//! resampler (rubato) later behind the same signature if measurement shows it matters.

use super::SAMPLE_RATE;

/// Downmix to mono by averaging interleaved channels.
pub fn to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let ch = channels as usize;
    interleaved
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

/// Resample mono `input` from `in_rate` to 16 kHz by linear interpolation.
pub fn to_16k_mono(input: &[f32], in_rate: u32) -> Vec<f32> {
    if in_rate == SAMPLE_RATE || input.is_empty() {
        return input.to_vec();
    }
    let ratio = SAMPLE_RATE as f64 / in_rate as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let j = src.floor() as usize;
        let frac = (src - j as f64) as f32;
        let a = input[j.min(input.len() - 1)];
        let b = input[(j + 1).min(input.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_passthrough() {
        assert_eq!(to_mono(&[1.0, 2.0, 3.0], 1), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn stereo_averages_to_mono() {
        // L,R,L,R → averaged
        assert_eq!(to_mono(&[0.0, 2.0, 4.0, 8.0], 2), vec![1.0, 6.0]);
    }

    #[test]
    fn same_rate_is_passthrough() {
        assert_eq!(to_16k_mono(&[1.0, 2.0], 16_000), vec![1.0, 2.0]);
    }

    #[test]
    fn downsampling_halves_length_from_32k() {
        let input = vec![0.5_f32; 320]; // 320 @ 32k → ~160 @ 16k
        let out = to_16k_mono(&input, 32_000);
        assert!((out.len() as i64 - 160).abs() <= 1, "unexpected length {}", out.len());
        assert!(out.iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }
}
