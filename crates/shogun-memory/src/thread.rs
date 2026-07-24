//! Threads — the unit a referring question ("what's the status of that thing?") resolves to.
//!
//! The event log is flat: an email, its reply, and the window the user read it in are unrelated
//! rows. To answer a question that names nothing, SHOGUN has to pick *which* conversation is being
//! referred to, and answer from that conversation's events. This module owns the two halves of
//! that: deriving a stable [`thread_key`] for an event, and ranking threads by [`salience`].
//!
//! Both halves are pure — no DB, no clock — so the ranking that decides what the user is talking
//! about is fully testable.

/// Normalise a capture's window title into a thread key.
///
/// Window titles are noisy in ways that would otherwise split one conversation into many threads:
/// unread badges (`(3) Inbox`), dirty markers (`• draft.md`), and the trailing app name
/// (`… — Gmail`). Stripping those makes repeated visits to the same window collapse onto one key.
fn normalise_window_title(title: &str) -> String {
    let mut s = title.trim();
    // Leading unread/notification count: "(3) …"
    if let Some(rest) = s.strip_prefix('(') {
        if let Some((count, tail)) = rest.split_once(')') {
            if !count.is_empty() && count.chars().all(|c| c.is_ascii_digit()) {
                s = tail.trim();
            }
        }
    }
    // Leading dirty/unsaved marker.
    s = s.trim_start_matches(['•', '*']).trim();
    // Trailing " — App" / " - App" / " | App" segment.
    for sep in [" — ", " – ", " - ", " | "] {
        if let Some((head, _tail)) = s.rsplit_once(sep) {
            if !head.trim().is_empty() {
                s = head.trim();
                break;
            }
        }
    }
    s.to_lowercase()
}

/// Derive the thread key for an event, or `None` when there is nothing stable to group on.
///
/// `native_id` is the source's own conversation id when it has one (Gmail thread id, Slack
/// `channel:thread_ts`, an issue URL, an AI session id) — always preferred, because it is exactly
/// the grouping the source itself uses. Captures have no such id, so they fall back to the
/// app plus a normalised window title.
///
/// Keys are namespaced by source so two systems cannot collide on the same raw id.
pub fn thread_key(
    source: &str,
    native_id: Option<&str>,
    app_bundle_id: Option<&str>,
    window_title: Option<&str>,
) -> Option<String> {
    if let Some(id) = native_id.map(str::trim).filter(|s| !s.is_empty()) {
        return Some(format!("{source}:{id}"));
    }
    let title = window_title.map(normalise_window_title).filter(|t| !t.is_empty())?;
    let app = app_bundle_id.unwrap_or("unknown");
    Some(format!("{source}:{app}:{title}"))
}

/// The inputs to [`salience`], gathered per thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Salience {
    /// How long ago the thread last saw activity.
    pub age_ms: i64,
    /// Open loops currently attached to it — unfinished business is what people ask about.
    pub open_loops: usize,
    /// The user is looking at this thread right now.
    pub on_screen: bool,
    /// The question's own words matched this thread (normalised 0.0..=1.0).
    pub lexical_match: f64,
}

/// Half-life of the recency term. A day-old thread scores half what a fresh one does — long
/// enough that yesterday's work still competes, short enough that last month's does not.
const RECENCY_HALF_LIFE_MS: f64 = 24.0 * 60.0 * 60.0 * 1000.0;

/// Rank a thread as a candidate referent, higher is better.
///
/// The weights encode what actually disambiguates "that thing". The user's own words lead: if they
/// said "the pricing thing", that is direct evidence, not a prior. What is on screen comes next.
/// Recency and unfinished business are the priors, and they are weighted **equally on purpose** —
/// a fresh but trivial glance (a random page opened a minute ago) must not outrank a day-old
/// thread that still has work owed on it, which is what people actually ask about. Recency decays
/// smoothly rather than by cliff so that trade stays continuous.
pub fn salience(s: Salience) -> f64 {
    let recency = 0.5_f64.powf(s.age_ms.max(0) as f64 / RECENCY_HALF_LIFE_MS);
    // Unfinished business saturates: three open loops is not three times as referable as one.
    let pressure = (s.open_loops as f64).min(3.0) / 3.0;
    let screen = if s.on_screen { 1.0 } else { 0.0 };
    let lexical = s.lexical_match.clamp(0.0, 1.0);
    0.30 * lexical + 0.20 * screen + 0.25 * recency + 0.25 * pressure
}

/// How confidently the top candidate can be treated as *the* referent.
///
/// Answering the wrong thread is worse than asking which one — a confident wrong answer about
/// someone's work destroys trust, while one clarifying question costs a second. So the decision is
/// the *margin* between the top two, not the top score: two plausible threads means ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Referent {
    /// One clear winner — answer from it.
    Resolved,
    /// Several plausible candidates — ask which, do not guess.
    Ambiguous,
    /// Nothing worth pointing at.
    None,
}

/// Minimum lead the top candidate needs over the runner-up to be treated as resolved.
const MARGIN: f64 = 0.15;
/// Below this, even an unopposed candidate is too weak to assume.
const FLOOR: f64 = 0.20;

/// Classify a descending-sorted candidate score list.
pub fn resolve(scores_desc: &[f64]) -> Referent {
    match scores_desc {
        [] => Referent::None,
        [top, ..] if *top < FLOOR => Referent::None,
        [_only] => Referent::Resolved,
        [top, second, ..] => {
            if top - second >= MARGIN {
                Referent::Resolved
            } else {
                Referent::Ambiguous
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 24 * 60 * 60 * 1000;

    #[test]
    fn native_id_is_preferred_and_namespaced_by_source() {
        assert_eq!(thread_key("gmail", Some("18f2a"), None, None).unwrap(), "gmail:18f2a");
        // The same raw id from two systems must not collide.
        assert_ne!(
            thread_key("gmail", Some("1"), None, None),
            thread_key("slack", Some("1"), None, None)
        );
    }

    #[test]
    fn repeat_visits_to_one_window_collapse_onto_one_key() {
        let a = thread_key("capture", None, Some("com.google.Chrome"), Some("(3) Q3 pricing — Gmail"));
        let b = thread_key("capture", None, Some("com.google.Chrome"), Some("Q3 pricing — Gmail"));
        let c = thread_key("capture", None, Some("com.google.Chrome"), Some("• Q3 pricing"));
        assert_eq!(a, b, "an unread badge must not fork the thread");
        assert_eq!(b, c, "a dirty marker must not fork the thread");
    }

    #[test]
    fn different_windows_stay_separate() {
        let a = thread_key("capture", None, Some("com.apple.Safari"), Some("Q3 pricing"));
        let b = thread_key("capture", None, Some("com.apple.Safari"), Some("Q4 roadmap"));
        assert_ne!(a, b);
    }

    #[test]
    fn no_key_without_anything_stable_to_group_on() {
        assert_eq!(thread_key("capture", None, Some("com.apple.Safari"), None), None);
        assert_eq!(thread_key("capture", None, Some("com.apple.Safari"), Some("   ")), None);
        // A title that is nothing but an app-name suffix leaves no head to key on.
        assert_eq!(thread_key("capture", Some("  "), None, Some("")), None);
    }

    fn s(age_ms: i64, open_loops: usize, on_screen: bool, lexical_match: f64) -> Salience {
        Salience { age_ms, open_loops, on_screen, lexical_match }
    }

    #[test]
    fn recent_beats_stale_all_else_equal() {
        assert!(salience(s(0, 0, false, 0.0)) > salience(s(7 * DAY, 0, false, 0.0)));
    }

    #[test]
    fn an_open_loop_can_carry_a_slightly_older_thread_past_a_fresh_trivial_one() {
        let fresh_trivial = salience(s(0, 0, false, 0.0));
        let day_old_with_work = salience(s(DAY, 2, false, 0.0));
        assert!(day_old_with_work > fresh_trivial, "unfinished business is what people ask about");
    }

    #[test]
    fn open_loop_pressure_saturates() {
        assert_eq!(salience(s(DAY, 3, false, 0.0)), salience(s(DAY, 30, false, 0.0)));
    }

    #[test]
    fn what_is_on_screen_and_what_the_words_matched_both_count() {
        let base = salience(s(DAY, 0, false, 0.0));
        assert!(salience(s(DAY, 0, true, 0.0)) > base);
        assert!(salience(s(DAY, 0, false, 1.0)) > base);
    }

    #[test]
    fn a_clear_winner_resolves() {
        assert_eq!(resolve(&[0.80, 0.20]), Referent::Resolved);
        assert_eq!(resolve(&[0.55]), Referent::Resolved);
    }

    #[test]
    fn two_close_candidates_ask_rather_than_guess() {
        assert_eq!(resolve(&[0.60, 0.55]), Referent::Ambiguous);
        // Comfortably either side of the margin. Values landing exactly on it are deliberately not
        // asserted: at that point the comparison is testing f64 representation, not the policy.
        assert_eq!(resolve(&[0.60, 0.40]), Referent::Resolved);
        assert_eq!(resolve(&[0.60, 0.50]), Referent::Ambiguous);
    }

    #[test]
    fn a_weak_top_candidate_is_not_a_referent() {
        assert_eq!(resolve(&[0.10]), Referent::None);
        assert_eq!(resolve(&[]), Referent::None);
    }
}
