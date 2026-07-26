//! Recap assembly (FR-MT-16, FR-MT-19).
//!
//! MT2 builds the **degraded** Recap: what can be said about a meeting from the interval itself —
//! the user's notes, the title, the duration, the text captured while it ran. No model is called
//! here. MT4 adds the summary and the extracted commitments on top; this is what gets shown in
//! the meantime, and what gets shown when the summary does not arrive inside 60 seconds.
//!
//! The rule this module exists to keep: **never show an empty Recap** (FR-MT-19). A meeting the
//! user agreed to have noted must come back as *something*, even if that something is only "32
//! minutes with Zoom, here are your notes". A blank panel reads as "your meeting was lost".

use shogun_memory::session::Session;

/// The degraded Recap: assembled locally, no model, no network (L1).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Recap {
    /// Meeting title, or a stated fallback — never an empty string.
    pub title: String,
    /// Whole minutes the interval ran. `None` while it is still open.
    pub duration_minutes: Option<i64>,
    /// What the user typed, if anything (FR-MT-10).
    pub notes: Option<String>,
    /// How many events were captured inside the interval — the honest measure of how much this
    /// Recap had to work with.
    pub captured_events: usize,
    /// True when this is the fallback shown because the full Recap is not ready (FR-MT-19).
    pub degraded: bool,
}

/// Words used when a meeting has no title at all. Stated plainly rather than left blank: an empty
/// header looks like a bug, "Untitled meeting" looks like a meeting nobody named.
pub const UNTITLED: &str = "Untitled meeting";

/// Build the degraded Recap for a closed (or still-open) session.
pub fn degraded(session: &Session, notes: Option<String>, captured_events: usize) -> Recap {
    let title = session
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or(UNTITLED)
        .to_string();

    // A duration is only reported when it is one. An open interval has none yet, and a clock that
    // moved backwards (NTP, sleep/wake) would otherwise render "-4 minutes" in the header — worse
    // than showing no number.
    let duration_minutes = session
        .ended_at
        .map(|end| end - session.started_at)
        .filter(|elapsed| *elapsed >= 0)
        .map(|elapsed| elapsed / 60_000);

    Recap { title, duration_minutes, notes, captured_events, degraded: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(title: Option<&str>, started_at: i64, ended_at: Option<i64>) -> Session {
        Session {
            id: 1,
            kind: "meeting".into(),
            started_at,
            ended_at,
            title: title.map(str::to_string),
            app_bundle_id: Some("us.zoom.xos".into()),
            calendar_occurrence_id: None,
            confidence: 0.65,
        }
    }

    #[test]
    fn a_meeting_with_nothing_captured_still_produces_a_recap() {
        // The property that matters most here: agreeing to have a meeting noted and getting a
        // blank panel back reads as "it was lost" (FR-MT-19).
        let s = session(Some("Weekly sync"), 0, Some(32 * 60_000));
        let r = degraded(&s, None, 0);

        assert_eq!(r.title, "Weekly sync");
        assert_eq!(r.duration_minutes, Some(32));
        assert!(r.degraded);
    }

    #[test]
    fn an_untitled_meeting_is_named_rather_than_left_blank() {
        let r = degraded(&session(None, 0, Some(60_000)), None, 0);
        assert_eq!(r.title, UNTITLED);
    }

    #[test]
    fn an_empty_title_is_treated_as_no_title() {
        // A window title can be whitespace. Rendering that as the header shows an empty box.
        let r = degraded(&session(Some("   "), 0, Some(60_000)), None, 0);
        assert_eq!(r.title, UNTITLED);
    }

    #[test]
    fn the_users_notes_are_carried_through_untouched() {
        let s = session(Some("1:1"), 0, Some(60_000));
        let r = degraded(&s, Some("- raise discussed".into()), 3);
        assert_eq!(r.notes.as_deref(), Some("- raise discussed"));
    }

    #[test]
    fn duration_is_whole_minutes_of_the_interval() {
        let s = session(Some("Standup"), 1_000, Some(1_000 + 9 * 60_000 + 30_000));
        assert_eq!(degraded(&s, None, 0).duration_minutes, Some(9), "9m30s reads as 9 minutes");
    }

    #[test]
    fn a_meeting_still_running_has_no_duration_yet() {
        let s = session(Some("Weekly sync"), 1_000, None);
        assert_eq!(degraded(&s, None, 0).duration_minutes, None);
    }

    #[test]
    fn a_clock_that_went_backwards_does_not_produce_a_negative_duration() {
        // NTP correction or sleep/wake can move the clock. "-4 minutes" in the header is worse
        // than no number at all.
        let s = session(Some("Weekly sync"), 5_000, Some(1_000));
        assert_eq!(degraded(&s, None, 0).duration_minutes, None);
    }

    #[test]
    fn the_recap_reports_how_much_it_had_to_work_with() {
        let s = session(Some("Weekly sync"), 0, Some(60_000));
        assert_eq!(degraded(&s, None, 41).captured_events, 41);
    }
}
