//! The Patterns layer's learning input (FR-PAT-01): a metadata-only log of how the user decided
//! on proposed actions.
//!
//! v1 only records; nothing reads this at runtime yet. The two consumers it exists for:
//! - the FR-CF-03 priority score's "recent adoption rate of this action kind" input
//!   ([`acceptance_by_kind`] is that aggregation), and
//! - the v1.5 Patterns/Lessons work (FR-PAT-02), which needs history from day one — recording
//!   cannot start retroactively, which is why the log ships before anything learns from it.
//!
//! Privacy: rows carry action kind / surface / rank / latency — never the action's content or any
//! captured text — and the table is never exported or sent anywhere (FR-PAT-01).

use rusqlite::{params, Connection};

/// Where a decision happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Notch,
    OptionKey,
    Chat,
    Recap,
    Api,
}

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
}

/// The decision itself. `Tracked`/`Discarded` are the Recap-candidate pair (FR-MT-17); the other
/// three are the proposal pair plus the edited-then-ran middle ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Accepted,
    Edited,
    Dismissed,
    Tracked,
    Discarded,
}

/// Every outcome, for deriving SQL predicates from the enum instead of duplicating the list.
const ALL_OUTCOMES: &[Outcome] =
    &[Outcome::Accepted, Outcome::Edited, Outcome::Dismissed, Outcome::Tracked, Outcome::Discarded];

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Accepted => "accepted",
            Outcome::Edited => "edited",
            Outcome::Dismissed => "dismissed",
            Outcome::Tracked => "tracked",
            Outcome::Discarded => "discarded",
        }
    }

    /// Whether this outcome counts as adoption for the acceptance rate. An edit that still ran
    /// is adoption (the proposal was useful enough to fix rather than discard).
    fn is_adoption(self) -> bool {
        matches!(self, Outcome::Accepted | Outcome::Edited | Outcome::Tracked)
    }
}

/// One decision to record.
#[derive(Debug, Clone)]
pub struct NewFeedback<'a> {
    pub ts: i64,
    pub action_kind: &'a str,
    pub surface: Surface,
    pub outcome: Outcome,
    /// Frontmost bundle id at decision time — context, not content.
    pub context_app: Option<&'a str>,
    /// Candidate position when offered (0 = top). `None` when the surface has no ranking.
    pub rank: Option<i64>,
    pub latency_ms: Option<i64>,
}

/// Record one decision. Append-only — feedback is history, never edited.
pub fn record(conn: &Connection, f: &NewFeedback<'_>) -> Result<i64, rusqlite::Error> {
    conn.execute(
        "INSERT INTO action_feedback (ts, action_kind, surface, outcome, context_app, rank, latency_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            f.ts,
            f.action_kind,
            f.surface.as_str(),
            f.outcome.as_str(),
            f.context_app,
            f.rank,
            f.latency_ms
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Adoption rate per action kind since `since_ts`: `(kind, decided, adopted)`. This is the
/// FR-CF-03 "recent adoption rate of this action kind" supply; the caller decides the window and
/// the smoothing (a kind with 1 decision should not swing ranking).
pub fn acceptance_by_kind(
    conn: &Connection,
    since_ts: i64,
) -> Result<Vec<(String, i64, i64)>, rusqlite::Error> {
    // The adoption predicate is derived from `Outcome::is_adoption` so the SQL can never drift
    // from the enum's definition of adoption. Static enum strings only — no injection surface.
    let adopted: Vec<String> = ALL_OUTCOMES
        .iter()
        .filter(|o| o.is_adoption())
        .map(|o| format!("'{}'", o.as_str()))
        .collect();
    let mut stmt = conn.prepare(&format!(
        "SELECT action_kind,
                count(*),
                sum(CASE WHEN outcome IN ({}) THEN 1 ELSE 0 END)
         FROM action_feedback
         WHERE ts >= ?1
         GROUP BY action_kind
         ORDER BY action_kind",
        adopted.join(", ")
    ))?;
    let rows = stmt.query_map([since_ts], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    rows.collect()
}

/// Total decisions and adoptions since `since_ts` — the Evening Wrap's "actions decided /
/// adopted today" counts (§6.17). Same adoption definition as [`acceptance_by_kind`].
pub fn counts_since(conn: &Connection, since_ts: i64) -> Result<(i64, i64), rusqlite::Error> {
    let adopted: Vec<String> = ALL_OUTCOMES
        .iter()
        .filter(|o| o.is_adoption())
        .map(|o| format!("'{}'", o.as_str()))
        .collect();
    conn.query_row(
        &format!(
            "SELECT count(*),
                    sum(CASE WHEN outcome IN ({}) THEN 1 ELSE 0 END)
             FROM action_feedback WHERE ts >= ?1",
            adopted.join(", ")
        ),
        [since_ts],
        |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fb<'a>(ts: i64, kind: &'a str, outcome: Outcome) -> NewFeedback<'a> {
        NewFeedback {
            ts,
            action_kind: kind,
            surface: Surface::Notch,
            outcome,
            context_app: Some("com.apple.mail"),
            rank: Some(0),
            latency_ms: Some(900),
        }
    }

    #[test]
    fn decisions_are_recorded_and_aggregate_into_adoption_rates() {
        let conn = crate::open_in_memory().unwrap();
        record(&conn, &fb(100, "draft_reply", Outcome::Accepted)).unwrap();
        record(&conn, &fb(200, "draft_reply", Outcome::Edited)).unwrap();
        record(&conn, &fb(300, "draft_reply", Outcome::Dismissed)).unwrap();
        record(&conn, &fb(400, "save_note", Outcome::Tracked)).unwrap();

        let rates = acceptance_by_kind(&conn, 0).unwrap();
        // edited counts as adoption (the proposal ran after a fix); dismissed does not.
        assert_eq!(
            rates,
            vec![("draft_reply".to_string(), 3, 2), ("save_note".to_string(), 1, 1)]
        );
    }

    #[test]
    fn the_window_bound_excludes_old_decisions() {
        let conn = crate::open_in_memory().unwrap();
        record(&conn, &fb(100, "draft_reply", Outcome::Accepted)).unwrap();
        record(&conn, &fb(2_000, "draft_reply", Outcome::Dismissed)).unwrap();

        let rates = acceptance_by_kind(&conn, 1_000).unwrap();
        assert_eq!(rates, vec![("draft_reply".to_string(), 1, 0)]);
    }

    #[test]
    fn the_outcome_vocabulary_is_closed_at_the_schema() {
        // The CHECK constraint is the audit-cheap guarantee that aggregation stays meaningful —
        // a typo'd outcome is a hard error, not a silently uncounted row.
        let conn = crate::open_in_memory().unwrap();
        let err = conn.execute(
            "INSERT INTO action_feedback (ts, action_kind, surface, outcome)
             VALUES (1, 'draft_reply', 'notch', 'maybe')",
            [],
        );
        assert!(err.is_err());
    }

    #[test]
    fn rows_carry_metadata_only_no_content_column_exists() {
        // FR-PAT-01's privacy shape is structural: the table has no column that could hold
        // captured text or an action body. If someone adds one, this inventory fails review.
        let conn = crate::open_in_memory().unwrap();
        let mut stmt = conn.prepare("SELECT name FROM pragma_table_info('action_feedback')").unwrap();
        let cols: Vec<String> =
            stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
        assert_eq!(
            cols,
            ["id", "ts", "action_kind", "surface", "outcome", "context_app", "rank", "latency_ms"]
        );
    }
}
