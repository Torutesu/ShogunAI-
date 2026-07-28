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

/// Poll several sources as one. Round-robins `try_recv` so neither mic nor system tap starves the
/// other; `stop` stops all. This is how the desktop feeds one `Worker` from mic + system tap
/// without the core worker needing to know how many sources there are.
pub struct MultiSource {
    sources: Vec<Box<dyn AudioSource>>,
    next: usize,
}

impl MultiSource {
    pub fn new(sources: Vec<Box<dyn AudioSource>>) -> Self {
        MultiSource { sources, next: 0 }
    }
}

impl AudioSource for MultiSource {
    fn try_recv(&mut self) -> Option<Frame> {
        let n = self.sources.len();
        if n == 0 {
            return None;
        }
        for _ in 0..n {
            let i = self.next % n;
            self.next = self.next.wrapping_add(1);
            if let Some(f) = self.sources.get_mut(i).and_then(|s| s.try_recv()) {
                return Some(f);
            }
        }
        None
    }
    fn stop(&mut self) {
        for s in self.sources.iter_mut() {
            s.stop();
        }
    }
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

    #[test]
    fn multisource_drains_both_and_stops_all() {
        // Uneven lengths on purpose: the round-robin must not drop the mic's extra frame just
        // because the tap ran dry first.
        let a = FakeSource::new(vec![
            Frame { speaker: Speaker::Me, samples: vec![0.1] },
            Frame { speaker: Speaker::Me, samples: vec![0.2] },
        ]);
        let b = FakeSource::new(vec![Frame { speaker: Speaker::Other, samples: vec![0.9] }]);
        let mut multi = MultiSource::new(vec![Box::new(a), Box::new(b)]);

        let mut me = 0;
        let mut other = 0;
        while let Some(f) = multi.try_recv() {
            match f.speaker {
                Speaker::Me => me += 1,
                Speaker::Other => other += 1,
            }
        }
        assert_eq!(me, 2, "both mic frames come out");
        assert_eq!(other, 1, "the tap frame comes out too");
        assert!(multi.try_recv().is_none(), "empty once both sources are drained");
    }

    #[test]
    fn multisource_stop_propagates_to_all() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        // A source whose only job is to record that `stop` reached it.
        struct StopSpy(Arc<AtomicUsize>);
        impl AudioSource for StopSpy {
            fn try_recv(&mut self) -> Option<Frame> {
                None
            }
            fn stop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let stops = Arc::new(AtomicUsize::new(0));
        let mut multi = MultiSource::new(vec![
            Box::new(StopSpy(stops.clone())),
            Box::new(StopSpy(stops.clone())),
        ]);
        multi.stop();
        assert_eq!(stops.load(Ordering::SeqCst), 2, "stop reaches every source");
    }

    #[test]
    fn multisource_empty_never_panics() {
        let mut multi = MultiSource::new(vec![]);
        assert!(multi.try_recv().is_none());
        multi.stop();
    }
}
