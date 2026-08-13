//! Evening Wrap assembly (§6.17 / FR-EB-01..03) — the Morning Brief's local twin.
//!
//! The Wrap closes the day the way the Brief opens it, with one structural difference (FR-EB-02):
//! **nothing here is generated**. No LLM prose, no egress — every section is deterministic
//! aggregation over local state, so the Wrap costs nothing and works offline. The Brief then
//! returns the same picture the next morning, reorganized by the Dream Cycle: the two form the
//! day's loop ("夜のWrapで見た状態が翌朝のBriefで整理されて返る").
//!
//! Rules encoded:
//! - Sections (FR-EB-01): Today's outcome (counts) · Still open (activity today, priority order,
//!   ≤5) · Tomorrow first (calendar ≤3 + tomorrow-due commitments) · Loose ends (loops opened
//!   today, ≤5).
//! - Priority (FR-EB-02): the existing signals only — overdue first, then staleness/due time.
//! - Confidence (FR-ST-20, via [`crate::confidence`]): Low is excluded, Medium is flagged
//!   `possibly` — the same gate the Brief applies, reusing [`crate::brief`]'s item types so the
//!   two surfaces can never drift apart on what a "shown fact" is.
//!
//! The day window itself is NOT computed here. `shogun_core::daemon::local_wrap_window` owns it,
//! on top of the same libc `localtime_r`/`mktime` path `local_day_bounds` already uses — a second
//! day-boundary rule built from a fixed UTC offset would disagree with the first one twice a year.

use crate::brief::{BriefItem, CalendarLine, CommitmentDue, OpenLoopItem};
use crate::confidence::{band, Band};

/// Max "Still open" lines (FR-EB-01).
pub const STILL_OPEN_MAX: usize = 5;
/// Max "Loose ends" lines (FR-EB-01).
pub const LOOSE_ENDS_MAX: usize = 5;
/// Max tomorrow calendar lines (FR-EB-01: 先頭3件).
pub const TOMORROW_CALENDAR_MAX: usize = 3;

/// The day's countable outcome (FR-EB-01 "Today's outcome"). Counts, not content — the section
/// answers "did today move anything", and a number does that without re-listing the day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WrapOutcome {
    /// Commitments marked done today.
    pub commitments_done: u32,
    /// Open loops closed today.
    pub loops_closed: u32,
    /// Action proposals decided on today (accepted + edited + dismissed…, FR-PAT-01).
    pub actions_decided: u32,
    /// …of which were adopted (accepted / edited / tracked).
    pub actions_adopted: u32,
}

/// The assembled Evening Wrap. Local-only by construction: there is no generated field.
#[derive(Debug, Clone)]
pub struct EveningWrap {
    pub outcome: WrapOutcome,
    /// Open items that saw activity today, priority order (overdue → staleness), ≤5.
    pub still_open: Vec<BriefItem>,
    /// Tomorrow's first calendar entries (time-ordered, ≤3).
    pub tomorrow_calendar: Vec<CalendarLine>,
    /// Commitments due tomorrow (soonest first).
    pub tomorrow_commitments: Vec<BriefItem>,
    /// Open loops that appeared today and are still open, ≤5.
    pub loose_ends: Vec<BriefItem>,
}

/// The shared confidence gate: Low excluded, Medium flagged (identical to the Brief's rule).
fn gate(text: &str, confidence: f64, provenance_event_id: i64) -> Option<BriefItem> {
    match band(confidence) {
        Band::Low => None,
        b => Some(BriefItem {
            text: text.to_string(),
            provenance_event_id,
            possibly: b == Band::Medium,
        }),
    }
}

/// "Still open": overdue commitments first, then stalest loops — the same priority signals the
/// rest of the product uses (FR-EB-02), capped at [`STILL_OPEN_MAX`].
fn still_open(commitments: &[CommitmentDue], loops: &[OpenLoopItem]) -> Vec<BriefItem> {
    let mut ranked: Vec<(i64, BriefItem)> = Vec::new();
    for c in commitments {
        if let Some(item) = gate(&c.description, c.confidence, c.provenance_event_id) {
            // overdue outranks everything; within commitments, sooner due first.
            let key = if c.overdue { i64::MIN } else { c.due_at_ms.unwrap_or(i64::MAX - 1) };
            ranked.push((key, item));
        }
    }
    for o in loops {
        if let Some(item) = gate(&o.description, o.confidence, o.provenance_event_id) {
            // loops rank below dated commitments, stalest first.
            ranked.push((i64::MAX - i64::from(o.staleness_days), item));
        }
    }
    ranked.sort_by_key(|(k, _)| *k);
    ranked.into_iter().take(STILL_OPEN_MAX).map(|(_, i)| i).collect()
}

/// Assemble the Evening Wrap (FR-EB-01). Callers supply pre-filtered day/tomorrow windows — the
/// daemon owns "what happened today"; this function owns order, caps and the confidence gate.
pub fn assemble_wrap(
    outcome: WrapOutcome,
    commitments_active_today: &[CommitmentDue],
    loops_active_today: &[OpenLoopItem],
    calendar_tomorrow: Vec<CalendarLine>,
    commitments_due_tomorrow: &[CommitmentDue],
    loops_opened_today: &[OpenLoopItem],
) -> EveningWrap {
    let mut cal = calendar_tomorrow;
    cal.sort_by_key(|c| c.start_ms);
    cal.truncate(TOMORROW_CALENDAR_MAX);

    let mut tomorrow: Vec<(&CommitmentDue, BriefItem)> = commitments_due_tomorrow
        .iter()
        .filter_map(|c| gate(&c.description, c.confidence, c.provenance_event_id).map(|i| (c, i)))
        .collect();
    tomorrow.sort_by_key(|(c, _)| c.due_at_ms.unwrap_or(i64::MAX));

    let loose: Vec<BriefItem> = loops_opened_today
        .iter()
        .filter_map(|o| gate(&o.description, o.confidence, o.provenance_event_id))
        .take(LOOSE_ENDS_MAX)
        .collect();

    EveningWrap {
        outcome,
        still_open: still_open(commitments_active_today, loops_active_today),
        tomorrow_calendar: cal,
        tomorrow_commitments: tomorrow.into_iter().map(|(_, i)| i).collect(),
        loose_ends: loose,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commitment(desc: &str, due: Option<i64>, overdue: bool, conf: f64, ev: i64) -> CommitmentDue {
        CommitmentDue { description: desc.into(), due_at_ms: due, overdue, confidence: conf, provenance_event_id: ev }
    }
    fn loop_item(desc: &str, staleness: u32, conf: f64, ev: i64) -> OpenLoopItem {
        OpenLoopItem { description: desc.into(), staleness_days: staleness, confidence: conf, provenance_event_id: ev }
    }
    fn cal(start: i64, title: &str) -> CalendarLine {
        CalendarLine { start_ms: start, title: title.into(), updated: false }
    }

    #[test]
    fn still_open_puts_overdue_first_then_dated_then_stalest_loops() {
        let wrap = assemble_wrap(
            WrapOutcome::default(),
            &[
                commitment("due later", Some(500), false, 0.9, 1),
                commitment("overdue", Some(100), true, 0.9, 2),
            ],
            &[loop_item("very stale", 9, 0.9, 3), loop_item("fresh loop", 1, 0.9, 4)],
            vec![],
            &[],
            &[],
        );
        let order: Vec<&str> = wrap.still_open.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(order, vec!["overdue", "due later", "very stale", "fresh loop"]);
    }

    #[test]
    fn still_open_is_capped_and_gated() {
        let commitments: Vec<CommitmentDue> =
            (0..7).map(|i| commitment(&format!("c{i}"), Some(i), false, 0.9, i)).collect();
        let wrap = assemble_wrap(
            WrapOutcome::default(),
            &commitments,
            &[loop_item("shaky", 5, 0.3, 99)], // Low → excluded (FR-ST-20)
            vec![],
            &[],
            &[],
        );
        assert_eq!(wrap.still_open.len(), STILL_OPEN_MAX);
        assert!(wrap.still_open.iter().all(|i| i.text != "shaky"));
    }

    #[test]
    fn tomorrow_calendar_is_time_ordered_and_capped_at_three() {
        let wrap = assemble_wrap(
            WrapOutcome::default(),
            &[],
            &[],
            vec![cal(400, "d"), cal(100, "a"), cal(300, "c"), cal(200, "b")],
            &[],
            &[],
        );
        let titles: Vec<&str> = wrap.tomorrow_calendar.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["a", "b", "c"]);
    }

    #[test]
    fn tomorrow_commitments_are_soonest_first_and_medium_flagged_possibly() {
        let wrap = assemble_wrap(
            WrapOutcome::default(),
            &[],
            &[],
            vec![],
            &[
                commitment("second", Some(200), false, 0.6, 1),
                commitment("first", Some(100), false, 0.9, 2),
            ],
            &[],
        );
        assert_eq!(wrap.tomorrow_commitments[0].text, "first");
        assert_eq!(wrap.tomorrow_commitments[1].text, "second");
        assert!(wrap.tomorrow_commitments[1].possibly, "0.6 must be flagged possibly");
    }

    #[test]
    fn loose_ends_are_gated_and_capped() {
        let loops: Vec<OpenLoopItem> = (0..8)
            .map(|i| loop_item(&format!("l{i}"), 0, if i == 0 { 0.2 } else { 0.9 }, i))
            .collect();
        let wrap = assemble_wrap(WrapOutcome::default(), &[], &[], vec![], &[], &loops);
        assert_eq!(wrap.loose_ends.len(), LOOSE_ENDS_MAX);
        assert!(wrap.loose_ends.iter().all(|i| i.text != "l0"), "low confidence excluded");
    }

    #[test]
    fn outcome_counts_pass_through() {
        let wrap = assemble_wrap(
            WrapOutcome { commitments_done: 3, loops_closed: 2, actions_decided: 5, actions_adopted: 4 },
            &[],
            &[],
            vec![],
            &[],
            &[],
        );
        assert_eq!(wrap.outcome.commitments_done, 3);
        assert_eq!(wrap.outcome.actions_adopted, 4);
    }
}
