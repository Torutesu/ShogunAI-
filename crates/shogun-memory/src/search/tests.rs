#[cfg(test)]
mod excerpt_tests {
    use crate::search::excerpt;

    #[test]
    fn short_content_is_returned_whole() {
        assert_eq!(
            excerpt("  send Alice the deck  ", "deck", 100),
            "send Alice the deck"
        );
    }

    #[test]
    fn window_centres_on_the_match_not_the_head() {
        // The interesting sentence is buried at the end of a long window capture.
        let content = format!(
            "{}NEEDLE pricing decision{}",
            "chrome ".repeat(200),
            " x".repeat(200)
        );
        let got = excerpt(&content, "needle", 80);
        assert!(
            got.contains("NEEDLE pricing decision"),
            "match must survive the cut: {got}"
        );
        assert!(
            got.chars().count() <= 82,
            "budget respected (plus ellipses): {got}"
        );
    }

    #[test]
    fn falls_back_to_the_head_when_nothing_matches() {
        let content = "alpha ".repeat(100);
        let got = excerpt(&content, "zzz", 40);
        assert!(got.starts_with("alpha"), "no match → head window: {got}");
        assert!(got.ends_with('…'));
    }

    #[test]
    fn never_splits_a_multi_byte_char() {
        // Pure multi-byte content: slicing by byte here would panic or produce invalid UTF-8.
        let content = "あ".repeat(500);
        let got = excerpt(&content, "あ", 50);
        assert!(got.chars().all(|c| c == 'あ' || c == '…'));
        assert!(got.chars().filter(|&c| c == 'あ').count() <= 50);
    }

    #[test]
    fn matching_is_case_insensitive_for_the_english_path() {
        let content = format!(
            "{}Quarterly Deck review{}",
            "z ".repeat(200),
            " y".repeat(200)
        );
        let got = excerpt(&content, "QUARTERLY", 60);
        assert!(
            got.contains("Quarterly Deck"),
            "case-insensitive hit expected: {got}"
        );
    }

    #[test]
    fn zero_budget_yields_nothing() {
        assert_eq!(excerpt("anything at all", "any", 0), "");
    }
}

use crate::event_log::{insert, NewEvent};
use crate::search::*;
use rusqlite::Connection;

fn add(conn: &Connection, content: &str, source: &str, hash: &str) -> i64 {
    insert(
        conn,
        &NewEvent {
            ts: 1,
            source,
            kind: "text",
            app_bundle_id: Some("com.apple.Safari"),
            window_title: Some("t"),
            content,
            content_hash: hash,
            dwell_ms: 0,
            display_id: None,
            window_bounds: None,
        },
    )
    .unwrap()
}

#[test]
fn rrf_ranks_items_appearing_high_in_multiple_lists_first() {
    // id 7 is rank 1 in one list and rank 2 in the other → should win.
    let a = [7, 3, 9];
    let b = [5, 7, 1];
    let fused = reciprocal_rank_fusion(&[&a, &b], 60.0);
    assert_eq!(fused[0].0, 7);
}

#[test]
fn rrf_is_deterministic_on_ties() {
    let a = [1, 2];
    let b = [2, 1];
    // 1 and 2 have identical fused scores; tie-break picks the smaller id first.
    let fused = reciprocal_rank_fusion(&[&a, &b], 60.0);
    assert_eq!(
        fused.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn rrf_empty_is_empty() {
    assert!(reciprocal_rank_fusion(&[], 60.0).is_empty());
    assert!(reciprocal_rank_fusion(&[&[]], 60.0).is_empty());
}

#[test]
fn fts_search_finds_and_orders() {
    let conn = crate::open_in_memory().unwrap();
    add(&conn, "the annual budget spreadsheet", "capture", "h1");
    add(&conn, "unrelated lunch plans", "capture", "h2");
    let ids = fts_search(&conn, "budget", 10).unwrap();
    assert_eq!(ids.len(), 1);
}

#[test]
fn empty_query_returns_nothing() {
    let conn = crate::open_in_memory().unwrap();
    add(&conn, "anything", "capture", "h1");
    assert!(fts_search(&conn, "   ", 10).unwrap().is_empty());
    assert!(search(&conn, "", 10).unwrap().is_empty());
}

#[test]
fn query_with_quotes_does_not_break() {
    let conn = crate::open_in_memory().unwrap();
    add(&conn, "he said \"ship it\" today", "capture", "h1");
    // A query containing a double quote must not be parsed as an FTS operator / must not error.
    let hits = search(&conn, "ship", 10).unwrap();
    assert_eq!(hits.len(), 1);
    let none = search(&conn, "\"", 10);
    assert!(none.is_ok());
}

#[test]
fn search_hydrates_with_source_attribution() {
    let conn = crate::open_in_memory().unwrap();
    add(&conn, "quarterly review notes", "gmail", "h1");
    let hits = search(&conn, "quarterly", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source, "gmail"); // FR-MEM-23 attribution
    assert!(hits[0].content.contains("quarterly"));
}

#[test]
fn search_meetings_finds_recap_by_query_term() {
    use crate::meeting_recaps;
    use crate::session::{open, NewSession};

    let conn = crate::open_in_memory().unwrap();
    let sid = open(
        &conn,
        &NewSession {
            kind: "meeting",
            started_at: 5_000,
            title: Some("Vendor pricing sync"),
            app_bundle_id: Some("us.zoom.xos"),
            calendar_occurrence_id: None,
            confidence: 0.8,
            provenance: "{}",
        },
    )
    .unwrap();
    meeting_recaps::save(
        &conn,
        sid,
        "Discussed renewal pricing and the 12k quote.",
        r#"["Approve the vendor renewal"]"#,
        r#"[{"text":"email procurement","owner":"Alice"}]"#,
        "claude-batch",
        6_000,
    )
    .unwrap();

    let hits = search_meetings(&conn, "vendor pricing", 5).unwrap();
    assert_eq!(hits.len(), 1, "recap matched by query: {hits:?}");
    assert!(hits[0].content.contains("12k"));
    assert_eq!(hits[0].title.as_deref(), Some("Vendor pricing sync"));
}

#[test]
fn search_meetings_prefers_the_relevant_session_not_the_latest() {
    use crate::meeting_recaps;
    use crate::session::{open, NewSession};

    let conn = crate::open_in_memory().unwrap();
    let old = open(
        &conn,
        &NewSession {
            kind: "meeting",
            started_at: 1_000,
            title: Some("Design review"),
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
    meeting_recaps::save(
        &conn,
        old,
        "Roadmap and launch timeline for Phoenix.",
        "[]",
        "[]",
        "m",
        2_000,
    )
    .unwrap();
    meeting_recaps::save(
        &conn,
        recent,
        "Nothing blocking today.",
        "[]",
        "[]",
        "m",
        10_000,
    )
    .unwrap();

    let hits = search_meetings(&conn, "Phoenix launch", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].session_id, old,
        "older but relevant meeting wins over latest"
    );
}

#[test]
fn search_meetings_finds_transcript_when_recap_is_missing() {
    use crate::session::{open, NewSession};
    use crate::transcript_segments::{append, NewSegment, Speaker};

    let conn = crate::open_in_memory().unwrap();
    let sid = open(
        &conn,
        &NewSession {
            kind: "meeting",
            started_at: 3_000,
            title: Some("Budget call"),
            app_bundle_id: None,
            calendar_occurrence_id: None,
            confidence: 0.8,
            provenance: "{}",
        },
    )
    .unwrap();
    append(
        &conn,
        &NewSegment {
            session_id: sid,
            ts: 3_100,
            speaker: Speaker::Other,
            text: "We agreed to cap infrastructure spend at forty thousand.",
            confidence: 0.9,
        },
        3_200,
    )
    .unwrap();

    let hits = search_meetings(&conn, "infrastructure spend", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].content.contains("forty thousand"));
}

#[test]
fn search_meetings_returns_nothing_for_unrelated_queries() {
    use crate::meeting_recaps;
    use crate::session::{open, NewSession};

    let conn = crate::open_in_memory().unwrap();
    let sid = open(
        &conn,
        &NewSession {
            kind: "meeting",
            started_at: 1_000,
            title: Some("Weekly sync"),
            app_bundle_id: None,
            calendar_occurrence_id: None,
            confidence: 0.8,
            provenance: "{}",
        },
    )
    .unwrap();
    meeting_recaps::save(
        &conn,
        sid,
        "Discussed hiring plans.",
        "[]",
        "[]",
        "m",
        2_000,
    )
    .unwrap();

    assert!(search_meetings(&conn, "vendor migration", 5)
        .unwrap()
        .is_empty());
}

#[test]
fn hybrid_search_finds_semantic_match_fts_would_miss() {
    use crate::embed::{Embedder, MockEmbedder, E5_SMALL_DIM};
    let conn = crate::open_in_memory().unwrap();
    let m = MockEmbedder::new(E5_SMALL_DIM);

    // A doc that shares tokens with the query but NOT the exact FTS term.
    let id = add(
        &conn,
        "the budget review meeting is on friday",
        "gmail",
        "h1",
    );
    let v = m
        .embed_passages(&["the budget review meeting is on friday"])
        .unwrap()[0]
        .clone();
    crate::vector::upsert(&conn, id, &v).unwrap();

    // Query term "standup" isn't in the doc (FTS finds nothing), but the embedding overlaps
    // on "review"/"meeting" so the vector list surfaces it — hybrid fusion returns it.
    let q = m.embed_query("review meeting standup").unwrap();
    let fts_only = search(&conn, "standup", 10).unwrap();
    assert!(fts_only.is_empty(), "FTS alone should miss it");
    let hybrid = search_hybrid(&conn, "standup", Some(&q), 10).unwrap();
    assert_eq!(
        hybrid.len(),
        1,
        "the vector half should surface the semantic match"
    );
    assert_eq!(hybrid[0].event_id, id);
}

/// The retrieval bug this file exists to prevent a repeat of: a question is not a phrase.
#[cfg(test)]
mod fts_query_tests {
    use crate::event_log::{insert, NewEvent};
    use crate::search::*;
    use rusqlite::Connection;

    fn seeded() -> Connection {
        let conn = crate::open_in_memory().unwrap();
        insert(
            &conn,
            &NewEvent {
                ts: 1,
                source: "capture",
                kind: "text",
                app_bundle_id: Some("com.apple.Safari"),
                window_title: Some("notes"),
                content: "The vendor renewal discussion continued; pricing was raised again.",
                content_hash: "h1",
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap();
        conn
    }

    #[test]
    fn a_multi_word_question_matches_without_being_contiguous() {
        // These words all appear, but never as this phrase — the whole-query-quoted version
        // returned nothing here, which silently emptied retrieval for most real questions.
        let conn = seeded();
        assert_eq!(
            fts_search(&conn, "vendor renewal pricing", 10)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            fts_search(&conn, "what did we decide about pricing?", 10)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn fts_operators_in_a_question_are_treated_as_text() {
        // Unquoted, these would parse as FTS5 syntax and error or silently mean something else.
        let conn = seeded();
        for q in [
            "pricing OR vendor",
            "vendor NEAR renewal",
            "pricing - vendor",
            "vendor*",
        ] {
            assert!(
                fts_search(&conn, q, 10).is_ok(),
                "must not be a syntax error: {q}"
            );
        }
        // A quote in the question must not break out of the quoting.
        assert!(fts_search(&conn, "he said \"pricing\" twice", 10).is_ok());
    }

    #[test]
    fn terms_too_short_for_the_trigram_index_are_dropped() {
        // "is"/"it" cannot match a trigram index; only the usable term survives.
        assert_eq!(fts_query("is it pricing"), Some("\"pricing\"".to_string()));
        // Function words carry no signal and would match nearly every document.
        assert_eq!(
            fts_query("what was the pricing"),
            Some("\"pricing\"".to_string())
        );
        // Nothing usable at all → no query, rather than an empty MATCH.
        assert_eq!(fts_query("is it"), None);
        assert_eq!(
            fts_query("what was that"),
            None,
            "all function words → nothing to retrieve on"
        );
        assert_eq!(fts_query("   "), None);
    }

    #[test]
    fn cjk_runs_are_expanded_into_trigrams() {
        // No spaces to split on, so the run becomes its overlapping trigrams.
        assert_eq!(
            fts_query("資料の期限"),
            Some("\"資料の\" OR \"料の期\" OR \"の期限\"".to_string())
        );
    }

    #[test]
    fn a_pathological_query_is_capped() {
        let huge = (0..500)
            .map(|i| format!("term{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let expr = fts_query(&huge).unwrap();
        assert_eq!(
            expr.matches(" OR ").count(),
            MAX_FTS_TERMS - 1,
            "term count is bounded"
        );
    }
}

/// The recency bound must speed things up without making older memory unreachable.
#[cfg(test)]
mod warm_window_tests {
    use crate::event_log::{insert, NewEvent};
    use crate::search::*;
    use rusqlite::Connection;

    const DAY: i64 = 24 * 60 * 60 * 1000;

    fn add(conn: &Connection, content: &str, hash: &str, ts: i64) {
        insert(
            conn,
            &NewEvent {
                ts,
                source: "capture",
                kind: "text",
                app_bundle_id: Some("com.apple.Safari"),
                window_title: Some("notes"),
                content,
                content_hash: hash,
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn the_bound_excludes_what_is_older_than_the_window() {
        let conn = crate::open_in_memory().unwrap();
        let now = 100 * DAY;
        // Chronological insertion — how capture and sync actually write.
        add(&conn, "ancient vendor pricing note", "h2", now - 90 * DAY);
        add(&conn, "recent vendor pricing note", "h1", now - DAY);

        let bounded =
            fts_search_since(&conn, "vendor pricing", Some(now - WARM_WINDOW_MS), 10).unwrap();
        assert_eq!(bounded.len(), 1, "only the in-window row");
        let unbounded = fts_search_since(&conn, "vendor pricing", None, 10).unwrap();
        assert_eq!(unbounded.len(), 2, "the whole history is still reachable");
    }

    /// The docid bound approximates the time bound, and the approximation errs the safe way.
    ///
    /// A backfilled older item gets a higher id than rows that predate it, so it can fall inside
    /// a docid range its timestamp is outside of. That returns an *extra* old row — harmless, it
    /// is ranked and capped like any other. The dangerous direction, silently dropping something
    /// recent, cannot happen: everything newer has a higher id by construction.
    #[test]
    fn an_out_of_order_backfill_may_be_included_but_nothing_recent_is_lost() {
        let conn = crate::open_in_memory().unwrap();
        let now = 100 * DAY;
        add(&conn, "recent vendor pricing note", "h1", now - DAY);
        add(
            &conn,
            "backfilled old vendor pricing note",
            "h2",
            now - 90 * DAY,
        );

        let bounded =
            fts_search_since(&conn, "vendor pricing", Some(now - WARM_WINDOW_MS), 10).unwrap();
        assert!(!bounded.is_empty(), "the recent row is never lost");
        let hits = hydrate(
            &conn,
            &bounded.iter().map(|id| (*id, 1.0)).collect::<Vec<_>>(),
        )
        .unwrap();
        assert!(
            hits.iter().any(|h| h.content.contains("recent")),
            "the in-window row must be present: {hits:?}"
        );
    }

    /// The bound must not turn "I asked about something old" into "SHOGUN has no idea".
    #[test]
    fn a_question_only_answered_by_old_memory_still_finds_it() {
        let conn = crate::open_in_memory().unwrap();
        let now = 100 * DAY;
        add(
            &conn,
            "the vendor migration was cancelled for downtime",
            "h1",
            now - 80 * DAY,
        );

        // Warm alone finds nothing, so the search widens rather than answering "nothing found".
        let warm_only = search_hybrid_since(
            &conn,
            "vendor migration",
            None,
            Some(now - WARM_WINDOW_MS),
            6,
        )
        .unwrap();
        assert!(warm_only.is_empty(), "precondition: outside the window");

        let escalated = search_warm_first(&conn, "vendor migration", None, now, 6).unwrap();
        assert_eq!(
            escalated.len(),
            1,
            "escalation reaches the old answer: {escalated:?}"
        );
        assert!(escalated[0].content.contains("cancelled"));
    }

    /// When the window does hold enough, the expensive full-history pass is not run.
    #[test]
    fn a_well_answered_question_stays_on_the_fast_path() {
        let conn = crate::open_in_memory().unwrap();
        let now = 100 * DAY;
        for i in 0..6 {
            add(
                &conn,
                "recent vendor pricing note",
                &format!("h{i}"),
                now - DAY - i,
            );
        }
        add(&conn, "ancient vendor pricing note", "old", now - 90 * DAY);

        let hits = search_warm_first(&conn, "vendor pricing", None, now, 6).unwrap();
        assert_eq!(hits.len(), 6);
        assert!(
            hits.iter().all(|h| h.ts > now - WARM_WINDOW_MS),
            "the old row must not appear when the window sufficed"
        );
    }

    #[test]
    fn screen_query_heuristic_matches_natural_phrases() {
        let days = LocalDayBounds {
            yesterday_start_ms: 0,
            today_start_ms: 86_400_000,
        };
        assert!(query_asks_about_screen("what was on my screen yesterday"));
        assert!(!query_asks_about_screen("vendor pricing email"));
        assert!(query_wants_visual_recall(
            "what was on my screen yesterday",
            0,
            days
        ));
        assert!(query_wants_visual_recall(
            "what did I see on screen today",
            86_400_000,
            days
        ));
        assert!(!query_wants_visual_recall(
            "what happened today",
            86_400_000,
            days
        ));
        let now = 86_400_000 * 2;
        let Some((from, to)) = query_time_window("yesterday", now, days) else {
            panic!("expected window");
        };
        assert_eq!(from, 0);
        assert_eq!(to, 86_400_000);
    }

    #[test]
    fn query_time_window_uses_exact_local_midnights() {
        // 2020-01-02 01:00 UTC = 2020-01-02 10:00 JST (+9h)
        let now = 1_577_894_400_000_i64;
        let days = LocalDayBounds {
            yesterday_start_ms: 1_577_804_400_000,
            today_start_ms: 1_577_890_800_000,
        };
        let Some((from, to)) = query_time_window("today", now, days) else {
            panic!("expected window");
        };
        assert_eq!(from, days.today_start_ms);
        assert_eq!(to, now);
        let Some((from, to)) = query_time_window("yesterday", now, days) else {
            panic!("expected window");
        };
        assert_eq!((from, to), (days.yesterday_start_ms, days.today_start_ms));
    }

    #[test]
    fn yesterday_window_can_span_a_dst_transition() {
        let days = LocalDayBounds {
            yesterday_start_ms: 1_000,
            today_start_ms: 1_000 + 23 * 60 * 60 * 1_000,
        };
        assert_eq!(
            query_time_window("yesterday", days.today_start_ms + 1, days),
            Some((days.yesterday_start_ms, days.today_start_ms))
        );
    }

    #[test]
    fn visual_recall_window_never_exceeds_selected_retention() {
        let now = 3 * 86_400_000;
        let days = LocalDayBounds {
            yesterday_start_ms: 0,
            today_start_ms: 2 * 86_400_000,
        };
        assert_eq!(
            visual_recall_window("what was on my screen yesterday", now, days, 86_400_000),
            (2 * 86_400_000, 2 * 86_400_000)
        );
        assert_eq!(
            visual_recall_window("show my screen", now, days, 2 * 86_400_000),
            (86_400_000, now)
        );
    }

    #[test]
    fn fts_search_source_scopes_to_one_tag() {
        let conn = crate::open_in_memory().unwrap();
        add(&conn, "quarterly roadmap slide text", "ocr1", 1_000);
        conn.execute(
            "UPDATE event_log SET source = 'screen_ocr' WHERE content_hash = 'ocr1'",
            [],
        )
        .unwrap();
        add(&conn, "quarterly roadmap from accessibility", "cap1", 1_100);

        let ocr_ids = fts_search_source(&conn, "roadmap", "screen_ocr", 5).unwrap();
        assert_eq!(ocr_ids.len(), 1);
        let cap_ids = fts_search_source(&conn, "roadmap", "capture", 5).unwrap();
        assert_eq!(cap_ids.len(), 1);
        assert_ne!(ocr_ids[0], cap_ids[0]);
    }
}

/// Cold-tier semantic search (design §2.1, E-09): the archive must be reachable on request and
/// untouchable by default.
#[cfg(test)]
mod cold_search_tests {
    use crate::cold::{self, PARTITION_MS};
    use crate::embed::{Embedder, MockEmbedder, E5_SMALL_DIM};
    use crate::event_log::{insert, NewEvent};
    use crate::search::*;
    use rusqlite::Connection;

    const DAY: i64 = 24 * 60 * 60 * 1000;

    fn add(conn: &Connection, content: &str, hash: &str, ts: i64) -> i64 {
        insert(
            conn,
            &NewEvent {
                ts,
                source: "capture",
                kind: "text",
                app_bundle_id: Some("com.apple.Safari"),
                window_title: Some("notes"),
                content,
                content_hash: hash,
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap()
    }

    fn embed(conn: &Connection, m: &MockEmbedder, id: i64, text: &str) {
        let v = m.embed_passages(&[text]).unwrap()[0].clone();
        crate::vector::upsert(conn, id, &v).unwrap();
    }

    /// One event well past the cutoff, embedded and demoted through the real demotion path, plus
    /// a fresh Warm event. The old content shares no token with the query "standup", so lexical
    /// search alone cannot surface it — only its embedding can.
    fn seeded_across_cutoff() -> (Connection, i64, i64) {
        let mut conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let now = 100 * PARTITION_MS;

        let old_text = "the budget review meeting is on friday";
        let old_id = add(&conn, old_text, "h_old", now - 80 * DAY);
        embed(&conn, &m, old_id, old_text);

        let fresh_text = "lunch plans for saturday afternoon";
        let fresh_id = add(&conn, fresh_text, "h_fresh", now - DAY);
        embed(&conn, &m, fresh_id, fresh_text);

        let moved = cold::demote_older_than(&mut conn, now - COLD_CUTOFF_MS).unwrap();
        assert_eq!(
            moved, 1,
            "precondition: the old embedding is demoted to Cold"
        );
        assert_eq!(cold::count(&conn).unwrap(), 1);
        (conn, now, old_id)
    }

    #[test]
    fn warm_only_never_touches_cold() {
        let (conn, now, old_id) = seeded_across_cutoff();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let q = m.embed_query("review meeting standup").unwrap();

        // Default options (WarmOnly, no explicit floor) — the archive stays closed.
        let res = search_hybrid_with_options(
            &conn,
            "standup",
            Some(&q),
            now,
            &SearchOptions::default(),
            10,
        )
        .unwrap();
        assert_eq!(
            res.cold,
            ColdScanStats::default(),
            "no partition opened, no row scanned"
        );
        assert!(
            res.hits.iter().all(|h| h.event_id != old_id),
            "the demoted event must not surface on the Warm path: {:?}",
            res.hits
        );

        // Same with an explicit floor inside the Warm window.
        let opts = SearchOptions {
            since_ts: Some(now - 7 * DAY),
            ..Default::default()
        };
        let res = search_hybrid_with_options(&conn, "standup", Some(&q), now, &opts, 10).unwrap();
        assert_eq!(res.cold, ColdScanStats::default());

        // And default options reproduce the pre-existing entry point exactly.
        let legacy = search_hybrid(&conn, "standup", Some(&q), 10).unwrap();
        let via_opts = search_hybrid_with_options(
            &conn,
            "standup",
            Some(&q),
            now,
            &SearchOptions::default(),
            10,
        )
        .unwrap();
        assert_eq!(
            legacy, via_opts.hits,
            "default options must not change existing behavior"
        );
    }

    #[test]
    fn depth_all_finds_an_old_semantic_match_lexical_search_misses() {
        let (conn, now, old_id) = seeded_across_cutoff();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let q = m.embed_query("review meeting standup").unwrap();

        // Lexical alone: "standup" appears nowhere, so even unbounded FTS returns nothing.
        assert!(
            fts_search(&conn, "standup", 10).unwrap().is_empty(),
            "precondition: FTS miss"
        );

        let opts = SearchOptions {
            depth: SearchDepth::All,
            ..Default::default()
        };
        let res = search_hybrid_with_options(&conn, "standup", Some(&q), now, &opts, 10).unwrap();
        assert!(
            res.cold.partitions_visited >= 1,
            "the archive was actually opened"
        );
        assert!(res.cold.rows_scanned >= 1);
        assert!(
            res.hits.iter().any(|h| h.event_id == old_id),
            "the >30-day-old semantic match must surface via the Cold RRF source: {:?}",
            res.hits
        );
    }

    #[test]
    fn an_explicit_range_past_the_cutoff_reaches_cold_without_depth_all() {
        let (conn, now, old_id) = seeded_across_cutoff();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let q = m.embed_query("review meeting standup").unwrap();

        // depth stays WarmOnly, but the asked-for window plainly includes Cold territory.
        let opts = SearchOptions {
            since_ts: Some(now - 90 * DAY),
            ..Default::default()
        };
        let res = search_hybrid_with_options(&conn, "standup", Some(&q), now, &opts, 10).unwrap();
        assert!(
            res.cold.partitions_visited >= 1,
            "an explicitly old range implies Cold"
        );
        assert!(res.hits.iter().any(|h| h.event_id == old_id));
    }

    #[test]
    fn partition_cap_limits_scanning_newest_first() {
        let mut conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        // Five populated partitions, one event each, at periods 10..14.
        let mut ids = Vec::new();
        for i in 0..5i64 {
            let ts = (10 + i) * PARTITION_MS + 5;
            let text = format!("archived note number {i}");
            let id = add(&conn, &text, &format!("h{i}"), ts);
            embed(&conn, &m, id, &text);
            assert!(cold::demote(&mut conn, id, ts).unwrap());
            ids.push(id);
        }
        let q = m.embed_query("archived note").unwrap();
        let range = (0, 20 * PARTITION_MS);

        let started = std::time::Instant::now();
        let capped = search_cold_partitions(&conn, &q, range, 2, 10).unwrap();
        println!(
            "cold scan: {} partitions / {} rows in {:?}",
            capped.stats.partitions_visited,
            capped.stats.rows_scanned,
            started.elapsed()
        );
        assert_eq!(capped.stats.partitions_visited, 2, "cap bounds the scan");
        assert_eq!(capped.stats.rows_scanned, 2);
        // Newest partitions win the cap slots: only the two most recent events are reachable.
        assert_eq!(capped.ids.len(), 2);
        assert!(
            capped.ids.contains(&ids[4]) && capped.ids.contains(&ids[3]),
            "{:?}",
            capped.ids
        );

        // Uncapped (default cap ≥ 5): every populated partition is visited.
        let full =
            search_cold_partitions(&conn, &q, range, DEFAULT_MAX_COLD_PARTITIONS, 10).unwrap();
        assert_eq!(full.stats.partitions_visited, 5);
        assert_eq!(full.stats.rows_scanned, 5);
        assert_eq!(full.ids.len(), 5);

        // The time range prunes partitions before the cap does.
        let narrow =
            search_cold_partitions(&conn, &q, (13 * PARTITION_MS, 20 * PARTITION_MS), 6, 10)
                .unwrap();
        assert_eq!(
            narrow.stats.partitions_visited, 2,
            "only periods 13 and 14 intersect"
        );
    }

    #[test]
    fn cold_scan_ranks_by_similarity_and_breaks_ties_deterministically() {
        let mut conn = crate::open_in_memory().unwrap();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let ts = 10 * PARTITION_MS + 5;

        // Two identical contents (identical vectors → identical scores) and one distant decoy.
        let a = add(&conn, "vendor contract renewal", "ha", ts);
        let b = add(&conn, "vendor contract renewal", "hb", ts + 1);
        let decoy = add(&conn, "weekend hiking photos", "hc", ts + 2);
        for (id, text) in [
            (a, "vendor contract renewal"),
            (b, "vendor contract renewal"),
            (decoy, "weekend hiking photos"),
        ] {
            embed(&conn, &m, id, text);
            assert!(cold::demote(&mut conn, id, ts).unwrap());
        }
        let q = m.embed_query("vendor contract renewal").unwrap();
        let range = (0, 20 * PARTITION_MS);

        let scan = search_cold_partitions(&conn, &q, range, 6, 3).unwrap();
        assert_eq!(
            scan.ids,
            vec![a, b, decoy],
            "score order, ties by ascending id"
        );

        // k=1 under a tie must deterministically keep the smaller id.
        let top1 = search_cold_partitions(&conn, &q, range, 6, 1).unwrap();
        assert_eq!(top1.ids, vec![a]);
    }

    #[test]
    fn rrf_merge_with_cold_source_is_stable_and_deterministic() {
        let (conn, now, _old_id) = seeded_across_cutoff();
        let m = MockEmbedder::new(E5_SMALL_DIM);
        // A query that exercises all three sources: "friday" hits FTS (old event text is still
        // indexed), the embedding hits Warm KNN and the Cold scan.
        let q = m.embed_query("budget review friday").unwrap();
        let opts = SearchOptions {
            depth: SearchDepth::All,
            ..Default::default()
        };

        let first =
            search_hybrid_with_options(&conn, "budget friday", Some(&q), now, &opts, 10).unwrap();
        let second =
            search_hybrid_with_options(&conn, "budget friday", Some(&q), now, &opts, 10).unwrap();
        assert!(!first.hits.is_empty());
        assert_eq!(first.hits, second.hits, "same inputs, same fused ranking");
        assert_eq!(first.cold, second.cold);
        // Scores are strictly ordered best-first (RRF output is sorted and hydration preserves it).
        assert!(first.hits.windows(2).all(|w| w[0].score >= w[1].score));
    }

    #[test]
    fn cold_scan_degenerate_inputs_return_empty() {
        let conn = crate::open_in_memory().unwrap();
        let q = vec![0.5f32; E5_SMALL_DIM];
        // Empty archive, zero cap, zero k, inverted range — all empty, none error.
        assert!(search_cold_partitions(&conn, &q, (0, i64::MAX), 6, 10)
            .unwrap()
            .ids
            .is_empty());
        assert!(search_cold_partitions(&conn, &q, (0, 100), 0, 10)
            .unwrap()
            .ids
            .is_empty());
        assert!(search_cold_partitions(&conn, &q, (0, 100), 6, 0)
            .unwrap()
            .ids
            .is_empty());
        assert!(search_cold_partitions(&conn, &q, (100, 0), 6, 10)
            .unwrap()
            .ids
            .is_empty());
        assert!(search_cold_partitions(&conn, &[], (0, 100), 6, 10)
            .unwrap()
            .ids
            .is_empty());
    }
}
