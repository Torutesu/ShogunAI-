//! L5 lessons / patterns (docs/layer-completion-designs.md §5, Plan D).
//!
//! The compounding loop: approval-time corrections are recorded as `feedback_events`, a pure
//! rule distiller ([`distill`]) turns repeated same-direction corrections into one-sentence
//! English instructions, and [`upsert_lesson`] persists them with the state-table discipline —
//! provenance mandatory, confidence mandatory, low confidence never injected.
//!
//! Confidence lifecycle mirrors `recompute.rs`:
//! - **Corroboration**: [`crate::recompute::corroborated_confidence`] (the same curve, reused,
//!   not reinvented) maps evidence count to confidence, ceilinged below the High band — a
//!   locally-distilled lesson can be *offered*, never *asserted*.
//! - **Decay**: confidence is **derived**, not accumulated. There is no `base_confidence`
//!   column here because the base is a function of `evidence_count` (the corroboration curve):
//!   nothing else ever raises a lesson in v1, so `decay_and_deactivate` recomputes the absolute
//!   value from `evidence_count` × elapsed silence — running it twice at the same `now` changes
//!   nothing, and fresh evidence (which moves `last_evidence_at`) restores the row.
//!
//! Privacy: `before_text` / `after_text` are local-DB-only. Nothing in this module logs them,
//! and [`FeedbackRow`]'s `Debug` is redacted so the content cannot reach a log through a derived
//! `{:?}` on some enclosing struct (CLAUDE.md: capture content never goes to logs or telemetry).

use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::recompute::corroborated_confidence;
use crate::MemoryError;

// ---------------------------------------------------------------- constants

/// Same-direction occurrences a local rule needs before it may emit a candidate. Two matching
/// corrections can be coincidence; three is a pattern (Plan D-4: "3回以上同方向で発火").
pub const MIN_RULE_OCCURRENCES: usize = 3;

/// Length reduction (fraction of `before`) that counts as a deliberate shortening.
const SHORTEN_RATIO: f64 = 0.30;

/// Confidence a lesson is born with. 0.5 is the Medium floor: a distilled lesson already rests
/// on [`MIN_RULE_OCCURRENCES`] same-direction corrections, so it starts *offerable* — and the
/// corroboration ceiling (0.75, see `recompute.rs`) keeps it below High forever: no local rule
/// may produce an instruction that gets stated as fact.
pub const LESSON_BASE_CONFIDENCE: f64 = 0.5;

/// Injection floor for [`active_lessons`]. Mirrors the Low/Medium boundary in
/// `crates/shogun-fusion/src/confidence.rs` (`band()`: confidence < 0.5 is Low, excluded from
/// generations entirely). Duplicated by value because the dependency points the other way —
/// fusion depends on memory, so memory cannot import fusion's constant.
pub const INJECTION_FLOOR: f64 = 0.5;

/// A lesson whose derived confidence falls below this sleeps (`active = 0`): half the injection
/// floor, i.e. long past the point where it stopped being injected anyway.
pub const DEACTIVATION_FLOOR: f64 = 0.25;

/// Maximum simultaneously active lessons (designs §5.3). Bounded so the Learned list stays
/// reviewable and the prompt budget can never be flooded; the weakest sleep first.
pub const ACTIVE_LESSON_CAP: usize = 50;

/// Half-life of an un-evidenced lesson, matching the state-table maintenance cadence: one month
/// of silence halves it.
pub const LESSON_HALF_LIFE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

// ---------------------------------------------------------------- enums

/// Feedback signal kinds (the `feedback_events.kind` CHECK).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackKind {
    EditBeforeApprove,
    Reject,
    ApproveUnchanged,
    StateResolve,
    Undo,
}

impl FeedbackKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FeedbackKind::EditBeforeApprove => "edit_before_approve",
            FeedbackKind::Reject => "reject",
            FeedbackKind::ApproveUnchanged => "approve_unchanged",
            FeedbackKind::StateResolve => "state_resolve",
            FeedbackKind::Undo => "undo",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "edit_before_approve" => FeedbackKind::EditBeforeApprove,
            "reject" => FeedbackKind::Reject,
            "approve_unchanged" => FeedbackKind::ApproveUnchanged,
            "state_resolve" => FeedbackKind::StateResolve,
            "undo" => FeedbackKind::Undo,
            _ => return None,
        })
    }

    /// Whether this kind is a *decision on a proposal SHOGUN made*. `state_resolve` is not: the
    /// user resolved a state record directly, with no proposal in front of them, so counting it
    /// would inflate "how often did I decide on SHOGUN's suggestions" with work SHOGUN never
    /// suggested.
    pub fn is_action_decision(self) -> bool {
        !matches!(self, FeedbackKind::StateResolve)
    }

    /// Whether the decision *took* the proposal. Editing before approving still counts: the
    /// proposal was the starting point and it shipped. `Undo` does not — the user took it back.
    pub fn is_adoption(self) -> bool {
        matches!(self, FeedbackKind::EditBeforeApprove | FeedbackKind::ApproveUnchanged)
    }
}

/// Every feedback kind, so a SQL predicate built from [`FeedbackKind::is_action_decision`] /
/// [`FeedbackKind::is_adoption`] cannot silently miss a variant added later.
pub const ALL_FEEDBACK_KINDS: &[FeedbackKind] = &[
    FeedbackKind::EditBeforeApprove,
    FeedbackKind::Reject,
    FeedbackKind::ApproveUnchanged,
    FeedbackKind::StateResolve,
    FeedbackKind::Undo,
];

/// Where a lesson applies (the `scope` CHECK on both tables). `scope_ref` names the app bundle
/// id / person id / project id; `Global` carries no ref.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LessonScope {
    Global,
    App,
    Person,
    Project,
}

impl LessonScope {
    pub fn as_str(self) -> &'static str {
        match self {
            LessonScope::Global => "global",
            LessonScope::App => "app",
            LessonScope::Person => "person",
            LessonScope::Project => "project",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "global" => LessonScope::Global,
            "app" => LessonScope::App,
            "person" => LessonScope::Person,
            "project" => LessonScope::Project,
            _ => return None,
        })
    }
}

/// Lesson categories (the `lessons.kind` CHECK).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LessonKind {
    Style,
    Preference,
    Correction,
    Pattern,
}

impl LessonKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LessonKind::Style => "style",
            LessonKind::Preference => "preference",
            LessonKind::Correction => "correction",
            LessonKind::Pattern => "pattern",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "style" => LessonKind::Style,
            "preference" => LessonKind::Preference,
            "correction" => LessonKind::Correction,
            "pattern" => LessonKind::Pattern,
            _ => return None,
        })
    }
}

/// The stored strings pass the schema CHECKs on write, so an unparseable value on read means the
/// database was edited out-of-band — surfaced as a conversion error, never a panic.
fn parse_err(what: &str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        format!("unknown {what}: {value:?}").into(),
    )
}

// ---------------------------------------------------------------- feedback recording

/// Where a decision happened (the V19 `feedback_events.surface` CHECK).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Notch,
    OptionKey,
    Chat,
    Recap,
    Api,
}

/// Every surface, so a parser or settings UI stays exhaustive by construction.
pub const ALL_SURFACES: &[Surface] =
    &[Surface::Notch, Surface::OptionKey, Surface::Chat, Surface::Recap, Surface::Api];

impl Surface {
    pub fn as_str(self) -> &'static str {
        match self {
            Surface::Notch => "notch",
            Surface::OptionKey => "option_key",
            Surface::Chat => "chat",
            Surface::Recap => "recap",
            Surface::Api => "api",
        }
    }

    /// Parse a wire name (the UI layer speaks strings). Unknown names are rejected rather than
    /// defaulted: a typo that silently became "notch" would quietly corrupt the very statistics
    /// this column exists to produce.
    pub fn from_wire(s: &str) -> Option<Surface> {
        ALL_SURFACES.iter().copied().find(|v| v.as_str() == s)
    }
}

/// A new feedback signal (struct-parameter style shared with `event_log::NewEvent`).
///
/// The offer-context fields (V19) are all optional and default to `None` — "not recorded" is a
/// real answer here, and a caller that cannot say where a decision came from should say nothing
/// rather than pick a plausible surface.
#[derive(Clone, Default)]
pub struct NewFeedback<'a> {
    pub ts_ms: i64,
    pub action_kind: Option<&'a str>,
    pub scope_ref: Option<&'a str>,
    pub before_text: Option<&'a str>,
    pub after_text: Option<&'a str>,
    /// Where the proposal was shown.
    pub surface: Option<Surface>,
    /// The candidate's slot when offered (0 = top). `None` when the surface does not rank.
    pub rank: Option<i64>,
    /// Frontmost bundle id at decision time — context, not content.
    pub context_app: Option<&'a str>,
    /// Offer → decision in ms.
    pub latency_ms: Option<i64>,
}

/// Record one feedback signal (D-2 hooks call this from the approval commands). Returns the new
/// row id. The text stays in the local DB: no egress path exists for these tables, and nothing
/// here logs it.
pub fn record_feedback(
    conn: &Connection,
    kind: FeedbackKind,
    scope: LessonScope,
    f: &NewFeedback<'_>,
) -> Result<i64, rusqlite::Error> {
    conn.execute(
        "INSERT INTO feedback_events
           (ts_ms, kind, action_kind, scope, scope_ref, before_text, after_text,
            surface, rank, context_app, latency_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            f.ts_ms,
            kind.as_str(),
            f.action_kind,
            scope.as_str(),
            f.scope_ref,
            f.before_text,
            f.after_text,
            f.surface.map(Surface::as_str),
            f.rank,
            f.context_app,
            f.latency_ms,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// One feedback event read back for distillation.
#[derive(Clone)]
pub struct FeedbackRow {
    pub id: i64,
    pub ts_ms: i64,
    pub kind: FeedbackKind,
    pub action_kind: Option<String>,
    pub scope: LessonScope,
    pub scope_ref: Option<String>,
    pub before_text: Option<String>,
    pub after_text: Option<String>,
}

/// `before_text` / `after_text` are the user's words — they must not reach a log through a
/// `{:?}` render, so `Debug` shows only presence.
impl std::fmt::Debug for FeedbackRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeedbackRow")
            .field("id", &self.id)
            .field("ts_ms", &self.ts_ms)
            .field("kind", &self.kind)
            .field("action_kind", &self.action_kind)
            .field("scope", &self.scope)
            .field("scope_ref", &self.scope_ref)
            .field("before_text", &self.before_text.as_ref().map(|_| "***redacted***"))
            .field("after_text", &self.after_text.as_ref().map(|_| "***redacted***"))
            .finish()
    }
}

fn feedback_from_row(r: &rusqlite::Row<'_>) -> Result<FeedbackRow, rusqlite::Error> {
    let kind_s: String = r.get(2)?;
    let scope_s: String = r.get(4)?;
    Ok(FeedbackRow {
        id: r.get(0)?,
        ts_ms: r.get(1)?,
        kind: FeedbackKind::parse(&kind_s).ok_or_else(|| parse_err("feedback kind", &kind_s))?,
        action_kind: r.get(3)?,
        scope: LessonScope::parse(&scope_s).ok_or_else(|| parse_err("scope", &scope_s))?,
        scope_ref: r.get(5)?,
        before_text: r.get(6)?,
        after_text: r.get(7)?,
    })
}

/// List feedback events at or after `since_ts_ms`, oldest first — the distillation job's input.
pub fn list_feedback_since(
    conn: &Connection,
    since_ts_ms: i64,
) -> Result<Vec<FeedbackRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, ts_ms, kind, action_kind, scope, scope_ref, before_text, after_text
         FROM feedback_events WHERE ts_ms >= ?1 ORDER BY ts_ms, id",
    )?;
    let rows = stmt.query_map([since_ts_ms], feedback_from_row)?;
    rows.collect()
}

/// List feedback events with id strictly greater than `after_id`, in id order — the
/// watermark-driven variant of [`list_feedback_since`] the LessonDistillation Dream job reads
/// (Plan D-4: "unprocessed feedback" = everything above the stored watermark).
pub fn list_feedback_after(
    conn: &Connection,
    after_id: i64,
) -> Result<Vec<FeedbackRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, ts_ms, kind, action_kind, scope, scope_ref, before_text, after_text
         FROM feedback_events WHERE id > ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([after_id], feedback_from_row)?;
    rows.collect()
}

/// Count feedback events at or after `since_ts_ms` — the D-6 `feedback_events_last_7d` counter.
/// A count only: no text leaves the table.
pub fn count_feedback_since(conn: &Connection, since_ts_ms: i64) -> Result<i64, rusqlite::Error> {
    conn.query_row("SELECT count(*) FROM feedback_events WHERE ts_ms >= ?1", [since_ts_ms], |r| {
        r.get(0)
    })
}

/// Decisions on SHOGUN's proposals since `since_ts_ms`, and how many of them adopted the
/// proposal — the Evening Wrap's "Today's outcome" counts (§6.17, FR-EB-01).
///
/// Reads `feedback_events`, the table that already records approval-time decisions, rather than
/// a second parallel ledger: two tables counting the same act would drift, and the one the user
/// sees in the Wrap must be the one the lessons learn from.
///
/// Counts only. No `before_text` / `after_text` is read here, and none can be — those columns are
/// local-only user content (V16's rule).
pub fn decision_counts_since(
    conn: &Connection,
    since_ts_ms: i64,
) -> Result<(i64, i64), rusqlite::Error> {
    // Both lists are built from the enum, so a kind added later is a compile-time decision about
    // which bucket it falls in rather than a silently-wrong count. The values are `as_str()`
    // literals — never caller input — so the inlined IN lists cannot carry SQL from outside.
    let quoted = |f: &dyn Fn(FeedbackKind) -> bool| -> String {
        ALL_FEEDBACK_KINDS
            .iter()
            .copied()
            .filter(|&k| f(k))
            .map(|k| format!("'{}'", k.as_str()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let decisions = quoted(&FeedbackKind::is_action_decision);
    let adoptions = quoted(&FeedbackKind::is_adoption);
    conn.query_row(
        &format!(
            "SELECT count(*),
                    sum(CASE WHEN kind IN ({adoptions}) THEN 1 ELSE 0 END)
             FROM feedback_events
             WHERE ts_ms >= ?1 AND kind IN ({decisions})"
        ),
        [since_ts_ms],
        // `sum()` over zero rows is NULL, not 0 — count and sum disagree on the empty case.
        |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0))),
    )
}

/// Adoption rate per action kind since `since_ts_ms`: `(kind, decided, adopted)`, most-decided
/// first. This is FR-CF-03's "recent adoption rate of this action kind" supply.
///
/// The caller owns the window AND the smoothing. A kind with one decision has an adoption rate of
/// either 0% or 100%, and neither number should be allowed to swing a ranking — so this returns
/// the two counts rather than a ratio, leaving the scorer no way to use the rate without also
/// seeing how thin the evidence behind it is.
///
/// Rows with no `action_kind` are excluded: they are decisions about state, not about a proposed
/// action, and they have no kind whose adoption rate this could be.
pub fn acceptance_by_kind(
    conn: &Connection,
    since_ts_ms: i64,
) -> Result<Vec<(String, i64, i64)>, rusqlite::Error> {
    let adoptions = ALL_FEEDBACK_KINDS
        .iter()
        .copied()
        .filter(|&k| k.is_adoption())
        .map(|k| format!("'{}'", k.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    let decisions = ALL_FEEDBACK_KINDS
        .iter()
        .copied()
        .filter(|&k| k.is_action_decision())
        .map(|k| format!("'{}'", k.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    let mut stmt = conn.prepare(&format!(
        "SELECT action_kind,
                count(*),
                sum(CASE WHEN kind IN ({adoptions}) THEN 1 ELSE 0 END)
         FROM feedback_events
         WHERE ts_ms >= ?1 AND action_kind IS NOT NULL AND kind IN ({decisions})
         GROUP BY action_kind
         ORDER BY count(*) DESC, action_kind"
    ))?;
    let rows = stmt.query_map([since_ts_ms], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0)))
    })?;
    rows.collect()
}

// ---------------------------------------------------------------- distillation watermark (D-4)

/// The distillation watermark: the highest `feedback_events.id` a completed LessonDistillation
/// pass has consumed (V17 single-row meta table, 0 = nothing processed yet). The job reads
/// strictly above it via [`list_feedback_after`] and advances it via [`set_distill_watermark`]
/// only after its upserts land, so a crash between the two re-reads the same window — safe,
/// because [`upsert_lesson`] dedupes already-linked evidence.
pub fn distill_watermark(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("SELECT last_processed_feedback_id FROM lesson_distill_meta WHERE id = 1", [], |r| {
        r.get(0)
    })
}

/// Advance the distillation watermark (see [`distill_watermark`]). Monotonic: a smaller value
/// never rewinds it, so a stale re-run cannot cause later passes to re-consume old feedback.
pub fn set_distill_watermark(
    conn: &Connection,
    last_processed_feedback_id: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE lesson_distill_meta
         SET last_processed_feedback_id = MAX(last_processed_feedback_id, ?1) WHERE id = 1",
        [last_processed_feedback_id],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- lessons: upsert

/// A distilled lesson candidate: one prompt-injectable English sentence plus the feedback events
/// that evidence it (fed to [`upsert_lesson`] as provenance).
#[derive(Debug, Clone, PartialEq)]
pub struct LessonCandidate {
    pub kind: LessonKind,
    pub scope: LessonScope,
    pub scope_ref: Option<String>,
    pub instruction: String,
    /// Supporting `feedback_events.id`s, ascending.
    pub evidence: Vec<i64>,
}

/// The identity key for merging: same scope + same instruction up to whitespace and case.
/// Instructions come from fixed templates, so this catches re-distillation of the same pattern
/// across runs without a fuzzy match.
fn normalize_instruction(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Insert a lesson or merge into an existing one (designs §5.3: 同義lessonsはマージ).
///
/// Match: same `(scope, scope_ref)` and normalized-equal instruction. On match, newly-linked
/// evidence raises `evidence_count` and confidence follows the corroboration curve from
/// `recompute.rs` — monotonically (never lowered here; lowering is decay's job) and bounded by
/// that curve's below-High ceiling. On miss, the lesson is born at [`LESSON_BASE_CONFIDENCE`]
/// or the curve's value for its evidence, whichever is higher. Provenance rows are written in
/// both cases, in the same transaction; empty evidence is rejected before any write (the same
/// FR-ST-02 discipline as state rows). A lesson the user switched off stays off — new evidence
/// updates its bookkeeping but never overrides an explicit OFF.
///
/// Returns the lesson id (existing on merge, new on insert).
pub fn upsert_lesson(
    conn: &mut Connection,
    candidate: &LessonCandidate,
    evidence_ids: &[i64],
    now: i64,
) -> Result<i64, MemoryError> {
    if evidence_ids.is_empty() {
        return Err(MemoryError::EmptyProvenance);
    }
    let tx = conn.transaction()?;

    // Find a same-scope lesson whose instruction normalizes equal. The normalized compare runs
    // in Rust because SQLite's lower() is ASCII-only.
    let wanted = normalize_instruction(&candidate.instruction);
    let existing: Option<(i64, i64)> = {
        let mut stmt = tx.prepare(
            "SELECT id, instruction, evidence_count FROM lessons
             WHERE scope = ?1 AND scope_ref IS ?2",
        )?;
        let rows = stmt.query_map(
            params![candidate.scope.as_str(), candidate.scope_ref.as_deref()],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)),
        )?;
        let mut found = None;
        for row in rows {
            let (id, instruction, count) = row?;
            if normalize_instruction(&instruction) == wanted {
                found = Some((id, count));
                break;
            }
        }
        found
    };

    let id = match existing {
        Some((id, old_count)) => {
            // Link evidence first; only genuinely new links count (the composite PK dedupes),
            // so re-running a distillation pass over the same events raises nothing.
            let mut newly_linked = 0i64;
            for &event_id in evidence_ids {
                newly_linked += tx.execute(
                    "INSERT OR IGNORE INTO lesson_provenance (lesson_id, feedback_event_id)
                     VALUES (?1, ?2)",
                    params![id, event_id],
                )? as i64;
            }
            let new_count = old_count + newly_linked;
            // Same corroboration curve as state rows; MAX() keeps the raise monotonic and the
            // curve's ceiling keeps it below High (and so ≤ 1) forever.
            let target = corroborated_confidence(new_count as f64)
                .max(LESSON_BASE_CONFIDENCE)
                .clamp(0.0, 1.0);
            tx.execute(
                "UPDATE lessons SET evidence_count = ?2, confidence = MAX(confidence, ?3),
                                    updated_at = ?4, last_evidence_at = ?4
                 WHERE id = ?1",
                params![id, new_count, target, now],
            )?;
            id
        }
        None => {
            let mut seen = evidence_ids.to_vec();
            seen.sort_unstable();
            seen.dedup();
            let confidence = corroborated_confidence(seen.len() as f64)
                .max(LESSON_BASE_CONFIDENCE)
                .clamp(0.0, 1.0);
            tx.execute(
                "INSERT INTO lessons
                   (kind, scope, scope_ref, instruction, confidence, evidence_count,
                    active, created_at, updated_at, last_evidence_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7, ?7)",
                params![
                    candidate.kind.as_str(),
                    candidate.scope.as_str(),
                    candidate.scope_ref.as_deref(),
                    candidate.instruction,
                    confidence,
                    seen.len() as i64,
                    now,
                ],
            )?;
            let id = tx.last_insert_rowid();
            for event_id in seen {
                tx.execute(
                    "INSERT OR IGNORE INTO lesson_provenance (lesson_id, feedback_event_id)
                     VALUES (?1, ?2)",
                    params![id, event_id],
                )?;
            }
            id
        }
    };
    tx.commit()?;
    Ok(id)
}

// ---------------------------------------------------------------- lifecycle

/// What one [`decay_and_deactivate`] pass did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LifecycleOutcome {
    /// Rows whose confidence moved.
    pub decayed: usize,
    /// Rows put to sleep by an opposing newer lesson (counter-direction feedback stream).
    pub contradicted: usize,
    /// Rows put to sleep for confidence below [`DEACTIVATION_FLOOR`].
    pub below_floor: usize,
    /// Rows put to sleep to enforce [`ACTIVE_LESSON_CAP`].
    pub over_cap: usize,
}

/// Age, contradict, and cap lessons — the maintenance pass a later Dream Cycle job will run.
///
/// 1. **Decay** (all rows, sleeping included): confidence is recomputed as the corroboration
///    curve's value for `evidence_count`, halved per [`LESSON_HALF_LIFE_MS`] of silence since
///    `last_evidence_at` — the derived-not-accumulated shape from `recompute.rs`, so the pass is
///    idempotent for a given `now` and compounds nothing however often it runs.
/// 2. **Contradiction**: two active lessons in the same `(scope, scope_ref)` whose instructions
///    are template opposites ([`opposes`]) are a counter-direction feedback stream — the one
///    with the older `last_evidence_at` sleeps; the user's current behavior wins.
/// 3. **Floor**: active lessons below [`DEACTIVATION_FLOOR`] sleep.
/// 4. **Cap**: if more than [`ACTIVE_LESSON_CAP`] remain active, the weakest (lowest confidence,
///    then oldest evidence) sleep until the cap holds.
pub fn decay_and_deactivate(
    conn: &mut Connection,
    now_ms: i64,
) -> Result<LifecycleOutcome, rusqlite::Error> {
    let mut outcome = LifecycleOutcome::default();
    let tx = conn.transaction()?;

    // 1. decay — absolute recompute from evidence_count + elapsed silence.
    let rows: Vec<(i64, f64, i64, i64)> = {
        let mut stmt =
            tx.prepare("SELECT id, confidence, evidence_count, last_evidence_at FROM lessons")?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        mapped.collect::<Result<Vec<_>, _>>()?
    };
    for (id, confidence, evidence_count, last_evidence_at) in rows {
        let elapsed = (now_ms - last_evidence_at).max(0) as f64;
        let base = corroborated_confidence(evidence_count as f64).max(LESSON_BASE_CONFIDENCE);
        let target =
            (base * 0.5_f64.powf(elapsed / LESSON_HALF_LIFE_MS as f64)).clamp(0.0, 1.0);
        if (target - confidence).abs() > 1e-9 {
            tx.execute(
                "UPDATE lessons SET confidence = ?1, updated_at = ?2 WHERE id = ?3",
                params![target, now_ms, id],
            )?;
            outcome.decayed += 1;
        }
    }

    // 2. contradiction — the older of an opposing active pair sleeps.
    let active: Vec<(i64, String, Option<String>, String, f64, i64)> = {
        let mut stmt = tx.prepare(
            "SELECT id, scope, scope_ref, instruction, confidence, last_evidence_at
             FROM lessons WHERE active = 1 ORDER BY id",
        )?;
        let mapped = stmt.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })?;
        mapped.collect::<Result<Vec<_>, _>>()?
    };
    let mut to_sleep: Vec<i64> = Vec::new();
    for (i, a) in active.iter().enumerate() {
        for b in active.iter().skip(i + 1) {
            if a.1 == b.1 && a.2 == b.2 && opposes(&a.3, &b.3) {
                // Older evidence loses; ties break toward keeping the more confident row.
                let a_loses = (a.5, a.4, std::cmp::Reverse(a.0)) < (b.5, b.4, std::cmp::Reverse(b.0));
                to_sleep.push(if a_loses { a.0 } else { b.0 });
            }
        }
    }
    to_sleep.sort_unstable();
    to_sleep.dedup();
    for id in to_sleep {
        outcome.contradicted +=
            tx.execute("UPDATE lessons SET active = 0, updated_at = ?1 WHERE id = ?2 AND active = 1",
                params![now_ms, id])?;
    }

    // 3. floor.
    outcome.below_floor = tx.execute(
        "UPDATE lessons SET active = 0, updated_at = ?1 WHERE active = 1 AND confidence < ?2",
        params![now_ms, DEACTIVATION_FLOOR],
    )?;

    // 4. cap — weakest first.
    let active_count: i64 =
        tx.query_row("SELECT count(*) FROM lessons WHERE active = 1", [], |r| r.get(0))?;
    let excess = active_count - ACTIVE_LESSON_CAP as i64;
    if excess > 0 {
        outcome.over_cap = tx.execute(
            "UPDATE lessons SET active = 0, updated_at = ?1 WHERE id IN (
                SELECT id FROM lessons WHERE active = 1
                ORDER BY confidence ASC, last_evidence_at ASC, id ASC LIMIT ?2)",
            params![now_ms, excess],
        )?;
    }

    tx.commit()?;
    Ok(outcome)
}

// ---------------------------------------------------------------- reads (injection supply)

/// A lesson read back for the Learned UI / prompt injection.
#[derive(Debug, Clone, PartialEq)]
pub struct Lesson {
    pub id: i64,
    pub kind: LessonKind,
    pub scope: LessonScope,
    pub scope_ref: Option<String>,
    pub instruction: String,
    pub confidence: f64,
    pub evidence_count: i64,
    pub active: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_evidence_at: i64,
}

/// One scope the caller is currently in (the app in focus, the counterparty, the project).
/// `scope_ref: None` matches every ref within the scope kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeFilter<'a> {
    pub scope: LessonScope,
    pub scope_ref: Option<&'a str>,
}

impl ScopeFilter<'_> {
    fn matches(&self, lesson: &Lesson) -> bool {
        lesson.scope == self.scope
            && match self.scope_ref {
                None => true,
                Some(wanted) => lesson.scope_ref.as_deref() == Some(wanted),
            }
    }
}

/// The lessons eligible for injection right now: `active = 1`, confidence at or above
/// [`INJECTION_FLOOR`] (the fusion Low-band gate), matching any of `scopes` (empty = no scope
/// restriction), strongest first, at most `top_k`.
pub fn active_lessons(
    conn: &Connection,
    scopes: &[ScopeFilter<'_>],
    top_k: usize,
) -> Result<Vec<Lesson>, rusqlite::Error> {
    // "at most `top_k`" includes zero: the cap below is only checked after a push, so without this
    // a `top_k` of 0 would never be hit and every lesson would be injected.
    if top_k == 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT id, kind, scope, scope_ref, instruction, confidence, evidence_count,
                active, created_at, updated_at, last_evidence_at
         FROM lessons
         WHERE active = 1 AND confidence >= ?1
         ORDER BY confidence DESC, last_evidence_at DESC, id",
    )?;
    let rows = stmt.query_map([INJECTION_FLOOR], lesson_from_row)?;
    let mut out = Vec::new();
    for row in rows {
        let lesson = row?;
        if scopes.is_empty() || scopes.iter().any(|s| s.matches(&lesson)) {
            out.push(lesson);
            if out.len() == top_k {
                break;
            }
        }
    }
    Ok(out)
}

/// The shared column order for lesson reads:
/// `id, kind, scope, scope_ref, instruction, confidence, evidence_count, active, created_at,
/// updated_at, last_evidence_at`.
fn lesson_from_row(r: &rusqlite::Row<'_>) -> Result<Lesson, rusqlite::Error> {
    let kind_s: String = r.get(1)?;
    let scope_s: String = r.get(2)?;
    Ok(Lesson {
        id: r.get(0)?,
        kind: LessonKind::parse(&kind_s).ok_or_else(|| parse_err("lesson kind", &kind_s))?,
        scope: LessonScope::parse(&scope_s).ok_or_else(|| parse_err("scope", &scope_s))?,
        scope_ref: r.get(3)?,
        instruction: r.get(4)?,
        confidence: r.get(5)?,
        evidence_count: r.get(6)?,
        active: r.get::<_, i64>(7)? != 0,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
        last_evidence_at: r.get(10)?,
    })
}

/// Every lesson row, sleeping included, strongest first — the Learned UI / `lessons.list` supply
/// (invariant 6: the API sees the same rows the human list shows). Carries instructions and
/// bookkeeping only — never `feedback_events` text.
pub fn list_lessons(conn: &Connection) -> Result<Vec<Lesson>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, scope, scope_ref, instruction, confidence, evidence_count,
                active, created_at, updated_at, last_evidence_at
         FROM lessons
         ORDER BY active DESC, confidence DESC, last_evidence_at DESC, id",
    )?;
    let rows = stmt.query_map([], lesson_from_row)?;
    rows.collect()
}

/// Flip one lesson's `active` switch (`lessons.set_active` / the Learned UI toggle). Returns
/// `false` when no such lesson exists. An OFF here is the user's explicit choice — the lifecycle
/// never turns a lesson back on ([`upsert_lesson`] respects it).
pub fn set_lesson_active(
    conn: &Connection,
    lesson_id: i64,
    active: bool,
    now: i64,
) -> Result<bool, rusqlite::Error> {
    let n = conn.execute(
        "UPDATE lessons SET active = ?2, updated_at = ?3 WHERE id = ?1",
        params![lesson_id, active as i64, now],
    )?;
    Ok(n > 0)
}

/// How many lessons are currently active — the D-6 `active_lessons` counter.
pub fn count_active_lessons(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("SELECT count(*) FROM lessons WHERE active = 1", [], |r| r.get(0))
}

// ---------------------------------------------------------------- instruction templates

// Fixed English templates (v1 UI language). Fixed text is load-bearing twice over: normalized
// equality is how re-distilled candidates merge instead of duplicating, and prefix pairs are how
// [`opposes`] recognizes a counter-direction stream.
const SIGNATURE_REMOVAL_PREFIX: &str = "Do not end drafts with the closing line ";
const SIGNATURE_ADDITION_PREFIX: &str = "End drafts with the closing line ";
const GREETING_REMOVAL_PREFIX: &str = "Do not start drafts with the greeting line ";
const GREETING_ADDITION_PREFIX: &str = "Start drafts with the greeting line ";
/// Rule (b): consistent shortening.
pub const SHORTEN_INSTRUCTION: &str =
    "Keep drafts significantly shorter; the user consistently trims them before approving.";
/// Rule (c): the user rewrites Japanese drafts into English.
pub const REPLY_IN_ENGLISH_INSTRUCTION: &str =
    "Write replies in English, even when the surrounding context is Japanese.";
/// Rule (c): the user rewrites English drafts into Japanese.
pub const REPLY_IN_JAPANESE_INSTRUCTION: &str =
    "Write replies in Japanese, even when the surrounding context is English.";

fn line_instruction(prefix: &str, line: &str) -> String {
    format!("{prefix}{line:?}.")
}

/// True when two instructions are template opposites — the marker of a counter-direction
/// feedback stream (e.g. the user used to delete a signature, now keeps adding it back).
/// Symmetric. Only recognizes this module's templates; free-text directives never oppose.
pub fn opposes(a: &str, b: &str) -> bool {
    fn one_way(a: &str, b: &str) -> bool {
        let pairs = [
            (SIGNATURE_REMOVAL_PREFIX, SIGNATURE_ADDITION_PREFIX),
            (GREETING_REMOVAL_PREFIX, GREETING_ADDITION_PREFIX),
        ];
        pairs.iter().any(|(neg, pos)| {
            matches!(
                (a.strip_prefix(neg), b.strip_prefix(pos)),
                (Some(x), Some(y)) if x == y
            )
        }) || (a == REPLY_IN_ENGLISH_INSTRUCTION && b == REPLY_IN_JAPANESE_INSTRUCTION)
    }
    one_way(a, b) || one_way(b, a)
}

// ---------------------------------------------------------------- distillation (pure)

/// Non-empty trimmed lines.
fn lines_of(s: &str) -> Vec<&str> {
    s.lines().map(str::trim).filter(|l| !l.is_empty()).collect()
}

/// Dominant script of a text: `Some(true)` = Japanese, `Some(false)` = English/Latin, `None` =
/// neither dominates. Same CJK ranges as `extract::is_cjk` (private there; a two-crate-local
/// duplicate beats a public export for four match arms).
fn dominant_japanese(s: &str) -> Option<bool> {
    let mut cjk = 0usize;
    let mut latin = 0usize;
    for c in s.chars() {
        match c as u32 {
            0x3040..=0x309F | 0x30A0..=0x30FF | 0x4E00..=0x9FFF | 0xFF66..=0xFF9F => cjk += 1,
            _ if c.is_ascii_alphabetic() => latin += 1,
            _ => {}
        }
    }
    match cjk.cmp(&latin) {
        std::cmp::Ordering::Greater => Some(true),
        std::cmp::Ordering::Less if latin > 0 => Some(false),
        _ => None,
    }
}

/// One boundary-line feature accessor paired with its instruction-template prefix.
type LineRule = (for<'a> fn(&'a EditFeatures<'a>) -> Option<&'a str>, &'static str);

/// What one approval-time edit did, in machine-checkable terms.
struct EditFeatures<'a> {
    id: i64,
    removed_first: Option<&'a str>,
    added_first: Option<&'a str>,
    removed_last: Option<&'a str>,
    added_last: Option<&'a str>,
    shortened: bool,
    /// `Some(true)` = JA→EN, `Some(false)` = EN→JA.
    switched_to_english: Option<bool>,
}

fn edit_features<'a>(id: i64, before: &'a str, after: &'a str) -> EditFeatures<'a> {
    let before_lines = lines_of(before);
    let after_lines = lines_of(after);

    // A greeting/signature is a boundary line of a *multi-line* text: on a single-line draft
    // "first line" and "last line" are the whole body and the diff means nothing.
    let (mut removed_first, mut removed_last) = (None, None);
    if before_lines.len() >= 2 {
        let first = before_lines[0];
        if !after_lines.contains(&first) {
            removed_first = Some(first);
        }
        if let Some(&last) = before_lines.last() {
            if !after_lines.contains(&last) {
                removed_last = Some(last);
            }
        }
    }
    let (mut added_first, mut added_last) = (None, None);
    if after_lines.len() >= 2 {
        let first = after_lines[0];
        if !before_lines.contains(&first) {
            added_first = Some(first);
        }
        if let Some(&last) = after_lines.last() {
            if !before_lines.contains(&last) {
                added_last = Some(last);
            }
        }
    }

    let before_len = before.chars().count() as f64;
    let after_len = after.chars().count() as f64;
    let shortened = before_len > 0.0 && after_len < before_len * (1.0 - SHORTEN_RATIO);

    let switched_to_english = match (dominant_japanese(before), dominant_japanese(after)) {
        (Some(true), Some(false)) => Some(true),
        (Some(false), Some(true)) => Some(false),
        _ => None,
    };

    EditFeatures { id, removed_first, added_first, removed_last, added_last, shortened, switched_to_english }
}

/// Distill lesson candidates from raw feedback — pure, no DB, no clock (Plan D-4 local rules).
///
/// Only `edit_before_approve` events with both texts participate. Events are grouped by
/// `(scope, scope_ref, action_kind)`; within a group each rule fires on
/// [`MIN_RULE_OCCURRENCES`]+ **same-direction** observations:
///
/// - **(a) greeting / signature**: the same first (greeting) or last (signature) line
///   consistently removed — or consistently added — across edits.
/// - **(b) shortening**: the approved text is >30% shorter than the proposal, repeatedly.
/// - **(c) reply language**: the dominant Unicode script consistently flips JA→EN or EN→JA.
///
/// Each rule emits one templated English instruction; output is deterministic (sorted by scope,
/// ref, instruction) with evidence ids ascending.
pub fn distill(feedback: &[FeedbackRow]) -> Vec<LessonCandidate> {
    type GroupKey<'a> = (LessonScope, Option<&'a str>, Option<&'a str>);
    let mut groups: HashMap<GroupKey<'_>, Vec<EditFeatures<'_>>> = HashMap::new();
    for row in feedback {
        if row.kind != FeedbackKind::EditBeforeApprove {
            continue;
        }
        let (Some(before), Some(after)) = (row.before_text.as_deref(), row.after_text.as_deref())
        else {
            continue;
        };
        groups
            .entry((row.scope, row.scope_ref.as_deref(), row.action_kind.as_deref()))
            .or_default()
            .push(edit_features(row.id, before, after));
    }

    let mut out = Vec::new();
    for ((scope, scope_ref, _action_kind), edits) in &groups {
        let scope_ref = scope_ref.map(str::to_owned);
        let mut push = |kind: LessonKind, instruction: String, mut evidence: Vec<i64>| {
            evidence.sort_unstable();
            out.push(LessonCandidate { kind, scope: *scope, scope_ref: scope_ref.clone(), instruction, evidence });
        };

        // (a) boundary lines: bucket per exact (trimmed) line so only a *consistent* edit fires.
        let line_rules: [LineRule; 4] = [
            (|e| e.removed_first, GREETING_REMOVAL_PREFIX),
            (|e| e.added_first, GREETING_ADDITION_PREFIX),
            (|e| e.removed_last, SIGNATURE_REMOVAL_PREFIX),
            (|e| e.added_last, SIGNATURE_ADDITION_PREFIX),
        ];
        for (feature, prefix) in line_rules {
            let mut per_line: HashMap<&str, Vec<i64>> = HashMap::new();
            for e in edits {
                if let Some(line) = feature(e) {
                    per_line.entry(line).or_default().push(e.id);
                }
            }
            for (line, ids) in per_line {
                if ids.len() >= MIN_RULE_OCCURRENCES {
                    push(LessonKind::Style, line_instruction(prefix, line), ids);
                }
            }
        }

        // (b) consistent shortening.
        let shortened: Vec<i64> = edits.iter().filter(|e| e.shortened).map(|e| e.id).collect();
        if shortened.len() >= MIN_RULE_OCCURRENCES {
            push(LessonKind::Style, SHORTEN_INSTRUCTION.to_owned(), shortened);
        }

        // (c) consistent reply-language switch — each direction counted on its own.
        for (to_english, instruction) in
            [(true, REPLY_IN_ENGLISH_INSTRUCTION), (false, REPLY_IN_JAPANESE_INSTRUCTION)]
        {
            let ids: Vec<i64> = edits
                .iter()
                .filter(|e| e.switched_to_english == Some(to_english))
                .map(|e| e.id)
                .collect();
            if ids.len() >= MIN_RULE_OCCURRENCES {
                push(LessonKind::Preference, instruction.to_owned(), ids);
            }
        }
    }

    out.sort_by(|a, b| {
        (a.scope.as_str(), &a.scope_ref, &a.instruction)
            .cmp(&(b.scope.as_str(), &b.scope_ref, &b.instruction))
    });
    out
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    fn record_edit(
        conn: &Connection,
        ts: i64,
        scope: LessonScope,
        scope_ref: Option<&str>,
        before: &str,
        after: &str,
    ) -> i64 {
        record_feedback(
            conn,
            FeedbackKind::EditBeforeApprove,
            scope,
            &NewFeedback {
                ts_ms: ts,
                action_kind: Some("draft_reply"),
                scope_ref,
                before_text: Some(before),
                after_text: Some(after),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn lesson_row(conn: &Connection, id: i64) -> (f64, i64, i64) {
        conn.query_row(
            "SELECT confidence, evidence_count, active FROM lessons WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
    }

    /// A body long enough that stripping one closing line is nowhere near a 30% cut.
    const BODY: &str = "Hi team,\nHere is the current status of the migration work.\nEverything is on track for the Friday checkpoint and the remaining items are listed in the tracker.";

    fn signature_edits(conn: &Connection, n: usize, base_ts: i64) -> Vec<i64> {
        (0..n)
            .map(|i| {
                let before = format!("{BODY}\nExtra note number {i}.\nBest, Taro");
                let after = format!("{BODY}\nExtra note number {i}.");
                record_edit(
                    conn,
                    base_ts + i as i64,
                    LessonScope::App,
                    Some("com.apple.Mail"),
                    &before,
                    &after,
                )
            })
            .collect()
    }

    // ------------------------------------------------ schema

    #[test]
    fn migration_creates_the_lessons_tables_with_checks() {
        let conn = crate::open_in_memory().unwrap();
        for table in ["feedback_events", "lessons", "lesson_provenance"] {
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "missing table {table}");
        }
        // kind / scope / confidence CHECKs hold.
        assert!(conn
            .execute(
                "INSERT INTO feedback_events (ts_ms, kind, scope) VALUES (1, 'bogus', 'global')",
                [],
            )
            .is_err());
        assert!(conn
            .execute(
                "INSERT INTO lessons (kind, scope, instruction, confidence, created_at, updated_at, last_evidence_at)
                 VALUES ('style', 'global', 'x', 1.5, 0, 0, 0)",
                [],
            )
            .is_err());
        // provenance FK needs a real lesson + feedback event.
        assert!(conn
            .execute(
                "INSERT INTO lesson_provenance (lesson_id, feedback_event_id) VALUES (99, 99)",
                [],
            )
            .is_err());
    }

    // ------------------------------------------------ record → distill

    #[test]
    fn three_signature_removals_distill_to_exactly_one_candidate() {
        let conn = crate::open_in_memory().unwrap();
        let ids = signature_edits(&conn, 3, 1_000);
        let candidates = distill(&list_feedback_since(&conn, 0).unwrap());
        assert_eq!(candidates.len(), 1, "exactly one candidate: {candidates:?}");
        let c = &candidates[0];
        assert_eq!(c.kind, LessonKind::Style);
        assert_eq!(c.scope, LessonScope::App);
        assert_eq!(c.scope_ref.as_deref(), Some("com.apple.Mail"));
        assert_eq!(c.instruction, "Do not end drafts with the closing line \"Best, Taro\".");
        assert_eq!(c.evidence, ids);
    }

    #[test]
    fn two_signature_removals_are_not_enough() {
        let conn = crate::open_in_memory().unwrap();
        signature_edits(&conn, 2, 1_000);
        assert!(distill(&list_feedback_since(&conn, 0).unwrap()).is_empty());
    }

    #[test]
    fn language_switch_rule_fires_on_three_same_direction_switches_only() {
        let conn = crate::open_in_memory().unwrap();
        let ja = "承知しました。明日の会議までに資料を準備して共有します。よろしくお願いいたします。";
        let en = "Understood. I will prepare the materials and share them before tomorrow's meeting.";
        for i in 0..2 {
            record_edit(&conn, 10 + i, LessonScope::Person, Some("p42"), ja, en);
        }
        // Two JA→EN plus one opposite-direction edit: nothing may fire.
        record_edit(&conn, 20, LessonScope::Person, Some("p42"), en, ja);
        assert!(distill(&list_feedback_since(&conn, 0).unwrap()).is_empty());

        // A third JA→EN makes it three same-direction: exactly the English-preference lesson.
        record_edit(&conn, 30, LessonScope::Person, Some("p42"), ja, en);
        let candidates = distill(&list_feedback_since(&conn, 0).unwrap());
        assert_eq!(candidates.len(), 1, "{candidates:?}");
        assert_eq!(candidates[0].instruction, REPLY_IN_ENGLISH_INSTRUCTION);
        assert_eq!(candidates[0].kind, LessonKind::Preference);
    }

    #[test]
    fn consistent_shortening_fires_at_three() {
        let conn = crate::open_in_memory().unwrap();
        let before = "This proposal keeps restating the same point with more and more words. ".repeat(4);
        let after = "This proposal keeps restating the same point.";
        for i in 0..3 {
            record_edit(&conn, i, LessonScope::Global, None, &before, after);
        }
        let candidates = distill(&list_feedback_since(&conn, 0).unwrap());
        assert_eq!(candidates.len(), 1, "{candidates:?}");
        assert_eq!(candidates[0].instruction, SHORTEN_INSTRUCTION);
        assert_eq!(candidates[0].scope, LessonScope::Global);
        assert!(candidates[0].scope_ref.is_none());
    }

    #[test]
    fn a_mild_trim_is_not_a_shortening_signal() {
        let conn = crate::open_in_memory().unwrap();
        let before = "One two three four five six seven eight nine ten.";
        let after = "One two three four five six seven eight nine."; // ~10% cut
        for i in 0..3 {
            record_edit(&conn, i, LessonScope::Global, None, before, after);
        }
        assert!(distill(&list_feedback_since(&conn, 0).unwrap()).is_empty());
    }

    // ------------------------------------------------ upsert lifecycle

    #[test]
    fn upsert_merges_on_normalized_instruction_and_raises_confidence_monotonically() {
        let mut conn = crate::open_in_memory().unwrap();
        let evidence = signature_edits(&conn, 6, 1_000);
        let candidate = LessonCandidate {
            kind: LessonKind::Style,
            scope: LessonScope::App,
            scope_ref: Some("com.apple.Mail".into()),
            instruction: "Do not end drafts with the closing line \"Best, Taro\".".into(),
            evidence: vec![],
        };
        let id = upsert_lesson(&mut conn, &candidate, &evidence[..3], 2_000).unwrap();
        let (c1, n1, _) = lesson_row(&conn, id);
        assert_eq!(n1, 3);
        assert!((c1 - corroborated_confidence(3.0)).abs() < 1e-9, "born on the curve: {c1}");
        assert!(c1 >= INJECTION_FLOOR, "a distilled lesson must be injectable at birth");

        // Same instruction up to case/whitespace merges rather than duplicating.
        let recased = LessonCandidate {
            instruction: "  DO NOT end drafts with the closing line \"best, taro\". ".into(),
            ..candidate.clone()
        };
        let id2 = upsert_lesson(&mut conn, &recased, &evidence[3..5], 3_000).unwrap();
        assert_eq!(id2, id, "must merge, not insert");
        let (c2, n2, _) = lesson_row(&conn, id);
        assert_eq!(n2, 5);
        assert!(c2 > c1, "more evidence must raise confidence: {c1} -> {c2}");
        let total: i64 =
            conn.query_row("SELECT count(*) FROM lessons", [], |r| r.get(0)).unwrap();
        assert_eq!(total, 1);

        // Re-linking known evidence is a no-op on the count and never lowers.
        let id3 = upsert_lesson(&mut conn, &candidate, &evidence[..5], 4_000).unwrap();
        assert_eq!(id3, id);
        let (c3, n3, _) = lesson_row(&conn, id);
        assert_eq!(n3, 5, "already-linked evidence must not double-count");
        assert!(c3 >= c2);

        // Bounded: pile on evidence, stays ≤ 1 (and below the High band by the curve's ceiling).
        let id4 = upsert_lesson(&mut conn, &candidate, &evidence[5..], 5_000).unwrap();
        assert_eq!(id4, id);
        let (c4, _, _) = lesson_row(&conn, id);
        assert!(c4 > c2 && c4 <= 1.0);
        assert!(c4 < 0.8, "local corroboration must never reach High: {c4}");

        let prov: i64 = conn
            .query_row("SELECT count(*) FROM lesson_provenance WHERE lesson_id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(prov, 6, "every evidence id must be linked exactly once");
    }

    #[test]
    fn upsert_without_evidence_is_rejected_and_nothing_written() {
        let mut conn = crate::open_in_memory().unwrap();
        let candidate = LessonCandidate {
            kind: LessonKind::Style,
            scope: LessonScope::Global,
            scope_ref: None,
            instruction: "x".into(),
            evidence: vec![],
        };
        assert!(matches!(
            upsert_lesson(&mut conn, &candidate, &[], 1),
            Err(MemoryError::EmptyProvenance)
        ));
        let n: i64 = conn.query_row("SELECT count(*) FROM lessons", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn upsert_provenance_fk_requires_a_real_feedback_event() {
        let mut conn = crate::open_in_memory().unwrap();
        let candidate = LessonCandidate {
            kind: LessonKind::Style,
            scope: LessonScope::Global,
            scope_ref: None,
            instruction: "x".into(),
            evidence: vec![],
        };
        assert!(upsert_lesson(&mut conn, &candidate, &[999], 1).is_err());
        let n: i64 = conn.query_row("SELECT count(*) FROM lessons", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "the failed transaction must leave nothing behind");
    }

    // ------------------------------------------------ decay / contradiction / cap

    #[test]
    fn a_counter_direction_stream_deactivates_the_older_lesson() {
        let mut conn = crate::open_in_memory().unwrap();
        // Phase 1: the user strips the signature three times → removal lesson.
        let removal_evidence = signature_edits(&conn, 3, 1_000);
        let removal = distill(&list_feedback_since(&conn, 0).unwrap()).remove(0);
        let removal_id = upsert_lesson(&mut conn, &removal, &removal_evidence, 2_000).unwrap();

        // Phase 2: later the user adds the same signature back three times.
        let addition_evidence: Vec<i64> = (0..3)
            .map(|i| {
                let before = format!("{BODY}\nAnother note {i}.");
                let after = format!("{BODY}\nAnother note {i}.\nBest, Taro");
                record_edit(&conn, 10_000 + i, LessonScope::App, Some("com.apple.Mail"), &before, &after)
            })
            .collect();
        let candidates = distill(&list_feedback_since(&conn, 10_000).unwrap());
        assert_eq!(candidates.len(), 1);
        let addition_id =
            upsert_lesson(&mut conn, &candidates[0], &addition_evidence, 11_000).unwrap();
        assert!(opposes(&removal.instruction, &candidates[0].instruction));

        let outcome = decay_and_deactivate(&mut conn, 12_000).unwrap();
        assert_eq!(outcome.contradicted, 1);
        assert_eq!(lesson_row(&conn, removal_id).2, 0, "the older direction must sleep");
        assert_eq!(lesson_row(&conn, addition_id).2, 1, "the current direction stays");
    }

    #[test]
    fn silence_decays_confidence_idempotently_and_the_floor_deactivates() {
        let mut conn = crate::open_in_memory().unwrap();
        let evidence = signature_edits(&conn, 3, 0);
        let candidate = distill(&list_feedback_since(&conn, 0).unwrap()).remove(0);
        let id = upsert_lesson(&mut conn, &candidate, &evidence, 0).unwrap();
        let born = lesson_row(&conn, id).0;

        // One half-life of silence halves it; a second pass at the same instant changes nothing.
        let outcome = decay_and_deactivate(&mut conn, LESSON_HALF_LIFE_MS).unwrap();
        assert_eq!(outcome.decayed, 1);
        let halved = lesson_row(&conn, id).0;
        assert!((halved - born / 2.0).abs() < 1e-6, "{born} should halve to {halved}");
        let again = decay_and_deactivate(&mut conn, LESSON_HALF_LIFE_MS).unwrap();
        assert_eq!(again.decayed, 0, "idempotent for a given now");
        assert_eq!(lesson_row(&conn, id).2, 1, "still above the floor");

        // Fresh evidence restores the derived value (last_evidence_at moves forward).
        conn.execute(
            "UPDATE lessons SET last_evidence_at = ?1 WHERE id = ?2",
            params![LESSON_HALF_LIFE_MS, id],
        )
        .unwrap();
        decay_and_deactivate(&mut conn, LESSON_HALF_LIFE_MS).unwrap();
        assert!((lesson_row(&conn, id).0 - born).abs() < 1e-9, "evidence restores the base");

        // Long silence drops it under the deactivation floor and it sleeps.
        let outcome =
            decay_and_deactivate(&mut conn, LESSON_HALF_LIFE_MS + 4 * LESSON_HALF_LIFE_MS).unwrap();
        assert_eq!(outcome.below_floor, 1);
        let (c, _, active) = lesson_row(&conn, id);
        assert!(c < DEACTIVATION_FLOOR, "{c}");
        assert_eq!(active, 0);
    }

    #[test]
    fn the_active_cap_puts_the_weakest_to_sleep() {
        let mut conn = crate::open_in_memory().unwrap();
        // 55 active lessons with evidence counts 2..=56 — confidence is derived from the count,
        // so ids 1..=5 (the least-evidenced) are the weakest.
        for i in 0..55i64 {
            conn.execute(
                "INSERT INTO lessons (kind, scope, scope_ref, instruction, confidence,
                                      evidence_count, active, created_at, updated_at, last_evidence_at)
                 VALUES ('style', 'global', NULL, ?1, 0.5, ?2, 1, 0, 0, 0)",
                params![format!("lesson {i}"), i + 2],
            )
            .unwrap();
        }
        let outcome = decay_and_deactivate(&mut conn, 0).unwrap();
        assert_eq!(outcome.over_cap, 5);
        let active: i64 =
            conn.query_row("SELECT count(*) FROM lessons WHERE active = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(active, ACTIVE_LESSON_CAP as i64);
        let weakest_asleep: i64 = conn
            .query_row("SELECT count(*) FROM lessons WHERE active = 0 AND id <= 5", [], |r| r.get(0))
            .unwrap();
        assert_eq!(weakest_asleep, 5, "eviction must pick the weakest, not arbitrary rows");
    }

    // ------------------------------------------------ active_lessons

    #[test]
    fn active_lessons_honors_scope_filters_the_band_floor_and_top_k() {
        let mut conn = crate::open_in_memory().unwrap();
        let mut seed = |scope: LessonScope, scope_ref: Option<&str>, instruction: &str, n: usize| {
            let evidence: Vec<i64> = (0..n)
                .map(|i| {
                    record_feedback(
                        &conn,
                        FeedbackKind::EditBeforeApprove,
                        scope,
                        &NewFeedback { ts_ms: i as i64, scope_ref, ..Default::default() },
                    )
                    .unwrap()
                })
                .collect();
            let candidate = LessonCandidate {
                kind: LessonKind::Style,
                scope,
                scope_ref: scope_ref.map(str::to_owned),
                instruction: instruction.into(),
                evidence: vec![],
            };
            upsert_lesson(&mut conn, &candidate, &evidence, 100).unwrap()
        };
        let mail = seed(LessonScope::App, Some("com.apple.Mail"), "mail lesson", 5);
        let slack = seed(LessonScope::App, Some("com.tinyspeck.slackmacgap"), "slack lesson", 3);
        let global = seed(LessonScope::Global, None, "global lesson", 4);
        let person = seed(LessonScope::Person, Some("p1"), "person lesson", 3);

        // Below the injection floor (Low band, cf. shogun-fusion confidence.rs) and switched-off
        // rows never surface, however well they match the scope.
        conn.execute("UPDATE lessons SET confidence = 0.4 WHERE id = ?1", [slack]).unwrap();
        conn.execute("UPDATE lessons SET active = 0 WHERE id = ?1", [person]).unwrap();

        // Fusion's view from Mail: the mail lesson and the global one, strongest first.
        let filters =
            [ScopeFilter { scope: LessonScope::App, scope_ref: Some("com.apple.Mail") },
             ScopeFilter { scope: LessonScope::Global, scope_ref: None }];
        let got = active_lessons(&conn, &filters, 10).unwrap();
        assert_eq!(got.iter().map(|l| l.id).collect::<Vec<_>>(), vec![mail, global]);
        assert!(got[0].confidence >= got[1].confidence, "strongest first");

        // No filters = every injectable lesson; top_k truncates from the strongest end.
        let all = active_lessons(&conn, &[], 10).unwrap();
        assert_eq!(all.iter().map(|l| l.id).collect::<Vec<_>>(), vec![mail, global]);
        let top1 = active_lessons(&conn, &[], 1).unwrap();
        assert_eq!(top1.len(), 1);
        assert_eq!(top1[0].id, mail);

        // A scope_ref-less App filter matches any app lesson at or above the floor.
        let any_app = active_lessons(
            &conn,
            &[ScopeFilter { scope: LessonScope::App, scope_ref: None }],
            10,
        )
        .unwrap();
        assert_eq!(any_app.iter().map(|l| l.id).collect::<Vec<_>>(), vec![mail]);
    }

    #[test]
    fn active_lessons_with_a_top_k_of_zero_returns_nothing() {
        // The contract is "at most top_k", zero included. The cap is only tested after a push, so
        // a zero could never be reached and the caller got every lesson — the exact opposite of
        // what it asked for.
        let mut conn = crate::open_in_memory().unwrap();
        let instructions = ["first lesson", "second lesson", "third lesson"];
        for (i, instruction) in instructions.into_iter().enumerate() {
            let evidence: Vec<i64> = (0..3i64)
                .map(|j| {
                    record_feedback(
                        &conn,
                        FeedbackKind::EditBeforeApprove,
                        LessonScope::Global,
                        &NewFeedback { ts_ms: i as i64 * 10 + j, ..Default::default() },
                    )
                    .unwrap()
                })
                .collect();
            let candidate = LessonCandidate {
                kind: LessonKind::Style,
                scope: LessonScope::Global,
                scope_ref: None,
                instruction: instruction.into(),
                evidence: vec![],
            };
            upsert_lesson(&mut conn, &candidate, &evidence, 100).unwrap();
        }

        assert_eq!(active_lessons(&conn, &[], 3).unwrap().len(), 3, "all three are injectable");
        assert!(active_lessons(&conn, &[], 0).unwrap().is_empty(), "top_k = 0 means none");
    }

    // ------------------------------------------------ helpers

    #[test]
    fn opposes_recognizes_template_pairs_symmetrically_and_nothing_else() {
        let remove = line_instruction(SIGNATURE_REMOVAL_PREFIX, "Best, Taro");
        let add = line_instruction(SIGNATURE_ADDITION_PREFIX, "Best, Taro");
        let add_other = line_instruction(SIGNATURE_ADDITION_PREFIX, "Regards, Jiro");
        assert!(opposes(&remove, &add));
        assert!(opposes(&add, &remove), "must be symmetric");
        assert!(!opposes(&remove, &add_other), "different lines do not oppose");
        assert!(opposes(REPLY_IN_ENGLISH_INSTRUCTION, REPLY_IN_JAPANESE_INSTRUCTION));
        assert!(!opposes(SHORTEN_INSTRUCTION, &remove));
        assert!(!opposes("free text", "other free text"));
    }

    // ------------------------------------------------ watermark / list / set_active

    #[test]
    fn watermark_starts_at_zero_advances_monotonically_and_bounds_list_feedback_after() {
        let conn = crate::open_in_memory().unwrap();
        assert_eq!(distill_watermark(&conn).unwrap(), 0);
        let ids = signature_edits(&conn, 3, 1_000);

        // everything is unprocessed at watermark 0
        let unprocessed = list_feedback_after(&conn, distill_watermark(&conn).unwrap()).unwrap();
        assert_eq!(unprocessed.iter().map(|f| f.id).collect::<Vec<_>>(), ids);

        // advancing past the first two leaves only the third
        set_distill_watermark(&conn, ids[1]).unwrap();
        assert_eq!(distill_watermark(&conn).unwrap(), ids[1]);
        let rest = list_feedback_after(&conn, ids[1]).unwrap();
        assert_eq!(rest.iter().map(|f| f.id).collect::<Vec<_>>(), vec![ids[2]]);

        // monotonic: a stale (smaller) write never rewinds
        set_distill_watermark(&conn, 0).unwrap();
        assert_eq!(distill_watermark(&conn).unwrap(), ids[1]);
    }

    #[test]
    fn list_lessons_returns_sleeping_rows_and_set_active_flips_the_switch() {
        let mut conn = crate::open_in_memory().unwrap();
        let evidence = signature_edits(&conn, 3, 0);
        let candidate = distill(&list_feedback_since(&conn, 0).unwrap()).remove(0);
        let id = upsert_lesson(&mut conn, &candidate, &evidence, 100).unwrap();

        let all = list_lessons(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].active);
        assert_eq!(count_active_lessons(&conn).unwrap(), 1);

        // switch off: still listed (the Learned list shows sleeping rows), no longer injectable
        assert!(set_lesson_active(&conn, id, false, 200).unwrap());
        let all = list_lessons(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert!(!all[0].active);
        assert_eq!(count_active_lessons(&conn).unwrap(), 0);
        assert!(active_lessons(&conn, &[], 10).unwrap().is_empty());

        // back on, and an unknown id reports false
        assert!(set_lesson_active(&conn, id, true, 300).unwrap());
        assert_eq!(count_active_lessons(&conn).unwrap(), 1);
        assert!(!set_lesson_active(&conn, 9999, false, 300).unwrap());
    }

    #[test]
    fn count_feedback_since_counts_without_reading_text() {
        let conn = crate::open_in_memory().unwrap();
        signature_edits(&conn, 3, 1_000);
        assert_eq!(count_feedback_since(&conn, 0).unwrap(), 3);
        assert_eq!(count_feedback_since(&conn, 1_002).unwrap(), 1);
        assert_eq!(count_feedback_since(&conn, 2_000).unwrap(), 0);
    }

    #[test]
    fn decision_counts_separate_adoption_from_rejection_and_ignore_state_resolve() {
        let conn = crate::open_in_memory().unwrap();
        let f = |ts| NewFeedback {
            ts_ms: ts,
            action_kind: Some("draft_reply"),
            ..Default::default()
        };
        let rec = |k, ts| record_feedback(&conn, k, LessonScope::Global, &f(ts)).unwrap();
        rec(FeedbackKind::ApproveUnchanged, 1_000);
        rec(FeedbackKind::EditBeforeApprove, 1_001); // edited, but it shipped → adoption
        rec(FeedbackKind::Reject, 1_002);
        rec(FeedbackKind::Undo, 1_003); // decided, taken back → not an adoption
        rec(FeedbackKind::StateResolve, 1_004); // no proposal was shown → not a decision at all

        assert_eq!(decision_counts_since(&conn, 0).unwrap(), (4, 2));
        // the window cuts from the front, and `sum()` over zero rows must read 0, not NULL.
        assert_eq!(decision_counts_since(&conn, 1_002).unwrap(), (2, 0));
        assert_eq!(decision_counts_since(&conn, 9_999).unwrap(), (0, 0));
    }

    #[test]
    fn acceptance_by_kind_groups_and_excludes_kindless_state_decisions() {
        let conn = crate::open_in_memory().unwrap();
        let rec = |k, action_kind, ts| {
            record_feedback(
                &conn,
                k,
                LessonScope::Global,
                &NewFeedback {
                    ts_ms: ts,
                    action_kind,
                    surface: Some(Surface::Notch),
                    rank: Some(0),
                    ..Default::default()
                },
            )
            .unwrap()
        };
        rec(FeedbackKind::ApproveUnchanged, Some("draft_reply"), 1_000);
        rec(FeedbackKind::EditBeforeApprove, Some("draft_reply"), 1_001);
        rec(FeedbackKind::Reject, Some("draft_reply"), 1_002);
        rec(FeedbackKind::Reject, Some("save_note"), 1_003);
        // no action_kind, and not a decision on a proposal either — must not appear at all.
        rec(FeedbackKind::StateResolve, None, 1_004);

        let by_kind = acceptance_by_kind(&conn, 0).unwrap();
        assert_eq!(by_kind, vec![("draft_reply".into(), 3, 2), ("save_note".into(), 1, 0)]);

        // the window applies before the grouping
        assert_eq!(acceptance_by_kind(&conn, 1_003).unwrap(), vec![("save_note".into(), 1, 0)]);
        assert!(acceptance_by_kind(&conn, 9_999).unwrap().is_empty());
    }

    #[test]
    fn the_surface_vocabulary_is_closed_at_the_boundary_and_in_the_schema() {
        assert_eq!(Surface::from_wire("option_key"), Some(Surface::OptionKey));
        assert_eq!(Surface::from_wire("Notch"), None, "wire names are exact");
        assert_eq!(Surface::from_wire("keyboard"), None);
        for &s in ALL_SURFACES {
            assert_eq!(Surface::from_wire(s.as_str()), Some(s));
        }

        // V19's CHECK is the second line of defence: a surface that got past Rust is still refused.
        let conn = crate::open_in_memory().unwrap();
        let err = conn.execute(
            "INSERT INTO feedback_events (ts_ms, kind, scope, surface)
             VALUES (1, 'reject', 'global', 'telepathy')",
            [],
        );
        assert!(err.is_err(), "the schema must reject an unknown surface too");
        // …and NULL stays legal, because pre-V19 rows have no surface and neither does a caller
        // that genuinely does not know.
        conn.execute(
            "INSERT INTO feedback_events (ts_ms, kind, scope) VALUES (1, 'reject', 'global')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn the_offer_context_round_trips_and_stays_metadata_only() {
        let conn = crate::open_in_memory().unwrap();
        record_feedback(
            &conn,
            FeedbackKind::ApproveUnchanged,
            LessonScope::Global,
            &NewFeedback {
                ts_ms: 42,
                action_kind: Some("draft_reply"),
                surface: Some(Surface::Recap),
                rank: Some(3),
                context_app: Some("com.apple.mail"),
                latency_ms: Some(1_500),
                ..Default::default()
            },
        )
        .unwrap();
        let got: (String, i64, String, i64) = conn
            .query_row(
                "SELECT surface, rank, context_app, latency_ms FROM feedback_events",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(got, ("recap".into(), 3, "com.apple.mail".into(), 1_500));
    }

    #[test]
    fn feedback_row_debug_never_renders_the_text() {
        let row = FeedbackRow {
            id: 1,
            ts_ms: 2,
            kind: FeedbackKind::EditBeforeApprove,
            action_kind: None,
            scope: LessonScope::Global,
            scope_ref: None,
            before_text: Some("the quarterly numbers".into()),
            after_text: Some("secret draft".into()),
        };
        let rendered = format!("{row:?}");
        assert!(!rendered.contains("quarterly"), "content leaked into Debug: {rendered}");
        assert!(!rendered.contains("secret"), "content leaked into Debug: {rendered}");
        assert!(rendered.contains("redacted"));
    }
}
