//! Non-blocking recording facade + background JSONL flusher (spec §4.1, §4.5).
//!
//! Hot paths call [`Recorder::record`] (stamp `ts`/`mono`, push to the ring buffer — no I/O).
//! A background thread drains and appends to the day's JSONL file every second. The
//! serialization path ([`Recorder::flush_to`]) is unit-tested; the thread is a thin wrapper.

use crate::record::{Body, Record};
use crate::writer::{flush_to, FlushStats, RingBuffer};
use crate::{now_epoch_ms, MonoClock};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Cheaply-cloneable handle shared across capture threads.
#[derive(Clone)]
pub struct Recorder {
    clock: MonoClock,
    buf: Arc<Mutex<RingBuffer>>,
}

impl Recorder {
    /// `cap` is the ring-buffer capacity (spec §4.1 uses 8192).
    pub fn new(cap: usize) -> Self {
        Self { clock: MonoClock::new(), buf: Arc::new(Mutex::new(RingBuffer::new(cap))) }
    }

    /// Stamp and enqueue a record. Never blocks on I/O; drops the oldest on overflow.
    pub fn record(&self, body: Body) {
        let r = Record::new(now_epoch_ms(), self.clock.elapsed_ns(), body);
        if let Ok(mut b) = self.buf.lock() {
            b.push(r);
        }
    }

    /// Records discarded due to overflow since start (a stall shows up here, spec §4.5).
    pub fn dropped(&self) -> u64 {
        self.buf.lock().map(|b| b.dropped()).unwrap_or(0)
    }

    /// Move all pending records out.
    pub fn drain(&self) -> Vec<Record> {
        self.buf.lock().map(|mut b| b.drain()).unwrap_or_default()
    }

    /// Drain and serialize pending records to `w` as JSONL.
    pub fn flush_to<W: Write>(&self, w: &mut W) -> std::io::Result<FlushStats> {
        let recs = self.drain();
        flush_to(&recs, w)
    }

    /// Spawn the background flusher: append pending records to `path` every `interval`.
    /// Runs for the process lifetime (the spike never stops it).
    pub fn spawn_file_flusher(&self, path: PathBuf, interval: Duration) -> std::thread::JoinHandle<()> {
        let me = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(interval);
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                let _ = me.flush_to(&mut f);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{ExpandLatency, Mode};

    fn expand(latency: f64) -> Body {
        Body::ExpandLatency(ExpandLatency {
            latency_ms: latency,
            total_perceived_ms: latency + 100.0,
            hover_enter_offset_ms: 100.0,
            mode: Mode::Notch,
            fullscreen: false,
            display_count: 1,
        })
    }

    #[test]
    fn record_then_flush_produces_jsonl() {
        let rec = Recorder::new(16);
        rec.record(expand(50.0));
        rec.record(expand(60.0));
        let mut out: Vec<u8> = Vec::new();
        let stats = rec.flush_to(&mut out).unwrap();
        assert_eq!(stats.written, 2);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.lines().count(), 2);
        for line in text.lines() {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(v["type"], "metric.expand_latency");
            assert!(v["mono"].as_u64().is_some());
        }
        // Drained: a second flush writes nothing.
        let mut out2: Vec<u8> = Vec::new();
        assert_eq!(rec.flush_to(&mut out2).unwrap().written, 0);
    }

    #[test]
    fn clone_shares_the_same_buffer() {
        let a = Recorder::new(16);
        let b = a.clone();
        a.record(expand(1.0));
        b.record(expand(2.0));
        assert_eq!(b.drain().len(), 2); // both writes land in one buffer
    }
}
