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
}

/// The grounded context for one question: confidence-gated state facts plus the retrieved
/// evidence that mentions it. Facts say what SHOGUN believes; evidence says what was actually
/// seen, and only evidence can answer "what happened with X".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ContextPack {
    pub facts: Vec<String>,
    pub evidence: Vec<Evidence>,
}

/// How much of a thread a reply context carries. Enough to answer in the conversation's own
/// terms, bounded so assembly stays inside the pre-press budget.
const REPLY_TURNS: usize = 12;
const REPLY_TURN_CHARS: usize = 800;
const REPLY_RELATED: usize = 4;
const REPLY_RELATED_CHARS: usize = 300;

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

    /// The warm pack for `thread_key`, if that is the one currently held.
    pub fn get(&self, thread_key: &str) -> Option<ReplyContext> {
        let g = self.inner.lock().ok()?;
        g.as_ref().filter(|c| c.thread_key == thread_key).cloned()
    }

    /// The warm pack, whatever thread it is for.
    pub fn current(&self) -> Option<ReplyContext> {
        self.inner.lock().ok().and_then(|g| g.clone())
    }
}

/// What one local maintenance pass changed ([`Db::run_local_maintenance`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LocalMaintenance {
    pub decayed: usize,
    pub corroborated: usize,
    pub overdue: usize,
    pub stale: usize,
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
}

impl Db {
    /// Wrap an already-open, migrated connection.
    pub fn new(conn: Connection, clock: Clock) -> Self {
        Self { conn: Arc::new(Mutex::new(conn)), clock, embedder: None }
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

    /// Embed events that do not have a vector yet (FR-MEM-22: embedding is off the write path,
    /// so a slow model never delays a capture). No-op without a model. Returns how many were
    /// embedded.
    pub fn embed_pending(&self, limit: usize) -> usize {
        let Some(e) = self.embedder.as_deref() else { return 0 };
        let Ok(mut conn) = self.conn.lock() else { return 0 };
        shogun_memory::embed_job::embed_all_pending(&mut conn, e, limit).unwrap_or(0)
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
        let ev = NewEvent {
            ts: self.now_ms(),
            source: "capture",
            kind: "text",
            app_bundle_id: bundle_id,
            window_title,
            content: text,
            content_hash: "", // ignored — capture_collapsed decides it
            dwell_ms,
            display_id: None,
            window_bounds: None,
        };
        let (id, touched) = self.capture_collapsed(&ev)?;
        if touched {
            return Some((id, touched, Vec::new()));
        }
        let candidates = shogun_memory::extract::extract(text);
        let now = self.now_ms();
        let ids = {
            let mut g = self.conn.lock().ok()?;
            shogun_memory::extract::persist_candidates(&mut g, id, &candidates, now).unwrap_or_default()
        };
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
    /// persists. `newly_inserted` is what an `IntegrationSynced` bus event should report as the
    /// count. Returns a zeroed summary on a lock failure (never panics).
    pub fn ingest_integration(&self, items: &[shogun_mcp::sync::IngestItem]) -> IngestSummary {
        let now = self.now_ms();
        let Ok(mut guard) = self.conn.lock() else {
            return IngestSummary::default();
        };
        let mut summary = IngestSummary::default();
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
                continue;
            };
            summary.processed += 1;
            if touched {
                continue;
            }
            summary.newly_inserted += 1;
            // A newly-ingested item is extracted for commitments / open loops, linked to it.
            let candidates = shogun_memory::extract::extract(&it.body);
            if !candidates.is_empty() {
                let ids = shogun_memory::extract::persist_candidates(&mut guard, id, &candidates, now)
                    .unwrap_or_default();
                summary.candidates += ids.len();
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

    /// Confidence-gated memory lines for the inline draft prompt ([`crate::inline::compose_inline`]):
    /// the commitments the user owes and the open loops in play, passed through the FR-ST-20 gate
    /// (High stated as fact, Medium prefixed `possibly:`, Low dropped) so a low-confidence guess is
    /// never handed to the model as a fact. Capped at `limit` lines.
    pub fn inline_memory(&self, limit: usize) -> Vec<String> {
        let mut pairs: Vec<(String, f64)> = Vec::new();
        for c in self.commitments_due(self.now_ms()) {
            pairs.push((format!("you committed: {}", c.description), c.confidence));
        }
        for l in self.open_loops() {
            pairs.push((format!("open loop: {}", l.description), l.confidence));
        }
        let refs: Vec<(&str, f64)> = pairs.iter().map(|(s, c)| (s.as_str(), *c)).collect();
        let mut facts = shogun_fusion::confidence::assemble_facts(&refs);
        facts.truncate(limit);
        facts
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
        let Ok(mut guard) = self.conn.lock() else {
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
                summary.candidates += candidates.len();
                let _ = shogun_memory::extract::persist_candidates(&mut guard, id, &candidates, now);
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
            let Ok(conn) = self.conn.lock() else {
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
                    // The thread's own text is what a reply is grounded in, so it is kept whole
                    // up to a generous cap rather than excerpted around a query — there is no
                    // query yet.
                    excerpt: shogun_memory::search::excerpt(&content, "", REPLY_TURN_CHARS),
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
        let Ok(conn) = self.conn.lock() else {
            return ReferentOutcome::default();
        };
        let Ok(threads) = thread::recent(&conn, 20) else {
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
        let facts = self.inline_memory(8);
        let evidence = self
            .search(query, max_hits)
            .into_iter()
            .map(|h| Evidence {
                event_id: h.event_id,
                ts: h.ts,
                source: h.source,
                title: h.window_title,
                excerpt: shogun_memory::search::excerpt(&h.content, query, excerpt_chars),
            })
            .filter(|e| !e.excerpt.is_empty())
            .collect();
        ContextPack { facts, evidence }
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
    /// Raise confidence for state rows with several independent evidence events
    /// ([`shogun_memory::recompute::corroborate`]). Part of the local maintenance pass.
    pub fn corroborate(&self) -> usize {
        self.conn
            .lock()
            .ok()
            .and_then(|mut g| shogun_memory::recompute::corroborate(&mut g).ok())
            .unwrap_or(0)
    }

    /// The apps SHOGUN has actually captured from, most-seen first.
    ///
    /// The exclusion UI offers these rather than asking for bundle identifiers: a person can
    /// recognise "the app I was just in", not `com.acme.thing`.
    pub fn captured_apps(&self, limit: usize) -> Vec<(String, i64)> {
        let Ok(conn) = self.conn.lock() else { return Vec::new() };
        let Ok(mut stmt) = conn.prepare(
            "SELECT app_bundle_id, count(*) FROM event_log
              WHERE source = 'capture' AND app_bundle_id IS NOT NULL AND app_bundle_id <> ''
              GROUP BY app_bundle_id ORDER BY count(*) DESC LIMIT ?1",
        ) else {
            return Vec::new();
        };
        stmt.query_map([limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
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
        let mut guard = self.conn.lock().ok()?;
        shogun_memory::identity::observe(&mut guard, incoming, seen_name, event_id, now).ok()
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
        let decayed = self.decay_confidence(now_ms, half_life_ms);
        let corroborated = self.corroborate();
        let (overdue, stale) = self.recompute_overdue_and_staleness(now_ms);
        LocalMaintenance { decayed, corroborated, overdue, stale }
    }

    pub fn decay_confidence(&self, now_ms: i64, half_life_ms: i64) -> usize {
        self.conn
            .lock()
            .ok()
            .and_then(|mut g| shogun_memory::recompute::decay_confidence(&mut g, now_ms, half_life_ms).ok())
            .unwrap_or(0)
    }

    /// The high-water mark of already-consolidated events (max `input_to_ts` of completed
    /// consolidations) — the scheduler's next window starts here (FR-DC-04). `None` before any cycle.
    pub fn last_consolidated_to(&self) -> Option<i64> {
        self.conn.lock().ok().and_then(|c| shogun_memory::jobs::last_consolidated_to(&c).ok()).flatten()
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

        for c in self.conn.lock().ok().and_then(|c| state::list_commitments(&c).ok()).unwrap_or_default() {
            let summary = c.description.clone();
            states.push(StateCandidate {
                kind: StateKind::CommitmentMine,
                relevance: rel(&summary, &summary),
                subject: summary.clone(),
                summary,
                confidence: c.confidence,
            });
        }
        for l in self.conn.lock().ok().and_then(|c| state::list_open_loops(&c).ok()).unwrap_or_default() {
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

        assemble(screen, &states, "", &Intent { hint: intent_hint })
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
        let rows = self.conn.lock().ok().and_then(|c| state::list_commitments(&c).ok()).unwrap_or_default();
        rows.into_iter()
            .filter(|r| r.status != "done" && r.status != "cancelled")
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

    /// All commitment rows with ids (panel list — the UI needs the id to resolve a row).
    pub fn commitment_rows(&self) -> Vec<state::CommitmentRow> {
        self.conn.lock().ok().and_then(|c| state::list_commitments(&c).ok()).unwrap_or_default()
    }

    /// All open-loop rows with ids (panel list).
    pub fn open_loop_rows(&self) -> Vec<state::OpenLoopRow> {
        self.conn.lock().ok().and_then(|c| state::list_open_loops(&c).ok()).unwrap_or_default()
    }

    /// Mark a commitment done (user resolved it from the panel). `true` if a row changed.
    pub fn resolve_commitment(&self, id: i64) -> bool {
        let now = self.now_ms();
        self.conn
            .lock()
            .ok()
            .and_then(|c| state::set_commitment_status(&c, id, state::CommitmentStatus::Done, now).ok())
            .is_some_and(|n| n > 0)
    }

    /// Close an open loop (user resolved it from the panel). `true` if a row changed.
    pub fn resolve_open_loop(&self, id: i64) -> bool {
        let now = self.now_ms();
        self.conn
            .lock()
            .ok()
            .and_then(|c| state::close_open_loop(&c, id, now).ok())
            .is_some_and(|n| n > 0)
    }

    /// Delete all extracted state (commitments + open loops + their provenance). Event log,
    /// people, and projects are untouched. `true` on success.
    pub fn clear_state(&self) -> bool {
        self.conn.lock().ok().and_then(|mut c| state::clear_state(&mut c).ok()).is_some()
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
        self.conn
            .lock()
            .ok()
            .and_then(|c| {
                shogun_memory::search::search_warm_first(&c, query, query_vec.as_deref(), now, limit)
                    .ok()
            })
            .unwrap_or_default()
    }

    /// Open loops as Fusion/Brief input (stalest first; the Brief caps the count). Closed loops
    /// are excluded so resolving one from the panel removes it everywhere (memory, counts, Brief).
    pub fn open_loops(&self) -> Vec<OpenLoopItem> {
        let rows = self.conn.lock().ok().and_then(|c| state::list_open_loops(&c).ok()).unwrap_or_default();
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
            .conn
            .lock()
            .ok()
            .map(|c| {
                (
                    event_log::count_in_range(&c, input_from_ts, input_to_ts).unwrap_or(0),
                    state::count_changed_since(&c, run_started_ms).unwrap_or(0),
                    shogun_memory::traceability::count_since(&c, run_started_ms).unwrap_or(0),
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
        assert!(cache.actions.iter().all(|a| a.level != shogun_fusion::Level::L3),
            "v1 context actions are local (L1/L2) — no external sends (invariant 4)");
        assert!(cache.facts.iter().any(|f| f.contains("roadmap")), "gated fact present");
        assert!(!cache.facts.iter().any(|f| f.contains("vague")), "low-confidence fact excluded");
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
