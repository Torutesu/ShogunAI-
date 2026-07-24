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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Referent {
    /// One clear winner — answer from it.
    Resolved,
    /// Several plausible candidates — ask which, do not guess.
    Ambiguous,
    /// Nothing worth pointing at.
    #[default]
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

/// Does this question refer to something without naming it ("how's that going?")?
///
/// A named question ("what did Alice say about pricing?") is answered by search; a referring one
/// has to be resolved to a thread first, and getting that wrong means confidently answering about
/// the wrong piece of work. The cues are deliberately narrow — a false positive sends a
/// perfectly answerable question down the disambiguation path.
///
/// English leads, as everywhere; the Japanese forms are additive and, being in their own script,
/// cannot fire on English text.
pub fn is_referring(query: &str) -> bool {
    let q = query.trim().to_lowercase();
    const EN: &[&str] = &[
        "that thing", "that one", "that project", "that issue", "that ticket",
        "the thing we", "the one we", "what we discussed", "what we talked about",
        "how's that", "how is that", "where are we on that", "any update on that",
        "status of that", "that other",
    ];
    const JA: &[&str] = &["あの件", "その件", "例の", "さっきの", "先ほどの", "この件"];
    EN.iter().chain(JA.iter()).any(|c| q.contains(c))
}

// ------------------------------------------------------------------ persistence

/// A thread row as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadRow {
    pub id: i64,
    pub thread_key: String,
    pub source: String,
    pub title: Option<String>,
    pub last_activity_at: i64,
    pub event_count: i64,
}

/// Record that an event belongs to a thread, creating the thread on first sight and extending its
/// activity window otherwise.
///
/// The title is only set when the thread has none: the first title seen is usually the cleanest,
/// and later captures of the same window often carry noisier variants.
pub fn upsert_from_event(
    conn: &rusqlite::Connection,
    thread_key: &str,
    source: &str,
    title: Option<&str>,
    ts: i64,
) -> Result<i64, rusqlite::Error> {
    use rusqlite::params;
    conn.execute(
        "INSERT INTO threads
           (thread_key, source, title, first_activity_at, last_activity_at, event_count,
            salience, confidence, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4, 1, 0.0, 0.5, ?4, ?4)
         ON CONFLICT(thread_key) DO UPDATE SET
           last_activity_at = max(last_activity_at, excluded.last_activity_at),
           first_activity_at = min(first_activity_at, excluded.first_activity_at),
           event_count = event_count + 1,
           title = COALESCE(title, excluded.title),
           updated_at = excluded.updated_at",
        params![thread_key, source, title, ts],
    )?;
    conn.query_row("SELECT id FROM threads WHERE thread_key = ?1", params![thread_key], |r| r.get(0))
}

/// The most recently active threads — the candidate pool a referring question is resolved against.
/// Bounded because salience is dominated by recency: a thread untouched for weeks is not what
/// "that thing" means.
pub fn recent(
    conn: &rusqlite::Connection,
    limit: usize,
) -> Result<Vec<ThreadRow>, rusqlite::Error> {
    use rusqlite::params;
    let mut stmt = conn.prepare(
        "SELECT id, thread_key, source, title, last_activity_at, event_count
           FROM threads ORDER BY last_activity_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |r| {
        Ok(ThreadRow {
            id: r.get(0)?,
            thread_key: r.get(1)?,
            source: r.get(2)?,
            title: r.get(3)?,
            last_activity_at: r.get(4)?,
            event_count: r.get(5)?,
        })
    })?;
    rows.collect()
}

/// Open loops currently attached to each thread, via the events that evidence them.
pub fn open_loop_counts(
    conn: &rusqlite::Connection,
) -> Result<std::collections::HashMap<String, usize>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT e.thread_key, count(DISTINCT l.id)
           FROM open_loops l
           JOIN state_provenance p ON p.state_id = l.id AND p.state_table = 'open_loops'
           JOIN event_log e ON e.id = p.event_id
          WHERE l.status = 'open' AND e.thread_key IS NOT NULL
          GROUP BY e.thread_key",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize)))?;
    rows.collect::<Result<Vec<_>, _>>().map(|v| v.into_iter().collect())
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

    #[test]
    fn referring_questions_are_recognised_in_both_languages() {
        for q in [
            "how's that going?",
            "any update on that?",
            "what we discussed yesterday",
            "あの件どうなってる?",
            "その件の進捗は",
        ] {
            assert!(is_referring(q), "should be referring: {q}");
        }
    }

    #[test]
    fn named_questions_are_not_sent_down_the_disambiguation_path() {
        // These are answerable directly; treating them as referring would make SHOGUN ask a
        // pointless clarifying question.
        for q in [
            "what did Alice say about pricing?",
            "when is the vendor renewal due?",
            "send the deck",
            "資料の期限は?",
        ] {
            assert!(!is_referring(q), "should not be referring: {q}");
        }
    }

    fn conn() -> rusqlite::Connection {
        crate::open_in_memory().unwrap()
    }

    #[test]
    fn a_thread_is_created_once_then_extended() {
        let c = conn();
        let a = upsert_from_event(&c, "gmail:1", "gmail", Some("Q3 pricing"), 100).unwrap();
        let b = upsert_from_event(&c, "gmail:1", "gmail", Some("(2) Q3 pricing"), 300).unwrap();
        assert_eq!(a, b, "same key is one thread");

        let rows = recent(&c, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].event_count, 2);
        assert_eq!(rows[0].last_activity_at, 300, "activity window extends");
        assert_eq!(rows[0].title.as_deref(), Some("Q3 pricing"), "the first title is kept");
    }

    #[test]
    fn an_out_of_order_event_does_not_rewind_the_activity_window() {
        let c = conn();
        upsert_from_event(&c, "gmail:1", "gmail", None, 500).unwrap();
        upsert_from_event(&c, "gmail:1", "gmail", None, 100).unwrap();
        let rows = recent(&c, 10).unwrap();
        assert_eq!(rows[0].last_activity_at, 500, "a late-arriving older event must not rewind it");
    }

    #[test]
    fn recent_threads_come_back_newest_first() {
        let c = conn();
        upsert_from_event(&c, "a", "capture", None, 100).unwrap();
        upsert_from_event(&c, "b", "capture", None, 300).unwrap();
        upsert_from_event(&c, "c", "capture", None, 200).unwrap();
        let keys: Vec<String> = recent(&c, 10).unwrap().into_iter().map(|t| t.thread_key).collect();
        assert_eq!(keys, vec!["b", "c", "a"]);
    }
}
