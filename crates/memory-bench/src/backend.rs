//! How we connect to SHOGUN.
//!
//! Everything the runner does to memory goes through [`MemoryBackend`]. That indirection is the
//! whole reason this crate is shaped the way it is: a later experiment (selective update,
//! consolidation, a retention policy) implements this trait a second time, and the workload, the
//! metrics and the report stay byte-for-byte the same. If an intervention needed changes to the
//! evaluator, the two runs would no longer be measuring the same thing.
//!
//! [`ShogunBackend`] is the baseline implementation and it deliberately owns no memory logic of
//! its own. It calls [`shogun_memory::event_log::insert_or_touch`] and
//! [`shogun_memory::search::search_hybrid_with_options`] — the real product paths — so the numbers
//! describe SHOGUN, not a reimplementation of SHOGUN that happens to live in a benchmark.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use shogun_memory::search::{SearchDepth, SearchOptions};
use shogun_memory::{event_log, search};

use crate::workload::BenchEvent;

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("memory: {0}")]
    Memory(#[from] shogun_memory::MemoryError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// What one write actually did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteOutcome {
    /// The row the event now lives in — a new row, or the existing row that absorbed it.
    pub event_id: i64,
    /// `true` when the backend recognised this as a repeat and touched an existing row instead of
    /// appending. This is the measurement, not an assumption: the bench never decides for itself
    /// what counted as a duplicate.
    pub deduplicated: bool,
}

/// How big the store is, measured two ways because they answer different questions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StorageSize {
    /// `page_count * page_size` — the logical size SQLite accounts for. Defined for in-memory
    /// databases too, so it is the number that is always comparable across runs.
    pub logical_bytes: u64,
    /// Bytes actually on disk, including the `-wal` and `-shm` sidecars. `None` for an in-memory
    /// database. Larger than `logical_bytes` mid-run, because the WAL has not been checkpointed.
    pub file_bytes: Option<u64>,
}

/// The seam every experiment plugs into.
pub trait MemoryBackend {
    /// Recorded in the report so a result names the thing it measured.
    fn name(&self) -> &'static str;

    /// Ingest one event.
    fn write(&mut self, ev: &BenchEvent) -> Result<WriteOutcome, BackendError>;

    /// Retrieve up to `k` event ids, best first.
    fn search(&self, query: &str, k: usize) -> Result<Vec<i64>, BackendError>;

    /// Rows durably held. Compared against writes submitted to get write amplification.
    fn count(&self) -> Result<i64, BackendError>;

    fn size(&self) -> Result<StorageSize, BackendError>;

    /// Open a bulk-load transaction. Default is a no-op, so a backend with no notion of batching
    /// still satisfies the trait and the runner needs no special case.
    fn begin_batch(&mut self) -> Result<(), BackendError> {
        Ok(())
    }

    fn commit_batch(&mut self) -> Result<(), BackendError> {
        Ok(())
    }
}

/// The current memory layer, unmodified.
pub struct ShogunBackend {
    conn: Connection,
    path: Option<PathBuf>,
}

impl ShogunBackend {
    /// Open a file-backed database. Prefer this over [`Self::in_memory`] for any run whose numbers
    /// are going to be quoted: an in-memory database has no WAL, no page cache eviction and no
    /// filesystem, so its write latency is not the latency the product experiences.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, BackendError> {
        let path = path.as_ref().to_path_buf();
        let conn = shogun_memory::open(&path)?;
        Ok(Self {
            conn,
            path: Some(path),
        })
    }

    /// In-memory database, for the CI smoke run where the point is that the pipeline works at all,
    /// not what it costs.
    pub fn in_memory() -> Result<Self, BackendError> {
        Ok(Self {
            conn: shogun_memory::open_in_memory()?,
            path: None,
        })
    }

    /// Escape hatch for measurements that need the connection directly (the tier and cold-scan
    /// accounting a later commit will add).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

impl MemoryBackend for ShogunBackend {
    fn name(&self) -> &'static str {
        "shogun-memory"
    }

    fn write(&mut self, ev: &BenchEvent) -> Result<WriteOutcome, BackendError> {
        // `content_hash` is left to the memory layer to compute. Passing our own would be passing
        // our own dedup contract, and the dedup contract is one of the things under measurement:
        // `event_log::content_hash` normalises the body first, and a bench that hashed the raw
        // string would report collapses the product would not make.
        let hash = event_log::content_hash(&ev.content);
        let new = event_log::NewEvent {
            ts: ev.ts,
            source: &ev.source,
            kind: &ev.kind,
            app_bundle_id: ev.app_bundle_id.as_deref(),
            window_title: ev.window_title.as_deref(),
            content: &ev.content,
            content_hash: &hash,
            dwell_ms: ev.dwell_ms,
            display_id: None,
            window_bounds: None,
        };
        let (event_id, deduplicated) = event_log::insert_or_touch(&self.conn, &new)?;
        Ok(WriteOutcome {
            event_id,
            deduplicated,
        })
    }

    fn search(&self, query: &str, k: usize) -> Result<Vec<i64>, BackendError> {
        // `query_embedding: None` means the semantic half contributes nothing and RRF fuses the
        // lexical list alone. That is not a shortcut — it is the configuration CI runs in, because
        // the ONNX embedder is behind an off-by-default feature and needs a model file on disk.
        // The report records `semantic: false` so a lexical number is never mistaken for a hybrid
        // one; `retrieval_eval.rs` established that the gap between them is real (recall@5 0.93 vs
        // 1.00), so the two must never be compared to each other.
        let opts = SearchOptions {
            since_ts: None,
            depth: SearchDepth::WarmOnly,
            ..Default::default()
        };
        let now_ms = 0; // Only consulted to decide whether a since_ts reaches into Cold; it cannot here.
        let result = search::search_hybrid_with_options(&self.conn, query, None, now_ms, &opts, k)?;
        Ok(result.hits.into_iter().map(|h| h.event_id).collect())
    }

    fn count(&self) -> Result<i64, BackendError> {
        Ok(self
            .conn
            .query_row("SELECT count(*) FROM event_log", [], |r| r.get(0))?)
    }

    fn size(&self) -> Result<StorageSize, BackendError> {
        let page_count: i64 = self.conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size: i64 = self.conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        let logical_bytes = (page_count.max(0) as u64).saturating_mul(page_size.max(0) as u64);

        let file_bytes = match &self.path {
            Some(p) => {
                let mut total = 0u64;
                for suffix in ["", "-wal", "-shm"] {
                    let candidate = if suffix.is_empty() {
                        p.clone()
                    } else {
                        let mut s = p.clone().into_os_string();
                        s.push(suffix);
                        PathBuf::from(s)
                    };
                    // A missing -wal/-shm is normal, not an error.
                    if let Ok(meta) = std::fs::metadata(&candidate) {
                        total = total.saturating_add(meta.len());
                    }
                }
                Some(total)
            }
            None => None,
        };
        Ok(StorageSize {
            logical_bytes,
            file_bytes,
        })
    }

    fn begin_batch(&mut self) -> Result<(), BackendError> {
        self.conn.execute_batch("BEGIN")?;
        Ok(())
    }

    fn commit_batch(&mut self) -> Result<(), BackendError> {
        self.conn.execute_batch("COMMIT")?;
        Ok(())
    }
}
