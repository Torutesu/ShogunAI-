//! Retrieval quality measurement (Phase Q — docs/context-layer-audit-and-plan.md §9).
//!
//! The core experience is "what's the story with X?" → the answer. That is a question→answer
//! retrieval task, and the device run in §9 showed the embedding model ranks the *answering*
//! passage below a merely on-topic one about half the time. The contract that came out of it:
//!
//! > The retrieval layer owes **recall** — get the answer into the handful of lines that reach the
//! > reading model — not rank-1 precision.
//!
//! Nothing measured that. This does, over a corpus rather than a handful of candidates, through the
//! real `search_hybrid` path (FTS + vector fused by RRF) rather than raw cosine — so the number
//! describes what the product does, not what a component does in isolation.
//!
//! Two modes, and the comparison is the point:
//!
//! ```text
//! cargo test -p shogun-memory --test retrieval_eval -- --ignored --nocapture
//!   → lexical only. Runs anywhere, no model needed.
//!
//! SHOGUN_EMBED_MODEL=…/model.onnx SHOGUN_EMBED_TOKENIZER=…/tokenizer.json \
//!   cargo test -p shogun-memory --features onnx --test retrieval_eval -- --ignored --nocapture
//!   → hybrid. The difference between the two runs is what the embedding model is worth.
//! ```
//!
//! The floors asserted here are deliberately loose — they catch a broken pipeline, not a regression.
//! Real floors get set from measurement; see the note at the bottom of this file.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use shogun_memory::{event_log, search};

/// One thing SHOGUN saw. `id` is referenced by the queries below.
struct Doc {
    id: &'static str,
    text: &'static str,
}

/// A question, and the id of the document that answers it.
struct Query {
    ask: &'static str,
    answer: &'static str,
}

/// A day's worth of captured work, in the shape capture actually produces: fragments of mail,
/// chat, tickets and docs. Deliberately full of near-misses — several documents about the same
/// subject as each question, so recall is not free.
const CORPUS: &[Doc] = &[
    // --- vendor / pricing ---------------------------------------------------------------
    Doc { id: "vendor-settled", text: "Re: renewal — we agreed to 12k for the year, same terms as last time. I'll send the paperwork Monday." },
    Doc { id: "vendor-ask", text: "We should ask the vendor for updated pricing before the next quarter starts." },
    Doc { id: "vendor-catalogue", text: "The vendor sent over their new product catalogue, nothing urgent in it." },
    Doc { id: "vendor-intro", text: "Intro call with the vendor went fine. They want to talk numbers next week." },
    // --- migration PR -------------------------------------------------------------------
    Doc { id: "pr-reviewer", text: "Priya picked up the review on the migration PR this afternoon." },
    Doc { id: "pr-ci", text: "The migration PR is still waiting on CI, the integration job keeps timing out." },
    Doc { id: "pr-opened", text: "Opened the PR for the schema migration this morning, it's a big one." },
    Doc { id: "pr-rollback", text: "If the migration goes wrong we can roll back with the snapshot from Friday." },
    // --- security audit -----------------------------------------------------------------
    Doc { id: "audit-date", text: "The security audit is booked for the week of the 14th, on site both days." },
    Doc { id: "audit-staffing", text: "The security audit will need two engineers on call for questions." },
    Doc { id: "audit-lastyear", text: "Last year's security audit turned up three findings, all closed since." },
    // --- hiring -------------------------------------------------------------------------
    Doc { id: "hire-start", text: "Dana starts on the design team on the 3rd of March. Laptop is ordered." },
    Doc { id: "hire-loop", text: "Two more candidates in the design loop this week, both strong on systems." },
    Doc { id: "hire-budget", text: "Headcount for design is approved for one more role this half." },
    // --- launch -------------------------------------------------------------------------
    Doc { id: "launch-slip", text: "We're moving the launch to the 20th — the localisation work isn't done." },
    Doc { id: "launch-checklist", text: "Launch checklist is in the shared doc, most of it is still unassigned." },
    Doc { id: "launch-press", text: "Press embargo lifts the morning of launch day, confirmed with comms." },
    // --- invoice / finance --------------------------------------------------------------
    Doc { id: "invoice-when", text: "The invoice goes out at the end of the month, payment terms are net 30." },
    Doc { id: "invoice-format", text: "Finance changed the invoice template, use the new one from now on." },
    Doc { id: "invoice-missing", text: "The invoice from the contractor still hasn't arrived, chasing them." },
    // --- infra --------------------------------------------------------------------------
    Doc { id: "infra-outage-cause", text: "The outage on Tuesday was a bad config push, not the database." },
    Doc { id: "infra-oncall", text: "On-call rotation swaps to the platform team starting next sprint." },
    Doc { id: "infra-cost", text: "Cloud spend is up 18% this month, mostly the new staging cluster." },
    // --- customer -----------------------------------------------------------------------
    Doc { id: "cust-churn", text: "Northwind said they're not renewing. Their champion left in January." },
    Doc { id: "cust-escalation", text: "Escalation from Acme about export performance, they want a date." },
    Doc { id: "cust-feedback", text: "Three customers this week asked for SSO, worth putting on the roadmap." },
    // --- Japanese (secondary: multilingual must work, English sets the bar) ---------------
    Doc { id: "ja-meeting-room", text: "会議室の予約は来週から新しいシステムに切り替わります。" },
    Doc { id: "ja-contract", text: "契約更新は3月末が期限です。法務のレビューはもう終わっています。" },
    Doc { id: "ja-contract-draft", text: "契約書のドラフトを送りました。条項の確認をお願いします。" },
    // --- background noise ---------------------------------------------------------------
    Doc { id: "noise-lunch", text: "Lunch options near the office on Thursday, the ramen place is closed." },
    Doc { id: "noise-wifi", text: "The office wifi has been dropping since the upgrade, IT knows." },
    Doc { id: "noise-bike", text: "Someone left a bike in the stairwell again, building management complained." },
    Doc { id: "noise-coffee", text: "The kitchen coffee machine is being replaced next week." },
    Doc { id: "noise-expenses", text: "Reminder to submit expenses before the end of the month." },
    Doc { id: "noise-parking", text: "駐輪場が満車で停められませんでした。" },
];

/// The questions the product promises to answer. Phrased as a person would ask them — not as
/// keyword queries — which is exactly where lexical search alone struggles.
const QUERIES: &[Query] = &[
    Query { ask: "what did we decide about the vendor pricing?", answer: "vendor-settled" },
    Query { ask: "who is reviewing the migration PR?", answer: "pr-reviewer" },
    Query { ask: "why is the migration PR not merged yet?", answer: "pr-ci" },
    Query { ask: "when is the security audit happening?", answer: "audit-date" },
    Query { ask: "when does the new designer start?", answer: "hire-start" },
    Query { ask: "did the launch date move?", answer: "launch-slip" },
    Query { ask: "when do we send the invoice?", answer: "invoice-when" },
    Query { ask: "what caused the outage on Tuesday?", answer: "infra-outage-cause" },
    Query { ask: "is Northwind renewing?", answer: "cust-churn" },
    Query { ask: "what does Acme want?", answer: "cust-escalation" },
    Query { ask: "why is our cloud bill higher?", answer: "infra-cost" },
    Query { ask: "what are customers asking for?", answer: "cust-feedback" },
    Query { ask: "契約の更新はいつまで?", answer: "ja-contract" },
    Query { ask: "会議室の予約はどうなる?", answer: "ja-meeting-room" },
];

/// recall@k plus MRR over the query set.
#[derive(Default)]
struct Metrics {
    ranks: Vec<Option<usize>>, // 1-based rank of the answer, None if it never appeared
}

impl Metrics {
    fn recall_at(&self, k: usize) -> f64 {
        let hit = self.ranks.iter().filter(|r| matches!(r, Some(rank) if *rank <= k)).count();
        hit as f64 / self.ranks.len() as f64
    }

    /// Mean reciprocal rank — sensitive to *how far down* the answer sits, which recall@k is not.
    fn mrr(&self) -> f64 {
        let sum: f64 = self.ranks.iter().map(|r| r.map_or(0.0, |rank| 1.0 / rank as f64)).sum();
        sum / self.ranks.len() as f64
    }
}

/// The embedder, when the crate was built with `onnx` and the model paths are set. `None` means the
/// run measures the lexical half alone — which is a useful measurement in its own right, and the
/// baseline the hybrid number has to beat.
#[cfg(feature = "onnx")]
fn embedder() -> Option<shogun_memory::embed_onnx::OnnxEmbedder> {
    let (Ok(model), Ok(tok)) =
        (std::env::var("SHOGUN_EMBED_MODEL"), std::env::var("SHOGUN_EMBED_TOKENIZER"))
    else {
        return None;
    };
    match shogun_memory::embed_onnx::OnnxEmbedder::load(model, tok) {
        Ok(e) => Some(e),
        Err(e) => {
            eprintln!("[retrieval_eval] model present but failed to load ({e}) — lexical only");
            None
        }
    }
}

#[cfg(not(feature = "onnx"))]
fn embedder() -> Option<std::convert::Infallible> {
    None
}

#[test]
#[ignore = "retrieval quality measurement: run with --ignored --nocapture"]
fn recall_at_k_over_the_eval_set() {
    // The trait brings `embed_passages` / `embed_query` into scope; only the model path uses it.
    #[cfg(feature = "onnx")]
    use shogun_memory::embed::Embedder;

    let conn = shogun_memory::open_in_memory().unwrap();
    let model = embedder();

    // Load the corpus, remembering which event id each document became.
    let mut id_of: std::collections::HashMap<&str, i64> = std::collections::HashMap::new();
    conn.execute_batch("BEGIN").unwrap();
    for (i, doc) in CORPUS.iter().enumerate() {
        let event_id = event_log::insert(
            &conn,
            &event_log::NewEvent {
                ts: i as i64 * 1000,
                source: "capture",
                kind: "text",
                app_bundle_id: Some("com.apple.Mail"),
                window_title: Some("inbox"),
                content: doc.text,
                content_hash: doc.id,
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap();
        id_of.insert(doc.id, event_id);
    }
    conn.execute_batch("COMMIT").unwrap();

    // Embed the corpus, if there is a model.
    #[cfg(feature = "onnx")]
    if let Some(m) = model.as_ref() {
        let texts: Vec<&str> = CORPUS.iter().map(|d| d.text).collect();
        let vectors = m.embed_passages(&texts).unwrap();
        for (doc, v) in CORPUS.iter().zip(vectors) {
            shogun_memory::vector::upsert(&conn, id_of[doc.id], &v).unwrap();
        }
    }

    const LIMIT: usize = 10;
    let mut metrics = Metrics::default();
    let mut misses: Vec<&str> = Vec::new();

    for q in QUERIES {
        #[cfg(feature = "onnx")]
        let query_vec: Option<Vec<f32>> = model.as_ref().map(|m| m.embed_query(q.ask).unwrap());
        #[cfg(not(feature = "onnx"))]
        let query_vec: Option<Vec<f32>> = None;

        let hits =
            search::search_hybrid(&conn, q.ask, query_vec.as_deref(), LIMIT).unwrap();
        let want = id_of[q.answer];
        let rank = hits.iter().position(|h| h.event_id == want).map(|i| i + 1);
        if rank.is_none() {
            misses.push(q.ask);
        }
        eprintln!(
            "  {:<44} rank={}",
            q.ask,
            rank.map_or_else(|| format!("miss (>{LIMIT})"), |r| r.to_string())
        );
        metrics.ranks.push(rank);
    }

    let mode = if model.is_some() { "hybrid (FTS + vector)" } else { "lexical only (FTS)" };
    eprintln!(
        "\n[retrieval_eval] {mode} over {} docs, {} queries\n  \
         recall@1={:.2}  recall@3={:.2}  recall@5={:.2}  recall@10={:.2}  MRR={:.3}",
        CORPUS.len(),
        QUERIES.len(),
        metrics.recall_at(1),
        metrics.recall_at(3),
        metrics.recall_at(5),
        metrics.recall_at(10),
        metrics.mrr()
    );
    if !misses.is_empty() {
        eprintln!("  not retrieved at all: {misses:?}");
    }

    // Loose floors: these catch a broken pipeline (an index that stopped being written, a fusion
    // that lost one half), not a quality regression. Tightening them to just under the measured
    // value is the point of running this — see the note below.
    assert!(
        metrics.recall_at(10) >= 0.5,
        "recall@10 {:.2} — over half the questions cannot reach their answer in {LIMIT} results; \
         retrieval is broken, not merely imprecise",
        metrics.recall_at(10)
    );
    assert!(metrics.mrr() > 0.0, "no query retrieved its answer at all");
}

// ---------------------------------------------------------------------------------------------
// Setting the real floors
//
// The first run on a machine with the model establishes the baseline; the floors above are
// intentionally too loose to catch anything but a break. Once both modes have been measured:
//
//   * raise recall@5 and recall@10 to just under the hybrid numbers,
//   * keep the lexical-only run as a separate expectation (it is what a device without the model
//     gets, and it must not silently rot),
//   * treat the hybrid-minus-lexical gap as the embedding model's contribution — that is the
//     number to check before spending several hundred MB and CPU on a reranker
//     (docs/context-layer-audit-and-plan.md §9).
//
// recall@k is the primary metric because of the contract in §9: this layer must get the answer in
// front of the reading model, not rank it first. MRR is reported alongside because a corpus where
// every answer sits at rank 9 would satisfy recall@10 while being useless in practice.
