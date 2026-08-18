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

use rusqlite::{Connection, OptionalExtension};
use shogun_memory::event_log::{self, NewEvent};
use shogun_memory::lessons;
use shogun_memory::state::{self, NewCommitment, NewOpenLoop, NewPerson, NewProject, Provenance};
use shogun_memory::traceability::{Filter, TraceRow};
use shogun_memory::MemoryError;
use shogun_fusion::brief::{assemble_brief, assemble_degraded, CalendarLine, CommitmentDue, MorningBrief, OpenLoopItem};
use shogun_fusion::assemble::ActionCandidate;
use shogun_fusion::block::{BlockRef, ContextBlock, ScoreInputs, SourceKind};
use shogun_fusion::budget::TokenEstimator;

use crate::capture::dedup::{decide_hash, Recent};
use crate::memory_health::{FaultClass, MemoryFault, MemoryResult};
use crate::db_sink::DbTraceabilitySink;
use crate::dreamcycle::plan::{remaining, CycleKind, JobKind, JobRun, JobState, DEGRADED_SEQUENCE};

/// How many recent capture bodies the near-dup collapse (FR-CAP-03) compares against. Bounds the
/// per-capture comparison cost; window re-reads are near each other in the log, so a small window
/// catches them.
const RECENT_DEDUP_WINDOW: usize = 8;

/// `inline_memory` に渡す fact 上限。`assemble_context` と `assemble_context_compressed` の
/// 両パスで一致させ、fact の一貫性を保つ。
const FACT_LIMIT: usize = 8;

/// The outcome of ingesting a batch of synced integration items ([`Db::ingest_integration`]):
/// how many were processed, how many were genuinely new (the `IntegrationSynced` bus count), and
/// how many low-confidence state candidates the new items yielded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IngestSummary {
    pub processed: usize,
    pub newly_inserted: usize,
    pub candidates: usize,
}

/// The first-layer connector runtime ([`shogun_integrations::ConnectorRuntime`]) hands each synced
/// batch to this sink; the daemon persists it into the event log via [`Db::ingest_integration`].
/// `newly_inserted` is what an `IntegrationSynced` bus event reports (§6.9). This keeps data gravity
/// in the core (invariant 1) — the connector crate never touches the DB.
impl shogun_integrations::IngestSink for Db {
    fn ingest(&self, items: &[shogun_mcp::sync::IngestItem]) -> usize {
        self.ingest_integration(items).newly_inserted
    }
}

/// One retrieved piece of evidence behind an answer ([`Db::assemble_context`]). Carries its
/// `event_id` so a generated answer can cite what it was grounded in (provenance is the whole
/// point of the state/event split) and its `source` so mail is distinguishable from a captured
/// window (FR-MEM-23).
#[derive(Debug, Clone, PartialEq)]
pub struct Evidence {
    pub event_id: i64,
    pub ts: i64,
    pub source: String,
    pub title: Option<String>,
    pub excerpt: String,
    /// Linked `screen_frames` row when a JPEG is stored for this evidence (≤72 h).
    pub frame_id: Option<i64>,
}

/// A stored screen capture available for visual recall (metadata only — bytes via [`Db::get_screen_frame`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenFrameRef {
    pub frame_id: i64,
    pub event_id: i64,
    pub ts: i64,
    pub app_bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub width: u32,
    pub height: u32,
    pub ocr_excerpt: String,
    /// Thin stored OCR — caller should re-scan the JPEG (Vision) before answering.
    pub needs_rescan: bool,
    /// Linked event source.
    pub source: String,
}

/// The grounded context for one question: confidence-gated state facts plus the retrieved
/// evidence that mentions it. Facts say what SHOGUN believes; evidence says what was actually
/// seen, and only evidence can answer "what happened with X".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ContextPack {
    pub facts: Vec<String>,
    pub evidence: Vec<Evidence>,
    /// Stored JPEG frames matching a visual-recall question (hook for future vision input).
    pub screen_frames: Vec<ScreenFrameRef>,
}

/// How much of a thread a reply context carries. Enough to answer in the conversation's own
/// terms, bounded so assembly stays inside the pre-press budget.
const REPLY_TURNS: usize = 12;
const REPLY_TURN_CHARS: usize = 800;
const REPLY_RELATED: usize = 4;
const REPLY_RELATED_CHARS: usize = 300;

/// クエリ時のローカル圧縮に許す時間予算。超えたら raw にフォールバック（SLO +300ms 厳守）。
const COMPRESS_BUDGET_MS: u64 = 50;

// 圧縮ブロックの relevance/freshness/task_link/confidence 係数（設計 §3.3/§3.4）。
// 従来は各所にインラインのリテラルで散らばっていた（Issue #63 finding #7）。ここに集約して
// 由来ごとの相対関係（thread ≥ session ≥ evidence、fact は現在の作業に紐づく前提でやや高め）を
// 一望できるようにし、thread/session の対称性を型で担保する。数値は従来と同一。
//
/// 検索 evidence: relevance は呼び出し側の検索スコア由来なので EVIDENCE_RELEVANCE を上書きする。
const EVIDENCE_RELEVANCE: f64 = 0.7;

/// How many learned lessons ride into one context assembly / generation prompt (Plan D-5's
/// default top-k of 5). The token budget in `shogun_fusion::assemble::LESSON_BUDGET_TOKENS`
/// bounds them again by size.
const LESSON_TOP_K: usize = 5;
const EVIDENCE_SCORE: ScoreInputs =
    ScoreInputs { relevance: EVIDENCE_RELEVANCE, freshness: 0.5, task_link: 0.0, confidence: 1.0 };
/// retrieved evidence の属する session の保存済み要約。参照先＝関連度高、要約＝confidence 1.0。
const SESSION_SUMMARY_SCORE: ScoreInputs =
    ScoreInputs { relevance: 0.85, freshness: 0.7, task_link: 0.5, confidence: 1.0 };
/// 解決済みスレッドの保存済み要約。session と対称、参照先＝関連度高、要約＝confidence 1.0。
const THREAD_SUMMARY_SCORE: ScoreInputs =
    ScoreInputs { relevance: 0.9, freshness: 0.7, task_link: 0.5, confidence: 1.0 };
/// confidence ゲート済み state fact。現在の作業に紐づく前提でやや高め、confidence は High 相当 0.9。
const FACT_SCORE: ScoreInputs =
    ScoreInputs { relevance: 0.6, freshness: 0.6, task_link: 0.6, confidence: 0.9 };

/// ドラフトの本文がどこ由来か。融合の provenance（設計 §3）。
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PayloadSource {
    /// 取得した実メール由来（高信頼）。thread_key が provenance（同期スレッドの識別子）。
    Fetched { thread_key: String },
    /// 取得データに解決できず、画面キャプチャの断片のみ。
    #[default]
    OnScreenOnly,
}

/// Everything a one-press reply needs, assembled before the press ([`Db::build_reply_context`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReplyContext {
    pub thread_key: String,
    pub title: Option<String>,
    /// The thread's own recent events, oldest first.
    pub turns: Vec<Evidence>,
    /// Confidence-gated state facts (what is owed, what is waiting).
    pub facts: Vec<String>,
    /// Earlier threads that resemble this one.
    pub related: Vec<Evidence>,
    /// How long assembly took, in ms — the SLO measurement, carried with the data it describes.
    pub build_ms: u64,
    /// 本文の出所（融合の provenance）。
    pub payload_source: PayloadSource,
}

impl ReplyContext {
    /// Flatten into the memory lines the inline composer takes.
    ///
    /// Order is deliberate: the thread's own words first (a reply is written *into* a
    /// conversation, so that is the primary material), then what is owed or waiting, then
    /// anything similar from before. `max_lines` bounds the prompt.
    pub fn as_memory_lines(&self, max_lines: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if let Some(t) = self.title.as_deref().filter(|t| !t.is_empty()) {
            out.push(format!("in view: {t}"));
        }
        for turn in &self.turns {
            out.push(turn.excerpt.clone());
        }
        out.extend(self.facts.iter().cloned());
        for r in &self.related {
            out.push(format!("earlier: {}", r.excerpt));
        }
        out.truncate(max_lines);
        out
    }

    /// True when there is nothing here worth preferring over the plain state facts.
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty() && self.facts.is_empty() && self.related.is_empty()
    }
}

/// The pre-assembled reply context, kept warm so a press only starts generation.
///
/// The whole point is that reading it costs nothing: the focus path writes, the button reads.
/// A miss (focus moved to a thread that has not been built yet) returns `None` rather than
/// building inline — building on the press is exactly what the SLO forbids, and a caller that
/// silently fell back to it would hide the regression.
#[derive(Clone, Default)]
pub struct ReplyContextCache {
    inner: Arc<Mutex<Option<ReplyContext>>>,
}

impl ReplyContextCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the warm pack (called off the focus path).
    pub fn put(&self, ctx: ReplyContext) {
        if let Ok(mut g) = self.inner.lock() {
            *g = Some(ctx);
        }
    }

    /// Drop warm context when the capture source can no longer vouch for the focused surface.
    pub fn clear(&self) {
        if let Ok(mut g) = self.inner.lock() {
            *g = None;
        }
    }

    /// The warm pack for `thread_key`, if that is the one currently held.
    pub fn get(&self, thread_key: &str) -> Option<ReplyContext> {
        let g = self.inner.lock().ok()?;
        g.as_ref().filter(|c| c.thread_key == thread_key).cloned()
    }

    /// The warm pack, whatever thread it is for.
    pub fn current(&self) -> Option<ReplyContext> {
        self.inner.lock().ok().and_then(|g| g.clone())
    }

    /// Drop the warm pack: something changed underneath it (an integration sync landed new items),
    /// so whatever was assembled before is stale. The next `get` is an honest miss until the focus
    /// path re-assembles via [`Self::put`] — invalidation never rebuilds inline, because building
    /// on the press path is exactly what this cache exists to prevent.
    pub fn invalidate(&self) {
        self.clear();
    }
}

/// The daemon-owned bus subscription that keeps the reply-context cache honest across integration
/// syncs (design §2.2 / E-49): an [`crate::bus::BusEvent::IntegrationSynced`] means new items just
/// landed in the event log, so a pack assembled before the sync may answer with a stale thread.
///
/// No thread of its own: like the daemon's other background work (embedding, local maintenance,
/// the dream driver's `tick`) this is tick-driven — the host's existing loop calls [`Self::pump`],
/// which drains whatever the bus has buffered via the non-blocking `try_recv` and returns
/// immediately when the bus is quiet (no busy loop, no waiting). Invalidation only clears; the
/// rebuild stays on the focus path ([`ReplyContextCache::put`]), preserving the cache's
/// "None on miss" contract.
pub struct SyncInvalidator {
    sub: crate::bus::Subscriber,
    cache: ReplyContextCache,
}

impl SyncInvalidator {
    /// Subscribe to `bus` on behalf of `cache`. Only events published after this call are seen,
    /// so wire it up before the first sync can run.
    pub fn new(bus: &crate::bus::Bus, cache: ReplyContextCache) -> Self {
        Self { sub: bus.subscribe(), cache }
    }

    /// Drain every buffered bus event, invalidating the cache for each `IntegrationSynced` seen
    /// (other event kinds pass through untouched). Non-blocking — an empty bus returns 0 at once.
    /// Returns how many sync events were handled, so the caller's tick can log/meter quietly.
    pub fn pump(&mut self) -> usize {
        let mut handled = 0;
        while let Some(ev) = self.sub.try_recv() {
            if let crate::bus::BusEvent::IntegrationSynced { .. } = *ev {
                self.cache.invalidate();
                handled += 1;
            }
        }
        handled
    }
}

/// What one local maintenance pass changed ([`Db::run_local_maintenance`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalMaintenance {
    pub decayed: usize,
    pub corroborated: usize,
    pub overdue: usize,
    pub stale: usize,
    /// The commitments THIS pass flipped `open` → `overdue` (C-3). Each appears in exactly one
    /// pass — the status transition is the dedup watermark (see
    /// [`shogun_memory::recompute::recompute_overdue_and_staleness_detailed`]) — so
    /// [`overdue_notifications`] over this list fires once per item, never repeating.
    pub newly_overdue: Vec<shogun_memory::recompute::NewlyOverdue>,
}

/// Decide WHAT to notify for a maintenance pass's newly-overdue commitments (C-3). Pure: maps each
/// item to a [`ShowNotification`](shogun_agents::permission::LocalAction::ShowNotification) —
/// a non-egress, L1-permitted local action (pinned by `overdue_notifications_are_l1_non_sends`).
/// Dedup is upstream: the input list already contains each commitment exactly once, ever.
pub fn overdue_notifications(
    newly: &[shogun_memory::recompute::NewlyOverdue],
) -> Vec<shogun_agents::permission::Action> {
    use shogun_agents::permission::{Action, LocalAction};
    newly
        .iter()
        .map(|c| Action::Local(LocalAction::ShowNotification { text: format!("Overdue: {}", c.description) }))
        .collect()
}

/// One candidate answer to "which thread is this question about".
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadCandidate {
    pub thread_key: String,
    pub title: Option<String>,
    pub score: f64,
}

/// The outcome of resolving a referring question ([`Db::resolve_referent`]). `candidates` is
/// best-first and is what the UI offers when the verdict is `Ambiguous`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReferentOutcome {
    pub verdict: shogun_memory::thread::Referent,
    pub candidates: Vec<ThreadCandidate>,
}

/// Fraction of the thread title's words that appear in the question — the lexical agreement term
/// of salience. Words of one or two characters are skipped as too common to carry signal.
fn title_overlap(question_lower: &str, title_lower: &str) -> f64 {
    let words: Vec<&str> = title_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() > 2)
        .collect();
    if words.is_empty() {
        return 0.0;
    }
    let hits = words.iter().filter(|w| question_lower.contains(**w)).count();
    hits as f64 / words.len() as f64
}

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
    /// The local embedding model, when one is loaded. `None` means search runs lexical-only:
    /// every result still comes back via FTS, just without the semantic half (FR-MEM-22 — an
    /// un-embedded event is never invisible).
    embedder: Option<Arc<dyn shogun_memory::embed::Embedder>>,
    /// The compression config, when injected (Issue #63). `None` means the raw context path stays
    /// in effect — the desktop only supplies an `enabled` config behind a flag, so the default is
    /// unchanged behaviour.
    compression_config: Option<shogun_fusion::compress::CompressionConfig>,
    /// The internal event bus, when wired (design §2.2 / E-49). `None` keeps every publish a
    /// no-op, so a `Db` opened without the daemon composition behaves exactly as before.
    bus: Option<crate::bus::Bus>,
    /// Whether the store is answering (issue #121). Shared by every clone, because a failure the
    /// capture thread hits is the same failure the panel has to report.
    health: Arc<crate::memory_health::MemoryHealth>,
}

impl Db {
    /// Wrap an already-open, migrated connection.
    pub fn new(conn: Connection, clock: Clock) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
            clock,
            embedder: None,
            compression_config: None,
            bus: None,
            health: Arc::new(crate::memory_health::MemoryHealth::new()),
        }
    }

    /// The live memory-health signal (issue #121): what the last store operation did, and how
    /// many have failed since launch. The shell polls this for the notch's degraded indicator.
    pub fn memory_health(&self) -> crate::memory_health::MemoryHealthSnapshot {
        self.health.snapshot()
    }

    /// Record a failure against the health signal and say so in the log — **operation name and
    /// failure class only**, never a row, a query, or captured text (コード規約).
    fn note_fault(&self, op: &'static str, fault: MemoryFault, class: &'static str) {
        self.health.record_fault(fault, self.now_ms());
        crate::elog!("[memory] {op} failed: {class}");
    }

    /// Run `f` against the shared connection, recording what happened in the health signal.
    ///
    /// This is the seam issue #121 asks for: a lock failure and a query failure come back as
    /// distinct [`MemoryFault`]s instead of collapsing into the same empty value a genuinely
    /// empty table produces. Callers that legitimately cannot act on a failure still swallow the
    /// `Err` — but the failure is now recorded and visible, rather than silent.
    fn with_conn<T, E: FaultClass>(
        &self,
        op: &'static str,
        f: impl FnOnce(&Connection) -> Result<T, E>,
    ) -> MemoryResult<T> {
        let guard = match self.conn.lock() {
            Ok(g) => g,
            Err(_) => {
                self.note_fault(op, MemoryFault::LockPoisoned, "lock_poisoned");
                return Err(MemoryFault::LockPoisoned);
            }
        };
        match f(&guard) {
            Ok(v) => {
                self.health.record_success();
                Ok(v)
            }
            Err(e) => {
                self.note_fault(op, MemoryFault::Query, e.fault_class());
                Err(MemoryFault::Query)
            }
        }
    }

    /// [`Self::with_conn`] for the writes that need `&mut Connection` (transactions).
    fn with_conn_mut<T, E: FaultClass>(
        &self,
        op: &'static str,
        f: impl FnOnce(&mut Connection) -> Result<T, E>,
    ) -> MemoryResult<T> {
        let mut guard = match self.conn.lock() {
            Ok(g) => g,
            Err(_) => {
                self.note_fault(op, MemoryFault::LockPoisoned, "lock_poisoned");
                return Err(MemoryFault::LockPoisoned);
            }
        };
        match f(&mut guard) {
            Ok(v) => {
                self.health.record_success();
                Ok(v)
            }
            Err(e) => {
                self.note_fault(op, MemoryFault::Query, e.fault_class());
                Err(MemoryFault::Query)
            }
        }
    }

    /// [`Self::with_conn`] for helpers that return a plain value rather than a `Result` — the
    /// lock is still the part that can fail, and it is still recorded.
    fn read_conn<T>(&self, op: &'static str, f: impl FnOnce(&Connection) -> T) -> MemoryResult<T> {
        self.with_conn(op, |c| Ok::<T, String>(f(c)))
    }

    /// Take the lock for a body that must hold the guard across a loop, recording a poisoned lock
    /// before giving up. The closure helpers cannot express those bodies (the guard outlives any
    /// single statement), but the failure still has to be visible rather than an early `return`
    /// into a default value.
    fn lock_or_note(&self, op: &'static str) -> Option<std::sync::MutexGuard<'_, Connection>> {
        match self.conn.lock() {
            Ok(g) => Some(g),
            Err(_) => {
                self.note_fault(op, MemoryFault::LockPoisoned, "lock_poisoned");
                None
            }
        }
    }

    /// [`Self::with_conn`] for the public APIs that already report a reason to their caller.
    ///
    /// Those signatures predate the health signal and are the shape issue #121 wants everywhere —
    /// they never turned a failure into a success. The reason reaches the caller unchanged; the
    /// only new behaviour is that the failure now also registers as degraded memory.
    fn with_conn_reported<T>(
        &self,
        op: &'static str,
        f: impl FnOnce(&Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let guard = match self.conn.lock() {
            Ok(g) => g,
            Err(_) => {
                self.note_fault(op, MemoryFault::LockPoisoned, "lock_poisoned");
                return Err(format!("memory DB unavailable ({op}): lock poisoned"));
            }
        };
        match f(&guard) {
            Ok(v) => {
                self.health.record_success();
                Ok(v)
            }
            Err(e) => {
                self.note_fault(op, MemoryFault::Query, "error");
                Err(e)
            }
        }
    }

    /// [`Self::with_conn_reported`] for the writes that need `&mut Connection`.
    fn with_conn_mut_reported<T>(
        &self,
        op: &'static str,
        f: impl FnOnce(&mut Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = match self.conn.lock() {
            Ok(g) => g,
            Err(_) => {
                self.note_fault(op, MemoryFault::LockPoisoned, "lock_poisoned");
                return Err(format!("memory DB unavailable ({op}): lock poisoned"));
            }
        };
        match f(&mut guard) {
            Ok(v) => {
                self.health.record_success();
                Ok(v)
            }
            Err(e) => {
                self.note_fault(op, MemoryFault::Query, "error");
                Err(e)
            }
        }
    }

    /// Attach the internal event bus (design §2.2). Same handoff pattern as [`Self::with_embedder`]:
    /// the daemon composition owns the [`crate::bus::Bus`] and hands this handle over; without it,
    /// ingest publishes nothing and behaviour is unchanged.
    pub fn with_bus(mut self, bus: crate::bus::Bus) -> Self {
        self.bus = Some(bus);
        self
    }

    /// Attach the local embedding model, turning search from lexical-only into hybrid.
    ///
    /// Taken as a handle rather than constructed here so this crate stays free of the model
    /// runtime: the desktop loads the bundled model and hands it over, and tests inject a
    /// deterministic one.
    pub fn with_embedder(mut self, embedder: Arc<dyn shogun_memory::embed::Embedder>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    /// Inject the compression config (unset = the raw path stays in effect). Same handoff pattern
    /// as [`with_embedder`]: the desktop decides whether compression is on (behind a flag) and hands
    /// the config over.
    pub fn with_compression_config(
        mut self,
        config: shogun_fusion::compress::CompressionConfig,
    ) -> Self {
        self.compression_config = Some(config);
        self
    }

    /// The current compression config (`None` when unset).
    pub fn compression_config(&self) -> Option<&shogun_fusion::compress::CompressionConfig> {
        self.compression_config.as_ref()
    }

    /// Embed events that do not have a vector yet (FR-MEM-22: embedding is off the write path,
    /// so a slow model never delays a capture). No-op without a model. Returns how many were
    /// embedded.
    pub fn embed_pending(&self, limit: usize) -> usize {
        let Some(e) = self.embedder.as_deref() else { return 0 };
        self.with_conn_mut("embed.pending", |conn| {
            shogun_memory::embed_job::embed_all_pending(conn, e, limit)
        })
        .unwrap_or(0)
    }

    /// Open the on-disk database (runs migrations) and wrap it.
    pub fn open(path: impl AsRef<std::path::Path>, clock: Clock) -> Result<Self, MemoryError> {
        Ok(Self::new(shogun_memory::open(path)?, clock))
    }

    /// Open the encrypted on-device database (memory at rest). The key comes from the caller —
    /// the macOS layer reads it from the Keychain, its only permitted home (invariant 7).
    pub fn open_encrypted(
        path: impl AsRef<std::path::Path>,
        key: &shogun_memory::DbKey,
        clock: Clock,
    ) -> Result<Self, MemoryError> {
        Ok(Self::new(shogun_memory::open_encrypted(path, key)?, clock))
    }

    /// Open a fresh in-memory database (migrations applied) — for tests and ephemeral use.
    pub fn open_in_memory(clock: Clock) -> Result<Self, MemoryError> {
        Ok(Self::new(shogun_memory::open_in_memory()?, clock))
    }

    /// Open plaintext or encrypted DB at `path`. Encrypted files use Keychain on macOS or
    /// `SHOGUN_DB_KEY` (hex).
    pub fn open_at_path(path: impl AsRef<std::path::Path>, clock: Clock) -> Result<Self, String> {
        let path = path.as_ref();
        if path.exists() && !shogun_memory::is_plaintext_db(path) {
            let key = load_db_encryption_key()?;
            return Self::open_encrypted(path, &key, clock).map_err(|e| e.to_string());
        }
        Self::open(path, clock).map_err(|e| e.to_string())
    }

    /// Record a captured event (capture → memory, FR-CAP-03 dedup-touch). Swallows storage errors
    /// so the capture daemon never crashes on a write hiccup; returns `(id, touched)` on success.
    ///
    /// Swallowed, but no longer silent (issue #121): a failed write marks memory degraded, which
    /// is what turns the notch indicator amber instead of letting capture quietly stop recording.
    pub fn capture(&self, ev: &NewEvent<'_>) -> Option<(i64, bool)> {
        self.try_capture(ev).ok()
    }

    /// [`Self::capture`] with the failure kept (issue #121) — for callers that must tell a
    /// rejected write from a successful one.
    pub fn try_capture(&self, ev: &NewEvent<'_>) -> MemoryResult<(i64, bool)> {
        self.with_conn("capture", |c| event_log::insert_or_touch(c, ev))
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
        let ids = self
            .with_conn_mut("extract.persist_candidates", |c| {
                shogun_memory::extract::persist_candidates(c, id, &candidates, ev.ts, now)
            })
            .unwrap_or_default();
        Some((id, touched, ids))
    }

    /// The canonical content hash used across capture and notes. Delegates to the event log's
    /// own definition — the dedup key belongs to the log it locks, and two definitions would
    /// eventually stop colliding.
    fn content_hash(text: &str) -> String {
        shogun_memory::event_log::content_hash(text)
    }

    /// Capture a window body with near-duplicate collapse (FR-CAP-03): if `ev.content` is ≥98%
    /// similar to a recent capture body, reuse that body's hash so the event log dedup-touches
    /// instead of appending a near-identical row; otherwise a fresh hash makes a new event. The
    /// `content_hash` on the passed `ev` is ignored — this method decides it. Returns `(id, touched)`.
    pub fn capture_collapsed(&self, ev: &NewEvent<'_>) -> Option<(i64, bool)> {
        let recents = if ev.source == "capture" {
            self.recent_capture_bodies(ev.app_bundle_id, RECENT_DEDUP_WINDOW)
        } else {
            self.recent_source_bodies(ev.source, RECENT_DEDUP_WINDOW)
        };
        let recent_refs: Vec<Recent<'_>> =
            recents.iter().map(|(h, c)| Recent { content_hash: h, content: c }).collect();
        let decision = decide_hash(ev.content, &recent_refs, Self::content_hash);
        let collapsed = NewEvent { content_hash: decision.hash(), ..ev.clone() };
        self.capture(&collapsed)
    }

    /// Recent event bodies for one `source`, newest-first — used by near-dup collapse (FR-CAP-03).
    fn recent_source_bodies(&self, source: &str, limit: usize) -> Vec<(String, String)> {
        self.with_conn("event_log.recent_source_bodies", |c| event_log::recent_source_bodies(c, source, limit))
            .unwrap_or_default()
    }

    /// Recent user notes (`source = user`), newest-first, for explicit Memory API context reads.
    pub fn recent_user_notes(&self, limit: usize) -> Vec<String> {
        self.recent_source_bodies("user", limit)
            .into_iter()
            .map(|(_hash, content)| content)
            .collect()
    }

    /// Recent durable text activity for explicit Memory API context reads.
    ///
    /// Query-free context cannot rank evidence by relevance, so this returns a small newest-first
    /// tail from the durable text sources that already feed memory. Exact duplicate excerpts are
    /// collapsed because accessibility capture and screen OCR can observe the same window.
    pub(crate) fn recent_context_previews(
        &self,
        limit: usize,
        excerpt_chars: usize,
    ) -> Vec<(String, shogun_memory::event_log::RecentEventPreview)> {
        use std::collections::HashSet;

        const SOURCES: [&str; 6] = [
            "capture",
            "screen_ocr",
            "ai_session",
            "meeting",
            "gmail",
            "gcal",
        ];

        if limit == 0 {
            return Vec::new();
        }

        self.with_conn("memory.context_previews", |conn| {
            let mut previews = Vec::with_capacity(SOURCES.len() * limit);
            for source in SOURCES {
                let rows = shogun_memory::event_log::recent_previews_by_source(
                    conn,
                    source,
                    limit,
                    excerpt_chars,
                )?;
                previews.extend(rows.into_iter().map(|row| (source.to_string(), row)));
            }

            previews.sort_by(|(_, a), (_, b)| b.ts.cmp(&a.ts).then_with(|| b.id.cmp(&a.id)));
            let mut seen = HashSet::new();
            previews.retain(|(_, row)| {
                let normalized = row
                    .excerpt
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
                !normalized.is_empty() && seen.insert(normalized)
            });
            previews.truncate(limit);
            Ok::<_, rusqlite::Error>(previews)
        })
        .unwrap_or_default()
    }

    /// Recent capture bodies `(hash, content)` newest-first for one app, for the near-dup collapse.
    fn recent_capture_bodies(&self, app_bundle_id: Option<&str>, limit: usize) -> Vec<(String, String)> {
        self.with_conn("event_log.recent_capture_bodies", |c| event_log::recent_capture_bodies(c, app_bundle_id, limit))
            .unwrap_or_default()
    }

    /// Ingest a window capture end-to-end (the real capture path, FR-CAP-01/03 + WP2.7): near-dup
    /// collapse decides the hash, the event is inserted-or-touched, and on a *new* insert the
    /// first-stage local-rule extraction runs over the text. `dwell_ms` accumulates on a touch.
    /// Source is `capture`. Returns `(id, touched, candidate_ids)`; `None` only on a lock failure.
    pub fn ingest_capture(
        &self,
        bundle_id: Option<&str>,
        window_title: Option<&str>,
        text: &str,
        dwell_ms: i64,
    ) -> Option<(i64, bool, Vec<i64>)> {
        self.ingest_text_event("capture", bundle_id, window_title, text, dwell_ms, None)
    }

    /// Ingest on-device screen OCR text (issue #107). Source is `screen_ocr`; only the extracted
    /// string + provenance reach this method. Optional JPEG frames are stored separately via
    /// [`store_screen_frame`] (72 h retention — explicit invariant-2 exception, 2026-08-02).
    pub fn ingest_screen_ocr(
        &self,
        bundle_id: Option<&str>,
        window_title: Option<&str>,
        text: &str,
        dwell_ms: i64,
        display_id: Option<i64>,
    ) -> Option<(i64, bool, Vec<i64>)> {
        self.ingest_text_event("screen_ocr", bundle_id, window_title, text, dwell_ms, display_id)
    }

    fn ingest_text_event(
        &self,
        source: &'static str,
        bundle_id: Option<&str>,
        window_title: Option<&str>,
        text: &str,
        dwell_ms: i64,
        display_id: Option<i64>,
    ) -> Option<(i64, bool, Vec<i64>)> {
        let ev = NewEvent {
            ts: self.now_ms(),
            source,
            kind: "text",
            app_bundle_id: bundle_id,
            window_title,
            content: text,
            content_hash: "", // ignored — capture_collapsed decides it
            dwell_ms,
            display_id,
            window_bounds: None,
        };
        let (id, touched) = self.capture_collapsed(&ev)?;
        if touched {
            return Some((id, touched, Vec::new()));
        }
        let candidates = shogun_memory::extract::extract(text);
        let now = self.now_ms();
        let ids = self
            .with_conn_mut("extract.persist_candidates", |c| {
                shogun_memory::extract::persist_candidates(c, id, &candidates, ev.ts, now)
            })
            .unwrap_or_default();
        Some((id, touched, ids))
    }

    /// Ingest a batch of synced integration items into the event log (WP4.2, §6.9:
    /// `integration.synced` → event log → search/Fusion). Each record is appended under its own
    /// `source` tag (`gmail` / `gcal` / …) so a synced email is a first-class event next to a
    /// captured window (FR-INT-05); the item's own timestamp is preserved. Dedup is per-source
    /// (FR-CAP-03): re-syncing the same item touches `last_seen_at` rather than duplicating it.
    ///
    /// On a **new** insert (not a dedup-touch) the first-stage local-rule extraction (WP2.7) runs
    /// over the item body, so a commitment or open-loop stated in an email/message flows into the
    /// state tables just as one captured on screen does — each candidate is low-confidence
    /// (≤ [`shogun_memory::extract::LOCAL_RULE_MAX_CONFIDENCE`]) and linked to the ingested event
    /// (FR-ST-02). Extraction is skipped on a touch so a re-sync never multiplies candidates.
    ///
    /// The caller ([`crate::service_gate`]) has already authorized the sync; this method only
    /// persists. On completion, a batch that inserted ≥ 1 **new** item publishes
    /// [`crate::bus::BusEvent::IntegrationSynced`] per source on the bus (§6.9, design §2.2) so
    /// subscribers — e.g. the reply-context invalidator — learn that memory just changed; a
    /// dedup-only or empty batch publishes nothing (nothing changed). Returns a zeroed summary on
    /// a lock failure (never panics).
    pub fn ingest_integration(&self, items: &[shogun_mcp::sync::IngestItem]) -> IngestSummary {
        let now = self.now_ms();
        let mut summary = IngestSummary::default();
        // Per-source newly-inserted counts for the bus. A batch normally carries one service, so a
        // tiny vec beats a map.
        let mut synced: Vec<(&'static str, u64)> = Vec::new();
        {
            let Some(mut guard) = self.lock_or_note("ingest.items") else {
                return IngestSummary::default();
            };
            // Rejected writes are counted, not logged per item: a batch that fails wholesale
            // would otherwise write one log line per row. One line, and the health signal.
            let mut rejected = 0usize;
            for it in items {
                let hash = Self::content_hash(&it.body);
                let ev = NewEvent {
                    ts: it.ts_ms,
                    source: it.source,
                    kind: it.kind,
                    app_bundle_id: None,
                    window_title: (!it.title.is_empty()).then_some(it.title.as_str()),
                    content: &it.body,
                    content_hash: &hash,
                    dwell_ms: 0,
                    display_id: None,
                    window_bounds: None,
                };
                let Ok((id, touched)) = event_log::insert_or_touch(&guard, &ev) else {
                    rejected += 1;
                    continue;
                };
                summary.processed += 1;
                if touched {
                    continue;
                }
                summary.newly_inserted += 1;
                match synced.iter_mut().find(|(s, _)| *s == it.source) {
                    Some((_, n)) => *n += 1,
                    None => synced.push((it.source, 1)),
                }
                // A newly-ingested item is extracted for commitments / open loops, linked to it.
                let candidates = shogun_memory::extract::extract(&it.body);
                if !candidates.is_empty() {
                    let ids =
                        shogun_memory::extract::persist_candidates(&mut guard, id, &candidates, it.ts_ms, now)
                            .unwrap_or_default();
                    summary.candidates += ids.len();
                }
            }
            drop(guard);
            if rejected > 0 {
                self.note_fault("ingest.items", MemoryFault::Query, "write_rejected");
                crate::elog!("[memory] ingest.items rejected {rejected} of {} item(s)", summary.processed + rejected);
            } else {
                self.health.record_success();
            }
        }
        // Publish after the DB lock is released. Non-blocking (AR-07); carries only the source tag
        // and a count — never item content.
        if let Some(bus) = &self.bus {
            for (source, count) in synced {
                bus.publish(crate::bus::BusEvent::IntegrationSynced { source, count });
            }
        }
        summary
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

    /// Open a meeting interval (FR-MT-05). The macOS adapter has no database access of its own —
    /// every write goes through `Db`, keeping the data tier in the core (invariant 1).
    pub fn open_meeting(&self, title: Option<&str>, app_bundle_id: Option<&str>, confidence: f64, provenance: &str) -> Option<i64> {
        self.with_conn("meeting.open", |conn| {
        shogun_memory::session::open(
            conn,
            &shogun_memory::session::NewSession {
                kind: "meeting",
                started_at: self.now_ms(),
                title,
                app_bundle_id,
                calendar_occurrence_id: None,
                confidence,
                provenance,
            },
        )
        })
        .ok()
    }

    /// Attach an already-recorded event to a session (FR-MT-05). Best-effort: the event is durable
    /// whether or not it attaches, so a lock/write failure is swallowed. Returns whether it stuck.
    pub fn attach_event_to_meeting(&self, session_id: i64, event_id: i64) -> bool {
        self.with_conn("meeting.attach_event", |conn| {
            shogun_memory::session::attach_event(conn, session_id, event_id)
        })
        .is_ok()
    }

    /// Close a meeting interval. Idempotent — the first close wins (FR-MT-11).
    ///
    /// Closing also puts the finished meeting on the searchable spine (FR-MT-14,
    /// `meeting_index::index_session`): the transcript is final exactly now, and a close that
    /// skipped indexing would leave a meeting the user can read back but never find. Indexing is
    /// best-effort — the close itself must never fail because search maintenance did.
    pub fn close_meeting(&self, id: i64) -> bool {
        let now = self.now_ms();
        self.with_conn("meeting.close", |conn| {
            shogun_memory::session::close(conn, id, now)?;
            // Search maintenance is best-effort on purpose: the close itself has already stuck,
            // and failing it here would re-open a meeting the user ended.
            let _ = shogun_memory::meeting_index::index_session(conn, id);
            Ok::<(), rusqlite::Error>(())
        })
        .is_ok()
    }

    /// Save the note typed during a meeting (FR-MT-10).
    ///
    /// A note is often flushed (blur / debounce) moments after auto-wrap already closed the
    /// session, and users edit notes from the Recap afterwards — so if the session has ended,
    /// the index row is refreshed here (`index_session` replaces a changed note's row). While
    /// the session is still open, indexing waits for the close: half-typed notes do not belong
    /// in search.
    pub fn save_meeting_note(&self, session_id: i64, body: &str) -> bool {
        let now = self.now_ms();
        self.with_conn("meeting.save_note", |conn| {
            shogun_memory::session_notes::save(conn, session_id, body, now)?;
            let ended = shogun_memory::session::get(conn, session_id)
                .ok()
                .flatten()
                .is_some_and(|s| s.ended_at.is_some());
            if ended {
                let _ = shogun_memory::meeting_index::index_session(conn, session_id);
            }
            Ok::<(), rusqlite::Error>(())
        })
        .is_ok()
    }

    /// Store the model-generated Recap for a meeting interval (MT4, FR-MT-19). Upsert on
    /// `session_id` (one Recap per interval): a re-run replaces the degraded/previous minutes. The
    /// summary is redacted inside the memory writer. Best-effort — a write failure leaves the
    /// existing (degraded) Recap in place rather than interrupting anything.
    pub fn save_meeting_recap(
        &self,
        session_id: i64,
        summary: &str,
        decisions_json: &str,
        next_actions_json: &str,
        model: &str,
    ) -> bool {
        let now = self.now_ms();
        self.with_conn("meeting.save_recap", |conn| {
            shogun_memory::meeting_recaps::save(
                conn,
                session_id,
                summary,
                decisions_json,
                next_actions_json,
                model,
                now,
            )
        })
        .is_ok()
    }

    /// The transcript of a meeting interval as `(speaker, text)` in time order (MT4 input). Drops
    /// the ts/confidence columns the Recap builder does not need. Empty when the interval has no
    /// transcript (audio degraded to notes-only) or on a DB error.
    pub fn transcript_for_recap(&self, session_id: i64) -> Vec<(Option<String>, String)> {
        self.with_conn("meeting.transcript_for_recap", |conn| {
            shogun_memory::transcript_segments::for_session(conn, session_id)
        })
        .unwrap_or_default()
        .into_iter()
        .map(|(_ts, speaker, text, _confidence)| (speaker, text))
        .collect()
    }

    /// Full transcript lines for the post-meeting viewer (FR-MT-10): `(ts, speaker, text)` in time
    /// order. Empty when the interval has no transcript or on a DB error.
    pub fn meeting_transcript(&self, session_id: i64) -> Vec<(i64, Option<String>, String)> {
        self.with_conn("meeting.transcript", |conn| {
            shogun_memory::transcript_segments::for_session(conn, session_id)
        })
        .unwrap_or_default()
        .into_iter()
        .map(|(ts, speaker, text, _confidence)| (ts, speaker, text))
        .collect()
    }

    /// The note typed during a meeting interval (FR-MT-10), if any. `None` covers both "no note"
    /// and a DB error — the Recap builder treats both the same (nothing to add from notes), and
    /// the failure itself is recorded in the health signal rather than lost (issue #121).
    pub fn meeting_note(&self, session_id: i64) -> Option<String> {
        self.with_conn("meeting.note", |conn| shogun_memory::session_notes::get(conn, session_id))
            .ok()
            .flatten()
    }

    /// Append one transcribed line to a meeting interval (FR-MT-13). The text is redacted inside the
    /// memory writer. Best-effort: a write failure drops the line rather than interrupting capture.
    pub fn append_transcript(
        &self,
        session_id: i64,
        ts: i64,
        speaker: shogun_memory::transcript_segments::Speaker,
        text: &str,
        confidence: f64,
    ) -> bool {
        let now = self.now_ms();
        self.with_conn("meeting.append_transcript", |conn| {
            shogun_memory::transcript_segments::append(
                conn,
                &shogun_memory::transcript_segments::NewSegment { session_id, ts, speaker, text, confidence },
                now,
            )
        })
        .is_ok()
    }

    /// Close intervals left open by a previous run (crash, force-quit, power cut).
    ///
    /// Only rows that started before `started_before_ms` (the caller's boot time) are touched, so
    /// a call that lands after this run has opened a live meeting cannot zero-length it. Returns
    /// how many were closed. They are closed at their `started_at`, not at "now": the app has no
    /// idea when the meeting actually ended, and inventing a duration that spans the time the
    /// machine was off would be a worse answer than a zero-length interval.
    pub fn close_abandoned_meetings(&self, started_before_ms: i64) -> usize {
        let now = self.now_ms();
        self.read_conn("meeting.close_abandoned", |conn| {
        // Ids first, then close, then index: a crash-abandoned meeting still holds whatever
        // transcript and note it captured, and closing is the moment that text goes on the search
        // spine (FR-MT-14) — the bulk UPDATE alone would leave these the only meetings the user
        // can read back but never find. Indexing is best-effort, same as `close_meeting`.
        let ids: Vec<i64> = conn
            .prepare("SELECT id FROM sessions WHERE ended_at IS NULL AND started_at < ?1")
            .and_then(|mut s| {
                s.query_map([started_before_ms], |r| r.get(0))
                    .and_then(|rows| rows.collect())
            })
            .unwrap_or_default();
        let closed = conn
            .execute(
                "UPDATE sessions SET ended_at = started_at, updated_at = ?1
                  WHERE ended_at IS NULL AND started_at < ?2",
                rusqlite::params![now, started_before_ms],
            )
            .unwrap_or(0);
        for id in ids {
            let _ = shogun_memory::meeting_index::index_session(conn, id);
        }
        closed
        })
        .unwrap_or(0)
    }

    /// The degraded Recap for an interval (FR-MT-19): what can be said locally, with no model and
    /// no network. `None` only when the interval does not exist.
    pub fn meeting_recap(&self, session_id: i64) -> Option<crate::meeting::recap::Recap> {
        self.read_conn("meeting.recap", |conn| {
        let session = shogun_memory::session::get(conn, session_id).ok().flatten()?;
        let notes = shogun_memory::session_notes::get(conn, session_id).ok().flatten();
        // How much this Recap had to work with. Counted rather than estimated — it is the honest
        // answer to "is anything actually being captured in meetings?" (context health).
        let captured: i64 = conn
            .query_row(
                "SELECT count(*) FROM event_log WHERE session_id = ?1",
                [session_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Some(crate::meeting::recap::degraded(&session, notes, captured as usize))
        })
        .ok()
        .flatten()
    }

    /// The stored (model-generated) minutes for an interval (MT4, FR-MT-19), if they exist yet.
    ///
    /// Unlike [`Self::meeting_recap`] (the always-available degraded Recap), this returns `None`
    /// until the Batch lane has finished and written a row: the minutes land asynchronously after
    /// the degraded Recap is already on screen, so the panel refetches on the `meeting_recap`
    /// event. The two structured columns come back as raw JSON strings — the wiring layer
    /// deserializes them (never panicking on a bad column).
    pub fn meeting_recap_full(&self, session_id: i64) -> Option<shogun_memory::meeting_recaps::StoredRecap> {
        self.with_conn("meeting.recap_full", |c| shogun_memory::meeting_recaps::get(c, session_id))
            .ok()
            .flatten()
    }

    /// Confidence-gated memory lines for the inline draft prompt ([`crate::inline::compose_inline`]):
    /// the commitments the user owes and the open loops in play, passed through the FR-ST-20 gate
    /// (High stated as fact, Medium prefixed `possibly:`, Low dropped) so a low-confidence guess is
    /// never handed to the model as a fact. Capped at `limit` lines.
    pub fn inline_memory(&self, limit: usize) -> Vec<String> {
        self.inline_memory_with_refs(limit).into_iter().map(|(s, _, _)| s).collect()
    }

    /// `inline_memory` の provenance 付き版: (confidence ゲート済み fact, 由来テーブル, row id)。
    /// `inline_memory`（文字列版・公開 API）はこれに委譲する（DRY・API 不変）。
    /// commitments → open_loops の順、同じ confidence gate、同じ truncate。
    fn inline_memory_with_refs(
        &self,
        limit: usize,
    ) -> Vec<(String, shogun_fusion::block::StateTable, i64)> {
        self.try_inline_memory_with_refs(limit).unwrap_or_default()
    }

    /// [`Self::inline_memory_with_refs`] with the failure kept (issue #121).
    ///
    /// The grounding path is where an unreported failure does the most damage: an empty fact list
    /// reads to the model as "this user owes nothing and is waiting on nothing", and it will say
    /// so confidently. A caller that can tell the two apart can hedge instead.
    fn try_inline_memory_with_refs(
        &self,
        limit: usize,
    ) -> MemoryResult<Vec<(String, shogun_fusion::block::StateTable, i64)>> {
        use shogun_fusion::block::StateTable;
        use shogun_fusion::confidence::{treat_fact, Treatment};
        let (commitments, open_loops) = self.with_conn("state.for_grounding", |conn| {
            Ok::<_, rusqlite::Error>((state::list_commitments(conn)?, state::list_open_loops(conn)?))
        })?;

        let mut out: Vec<(String, StateTable, i64)> = Vec::new();
        for c in commitments {
            // mirror commitments_due: skip done/cancelled rows
            if c.status == "done" || c.status == "cancelled" {
                continue;
            }
            let text = format!("you committed: {}", c.description);
            match treat_fact(&text, c.confidence) {
                Treatment::Fact(s) | Treatment::Possible(s) => {
                    out.push((s, StateTable::Commitments, c.id));
                }
                Treatment::Excluded => {}
            }
        }
        for l in open_loops {
            // mirror open_loops: skip closed rows
            if l.status == "closed" {
                continue;
            }
            let text = format!("open loop: {}", l.description);
            match treat_fact(&text, l.confidence) {
                Treatment::Fact(s) | Treatment::Possible(s) => {
                    out.push((s, StateTable::OpenLoops, l.id));
                }
                Treatment::Excluded => {}
            }
        }
        out.truncate(limit);
        Ok(out)
    }

    /// Ingest turns recovered from an AI coding-tool session log (Phase R4).
    ///
    /// Each turn becomes an `ai_session` event, keyed by the tool's own session id so the whole
    /// conversation is one thread — which is exactly the grouping a later "what did we decide
    /// about X" needs. Re-importing a log is safe: the content hash is per turn, so already-seen
    /// turns touch their existing row instead of duplicating (a session log is append-only and
    /// gets re-read as it grows).
    ///
    /// Turns are extracted for commitments and open loops like any other source — a promise made
    /// while pairing with a tool is still a promise.
    pub fn ingest_ai_session(
        &self,
        turns: &[shogun_memory::ai_session::SessionTurn],
    ) -> IngestSummary {
        let now = self.now_ms();
        let Some(mut guard) = self.lock_or_note("ingest.ai_session") else {
            return IngestSummary::default();
        };
        let mut summary = IngestSummary::default();
        for t in turns {
            // The session id is the thread; hashing it with the turn keeps two identical messages
            // in different sessions distinct.
            let hash = Self::content_hash(&format!("{}:{}:{}", t.session_id, t.ts_ms, t.text));
            let title = format!("{} · {}", t.role.as_str(), t.cwd.as_deref().unwrap_or("session"));
            let ev = NewEvent {
                ts: t.ts_ms,
                source: "ai_session",
                kind: "message",
                app_bundle_id: None,
                window_title: Some(&title),
                content: &t.text,
                content_hash: &hash,
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            };
            // The session id is the thread — passed explicitly so both sides of the conversation
            // land in one thread regardless of what the per-turn title says.
            let Ok((id, touched)) =
                event_log::insert_or_touch_with_thread(&guard, &ev, Some(&t.session_id))
            else {
                continue;
            };
            summary.processed += 1;
            if touched {
                continue;
            }
            summary.newly_inserted += 1;
            let candidates = shogun_memory::extract::extract(&t.text);
            if !candidates.is_empty() {
                let ids =
                    shogun_memory::extract::persist_candidates(&mut guard, id, &candidates, t.ts_ms, now)
                        .unwrap_or_default();
                summary.candidates += ids.len();
            }
        }
        summary
    }

    /// Build the reply context for a thread — everything a one-press draft needs, assembled
    /// **before** the press.
    ///
    /// The SLO is 150ms to offer the action and 1s to first token, which rules out collecting
    /// context on the press (CLAUDE.md: the cache is pre-assembled, never gathered on demand).
    /// The caller runs this off the focus path and holds the result; pressing the button then
    /// only starts generation.
    ///
    /// `build_ms` is carried on the pack so the assembly cost is measurable in place rather than
    /// inferred — the SLO is an acceptance criterion, so the measurement ships with the code.
    pub fn build_reply_context(&self, thread_key: &str) -> ReplyContext {
        let started = std::time::Instant::now();
        let facts = self.inline_memory(6);
        let (title, turns) = {
            let Some(conn) = self.lock_or_note("reply_context.build") else {
                return ReplyContext { thread_key: thread_key.to_string(), ..Default::default() };
            };
            let title = shogun_memory::thread::recent(&conn, 50)
                .ok()
                .and_then(|rows| rows.into_iter().find(|t| t.thread_key == thread_key))
                .and_then(|t| t.title);
            let turns = shogun_memory::thread::recent_events(&conn, thread_key, REPLY_TURNS)
                .unwrap_or_default()
                .into_iter()
                .map(|(event_id, ts, content)| Evidence {
                    event_id,
                    ts,
                    source: String::new(),
                    title: None,
                    excerpt: shogun_memory::search::excerpt(&content, "", REPLY_TURN_CHARS),
                    frame_id: None,
                })
                .collect::<Vec<_>>();
            (title, turns)
        };
        // Past threads that resemble this one, so a reply can recall what was said before.
        let related = match title.as_deref().filter(|t| !t.is_empty()) {
            Some(t) => self
                .search(t, REPLY_RELATED)
                .into_iter()
                .filter(|h| h.window_title.as_deref() != title.as_deref())
                .map(|h| Evidence {
                    event_id: h.event_id,
                    ts: h.ts,
                    source: h.source,
                    title: h.window_title,
                    excerpt: shogun_memory::search::excerpt(&h.content, t, REPLY_RELATED_CHARS),
                    frame_id: None,
                })
                .collect(),
            None => Vec::new(),
        };
        ReplyContext {
            thread_key: thread_key.to_string(),
            title,
            turns,
            facts,
            related,
            build_ms: started.elapsed().as_millis() as u64,
            payload_source: PayloadSource::OnScreenOnly,
        }
    }

    /// event log 上の gmail スレッド候補 `(thread_key, title)`。融合リンカの入力。
    pub fn gmail_thread_candidates(&self, limit: usize) -> Vec<(String, String)> {
        self.with_conn("thread.recent", |conn| shogun_memory::thread::recent(conn, limit))
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t.thread_key.starts_with("gmail:"))
            .filter_map(|t| t.title.map(|title| (t.thread_key, title)))
            .collect()
    }

    /// 画面セレクタ（on-screen のタイトル）を使って、取得済み gmail スレッドに解決してから
    /// 文脈を組む。解決できれば gmail スレッドの turns を使い `Fetched`、できなければ元の
    /// thread_key で `OnScreenOnly`（設計 §3）。
    pub fn build_reply_context_for_screen(
        &self,
        on_screen_thread_key: &str,
        on_screen_title: &str,
    ) -> ReplyContext {
        let candidates = self.gmail_thread_candidates(50);
        match shogun_memory::thread::link_on_screen_to_thread(on_screen_title, &candidates) {
            Some(gmail_key) => {
                let mut ctx = self.build_reply_context(&gmail_key);
                // 同期スレッドの識別子（thread_key）を provenance に。
                ctx.payload_source = PayloadSource::Fetched { thread_key: gmail_key };
                ctx
            }
            None => {
                let mut ctx = self.build_reply_context(on_screen_thread_key);
                ctx.payload_source = PayloadSource::OnScreenOnly;
                ctx
            }
        }
    }

    /// Resolve what a referring question ("how's that going?") is about.
    ///
    /// Ranks the recently-active threads by [`shogun_memory::thread::salience`] and classifies the
    /// result. When two threads are close the answer is [`Referent::Ambiguous`] and the caller must
    /// ask rather than pick: answering confidently about the wrong piece of someone's work is a
    /// worse failure than one clarifying question.
    ///
    /// `on_screen` is the thread the user is currently looking at, when known — the strongest
    /// single signal for what "that" means.
    pub fn resolve_referent(&self, query: &str, on_screen: Option<&str>) -> ReferentOutcome {
        use shogun_memory::thread;
        let now = self.now_ms();
        let Some(conn) = self.lock_or_note("referent.resolve") else {
            return ReferentOutcome::default();
        };
        let Ok(threads) = thread::recent(&conn, 20) else {
            self.note_fault("referent.resolve", MemoryFault::Query, "query");
            return ReferentOutcome::default();
        };
        let loops = thread::open_loop_counts(&conn).unwrap_or_default();
        // Lexical agreement between the question and the thread's own title.
        let q = query.to_lowercase();
        let mut scored: Vec<ThreadCandidate> = threads
            .into_iter()
            .map(|t| {
                let title = t.title.clone().unwrap_or_default();
                let lexical = title_overlap(&q, &title.to_lowercase());
                let score = thread::salience(thread::Salience {
                    age_ms: now - t.last_activity_at,
                    open_loops: loops.get(&t.thread_key).copied().unwrap_or(0),
                    on_screen: on_screen == Some(t.thread_key.as_str()),
                    lexical_match: lexical,
                });
                ThreadCandidate { thread_key: t.thread_key, title: t.title, score }
            })
            .collect();
        scored.sort_by(|a, b| b.score.total_cmp(&a.score));
        let scores: Vec<f64> = scored.iter().map(|c| c.score).collect();
        ReferentOutcome { verdict: thread::resolve(&scores), candidates: scored }
    }

    /// Assemble the grounded context for one question (Phase R1 of the context-layer plan).
    ///
    /// [`Self::inline_memory`] alone answers "what does SHOGUN track about me" but not "what
    /// happened with X" — it never looks at the event log, so mail, messages and captured
    /// windows could not reach an answer. This adds the retrieval half: the question is run
    /// through hybrid search and the best-matching events come back as dated, attributed
    /// evidence next to the state facts.
    ///
    /// `max_hits` caps the evidence lines and `excerpt_chars` caps each one, so a single huge
    /// window capture cannot eat the whole prompt. Search is FTS-only until the embedding model
    /// lands (`search_hybrid` takes the vector half then, with no change here).
    pub fn assemble_context(&self, query: &str, max_hits: usize, excerpt_chars: usize) -> ContextPack {
        let (evidence, screen_frames) = self.assemble_evidence_with_frames(query, max_hits, excerpt_chars);
        ContextPack { facts: self.inline_memory(FACT_LIMIT), evidence, screen_frames }
    }

    /// Lexical search over meeting recaps and transcripts. Query-relevant, not latest-session.
    pub fn search_meetings(&self, query: &str, limit: usize) -> Vec<shogun_memory::search::MeetingSearchHit> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        self.with_conn("search.search_meetings", |c| shogun_memory::search::search_meetings(c, query, limit))
            .unwrap_or_default()
    }

    /// The evidence half of [`Self::assemble_context`]: hybrid-search the query and turn the best
    /// hits into dated, attributed [`Evidence`], each excerpt capped at `excerpt_chars`. Split out
    /// so the compressed path can take evidence WITHOUT also loading state facts (which it rebuilds
    /// from the ref version), instead of running the two state queries twice (Issue #63 finding #2).
    ///
    /// Event-log hits and meeting-interval hits (recap + transcript) are merged by relevance score
    /// so a question about a specific past meeting surfaces that session, not whatever ended last.
    fn assemble_evidence_with_frames(
        &self,
        query: &str,
        max_hits: usize,
        excerpt_chars: usize,
    ) -> (Vec<Evidence>, Vec<ScreenFrameRef>) {
        let now = self.now_ms();
        let local_days = local_day_bounds(now);
        let screen_frames =
            if shogun_memory::search::query_wants_visual_recall(query, now, local_days) {
            self.recall_screen_frames(query, max_hits, excerpt_chars)
        } else {
            Vec::new()
        };
        let mut frame_by_event: std::collections::HashMap<i64, i64> = screen_frames
            .iter()
            .map(|f| (f.event_id, f.frame_id))
            .collect();

        let mut event_hits = self.search(query, max_hits);
        if shogun_memory::search::query_asks_about_screen(query) {
            let ocr_hits = self.search_source(query, "screen_ocr", max_hits);
            let seen: std::collections::HashSet<i64> = event_hits.iter().map(|h| h.event_id).collect();
            for mut h in ocr_hits {
                if seen.contains(&h.event_id) {
                    continue;
                }
                h.score *= 1.15;
                event_hits.push(h);
            }
            event_hits.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(b.ts.cmp(&a.ts))
            });
            event_hits.truncate(max_hits);
        }
        let meeting_hits = self.search_meetings(query, max_hits);

        let event_ids: Vec<i64> = event_hits.iter().map(|h| h.event_id).collect();
        for (event_id, frame_id) in self.frame_ids_for_events(&event_ids) {
            frame_by_event.entry(event_id).or_insert(frame_id);
        }

        let mut ranked: Vec<(f64, Evidence)> = Vec::with_capacity(event_hits.len() + meeting_hits.len());
        for h in event_hits {
            let frame_id = frame_by_event.get(&h.event_id).copied();
            let mut excerpt = shogun_memory::search::excerpt(&h.content, query, excerpt_chars);
            if let Some(fid) = frame_id {
                excerpt.push_str(&format!(" [screen frame {fid} stored]"));
            }
            ranked.push((
                h.score,
                Evidence {
                    event_id: h.event_id,
                    ts: h.ts,
                    source: h.source,
                    title: h.window_title,
                    excerpt,
                    frame_id,
                },
            ));
        }
        for h in meeting_hits {
            ranked.push((
                h.score,
                Evidence {
                    event_id: -h.session_id,
                    ts: h.ts,
                    source: "meeting".to_string(),
                    title: h.title,
                    excerpt: shogun_memory::search::excerpt(&h.content, query, excerpt_chars),
                    frame_id: None,
                },
            ));
        }

        // Frame-only hits: add evidence lines from stored JPEGs not already in ranked set.
        let seen_events: std::collections::HashSet<i64> =
            ranked.iter().map(|(_, e)| e.event_id).collect();
        let top_event_score = ranked.iter().map(|(s, _)| *s).fold(0.0_f64, f64::max);
        let frame_score = if top_event_score > 0.0 {
            top_event_score * 0.75
        } else {
            0.5
        };
        for f in &screen_frames {
            if seen_events.contains(&f.event_id) {
                continue;
            }
            let mut excerpt = f.ocr_excerpt.clone();
            if f.needs_rescan {
                excerpt.push_str(" [thin OCR — re-scan frame recommended]");
            }
            excerpt.push_str(&format!(" [screen frame {} stored]", f.frame_id));
            ranked.push((
                frame_score,
                Evidence {
                    event_id: f.event_id,
                    ts: f.ts,
                    source: "screen_ocr".to_string(),
                    title: f.window_title.clone(),
                    excerpt,
                    frame_id: Some(f.frame_id),
                },
            ));
        }

        ranked.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.1.ts.cmp(&a.1.ts))
        });
        let evidence = ranked
            .into_iter()
            .map(|(_, e)| e)
            .filter(|e| !e.excerpt.is_empty())
            .take(max_hits)
            .collect();
        (evidence, screen_frames)
    }

    /// 圧縮版のコンテキスト組み立て（Issue #63）。`config.enabled` が false のときは
    /// 呼び出し側が `assemble_context` を使う想定なので、ここは true 前提。ローカル処理のみ
    /// （LLM を呼ばない）。処理が `COMPRESS_BUDGET_MS` を超えた/失敗したら raw にフォールバック。
    pub fn assemble_context_compressed(
        &self,
        query: &str,
        max_hits: usize,
        excerpt_chars: usize,
        thread_keys: &[String],
        config: &shogun_fusion::compress::CompressionConfig,
    ) -> (ContextPack, shogun_fusion::compress::CompressionStats, bool) {
        use shogun_fusion::budget::HeuristicEstimator;
        use shogun_fusion::compress::{compress, Candidates};

        let started = std::time::Instant::now();
        // 支配コストは evidence 検索。まずそれだけ走らせ、直後に予算をチェックする（finding #1）。
        // fact は各由来で 1 回だけ読む（compressed は ref 版、fallback は文字列版）ので、ここで
        // assemble_context を呼んで両方を二重ロードすることはしない（finding #2）。
        let (evidence, screen_frames) = self.assemble_evidence_with_frames(query, max_hits, excerpt_chars);
        let est = HeuristicEstimator::default();

        // 検索直後の早期フォールバック: 支配コストの直後で予算を超えていたら、要約組み立てや
        // compress に進まず raw をそのまま返す（この guard こそが実際に総処理時間を縛る）。
        if started.elapsed().as_millis() as u64 > COMPRESS_BUDGET_MS {
            return self.raw_fallback(query, evidence, screen_frames, started);
        }

        // Task 2: fact ブロックは実 state id 付きの ref 版から組み立てる（fallback 用の文字列版とは
        // 別。ここで二重に読まない）。
        let mut blocks = facts_to_blocks(&self.inline_memory_with_refs(FACT_LIMIT), &est);
        blocks.extend(evidence_to_blocks(&evidence, EVIDENCE_RELEVANCE, &est));
        // 解決済みスレッドの保存済み要約を候補に加える（差し替えレバー）。
        blocks.extend(self.thread_summaries_to_blocks(thread_keys, &est));
        // 取得 evidence の属する session の保存済み要約を候補に加える（thread と対称・Issue #63）。
        // N+1 を避け 1 クエリで一括取得（finding #1）。既に ThreadSummary として消費済みの
        // thread_key を持つ session は重複になるのでスキップ（finding #3）。
        let consumed_threads: std::collections::HashSet<&str> =
            thread_keys.iter().map(String::as_str).collect();
        let evidence_event_ids: Vec<i64> = evidence.iter().map(|e| e.event_id).collect();
        let session_ids = self.session_ids_for_events(&evidence_event_ids);
        for (sid, thread_key, summary) in self.session_summaries_for(&session_ids) {
            if consumed_threads.contains(thread_key.as_str()) {
                // 同じ会話が ThreadSummary として既に候補に入っている。重複を避ける。
                continue;
            }
            let Some(s) = summary else { continue };
            blocks.push(shogun_fusion::block::ContextBlock::new(
                shogun_fusion::block::BlockRef::Session(sid),
                shogun_fusion::block::SourceKind::SessionSummary,
                s,
                // 参照先として retrieved evidence が属する＝関連度は高い。confidence は要約＝1.0。
                SESSION_SUMMARY_SCORE,
                &est,
            ));
        }

        // 最終 guard: 要約組み立て後・compress 前にもう一度予算をチェック（保険）。
        if started.elapsed().as_millis() as u64 > COMPRESS_BUDGET_MS {
            return self.raw_fallback(query, evidence, screen_frames, started);
        }

        let out = compress(Candidates { blocks }, config);
        // 圧縮済みブロックから ContextPack を再構成（facts と evidence に振り分け）。
        // Task 1: evidence の ts/source/title を検索結果から復元するため、
        // event_id → &Evidence マップを事前に作る。
        let ev_by_id: std::collections::HashMap<i64, &Evidence> =
            evidence.iter().map(|e| (e.event_id, e)).collect();
        let mut out_facts = Vec::new();
        let mut out_evidence = Vec::new();
        for b in &out.blocks {
            match b.id_ref {
                shogun_fusion::block::BlockRef::Event(id) => {
                    let (ts, source, title, frame_id) = ev_by_id
                        .get(&id)
                        .map(|e| (e.ts, e.source.clone(), e.title.clone(), e.frame_id))
                        .unwrap_or((0, String::new(), None, None));
                    out_evidence.push(Evidence {
                        event_id: id,
                        ts,
                        source,
                        title,
                        excerpt: b.text.clone(),
                        frame_id,
                    });
                }
                // State facts passed the confidence gate upstream (High asserted, Medium already
                // "possibly:"-prefixed) — they may stand as facts.
                shogun_fusion::block::BlockRef::State { .. } => out_facts.push(b.text.clone()),
                // Thread/session summaries are extractive or model text with NO confidence gate
                // and no provenance row. Handing them to the prompt naked would give a summary
                // the same authority as a gated fact — label them so the model treats them as
                // context, never as something it may assert.
                shogun_fusion::block::BlockRef::Thread(_) | shogun_fusion::block::BlockRef::Session(_) => {
                    out_facts.push(format!("summary (unverified): {}", b.text));
                }
                // Lesson blocks are built only inside `assemble` (D-5, ContextCache.lesson_lines)
                // — this compression path never constructs one. If one ever appears, treat it
                // like the unverified-summary case: context, never an assertable fact.
                shogun_fusion::block::BlockRef::Lesson(_) => {
                    out_facts.push(format!("summary (unverified): {}", b.text));
                }
            }
        }

        // 計測を記録（best-effort。本文は保存せず query_hash のみ）。
        let compress_ms = started.elapsed().as_millis() as i64;
        self.record_compression_metric(
            query,
            "compressed",
            out.stats.pre_tokens as i64,
            out.stats.post_tokens as i64,
            compress_ms,
            compress_ms,
        );

        (ContextPack { facts: out_facts, evidence: out_evidence, screen_frames }, out.stats, false)
    }

    /// 予算超過時の raw フォールバック。fact は文字列版を「ここで」1 回だけ読む（compressed 経路の
    /// ref 版とは別・二重ロード回避、finding #2）。AB を対称化するため "raw" 計測も best-effort で
    /// 記録する。返り値は `(pack, default_stats, true)`。
    fn raw_fallback(
        &self,
        query: &str,
        evidence: Vec<Evidence>,
        screen_frames: Vec<ScreenFrameRef>,
        started: std::time::Instant,
    ) -> (ContextPack, shogun_fusion::compress::CompressionStats, bool) {
        use shogun_fusion::budget::TokenEstimator;
        let est = shogun_fusion::budget::HeuristicEstimator::default();
        // fact は文字列版を「ここで」1 回だけ読む（ref 版は読まない・二重ロード回避）。
        let facts = self.inline_memory(FACT_LIMIT);
        // pre==post（圧縮していない）。fact テキストと evidence 抜粋のトークン量を best-effort で
        // 見積もる（block 化と同じ est.count なので概ね一致）。
        let pre_tokens: usize = facts
            .iter()
            .map(|f| est.count(f))
            .chain(evidence.iter().map(|e| est.count(&e.excerpt)))
            .sum();
        let elapsed_ms = started.elapsed().as_millis() as i64;
        self.record_compression_metric(
            query,
            "raw",
            pre_tokens as i64,
            pre_tokens as i64,
            elapsed_ms,
            elapsed_ms,
        );
        (ContextPack { facts, evidence, screen_frames }, shogun_fusion::compress::CompressionStats::default(), true)
    }

    /// 解決済みスレッドの保存済み要約を ThreadSummary ブロックにする。要約は raw ターンより
    /// 短くトークン効率が高いので、高 relevance を与えると予算逼迫時に raw を押しのけて残る
    /// （設計 §3.3/§3.4 の差し替えレバー）。summary 未設定のスレッドはスキップ。
    fn thread_summaries_to_blocks(
        &self,
        thread_keys: &[String],
        est: &dyn TokenEstimator,
    ) -> Vec<ContextBlock> {
        thread_keys
            .iter()
            .filter_map(|tk| {
                self.thread_summary(tk).map(|s| {
                    ContextBlock::new(
                        BlockRef::Thread(tk.clone()),
                        SourceKind::ThreadSummary,
                        s,
                        // 参照先として解決済み＝関連度は高い。confidence は要約＝1.0。
                        THREAD_SUMMARY_SCORE,
                        est,
                    )
                })
            })
            .collect()
    }

    /// 圧縮計測を 1 行記録する。best-effort（失敗は握りつぶし、作業を止めない）。
    /// query は xxh64 化して保存（本文は保存しない、テレメトリ規約 G8）。ハッシュは capture /
    /// traceability と同じ twox-hash（seed 0）で、下位 16 桁の lower-hex に揃える。
    pub fn record_compression_metric(
        &self,
        query: &str,
        path: &str,
        pre_tokens: i64,
        post_tokens: i64,
        compress_ms: i64,
        assemble_ms: i64,
    ) {
        let query_hash = Self::content_hash(query);
        let _ = self.with_conn("compression_metrics.insert", |conn| {
            shogun_memory::compression_metrics::insert(
                conn,
                &shogun_memory::compression_metrics::MetricRow {
                    ts: self.now_ms(),
                    query_hash,
                    path: path.to_string(),
                    pre_tokens,
                    post_tokens,
                    compress_ms,
                    assemble_ms,
                },
            )
        });
    }

    /// A traceability sink that writes through this same handle (the LLM egress records here).
    pub fn traceability_sink(&self) -> DbTraceabilitySink {
        DbTraceabilitySink::new(self.conn.clone(), self.clock.clone())
    }

    /// Execute a confirmed L3 send (WP4.3, §6.14): perform the send over `transport` and, on a
    /// successful egress, persist a traceability row through this handle — no send reaches the wire
    /// without a trace (invariant 3 / FR-TR-03). A failed send traces nothing (nothing left the
    /// device). The DB write is the same digest-only row every egress records (no body text, G8).
    #[cfg(feature = "daemon-server")]
    pub fn execute_confirmed_send<T: crate::send_exec::SendTransport + ?Sized>(
        &self,
        confirmed: &shogun_agents::approval::ConfirmedSend,
        transport: &T,
    ) -> crate::send_exec::SendExecOutcome {
        crate::send_exec::execute_send(confirmed, transport, &self.traceability_sink())
    }

    /// Read traceability rows for the viewer (FR-TR-02). Empty on any read failure.
    pub fn trace_rows(&self, filter: &Filter) -> Vec<TraceRow> {
        self.with_conn("traceability.list", |c| shogun_memory::traceability::list(c, filter))
            .unwrap_or_default()
    }

    /// The current time via the injected clock.
    pub fn now_ms(&self) -> i64 {
        (self.clock)()
    }

    /// Export all user data as JSON (FR-SET-07). Local only — never a network send. `None` on a
    /// read failure.
    pub fn export_json(&self) -> Option<String> {
        self.with_conn("maintenance.export_json", shogun_memory::maintenance::export_json).ok()
    }

    /// Delete all user data, keeping the schema (FR-SET-07). Returns the per-table deletion report,
    /// or `None` on failure (the transaction leaves the DB untouched).
    pub fn delete_all(&self) -> Option<shogun_memory::maintenance::DeleteReport> {
        self.with_conn_mut("maintenance.delete_all", shogun_memory::maintenance::delete_all).ok()
    }

    /// Delete user data at or after `cutoff_ts` (unix ms), sweeping orphaned state (FR-SET-07 /
    /// #28). `None` on failure (the transaction leaves the DB untouched).
    pub fn delete_since(&self, cutoff_ts: i64) -> Option<shogun_memory::maintenance::DeleteReport> {
        self.with_conn_mut("maintenance.delete_since", |c| {
            shogun_memory::maintenance::delete_since(c, cutoff_ts)
        })
        .ok()
    }

    // -------------------------------------------------------------- state writes (deliberate)
    // Unlike capture, state writes are low-frequency and deliberate (Dream Cycle consolidation,
    // API propose). They return the new id or `None` on failure so the caller (e.g. a Dream Cycle
    // job) can mark itself failed rather than silently continuing.

    /// Insert a person with provenance (FR-ST-02).
    pub fn insert_person(&self, p: &NewPerson<'_>, provenance: &[Provenance]) -> Option<i64> {
        self.with_conn_mut("state.insert_person", |c| state::insert_person(c, p, provenance)).ok()
    }

    /// Insert a project with provenance.
    pub fn insert_project(&self, p: &NewProject<'_>, provenance: &[Provenance]) -> Option<i64> {
        self.with_conn_mut("state.insert_project", |c| state::insert_project(c, p, provenance)).ok()
    }

    /// Insert a commitment with provenance.
    pub fn insert_commitment(&self, c: &NewCommitment<'_>, provenance: &[Provenance]) -> Option<i64> {
        self.with_conn_mut("state.insert_commitment", |conn| {
            state::insert_commitment(conn, c, provenance)
        })
        .ok()
    }

    /// Insert an open loop with provenance.
    pub fn insert_open_loop(&self, l: &NewOpenLoop<'_>, provenance: &[Provenance]) -> Option<i64> {
        self.with_conn_mut("state.insert_open_loop", |c| state::insert_open_loop(c, l, provenance)).ok()
    }

    // -------------------------------------------------------------- Dream Cycle job effects
    // Concrete effects the nightly cycle drives through the `DreamJobRunner` seam (dreamcycle::jobs).
    // Each swallows storage errors into a safe default so a hiccup fails the *job* (leaving the cycle
    // resumable) rather than crashing the daemon.

    /// Events in `[from_ts, to_ts)` — the window a Consolidation job classifies (FR-DC-03).
    pub fn events_in_range(&self, from_ts: i64, to_ts: i64) -> Vec<event_log::EventText> {
        self.with_conn("events.in_range", |c| event_log::events_in_range(c, from_ts, to_ts))
            .unwrap_or_default()
    }

    /// The same window split by destination: `cloud` may go to the Batch lane, `local_only`
    /// (meeting text) is classified on-device (A-2, `docs/meeting-text-on-the-search-spine.md`).
    pub fn events_in_range_partitioned(
        &self,
        from_ts: i64,
        to_ts: i64,
    ) -> event_log::PartitionedEvents {
        self.with_conn("events.in_range_partitioned", |c| {
            event_log::events_in_range_partitioned(c, from_ts, to_ts)
        })
        .unwrap_or_default()
    }

    /// Threads whose last activity is in `[from_ts, to_ts]` — the window a Compression job
    /// summarises (Issue #63). Empty on a lock/read failure so a hiccup fails the job (leaving the
    /// cycle resumable) rather than crashing the daemon.
    pub fn active_threads_between(&self, from_ts: i64, to_ts: i64) -> Vec<shogun_memory::thread::ThreadRow> {
        self.with_conn("thread.active_between", |c| shogun_memory::thread::active_between(c, from_ts, to_ts))
            .unwrap_or_default()
    }

    /// Every event body in one thread, oldest first — the material the Compression summariser reads
    /// (Issue #63). Empty on a lock/read failure.
    pub fn thread_event_texts(&self, thread_key: &str) -> Vec<event_log::EventText> {
        self.with_conn("thread.event_texts", |c| shogun_memory::thread::event_texts(c, thread_key))
            .unwrap_or_default()
    }

    /// Write a thread's day-summary (Issue #63). Best-effort: a lock/write failure is swallowed so
    /// a hiccup fails the job, not the daemon. Uses the daemon clock for `updated_at`.
    pub fn set_thread_summary(&self, thread_key: &str, summary: &str) {
        let now = self.now_ms();
        let _ = self.with_conn("thread.set_summary", |c| {
            shogun_memory::thread::set_summary(c, thread_key, summary, now)
        });
    }

    /// Read back a thread's summary (`None` when unset, absent, or on a read failure) — the
    /// Compression job's effect is verified through this, since `ThreadRow` does not carry it.
    pub fn thread_summary(&self, thread_key: &str) -> Option<String> {
        self.with_conn("thread.get_summary", |c| shogun_memory::thread::get_summary(c, thread_key))
            .ok()
            .flatten()
    }

    /// Sessions whose `started_at` is in `[from_ts, to_ts]` — the window a Compression job
    /// summarises (Issue #63), the interval analogue of [`Self::active_threads_between`]. Empty on
    /// a lock/read failure so a hiccup fails the job (leaving the cycle resumable) rather than
    /// crashing the daemon.
    pub fn active_sessions_between(&self, from_ts: i64, to_ts: i64) -> Vec<i64> {
        self.with_conn("session.active_between", |c| {
            shogun_memory::session::active_between(c, from_ts, to_ts)
        })
        .unwrap_or_default()
    }

    /// Every event body attached to one session, oldest first — the material the Compression
    /// summariser reads (Issue #63). Empty on a lock/read failure.
    pub fn session_event_texts(&self, session_id: i64) -> Vec<event_log::EventText> {
        self.with_conn("session.event_texts", |c| shogun_memory::session::event_texts(c, session_id))
            .unwrap_or_default()
    }

    /// Write a session's day-summary (Issue #63). Best-effort: a lock/write failure is swallowed so
    /// a hiccup fails the job, not the daemon. Uses the daemon clock for `updated_at`.
    pub fn set_session_summary(&self, session_id: i64, summary: &str) {
        let now = self.now_ms();
        let _ = self.with_conn("session.set_summary", |c| {
            shogun_memory::session::set_summary(c, session_id, summary, now)
        });
    }

    /// Read back a session's summary (`None` when unset, absent, or on a read failure).
    pub fn session_summary(&self, session_id: i64) -> Option<String> {
        self.with_conn("session.get_summary", |c| shogun_memory::session::get_summary(c, session_id))
            .ok()
            .flatten()
    }

    /// The DISTINCT sessions owning the given events — the query-time consume path (Issue #63).
    /// Empty on a lock/read failure or empty input.
    pub fn session_ids_for_events(&self, event_ids: &[i64]) -> Vec<i64> {
        self.with_conn("session.session_ids_for_events", |c| shogun_memory::session::session_ids_for_events(c, event_ids))
            .unwrap_or_default()
    }

    /// The saved summaries of the given sessions in ONE query (Issue #63) — `(id, thread_key,
    /// summary)` for the sessions that have a non-null summary, ordered by id. Replaces the
    /// per-session [`Self::session_summary`] N+1 on the consume path. Empty on a lock/read failure
    /// or empty input.
    pub fn session_summaries_for(&self, ids: &[i64]) -> Vec<(i64, String, Option<String>)> {
        self.with_conn("session.summaries_for_sessions", |c| shogun_memory::session::summaries_for_sessions(c, ids))
            .unwrap_or_default()
    }

    /// Descriptions already present in `commitments` + `open_loops`, for consolidation dedup — so a
    /// re-run over the same range (crash-resume, FR-DC-04) doesn't add the same candidate twice.
    /// Distinct hours in `[from_ts, to_ts)` that produced at least one event — the Coverage
    /// numerator (spec §D2). Zero on a read failure so a locked DB degrades to "nothing seen"
    /// rather than taking the window down.
    pub fn hours_covered(&self, from_ts: i64, to_ts: i64) -> i64 {
        self.with_conn("event_log.hours_covered", |c| event_log::hours_covered(c, from_ts, to_ts))
            .unwrap_or(0)
    }

    /// Events recorded in `[from_ts, to_ts)` — the first number in the Yield funnel.
    pub fn events_count(&self, from_ts: i64, to_ts: i64) -> i64 {
        self.with_conn("event_log.count_in_range", |c| event_log::count_in_range(c, from_ts, to_ts))
            .unwrap_or(0)
    }

    pub fn existing_state_descriptions(&self) -> std::collections::HashSet<String> {
        self.with_conn("state.existing_descriptions", |c| {
            let mut set = std::collections::HashSet::new();
            set.extend(state::list_commitments(c)?.into_iter().map(|r| r.description));
            set.extend(state::list_open_loops(c)?.into_iter().map(|r| r.description));
            Ok::<_, rusqlite::Error>(set)
        })
        .unwrap_or_default()
    }

    /// Persist extracted candidates linked to `event_id` (FR-ST-02). Returns the new row ids.
    pub fn persist_candidates(&self, event_id: i64, candidates: &[shogun_memory::extract::Candidate]) -> Vec<i64> {
        let now = self.now_ms();
        self.with_conn_mut("extract.persist_candidates", |c| {
            shogun_memory::extract::persist_candidates(c, event_id, candidates, now, now)
        })
        .unwrap_or_default()
    }

    /// Recompute overdue status + open-loop staleness from `now` (FR-ST-21). Returns
    /// `(commitments_flagged, loops_touched)`; `(0,0)` on a lock/write failure.
    pub fn recompute_overdue_and_staleness(&self, now_ms: i64) -> (usize, usize) {
        self.with_conn_mut("recompute.recompute_overdue_and_staleness", |g| shogun_memory::recompute::recompute_overdue_and_staleness(g, now_ms))
            .unwrap_or((0, 0))
    }

    /// Age-decay state-row confidence (FR-ST-21). Returns the number of rows changed.
    /// Raise confidence for state rows with several independent evidence events
    /// ([`shogun_memory::recompute::corroborate`]). Part of the local maintenance pass.
    pub fn corroborate(&self) -> usize {
        self.with_conn_mut("recompute.corroborate", shogun_memory::recompute::corroborate)
            .unwrap_or(0)
    }

    /// Record a person identity seen in an event, merging it into the right person
    /// ([`shogun_memory::identity::observe`]).
    ///
    /// The rule this enforces is do-not-mis-merge: only an exact channel identity (same address,
    /// or same handle on the same platform) merges automatically. A name collision is kept
    /// separate and reported, because fusing two real people is disruptive to undo while a missed
    /// merge is easy to fix later.
    ///
    /// Nothing calls this automatically yet — the connectors are what will supply senders and
    /// participants, and they are not live. It is exercised by tests and reachable from the
    /// Memory API in the meantime.
    pub fn observe_identity(
        &self,
        incoming: &shogun_memory::identity::Identity,
        seen_name: Option<&str>,
        event_id: i64,
    ) -> Option<shogun_memory::identity::Observed> {
        let now = self.now_ms();
        self.with_conn_mut("identity.observe", |c| {
            shogun_memory::identity::observe(c, incoming, seen_name, event_id, now)
        })
        .ok()
    }

    /// The maintenance that needs no model call.
    ///
    /// The full Dream Cycle also classifies the day's events through the Batch lane, which needs
    /// the Select KK key (invariant 5) — that half is not wired yet. These passes are the part
    /// that can run today, and they matter on their own: without them a locally-extracted
    /// commitment stays below the Low/Medium boundary forever and the user never sees anything
    /// from their own captured work, and nothing ever goes overdue.
    ///
    /// Order matters. Decay first (it reads `last_evidence_at`), then corroborate (which may lift
    /// a decayed row back up on the strength of repeated evidence), then recompute overdue and
    /// staleness so the surfaced state is current.
    pub fn run_local_maintenance(&self, now_ms: i64, half_life_ms: i64) -> LocalMaintenance {
        // Corroborate first: it raises the *base*, and the decay pass that follows is what turns a
        // base into the visible, age-correct confidence. The other order would leave a newly
        // corroborated row showing last pass's number until the next hour.
        let corroborated = self.corroborate();
        let decayed = self.decay_confidence(now_ms, half_life_ms);
        // The detailed pass reports WHICH commitments flipped open→overdue right now, so the
        // caller can notify each exactly once (C-3; the flip itself is the dedup watermark).
        let (newly_overdue, stale) = self
            .with_conn_mut("recompute.overdue_and_staleness_detailed", |c| {
                shogun_memory::recompute::recompute_overdue_and_staleness_detailed(c, now_ms)
            })
            .unwrap_or_default();
        let overdue = newly_overdue.len();
        LocalMaintenance { decayed, corroborated, overdue, stale, newly_overdue }
    }

    pub fn decay_confidence(&self, now_ms: i64, half_life_ms: i64) -> usize {
        self.with_conn_mut("recompute.decay_confidence", |g| shogun_memory::recompute::decay_confidence(g, now_ms, half_life_ms))
            .unwrap_or(0)
    }

    /// The high-water mark of already-consolidated events (max `input_to_ts` of completed
    /// consolidations) — the scheduler's next window starts here (FR-DC-04). `None` before any cycle.
    pub fn last_consolidated_to(&self) -> Option<i64> {
        self.with_conn("jobs.last_consolidated_to", shogun_memory::jobs::last_consolidated_to)
            .ok()
            .flatten()
    }

    /// Demote Warm embeddings older than `cutoff_ms` to the int8 Cold tier (FR-MEM-04). Returns the
    /// number moved.
    pub fn demote_cold(&self, cutoff_ms: i64) -> usize {
        self.with_conn_mut("cold.demote_older_than", |g| shogun_memory::cold::demote_older_than(g, cutoff_ms))
            .unwrap_or(0)
    }

    // -------------------------------------------------------------- Context Fusion → Notch actions
    // The product core (§6.1, "press a button, work is done"): map the current state rows into
    // Fusion input, assemble the ranked action candidates for the focused screen, and hand them to
    // the Notch panel. Pure ranking + the confidence gate live in shogun-fusion (FR-ST-20); the
    // daemon supplies the rows and scores screen relevance.

    /// Assemble the context-action candidates for the current screen (§6.1, SLO-02: ≤4 actions).
    /// Reads commitments / open loops / people / projects, maps them to Fusion candidates (scoring
    /// relevance by overlap with the focused screen), and returns the confidence-gated, ranked
    /// [`ContextCache`]. Low-confidence state never becomes an action (FR-ST-20).
    pub fn context_actions(
        &self,
        screen: shogun_fusion::assemble::ScreenContext,
        intent_hint: Option<String>,
    ) -> shogun_fusion::assemble::ContextCache {
        use shogun_fusion::assemble::{assemble, Intent, StateCandidate, StateKind};

        let mut states: Vec<StateCandidate> = Vec::new();
        let rel = |subject: &str, summary: &str| screen_relevance(&screen, subject, summary);

        // Resolved rows must not compete for the four action slots: a commitment the user ticked
        // off, or a loop they closed, is finished work — proposing it again is the panel telling
        // the user their click didn't count (every other read path filters the same way).
        for c in self.commitment_rows() {
            if !matches!(c.status.as_str(), "open" | "overdue") {
                continue;
            }
            let summary = c.description.clone();
            states.push(StateCandidate {
                kind: StateKind::CommitmentMine,
                relevance: rel(&summary, &summary),
                subject: summary.clone(),
                summary,
                confidence: c.confidence,
            });
        }
        for l in self.open_loop_rows() {
            if l.status != "open" {
                continue;
            }
            let summary = l.description.clone();
            states.push(StateCandidate {
                kind: open_loop_state_kind(&l.kind),
                relevance: rel(&summary, &summary),
                subject: summary.clone(),
                summary,
                confidence: l.confidence,
            });
        }
        for p in self.people() {
            states.push(StateCandidate {
                kind: StateKind::Person,
                relevance: rel(&p.display_name, &p.display_name),
                subject: p.display_name.clone(),
                summary: p.display_name,
                confidence: p.confidence,
            });
        }
        for p in self.projects() {
            states.push(StateCandidate {
                kind: StateKind::Project,
                relevance: rel(&p.name, &p.name),
                subject: p.name.clone(),
                summary: p.name,
                confidence: p.confidence,
            });
        }

        // D-5: scope-matched learned lessons (this app + global), top-k, ride along for the
        // generation prompt. Content only — assemble() never lets them touch levels.
        let lessons = self.lessons_for_screen(&screen.app_bundle_id, LESSON_TOP_K);
        assemble(screen, &states, "", &Intent { hint: intent_hint }, &lessons)
    }

    // -------------------------------------------------------------- state reads → Fusion supply
    // The daemon reads state rows and maps them into Fusion's input types, so Context Fusion and
    // the Morning Brief run on real DB data. The confidence gate lives in Fusion (FR-ST-20); the
    // daemon only supplies the rows.

    /// Commitments as Fusion/Brief input. `overdue` is derived from the status or a past due time.
    /// Resolved rows (`done`/`cancelled`) are excluded so a commitment the user marked done from
    /// the panel stops feeding drafts, chat memory, counts, and the Morning Brief — not just the
    /// panel view.
    pub fn commitments_due(&self, now_ms: i64) -> Vec<CommitmentDue> {
        self.try_commitments_due(now_ms).unwrap_or_default()
    }

    /// [`Self::commitments_due`] with the failure kept (issue #121). A caller that shows a count
    /// or feeds an answer must not read a dead store as "you owe nobody anything".
    pub fn try_commitments_due(&self, now_ms: i64) -> MemoryResult<Vec<CommitmentDue>> {
        let rows = self.try_commitment_rows()?;
        Ok(rows
            .into_iter()
            .filter(|r| r.status != "done" && r.status != "cancelled")
            .map(|r| CommitmentDue {
                overdue: r.status == "overdue" || r.due_at.is_some_and(|d| d < now_ms),
                description: r.description,
                due_at_ms: r.due_at,
                confidence: r.confidence,
                provenance_event_id: r.first_event_id.unwrap_or(0),
            })
            .collect())
    }

    /// People rows (Memory API `state.people.list`).
    pub fn people(&self) -> Vec<state::PersonRow> {
        self.try_people().unwrap_or_default()
    }

    /// [`Self::people`] with the failure kept (issue #121).
    pub fn try_people(&self) -> MemoryResult<Vec<state::PersonRow>> {
        self.with_conn("state.people.list", state::list_people)
    }

    /// One person by id (`state.people.get`).
    pub fn person(&self, id: i64) -> Option<state::PersonRow> {
        self.with_conn("state.people.get", |c| state::get_person(c, id)).ok().flatten()
    }

    /// Project rows (`state.projects.list`).
    pub fn projects(&self) -> Vec<state::ProjectRow> {
        self.try_projects().unwrap_or_default()
    }

    /// [`Self::projects`] with the failure kept (issue #121).
    pub fn try_projects(&self) -> MemoryResult<Vec<state::ProjectRow>> {
        self.with_conn("state.projects.list", state::list_projects)
    }

    /// One project by id (`state.projects.get`).
    pub fn project(&self, id: i64) -> Option<state::ProjectRow> {
        self.with_conn("state.projects.get", |c| state::get_project(c, id)).ok().flatten()
    }

    /// One commitment by id (`state.commitments.get`).
    pub fn commitment(&self, id: i64) -> Option<state::CommitmentRow> {
        self.with_conn("state.commitments.get", |c| state::get_commitment(c, id)).ok().flatten()
    }

    /// All commitment rows with ids (panel list — the UI needs the id to resolve a row).
    pub fn commitment_rows(&self) -> Vec<state::CommitmentRow> {
        self.try_commitment_rows().unwrap_or_default()
    }

    /// [`Self::commitment_rows`] with the failure kept (issue #121).
    pub fn try_commitment_rows(&self) -> MemoryResult<Vec<state::CommitmentRow>> {
        self.with_conn("state.commitments.list", state::list_commitments)
    }

    /// All open-loop rows with ids (panel list).
    pub fn open_loop_rows(&self) -> Vec<state::OpenLoopRow> {
        self.try_open_loop_rows().unwrap_or_default()
    }

    /// [`Self::open_loop_rows`] with the failure kept (issue #121).
    pub fn try_open_loop_rows(&self) -> MemoryResult<Vec<state::OpenLoopRow>> {
        self.with_conn("state.open_loops.list", state::list_open_loops)
    }

    /// Mark a commitment done (user resolved it from the panel). `true` if a row changed.
    pub fn resolve_commitment(&self, id: i64) -> bool {
        let now = self.now_ms();
        self.with_conn("state.commitments.resolve", |c| {
            state::set_commitment_status(c, id, state::CommitmentStatus::Done, now)
        })
        .is_ok_and(|n| n > 0)
    }

    /// Close an open loop (user resolved it from the panel). `true` if a row changed.
    pub fn resolve_open_loop(&self, id: i64) -> bool {
        let now = self.now_ms();
        self.with_conn("state.open_loops.close", |c| state::close_open_loop(c, id, now))
            .is_ok_and(|n| n > 0)
    }

    /// Delete all extracted state (commitments + open loops + their provenance). Event log,
    /// people, and projects are untouched. `true` on success.
    pub fn clear_state(&self) -> bool {
        self.with_conn_mut("state.clear", state::clear_state).is_ok()
    }

    /// One open loop by id (`state.open_loops.get`).
    pub fn open_loop(&self, id: i64) -> Option<state::OpenLoopRow> {
        self.with_conn("state.open_loops.get", |c| state::get_open_loop(c, id)).ok().flatten()
    }

    /// Hybrid/FTS search over the event log (`memory.search`). Empty on an empty query or failure.
    pub fn search(&self, query: &str, limit: usize) -> Vec<shogun_memory::search::SearchHit> {
        self.try_search(query, limit).unwrap_or_default()
    }

    /// [`Self::search`] with the failure kept (issue #121).
    ///
    /// This is the distinction the search box exists on: "nothing matched" is an answer the user
    /// can act on, and "the store did not answer" is not. Handing back an empty list for both
    /// tells them their memory is empty when it is merely unreachable.
    pub fn try_search(
        &self,
        query: &str,
        limit: usize,
    ) -> MemoryResult<Vec<shogun_memory::search::SearchHit>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        // With a model loaded this is hybrid (lexical + semantic, fused by RRF); without one it
        // degrades to lexical, which still answers — it just cannot match a paraphrase.
        let query_vec = self
            .embedder
            .as_deref()
            .and_then(|e| e.embed_query(query).ok());
        // Warm window first, widening to the full history only when that comes back thin — an
        // unbounded bm25 ranking costs in proportion to how much of the log matches, which on
        // device already reached the 500ms search budget at 40k events.
        let now = self.now_ms();
        self.with_conn("memory.search", |c| {
            shogun_memory::search::search_warm_first(c, query, query_vec.as_deref(), now, limit)
        })
    }

    /// FTS search scoped to one event-log `source` (visual recall uses `screen_ocr`).
    pub fn search_source(
        &self,
        query: &str,
        source: &str,
        limit: usize,
    ) -> Vec<shogun_memory::search::SearchHit> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        self.with_conn("memory.search_source", |c| {
            let ids = shogun_memory::search::fts_search_source(c, query, source, limit)?;
            let ranked: Vec<(i64, f64)> =
                ids.into_iter().enumerate().map(|(i, id)| (id, 1.0 / (i as f64 + 1.0))).collect();
            shogun_memory::search::hydrate(c, &ranked)
        })
        .unwrap_or_default()
    }

    /// Recent on-device screen OCR previews for settings / Full UI (text only, no pixels).
    pub fn screen_ocr_previews(
        &self,
        limit: usize,
        excerpt_chars: usize,
    ) -> Vec<shogun_memory::event_log::RecentEventPreview> {
        self.with_conn("screen_ocr.previews", |c| {
            shogun_memory::event_log::recent_previews_by_source(c, "screen_ocr", limit, excerpt_chars)
        })
        .unwrap_or_default()
    }

    /// How many `screen_ocr` events landed in the last 24 hours.
    pub fn screen_ocr_count_24h(&self) -> i64 {
        let now = self.now_ms();
        let since = now - 24 * 60 * 60 * 1000;
        self.with_conn("screen_ocr.count_24h", |c| {
            shogun_memory::event_log::count_source_in_range(c, "screen_ocr", since, now)
        })
        .unwrap_or(0)
    }

    /// Persist a compressed JPEG from visual-recall OCR, linked to its `screen_ocr` event.
    ///
    /// Explicit exception to invariant 2 (user decision 2026-08-02): frames are local-only,
    /// encrypted at rest with the memory DB, and purged after 72 h — not audio, not forever.
    #[allow(clippy::too_many_arguments)] // frame metadata is one row; a params struct adds nothing
    pub fn store_screen_frame(
        &self,
        event_id: i64,
        bundle_id: Option<&str>,
        window_title: Option<&str>,
        display_id: Option<i64>,
        width: u32,
        height: u32,
        jpeg: &[u8],
    ) -> Option<i64> {
        let now = self.now_ms();
        self.with_conn("screen_frames.insert", |c| {
                shogun_memory::screen_frames::insert(
                    c,
                    &shogun_memory::screen_frames::NewFrame {
                        created_at_ms: now,
                        event_id,
                        app_bundle_id: bundle_id,
                        window_title,
                        display_id,
                        width,
                        height,
                        jpeg,
                    },
                )
            })
            .ok()
    }

    /// Sweep the visual-recall frame cache: expire past the 72-hour window, then evict oldest-first
    /// until the cache is back under its byte ceiling (`retention::Policy::frames`).
    ///
    /// Age alone bounds nothing — 72 hours of a busy screen is not a fixed size — so a heavy few
    /// days could grow the memory DB without limit while every frame was still "within the window
    /// we promised". The budget half is what makes the cache's footprint answerable.
    pub fn purge_screen_frames(&self) -> Result<usize, String> {
        use shogun_memory::retention::Policy;
        let now = self.now_ms();
        self.with_conn_mut_reported("screen_frames.purge_expired", |conn| {
            let items = shogun_memory::screen_frames::retention_items(conn)
                .map_err(|e| format!("read frame retention items: {e}"))?;
            let sweep = Policy::frames().sweep(&items, now);
            shogun_memory::screen_frames::delete_ids(conn, &sweep.all())
                .map_err(|e| format!("purge screen frames: {e}"))
        })

    }

    /// Drop auto-capture frames only (passive OCR). User-initiated shots are kept.
    pub fn purge_auto_screen_frames(&self) -> Result<usize, String> {
        self.with_conn_reported("screen_frames.purge_auto", |conn| {
            shogun_memory::screen_frames::purge_auto_only(conn)
                .map_err(|e| format!("purge automatic screen frames: {e}"))
        })

    }

    /// Delete one stored frame and its linked OCR event when no other frame references it.
    pub fn delete_screen_frame(&self, frame_id: i64) -> Result<bool, String> {
        self.with_conn_mut_reported("screen_frames.delete_by_id", |conn| {
            shogun_memory::screen_frames::delete_by_id(conn, frame_id)
                .map_err(|e| format!("delete screen frame: {e}"))
        })

    }

    /// Persist refreshed OCR text for a screen frame's linked event (visual recall re-scan).
    pub fn update_event_ocr_text(&self, event_id: i64, text: &str) -> Result<bool, String> {
        let hash = Self::content_hash(text);
        self.with_conn_reported("event_log.update_ocr_text", |conn| {
            shogun_memory::event_log::update_content_and_hash(conn, event_id, text, &hash)
                .map_err(|e| format!("update event OCR text: {e}"))
        })

    }

    /// List frames in the retention window for UI timeline (newest first).
    pub fn list_screen_frames(&self, limit: usize) -> Vec<shogun_memory::screen_frames::FrameSummary> {
        let now = self.now_ms();
        let from = now - shogun_memory::screen_frames::RETENTION_MS;
        self.screen_frames_in_range(from, now, limit)
    }

    /// Frame-cache stats for settings (count / oldest / bytes — no pixels).
    pub fn screen_frame_stats(&self) -> shogun_memory::screen_frames::FrameStats {
        self.with_conn("screen_frames.stats", shogun_memory::screen_frames::stats)
            .unwrap_or_default()
    }

    /// Search stored screen frames for a visual-recall question (metadata + OCR excerpt).
    pub fn search_screen_frames(
        &self,
        query: &str,
        limit: usize,
        excerpt_chars: usize,
    ) -> Vec<ScreenFrameRef> {
        self.recall_screen_frames(query, limit, excerpt_chars)
    }

    /// Search stored frames in an explicit time window (Memory API / MCP).
    pub fn search_screen_frames_window(
        &self,
        query: &str,
        from_ms: i64,
        to_ms: i64,
        limit: usize,
        excerpt_chars: usize,
    ) -> Vec<ScreenFrameRef> {
        self.with_conn("screen_frames.search_for_recall", |c| {
            shogun_memory::screen_frames::search_for_recall(c, query, from_ms, to_ms, limit, excerpt_chars)
        })
        .unwrap_or_default()
        .into_iter()
            .map(|h| ScreenFrameRef {
                frame_id: h.frame_id,
                event_id: h.event_id,
                ts: h.ts,
                app_bundle_id: h.app_bundle_id,
                window_title: h.window_title,
                width: h.width,
                height: h.height,
                ocr_excerpt: h.ocr_excerpt,
                needs_rescan: h.needs_rescan,
                source: h.source,
            })
            .collect()
    }

    /// Count stored frames whose `created_at_ms` lies in `[from_ms, to_ms]`.
    pub fn screen_frames_count_in_range(&self, from_ms: i64, to_ms: i64) -> i64 {
        self.with_conn("screen_frames.count_in_range", |c| {
            c.query_row(
                "SELECT count(*) FROM screen_frames WHERE created_at_ms >= ?1 AND created_at_ms <= ?2",
                rusqlite::params![from_ms, to_ms],
                |r| r.get(0),
            )
        })
        .unwrap_or(0)
    }

    /// Latest stored frame metadata (no JPEG bytes).
    pub fn latest_screen_frame_summary(&self) -> Option<shogun_memory::screen_frames::FrameSummary> {
        let now = self.now_ms();
        self.screen_frames_in_range(0, now, 1).into_iter().next()
    }

    fn recall_screen_frames(&self, query: &str, limit: usize, excerpt_chars: usize) -> Vec<ScreenFrameRef> {
        let now = self.now_ms();
        let local_days = local_day_bounds(now);
        let (from_ms, to_ms) =
            shogun_memory::search::visual_recall_window(query, now, local_days);
        self.with_conn("screen_frames.search_for_recall", |c| {
                shogun_memory::screen_frames::search_for_recall(c, query, from_ms, to_ms, limit, excerpt_chars)
            })
            .unwrap_or_default()
            .into_iter()
            .map(|h| ScreenFrameRef {
                frame_id: h.frame_id,
                event_id: h.event_id,
                ts: h.ts,
                app_bundle_id: h.app_bundle_id,
                window_title: h.window_title,
                width: h.width,
                height: h.height,
                ocr_excerpt: h.ocr_excerpt,
                needs_rescan: h.needs_rescan,
                source: h.source,
            })
            .collect()
    }

    fn frame_ids_for_events(&self, event_ids: &[i64]) -> std::collections::HashMap<i64, i64> {
        self.with_conn("screen_frames.frame_ids_for_events", |c| shogun_memory::screen_frames::frame_ids_for_events(c, event_ids))
            .unwrap_or_default()
    }

    /// Fetch frame metadata and OCR without loading the JPEG BLOB.
    pub fn get_screen_frame_summary(
        &self,
        frame_id: i64,
    ) -> Option<shogun_memory::screen_frames::FrameSummary> {
        self.with_conn("screen_frames.get_summary_by_id", |c| shogun_memory::screen_frames::get_summary_by_id(c, frame_id))
            .ok()
            .flatten()
    }

    /// Fetch one stored frame by id (JPEG bytes + metadata). Local-only; never leaves device.
    pub fn get_screen_frame(&self, frame_id: i64) -> Option<shogun_memory::screen_frames::FrameRecord> {
        self.with_conn("screen_frames.get_by_id", |c| shogun_memory::screen_frames::get_by_id(c, frame_id))
            .ok()
            .flatten()
    }

    /// Frames captured in a wall-clock window (newest first).
    pub fn screen_frames_in_range(
        &self,
        from_ms: i64,
        to_ms: i64,
        limit: usize,
    ) -> Vec<shogun_memory::screen_frames::FrameSummary> {
        self.with_conn("screen_frames.list_in_range", |c| shogun_memory::screen_frames::list_in_range(c, from_ms, to_ms, limit))
            .unwrap_or_default()
    }

    /// Open loops as Fusion/Brief input (stalest first; the Brief caps the count). Closed loops
    /// are excluded so resolving one from the panel removes it everywhere (memory, counts, Brief).
    pub fn open_loops(&self) -> Vec<OpenLoopItem> {
        let rows = self.open_loop_rows();
        rows.into_iter()
            .filter(|r| r.status != "closed")
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

    /// Assemble the Evening Wrap (§6.17, FR-EB-01/02): the day's outcome, what's still open,
    /// tomorrow's first items, and today's loose ends — **local aggregation only**, no LLM call
    /// and no egress. The caller supplies the window boundaries (`day_start_ms` = local midnight,
    /// `tomorrow_end_ms` = end of tomorrow) because timezone math belongs to the shell, and the
    /// tomorrow calendar lines because calendar data flows in from the connector lane.
    pub fn evening_wrap(
        &self,
        calendar_tomorrow: Vec<CalendarLine>,
        day_start_ms: i64,
        now_ms: i64,
        tomorrow_end_ms: i64,
    ) -> shogun_fusion::wrap::EveningWrap {
        use shogun_fusion::wrap::{assemble_wrap, WrapOutcome};

        // One lock for every day-window read: five separate `lock()` calls would let the day's
        // counts and the day's lists come from different instants, and the Wrap would then show
        // "3 done" beside a still-open list that already dropped one of them.
        let (outcome, active_commitments, active_loops, opened_today, all_commitments) = self
            .read_conn("state.evening_wrap_window", |c| {
                let decisions =
                    shogun_memory::lessons::decision_counts_since(c, day_start_ms).unwrap_or((0, 0));
                let outcome = WrapOutcome {
                    commitments_done: u32::try_from(
                        state::count_commitments_done_since(c, day_start_ms).unwrap_or(0),
                    )
                    .unwrap_or(0),
                    loops_closed: u32::try_from(
                        state::count_open_loops_closed_since(c, day_start_ms).unwrap_or(0),
                    )
                    .unwrap_or(0),
                    actions_decided: u32::try_from(decisions.0).unwrap_or(0),
                    actions_adopted: u32::try_from(decisions.1).unwrap_or(0),
                };
                (
                    outcome,
                    state::list_commitments_active_since(c, day_start_ms).unwrap_or_default(),
                    state::list_open_loops_active_since(c, day_start_ms).unwrap_or_default(),
                    state::list_open_loops_opened_since(c, day_start_ms).unwrap_or_default(),
                    state::list_commitments(c).unwrap_or_default(),
                )
            })
            .unwrap_or_default();

        let to_due = |r: state::CommitmentRow| CommitmentDue {
            overdue: r.status == "overdue" || r.due_at.is_some_and(|d| d < now_ms),
            description: r.description,
            due_at_ms: r.due_at,
            confidence: r.confidence,
            provenance_event_id: r.first_event_id.unwrap_or(0),
        };
        let to_loop = |r: state::OpenLoopRow| OpenLoopItem {
            description: r.description,
            staleness_days: u32::try_from(r.staleness_days).unwrap_or(0),
            confidence: r.confidence,
            provenance_event_id: r.first_event_id.unwrap_or(0),
        };

        // Tomorrow-due: unresolved commitments with a due time after now and inside tomorrow.
        let tomorrow_commitments: Vec<CommitmentDue> = all_commitments
            .into_iter()
            .filter(|r| {
                r.status != "done"
                    && r.status != "cancelled"
                    && r.due_at.is_some_and(|d| d > now_ms && d <= tomorrow_end_ms)
            })
            .map(to_due)
            .collect();

        assemble_wrap(
            outcome,
            &active_commitments.into_iter().map(to_due).collect::<Vec<_>>(),
            &active_loops.into_iter().map(to_loop).collect::<Vec<_>>(),
            calendar_tomorrow,
            &tomorrow_commitments,
            &opened_today.into_iter().map(to_loop).collect::<Vec<_>>(),
        )
    }

    /// Persist the nightly Morning Brief for `date` (Plan C-1: the Dream Cycle's MorningBrief job
    /// writes it, the morning display reads it). Upsert on the day key — idempotent under a
    /// crash-resume re-run (FR-DC-04). Returns `None` on a lock/write failure so the job can
    /// report the night as failed and stay resumable.
    pub fn save_brief(&self, date: &str, payload_json: &str, generated: bool) -> Option<bool> {
        let now = self.now_ms();
        self.with_conn("briefs.upsert_brief", |c| {
            shogun_memory::briefs::upsert_brief(c, date, payload_json, generated, now)
        })
        .ok()
    }

    /// The persisted brief for `date` (`None` when the nightly job hasn't written one — the caller
    /// falls back to [`Self::local_morning_brief`], FR-MB-04).
    pub fn brief_for(&self, date: &str) -> Option<shogun_memory::briefs::StoredBrief> {
        self.with_conn("briefs.get", |c| shogun_memory::briefs::get_brief(c, date)).ok().flatten()
    }

    /// Where one event came from: its `source` plus, for captured events, the app it was captured
    /// in. This is the daily-summary card's deep-link data (issue #10): the chip label comes from
    /// the source, and a capture event's `app_bundle_id` is the app the chip re-opens. Metadata
    /// only — the event's content never rides along.
    pub fn event_source(&self, event_id: i64) -> Option<(String, Option<String>)> {
        self.with_conn("events.source", |c| {
            c.query_row(
                "SELECT source, app_bundle_id FROM event_log WHERE id = ?1",
                [event_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
        })
        .ok()
        .flatten()
    }

    // -------------------------------------------------------------- L5 lessons (Plan D-4/D-5/D-6)
    // Db wrappers over `shogun_memory::lessons`, next to the brief wrappers: the
    // LessonDistillation Dream job, Context Fusion injection, and the Memory API lessons tools
    // all read/write through these. Feedback text never leaves the DB through any of them —
    // the only wrapper that returns it ([`Self::feedback_after`]) feeds the local distiller.

    /// The distillation watermark: highest `feedback_events.id` already consumed (0 = none).
    pub fn lesson_distill_watermark(&self) -> i64 {
        self.with_conn("lessons.distill_watermark", lessons::distill_watermark).unwrap_or(0)
    }

    /// Advance the distillation watermark (monotonic). Returns false on a write failure so the
    /// job can report the night as failed and stay resumable.
    pub fn set_lesson_distill_watermark(&self, last_processed_feedback_id: i64) -> bool {
        self.with_conn("lessons.set_distill_watermark", |c| {
            lessons::set_distill_watermark(c, last_processed_feedback_id)
        })
        .is_ok()
    }

    /// Unprocessed feedback (id strictly above the watermark), oldest first — the distillation
    /// job's input. Local-only data: the rows carry the user's before/after text, so this must
    /// never feed a log or an egress path.
    pub fn feedback_after(&self, after_id: i64) -> Vec<lessons::FeedbackRow> {
        self.with_conn("lessons.list_feedback_after", |c| lessons::list_feedback_after(c, after_id))
            .unwrap_or_default()
    }

    /// Record one feedback signal (the D-2 approval hooks call this). Returns the new row id.
    pub fn record_feedback(
        &self,
        kind: lessons::FeedbackKind,
        scope: lessons::LessonScope,
        f: &lessons::NewFeedback<'_>,
    ) -> Option<i64> {
        self.with_conn("lessons.record_feedback", |c| lessons::record_feedback(c, kind, scope, f)).ok()
    }

    /// Insert-or-merge a distilled lesson with its evidence ids (provenance mandatory).
    /// `None` on a lock/write failure (the job reports failure without echoing any text).
    pub fn upsert_lesson(&self, candidate: &lessons::LessonCandidate, now_ms: i64) -> Option<i64> {
        self.with_conn_mut("lessons.upsert", |c| {
            lessons::upsert_lesson(c, candidate, &candidate.evidence, now_ms)
        })
        .ok()
    }

    /// Run the lesson lifecycle pass (decay, contradiction, floor, cap) at `now_ms`.
    pub fn decay_lessons(&self, now_ms: i64) -> Option<lessons::LifecycleOutcome> {
        self.with_conn_mut("lessons.decay", |c| lessons::decay_and_deactivate(c, now_ms)).ok()
    }

    /// The lessons eligible for injection right now (active, at/above the Low-band floor,
    /// scope-filtered, strongest first, at most `top_k`) — the Fusion/D-5 supply.
    pub fn active_lessons(
        &self,
        scopes: &[lessons::ScopeFilter<'_>],
        top_k: usize,
    ) -> Vec<lessons::Lesson> {
        self.with_conn("lessons.active_lessons", |c| lessons::active_lessons(c, scopes, top_k))
            .unwrap_or_default()
    }

    /// Every lesson row, sleeping included — the Learned UI / `lessons.list` supply. Instructions
    /// and bookkeeping only; never `feedback_events` text.
    pub fn lessons_all(&self) -> Vec<lessons::Lesson> {
        self.with_conn("lessons.list", lessons::list_lessons).unwrap_or_default()
    }

    /// Flip one lesson's active switch (`lessons.set_active`). `false` when the row is missing or
    /// the write failed.
    pub fn set_lesson_active(&self, lesson_id: i64, active: bool) -> bool {
        let now = self.now_ms();
        self.with_conn("lessons.set_lesson_active", |c| lessons::set_lesson_active(c, lesson_id, active, now))
            .unwrap_or(false)
    }

    /// D-6 counters for `shogun metrics`: (active lessons, feedback events in the last 7 days).
    /// `None` when the DB is unreadable — rendered as `measured:false`, never a fabricated zero.
    pub fn lesson_counters(&self) -> Option<crate::metrics::LessonCounters> {
        const WEEK_MS: i64 = 7 * 24 * 60 * 60 * 1000;
        let now = self.now_ms();
        self.with_conn("lessons.counters", |conn| {
            Ok::<_, rusqlite::Error>(crate::metrics::LessonCounters {
                active_lessons: lessons::count_active_lessons(conn)?,
                feedback_events_last_7d: lessons::count_feedback_since(conn, now - WEEK_MS)?,
            })
        })
        .ok()
    }

    /// The active lessons relevant to the current screen (this app + global scope), mapped into
    /// Fusion's input view — instruction + confidence only (D-5).
    pub fn lessons_for_screen(
        &self,
        app_bundle_id: &str,
        top_k: usize,
    ) -> Vec<shogun_fusion::assemble::LessonInput> {
        use lessons::{LessonScope, ScopeFilter};
        let scopes = [
            ScopeFilter { scope: LessonScope::App, scope_ref: Some(app_bundle_id) },
            ScopeFilter { scope: LessonScope::Global, scope_ref: None },
        ];
        self.active_lessons(&scopes, top_k)
            .into_iter()
            .map(|l| shogun_fusion::assemble::LessonInput {
                id: l.id,
                instruction: l.instruction,
                confidence: l.confidence,
            })
            .collect()
    }

    /// The active lessons mapped into the directive-render view (D-5a): the desktop joins these
    /// into the Shougun.md system-prompt block via
    /// [`crate::user_config::render_directives_with_lessons`]. Unscoped call sites (the standing
    /// prompt has no focused app) take every scope, top-k strongest.
    pub fn learned_lessons(&self, top_k: usize) -> Vec<crate::user_config::LearnedLesson> {
        self.active_lessons(&[], top_k)
            .into_iter()
            .map(|l| crate::user_config::LearnedLesson {
                instruction: l.instruction,
                confidence: l.confidence,
            })
            .collect()
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
        self.read_conn("jobs.upsert", |c| {
                shogun_memory::jobs::upsert(
                    c,
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
        let rows = self.with_conn("jobs.list_by_cycle", |c| shogun_memory::jobs::list_by_cycle(c, cycle_id))
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

    /// The most recent `limit` nights, newest first, reconstructed from the job ledger (FR-DC-04).
    /// This is what survives a relaunch: no run reports itself, the ledger is the record.
    pub fn recent_cycles(&self, limit: usize) -> Vec<crate::dreamcycle::run::CycleOutcome> {
        let cycles = self.with_conn("jobs.recent_cycles", |c| shogun_memory::jobs::recent_cycles(c, limit))
            .unwrap_or_default();

        cycles
            .into_iter()
            .map(|c| {
                let runs: Vec<JobRun> = c
                    .rows
                    .iter()
                    .filter_map(|r| {
                        Some(JobRun {
                            kind: parse_job_kind(&r.kind)?,
                            state: parse_job_state(&r.state)?,
                            input_from_ts: r.input_from_ts,
                            input_to_ts: r.input_to_ts,
                        })
                    })
                    .collect();
                let kind = if runs.iter().any(|r| !DEGRADED_SEQUENCE.contains(&r.kind)) {
                    CycleKind::Full
                } else {
                    CycleKind::Degraded
                };
                crate::dreamcycle::run::CycleOutcome {
                    cycle_id: c.cycle_id,
                    kind,
                    succeeded: crate::dreamcycle::plan::is_complete(kind, &runs),
                    jobs_done: runs.iter().filter(|r| r.state == JobState::Done).count(),
                    jobs_failed: runs.iter().filter(|r| r.state == JobState::Failed).count(),
                    // The window is the cycle's, not a job's — every job of a cycle carries it.
                    input_from_ts: runs.iter().map(|r| r.input_from_ts).min().unwrap_or(0),
                    input_to_ts: runs.iter().map(|r| r.input_to_ts).max().unwrap_or(0),
                    started_at: c.started_at,
                    ended_at: c.ended_at,
                }
            })
            .collect()
    }

    /// The Dream Cycle status for the Full UI (FR-DC-06) and the gate: the last night's outcome, the
    /// failure indicator (FR-DC-05), and whether tonight's full cycle is already done.
    ///
    /// `nights` bounds how far back the indicator looks — it only ever counts the unbroken run of
    /// failures at the front, so a short window is enough and keeps the query cheap.
    pub fn dream_status(&self, tonight_cycle_id: &str, nights: usize) -> crate::dreamcycle::run::DreamStatus {
        let recent = self.recent_cycles(nights.max(1));
        // Only full cycles carry Batch work, so only they can fail the Batch lane. A degraded
        // catch-up night is not evidence either way and must not reset an amber indicator.
        let outcomes: Vec<bool> = recent
            .iter()
            .filter(|c| c.kind == CycleKind::Full)
            .map(|c| c.succeeded)
            .collect();
        let full_run_done_today = recent
            .iter()
            .any(|c| c.cycle_id == tonight_cycle_id && c.kind == CycleKind::Full && c.succeeded);

        let last = recent.into_iter().next();
        let (events_processed, state_changes, chunks_sent) = match &last {
            Some(c) => self
                .read_conn("dream.summary_counts", |conn| {
                    (
                        event_log::count_in_range(conn, c.input_from_ts, c.input_to_ts).unwrap_or(0),
                        state::count_changed_since(conn, c.started_at).unwrap_or(0),
                        shogun_memory::traceability::count_since(conn, c.started_at).unwrap_or(0),
                    )
                })
                .unwrap_or((0, 0, 0)),
            None => (0, 0, 0),
        };

        crate::dreamcycle::run::DreamStatus {
            last,
            indicator: crate::dreamcycle::health::indicator(
                crate::dreamcycle::health::consecutive_failures(&outcomes),
            ),
            events_processed,
            state_changes,
            chunks_sent,
            full_run_done_today,
        }
    }

    /// Assemble the Dream Cycle run summary (FR-DC-06) from DB deltas. Everything a run changed —
    /// state rows and traceability chunks — carries a timestamp at or after `run_started_ms`, so a
    /// single "since run start" count captures the run's real effect without a before/after
    /// snapshot. `events_processed` is the size of the input window the cycle consolidated. The
    /// summary is what the Full UI renders. Zeroed counts on a lock failure (never panics).
    pub fn summarize_dream_run(
        &self,
        cycle: CycleKind,
        report: &crate::dreamcycle::run::CycleReport,
        input_from_ts: i64,
        input_to_ts: i64,
        run_started_ms: i64,
        run_ended_ms: i64,
    ) -> crate::dreamcycle::run::DreamRunSummary {
        let (events_processed, state_changes, chunks_sent) = self
            .read_conn("dream.run_summary_counts", |c| {
                (
                    event_log::count_in_range(c, input_from_ts, input_to_ts).unwrap_or(0),
                    state::count_changed_since(c, run_started_ms).unwrap_or(0),
                    shogun_memory::traceability::count_since(c, run_started_ms).unwrap_or(0),
                )
            })
            .unwrap_or((0, 0, 0));
        crate::dreamcycle::run::DreamRunSummary {
            cycle,
            jobs_completed: report.completed.len(),
            events_processed,
            state_changes,
            chunks_sent,
            duration_ms: run_ended_ms.saturating_sub(run_started_ms),
            completed_fully: report.is_complete(),
        }
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
        JobKind::LessonDistillation => "lesson_distillation",
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
        "lesson_distillation" => JobKind::LessonDistillation,
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

/// Map an open-loop `kind` string to the Fusion [`StateKind`](shogun_fusion::assemble::StateKind)
/// that selects its action (reply → draft, waiting/other → surface).
fn open_loop_state_kind(kind: &str) -> shogun_fusion::assemble::StateKind {
    use shogun_fusion::assemble::StateKind;
    match kind {
        "reply_needed" => StateKind::OpenLoopReplyNeeded,
        "waiting_on_them" => StateKind::OpenLoopWaiting,
        _ => StateKind::OpenLoopOther,
    }
}

/// Score a state candidate's relevance to the focused screen (0.0..=1.0): a hit when any salient
/// term, or the window title, overlaps the subject/summary; a small baseline otherwise so unrelated
/// state can still surface when nothing matches. Cheap (substring, lowercased) — runs per focus.
fn screen_relevance(screen: &shogun_fusion::assemble::ScreenContext, subject: &str, summary: &str) -> f64 {
    let hay = format!("{} {}", subject.to_lowercase(), summary.to_lowercase());
    let title = screen.window_title.to_lowercase();
    let title_hit = !title.is_empty()
        && title.split_whitespace().any(|w| w.len() >= 4 && hay.contains(w));
    let salient_hit = screen.salient.iter().any(|s| {
        let s = s.to_lowercase();
        !s.is_empty() && hay.contains(&s)
    });
    if salient_hit || title_hit {
        1.0
    } else {
        0.4
    }
}

/// 検索 evidence を圧縮ブロックへ正規化する。evidence は「実際に見たもの」なので
/// confidence=1.0、relevance は呼び出し側が渡す検索スコア由来の値。
fn evidence_to_blocks(evidence: &[Evidence], relevance: f64, est: &dyn TokenEstimator) -> Vec<ContextBlock> {
    evidence
        .iter()
        .map(|e| {
            ContextBlock::new(
                BlockRef::Event(e.event_id),
                SourceKind::Evidence,
                e.excerpt.clone(),
                ScoreInputs { relevance, ..EVIDENCE_SCORE },
                est,
            )
        })
        .collect()
}

/// confidence ゲートを通した facts を圧縮ブロックへ正規化する。facts は既に
/// `treat_fact` を通っている（低 confidence は除外済み）。
/// 各エントリは (表示テキスト, 由来テーブル, 実 row id) のタプル。
/// relevance はやや高めに固定（state は現在の作業に紐づく前提）、confidence は High 相当として 0.9。
fn facts_to_blocks(
    facts: &[(String, shogun_fusion::block::StateTable, i64)],
    est: &dyn TokenEstimator,
) -> Vec<ContextBlock> {
    facts
        .iter()
        .map(|(f, table, id)| {
            ContextBlock::new(
                BlockRef::State { table: *table, id: *id },
                SourceKind::StateFact,
                f.clone(),
                FACT_SCORE,
                est,
            )
        })
        .collect()
}

#[cfg(feature = "db")]
fn load_db_encryption_key() -> Result<shogun_memory::DbKey, String> {
    if let Ok(hex) = std::env::var("SHOGUN_DB_KEY") {
        let trimmed = hex.trim();
        if !trimmed.is_empty() {
            return shogun_memory::DbKey::from_hex(trimmed)
                .ok_or_else(|| "SHOGUN_DB_KEY is not valid hex".to_string());
        }
    }
    #[cfg(target_os = "macos")]
    {
        use shogun_integrations::keychain_store;
        const DB_KEY_ACCOUNT: &str = "memory-db-key";
        let bytes = keychain_store::get_generic_secret(DB_KEY_ACCOUNT).map_err(|e| {
            format!(
                "could not read memory DB key from Keychain (status {}): unlock Keychain and retry",
                e.code()
            )
        })?;
        let hex = String::from_utf8(bytes).map_err(|_| "memory DB key in Keychain is not valid text".to_string())?;
        shogun_memory::DbKey::from_hex(&hex)
            .ok_or_else(|| "memory DB key in Keychain is malformed".to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("encrypted memory DB requires SHOGUN_DB_KEY".to_string())
    }
}

/// Exact local midnight boundaries for the current and previous calendar day.
#[cfg(feature = "db")]
pub fn local_day_bounds(now_ms: i64) -> shogun_memory::search::LocalDayBounds {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: `tm` is initialized by localtime_r. mktime accepts and normalizes the copied
        // local calendar values; `tm_isdst = -1` makes libc resolve the correct offset per date.
        unsafe {
            let mut tm: libc::tm = std::mem::zeroed();
            let t = (now_ms / 1000) as libc::time_t;
            if libc::localtime_r(&t, &mut tm).is_null() {
                return utc_day_bounds(now_ms);
            }
            tm.tm_hour = 0;
            tm.tm_min = 0;
            tm.tm_sec = 0;
            tm.tm_isdst = -1;
            let today = libc::mktime(&mut tm);
            if today == -1 {
                return utc_day_bounds(now_ms);
            }
            let mut yesterday_tm = tm;
            yesterday_tm.tm_mday -= 1;
            yesterday_tm.tm_isdst = -1;
            let yesterday = libc::mktime(&mut yesterday_tm);
            if yesterday == -1 {
                utc_day_bounds(now_ms)
            } else {
                shogun_memory::search::LocalDayBounds {
                    yesterday_start_ms: (yesterday as i64) * 1000,
                    today_start_ms: (today as i64) * 1000,
                }
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        utc_day_bounds(now_ms)
    }
}

/// The Evening Wrap's window (§6.17): `(local midnight today, end of tomorrow)`.
///
/// Same libc path as [`local_day_bounds`] rather than `today_start + 2 × 24 h`. A day containing a
/// DST transition is 23 or 25 hours long, so fixed-length arithmetic would put "the end of
/// tomorrow" an hour off twice a year — and the Wrap's boundaries decide which commitments count
/// as tomorrow's, so an hour of slack at the edge silently moves items between sections.
#[cfg(feature = "db")]
pub fn local_wrap_window(now_ms: i64) -> (i64, i64) {
    let today_start = local_day_bounds(now_ms).today_start_ms;
    #[cfg(target_os = "macos")]
    {
        // SAFETY: `tm` is initialized by localtime_r and only read after it reports success.
        // mktime normalizes `tm_mday + 2` across month and year ends; `tm_isdst = -1` makes libc
        // resolve the offset in effect on *that* date, not today's.
        unsafe {
            let mut tm: libc::tm = std::mem::zeroed();
            let t = (today_start / 1000) as libc::time_t;
            if !libc::localtime_r(&t, &mut tm).is_null() {
                tm.tm_mday += 2;
                tm.tm_isdst = -1;
                let end = libc::mktime(&mut tm);
                if end != -1 {
                    return (today_start, (end as i64) * 1000);
                }
            }
        }
    }
    (today_start, today_start + 2 * 24 * 60 * 60 * 1000)
}

#[cfg(feature = "db")]
fn utc_day_bounds(now_ms: i64) -> shogun_memory::search::LocalDayBounds {
    const DAY_MS: i64 = 24 * 60 * 60 * 1000;
    let today_start_ms = now_ms.div_euclid(DAY_MS) * DAY_MS;
    shogun_memory::search::LocalDayBounds {
        yesterday_start_ms: today_start_ms - DAY_MS,
        today_start_ms,
    }
}

/// The local calendar date (`YYYY-MM-DD`) an instant falls on — the `briefs` row key (Plan C-1).
/// Mirrors [`local_day_bounds`]: real local time on macOS (`localtime_r`, DST folded in), UTC
/// elsewhere (the Linux-test path).
#[cfg(feature = "db")]
pub fn local_date_string(now_ms: i64) -> String {
    let secs = now_ms.div_euclid(1000);
    #[cfg(target_os = "macos")]
    {
        // SAFETY: `tm` is only read after localtime_r reports success by returning non-null.
        unsafe {
            let mut tm: libc::tm = std::mem::zeroed();
            let t = secs as libc::time_t;
            if !libc::localtime_r(&t, &mut tm).is_null() {
                return format!("{:04}-{:02}-{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday);
            }
        }
    }
    let ymd = crate::dreamcycle::schedule::local_time(secs, 0).yyyymmdd;
    format!("{:04}-{:02}-{:02}", ymd / 10_000, (ymd / 100) % 100, ymd % 100)
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

    // ---------------------------------------------------------------- memory health (issue #121)
    // The three outcomes a memory read can have — nothing stored, the query refused, the lock
    // poisoned — used to be one empty vector. These pin them apart, and pin the recovery.

    /// Break the store the way a corrupt file or a bad migration would: the table the read needs
    /// is gone, so SQLite refuses the statement while the connection itself is fine.
    fn drop_table(db: &Db, table: &str) {
        let conn = db.conn.lock().unwrap();
        conn.execute_batch(&format!("DROP TABLE IF EXISTS {table}")).unwrap();
    }

    /// Poison the connection mutex the way a panic inside a locked section would.
    fn poison_lock(db: &Db) {
        let conn = db.conn.clone();
        let _ = std::thread::spawn(move || {
            let _g = conn.lock().unwrap();
            panic!("poisoning the memory lock on purpose");
        })
        .join();
        assert!(db.conn.lock().is_err(), "the fixture must actually poison the lock");
    }

    #[test]
    fn an_empty_store_is_not_a_failure() {
        let db = Db::open_in_memory(clock(1_000)).unwrap();
        assert_eq!(db.try_search("anything", 10).unwrap(), Vec::new());
        assert_eq!(db.try_commitment_rows().unwrap().len(), 0);
        let h = db.memory_health();
        assert!(!h.degraded, "no rows is an answer, not a fault");
        assert_eq!(h.faults_total, 0);
    }

    #[test]
    fn a_query_failure_is_reported_instead_of_looking_empty() {
        let db = Db::open_in_memory(clock(1_000)).unwrap();
        db.capture(&ev("the budget review notes", "h1", 900)).unwrap();
        // Sanity: the same call answers before the break.
        assert!(!db.try_search("budget", 10).unwrap().is_empty());

        drop_table(&db, "event_log");
        let err = db.try_search("budget", 10).unwrap_err();
        assert_eq!(err, MemoryFault::Query, "a refused statement is not an empty result");
        // The lossy wrapper still returns a vec, but the store is now visibly degraded — which is
        // the whole point: the caller that cannot act on the error is no longer the only one told.
        assert_eq!(db.search("budget", 10), Vec::new());
        let h = db.memory_health();
        assert!(h.degraded);
        assert_eq!(h.fault, Some(MemoryFault::Query));
        assert!(h.faults_total >= 2);
        assert_eq!(h.last_fault_ms, Some(1_000));
    }

    #[test]
    fn a_poisoned_lock_is_distinguishable_from_a_query_failure() {
        let db = Db::open_in_memory(clock(1_000)).unwrap();
        poison_lock(&db);
        assert_eq!(db.try_search("anything", 10).unwrap_err(), MemoryFault::LockPoisoned);
        assert_eq!(db.try_commitment_rows().unwrap_err(), MemoryFault::LockPoisoned);
        assert_eq!(db.memory_health().fault, Some(MemoryFault::LockPoisoned));
    }

    #[test]
    fn state_reads_do_not_report_a_dead_store_as_nothing_owed() {
        let db = Db::open_in_memory(clock(1_000)).unwrap();
        drop_table(&db, "commitments");
        // "You owe nothing today" is the answer this must never invent.
        assert_eq!(db.try_commitments_due(1_000).unwrap_err(), MemoryFault::Query);
        assert!(db.memory_health().degraded);
        // …and the fault is per operation, not a blanket verdict: an intact table still answers,
        // which is exactly what makes the recovery rule below meaningful.
        assert_eq!(db.try_people().unwrap(), Vec::new());
    }

    #[test]
    fn the_degraded_state_lifts_once_the_store_answers_again() {
        // Recovery without a relaunch (the issue's acceptance criterion): a transient failure
        // must not leave the warning stuck on forever.
        let db = Db::open_in_memory(clock(1_000)).unwrap();
        drop_table(&db, "people");
        assert!(db.try_people().is_err());
        assert!(db.memory_health().degraded);

        // A different read against an intact table is a successful operation.
        assert!(db.try_commitment_rows().is_ok());
        let h = db.memory_health();
        assert!(!h.degraded, "a success clears the degraded state");
        assert_eq!(h.faults_total, 1, "…without erasing that it happened");
    }

    #[test]
    fn an_ordinary_missing_row_is_not_a_fault() {
        // The failure mode this guards against is a warning that is always on: every `get` for a
        // row that simply does not exist would light "memory isn't responding" if a miss were
        // treated as a query error. The memory helpers map a missing row to `Ok(None)` — this
        // pins that they keep doing so through the health seam.
        let db = Db::open_in_memory(clock(1_000)).unwrap();
        assert_eq!(db.person(404), None);
        assert_eq!(db.commitment(404), None);
        assert_eq!(db.open_loop(404), None);
        assert_eq!(db.meeting_note(404), None);
        assert_eq!(db.meeting_recap_full(404), None);
        assert_eq!(db.brief_for("2026-08-15"), None);
        assert_eq!(db.event_source(404), None);
        let h = db.memory_health();
        assert!(!h.degraded, "a miss is an answer");
        assert_eq!(h.faults_total, 0);
    }

    #[test]
    fn a_capture_write_failure_marks_memory_degraded() {
        let db = Db::open_in_memory(clock(1_000)).unwrap();
        drop_table(&db, "event_log");
        assert_eq!(db.try_capture(&ev("text", "h1", 1)).unwrap_err(), MemoryFault::Query);
        assert!(db.capture(&ev("text", "h2", 2)).is_none());
        assert!(db.memory_health().degraded, "capture that silently stops recording must show");
    }

    #[test]
    fn event_source_returns_origin_metadata_only() {
        let db = Db::open_in_memory(clock(10)).unwrap();
        let (id, _) = db.capture(&ev("some captured text", "h1", 1)).unwrap();
        assert_eq!(
            db.event_source(id),
            Some(("capture".to_string(), Some("com.apple.Safari".to_string())))
        );
        assert_eq!(db.event_source(id + 999), None, "unknown id is None, not an error");
    }

    #[test]
    fn the_wrap_window_starts_at_local_midnight_and_ends_after_tomorrow() {
        // On the Linux CI path local_day_bounds is UTC, so this pins the arithmetic; on macOS the
        // libc branch supplies the same shape with DST folded in.
        const DAY: i64 = 24 * 60 * 60 * 1_000;
        let now = 3 * DAY + 72_000_000; // 20:00 on day 3
        let (start, end) = local_wrap_window(now);
        assert!(start <= now && now < end);
        assert_eq!(start, local_day_bounds(now).today_start_ms, "the window opens at midnight");
        assert_eq!(end - start, 2 * DAY, "today plus tomorrow");
    }

    #[test]
    fn evening_wrap_aggregates_the_day_locally() {
        use shogun_memory::lessons::{FeedbackKind, LessonScope, NewFeedback};
        use shogun_memory::state::{
            CommitmentDirection, CommitmentStatus, NewCommitment, NewOpenLoop, OpenLoopKind,
            Provenance,
        };
        // A day: 00:00 = 0ms, "now" = 20:00 (72_000_000ms), tomorrow ends at 48h.
        let day_start = 0i64;
        let now = 72_000_000i64;
        let tomorrow_end = 172_800_000i64;
        let db = Db::open_in_memory(clock(now)).unwrap();
        let (e, _) = db.capture(&ev("evidence", "h1", 1)).unwrap();
        let prov = [Provenance::new(e)];

        // done today (outcome), still-open with today's activity, due-tomorrow, opened-today loop.
        let done_id = db
            .insert_commitment(
                &NewCommitment {
                    direction: CommitmentDirection::Mine,
                    counterparty_id: None,
                    description: "finished today",
                    due_at: Some(10),
                    status: CommitmentStatus::Open,
                    project_id: None,
                    confidence: 0.9,
                    now: 100,
                },
                &prov,
            )
            .unwrap();
        assert!(db.resolve_commitment(done_id));
        db.insert_commitment(
            &NewCommitment {
                direction: CommitmentDirection::Mine,
                counterparty_id: None,
                description: "due tomorrow",
                due_at: Some(now + 3_600_000),
                status: CommitmentStatus::Open,
                project_id: None,
                confidence: 0.9,
                now: 200, // updated today → also "still open"
            },
            &prov,
        )
        .unwrap();
        db.insert_open_loop(
            &NewOpenLoop {
                kind: OpenLoopKind::ReplyNeeded,
                description: "new loose end",
                counterparty_id: None,
                project_id: None,
                opened_at: 500,
                confidence: 0.9,
                now: 500,
            },
            &prov,
        )
        .unwrap();
        // Today's decisions come from `feedback_events` — the table the lessons already learn
        // from. One adopted (edited, then approved) and one that is not a decision on a proposal
        // at all (`state_resolve`), so the counts have something to get wrong.
        {
            let conn = db.conn.lock().unwrap();
            let f = NewFeedback {
                ts_ms: 600,
                action_kind: Some("draft_reply"),
                scope_ref: None,
                before_text: Some("draft"),
                after_text: Some("edited draft"),
                ..Default::default()
            };
            shogun_memory::lessons::record_feedback(
                &conn,
                FeedbackKind::EditBeforeApprove,
                LessonScope::Global,
                &f,
            )
            .unwrap();
            shogun_memory::lessons::record_feedback(
                &conn,
                FeedbackKind::StateResolve,
                LessonScope::Global,
                &NewFeedback { before_text: None, after_text: None, ..f },
            )
            .unwrap();
        }

        let wrap = db.evening_wrap(
            vec![CalendarLine {
                start_ms: tomorrow_end - 1000,
                title: "standup".into(),
                updated: false,
            }],
            day_start,
            now,
            tomorrow_end,
        );

        assert_eq!(wrap.outcome.commitments_done, 1);
        // `state_resolve` is not a decision on one of SHOGUN's proposals, so it counts in neither.
        assert_eq!(wrap.outcome.actions_decided, 1);
        assert_eq!(wrap.outcome.actions_adopted, 1);
        // the resolved commitment is outcome, not "still open"; the live one appears once.
        assert!(wrap.still_open.iter().any(|i| i.text == "due tomorrow"));
        assert!(wrap.still_open.iter().all(|i| i.text != "finished today"));
        assert_eq!(wrap.tomorrow_commitments.len(), 1);
        assert_eq!(wrap.tomorrow_commitments[0].text, "due tomorrow");
        assert_eq!(wrap.tomorrow_calendar.len(), 1);
        assert_eq!(wrap.loose_ends.len(), 1);
        assert_eq!(wrap.loose_ends[0].text, "new loose end");
    }

    #[cfg(feature = "daemon-server")]
    #[test]
    fn confirmed_send_persists_a_trace_row_only_on_success() {
        use crate::send_exec::{SendExecOutcome, SendTransport};
        use shogun_agents::approval::{ConfirmedSend, Preview, Route as ApprovalRoute};
        use shogun_agents::permission::SendAction;

        struct Transport {
            ok: bool,
        }
        impl SendTransport for Transport {
            fn send(&self, _a: &SendAction, _body: &str) -> Result<(), String> {
                if self.ok {
                    Ok(())
                } else {
                    Err("not connected".into())
                }
            }
        }

        let db = Db::open_in_memory(clock(1)).unwrap();
        let action = SendAction::SendEmail { to: "alice@example.com".into() };
        let body = "TOP SECRET send body";
        let cs = ConfirmedSend { action: action.clone(), preview: Preview::for_send(&action, body, ApprovalRoute::ViaComposio) };

        // a failed send traces nothing (nothing egressed)
        assert_eq!(db.execute_confirmed_send(&cs, &Transport { ok: false }), SendExecOutcome::Failed("not connected".into()));
        assert!(db.trace_rows(&Filter::default()).is_empty(), "a failed send must not write a trace");

        // a successful send writes exactly one digest-only, third-party row
        assert_eq!(db.execute_confirmed_send(&cs, &Transport { ok: true }), SendExecOutcome::Sent);
        let rows = db.trace_rows(&Filter::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].destination, "alice@example.com");
        assert!(rows[0].third_party, "a Composio send is disclosed third-party");
        assert_eq!(rows[0].chunk_bytes, body.len() as i64);
        assert!(!format!("{:?}", rows[0]).contains("SECRET"), "sent body must never reach the trace row");
    }

    #[test]
    fn ingested_integration_items_are_searchable_and_source_tagged() {
        use shogun_mcp::sync::IngestItem;
        let db = Db::open_in_memory(clock(1)).unwrap();
        let items = vec![
            IngestItem {
                source: "gmail",
                kind: "email",
                title: "Roadmap".into(),
                body: "Let's ship the quarterly deck on Friday".into(),
                ts_ms: 100,
            },
            IngestItem {
                source: "gcal",
                kind: "calendar_event",
                title: "Standup".into(),
                body: "Daily standup with the platform team".into(),
                ts_ms: 200,
            },
        ];
        let summary = db.ingest_integration(&items);
        assert_eq!((summary.processed, summary.newly_inserted), (2, 2), "both items are fresh inserts");

        // the synced email is now first-class memory: it comes back from hybrid search, tagged gmail…
        let hits = db.search("quarterly deck", 10);
        let email = hits.iter().find(|h| h.content.contains("quarterly deck")).expect("synced email searchable");
        assert_eq!(email.source, "gmail", "the hit carries its integration source (FR-INT-05)");
        // …and the calendar event too.
        let hits = db.search("standup", 10);
        assert!(hits.iter().any(|h| h.source == "gcal"), "synced calendar event must be searchable");
    }

    #[test]
    fn dream_run_summary_reflects_events_state_and_chunks() {
        use crate::dreamcycle::plan::JobKind;
        use crate::dreamcycle::run::CycleReport;
        use crate::llm::traceability::{Route, TraceRecord};

        // clock at 1000 so run-start filtering (>= 1000) is meaningful.
        let db = Db::open_in_memory(clock(1_000)).unwrap();
        // three events in the input window [10, 40)
        let mut first_event = 0;
        for (i, ts) in [10, 20, 30].into_iter().enumerate() {
            let (id, _) = db.capture(&ev("body", &format!("h{i}"), ts)).unwrap();
            if i == 0 {
                first_event = id;
            }
        }
        // a state change and two sent chunks happen "during the run" (updated_at/ts >= 1000 via clock)
        db.insert_open_loop(
            &shogun_memory::state::NewOpenLoop {
                kind: shogun_memory::state::OpenLoopKind::WaitingOnThem,
                description: "waiting on legal",
                counterparty_id: None,
                project_id: None,
                opened_at: 1_000,
                confidence: 0.9,
                now: 1_000,
            },
            &[shogun_memory::state::Provenance::new(first_event)],
        )
        .expect("open loop inserts with provenance");
        let sink = db.traceability_sink();
        sink.record(TraceRecord::for_chunk(Route::BatchApi, "consolidation", "api", "c1", false));
        sink.record(TraceRecord::for_chunk(Route::BatchApi, "consolidation", "api", "c2", false));

        let report = CycleReport { completed: vec![JobKind::Consolidation, JobKind::StateUpdate], failed: None };
        let summary = db.summarize_dream_run(CycleKind::Full, &report, 10, 40, 1_000, 1_250);

        assert_eq!(summary.events_processed, 3, "three events in the input window");
        assert!(summary.state_changes >= 1, "the open loop counts as a state change");
        assert_eq!(summary.chunks_sent, 2, "two traceability rows written during the run");
        assert_eq!(summary.jobs_completed, 2);
        assert_eq!(summary.duration_ms, 250);
        assert!(summary.completed_fully);
    }

    /// The reply context must carry the thread's own words, the state facts, and be measurable.
    #[test]
    fn a_reply_context_carries_the_thread_and_reports_its_build_cost() {
        let db = Db::open_in_memory(clock(10_000)).unwrap();
        db.capture(&NewEvent {
            window_title: Some("Q3 pricing"),
            ..ev("Alice asked for the renewal terms", "h1", 100)
        })
        .unwrap();
        db.capture(&NewEvent {
            window_title: Some("Q3 pricing"),
            ..ev("we settled at 12k for the year", "h2", 200)
        })
        .unwrap();
        let key = shogun_memory::thread::thread_key(
            "capture",
            None,
            Some("com.apple.Safari"),
            Some("Q3 pricing"),
        )
        .unwrap();

        let ctx = db.build_reply_context(&key);
        assert_eq!(ctx.title.as_deref(), Some("Q3 pricing"));
        assert_eq!(ctx.turns.len(), 2, "the whole conversation, not just the last line");
        // Oldest first — a reply reads the thread in order.
        assert!(ctx.turns[0].excerpt.contains("renewal terms"));
        assert!(ctx.turns[1].excerpt.contains("12k"));
        // The SLO measurement ships with the data it describes.
        assert!(ctx.build_ms < 1_000, "assembly should be fast: {}ms", ctx.build_ms);
    }

    /// The gap this closes: a locally-extracted commitment is emitted below the Low/Medium
    /// boundary and is therefore excluded from every generation. Until the model pass exists,
    /// repeated evidence is the only honest way it can become visible at all.
    #[test]
    fn local_maintenance_makes_a_repeatedly_seen_commitment_reach_the_user() {
        let db = Db::open_in_memory(clock(1_000_000)).unwrap();
        let events: Vec<i64> = (0..4)
            .map(|i| db.capture(&ev("I'll send the deck", &format!("h{i}"), 100 + i)).unwrap().0)
            .collect();
        let prov: Vec<_> =
            events.iter().map(|e| shogun_memory::state::Provenance::new(*e)).collect();
        db.insert_commitment(
            &shogun_memory::state::NewCommitment {
                direction: shogun_memory::state::CommitmentDirection::Mine,
                counterparty_id: None,
                description: "send the deck",
                due_at: None,
                status: shogun_memory::state::CommitmentStatus::Open,
                project_id: None,
                confidence: 0.35, // what the local rules assign: Low, so invisible
                now: 100,
            },
            &prov,
        )
        .unwrap();

        // Precondition: the confidence gate excludes it, so the user sees nothing.
        assert!(
            !db.inline_memory(8).iter().any(|f| f.contains("send the deck")),
            "precondition: a Low-confidence commitment is excluded"
        );

        let report = db.run_local_maintenance(1_000_000, 30 * 24 * 3_600_000);
        assert!(report.corroborated >= 1, "the repeatedly-evidenced row was raised: {report:?}");

        // Now it reaches the user — and, being corroborated rather than verified, it is offered
        // weakly rather than asserted.
        let facts = db.inline_memory(8);
        let line = facts.iter().find(|f| f.contains("send the deck"));
        assert!(line.is_some(), "must now be visible: {facts:?}");
        assert!(
            line.unwrap().contains(shogun_fusion::confidence::POSSIBLY_PREFIX),
            "corroboration alone must not let it be stated as fact: {line:?}"
        );
    }

    /// C-3 pin: an overdue notification is a non-egress, L1-permitted local action — it can never
    /// be (or become) a send. `required_level` derives L1 from the action type; the type is
    /// `Action::Local`, which has no send variant, so the property is structural.
    #[test]
    fn overdue_notifications_are_l1_non_sends() {
        use shogun_agents::permission::Level;
        let newly = vec![
            shogun_memory::recompute::NewlyOverdue { id: 1, description: "send the deck".into() },
            shogun_memory::recompute::NewlyOverdue { id: 2, description: "review the PR".into() },
        ];
        let actions = overdue_notifications(&newly);
        assert_eq!(actions.len(), newly.len(), "one notification per newly-overdue item");
        for a in &actions {
            assert_eq!(a.required_level(), Level::L1, "a notification is L1-permitted: {a:?}");
            assert!(a.is_l1_eligible());
            assert!(!a.is_external_send(), "a notification never leaves the device: {a:?}");
        }
        assert_ne!(actions[0], actions[1], "each item gets its own notification");
    }

    /// C-3 end-to-end (compilable half): the hourly maintenance reports a newly-overdue
    /// commitment exactly once — the pass that flips it — and later passes report nothing, so the
    /// desktop hook fires one notification per commitment, ever.
    #[test]
    fn maintenance_reports_newly_overdue_once_then_never_again() {
        let db = Db::open_in_memory(clock(1_000)).unwrap();
        let e = db.capture(&ev("promise", "h1", 100)).unwrap().0;
        db.insert_commitment(
            &shogun_memory::state::NewCommitment {
                direction: shogun_memory::state::CommitmentDirection::Mine,
                counterparty_id: None,
                description: "send the deck",
                due_at: Some(5_000),
                status: shogun_memory::state::CommitmentStatus::Open,
                project_id: None,
                confidence: 0.9,
                now: 100,
            },
            &[shogun_memory::state::Provenance::new(e)],
        )
        .unwrap();

        const HALF_LIFE: i64 = 30 * 24 * 3_600_000;
        // Before the due time: nothing to notify.
        let before = db.run_local_maintenance(1_000, HALF_LIFE);
        assert!(before.newly_overdue.is_empty());
        assert!(overdue_notifications(&before.newly_overdue).is_empty());

        // The pass that crosses the due time notifies exactly once.
        let first = db.run_local_maintenance(10_000, HALF_LIFE);
        assert_eq!(first.newly_overdue.len(), 1);
        assert_eq!(first.overdue, 1);
        let notes = overdue_notifications(&first.newly_overdue);
        assert_eq!(notes.len(), 1);

        // Every subsequent pass: silent for the already-notified item.
        for later in [10_000, 20_000, 1_000_000] {
            let again = db.run_local_maintenance(later, HALF_LIFE);
            assert!(again.newly_overdue.is_empty(), "must never re-notify: {again:?}");
        }
    }

    /// A reply is written *into* a conversation, so the thread's own words must lead the prompt —
    /// state facts and older similar threads are supporting material, not the subject.
    #[test]
    fn reply_context_flattens_with_the_thread_first() {
        let db = Db::open_in_memory(clock(10_000)).unwrap();
        db.capture(&NewEvent {
            window_title: Some("Q3 pricing"),
            ..ev("Alice asked for the renewal terms", "h1", 100)
        })
        .unwrap();
        let key = shogun_memory::thread::thread_key(
            "capture",
            None,
            Some("com.apple.Safari"),
            Some("Q3 pricing"),
        )
        .unwrap();

        let lines = db.build_reply_context(&key).as_memory_lines(10);
        assert!(lines[0].contains("Q3 pricing"), "what's in view is named first: {lines:?}");
        assert!(
            lines.iter().any(|l| l.contains("renewal terms")),
            "the thread's own words are present: {lines:?}"
        );
    }

    #[test]
    fn an_empty_reply_context_is_reported_so_the_caller_can_fall_back() {
        let db = Db::open_in_memory(clock(10_000)).unwrap();
        let ctx = db.build_reply_context("capture:nothing:here");
        assert!(ctx.is_empty(), "nothing warmed for this thread");
        assert!(ctx.as_memory_lines(10).is_empty());
    }

    /// A press must read a warm pack, never build one — building on the press is what the SLO
    /// forbids, so a miss is reported rather than silently papered over.
    #[test]
    fn the_cache_serves_the_current_thread_and_reports_a_miss_honestly() {
        let db = Db::open_in_memory(clock(10_000)).unwrap();
        db.capture(&NewEvent { window_title: Some("Alpha"), ..ev("alpha notes", "h1", 100) })
            .unwrap();
        let key =
            shogun_memory::thread::thread_key("capture", None, Some("com.apple.Safari"), Some("Alpha"))
                .unwrap();

        let cache = ReplyContextCache::new();
        assert!(cache.get(&key).is_none(), "cold cache is a miss, not an inline build");

        cache.put(db.build_reply_context(&key));
        assert!(cache.get(&key).is_some(), "warm for the built thread");
        assert!(
            cache.get("capture:com.apple.Safari:beta").is_none(),
            "a different thread is a miss — never serve the wrong thread's context"
        );
        assert_eq!(cache.current().map(|c| c.thread_key), Some(key));
    }

    #[test]
    fn clearing_the_reply_context_cache_removes_current_context() {
        let cache = ReplyContextCache::new();
        cache.put(ReplyContext {
            thread_key: "sensitive".into(),
            ..ReplyContext::default()
        });
        assert!(cache.current().is_some());

        cache.clear();

        assert!(cache.current().is_none());
    }

    /// "How's that going?" with one obvious candidate resolves to it.
    #[test]
    fn a_referring_question_resolves_when_one_thread_clearly_dominates() {
        use shogun_memory::thread::Referent;
        let now = 10_000_000_000;
        let db = Db::open_in_memory(clock(now)).unwrap();
        // One thread touched just now, one a week ago.
        db.capture(&NewEvent {
            window_title: Some("Q3 pricing"),
            ..ev("vendor pricing settled at 12k", "h1", now - 1_000)
        })
        .unwrap();
        db.capture(&NewEvent {
            window_title: Some("Old holiday plans"),
            ..ev("beach or mountains", "h2", now - 7 * 24 * 3_600_000)
        })
        .unwrap();

        let out = db.resolve_referent("how's that going?", None);
        assert_eq!(out.verdict, Referent::Resolved, "candidates: {:?}", out.candidates);
        assert!(
            out.candidates[0].title.as_deref() == Some("Q3 pricing"),
            "the live thread wins: {:?}",
            out.candidates
        );
    }

    /// Two equally-plausible threads must produce a question, not a guess. This is the behaviour
    /// that keeps a confident wrong answer about someone's work from ever being given.
    #[test]
    fn two_equally_live_threads_are_ambiguous_rather_than_guessed() {
        use shogun_memory::thread::Referent;
        let now = 10_000_000_000;
        let db = Db::open_in_memory(clock(now)).unwrap();
        db.capture(&NewEvent { window_title: Some("Alpha"), ..ev("alpha notes", "h1", now - 1_000) })
            .unwrap();
        db.capture(&NewEvent { window_title: Some("Beta"), ..ev("beta notes", "h2", now - 1_100) })
            .unwrap();

        let out = db.resolve_referent("any update on that?", None);
        assert_eq!(out.verdict, Referent::Ambiguous, "candidates: {:?}", out.candidates);
        assert!(out.candidates.len() >= 2, "the UI needs something to offer");
    }

    /// Naming the thread in the question breaks the tie without needing to ask.
    #[test]
    fn the_users_own_words_break_a_tie() {
        use shogun_memory::thread::Referent;
        let now = 10_000_000_000;
        let db = Db::open_in_memory(clock(now)).unwrap();
        db.capture(&NewEvent { window_title: Some("Alpha"), ..ev("alpha notes", "h1", now - 1_000) })
            .unwrap();
        db.capture(&NewEvent { window_title: Some("Beta"), ..ev("beta notes", "h2", now - 1_100) })
            .unwrap();

        let out = db.resolve_referent("any update on that alpha thing?", None);
        assert_eq!(out.verdict, Referent::Resolved);
        assert_eq!(out.candidates[0].title.as_deref(), Some("Alpha"));
    }

    /// What the user is looking at is a strong signal for what "that" means.
    #[test]
    fn the_thread_on_screen_wins_a_tie() {
        use shogun_memory::thread::Referent;
        let now = 10_000_000_000;
        let db = Db::open_in_memory(clock(now)).unwrap();
        db.capture(&NewEvent { window_title: Some("Alpha"), ..ev("alpha notes", "h1", now - 1_000) })
            .unwrap();
        db.capture(&NewEvent { window_title: Some("Beta"), ..ev("beta notes", "h2", now - 1_100) })
            .unwrap();
        let beta_key =
            shogun_memory::thread::thread_key("capture", None, Some("com.apple.Safari"), Some("Beta"))
                .unwrap();

        let out = db.resolve_referent("any update on that?", Some(&beta_key));
        assert_eq!(out.verdict, Referent::Resolved);
        assert_eq!(out.candidates[0].title.as_deref(), Some("Beta"));
    }

    #[test]
    fn an_empty_memory_has_no_referent_to_offer() {
        use shogun_memory::thread::Referent;
        let db = Db::open_in_memory(clock(1000)).unwrap();
        assert_eq!(db.resolve_referent("how's that going?", None).verdict, Referent::None);
    }

    /// The point of the semantic half: a question that shares no words with the answer still
    /// finds it. Lexical search cannot do this, so the same query is run both ways and the
    /// difference is the assertion.
    #[test]
    fn a_loaded_model_finds_an_answer_that_shares_no_words_with_the_question() {
        use shogun_memory::embed::{Embedder, EmbedError};

        /// A stand-in for the real model: it maps a fixed topic vocabulary onto the same vector,
        /// which is exactly the property (paraphrase → nearby vector) the real model provides.
        struct TopicEmbedder;
        impl TopicEmbedder {
            fn vector(text: &str) -> Vec<f32> {
                let t = text.to_lowercase();
                let pricing = ["pricing", "cost", "how much", "renewal fee"]
                    .iter()
                    .any(|w| t.contains(w));
                // Must match the store's fixed width (the vec0 table is E5-sized).
                let mut v = vec![0.0f32; shogun_memory::embed::E5_SMALL_DIM];
                if pricing {
                    v[0] = 1.0;
                } else {
                    v[1] = 1.0;
                }
                v
            }
        }
        impl Embedder for TopicEmbedder {
            fn dim(&self) -> usize {
                shogun_memory::embed::E5_SMALL_DIM
            }
            fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
                Ok(texts.iter().map(|t| Self::vector(t)).collect())
            }
            fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
                Ok(Self::vector(text))
            }
        }

        let plain = Db::open_in_memory(clock(1000)).unwrap();
        plain.capture(&ev("The renewal fee lands at 12k", "h1", 1)).unwrap();
        plain.capture(&ev("Standup notes: nothing blocking", "h2", 2)).unwrap();

        // Lexical only: "cost" appears nowhere, so there is nothing to match.
        let lexical = plain.assemble_context("what was the cost?", 5, 200);
        assert!(
            !lexical.evidence.iter().any(|e| e.excerpt.contains("12k")),
            "precondition: lexical search cannot find this"
        );

        // Same store, now with a model attached and the backlog embedded.
        let db = plain.with_embedder(std::sync::Arc::new(TopicEmbedder));
        assert_eq!(db.embed_pending(100), 2, "the backlog is embedded off the write path");

        let hybrid = db.assemble_context("what was the cost?", 5, 200);
        assert!(
            hybrid.evidence.iter().any(|e| e.excerpt.contains("12k")),
            "the semantic half must find the paraphrase: {:?}",
            hybrid.evidence
        );
    }

    /// Without a model nothing regresses — search stays lexical rather than returning nothing.
    #[test]
    fn search_still_works_with_no_model_loaded() {
        let db = Db::open_in_memory(clock(1000)).unwrap();
        db.capture(&ev("vendor pricing settled at 12k", "h1", 1)).unwrap();
        assert_eq!(db.embed_pending(100), 0, "no model, nothing embedded");
        let pack = db.assemble_context("vendor pricing", 5, 200);
        assert!(pack.evidence.iter().any(|e| e.excerpt.contains("12k")));
    }

    #[test]
    fn ai_session_turns_become_searchable_threaded_memory() {
        use shogun_memory::ai_session::{Role, SessionTurn};
        let db = Db::open_in_memory(clock(1000)).unwrap();
        let turns = vec![
            SessionTurn {
                session_id: "sess-1".into(),
                role: Role::User,
                ts_ms: 10,
                text: "why did we drop the vendor migration?".into(),
                cwd: Some("/proj".into()),
            },
            SessionTurn {
                session_id: "sess-1".into(),
                role: Role::Assistant,
                ts_ms: 20,
                text: "Because the vendor migration needed downtime we could not take.".into(),
                cwd: Some("/proj".into()),
            },
        ];
        let s = db.ingest_ai_session(&turns);
        assert_eq!(s.newly_inserted, 2);

        // The whole point: this is now answerable context.
        let pack = db.assemble_context("vendor migration", 5, 200);
        assert!(
            pack.evidence.iter().any(|e| e.excerpt.contains("downtime")),
            "the session answer must be retrievable: {:?}",
            pack.evidence
        );
        assert!(pack.evidence.iter().all(|e| e.source == "ai_session"));

        // Both turns share the session's thread, so the conversation stays one unit.
        let keys: Vec<Option<String>> = {
            let c = db.conn.lock().unwrap();
            let mut st = c.prepare("SELECT thread_key FROM event_log ORDER BY id").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
        };
        assert_eq!(keys.len(), 2);
        assert!(keys[0].is_some() && keys[0] == keys[1], "one session is one thread: {keys:?}");
    }

    /// A session log is append-only and gets re-read as it grows, so importing it twice must not
    /// double the memory.
    #[test]
    fn re_importing_a_session_log_does_not_duplicate_turns() {
        use shogun_memory::ai_session::{Role, SessionTurn};
        let db = Db::open_in_memory(clock(1000)).unwrap();
        let turns = vec![SessionTurn {
            session_id: "sess-1".into(),
            role: Role::User,
            ts_ms: 10,
            text: "ship it on Friday".into(),
            cwd: None,
        }];
        assert_eq!(db.ingest_ai_session(&turns).newly_inserted, 1);
        let second = db.ingest_ai_session(&turns);
        assert_eq!(second.newly_inserted, 0, "already-seen turns must not be re-inserted");
        assert_eq!(second.processed, 1);
    }

    /// Credentials pasted into a session must not survive into the database.
    #[test]
    fn secrets_in_an_ingested_turn_are_masked_before_storage() {
        use shogun_memory::ai_session::{Role, SessionTurn};
        let db = Db::open_in_memory(clock(1000)).unwrap();
        db.ingest_ai_session(&[SessionTurn {
            session_id: "s".into(),
            role: Role::User,
            ts_ms: 10,
            text: "deploy with ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ012345 please".into(),
            cwd: None,
        }]);
        let stored: String = {
            let c = db.conn.lock().unwrap();
            c.query_row("SELECT content FROM event_log", [], |r| r.get(0)).unwrap()
        };
        assert!(!stored.contains("ghp_ABCDEF"), "the token must not be stored: {stored}");
        assert!(stored.contains("[redacted]"));
        assert!(stored.contains("deploy with"), "surrounding text is preserved: {stored}");
    }

    /// The R1 gap: a question about something in the event log has to reach that event. Before
    /// `assemble_context` the prompt only ever carried state facts, so this content was
    /// unreachable no matter what the user asked.
    #[test]
    fn assemble_context_retrieves_the_event_that_answers_the_question() {
        let db = Db::open_in_memory(clock(1000)).unwrap();
        db.capture(&ev("Vendor pricing settled at 12k for the renewal", "h1", 1)).unwrap();
        db.capture(&ev("Standup notes: nothing blocking today", "h2", 2)).unwrap();

        let pack = db.assemble_context("vendor pricing", 5, 200);
        assert!(
            pack.evidence.iter().any(|e| e.excerpt.contains("12k")),
            "the answering event must be retrieved: {:?}",
            pack.evidence
        );
        // Attribution rides along so an answer can cite it.
        let hit = pack.evidence.iter().find(|e| e.excerpt.contains("12k")).unwrap();
        assert_eq!(hit.source, "capture");
        assert!(hit.event_id > 0);
    }

    #[test]
    fn assemble_context_retrieves_query_relevant_meeting_not_latest() {
        use shogun_memory::session::{open, NewSession};

        let db = Db::open_in_memory(clock(10_000)).unwrap();
        let conn = db.conn.lock().unwrap();
        let old = open(
            &conn,
            &NewSession {
                kind: "meeting",
                started_at: 1_000,
                title: Some("Phoenix planning"),
                app_bundle_id: None,
                calendar_occurrence_id: None,
                confidence: 0.8,
                provenance: "{}",
            },
        )
        .unwrap();
        let recent = open(
            &conn,
            &NewSession {
                kind: "meeting",
                started_at: 9_000,
                title: Some("Daily standup"),
                app_bundle_id: None,
                calendar_occurrence_id: None,
                confidence: 0.8,
                provenance: "{}",
            },
        )
        .unwrap();
        shogun_memory::meeting_recaps::save(
            &conn,
            old,
            "Phoenix launch is targeted for March with beta in February.",
            "[]",
            "[]",
            "m",
            2_000,
        )
        .unwrap();
        shogun_memory::meeting_recaps::save(&conn, recent, "Nothing blocking today.", "[]", "[]", "m", 10_000)
            .unwrap();
        drop(conn);

        let pack = db.assemble_context("Phoenix launch beta", 5, 300);
        let meeting = pack
            .evidence
            .iter()
            .find(|e| e.source == "meeting")
            .expect("meeting evidence must be present");
        assert!(meeting.excerpt.contains("Phoenix"));
        assert_eq!(meeting.event_id, -old);
        assert!(
            !pack.evidence.iter().any(|e| e.source == "meeting" && e.excerpt.contains("Nothing blocking")),
            "unrelated latest meeting must not appear: {:?}",
            pack.evidence
        );
    }

    #[test]
    fn assemble_context_caps_a_huge_capture_to_the_excerpt_budget() {
        let db = Db::open_in_memory(clock(1000)).unwrap();
        let huge = format!("{}decision: ship on Friday{}", "filler ".repeat(500), " tail".repeat(500));
        db.capture(&ev(&huge, "h1", 1)).unwrap();

        let pack = db.assemble_context("decision", 5, 120);
        let e = pack.evidence.first().expect("retrieved");
        assert!(e.excerpt.contains("decision: ship on Friday"), "match kept: {}", e.excerpt);
        assert!(e.excerpt.chars().count() <= 122, "budget held: {}", e.excerpt.chars().count());
    }

    /// An empty/garbage question must degrade to the state facts, never error or return junk.
    #[test]
    fn assemble_context_without_a_match_still_returns_state_facts() {
        let db = Db::open_in_memory(clock(1000)).unwrap();
        let e = db.capture(&ev("unrelated", "h1", 1)).unwrap().0;
        db.insert_commitment(
            &shogun_memory::state::NewCommitment {
                direction: shogun_memory::state::CommitmentDirection::Mine,
                counterparty_id: None,
                description: "send Alice the deck",
                due_at: None,
                status: shogun_memory::state::CommitmentStatus::Open,
                project_id: None,
                confidence: 0.9,
                now: 1,
            },
            &[shogun_memory::state::Provenance::new(e)],
        )
        .expect("commitment");

        let pack = db.assemble_context("", 5, 200);
        assert!(pack.evidence.is_empty(), "empty query retrieves nothing");
        assert!(
            pack.facts.iter().any(|f| f.contains("send Alice the deck")),
            "state facts still ground the answer: {:?}",
            pack.facts
        );
    }

    #[test]
    fn inline_memory_gates_low_confidence_out() {
        let db = Db::open_in_memory(clock(1)).unwrap();
        let e = db.capture(&ev("evidence", "h1", 1)).unwrap().0;
        let prov = [shogun_memory::state::Provenance::new(e)];
        // a high-confidence commitment (stated as fact) and a low-confidence one (dropped)
        db.insert_commitment(
            &shogun_memory::state::NewCommitment {
                direction: shogun_memory::state::CommitmentDirection::Mine,
                counterparty_id: None,
                description: "send Alice the deck",
                due_at: None,
                status: shogun_memory::state::CommitmentStatus::Open,
                project_id: None,
                confidence: 0.9,
                now: 1,
            },
            &prov,
        )
        .expect("high commitment");
        db.insert_commitment(
            &shogun_memory::state::NewCommitment {
                direction: shogun_memory::state::CommitmentDirection::Mine,
                counterparty_id: None,
                description: "maybe ping the vendor",
                due_at: None,
                status: shogun_memory::state::CommitmentStatus::Open,
                project_id: None,
                confidence: 0.3,
                now: 1,
            },
            &prov,
        )
        .expect("low commitment");

        let mem = db.inline_memory(10);
        assert!(mem.iter().any(|m| m.contains("send Alice the deck")), "high-confidence fact is included: {mem:?}");
        assert!(
            !mem.iter().any(|m| m.contains("maybe ping the vendor")),
            "low-confidence guess must not be handed to the model as fact (FR-ST-20): {mem:?}"
        );
    }

    #[test]
    fn ingested_email_commitment_reaches_the_state_tables() {
        use shogun_mcp::sync::IngestItem;
        let db = Db::open_in_memory(clock(1)).unwrap();
        // an email in which the user states a commitment — the same heuristic that fires on captured
        // text should fire here, feeding the state tables at low confidence (WP2.7 / FR-ST-02).
        let items = vec![IngestItem {
            source: "gmail",
            kind: "email",
            title: "Re: deck".into(),
            body: "Thanks — I'll send the final deck by Friday.".into(),
            ts_ms: 100,
        }];
        let summary = db.ingest_integration(&items);
        assert_eq!(summary.newly_inserted, 1);
        assert!(summary.candidates >= 1, "the email commitment should yield a candidate");

        // it lands as a low-confidence commitment, linked back to the ingested email.
        let commitments = db.commitments_due(1_000);
        assert!(!commitments.is_empty(), "the email commitment reached the state table");
        assert!(
            commitments.iter().all(|c| c.confidence <= shogun_memory::extract::LOCAL_RULE_MAX_CONFIDENCE),
            "an extracted email commitment is low-confidence, never asserted as fact (FR-ST-20)"
        );
    }

    #[test]
    fn re_syncing_the_same_item_touches_not_duplicates() {
        use shogun_mcp::sync::IngestItem;
        let db = Db::open_in_memory(clock(1)).unwrap();
        let item = IngestItem {
            source: "gmail",
            kind: "email",
            title: "Invoice".into(),
            body: "Payment is due next week".into(),
            ts_ms: 100,
        };
        let first = db.ingest_integration(std::slice::from_ref(&item));
        assert_eq!(first.newly_inserted, 1);
        // re-sync the identical item (same source + content) → dedup touch, no new row, no re-extract
        let second = db.ingest_integration(std::slice::from_ref(&item));
        assert_eq!(
            (second.processed, second.newly_inserted, second.candidates),
            (1, 0, 0),
            "an unchanged re-sync must not duplicate the event or re-extract"
        );
    }

    /// One sync batch with new items → exactly one `IntegrationSynced` on the bus, carrying the
    /// source tag and the newly-inserted count — never item content (design §2.2 wiring).
    #[test]
    fn a_sync_with_new_items_publishes_exactly_one_integration_synced() {
        use crate::bus::{Bus, BusEvent};
        use shogun_mcp::sync::IngestItem;
        let bus = Bus::new(8);
        let mut sub = bus.subscribe();
        let db = Db::open_in_memory(clock(1)).unwrap().with_bus(bus);
        let items = vec![
            IngestItem { source: "gmail", kind: "email", title: "A".into(), body: "first mail".into(), ts_ms: 100 },
            IngestItem { source: "gmail", kind: "email", title: "B".into(), body: "second mail".into(), ts_ms: 200 },
        ];
        let summary = db.ingest_integration(&items);
        assert_eq!(summary.newly_inserted, 2);
        let ev = sub.try_recv().expect("one event for the batch");
        assert_eq!(*ev, BusEvent::IntegrationSynced { source: "gmail", count: 2 });
        assert!(sub.try_recv().is_none(), "exactly one event per synced source, not one per item");
    }

    /// A sync that changes nothing (dedup-only re-sync, or an empty batch) stays silent — no
    /// event means no needless cache invalidation downstream.
    #[test]
    fn a_sync_with_zero_new_items_publishes_nothing() {
        use crate::bus::Bus;
        use shogun_mcp::sync::IngestItem;
        let bus = Bus::new(8);
        let db = Db::open_in_memory(clock(1)).unwrap().with_bus(bus.clone());
        let item = IngestItem {
            source: "gmail",
            kind: "email",
            title: "Invoice".into(),
            body: "Payment is due next week".into(),
            ts_ms: 100,
        };
        db.ingest_integration(std::slice::from_ref(&item));
        // Subscribe after the first (publishing) sync; only silence must follow.
        let mut sub = bus.subscribe();
        let second = db.ingest_integration(std::slice::from_ref(&item));
        assert_eq!(second.newly_inserted, 0, "precondition: an unchanged re-sync is dedup-only");
        assert!(sub.try_recv().is_none(), "a dedup-only sync must not publish");
        db.ingest_integration(&[]);
        assert!(sub.try_recv().is_none(), "an empty batch must not publish");
    }

    /// The §2.2 loop end-to-end: sync lands new items → `IntegrationSynced` → the daemon's
    /// subscription empties the cache, so the next `get` is an honest miss until the focus path
    /// re-assembles. Invalidation clears — it never rebuilds inline.
    #[test]
    fn an_integration_synced_event_empties_the_cache_until_reassembled() {
        use crate::bus::Bus;
        use shogun_mcp::sync::IngestItem;
        let bus = Bus::new(8);
        let db = Db::open_in_memory(clock(10_000)).unwrap().with_bus(bus.clone());
        let cache = ReplyContextCache::new();
        let mut inv = SyncInvalidator::new(&bus, cache.clone());

        db.capture(&NewEvent { window_title: Some("Alpha"), ..ev("alpha notes", "h1", 100) })
            .unwrap();
        let key =
            shogun_memory::thread::thread_key("capture", None, Some("com.apple.Safari"), Some("Alpha"))
                .unwrap();
        cache.put(db.build_reply_context(&key));
        assert!(cache.get(&key).is_some(), "precondition: warm before the sync");

        // A non-sync event leaves the warm pack alone.
        bus.publish(crate::bus::BusEvent::CacheUpdated);
        assert_eq!(inv.pump(), 0);
        assert!(cache.get(&key).is_some(), "unrelated events must not evict the pack");

        let summary = db.ingest_integration(&[IngestItem {
            source: "gmail",
            kind: "email",
            title: "Re: Alpha".into(),
            body: "new material for the alpha thread".into(),
            ts_ms: 200,
        }]);
        assert_eq!(summary.newly_inserted, 1);
        assert_eq!(inv.pump(), 1, "one sync event handled");
        assert!(cache.get(&key).is_none(), "stale pack is gone — a miss, not an inline rebuild");
        assert!(cache.current().is_none(), "cleared entirely, whatever thread was held");

        // The focus path re-assembles; the cache serves again.
        cache.put(db.build_reply_context(&key));
        assert!(cache.get(&key).is_some(), "fresh after re-assembly");
    }

    /// The subscription is tick-driven, not a spin: pumping a quiet bus does no work and returns
    /// at once (`try_recv` is non-blocking), so an idle daemon burns nothing between ticks.
    #[test]
    fn pumping_a_quiet_bus_returns_immediately_without_spinning() {
        use crate::bus::Bus;
        let bus = Bus::new(8);
        let cache = ReplyContextCache::new();
        let mut inv = SyncInvalidator::new(&bus, cache);
        let t0 = std::time::Instant::now();
        for _ in 0..1_000 {
            assert_eq!(inv.pump(), 0, "a quiet bus handles nothing");
        }
        assert!(
            t0.elapsed() < std::time::Duration::from_millis(100),
            "1000 quiet pumps must be near-instant (non-blocking drain, no spin/wait): {:?}",
            t0.elapsed()
        );
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
    fn context_actions_ranks_gated_candidates_for_the_screen() {
        use shogun_fusion::assemble::ScreenContext;
        let db = Db::open_in_memory(clock(1)).unwrap();
        let e = db.capture(&ev("evidence", "h1", 1)).unwrap().0;
        // a high-confidence reply-needed loop mentioning "roadmap", and a low-confidence one
        db.insert_open_loop(
            &shogun_memory::state::NewOpenLoop {
                kind: shogun_memory::state::OpenLoopKind::ReplyNeeded,
                description: "reply about the roadmap",
                counterparty_id: None,
                project_id: None,
                opened_at: 1,
                confidence: 0.9,
                now: 1,
            },
            &[shogun_memory::state::Provenance::new(e)],
        )
        .unwrap();
        db.insert_open_loop(
            &shogun_memory::state::NewOpenLoop {
                kind: shogun_memory::state::OpenLoopKind::Other,
                description: "vague low-confidence thing",
                counterparty_id: None,
                project_id: None,
                opened_at: 1,
                confidence: 0.2, // below the gate — must not become an action (FR-ST-20)
                now: 1,
            },
            &[shogun_memory::state::Provenance::new(e)],
        )
        .unwrap();

        let screen = ScreenContext {
            app_bundle_id: "com.apple.Mail".into(),
            window_title: "roadmap thread".into(),
            salient: vec!["roadmap".into()],
        };
        let cache = db.context_actions(screen, None);
        // the low-confidence loop is gated out; the reply-needed one is present as an action
        assert!(!cache.actions.is_empty());
        // B-5: the reply-needed loop also proposes DraftReply — the ONLY send-family candidate,
        // and it carries L3 (invariant 4: approval-required by construction, never auto-run).
        assert!(
            cache.actions.iter().all(|a| a.action.is_external_send() == (a.level == shogun_fusion::Level::L3)),
            "L3 iff external send — locals stay L1/L2, sends are never below L3 (invariant 4)"
        );
        assert!(
            cache.actions.iter().any(|a| a.action.is_external_send()),
            "a reply-needed loop must produce the DraftReply candidate (B-5)"
        );
        assert!(cache.facts.iter().any(|f| f.contains("roadmap")), "gated fact present");
        assert!(!cache.facts.iter().any(|f| f.contains("vague")), "low-confidence fact excluded");
    }

    /// A commitment the user resolved (or a loop they closed) is finished work — it must not
    /// re-surface as a fact or occupy one of the four action slots on the next panel open.
    #[test]
    fn context_actions_excludes_resolved_state() {
        use shogun_fusion::assemble::ScreenContext;
        let db = Db::open_in_memory(clock(1)).unwrap();
        let e = db.capture(&ev("evidence", "h1", 1)).unwrap().0;
        let cid = db
            .insert_commitment(
                &shogun_memory::state::NewCommitment {
                    direction: shogun_memory::state::CommitmentDirection::Mine,
                    counterparty_id: None,
                    description: "send the finished deck",
                    due_at: Some(10),
                    status: shogun_memory::state::CommitmentStatus::Open,
                    project_id: None,
                    confidence: 0.9,
                    now: 1,
                },
                &[shogun_memory::state::Provenance::new(e)],
            )
            .unwrap();
        let lid = db
            .insert_open_loop(
                &shogun_memory::state::NewOpenLoop {
                    kind: shogun_memory::state::OpenLoopKind::ReplyNeeded,
                    description: "reply about the finished deck",
                    counterparty_id: None,
                    project_id: None,
                    opened_at: 1,
                    confidence: 0.9,
                    now: 1,
                },
                &[shogun_memory::state::Provenance::new(e)],
            )
            .unwrap();
        db.resolve_commitment(cid);
        db.resolve_open_loop(lid);

        let screen = ScreenContext {
            app_bundle_id: "com.apple.Mail".into(),
            window_title: "finished deck".into(),
            salient: vec!["deck".into()],
        };
        let cache = db.context_actions(screen, None);
        assert!(
            !cache.facts.iter().any(|f| f.contains("finished deck")),
            "resolved state must not come back as a fact: {:?}",
            cache.facts
        );
        assert!(
            !cache.actions.iter().any(|a| a.rationale.contains("finished deck")),
            "resolved state must not occupy an action slot"
        );
    }

    #[test]
    fn ingest_capture_collapses_and_extracts() {
        let db = Db::open_in_memory(clock(1000)).unwrap();
        // a realistic-length window body (the 98% near-dup rule is calibrated for real captures,
        // not one-liners): filler around the actionable sentences.
        let filler = "Weekly sync notes. Attendees reviewed the roadmap and open items. ".repeat(3);
        let base = format!("{filler}I'll send the deck. Waiting on legal. {filler}");
        // first ingest: new event + extraction of the promise/open-loop candidates
        let (id1, t1, cands1) =
            db.ingest_capture(Some("com.apple.Mail"), Some("Inbox"), &base, 5).unwrap();
        assert!(!t1);
        assert_eq!(cands1.len(), 2, "one commitment + one open loop extracted");
        // near-duplicate re-read (one appended char on a long body): collapses (touch), no re-extract
        let typed = format!("{base}x");
        let (id2, t2, cands2) =
            db.ingest_capture(Some("com.apple.Mail"), Some("Inbox"), &typed, 3).unwrap();
        assert!(t2, "a near-duplicate body must dedup-touch");
        assert_eq!(id1, id2);
        assert!(cands2.is_empty(), "touch must not re-extract");
        // still exactly one commitment
        assert_eq!(db.commitments_due(1000).len(), 1);
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
    fn record_feedback_wrapper_persists_the_signal_locally() {
        use shogun_memory::lessons::{list_feedback_since, FeedbackKind, LessonScope, NewFeedback};

        let db = Db::open_in_memory(clock(1)).unwrap();
        let id = db.record_feedback(
            FeedbackKind::EditBeforeApprove,
            LessonScope::Person,
            &NewFeedback {
                ts_ms: 42,
                action_kind: Some("send_email"),
                scope_ref: Some("alice@example.com"),
                before_text: Some("proposed body"),
                after_text: Some("final body"),
                ..Default::default()
            },
        );
        assert!(id.is_some(), "a healthy DB accepts the feedback write");

        // Read it back through the same connection the wrapper wrote to.
        let rows = {
            let c = db.conn.lock().unwrap();
            list_feedback_since(&c, 0).unwrap()
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, FeedbackKind::EditBeforeApprove);
        assert_eq!(rows[0].scope, LessonScope::Person);
        assert_eq!(rows[0].scope_ref.as_deref(), Some("alice@example.com"));
        assert_eq!(rows[0].before_text.as_deref(), Some("proposed body"));
        assert_eq!(rows[0].after_text.as_deref(), Some("final body"));
        // Fire-and-forget shape: the wrapper returns Option, never Err — an approval action can
        // discard it with `let _ =` and can never be failed by it.
        let _: Option<i64> = id;
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
        assert_eq!(todo.len(), 5); // StateUpdate, ConfidenceRecalc, ColdDemotion, MorningBrief, LessonDistillation
        assert!(!todo.contains(&JobKind::Consolidation));
    }

    /// A clock the test can advance, so ledger rows land at distinct instants the way real ones do.
    fn ticking_clock() -> (Clock, Arc<std::sync::atomic::AtomicI64>) {
        let t = Arc::new(std::sync::atomic::AtomicI64::new(1_000));
        let handle = t.clone();
        (Arc::new(move || t.load(std::sync::atomic::Ordering::Relaxed)), handle)
    }

    /// FR-DC-06: the status view is rebuilt from the ledger, so it must show the same thing after a
    /// relaunch as it did live — nothing here is reported by the run that produced it.
    #[test]
    fn dream_status_reports_the_last_night_from_the_ledger() {
        let (clock, now) = ticking_clock();
        let db = Db::open_in_memory(clock).unwrap();
        let set = |t: i64| now.store(t, std::sync::atomic::Ordering::Relaxed);

        // an older night that completed fully
        for kind in crate::dreamcycle::plan::FULL_SEQUENCE {
            set(1_000);
            db.record_job("20260723", *kind, JobState::Done, 0, 100);
        }
        // last night: consolidation reached the Batch lane and failed
        set(9_000);
        db.record_job("20260724", JobKind::Consolidation, JobState::Failed, 100, 200);

        let status = db.dream_status("20260724", 7);
        let last = status.last.expect("a night has run");
        assert_eq!(last.cycle_id, "20260724");
        assert_eq!(last.kind, CycleKind::Full);
        assert!(!last.succeeded);
        assert_eq!(last.jobs_failed, 1);
        assert_eq!((last.input_from_ts, last.input_to_ts), (100, 200));
        // one failed night after a good one → amber, not red
        assert_eq!(status.indicator, crate::dreamcycle::health::Indicator::Amber);
        assert!(!status.full_run_done_today, "a failed cycle has not run today");
    }

    #[test]
    fn a_completed_night_clears_the_indicator_and_marks_today_done() {
        let (clock, now) = ticking_clock();
        let db = Db::open_in_memory(clock).unwrap();
        let set = |t: i64| now.store(t, std::sync::atomic::Ordering::Relaxed);

        set(1_000);
        db.record_job("20260723", JobKind::Consolidation, JobState::Failed, 0, 100);
        set(9_000);
        for kind in crate::dreamcycle::plan::FULL_SEQUENCE {
            db.record_job("20260724", *kind, JobState::Done, 100, 200);
        }

        let status = db.dream_status("20260724", 7);
        assert_eq!(status.indicator, crate::dreamcycle::health::Indicator::Normal);
        assert!(status.full_run_done_today, "tonight's full cycle completed");
    }

    /// A degraded catch-up does no Batch work, so it is not evidence about the Batch lane either
    /// way — it must not clear an amber indicator the failed full cycles earned.
    #[test]
    fn a_degraded_catch_up_does_not_clear_the_indicator() {
        let (clock, now) = ticking_clock();
        let db = Db::open_in_memory(clock).unwrap();
        let set = |t: i64| now.store(t, std::sync::atomic::Ordering::Relaxed);

        set(1_000);
        db.record_job("20260723", JobKind::Consolidation, JobState::Failed, 0, 100);
        set(5_000);
        for kind in DEGRADED_SEQUENCE {
            db.record_job("20260724", *kind, JobState::Done, 100, 200);
        }

        let status = db.dream_status("20260724", 7);
        assert_eq!(status.last.map(|c| c.kind), Some(CycleKind::Degraded));
        assert_eq!(status.indicator, crate::dreamcycle::health::Indicator::Amber);
        assert!(!status.full_run_done_today, "a catch-up is not the night's full cycle");
    }

    #[test]
    fn dream_status_on_a_fresh_install_is_empty_and_normal() {
        let db = Db::open_in_memory(clock(1)).unwrap();
        let status = db.dream_status("20260724", 7);
        assert!(status.last.is_none());
        assert_eq!(status.indicator, crate::dreamcycle::health::Indicator::Normal);
        assert!(!status.full_run_done_today);
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

    #[test]
    fn a_meeting_interval_opens_and_closes_through_the_db() {
        let db = Db::open_in_memory(clock(5_000)).unwrap();
        let id = db.open_meeting(Some("Weekly sync"), Some("us.zoom.xos"), 0.35, "{}").unwrap();

        assert!(db.close_meeting(id));

        let recap = db.meeting_recap(id).expect("a closed interval has a recap");
        assert_eq!(recap.title, "Weekly sync");
    }

    #[test]
    fn a_meeting_recap_carries_the_note_the_user_typed() {
        let db = Db::open_in_memory(clock(5_000)).unwrap();
        let id = db.open_meeting(Some("1:1"), None, 0.35, "{}").unwrap();

        assert!(db.save_meeting_note(id, "- discussed the roadmap"));
        db.close_meeting(id);

        assert_eq!(
            db.meeting_recap(id).unwrap().notes.as_deref(),
            Some("- discussed the roadmap")
        );
    }

    #[test]
    fn closing_a_meeting_puts_it_on_the_search_spine() {
        // FR-MT-14 at the Db seam: after close, the transcript is reachable through the SAME
        // search everything else uses, attributed to a meeting.
        let db = Db::open_in_memory(clock(5_000)).unwrap();
        let id = db.open_meeting(Some("Weekly sync"), Some("us.zoom.xos"), 0.35, "{}").unwrap();
        db.append_transcript(
            id,
            1_100,
            shogun_memory::transcript_segments::Speaker::Other,
            "the vendor renewal was settled",
            0.9,
        );

        assert!(db.search("vendor renewal", 10).is_empty(), "not findable while still open");
        assert!(db.close_meeting(id));

        let hits = db.search("vendor renewal", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, "meeting");
    }

    #[test]
    fn an_abandoned_meeting_reaches_search_when_swept() {
        // Crash / sleep: the session was never closed by the state machine. The startup sweep
        // closes it — and the transcript it captured must land on the spine then, or these become
        // the only meetings the user can read back but never find.
        let db = Db::open_in_memory(clock(5_000)).unwrap();
        let id = db.open_meeting(Some("Interrupted sync"), Some("us.zoom.xos"), 0.35, "{}").unwrap();
        db.append_transcript(
            id,
            1_100,
            shogun_memory::transcript_segments::Speaker::Other,
            "we agreed on the migration window",
            0.9,
        );

        assert_eq!(db.close_abandoned_meetings(i64::MAX), 1);
        let hits = db.search("migration window", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, "meeting");
    }

    #[test]
    fn a_note_flushed_after_close_still_reaches_search() {
        // The blur/debounce path: auto-wrap closed the session an instant before the webview
        // flushed the note. The save must refresh the index, or the note the user most wants
        // kept is the one line of the meeting that can never be found.
        let db = Db::open_in_memory(clock(5_000)).unwrap();
        let id = db.open_meeting(Some("1:1"), None, 0.35, "{}").unwrap();
        db.close_meeting(id);

        assert!(db.save_meeting_note(id, "follow up on the budget line"));
        let hits = db.search("budget line", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, "meeting");
    }

    #[test]
    fn a_recap_for_an_interval_that_never_existed_is_none() {
        let db = Db::open_in_memory(clock(5_000)).unwrap();
        assert!(db.meeting_recap(9_999).is_none());
    }

    #[test]
    fn a_meeting_with_no_note_still_has_a_recap() {
        // FR-MT-19: never an empty panel, even for a meeting where nobody typed anything.
        let db = Db::open_in_memory(clock(5_000)).unwrap();
        let id = db.open_meeting(None, Some("us.zoom.xos"), 0.35, "{}").unwrap();
        db.close_meeting(id);

        let recap = db.meeting_recap(id).unwrap();
        assert_eq!(recap.title, crate::meeting::recap::UNTITLED);
        assert_eq!(recap.notes, None);
    }

    #[test]
    fn reply_context_prefers_fetched_gmail_thread_when_screen_matches() {
        use shogun_mcp::sync::IngestItem;
        let db = Db::open_in_memory(clock(1)).unwrap();
        // gmail 同期でスレッドを入れる（thread_key は "gmail:unknown:Q3 pricing" になる）。
        db.ingest_integration(&[IngestItem {
            source: "gmail",
            kind: "email",
            title: "Q3 pricing".to_string(),
            body: "Full thread body about pricing".to_string(),
            ts_ms: 1,
        }]);
        // 画面側は capture スレッド（タブ名 "(3) Q3 pricing — Gmail"）。
        let ctx = db.build_reply_context_for_screen(
            "capture:com.google.Chrome:Q3 pricing",
            "(3) Q3 pricing — Gmail",
        );
        assert!(matches!(ctx.payload_source, PayloadSource::Fetched { .. }));
        assert!(ctx.turns.iter().any(|t| t.excerpt.contains("pricing")), "fetched body used");
    }

    #[test]
    fn reply_context_is_on_screen_only_without_a_gmail_match() {
        let db = Db::open_in_memory(clock(1)).unwrap();
        let ctx = db.build_reply_context_for_screen(
            "capture:com.apple.Safari:Nothing",
            "Nothing — Safari",
        );
        assert_eq!(ctx.payload_source, PayloadSource::OnScreenOnly);
    }

    #[test]
    fn evidence_to_blocks_preserves_provenance_and_counts_tokens() {
        use shogun_fusion::block::{BlockRef, SourceKind};
        use shogun_fusion::budget::HeuristicEstimator;
        let ev = vec![Evidence {
            event_id: 7,
            ts: 100,
            source: "capture".into(),
            title: Some("t".into()),
            excerpt: "a".repeat(40),
            frame_id: None,
        }];
        let est = HeuristicEstimator::default();
        let blocks = evidence_to_blocks(&ev, 0.7, &est);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].id_ref, BlockRef::Event(7));
        assert_eq!(blocks[0].source_kind, SourceKind::Evidence);
        assert!(blocks[0].tokens > 0);
    }

    #[test]
    fn record_compression_metric_persists_hash_not_text() {
        let db = Db::open_in_memory(clock(1_000)).unwrap();
        db.record_compression_metric("report", "compressed", 100, 30, 5, 20);
        let conn = db.conn.lock().unwrap();
        let (qh, path): (String, String) = conn
            .query_row(
                "SELECT query_hash, path FROM compression_metrics LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_ne!(qh, "report"); // 本文は保存しない
        assert_eq!(path, "compressed");
    }

    #[test]
    fn compressed_context_bounds_tokens_without_falling_back() {
        // 複数 evidence をヒットさせる小さな入力を seed する。
        let db = Db::open_in_memory(clock(10_000)).unwrap();
        for i in 0..6 {
            db.capture(&ev(
                "vendor pricing settled at 12k for the renewal report",
                &format!("h{i}"),
                100 + i,
            ))
            .unwrap();
        }
        use shogun_fusion::compress::CompressionConfig;
        let cfg = CompressionConfig { enabled: true, budget_tokens: 20, ..Default::default() };
        let (pack_c, stats, fell_back) = db.assemble_context_compressed("pricing report", 6, 600, &[], &cfg);
        assert!(!fell_back, "50ms 以内に収まるはず");
        assert!(stats.post_tokens <= 20, "post={}", stats.post_tokens);
        if stats.pre_tokens > 20 {
            assert!(stats.post_tokens < stats.pre_tokens, "pre={} post={}", stats.pre_tokens, stats.post_tokens);
        }
        assert!(!pack_c.facts.is_empty() || !pack_c.evidence.is_empty(), "全落ちしない");
    }

    /// Task 1: 圧縮パスの evidence が raw pack の source/title/ts を保持することを検証。
    #[test]
    fn compressed_evidence_preserves_source_and_title() {
        let db = Db::open_in_memory(clock(10_000)).unwrap();
        for i in 0..3 {
            db.capture(&ev("renewal report pricing detail", &format!("h{i}"), 100 + i)).unwrap();
        }
        let raw = db.assemble_context("renewal", 6, 600);
        assert!(raw.evidence.iter().any(|e| !e.source.is_empty()), "raw has source");
        let cfg = shogun_fusion::compress::CompressionConfig {
            enabled: true,
            budget_tokens: 100_000,
            ..Default::default()
        };
        let (pack, _stats, _fell) = db.assemble_context_compressed("renewal", 6, 600, &[], &cfg);
        // 予算十分＝全採用。各 evidence の source/title/ts が raw と一致（0/空でない）。
        for e in &pack.evidence {
            let orig = raw.evidence.iter().find(|o| o.event_id == e.event_id).unwrap();
            assert_eq!(e.source, orig.source, "source mismatch for event_id={}", e.event_id);
            assert_eq!(e.title, orig.title, "title mismatch for event_id={}", e.event_id);
            assert_eq!(e.ts, orig.ts, "ts mismatch for event_id={}", e.event_id);
        }
    }

    /// Task 2: `inline_memory_with_refs` が実 state row id と正しいテーブルを返し、
    /// `inline_memory`（文字列版）と内容が一致することを検証。
    #[test]
    fn inline_memory_with_refs_carries_real_ids_and_tables() {
        use shogun_fusion::block::StateTable;
        use shogun_memory::state::{
            CommitmentDirection, CommitmentStatus, NewCommitment, NewOpenLoop, OpenLoopKind,
            Provenance,
        };

        let db = Db::open_in_memory(clock(1)).unwrap();
        let e = db.capture(&ev("evidence for state", "h1", 1)).unwrap().0;
        let prov = [Provenance::new(e)];

        // 高 confidence の commitment と open_loop を 1 件ずつ挿入。
        let cid = db
            .insert_commitment(
                &NewCommitment {
                    direction: CommitmentDirection::Mine,
                    counterparty_id: None,
                    description: "send the test report",
                    due_at: None,
                    status: CommitmentStatus::Open,
                    project_id: None,
                    confidence: 0.9,
                    now: 1,
                },
                &prov,
            )
            .expect("commitment insert");

        let lid = db
            .insert_open_loop(
                &NewOpenLoop {
                    kind: OpenLoopKind::WaitingOnThem,
                    description: "waiting on test approval",
                    counterparty_id: None,
                    project_id: None,
                    opened_at: 1,
                    confidence: 0.9,
                    now: 1,
                },
                &prov,
            )
            .expect("open loop insert");

        let refs = db.inline_memory_with_refs(8);
        assert!(
            refs.iter().any(|(_, t, id)| *t == StateTable::Commitments && *id == cid),
            "commitment の実 id が含まれるべき: {refs:?}"
        );
        assert!(
            refs.iter().any(|(_, t, id)| *t == StateTable::OpenLoops && *id == lid),
            "open_loop の実 id が含まれるべき: {refs:?}"
        );

        // inline_memory（文字列版）と本文が一致（委譲による不変）。
        let strs = db.inline_memory(8);
        let refs_strs: Vec<String> = refs.iter().map(|(s, _, _)| s.clone()).collect();
        assert_eq!(strs, refs_strs, "inline_memory は inline_memory_with_refs に完全委譲するべき");
    }

    #[test]
    fn thread_summary_substitutes_for_raw_turns_under_budget() {
        use shogun_fusion::compress::CompressionConfig;

        let db = Db::open_in_memory(clock(10_000)).unwrap();
        // 同一スレッド（同 source/window_title）に長めの生ターンを複数投入。
        // 各ターンの生テキストには要約には無い語 "detail" を含めておく（差し替えの反証用）。
        for i in 0..6 {
            db.capture(&ev(
                "vendor renewal pricing discussion detail line",
                &format!("h{i}"),
                100 + i,
            ))
            .unwrap();
        }
        // そのスレッドに短い要約を付与（raw ターンより短くトークン効率が高い）。
        let tk = db
            .active_threads_between(0, 10_000)
            .first()
            .map(|t| t.thread_key.clone())
            .expect("capture が thread を作るので 1 本はあるはず");
        db.set_thread_summary(&tk, "Renewal priced at 12k; awaiting sign-off.");

        // 予算逼迫（12 トークン）: 短い要約(~10tok, relevance 0.9)が raw を押しのけて残る想定。
        let cfg = CompressionConfig { enabled: true, budget_tokens: 12, ..Default::default() };
        let (pack, stats, fell_back) =
            db.assemble_context_compressed("renewal pricing", 6, 600, std::slice::from_ref(&tk), &cfg);

        assert!(!fell_back, "ローカル組み立ては 50ms 以内で完了するはず");
        assert!(stats.post_tokens <= 12, "予算内に収まる: post={}", stats.post_tokens);

        // 要約テキストが採用され（ThreadSummary は fact 側に振り分け）、その語が生き残る。
        let joined = format!(
            "{} {}",
            pack.facts.join(" "),
            pack.evidence.iter().map(|e| e.excerpt.clone()).collect::<Vec<_>>().join(" ")
        );
        assert!(
            joined.contains("sign-off") || joined.contains("12k"),
            "要約が生き残るべき: {joined}"
        );
        // 差し替えの証明: 予算逼迫下で要約が採用された以上、要約に無い生ターンの語
        // "detail" は落ちている（要約が raw ターンを押しのけた）。
        assert!(
            !joined.contains("detail"),
            "要約が raw ターンを押しのけるべき（生ターンの語が残っている）: {joined}"
        );
    }

    #[test]
    fn session_summary_of_retrieved_evidence_reaches_the_compressed_pack() {
        use shogun_fusion::compress::CompressionConfig;

        let db = Db::open_in_memory(clock(10_000)).unwrap();
        // A meeting session, with a captured event attached to it that matches the query — so the
        // event is retrieved as evidence and its owning session's summary is pulled in.
        let sid = db.open_meeting(Some("Vendor sync"), Some("us.zoom.xos"), 0.6, "{}").unwrap();
        let (ev_id, _) = db.capture(&ev("vendor renewal pricing discussion detail line", "h0", 100)).unwrap();
        assert!(db.attach_event_to_meeting(sid, ev_id), "the event must attach to the session");
        // A short session summary, token-efficient relative to the raw turn — carries a word the raw
        // excerpt does not ("sign-off") so its arrival is unambiguous.
        db.set_session_summary(sid, "Renewal priced at 12k; awaiting sign-off.");

        // 十分予算: SessionSummary 由来テキストが pack に出る。
        let cfg = CompressionConfig { enabled: true, budget_tokens: 100_000, ..Default::default() };
        let (pack, _stats, fell_back) = db.assemble_context_compressed("renewal pricing", 6, 600, &[], &cfg);
        assert!(!fell_back);
        let joined = format!(
            "{} {}",
            pack.facts.join(" "),
            pack.evidence.iter().map(|e| e.excerpt.clone()).collect::<Vec<_>>().join(" ")
        );
        assert!(joined.contains("sign-off"), "session の要約が pack に届くべき: {joined}");

        // 予算逼迫: 短い要約(relevance 0.85)が raw ターンを押しのけて残る。
        let tight = CompressionConfig { enabled: true, budget_tokens: 12, ..Default::default() };
        let (pack_t, stats_t, fell_t) = db.assemble_context_compressed("renewal pricing", 6, 600, &[], &tight);
        assert!(!fell_t);
        assert!(stats_t.post_tokens <= 12, "予算内に収まる: post={}", stats_t.post_tokens);
        let joined_t = format!(
            "{} {}",
            pack_t.facts.join(" "),
            pack_t.evidence.iter().map(|e| e.excerpt.clone()).collect::<Vec<_>>().join(" ")
        );
        assert!(
            joined_t.contains("sign-off") || joined_t.contains("12k"),
            "予算逼迫下でも session 要約が生き残るべき: {joined_t}"
        );
    }

    #[test]
    fn session_summary_is_deduped_against_an_already_consumed_thread_summary() {
        use shogun_fusion::compress::CompressionConfig;

        let db = Db::open_in_memory(clock(10_000)).unwrap();

        // A meeting session whose event matches the query, so its owning session is on the consume
        // path. Give the session a distinctive summary word so its presence is unambiguous.
        let sid = db.open_meeting(Some("Vendor sync"), Some("us.zoom.xos"), 0.6, "{}").unwrap();
        let (ev_id, _) = db
            .capture(&ev("vendor renewal pricing discussion line", "h0", 100))
            .unwrap();
        assert!(db.attach_event_to_meeting(sid, ev_id));
        db.set_session_summary(sid, "Session recap sessionword only.");

        // The capture created a thread; use its real thread_key so set_thread_summary lands (the
        // threads UPDATE needs an existing row). The session carries that same thread_key — when it
        // is passed as a consumed ThreadSummary, the session's summary is redundant and skipped.
        let tk = db
            .active_threads_between(0, 10_000)
            .first()
            .map(|t| t.thread_key.clone())
            .expect("capture が thread を作る");
        {
            let conn = db.conn.lock().unwrap();
            conn.execute("UPDATE sessions SET thread_key = ?1 WHERE id = ?2", rusqlite::params![tk, sid])
                .unwrap();
        }
        // Give that thread a summary so it becomes a ThreadSummary candidate carrying its own word.
        db.set_thread_summary(&tk, "Thread recap threadword only.");

        let cfg = CompressionConfig { enabled: true, budget_tokens: 100_000, ..Default::default() };

        // thread_key consumed → the session's own summary is NOT re-added (dedup).
        let (pack, _s, fell) =
            db.assemble_context_compressed("renewal pricing", 6, 600, std::slice::from_ref(&tk), &cfg);
        assert!(!fell);
        let joined = pack.facts.join(" ");
        assert!(joined.contains("threadword"), "consumed thread summary is present: {joined}");
        assert!(
            !joined.contains("sessionword"),
            "session whose thread_key was consumed must be deduped out: {joined}"
        );

        // A DIFFERENT thread_key is passed (not the session's) → the session summary IS added.
        let (pack2, _s2, fell2) = db.assemble_context_compressed(
            "renewal pricing",
            6,
            600,
            &["mail:some-other-thread".to_string()],
            &cfg,
        );
        assert!(!fell2);
        let joined2 = pack2.facts.join(" ");
        assert!(
            joined2.contains("sessionword"),
            "an unconsumed session's summary is still added: {joined2}"
        );
    }
}
