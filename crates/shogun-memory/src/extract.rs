//! Inline extraction — first stage (WP2.7). **Local heuristic rules only.**
//!
//! This is the cheap, always-on pass that turns captured text into *candidate* commitments and
//! open loops. It makes **no model call** — Batch API classification is deliberately deferred to
//! M3's Dream Cycle, where the model-call plumbing is centralized (invariant 5: Select KK vs BYOK
//! key separation must not leak into the capture path). So everything here is regex-free string
//! matching over lowercased segments.
//!
//! Because these are shallow heuristics, every candidate is emitted at **low confidence**
//! (≤ [`LOCAL_RULE_MAX_CONFIDENCE`], below the Medium threshold of 0.5). That keeps the promise in
//! FR-ST-20 / the confidence invariant: Context Fusion must treat these as "possibly", never mix
//! them into a generation as fact. The second stage (M3) can raise confidence or discard them.
//!
//! FR-ST-11 (commitments are for *explicit-promise* evidence only) is honoured by matching on
//! explicit promise cues ("I'll send…", "will get back to you") rather than inferring intent.

use rusqlite::Connection;

use crate::state::{
    insert_commitment, insert_open_loop, CommitmentDirection, CommitmentStatus, NewCommitment,
    NewOpenLoop, OpenLoopKind, Provenance,
};
use crate::MemoryError;

/// Upper bound on the confidence any local rule may assign. Deliberately under the Medium
/// threshold (0.5) so downstream gates always classify a heuristic candidate as low-confidence.
pub const LOCAL_RULE_MAX_CONFIDENCE: f64 = 0.4;

/// Longest description we keep for a candidate; captured segments can be long and the state row
/// only needs the gist.
const MAX_DESCRIPTION_LEN: usize = 200;

/// A single candidate produced by the local rules — either a commitment or an open loop, with the
/// text it was extracted from and a low confidence. Persistence is a separate step
/// ([`persist_candidates`]) so the pure extraction stays testable without a database.
#[derive(Debug, Clone, PartialEq)]
pub enum Candidate {
    Commitment { direction: CommitmentDirection, description: String, confidence: f64 },
    OpenLoop { kind: OpenLoopKind, description: String, confidence: f64 },
}

impl Candidate {
    /// The confidence assigned, whichever variant this is.
    pub fn confidence(&self) -> f64 {
        match self {
            Candidate::Commitment { confidence, .. } | Candidate::OpenLoop { confidence, .. } => {
                *confidence
            }
        }
    }
}

/// One sentence-ish unit of the captured text, with whether it was a question (the terminator was
/// `?`). Segmentation is intentionally crude — capture text is not clean prose.
struct Segment<'a> {
    text: &'a str,
    is_question: bool,
}

/// Split text into segments on newlines and sentence terminators, recording whether each ended in
/// `?`. Empty / whitespace-only segments are dropped.
fn segments(text: &str) -> Vec<Segment<'_>> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if matches!(b, b'\n' | b'.' | b'!' | b'?') {
            let slice = text[start..i].trim();
            if !slice.is_empty() {
                out.push(Segment { text: slice, is_question: b == b'?' });
            }
            start = i + 1;
        }
    }
    // trailing segment with no terminator
    let tail = text[start..].trim();
    if !tail.is_empty() {
        out.push(Segment { text: tail, is_question: false });
    }
    out
}

/// Truncate a description to [`MAX_DESCRIPTION_LEN`], on a char boundary, without splitting a UTF-8
/// codepoint (important for Japanese capture text).
fn clip(s: &str) -> String {
    if s.len() <= MAX_DESCRIPTION_LEN {
        return s.to_string();
    }
    let mut end = MAX_DESCRIPTION_LEN;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// First-person promise cues → my commitment (FR-ST-11: explicit promise).
const MINE_CUES: &[&str] =
    &["i'll ", "i will ", "i'm going to ", "let me ", "i can send", "i'll send", "i'll get back",
      "i'll follow up", "i'll circle back", "i'll take care of", "i'll handle", "i shall "];

/// Second/third-person promise cues → their commitment to me.
const THEIRS_CUES: &[&str] =
    &["will get back to you", "will send you", "you'll get", "he'll ", "she'll ", "they'll ",
      "will follow up with you", "promised to"];

/// Waiting-on-them cues.
const WAITING_CUES: &[&str] =
    &["waiting on", "waiting for", "waiting to hear", "blocked on", "pending from", "still waiting"];

/// Review-pending cues.
const REVIEW_CUES: &[&str] =
    &["please review", "ptal", "take a look", "can you review", "needs review", "review needed",
      "could you review"];

/// Decision-pending cues.
const DECISION_CUES: &[&str] =
    &["need to decide", "decision needed", "let's decide", "we need to choose", "to be decided",
      "still deciding", "tbd"];

/// Follow-up cues.
const FOLLOWUP_CUES: &[&str] =
    &["follow up", "circle back", "check back", "loop back", "get back to them", "ping again"];

/// Question-shaped reply cues (only meaningful when the segment ended in `?`).
const REPLY_CUES: &[&str] =
    &["can you", "could you", "would you", "will you", "did you", "have you", "any chance you",
      "would it be possible"];

fn contains_any(haystack: &str, cues: &[&str]) -> bool {
    cues.iter().any(|c| haystack.contains(c))
}

/// Extract candidate commitments / open loops from one block of captured text using local rules
/// only. A segment yields at most one candidate: a commitment if a promise cue matches, otherwise
/// an open loop if an open-loop cue matches, otherwise nothing.
pub fn extract(text: &str) -> Vec<Candidate> {
    let mut out = Vec::new();
    for seg in segments(text) {
        let lower = seg.text.to_lowercase();

        // Commitments take precedence (an explicit promise is the strongest signal).
        if contains_any(&lower, MINE_CUES) {
            out.push(Candidate::Commitment {
                direction: CommitmentDirection::Mine,
                description: clip(seg.text),
                confidence: 0.35,
            });
            continue;
        }
        if contains_any(&lower, THEIRS_CUES) {
            out.push(Candidate::Commitment {
                direction: CommitmentDirection::Theirs,
                description: clip(seg.text),
                confidence: 0.3,
            });
            continue;
        }

        // Open loops: waiting → review → decision → follow-up → question-reply. First match wins.
        let kind = if contains_any(&lower, WAITING_CUES) {
            Some((OpenLoopKind::WaitingOnThem, 0.35))
        } else if contains_any(&lower, REVIEW_CUES) {
            Some((OpenLoopKind::ReviewPending, 0.35))
        } else if contains_any(&lower, DECISION_CUES) {
            Some((OpenLoopKind::DecisionPending, 0.3))
        } else if contains_any(&lower, FOLLOWUP_CUES) {
            Some((OpenLoopKind::FollowUp, 0.3))
        } else if seg.is_question && contains_any(&lower, REPLY_CUES) {
            Some((OpenLoopKind::ReplyNeeded, 0.3))
        } else {
            None
        };
        if let Some((kind, confidence)) = kind {
            out.push(Candidate::OpenLoop { kind, description: clip(seg.text), confidence });
        }
    }
    out
}

/// Persist extracted candidates as state rows, each linked (provenance, FR-ST-02) to the event it
/// came from. Every row is written at its heuristic low confidence — the caller does not get to
/// upgrade it here. Commitments are `Open` with no due date (a local rule can't reliably parse
/// one); open loops default to `open`. Returns the new row ids in candidate order.
pub fn persist_candidates(
    conn: &mut Connection,
    event_id: i64,
    candidates: &[Candidate],
    now: i64,
) -> Result<Vec<i64>, MemoryError> {
    let prov = [Provenance::new(event_id)];
    let mut ids = Vec::with_capacity(candidates.len());
    for c in candidates {
        let id = match c {
            Candidate::Commitment { direction, description, confidence } => insert_commitment(
                conn,
                &NewCommitment {
                    direction: *direction,
                    counterparty_id: None,
                    description,
                    due_at: None,
                    status: CommitmentStatus::Open,
                    project_id: None,
                    confidence: *confidence,
                    now,
                },
                &prov,
            )?,
            Candidate::OpenLoop { kind, description, confidence } => insert_open_loop(
                conn,
                &NewOpenLoop {
                    kind: *kind,
                    description,
                    counterparty_id: None,
                    project_id: None,
                    opened_at: now,
                    confidence: *confidence,
                    now,
                },
                &prov,
            )?,
        };
        ids.push(id);
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_log::{insert as insert_event, NewEvent};

    #[test]
    fn every_candidate_is_low_confidence() {
        // Whatever the rules fire on, nothing may reach the Medium threshold (0.5).
        let text = "I'll send the deck tomorrow. Can you review this? \
                    We're waiting on legal. Let's decide the vendor. \
                    She'll get back to you. I need to follow up with Sam.";
        let cands = extract(text);
        assert!(!cands.is_empty());
        for c in &cands {
            assert!(c.confidence() <= LOCAL_RULE_MAX_CONFIDENCE, "candidate above cap: {c:?}");
        }
    }

    #[test]
    fn first_person_promise_is_my_commitment() {
        let cands = extract("I'll send the report by Friday.");
        assert_eq!(cands.len(), 1);
        match &cands[0] {
            Candidate::Commitment { direction, description, .. } => {
                assert_eq!(*direction, CommitmentDirection::Mine);
                assert!(description.contains("send the report"));
            }
            other => panic!("expected a commitment, got {other:?}"),
        }
    }

    #[test]
    fn their_promise_is_a_theirs_commitment() {
        let cands = extract("No rush — she'll get back to you next week.");
        assert_eq!(cands.len(), 1);
        assert!(matches!(
            &cands[0],
            Candidate::Commitment { direction: CommitmentDirection::Theirs, .. }
        ));
    }

    #[test]
    fn waiting_phrase_is_an_open_loop() {
        let cands = extract("Still blocked on the design review from Priya.");
        assert_eq!(cands.len(), 1);
        assert!(matches!(
            &cands[0],
            Candidate::OpenLoop { kind: OpenLoopKind::WaitingOnThem, .. }
        ));
    }

    #[test]
    fn question_only_counts_as_reply_when_it_is_a_question() {
        // ends with '?' AND has a reply cue → ReplyNeeded
        let q = extract("Could you approve the budget?");
        assert!(matches!(&q[0], Candidate::OpenLoop { kind: OpenLoopKind::ReplyNeeded, .. }));
        // same cue words but no question mark → no open loop
        let not_q = extract("Could you approve the budget when you have a moment.");
        assert!(not_q.is_empty(), "a non-question must not become a ReplyNeeded loop: {not_q:?}");
    }

    #[test]
    fn plain_text_yields_nothing() {
        let cands = extract("The weather was nice and the coffee was good.");
        assert!(cands.is_empty());
    }

    #[test]
    fn commitment_beats_open_loop_in_the_same_segment() {
        // Has both a promise cue and a waiting cue; the commitment must win, one candidate only.
        let cands = extract("I'll follow up since we're waiting on the vendor.");
        assert_eq!(cands.len(), 1);
        assert!(matches!(&cands[0], Candidate::Commitment { .. }));
    }

    #[test]
    fn long_description_is_clipped_on_a_char_boundary() {
        let long = format!("I'll {}", "あ".repeat(300));
        let cands = extract(&long);
        if let Candidate::Commitment { description, .. } = &cands[0] {
            assert!(description.len() <= MAX_DESCRIPTION_LEN);
            // still valid UTF-8 (would panic on a bad boundary)
            let _ = description.chars().count();
        } else {
            panic!("expected commitment");
        }
    }

    #[test]
    fn persist_round_trips_through_the_state_tables() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = insert_event(
            &conn,
            &NewEvent {
                ts: 1,
                source: "capture",
                kind: "text",
                app_bundle_id: None,
                window_title: None,
                content: "I'll send the deck. Waiting on legal to reply.",
                content_hash: "h1",
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap();
        let cands = extract("I'll send the deck. Waiting on legal to reply.");
        let ids = persist_candidates(&mut conn, e, &cands, 100).unwrap();
        assert_eq!(ids.len(), 2);

        let commitments = crate::state::list_commitments(&conn).unwrap();
        assert_eq!(commitments.len(), 1);
        assert!(commitments[0].confidence <= LOCAL_RULE_MAX_CONFIDENCE);
        assert_eq!(commitments[0].first_event_id, Some(e), "provenance links to the event");

        let loops = crate::state::list_open_loops(&conn).unwrap();
        assert_eq!(loops.len(), 1);
        assert!(loops[0].confidence <= LOCAL_RULE_MAX_CONFIDENCE);
        assert_eq!(loops[0].first_event_id, Some(e));
    }

    #[test]
    fn persist_empty_candidates_writes_nothing() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = insert_event(
            &conn,
            &NewEvent {
                ts: 1,
                source: "capture",
                kind: "text",
                app_bundle_id: None,
                window_title: None,
                content: "nothing actionable here",
                content_hash: "h2",
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap();
        let ids = persist_candidates(&mut conn, e, &[], 100).unwrap();
        assert!(ids.is_empty());
        let n: i64 = conn.query_row("SELECT count(*) FROM commitments", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }
}
