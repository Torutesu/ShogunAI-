//! A fixed-capacity RAM buffer of PCM samples that drops the oldest when full (§2).
//!
//! The 30s cap is a design wall, not a tuning knob: without it, "keep what ASR hasn't caught up
//! on" becomes "keep everything", which is a recording. When the writer outruns the reader the
//! oldest audio is discarded, never spilled to disk.

use super::SAMPLE_RATE;

/// Seconds of audio kept at most. 30s matches §2.
pub const MAX_SECONDS: usize = 30;

pub struct Ring {
    buf: std::collections::VecDeque<f32>,
    cap: usize,
}

impl Ring {
    /// A ring holding at most `MAX_SECONDS` of 16 kHz mono audio.
    pub fn new() -> Self {
        let cap = MAX_SECONDS * SAMPLE_RATE as usize;
        Ring { buf: std::collections::VecDeque::with_capacity(cap), cap }
    }

    /// Append samples, dropping the oldest to stay within capacity.
    pub fn push(&mut self, samples: &[f32]) {
        self.buf.extend(samples.iter().copied());
        while self.buf.len() > self.cap {
            self.buf.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Drain everything currently buffered as a contiguous Vec, leaving the ring empty. Used at
    /// stop to flush the final utterance before the PCM is discarded.
    pub fn drain(&mut self) -> Vec<f32> {
        self.buf.drain(..).collect()
    }
}

impl Default for Ring {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_exceeds_capacity() {
        let mut r = Ring::new();
        let cap = MAX_SECONDS * SAMPLE_RATE as usize;
        r.push(&vec![0.1_f32; cap + 5_000]);
        assert_eq!(r.len(), cap, "ring grew past the 30s wall");
    }

    #[test]
    fn drops_oldest_first() {
        let mut r = Ring { buf: std::collections::VecDeque::new(), cap: 3 };
        r.push(&[1.0, 2.0, 3.0]);
        r.push(&[4.0]); // 1.0 should fall off the front
        assert_eq!(r.drain(), vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn drain_empties() {
        let mut r = Ring::new();
        r.push(&[1.0, 2.0]);
        assert_eq!(r.drain(), vec![1.0, 2.0]);
        assert!(r.is_empty());
    }
}
