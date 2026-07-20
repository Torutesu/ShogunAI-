//! Internal event bus (spec §5.3, AR-06/07).
//!
//! A single in-process broadcast bus carrying the daemon's cross-module events. Built on
//! `tokio::sync::broadcast`, whose bounded ring gives exactly the required backpressure
//! semantics (AR-07): a slow subscriber never blocks the publisher — instead it *lags* and the
//! oldest events it hasn't read are dropped for it. Those drops are counted so the count can be
//! surfaced as a metric (AR-07 "record the drop count").
//!
//! The publisher (especially capture) must never block; [`Bus::publish`] is non-blocking and
//! ignores the "no subscribers" condition.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::broadcast;

/// The event kinds on the bus (AR-06). Payloads are intentionally small — ids and short tags —
/// so the bus stays cheap and modules stay decoupled (the full data lives in Warm / state
/// tables, addressed by id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusEvent {
    /// New captured text was written to the event log.
    CaptureText { event_id: i64 },
    /// The frontmost app/window changed — triggers a context-cache rebuild.
    FocusChanged { pid: i32, bundle_id: String },
    /// The context cache was rebuilt.
    CacheUpdated,
    /// A state row changed (people/projects/commitments/open_loops).
    StateUpdated { table: &'static str, state_id: i64 },
    /// An action was proposed for the user (L1/L2/L3 = 1/2/3).
    ActionProposed { action_id: u64, level: u8 },
    /// An action was executed.
    ActionExecuted { action_id: u64 },
    /// An integration finished a sync.
    IntegrationSynced { source: &'static str, count: u64 },
    /// A component raised an error (drives the Notch indicator colour).
    ErrorRaised { code: &'static str },
}

/// The bus handle. Cheap to clone; all clones share one channel and one drop counter.
#[derive(Clone)]
pub struct Bus {
    tx: broadcast::Sender<Arc<BusEvent>>,
    dropped: Arc<AtomicU64>,
}

impl Bus {
    /// Create a bus whose per-subscriber ring holds `capacity` events before a slow subscriber
    /// starts lagging (and dropping its oldest unread events).
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity.max(1));
        Self { tx, dropped: Arc::new(AtomicU64::new(0)) }
    }

    /// Publish an event. Non-blocking and infallible from the caller's view: if there are no
    /// subscribers the event is simply discarded (not an error) — the publisher (capture) must
    /// never be blocked or made to handle a delivery failure (AR-07).
    pub fn publish(&self, ev: BusEvent) {
        let _ = self.tx.send(Arc::new(ev));
    }

    /// Subscribe. Each subscriber gets its own ring; a subscriber created now sees only events
    /// published after this call.
    pub fn subscribe(&self) -> Subscriber {
        Subscriber { rx: self.tx.subscribe(), dropped: self.dropped.clone() }
    }

    /// Total events dropped across all subscribers due to lag (AR-07 backpressure metric).
    pub fn dropped_total(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Current live subscriber count.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

/// A bus subscription. `recv` transparently absorbs lag (counting the drops) so a consumer only
/// ever sees a clean stream of the events it *did* receive, newest-available first after a lag.
pub struct Subscriber {
    rx: broadcast::Receiver<Arc<BusEvent>>,
    dropped: Arc<AtomicU64>,
}

impl Subscriber {
    /// Receive the next event. Returns `None` only when the bus is fully closed (all `Bus`
    /// handles dropped). On lag, the skipped count is added to the drop metric and reception
    /// continues from the oldest still-buffered event.
    pub async fn recv(&mut self) -> Option<Arc<BusEvent>> {
        loop {
            match self.rx.recv().await {
                Ok(ev) => return Some(ev),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    self.dropped.fetch_add(n, Ordering::Relaxed);
                    // Loop and read the next still-buffered event.
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }

    /// Non-blocking best-effort receive: `Some` if an event is ready, `None` if not (or closed).
    /// Lag is counted as in [`Self::recv`].
    pub fn try_recv(&mut self) -> Option<Arc<BusEvent>> {
        loop {
            match self.rx.try_recv() {
                Ok(ev) => return Some(ev),
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    self.dropped.fetch_add(n, Ordering::Relaxed);
                }
                Err(_) => return None, // Empty or Closed
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscriber_receives_published_events() {
        let bus = Bus::new(16);
        let mut sub = bus.subscribe();
        bus.publish(BusEvent::CaptureText { event_id: 42 });
        let got = sub.recv().await.unwrap();
        assert_eq!(*got, BusEvent::CaptureText { event_id: 42 });
    }

    #[tokio::test]
    async fn publish_without_subscribers_is_a_noop() {
        let bus = Bus::new(4);
        // No panic, no block, no error surfaced.
        bus.publish(BusEvent::CacheUpdated);
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn multiple_subscribers_each_get_the_event() {
        let bus = Bus::new(8);
        let mut a = bus.subscribe();
        let mut b = bus.subscribe();
        bus.publish(BusEvent::CacheUpdated);
        assert_eq!(*a.recv().await.unwrap(), BusEvent::CacheUpdated);
        assert_eq!(*b.recv().await.unwrap(), BusEvent::CacheUpdated);
    }

    #[tokio::test]
    async fn slow_subscriber_lags_and_drops_are_counted_without_blocking_publisher() {
        // Capacity 2; publish 5 before the subscriber reads → 3 of its oldest are dropped, but
        // every publish returns immediately (the publisher is never blocked).
        let bus = Bus::new(2);
        let mut slow = bus.subscribe();
        for i in 0..5 {
            bus.publish(BusEvent::CaptureText { event_id: i });
        }
        // Drain what survived; recv() absorbs the lag and counts it.
        let mut received = Vec::new();
        while let Some(ev) = slow.try_recv() {
            received.push(ev);
        }
        assert!(bus.dropped_total() >= 3, "dropped={}", bus.dropped_total());
        // The survivors are the newest events, in order.
        assert_eq!(*received.last().unwrap(), Arc::new(BusEvent::CaptureText { event_id: 4 }));
    }

    #[tokio::test]
    async fn recv_returns_none_when_bus_closed() {
        let bus = Bus::new(4);
        let mut sub = bus.subscribe();
        drop(bus);
        assert!(sub.recv().await.is_none());
    }
}
