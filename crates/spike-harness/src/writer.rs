//! Non-blocking record capture + JSONL flushing (spec §4.1, §4.4).
//!
//! Hot paths (event handlers, state machine) must never do file I/O. They push
//! [`Record`]s into a bounded [`RingBuffer`]; a separate drainer serializes and writes
//! them. Overflow drops the oldest record and increments a counter (silence is never
//! reported as success — spec §4.5), so a stall shows up as dropped records rather than
//! blocking the capture path.

use crate::record::Record;
use std::io::Write;

/// Bounded FIFO of pending records. Push is O(1) and never blocks on I/O.
#[derive(Debug)]
pub struct RingBuffer {
    cap: usize,
    buf: std::collections::VecDeque<Record>,
    dropped: u64,
}

impl RingBuffer {
    /// Spec §4.1 uses capacity 8192.
    pub fn new(cap: usize) -> Self {
        assert!(cap > 0, "capacity must be positive");
        Self { cap, buf: std::collections::VecDeque::with_capacity(cap), dropped: 0 }
    }

    /// Enqueue a record, dropping the oldest if full.
    pub fn push(&mut self, r: Record) {
        if self.buf.len() == self.cap {
            self.buf.pop_front();
            self.dropped += 1;
        }
        self.buf.push_back(r);
    }

    /// Number of records discarded due to overflow since start.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Move all pending records out for flushing.
    pub fn drain(&mut self) -> Vec<Record> {
        self.buf.drain(..).collect()
    }
}

/// Serialize records to a `Write` as newline-delimited JSON.
/// Returns the number of lines written. A record that fails to serialize is skipped
/// and counted in `skipped` rather than aborting the flush.
pub fn flush_to<W: Write>(records: &[Record], out: &mut W) -> std::io::Result<FlushStats> {
    let mut stats = FlushStats::default();
    for r in records {
        match r.to_line() {
            Ok(line) => {
                out.write_all(line.as_bytes())?;
                out.write_all(b"\n")?;
                stats.written += 1;
            }
            Err(_) => stats.skipped += 1,
        }
    }
    Ok(stats)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlushStats {
    pub written: u64,
    pub skipped: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Body, ExpandLatency, Mode};

    fn rec(mono: u64) -> Record {
        Record::new(
            mono,
            mono,
            Body::ExpandLatency(ExpandLatency {
                latency_ms: 50.0,
                total_perceived_ms: 150.0,
                hover_enter_offset_ms: 100.0,
                mode: Mode::Notch,
                fullscreen: false,
                display_count: 1,
            }),
        )
    }

    #[test]
    fn overflow_drops_oldest_and_counts() {
        let mut rb = RingBuffer::new(2);
        rb.push(rec(1));
        rb.push(rec(2));
        rb.push(rec(3)); // drops rec(1)
        assert_eq!(rb.dropped(), 1);
        let drained = rb.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].mono, 2);
        assert_eq!(drained[1].mono, 3);
        assert!(rb.is_empty());
    }

    #[test]
    fn flush_writes_one_line_per_record() {
        let recs = vec![rec(1), rec(2), rec(3)];
        let mut out: Vec<u8> = Vec::new();
        let stats = flush_to(&recs, &mut out).unwrap();
        assert_eq!(stats.written, 3);
        assert_eq!(stats.skipped, 0);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.lines().count(), 3);
        // Each line is independently valid JSON.
        for line in text.lines() {
            let _: serde_json::Value = serde_json::from_str(line).unwrap();
        }
    }
}
