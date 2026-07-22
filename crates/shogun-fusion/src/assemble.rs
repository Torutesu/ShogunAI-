//! Context Fusion assembly (WP3.2, §6.5 / AR-09/AR-10): `f(state, screen_ctx, intent) → cache`.
//!
//! This is the pre-assembled RAM artifact the Notch pulls from — never "collect on press"
//! (CLAUDE.md: context cache is always pre-assembled). The daemon rebuilds it on focus change
//! (SLO-05, 300ms) and the Notch presents up to four action candidates (SLO-02, 150ms); both are
//! *measured* in the daemon — this module is the pure builder those measurements wrap.
//!
//! Two rules ride on this assembly and are enforced here, not by callers:
//! - **Confidence gate** (FR-ST-20, via [`crate::confidence`]): High state is stated as fact,
//!   Medium is passed weakly (`possibly:`), **Low is excluded** — it neither becomes a fact nor
//!   proposes an action ([`may_inform_action`]).
//! - **Permission tagging** (invariant 4): every candidate carries the [`Level`] that
//!   [`Action::required_level`] assigns, so the Notch can gate L1 auto-run vs. L2/L3 confirm. In
//!   v1 Fusion proposes only on-device actions (sends open in M4), but the tag is derived, never
//!   asserted, so a send would still surface as L3.

use shogun_agents::permission::{Action, Level, LocalAction};

use crate::confidence::{assemble_facts, band, may_inform_action, Band};

/// The kind of state record a candidate came from. Fine-grained enough to map deterministically
/// to a sensible on-device action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateKind {
    Person,
    Project,
    /// A commitment I owe.
    CommitmentMine,
    /// A commitment owed to me.
    CommitmentTheirs,
    /// An open loop where I need to reply.
    OpenLoopReplyNeeded,
    /// An open loop where I'm waiting on someone else.
    OpenLoopWaiting,
    /// Any other open loop.
    OpenLoopOther,
}

/// A state record projected into fusion input. Fusion does not depend on shogun-memory's row
/// types — the daemon maps rows to this view — so the assembly stays pure and decoupled.
#[derive(Debug, Clone)]
pub struct StateCandidate {
    pub kind: StateKind,
    /// A one-line human summary (goes into the fact list under the confidence gate).
    pub summary: String,
    /// The entity/query seed an action can act on (a person's name, a project, …).
    pub subject: String,
    /// 0.0..=1.0 (FR-ST-02). Drives the confidence gate.
    pub confidence: f64,
    /// 0.0..=1.0 relevance to the current screen/intent (caller-scored; e.g. recency × overlap).
    pub relevance: f64,
}

/// The current screen context (AR-09): the focused app + title + salient extracted terms.
#[derive(Debug, Clone, Default)]
pub struct ScreenContext {
    pub app_bundle_id: String,
    pub window_title: String,
    /// Salient terms/entities pulled from the focused text (no raw capture is stored).
    pub salient: Vec<String>,
}

/// An optional intent signal (a typed query, or a hotkey with no words). When present its text
/// boosts the relevance of matching candidates.
#[derive(Debug, Clone, Default)]
pub struct Intent {
    pub hint: Option<String>,
}

/// One ranked action candidate, tagged with the permission level it requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionCandidate {
    pub action: Action,
    pub level: Level,
    /// Why this was proposed (shown as the button's supporting line; never a low-conf assertion).
    pub rationale: String,
}

/// The assembled context cache — the RAM artifact (AR-10). Holds the screen context, the
/// confidence-gated facts, the Hot-layer summary, and up to [`MAX_ACTIONS`] ranked candidates.
#[derive(Debug, Clone)]
pub struct ContextCache {
    pub screen: ScreenContext,
    /// Confidence-gated state facts: High verbatim, Medium `possibly:`-prefixed, Low dropped.
    pub facts: Vec<String>,
    pub hot_summary: String,
    /// Ranked, deduped, capped at [`MAX_ACTIONS`] (the Notch presents four — SLO-02).
    pub actions: Vec<ActionCandidate>,
}

/// The number of action buttons the Notch presents (spec §6.1 / SLO-02).
pub const MAX_ACTIONS: usize = 4;

/// Map a state candidate to the on-device action it should propose. v1 proposes only
/// [`LocalAction`]s (L1/L2) — no sends — so nothing here can auto-run off the device.
fn action_for(state: &StateCandidate) -> LocalAction {
    match state.kind {
        // Look the entity up in local memory.
        StateKind::Person | StateKind::Project => LocalAction::LocalSearch { query: state.subject.clone() },
        // Nudge about a commitment either direction.
        StateKind::CommitmentMine | StateKind::CommitmentTheirs => {
            LocalAction::ShowNotification { text: state.summary.clone() }
        }
        // Draft a reply (draft-stop default — never sends).
        StateKind::OpenLoopReplyNeeded => LocalAction::SaveDraft { target: "reply" },
        // Surface the waiting/other loop.
        StateKind::OpenLoopWaiting | StateKind::OpenLoopOther => {
            LocalAction::ShowNotification { text: state.summary.clone() }
        }
    }
}

/// The confidence weight a band contributes to a candidate's rank. Low is unreachable here (it is
/// filtered by [`may_inform_action`] before scoring), but is mapped to 0.0 for totality.
fn band_weight(b: Band) -> f64 {
    match b {
        Band::High => 1.0,
        Band::Medium => 0.6,
        Band::Low => 0.0,
    }
}

/// Assemble the context cache. Pure and allocation-bounded so the daemon can call it on every
/// focus change within the SLO. Ranking is `relevance × confidence-weight`, with an intent-hint
/// boost; Low-confidence state is excluded from both facts and actions.
pub fn assemble(
    screen: ScreenContext,
    states: &[StateCandidate],
    hot_summary: impl Into<String>,
    intent: &Intent,
) -> ContextCache {
    // Facts: the single confidence gate. Low-confidence summaries never appear.
    let fact_pairs: Vec<(&str, f64)> =
        states.iter().map(|s| (s.summary.as_str(), s.confidence)).collect();
    let facts = assemble_facts(&fact_pairs);

    let hint = intent.hint.as_deref().map(str::to_lowercase);

    // Score every action-eligible state (Low excluded), then rank.
    let mut scored: Vec<(f64, ActionCandidate)> = states
        .iter()
        .filter(|s| may_inform_action(s.confidence))
        .map(|s| {
            let relevance = s.relevance.clamp(0.0, 1.0);
            let mut score = relevance * band_weight(band(s.confidence));
            // Intent-hint boost: a candidate whose subject/summary matches the typed query ranks up.
            if let Some(h) = &hint {
                if !h.is_empty()
                    && (s.subject.to_lowercase().contains(h) || s.summary.to_lowercase().contains(h))
                {
                    score += 0.5;
                }
            }
            let action = Action::Local(action_for(s));
            let level = action.required_level();
            (score, ActionCandidate { action, level, rationale: s.summary.clone() })
        })
        .collect();

    // Rank by score desc; ties keep input order (stable sort) so the result is deterministic.
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Dedup identical actions (keep the first / highest-scored occurrence), then cap.
    let mut actions: Vec<ActionCandidate> = Vec::with_capacity(MAX_ACTIONS);
    for (_, cand) in scored {
        if actions.len() >= MAX_ACTIONS {
            break;
        }
        if actions.iter().any(|a| a.action == cand.action) {
            continue;
        }
        actions.push(cand);
    }

    // FR-CF-04: never present an empty panel. When no state is relevant (an unknown contact, a
    // brand-new context), fall back to the always-available generic actions so the Notch still has
    // something to do rather than showing nothing.
    if actions.is_empty() {
        actions = generic_actions(&screen);
    }

    ContextCache { screen, facts, hot_summary: hot_summary.into(), actions }
}

/// The generic, always-available actions (FR-CF-04): Save note / Search memory / Extract tasks.
/// All are device-local (L1), so the fallback panel can never contain a send. The memory search is
/// seeded from the screen so it is one tap from useful even with no state.
fn generic_actions(screen: &ScreenContext) -> Vec<ActionCandidate> {
    // Seed the search with the most salient term, else the window title (both device-local).
    let query =
        screen.salient.first().cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| screen.window_title.clone());
    let tag = |action: LocalAction, rationale: &str| {
        let action = Action::Local(action);
        let level = action.required_level();
        ActionCandidate { action, level, rationale: rationale.to_string() }
    };
    vec![
        tag(LocalAction::SaveDraft { target: "note" }, "Save a note"),
        tag(LocalAction::LocalSearch { query }, "Search memory"),
        tag(LocalAction::SaveDraft { target: "tasks" }, "Extract tasks"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(name: &str, conf: f64, rel: f64) -> StateCandidate {
        StateCandidate {
            kind: StateKind::Person,
            summary: format!("{name} owns the roadmap"),
            subject: name.to_string(),
            confidence: conf,
            relevance: rel,
        }
    }

    fn screen() -> ScreenContext {
        ScreenContext {
            app_bundle_id: "com.apple.mail".into(),
            window_title: "Inbox".into(),
            salient: vec!["roadmap".into()],
        }
    }

    #[test]
    fn low_confidence_state_is_excluded_from_facts_and_actions() {
        let states = vec![
            person("Alice", 0.95, 0.9),
            person("Bob", 0.3, 0.9), // low confidence → dropped everywhere
        ];
        let cache = assemble(screen(), &states, "hot", &Intent::default());
        assert!(cache.facts.iter().any(|f| f.contains("Alice")));
        assert!(!cache.facts.iter().any(|f| f.contains("Bob")), "low-conf fact must not appear");
        // Bob proposes no action either.
        assert!(cache.actions.iter().all(|a| a.rationale.contains("Alice")));
    }

    #[test]
    fn medium_confidence_fact_is_possibly_prefixed() {
        let cache = assemble(screen(), &[person("Carol", 0.6, 0.5)], "", &Intent::default());
        assert_eq!(cache.facts, vec!["possibly: Carol owns the roadmap".to_string()]);
    }

    #[test]
    fn candidates_are_tagged_with_their_permission_level() {
        // A reply-needed loop → SaveDraft (L1); a state mutation would be L2, a send L3.
        let states = vec![StateCandidate {
            kind: StateKind::OpenLoopReplyNeeded,
            summary: "reply to Dave".into(),
            subject: "Dave".into(),
            confidence: 0.9,
            relevance: 0.8,
        }];
        let cache = assemble(screen(), &states, "", &Intent::default());
        assert_eq!(cache.actions.len(), 1);
        assert_eq!(cache.actions[0].level, Level::L1);
        assert_eq!(cache.actions[0].action, Action::Local(LocalAction::SaveDraft { target: "reply" }));
        // Fusion proposes no external send in v1.
        assert!(!cache.actions[0].action.is_external_send());
    }

    #[test]
    fn ranking_is_relevance_times_confidence_and_capped_at_four() {
        // Six eligible candidates with distinct subjects → capped at MAX_ACTIONS, best first.
        let states = vec![
            person("A", 0.9, 0.2),
            person("B", 0.9, 0.9), // highest
            person("C", 0.9, 0.5),
            person("D", 0.6, 0.9), // medium band weight 0.6 → 0.54
            person("E", 0.9, 0.1),
            person("F", 0.9, 0.7),
        ];
        let cache = assemble(screen(), &states, "", &Intent::default());
        assert_eq!(cache.actions.len(), MAX_ACTIONS);
        // B (0.9) first, then F (0.63), then D (0.54) or C (0.45)... B must lead.
        assert_eq!(cache.actions[0].rationale, "B owns the roadmap");
    }

    #[test]
    fn intent_hint_boosts_matching_candidate() {
        let states = vec![
            person("Zoe", 0.9, 0.1),    // low relevance, but matches hint
            person("Yuki", 0.9, 0.6),   // higher base relevance
        ];
        let intent = Intent { hint: Some("zoe".into()) };
        let cache = assemble(screen(), &states, "", &intent);
        // The hint boost (+0.5) lifts Zoe above Yuki.
        assert_eq!(cache.actions[0].rationale, "Zoe owns the roadmap");
    }

    #[test]
    fn identical_actions_are_deduped() {
        // Two waiting loops with the same summary → same ShowNotification action → one candidate.
        let s = |c: f64| StateCandidate {
            kind: StateKind::OpenLoopWaiting,
            summary: "waiting on the vendor".into(),
            subject: "vendor".into(),
            confidence: c,
            relevance: 0.8,
        };
        let cache = assemble(screen(), &[s(0.9), s(0.85)], "", &Intent::default());
        assert_eq!(cache.actions.len(), 1);
    }

    #[test]
    fn cache_carries_screen_and_hot_summary_verbatim() {
        let cache = assemble(screen(), &[], "3 unread threads", &Intent::default());
        assert_eq!(cache.screen.app_bundle_id, "com.apple.mail");
        assert_eq!(cache.hot_summary, "3 unread threads");
        assert!(cache.facts.is_empty());
    }

    #[test]
    fn no_state_falls_back_to_generic_actions_never_empty() {
        // FR-CF-04: an unknown context must still offer Save note / Search memory / Extract tasks.
        let cache = assemble(screen(), &[], "hot", &Intent::default());
        assert!(!cache.actions.is_empty(), "the panel must never be empty (FR-CF-04)");
        let rationales: Vec<&str> = cache.actions.iter().map(|a| a.rationale.as_str()).collect();
        assert_eq!(rationales, vec!["Save a note", "Search memory", "Extract tasks"]);
        // every fallback is device-local — never a send.
        assert!(cache.actions.iter().all(|a| !a.action.is_external_send()));
        assert!(cache.actions.iter().all(|a| a.level == Level::L1));
        // the memory search is seeded from the screen's salient term.
        assert!(cache
            .actions
            .iter()
            .any(|a| a.action == Action::Local(LocalAction::LocalSearch { query: "roadmap".into() })));
    }

    #[test]
    fn low_confidence_only_still_falls_back_not_empty() {
        // All state is Low (excluded from actions) → still not empty (FR-CF-04 + FR-ST-20).
        let cache = assemble(screen(), &[person("Bob", 0.3, 0.9)], "hot", &Intent::default());
        assert!(!cache.actions.is_empty());
        assert_eq!(cache.actions[0].rationale, "Save a note");
    }
}
