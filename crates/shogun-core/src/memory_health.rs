//! Whether the memory store is answering (issue #121).
//!
//! Every `Db` read used to collapse a lock failure and a query failure into the same empty
//! vector a genuinely empty table produces. "Nothing is remembered" and "the memory database
//! failed" then look identical — to the panel, to Context Fusion, and to the user, who is told
//! with confidence that they have no commitments while the store is unreadable underneath.
//!
//! This module holds the two halves that fix that, both content-free by construction:
//! [`MemoryFault`] says *what kind* of failure happened, and [`MemoryHealth`] is the live signal
//! the shell polls to colour the notch. Neither ever carries a row, a query, or captured text —
//! so both are safe in a log line and safe to hand to the webview (コード規約: telemetry and logs
//! never contain capture content).
//!
//! Recovery is deliberate: the state reflects the **last** operation, so a successful read after
//! a transient failure clears it without a relaunch. A poisoned lock cannot produce a success, so
//! that state persists until the process restarts — which is the truth about a poisoned mutex.

use std::sync::atomic::{AtomicI64, AtomicU64, AtomicU8, Ordering};

/// Why an operation against the memory store produced no answer.
///
/// Carries no row content — only the class of failure — so it can go straight into a log line
/// and into the UI signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryFault {
    /// The connection mutex is poisoned: a previous holder panicked while holding it. Every later
    /// operation fails the same way, so this state never clears inside the process.
    LockPoisoned,
    /// SQLite refused the statement — I/O error, corruption, a full disk, a wrong encryption key.
    Query,
}

impl MemoryFault {
    /// Stable tag for logs and the wire. Never a message — only the class.
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryFault::LockPoisoned => "lock_poisoned",
            MemoryFault::Query => "query",
        }
    }
}

/// The result of a memory operation that can fail for a reason other than "there was nothing".
pub type MemoryResult<T> = Result<T, MemoryFault>;

/// The failure class of a store error, as a fixed tag.
///
/// A `rusqlite::Error`'s `Display` can name tables, columns and constraints; those are schema,
/// not user content, but a fixed vocabulary is easier to reason about than a message that grows
/// with the driver — so nothing but these tags ever reaches a log.
pub trait FaultClass {
    fn fault_class(&self) -> &'static str;
}

#[cfg(feature = "db")]
impl FaultClass for rusqlite::Error {
    fn fault_class(&self) -> &'static str {
        use rusqlite::ffi::ErrorCode;
        match self {
            rusqlite::Error::SqliteFailure(e, _) => match e.code {
                ErrorCode::DatabaseCorrupt => "corrupt",
                ErrorCode::DiskFull => "disk_full",
                ErrorCode::DatabaseBusy => "busy",
                ErrorCode::DatabaseLocked => "locked",
                ErrorCode::ReadOnly => "read_only",
                ErrorCode::CannotOpen => "cannot_open",
                ErrorCode::NotADatabase => "not_a_database",
                ErrorCode::PermissionDenied => "permission_denied",
                ErrorCode::SystemIoFailure => "io",
                ErrorCode::ConstraintViolation => "constraint",
                ErrorCode::OperationAborted => "aborted",
                ErrorCode::OperationInterrupted => "interrupted",
                _ => "sqlite",
            },
            rusqlite::Error::QueryReturnedNoRows => "no_rows",
            rusqlite::Error::InvalidColumnType(..) => "column_type",
            rusqlite::Error::InvalidColumnName(_) => "column_name",
            rusqlite::Error::InvalidColumnIndex(_) => "column_index",
            rusqlite::Error::FromSqlConversionFailure(..) => "from_sql",
            rusqlite::Error::ToSqlConversionFailure(_) => "to_sql",
            rusqlite::Error::ExecuteReturnedResults => "execute_returned_rows",
            rusqlite::Error::StatementChangedRows(_) => "changed_rows",
            // `rusqlite::Error` is non_exhaustive: a driver upgrade must not fail the build here,
            // and an unnamed class is still an honest one.
            _ => "other",
        }
    }
}

/// The memory crate's own error. Sqlite failures keep their precise class; the rest name the
/// layer that refused, never the row that caused it.
#[cfg(feature = "db")]
impl FaultClass for shogun_memory::MemoryError {
    fn fault_class(&self) -> &'static str {
        match self {
            shogun_memory::MemoryError::Sqlite(e) => e.fault_class(),
            shogun_memory::MemoryError::Migration(_) => "migration",
            shogun_memory::MemoryError::Integrity(_) => "integrity",
            shogun_memory::MemoryError::EmptyProvenance => "empty_provenance",
        }
    }
}

/// The embedding backfill's error. A model failure is not a store failure, and the tag keeps
/// them apart in the log even though both leave events un-embedded.
#[cfg(feature = "db")]
impl FaultClass for shogun_memory::embed_job::EmbedJobError {
    fn fault_class(&self) -> &'static str {
        match self {
            shogun_memory::embed_job::EmbedJobError::Sqlite(e) => e.fault_class(),
            shogun_memory::embed_job::EmbedJobError::Embed(_) => "embed_model",
        }
    }
}

/// A `String` error from a memory helper that wraps its own failure. There is nothing to
/// classify, and the message itself is not logged.
impl FaultClass for String {
    fn fault_class(&self) -> &'static str {
        "error"
    }
}

const STATE_OK: u8 = 0;
const STATE_POISONED: u8 = 1;
const STATE_QUERY: u8 = 2;

/// The live health of the memory store, shared by every `Db` clone.
///
/// Lock-free on purpose: it is written from the capture thread, the Dream Cycle thread and every
/// command handler, and read on the status poll. A mutex here would be one more thing that can
/// poison — in exactly the subsystem whose failure this exists to report.
#[derive(Debug)]
pub struct MemoryHealth {
    state: AtomicU8,
    faults_total: AtomicU64,
    last_fault_ms: AtomicI64,
}

impl Default for MemoryHealth {
    fn default() -> Self {
        Self {
            state: AtomicU8::new(STATE_OK),
            faults_total: AtomicU64::new(0),
            last_fault_ms: AtomicI64::new(0),
        }
    }
}

impl MemoryHealth {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a failure. `now_ms` comes from the caller's clock (this module reads no clock, so
    /// it stays testable without one).
    pub fn record_fault(&self, fault: MemoryFault, now_ms: i64) {
        let state = match fault {
            MemoryFault::LockPoisoned => STATE_POISONED,
            MemoryFault::Query => STATE_QUERY,
        };
        self.state.store(state, Ordering::Relaxed);
        self.faults_total.fetch_add(1, Ordering::Relaxed);
        self.last_fault_ms.store(now_ms, Ordering::Relaxed);
    }

    /// Record a success — the store answered, so the degraded state lifts.
    ///
    /// `faults_total` deliberately does NOT reset: a store that fails and recovers repeatedly
    /// would otherwise look perfectly healthy at every sampling instant, and the count is the
    /// only trace of that pattern.
    pub fn record_success(&self) {
        // Only write when it changes: this runs on every read, and an unconditional store would
        // bounce the cache line between the capture thread and every reader for nothing.
        if self.state.load(Ordering::Relaxed) != STATE_OK {
            self.state.store(STATE_OK, Ordering::Relaxed);
        }
    }

    /// The current fault, or `None` when the last operation succeeded.
    pub fn fault(&self) -> Option<MemoryFault> {
        match self.state.load(Ordering::Relaxed) {
            STATE_POISONED => Some(MemoryFault::LockPoisoned),
            STATE_QUERY => Some(MemoryFault::Query),
            _ => None,
        }
    }

    pub fn snapshot(&self) -> MemoryHealthSnapshot {
        let fault = self.fault();
        let last = self.last_fault_ms.load(Ordering::Relaxed);
        MemoryHealthSnapshot {
            degraded: fault.is_some(),
            fault,
            faults_total: self.faults_total.load(Ordering::Relaxed),
            last_fault_ms: (last != 0).then_some(last),
        }
    }
}

/// What the shell reads and the webview draws. Counts and classes only — never a message from
/// the driver, never a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct MemoryHealthSnapshot {
    /// The last memory operation failed. The UI shows a degraded-memory warning on this alone.
    pub degraded: bool,
    /// Which kind of failure, when degraded.
    pub fault: Option<MemoryFault>,
    /// Failures since this process started. Monotonic — survives recovery, so a store that keeps
    /// flapping is visible even while `degraded` is false.
    pub faults_total: u64,
    /// When the most recent failure happened (unix ms), if there has been one.
    pub last_fault_ms: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_store_is_not_degraded() {
        let h = MemoryHealth::new();
        assert_eq!(
            h.snapshot(),
            MemoryHealthSnapshot { degraded: false, fault: None, faults_total: 0, last_fault_ms: None }
        );
    }

    #[test]
    fn a_success_after_a_query_failure_lifts_the_degraded_state() {
        let h = MemoryHealth::new();
        h.record_fault(MemoryFault::Query, 1_700);
        let bad = h.snapshot();
        assert!(bad.degraded);
        assert_eq!(bad.fault, Some(MemoryFault::Query));
        assert_eq!(bad.last_fault_ms, Some(1_700));

        h.record_success();
        let good = h.snapshot();
        assert!(!good.degraded, "recovery without a relaunch");
        assert_eq!(good.fault, None);
        // …but the fact that it happened is not erased.
        assert_eq!(good.faults_total, 1);
        assert_eq!(good.last_fault_ms, Some(1_700));
    }

    #[test]
    fn repeated_failures_are_counted_across_recoveries() {
        let h = MemoryHealth::new();
        for i in 0..3 {
            h.record_fault(MemoryFault::Query, 100 + i);
            h.record_success();
        }
        let s = h.snapshot();
        assert!(!s.degraded, "the last operation succeeded");
        assert_eq!(s.faults_total, 3, "a flapping store is still visible in the count");
    }

    #[test]
    fn the_two_faults_stay_distinguishable() {
        let h = MemoryHealth::new();
        h.record_fault(MemoryFault::Query, 1);
        assert_eq!(h.fault(), Some(MemoryFault::Query));
        h.record_fault(MemoryFault::LockPoisoned, 2);
        assert_eq!(h.fault(), Some(MemoryFault::LockPoisoned));
        assert_eq!(MemoryFault::LockPoisoned.as_str(), "lock_poisoned");
        assert_eq!(MemoryFault::Query.as_str(), "query");
    }

    #[test]
    fn the_snapshot_serialises_without_any_message_field() {
        let h = MemoryHealth::new();
        h.record_fault(MemoryFault::LockPoisoned, 42);
        let json = serde_json::to_string(&h.snapshot()).unwrap();
        assert_eq!(
            json,
            r#"{"degraded":true,"fault":"lock_poisoned","faults_total":1,"last_fault_ms":42}"#
        );
    }
}
