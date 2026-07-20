//! The daemon's DB handle (daemon wiring, feature `db`). One shared SQLite connection that every
//! writer and reader uses — the data-gravity point of the whole system (CLAUDE.md invariant 1: the
//! DB is owned by the Rust core). Cheap to clone (it's an `Arc`), so the capture thread, the LLM
//! egress sink, and the traceability viewer all hold the same handle.
//!
//! Every method here swallows storage errors into an `Option`/empty result rather than
//! propagating a panic: the capture daemon must never crash on a write hiccup (CLAUDE.md
//! crash-resilience). Durable-write concerns (WAL, transactions) live in [`shogun_memory`].
//!
//! Clocks are injected ([`Clock`]) so timestamps are deterministic under test; production passes a
//! real wall-clock closure.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use shogun_memory::event_log::{self, NewEvent};
use shogun_memory::state::{self, NewCommitment, NewOpenLoop, NewPerson, NewProject, Provenance};
use shogun_memory::traceability::{Filter, TraceRow};
use shogun_memory::MemoryError;
use shogun_fusion::brief::{assemble_brief, assemble_degraded, CalendarLine, CommitmentDue, MorningBrief, OpenLoopItem};
use shogun_fusion::assemble::ActionCandidate;

use crate::capture::dedup::{decide_hash, Recent};
use crate::db_sink::DbTraceabilitySink;
use crate::dreamcycle::plan::{remaining, CycleKind, JobKind, JobRun, JobState};

/// How many recent capture bodies the near-dup collapse (FR-CAP-03) compares against. Bounds the
/// per-capture comparison cost; window re-reads are near each other in the log, so a small window
/// catches them.
const RECENT_DEDUP_WINDOW: usize = 8;

/// The shared connection handle. `Connection` is `Send` but not `Sync`, so it lives behind a
/// `Mutex`; the `Arc` lets every daemon component share the one handle.
pub type SharedConn = Arc<Mutex<Connection>>;

/// An injected millisecond clock (unix ms). Shared so every writer stamps from the same source.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The daemon's DB handle. Clone freely — clones share the same underlying connection.
#[derive(Clone)]
pub struct Db {
    conn: SharedConn,
    clock: Clock,
}

impl Db {
    /// Wrap an already-open, migrated connection.
    pub fn new(conn: Connection, clock: Clock) -> Self {
        Self { conn: Arc::new(Mutex::new(conn)), clock }
    }

    /// Open the on-disk database (runs migrations) and wrap it.
    pub fn open(path: impl AsRef<std::path::Path>, clock: Clock) -> Result<Self, MemoryError> {
        Ok(Self::new(shogun_memory::open(path)?, clock))
    }

    /// Open a fresh in-memory database (migrations applied) — for tests and ephemeral use.
    pub fn open_in_memory(clock: Clock) -> Result<Self, MemoryError> {
        Ok(Self::new(shogun_memory::open_in_memory()?, clock))
    }

    /// Record a captured event (capture → memory, FR-CAP-03 dedup-touch). Swallows storage errors
    /// so the capture daemon never crashes on a write hiccup; returns `(id, touched)` on success.
    pub fn capture(&self, ev: &NewEvent<'_>) -> Option<(i64, bool)> {
        self.conn.lock().ok().and_then(|c| event_log::insert_or_touch(&c, ev).ok())
    }

    /// Capture an event, then — only if it was newly inserted (not a dedup-touch) — run the
    /// first-stage local-rule extraction (WP2.7) over its content and persist any low-confidence
    /// commitment / open-loop candidates, each linked to this event (FR-ST-02). Extraction is
    /// skipped on a dedup-touch so repeated identical captures don't multiply candidates.
    ///
    /// Returns `(event_id, touched, candidate_ids)`. Extraction failures are swallowed (the
    /// candidate list comes back empty) so a heuristic hiccup never blocks capture.
    pub fn capture_and_extract(&self, ev: &NewEvent<'_>) -> Option<(i64, bool, Vec<i64>)> {
        let (id, touched) = self.capture(ev)?;
        if touched {
            return Some((id, touched, Vec::new()));
        }
        let candidates = shogun_memory::extract::extract(ev.content);
        let now = self.now_ms();
        let ids = {
            let mut g = self.conn.lock().ok()?;
            shogun_memory::extract::persist_candidates(&mut g, id, &candidates, now).unwrap_or_default()
        };
        Some((id, touched, ids))
    }

    /// The canonical content hash (xxhash64, hex) used across capture and notes.
    fn content_hash(text: &str) -> String {
        use std::hash::Hasher;
        let mut h = twox_hash::XxHash64::with_seed(0);
        h.write(text.as_bytes());
        format!("{:016x}", h.finish())
    }

    /// Capture a window body with near-duplicate collapse (FR-CAP-03): if `ev.content` is ≥98%
    /// similar to a recent capture body, reuse that body's hash so the event log dedup-touches
    /// instead of appending a near-identical row; otherwise a fresh hash makes a new event. The
    /// `content_hash` on the passed `ev` is ignored — this method decides it. Returns `(id, touched)`.
    pub fn capture_collapsed(&self, ev: &NewEvent<'_>) -> Option<(i64, bool)> {
        let recents = self.recent_capture_bodies(RECENT_DEDUP_WINDOW);
        let recent_refs: Vec<Recent<'_>> =
            recents.iter().map(|(h, c)| Recent { content_hash: h, content: c }).collect();
        let decision = decide_hash(ev.content, &recent_refs, Self::content_hash);
        let collapsed = NewEvent { content_hash: decision.hash(), ..ev.clone() };
        self.capture(&collapsed)
    }

    /// Recent capture bodies `(hash, content)` newest-first, for the near-dup collapse.
    fn recent_capture_bodies(&self, limit: usize) -> Vec<(String, String)> {
        self.conn.lock().ok().and_then(|c| event_log::recent_capture_bodies(&c, limit).ok()).unwrap_or_default()
    }

    /// Append a user note to the event log (`memory.append_note`, L1). Source `user`, kind `note`;
    /// content-hashed for dedup like any event. Returns the row id (or `None` on write failure).
    pub fn append_note(&self, text: &str) -> Option<i64> {
        let hash = Self::content_hash(text);
        let ev = NewEvent {
            ts: self.now_ms(),
            source: "user",
            kind: "note",
            app_bundle_id: None,
            window_title: None,
            content: text,
            content_hash: &hash,
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        };
        self.capture(&ev).map(|(id, _)| id)
    }

    /// A traceability sink that writes through this same handle (the LLM egress records here).
    pub fn traceability_sink(&self) -> DbTraceabilitySink {
        DbTraceabilitySink::new(self.conn.clone(), self.clock.clone())
    }

    /// Read traceability rows for the viewer (FR-TR-02). Empty on any read failure.
    pub fn trace_rows(&self, filter: &Filter) -> Vec<TraceRow> {
        self.conn
            .lock()
            .ok()
            .and_then(|c| shogun_memory::traceability::list(&c, filter).ok())
            .unwrap_or_default()
    }

    /// The current time via the injected clock.
    pub fn now_ms(&self) -> i64 {
        (self.clock)()
    }

    /// Export all user data as JSON (FR-SET-07). Local only — never a network send. `None` on a
    /// read failure.
    pub fn export_json(&self) -> Option<String> {
        self.conn.lock().ok().and_then(|c| shogun_memory::maintenance::export_json(&c).ok())
    }

    /// Delete all user data, keeping the schema (FR-SET-07). Returns the per-table deletion report,
    /// or `None` on failure (the transaction leaves the DB untouched).
    pub fn delete_all(&self) -> Option<shogun_memory::maintenance::DeleteReport> {
        let mut g = self.conn.lock().ok()?;
        shogun_memory::maintenance::delete_all(&mut g).ok()
    }

    // -------------------------------------------------------------- state writes (deliberate)
    // Unlike capture, state writes are low-frequency and deliberate (Dream Cycle consolidation,
    // API propose). They return the new id or `None` on failure so the caller (e.g. a Dream Cycle
    // job) can mark itself failed rather than silently continuing.

    /// Insert a person with provenance (FR-ST-02).
    pub fn insert_person(&self, p: &NewPerson<'_>, provenance: &[Provenance]) -> Option<i64> {
        let mut g = self.conn.lock().ok()?;
        state::insert_person(&mut g, p, provenance).ok()
    }

    /// Insert a project with provenance.
    pub fn insert_project(&self, p: &NewProject<'_>, provenance: &[Provenance]) -> Option<i64> {
        let mut g = self.conn.lock().ok()?;
        state::insert_project(&mut g, p, provenance).ok()
    }

    /// Insert a commitment with provenance.
    pub fn insert_commitment(&self, c: &NewCommitment<'_>, provenance: &[Provenance]) -> Option<i64> {
        let mut g = self.conn.lock().ok()?;
        state::insert_commitment(&mut g, c, provenance).ok()
    }

    /// Insert an open loop with provenance.
    pub fn insert_open_loop(&self, l: &NewOpenLoop<'_>, provenance: &[Provenance]) -> Option<i64> {
        let mut g = self.conn.lock().ok()?;
        state::insert_open_loop(&mut g, l, provenance).ok()
    }

    // -------------------------------------------------------------- Dream Cycle job effects
    // Concrete effects the nightly cycle drives through the `DreamJobRunner` seam (dreamcycle::jobs).
    // Each swallows storage errors into a safe default so a hiccup fails the *job* (leaving the cycle
    // resumable) rather than crashing the daemon.

    /// Events in `[from_ts, to_ts)` — the window a Consolidation job classifies (FR-DC-03).
    pub fn events_in_range(&self, from_ts: i64, to_ts: i64) -> Vec<event_log::EventText> {
        self.conn.lock().ok().and_then(|c| event_log::events_in_range(&c, from_ts, to_ts).ok()).unwrap_or_default()
    }

    /// Descriptions already present in `commitments` + `open_loops`, for consolidation dedup — so a
    /// re-run over the same range (crash-resume, FR-DC-04) doesn't add the same candidate twice.
    pub fn existing_state_descriptions(&self) -> std::collections::HashSet<String> {
        let mut set = std::collections::HashSet::new();
        if let Ok(c) = self.conn.lock() {
            if let Ok(rows) = state::list_commitments(&c) {
                set.extend(rows.into_iter().map(|r| r.description));
            }
            if let Ok(rows) = state::list_open_loops(&c) {
                set.extend(rows.into_iter().map(|r| r.description));
            }
        }
        set
    }

    /// Persist extracted candidates linked to `event_id` (FR-ST-02). Returns the new row ids.
    pub fn persist_candidates(&self, event_id: i64, candidates: &[shogun_memory::extract::Candidate]) -> Vec<i64> {
        let now = self.now_ms();
        self.conn
            .lock()
            .ok()
            .and_then(|mut g| shogun_memory::extract::persist_candidates(&mut g, event_id, candidates, now).ok())
            .unwrap_or_default()
    }

    /// Recompute overdue status + open-loop staleness from `now` (FR-ST-21). Returns
    /// `(commitments_flagged, loops_touched)`; `(0,0)` on a lock/write failure.
    pub fn recompute_overdue_and_staleness(&self, now_ms: i64) -> (usize, usize) {
        self.conn
            .lock()
            .ok()
            .and_then(|mut g| shogun_memory::recompute::recompute_overdue_and_staleness(&mut g, now_ms).ok())
            .unwrap_or((0, 0))
    }

    /// Age-decay state-row confidence (FR-ST-21). Returns the number of rows changed.
    pub fn decay_confidence(&self, now_ms: i64, half_life_ms: i64) -> usize {
        self.conn
            .lock()
            .ok()
            .and_then(|mut g| shogun_memory::recompute::decay_confidence(&mut g, now_ms, half_life_ms).ok())
            .unwrap_or(0)
    }

    /// Demote Warm embeddings older than `cutoff_ms` to the int8 Cold tier (FR-MEM-04). Returns the
    /// number moved.
    pub fn demote_cold(&self, cutoff_ms: i64) -> usize {
        self.conn
            .lock()
            .ok()
            .and_then(|mut g| shogun_memory::cold::demote_older_than(&mut g, cutoff_ms).ok())
            .unwrap_or(0)
    }

    // -------------------------------------------------------------- state reads → Fusion supply
    // The daemon reads state rows and maps them into Fusion's input types, so Context Fusion and
    // the Morning Brief run on real DB data. The confidence gate lives in Fusion (FR-ST-20); the
    // daemon only supplies the rows.

    /// Commitments as Fusion/Brief input. `overdue` is derived from the status or a past due time.
    pub fn commitments_due(&self, now_ms: i64) -> Vec<CommitmentDue> {
        let rows = self.conn.lock().ok().and_then(|c| state::list_commitments(&c).ok()).unwrap_or_default();
        rows.into_iter()
            .map(|r| CommitmentDue {
                overdue: r.status == "overdue" || r.due_at.is_some_and(|d| d < now_ms),
                description: r.description,
                due_at_ms: r.due_at,
                confidence: r.confidence,
                provenance_event_id: r.first_event_id.unwrap_or(0),
            })
            .collect()
    }

    /// People rows (Memory API `state.people.list`).
    pub fn people(&self) -> Vec<state::PersonRow> {
        self.conn.lock().ok().and_then(|c| state::list_people(&c).ok()).unwrap_or_default()
    }

    /// One person by id (`state.people.get`).
    pub fn person(&self, id: i64) -> Option<state::PersonRow> {
        self.conn.lock().ok().and_then(|c| state::get_person(&c, id).ok()).flatten()
    }

    /// Project rows (`state.projects.list`).
    pub fn projects(&self) -> Vec<state::ProjectRow> {
        self.conn.lock().ok().and_then(|c| state::list_projects(&c).ok()).unwrap_or_default()
    }

    /// One project by id (`state.projects.get`).
    pub fn project(&self, id: i64) -> Option<state::ProjectRow> {
        self.conn.lock().ok().and_then(|c| state::get_project(&c, id).ok()).flatten()
    }

    /// One commitment by id (`state.commitments.get`).
    pub fn commitment(&self, id: i64) -> Option<state::CommitmentRow> {
        self.conn.lock().ok().and_then(|c| state::get_commitment(&c, id).ok()).flatten()
    }

    /// One open loop by id (`state.open_loops.get`).
    pub fn open_loop(&self, id: i64) -> Option<state::OpenLoopRow> {
        self.conn.lock().ok().and_then(|c| state::get_open_loop(&c, id).ok()).flatten()
    }

    /// Hybrid/FTS search over the event log (`memory.search`). Empty on an empty query or failure.
    pub fn search(&self, query: &str, limit: usize) -> Vec<shogun_memory::search::SearchHit> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        self.conn.lock().ok().and_then(|c| shogun_memory::search::search(&c, query, limit).ok()).unwrap_or_default()
    }

    /// Open loops as Fusion/Brief input (stalest first; the Brief caps the count).
    pub fn open_loops(&self) -> Vec<OpenLoopItem> {
        let rows = self.conn.lock().ok().and_then(|c| state::list_open_loops(&c).ok()).unwrap_or_default();
        rows.into_iter()
            .map(|r| OpenLoopItem {
                description: r.description,
                staleness_days: u32::try_from(r.staleness_days).unwrap_or(0),
                confidence: r.confidence,
                provenance_event_id: r.first_event_id.unwrap_or(0),
            })
            .collect()
    }

    /// Assemble a full Morning Brief from DB state plus the supplied calendar / prose / suggestions
    /// (§6.8). The section rules + confidence gate are Fusion's; the daemon supplies the state.
    pub fn morning_brief(
        &self,
        calendar: Vec<CalendarLine>,
        what_happened: Vec<String>,
        suggested: Vec<ActionCandidate>,
        now_ms: i64,
    ) -> MorningBrief {
        assemble_brief(calendar, &self.commitments_due(now_ms), &self.open_loops(), what_happened, suggested)
    }

    /// The local-only degraded Brief (FR-MB-04): calendar + overdue commitments from the DB, no
    /// prose — used when the Batch-API generation is unavailable.
    pub fn local_morning_brief(&self, calendar: Vec<CalendarLine>, now_ms: i64) -> MorningBrief {
        assemble_degraded(calendar, &self.commitments_due(now_ms))
    }

    // -------------------------------------------------------------- Dream Cycle job ledger (FR-DC-04)
    // Persist each job's state so a killed cycle resumes by skipping the `done` jobs. The plan
    // vocabulary (JobKind/JobState) is shogun-core's; storage keeps strings, mapped here.

    /// Record a job's state for a cycle (upsert on `(cycle_id, kind)`). Returns false on a write
    /// failure so the caller can react. `input_from_ts..input_to_ts` is the range the job consumed.
    pub fn record_job(
        &self,
        cycle_id: &str,
        kind: JobKind,
        state: JobState,
        input_from_ts: i64,
        input_to_ts: i64,
    ) -> bool {
        let now = self.now_ms();
        self.conn
            .lock()
            .ok()
            .map(|c| {
                shogun_memory::jobs::upsert(
                    &c,
                    cycle_id,
                    job_kind_str(kind),
                    job_state_str(state),
                    input_from_ts,
                    input_to_ts,
                    now,
                )
                .is_ok()
            })
            .unwrap_or(false)
    }

    /// The persisted job runs for a cycle, as [`JobRun`]s (unrecognised rows are skipped).
    pub fn cycle_runs(&self, cycle_id: &str) -> Vec<JobRun> {
        let rows = self
            .conn
            .lock()
            .ok()
            .and_then(|c| shogun_memory::jobs::list_by_cycle(&c, cycle_id).ok())
            .unwrap_or_default();
        rows.into_iter()
            .filter_map(|r| {
                Some(JobRun {
                    kind: parse_job_kind(&r.kind)?,
                    state: parse_job_state(&r.state)?,
                    input_from_ts: r.input_from_ts,
                    input_to_ts: r.input_to_ts,
                })
            })
            .collect()
    }

    /// The jobs still to run for `cycle`, given what's persisted (FR-DC-04 resume). A killed cycle
    /// resumes here by skipping the jobs already `done`.
    pub fn resume(&self, cycle_id: &str, cycle: CycleKind) -> Vec<JobKind> {
        remaining(cycle, &self.cycle_runs(cycle_id))
    }
}

/// Map a [`JobKind`] to its stored string.
fn job_kind_str(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Consolidation => "consolidation",
        JobKind::Compression => "compression",
        JobKind::StateUpdate => "state_update",
        JobKind::ConfidenceRecalc => "confidence_recalc",
        JobKind::ColdDemotion => "cold_demotion",
        JobKind::MorningBrief => "morning_brief",
    }
}

fn parse_job_kind(s: &str) -> Option<JobKind> {
    Some(match s {
        "consolidation" => JobKind::Consolidation,
        "compression" => JobKind::Compression,
        "state_update" => JobKind::StateUpdate,
        "confidence_recalc" => JobKind::ConfidenceRecalc,
        "cold_demotion" => JobKind::ColdDemotion,
        "morning_brief" => JobKind::MorningBrief,
        _ => return None,
    })
}

fn job_state_str(state: JobState) -> &'static str {
    match state {
        JobState::Pending => "pending",
        JobState::Running => "running",
        JobState::Done => "done",
        JobState::Failed => "failed",
    }
}

fn parse_job_state(s: &str) -> Option<JobState> {
    Some(match s {
        "pending" => JobState::Pending,
        "running" => JobState::Running,
        "done" => JobState::Done,
        "failed" => JobState::Failed,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::traceability::{Route, TraceRecord};
    use crate::llm::traceability::TraceabilitySink;

    fn clock(v: i64) -> Clock {
        Arc::new(move || v)
    }

    fn ev<'a>(content: &'a str, hash: &'a str, ts: i64) -> NewEvent<'a> {
        NewEvent {
            ts,
            source: "capture",
            kind: "text",
            app_bundle_id: Some("com.apple.Safari"),
            window_title: Some("t"),
            content,
            content_hash: hash,
            dwell_ms: 0,
            display_id: Some(1),
            window_bounds: None,
        }
    }

    #[test]
    fn capture_writes_then_dedup_touches_same_row() {
        let db = Db::open_in_memory(clock(1)).unwrap();
        let (id1, touched1) = db.capture(&ev("hello", "h", 100)).unwrap();
        assert!(!touched1);
        let (id2, touched2) = db.capture(&ev("hello", "h", 200)).unwrap();
        assert!(touched2, "same content_hash must touch, not append");
        assert_eq!(id1, id2);
    }

    #[test]
    fn capture_collapsed_touches_near_duplicate_bodies() {
        let db = Db::open_in_memory(clock(1)).unwrap();
        let base: String = "sprint board: ticket triage, blockers, owner assignments ".repeat(10);
        // first capture creates a row (fresh hash, incoming hash ignored)
        let (id1, t1) = db.capture_collapsed(&ev(&base, "ignored-incoming-hash", 100)).unwrap();
        assert!(!t1);
        // one keystroke later: ≥98% similar → collapses onto the same row (dedup-touch)
        let typed = format!("{base}!");
        let (id2, t2) = db.capture_collapsed(&ev(&typed, "another-ignored-hash", 200)).unwrap();
        assert!(t2, "a near-duplicate body must dedup-touch, not append");
        assert_eq!(id1, id2);
        // an unrelated body makes a new event
        let (id3, t3) = db.capture_collapsed(&ev("completely unrelated capture", "x", 300)).unwrap();
        assert!(!t3);
        assert_ne!(id3, id1);

        let n: i64 = {
            let c = db.conn.lock().unwrap();
            c.query_row("SELECT count(*) FROM event_log", [], |r| r.get(0)).unwrap()
        };
        assert_eq!(n, 2, "two distinct events: the collapsed body + the unrelated one");
    }

    #[test]
    fn capture_and_extract_persists_low_confidence_candidates_once() {
        let db = Db::open_in_memory(clock(500)).unwrap();
        let (_id, touched, ids) =
            db.capture_and_extract(&ev("I'll send the deck. Waiting on legal.", "h", 100)).unwrap();
        assert!(!touched);
        assert_eq!(ids.len(), 2, "one commitment + one open loop");
        // every extracted candidate is low-confidence (FR-ST-20)
        for c in db.commitments_due(500) {
            assert!(c.confidence <= shogun_memory::extract::LOCAL_RULE_MAX_CONFIDENCE);
        }

        // a dedup-touch of the same content must NOT extract again
        let (_id2, touched2, ids2) =
            db.capture_and_extract(&ev("I'll send the deck. Waiting on legal.", "h", 200)).unwrap();
        assert!(touched2);
        assert!(ids2.is_empty(), "dedup-touch must not re-extract");
        assert_eq!(db.commitments_due(500).len(), 1, "still exactly one commitment");
    }

    #[test]
    fn clones_share_one_connection() {
        // A capture through the handle is visible to a traceability read on a *clone* — proving it
        // is the same underlying connection, not a copy.
        let db = Db::open_in_memory(clock(7)).unwrap();
        let sink = db.clone().traceability_sink();
        sink.record(TraceRecord::for_chunk(Route::BatchApi, "indexing", "api.anthropic.com", "x", false));
        // read via the original handle
        let rows = db.trace_rows(&Filter::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].ts, 7, "the injected clock stamped the row");
    }

    #[test]
    fn export_and_delete_all_through_the_handle() {
        let db = Db::open_in_memory(clock(1)).unwrap();
        db.capture(&ev("a note", "h1", 10)).unwrap();
        // export sees the event
        let json = db.export_json().unwrap();
        assert!(json.contains("a note"));
        // delete wipes it, schema survives (a re-capture still works)
        let report = db.delete_all().unwrap();
        assert_eq!(report.events, 1);
        let after: serde_json::Value = serde_json::from_str(&db.export_json().unwrap()).unwrap();
        assert!(after["event_log"].as_array().unwrap().is_empty());
        assert!(db.capture(&ev("again", "h2", 20)).is_some());
    }

    #[test]
    fn capture_and_traceability_hit_the_same_handle() {
        let db = Db::open_in_memory(clock(42)).unwrap();
        db.capture(&ev("note", "h1", 10)).unwrap();
        db.traceability_sink()
            .record(TraceRecord::for_chunk(Route::MessagesApi, "agent", "api.anthropic.com", "chunk", false));
        // both writes landed on the one connection
        assert_eq!(db.trace_rows(&Filter::default()).len(), 1);
    }

    #[test]
    fn state_write_then_fusion_supply_roundtrip() {
        use shogun_memory::state::{CommitmentDirection, CommitmentStatus, NewCommitment};

        let db = Db::open_in_memory(clock(1)).unwrap();
        // a captured event is the provenance for the commitment
        let (event_id, _) = db.capture(&ev("Alice asked for the report", "h1", 5)).unwrap();

        let id = db
            .insert_commitment(
                &NewCommitment {
                    direction: CommitmentDirection::Mine,
                    counterparty_id: None,
                    description: "send the report",
                    due_at: Some(50),
                    status: CommitmentStatus::Open,
                    project_id: None,
                    confidence: 0.9,
                    now: 5,
                },
                &[Provenance::new(event_id)],
            )
            .expect("commitment insert");
        assert!(id > 0);

        // now supply it to Fusion — at now=100 the due=50 commitment reads as overdue
        let due = db.commitments_due(100);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].description, "send the report");
        assert!(due[0].overdue, "due 50 < now 100 → overdue");
        assert_eq!(due[0].provenance_event_id, event_id);
    }

    #[test]
    fn local_brief_from_db_carries_overdue_commitments_only() {
        use shogun_memory::state::{CommitmentDirection, CommitmentStatus, NewCommitment};

        let db = Db::open_in_memory(clock(1)).unwrap();
        let (e, _) = db.capture(&ev("x", "h1", 1)).unwrap();
        let mk = |desc: &'static str, due: i64, status| NewCommitment {
            direction: CommitmentDirection::Mine,
            counterparty_id: None,
            description: desc,
            due_at: Some(due),
            status,
            project_id: None,
            confidence: 0.9,
            now: 1,
        };
        db.insert_commitment(&mk("overdue one", 10, CommitmentStatus::Overdue), &[Provenance::new(e)]).unwrap();
        db.insert_commitment(&mk("future one", 9999, CommitmentStatus::Open), &[Provenance::new(e)]).unwrap();

        // degraded/local brief keeps only overdue commitments (FR-MB-04)
        let brief = db.local_morning_brief(Vec::new(), 100);
        assert!(brief.degraded);
        assert_eq!(brief.commitments_due.len(), 1);
        assert_eq!(brief.commitments_due[0].text, "overdue one");
    }

    #[test]
    fn dream_cycle_resume_skips_done_jobs_from_the_db() {
        let db = Db::open_in_memory(clock(1)).unwrap();
        let cycle = "20260720";
        // first two jobs completed, the third was interrupted mid-run
        assert!(db.record_job(cycle, JobKind::Consolidation, JobState::Done, 0, 100));
        assert!(db.record_job(cycle, JobKind::Compression, JobState::Done, 0, 100));
        assert!(db.record_job(cycle, JobKind::StateUpdate, JobState::Running, 0, 100));

        // resume the full cycle: done jobs are skipped, the running one is rescheduled
        let todo = db.resume(cycle, CycleKind::Full);
        assert_eq!(todo.first(), Some(&JobKind::StateUpdate));
        assert_eq!(todo.len(), 4); // StateUpdate, ConfidenceRecalc, ColdDemotion, MorningBrief
        assert!(!todo.contains(&JobKind::Consolidation));
    }

    #[test]
    fn record_job_upsert_advances_state_for_resume() {
        let db = Db::open_in_memory(clock(1)).unwrap();
        let cycle = "night";
        db.record_job(cycle, JobKind::Consolidation, JobState::Running, 0, 100);
        // still to-do while running
        assert!(db.resume(cycle, CycleKind::Full).contains(&JobKind::Consolidation));
        // mark done → dropped from the resume set
        db.record_job(cycle, JobKind::Consolidation, JobState::Done, 0, 100);
        assert!(!db.resume(cycle, CycleKind::Full).contains(&JobKind::Consolidation));
        // one row, not two (idempotent upsert)
        assert_eq!(db.cycle_runs(cycle).len(), 1);
    }
}
