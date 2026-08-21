//! The three corpora v0.1 generates.
//!
//! All of them are built from the same small vocabulary of projects, people and topics, so that
//! the background is genuinely *confusable* with the answers. A corpus of random strings would
//! make retrieval look perfect and measure nothing: every needle would be the only text in the
//! database containing its own words.
//!
//! Query text is written for the retrieval path that actually exists. The index is FTS5 with a
//! trigram tokenizer and [`shogun_memory::search::lexical_terms`] drops terms under three
//! characters and stopwords, then ORs the rest — so every question here carries at least one
//! distinctive term that its answer also carries, and the distractors share some of the others.

use crate::rng::Rng;
use crate::workload::{BenchEvent, BenchQuery, GeneratedWorkload, Workload};

/// Project code names. Deliberately not real words: a project name is the rare, high-bm25 term
/// that makes a question selective, exactly as a real project name would be.
const PROJECTS: &[&str] = &[
    "kestrel", "lantern", "meridian", "obsidian", "pinnacle", "quarry", "redwood", "sable",
    "tundra", "vantage", "willow", "zephyr",
];

const PEOPLE: &[&str] = &["Priya", "Dana", "Marcus", "Yuki", "Ines", "Tobias", "Amara", "Rafael"];

/// Apps a capture would plausibly come from, so `app_bundle_id` is not a constant column.
const APPS: &[(&str, &str)] = &[
    ("com.apple.Safari", "Safari"),
    ("com.tinyspeck.slackmacgap", "Slack"),
    ("com.apple.mail", "Mail"),
    ("com.microsoft.VSCode", "Code"),
    ("com.figma.Desktop", "Figma"),
];

/// Background sentences: on-topic enough to compete for the same terms as the answers, without
/// answering anything.
const CHATTER: &[&str] = &[
    "standup notes for {p}: nothing blocking, review queue is short",
    "{who} shared the latest {p} dashboard screenshots in the channel",
    "{p} retrospective is scheduled, agenda still needs owners",
    "someone asked whether {p} needs another round of design review",
    "{who} is drafting the {p} onboarding document this week",
    "the {p} integration job flaked again, rerunning it now",
    "{p} ticket triage moved four items into the backlog",
    "{who} mentioned {p} in passing during the planning call",
    "budget spreadsheet for {p} was updated with the new headcount",
    "{p} changelog entry still needs a proper description",
];

/// The decisions the query set asks about. Each is a distinct, checkable fact.
const DECISIONS: &[(&str, &str)] = &[
    ("database", "{p} will store its records in PostgreSQL rather than MySQL"),
    ("deadline", "{p} ships to customers on the fourteenth of March"),
    ("owner", "{who} takes over as the engineering owner of {p}"),
    ("pricing", "{p} renewal was settled at twelve thousand for the year"),
    ("hosting", "{p} runs in the Frankfurt region for data residency"),
    ("auth", "{p} authenticates users through the shared identity service"),
];

fn app_for(rng: &mut Rng) -> (String, String) {
    let (bundle, title) = rng.pick(APPS).copied().unwrap_or(("com.apple.Safari", "Safari"));
    (bundle.to_string(), title.to_string())
}

fn fill(template: &str, project: &str, who: &str) -> String {
    template.replace("{p}", project).replace("{who}", who)
}

/// One background event. `ts` is milliseconds; callers pass a monotonically increasing value so
/// the log has a realistic time axis (and so Cold partitioning has something to bite on).
fn chatter_event(rng: &mut Rng, ts: i64, index: usize) -> BenchEvent {
    let project = rng.pick(PROJECTS).copied().unwrap_or("kestrel");
    let who = rng.pick(PEOPLE).copied().unwrap_or("Priya");
    let template = rng.pick(CHATTER).copied().unwrap_or(CHATTER[0]);
    let (bundle, title) = app_for(rng);
    // The index is part of the text so background events are distinct rows rather than accidental
    // duplicates — otherwise the "clean" corpus would silently contain a duplicate population and
    // its write-amplification number would be measuring the generator, not the memory layer.
    let content = format!("{} [note {}]", fill(template, project, who), index);
    BenchEvent {
        ts,
        source: "capture".to_string(),
        kind: "text".to_string(),
        app_bundle_id: Some(bundle),
        window_title: Some(format!("{title} — {project}")),
        content,
        dwell_ms: 0,
        fact_id: format!("chatter-{index}"),
    }
}

fn decision_event(ts: i64, project: &str, who: &str, topic: &str, template: &str) -> BenchEvent {
    BenchEvent {
        ts,
        source: "capture".to_string(),
        kind: "text".to_string(),
        app_bundle_id: Some("com.apple.mail".to_string()),
        window_title: Some(format!("Mail — {project} {topic}")),
        content: fill(template, project, who),
        dwell_ms: 0,
        fact_id: format!("decision-{project}-{topic}"),
    }
}

/// How many milliseconds separate one event from the next. One minute keeps a 100k corpus inside a
/// realistic couple of months rather than compressing it into an instant.
const TS_STEP_MS: i64 = 60_000;

/// Baseline corpus: unique events only, one answer per query, no duplicates and no contradictions.
///
/// This is the reference point every other number is read against — write amplification here
/// should be 1.0, and any duplicate collapse the layer reports is a false positive.
#[derive(Debug, Default, Clone, Copy)]
pub struct CleanWorkload;

impl Workload for CleanWorkload {
    fn name(&self) -> &'static str {
        "clean"
    }

    fn generate(&self, rng: &mut Rng, events: usize, queries: usize) -> GeneratedWorkload {
        // One answer-bearing event per query, planted at a reproducible position in the stream.
        let planned = queries.min(events);
        let mut slots: Vec<usize> = (0..events).collect();
        rng.shuffle(&mut slots);
        let mut plan: Vec<(usize, usize)> =
            slots.into_iter().take(planned).enumerate().map(|(q, slot)| (slot, q)).collect();
        plan.sort_unstable();

        let mut out: Vec<BenchEvent> = Vec::with_capacity(events);
        let mut query_specs: Vec<(String, String, String)> = Vec::with_capacity(planned);
        let mut next = 0usize;
        for i in 0..events {
            let ts = (i as i64 + 1) * TS_STEP_MS;
            if next < plan.len() && plan[next].0 == i {
                let q = plan[next].1;
                // Cycle projects and topics so each query gets a distinct (project, topic) pair;
                // two queries answered by the same fact would double-count in recall.
                let project = PROJECTS[q % PROJECTS.len()];
                let (topic, template) = DECISIONS[(q / PROJECTS.len()) % DECISIONS.len()];
                let who = PEOPLE[q % PEOPLE.len()];
                // Past PROJECTS x DECISIONS distinct pairs, suffix the topic to keep them unique.
                let round = q / (PROJECTS.len() * DECISIONS.len());
                let topic =
                    if round == 0 { topic.to_string() } else { format!("{topic}{round}") };
                let mut ev = decision_event(ts, project, who, &topic, template);
                if round > 0 {
                    ev.content = format!("{} (revision {})", ev.content, round);
                }
                query_specs.push((project.to_string(), topic, ev.fact_id.clone()));
                out.push(ev);
                next += 1;
            } else {
                out.push(chatter_event(rng, ts, i));
            }
        }

        let queries = query_specs
            .into_iter()
            .map(|(project, topic, fact_id)| BenchQuery {
                ask: format!("what did we decide about the {project} {topic}"),
                expected: vec![fact_id],
                superseded: Vec::new(),
            })
            .collect();

        GeneratedWorkload { name: self.name(), events: out, queries }
    }
}

/// Fraction of a duplicate corpus that repeats an earlier event rather than being new.
const DUPLICATE_SHARE: f64 = 0.30;

/// How many of those repeats are *near* duplicates (reworded) rather than byte-identical.
///
/// The split is the point of this workload. Exact repeats hit
/// [`shogun_memory::event_log::insert_or_touch`]'s `content_hash` match and collapse; near repeats
/// carry the same fact in different words, hash differently, and become second rows. Reporting the
/// two collapse rates separately says exactly where the current dedup contract stops.
const NEAR_DUPLICATE_SHARE: f64 = 0.50;

/// Duplicate-heavy corpus: ~30% of writes repeat a fact already in the log, half verbatim and half
/// reworded.
#[derive(Debug, Default, Clone, Copy)]
pub struct DuplicateWorkload;

impl Workload for DuplicateWorkload {
    fn name(&self) -> &'static str {
        "duplicate"
    }

    fn generate(&self, rng: &mut Rng, events: usize, queries: usize) -> GeneratedWorkload {
        let unique_target = ((events as f64) * (1.0 - DUPLICATE_SHARE)).round() as usize;
        let base = CleanWorkload.generate(rng, unique_target.max(1), queries);

        let mut out: Vec<BenchEvent> = Vec::with_capacity(events);
        out.extend(base.events.iter().cloned());

        let mut i = out.len();
        while i < events {
            let ts = (i as i64 + 1) * TS_STEP_MS;
            let src = match rng.pick(&base.events) {
                Some(e) => e.clone(),
                None => break,
            };
            let mut dup = src.clone();
            dup.ts = ts;
            dup.dwell_ms = 1_000;
            if rng.chance(NEAR_DUPLICATE_SHARE) {
                // Reworded, same meaning, same fact. A human would call these one memory.
                dup.content = format!("(forwarded) {} — resending for visibility", src.content);
            }
            // `fact_id` is inherited either way: both forms carry the fact the original carried.
            out.push(dup);
            i += 1;
        }

        GeneratedWorkload { name: self.name(), events: out, queries: base.queries }
    }
}

/// How many times a temporal fact changes its value over the corpus.
const REVISIONS: usize = 3;

/// The values a tracked project's database choice moves through, in order.
const TEMPORAL_VALUES: [&str; REVISIONS] = ["PostgreSQL", "SQLite", "PostgreSQL"];

/// Temporal corpus: facts that are overwritten by later facts.
///
/// Each tracked project states its database choice three times — PostgreSQL, then SQLite, then
/// PostgreSQL again — and the query asks in the present tense. Only the last statement is a
/// correct answer; the earlier ones are `superseded`. Nothing here tries to *fix* that (v0.1
/// changes no production behaviour); it measures how often the current retrieval path hands back a
/// fact the user already overruled, which is the baseline a selective-update controller must beat.
#[derive(Debug, Default, Clone, Copy)]
pub struct TemporalWorkload;

impl Workload for TemporalWorkload {
    fn name(&self) -> &'static str {
        "temporal"
    }

    fn generate(&self, rng: &mut Rng, events: usize, queries: usize) -> GeneratedWorkload {
        let tracked = queries.min(PROJECTS.len()).max(1);
        let revision_events = tracked * REVISIONS;
        let filler = events.saturating_sub(revision_events);

        // Revisions are spread across the corpus rather than written back to back, so the stale and
        // the current statement sit far apart in the log — which is the situation that makes a
        // present-tense question hard in the first place.
        let mut planned: Vec<(usize, usize, usize)> = Vec::with_capacity(revision_events);
        let band = events.max(1) / REVISIONS;
        for p in 0..tracked {
            for r in 0..REVISIONS {
                let base = r * band;
                let offset = if band > 0 { (p * 7 + r * 13) % band } else { 0 };
                planned.push(((base + offset).min(events.saturating_sub(1)), p, r));
            }
        }
        planned.sort_unstable();
        // Two projects can land on the same index in a small corpus. The runner walks the stream
        // once and plants at most one revision per position, so a collision would silently drop a
        // revision and shorten the corpus — turning a "50,000 events" run into 49,997 without
        // saying so. Spread collisions forward instead; band separation still keeps each project's
        // revisions in order.
        for i in 1..planned.len() {
            if planned[i].0 <= planned[i - 1].0 {
                planned[i].0 = (planned[i - 1].0 + 1).min(events.saturating_sub(1));
            }
        }

        let mut out: Vec<BenchEvent> = Vec::with_capacity(events);
        let mut specs: Vec<(String, Vec<String>, String)> = Vec::with_capacity(tracked);
        let mut superseded_by_project: Vec<Vec<String>> = vec![Vec::new(); tracked];
        let mut cursor = 0usize;
        let mut filler_written = 0usize;
        for i in 0..events {
            let ts = (i as i64 + 1) * TS_STEP_MS;
            if cursor < planned.len() && planned[cursor].0 == i {
                let (_, p, r) = planned[cursor];
                let project = PROJECTS[p % PROJECTS.len()];
                let value = TEMPORAL_VALUES[r];
                let fact_id = format!("temporal-{project}-database-r{r}");
                out.push(BenchEvent {
                    ts,
                    source: "capture".to_string(),
                    kind: "text".to_string(),
                    app_bundle_id: Some("com.tinyspeck.slackmacgap".to_string()),
                    window_title: Some(format!("Slack — {project}")),
                    content: format!(
                        "update: {project} will store its records in {value} from now on"
                    ),
                    dwell_ms: 0,
                    fact_id: fact_id.clone(),
                });
                if r + 1 == REVISIONS {
                    specs.push((project.to_string(), superseded_by_project[p].clone(), fact_id));
                } else {
                    superseded_by_project[p].push(fact_id);
                }
                cursor += 1;
            } else if filler_written < filler {
                out.push(chatter_event(rng, ts, i));
                filler_written += 1;
            }
        }

        // Guarantee the requested corpus size. Positions clamped against the end of the stream can
        // leave the walk a few events short; padding here keeps "--events N" literally true, which
        // the determinism tests assert and which any cross-run comparison depends on.
        while out.len() < events {
            let ts = (out.len() as i64 + 1) * TS_STEP_MS;
            let index = out.len();
            out.push(chatter_event(rng, ts, index));
        }

        let queries = specs
            .into_iter()
            .map(|(project, superseded, fact_id)| BenchQuery {
                ask: format!("where does {project} store its records"),
                expected: vec![fact_id],
                superseded,
            })
            .collect();

        GeneratedWorkload { name: self.name(), events: out, queries }
    }
}

/// Resolve a `--workload` name. Returns `None` for an unknown name so the CLI can list the valid
/// ones rather than silently running the default.
pub fn by_name(name: &str) -> Option<Box<dyn Workload>> {
    match name {
        "clean" => Some(Box::new(CleanWorkload)),
        "duplicate" => Some(Box::new(DuplicateWorkload)),
        "temporal" => Some(Box::new(TemporalWorkload)),
        _ => None,
    }
}

/// Every workload name, for `--help` and for the tests that sweep all of them.
pub const ALL: &[&str] = &["clean", "duplicate", "temporal"];
