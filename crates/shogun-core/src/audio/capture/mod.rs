//! The capture seam. A source pushes 16 kHz mono f32 frames tagged with a speaker. The real
//! backends (`mic` via cpal, `system_tap` via Core Audio) live behind the `audio` feature and
//! macOS; the fake drives worker tests on CI.

#[cfg(all(feature = "audio", target_os = "macos"))]
pub mod mic;
#[cfg(all(feature = "audio", target_os = "macos"))]
pub mod system_tap;

use super::Speaker;

/// A frame of already-resampled 16 kHz mono audio from one source.
pub struct Frame {
    pub speaker: Speaker,
    pub samples: Vec<f32>,
}

/// A running capture source. `try_recv` is non-blocking so the worker can poll mic and tap on one
/// thread without either starving the other. `None` means "no data right now", not "closed".
pub trait AudioSource: Send {
    fn try_recv(&mut self) -> Option<Frame>;
    /// Stop capture and release the device. Idempotent.
    fn stop(&mut self);
}

/// A scripted source for tests: hands out pre-canned frames, then reports empty.
pub struct FakeSource {
    frames: std::collections::VecDeque<Frame>,
    pub stopped: bool,
}

impl FakeSource {
    pub fn new(frames: Vec<Frame>) -> Self {
        FakeSource { frames: frames.into(), stopped: false }
    }
}

impl AudioSource for FakeSource {
    fn try_recv(&mut self) -> Option<Frame> {
        self.frames.pop_front()
    }
    fn stop(&mut self) {
        self.stopped = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_yields_frames_then_empty() {
        let mut s = FakeSource::new(vec![Frame { speaker: Speaker::Me, samples: vec![0.1] }]);
        assert!(s.try_recv().is_some());
        assert!(s.try_recv().is_none());
        s.stop();
        assert!(s.stopped);
    }
}
