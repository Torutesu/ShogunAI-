//! Hot layer — the last ~24h of events held in RAM (FR-MEM-01/02).
//!
//! The Hot layer is a *cache*, never the source of truth: every event is written to Warm
//! (the DB) first, then mirrored here for fast context assembly. It is bounded two ways:
//! a time window (default 24h) and a byte budget (default 200MB, FR-MEM-02). When the budget
//! is exceeded the oldest events are evicted (folded away); the count of evicted events is
//! kept so the caller can decide when to summarise. On process start the layer is rebuilt from
//! Warm ([`HotLayer::rebuild_from_warm`]) — Hot holds nothing that isn't already durable.

use std::collections::VecDeque;

use rusqlite::{params, Connection};

/// One event mirrored in the Hot layer. Carries only what context assembly needs; the full row
/// stays in Warm.
#[derive(Debug, Clone, PartialEq)]
pub struct HotEvent {
    pub id: i64,
    pub ts: i64,
    pub source: String,
    pub content: String,
}

impl HotEvent {
    /// Approximate resident size: the string bytes plus a fixed per-entry overhead (struct +
    /// allocator headers + deque slot). Deliberately an over-estimate so the budget is a real
    /// ceiling, not an optimistic one.
    fn approx_bytes(&self) -> usize {
        const OVERHEAD: usize = 64;
        self.source.len() + self.content.len() + OVERHEAD
    }
}

/// Bounds for the Hot layer (FR-MEM-02).
#[derive(Debug, Clone, Copy)]
pub struct HotBounds {
    pub window_ms: i64,
    pub max_bytes: usize,
}

impl Default for HotBounds {
    fn default() -> Self {
        Self { window_ms: 24 * 60 * 60 * 1000, max_bytes: 200 * 1024 * 1024 }
    }
}

/// The in-RAM Hot layer.
#[derive(Debug)]
pub struct HotLayer {
    bounds: HotBounds,
    buf: VecDeque<HotEvent>,
    bytes: usize,
    /// Total events evicted by the byte budget since construction (fold-to-summary signal).
    evicted: u64,
}

impl HotLayer {
    pub fn new(bounds: HotBounds) -> Self {
        Self { bounds, buf: VecDeque::new(), bytes: 0, evicted: 0 }
    }

    pub fn with_defaults() -> Self {
        Self::new(HotBounds::default())
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn byte_size(&self) -> usize {
        self.bytes
    }

    pub fn evicted_count(&self) -> u64 {
        self.evicted
    }

    /// Append an event (assumed newest). Evicts the oldest events until the byte budget holds.
    /// A single event larger than the whole budget is still stored (so nothing is silently
    /// dropped on the write path) but leaves the buffer holding only it.
    pub fn push(&mut self, ev: HotEvent) {
        self.bytes += ev.approx_bytes();
        self.buf.push_back(ev);
        while self.bytes > self.bounds.max_bytes && self.buf.len() > 1 {
            if let Some(old) = self.buf.pop_front() {
                self.bytes -= old.approx_bytes();
                self.evicted += 1;
            }
        }
    }

    /// Drop events older than the time window relative to `now` (the 24h horizon). Returns the
    /// number dropped. Time-based eviction is not counted as `evicted` (that counter is about
    /// the byte-budget fold, not the normal age-out).
    pub fn evict_older_than(&mut self, now: i64) -> usize {
        let cutoff = now - self.bounds.window_ms;
        let mut dropped = 0;
        while let Some(front) = self.buf.front() {
            if front.ts < cutoff {
                if let Some(old) = self.buf.pop_front() {
                    self.bytes -= old.approx_bytes();
                    dropped += 1;
                }
            } else {
                break;
            }
        }
        dropped
    }

    /// The `n` most recent events, newest first — the main material for context assembly.
    pub fn recent(&self, n: usize) -> Vec<HotEvent> {
        self.buf.iter().rev().take(n).cloned().collect()
    }

    /// Rebuild from Warm on startup (FR-MEM-02): load events within the time window, oldest
    /// first, applying the byte budget as they load. Replaces any current contents.
    pub fn rebuild_from_warm(&mut self, conn: &Connection, now: i64) -> Result<(), rusqlite::Error> {
        self.buf.clear();
        self.bytes = 0;
        self.evicted = 0;
        let cutoff = now - self.bounds.window_ms;
        let mut stmt = conn.prepare(
            "SELECT id, ts, source, content FROM event_log WHERE ts >= ?1 ORDER BY ts ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![cutoff], |r| {
            Ok(HotEvent { id: r.get(0)?, ts: r.get(1)?, source: r.get(2)?, content: r.get(3)? })
        })?;
        for row in rows {
            self.push(row?);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_log::{insert, NewEvent};

    fn ev(id: i64, ts: i64, content: &str) -> HotEvent {
        HotEvent { id, ts, source: "capture".into(), content: content.into() }
    }

    #[test]
    fn recent_returns_newest_first() {
        let mut h = HotLayer::with_defaults();
        h.push(ev(1, 10, "a"));
        h.push(ev(2, 20, "b"));
        h.push(ev(3, 30, "c"));
        let r = h.recent(2);
        assert_eq!(r.iter().map(|e| e.id).collect::<Vec<_>>(), vec![3, 2]);
    }

    #[test]
    fn byte_budget_evicts_oldest_and_counts() {
        // Tiny budget so a few entries overflow.
        let bounds = HotBounds { window_ms: i64::MAX, max_bytes: 200 };
        let mut h = HotLayer::new(bounds);
        for i in 0..10 {
            h.push(ev(i, i * 10, &"x".repeat(50))); // ~114 bytes each
        }
        assert!(h.byte_size() <= 200);
        assert!(h.evicted_count() >= 1);
        // The newest survivor must still be present.
        assert_eq!(h.recent(1)[0].id, 9);
    }

    #[test]
    fn oversized_single_event_is_still_stored() {
        let bounds = HotBounds { window_ms: i64::MAX, max_bytes: 100 };
        let mut h = HotLayer::new(bounds);
        h.push(ev(1, 0, &"y".repeat(1000))); // far over budget
        assert_eq!(h.len(), 1, "the write path must not drop the event itself");
    }

    #[test]
    fn time_window_eviction() {
        let bounds = HotBounds { window_ms: 100, max_bytes: usize::MAX };
        let mut h = HotLayer::new(bounds);
        h.push(ev(1, 0, "old"));
        h.push(ev(2, 50, "mid"));
        h.push(ev(3, 200, "new"));
        let dropped = h.evict_older_than(200); // cutoff = 100 → ids 1,2 drop
        assert_eq!(dropped, 2);
        assert_eq!(h.recent(9).iter().map(|e| e.id).collect::<Vec<_>>(), vec![3]);
    }

    #[test]
    fn rebuild_loads_recent_from_warm() {
        let conn = crate::open_in_memory().unwrap();
        // Two events: one inside the 24h window, one far outside.
        let now = 100_000_000_000i64;
        for (ts, hash, content) in [(now - 1000, "h1", "inside"), (now - 48 * 3600 * 1000, "h2", "outside")] {
            insert(
                &conn,
                &NewEvent {
                    ts,
                    source: "capture",
                    kind: "text",
                    app_bundle_id: None,
                    window_title: None,
                    content,
                    content_hash: hash,
                    dwell_ms: 0,
                    display_id: None,
                    window_bounds: None,
                },
            )
            .unwrap();
        }
        let mut h = HotLayer::with_defaults();
        h.rebuild_from_warm(&conn, now).unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(h.recent(1)[0].content, "inside");
    }
}
