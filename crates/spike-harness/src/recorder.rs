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
        Self::with_clock(cap, MonoClock::new())
    }

    /// Build with a caller-supplied clock so every subsystem (engine t0 markers, record
    /// `mono` stamps, offset estimation) shares ONE monotonic timeline — three independent
    /// `MonoClock::new()` epochs would bias Q2 latency math by their creation skew.
    pub fn with_clock(cap: usize, clock: MonoClock) -> Self {
        Self { clock, buf: Arc::new(Mutex::new(RingBuffer::new(cap))) }
    }

    /// The shared clock (Copy) for stamping externally-computed timestamps on the same
    /// timeline as this recorder's `mono` field.
    pub fn clock(&self) -> MonoClock {
        self.clock
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

    /// Spawn the background flusher: append pending records to `dir/YYYYMMDD.jsonl`
    /// (UTC daily rotation, spec §4.4 — a 24h soak spans at most 2 files) every
    /// `interval`. Skips the file open entirely when nothing was recorded (no idle
    /// syscall churn during the Q3 CPU soak). Runs for the process lifetime.
    pub fn spawn_file_flusher(&self, dir: PathBuf, interval: Duration) -> std::thread::JoinHandle<()> {
        let me = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(interval);
            let recs = me.drain();
            if recs.is_empty() {
                continue;
            }
            let path = dir.join(format!("{}.jsonl", yyyymmdd_utc(now_epoch_ms())));
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                // A failed write drops this batch; the loss is visible as a mono-stamp gap
                // plus the dropped() counter never explains it — accepted for the spike,
                // noted in the report's error margin section.
                let _ = flush_to(&recs, &mut f);
            }
        })
    }
}

/// `YYYYMMDD` (UTC) for an epoch-milliseconds timestamp. Civil-from-days algorithm
/// (Howard Hinnant) — no chrono dependency.
pub fn yyyymmdd_utc(epoch_ms: u64) -> u32 {
    let days = (epoch_ms / 86_400_000) as i64;
    // Shift epoch from 1970-01-01 to 0000-03-01 era.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // year of era
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year (Mar-based)
    let mp = (5 * doy + 2) / 153; // month index Mar=0
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u32) * 10_000 + (m as u32) * 100 + d as u32
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

    #[test]
    fn yyyymmdd_known_dates() {
        assert_eq!(yyyymmdd_utc(0), 19_700_101); // epoch
        assert_eq!(yyyymmdd_utc(86_400_000 - 1), 19_700_101); // last ms of day 0
        assert_eq!(yyyymmdd_utc(86_400_000), 19_700_102);
        // 2000-02-29 (leap): days since epoch = 11016.
        assert_eq!(yyyymmdd_utc(11_016 * 86_400_000), 20_000_229);
        // 2026-07-19: days since epoch = 20653.
        assert_eq!(yyyymmdd_utc(20_653 * 86_400_000), 20_260_719);
        // 2023-12-31 → 2024-01-01 rollover: 2024-01-01 is day 19723.
        assert_eq!(yyyymmdd_utc(19_723 * 86_400_000 - 1), 20_231_231);
        assert_eq!(yyyymmdd_utc(19_723 * 86_400_000), 20_240_101);
    }
}
